use crate::config::{NodeConfig, load_from_path};
use crate::control::build_registration;
use crate::proxy::{run_tcp_proxy_with_shutdown, spawn_configured_connectors};
use anyhow::Context;
use overlay_protocol::RegisterNodeRequest;
use std::path::Path;
use tokio::sync::oneshot;

pub struct PreparedAgent {
    cfg: NodeConfig,
    registration: RegisterNodeRequest,
}

impl PreparedAgent {
    pub fn startup_summary(&self) -> String {
        let services = self
            .registration
            .services
            .iter()
            .map(|service| {
                format!(
                    "{}:{}@{}",
                    service.id,
                    service.kind.as_str(),
                    service.target
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "agent ready for {} ({} services): {}",
            self.registration.node_id,
            self.registration.services.len(),
            services
        )
    }

    pub async fn run_until_shutdown(self) -> anyhow::Result<()> {
        println!("{}", self.startup_summary());
        let node_id = self.registration.node_id.clone();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::warn!(%error, "failed while waiting for shutdown signal");
            }
            let _ = shutdown_tx.send(());
        });
        self.run_with_shutdown(shutdown_rx).await?;
        println!("agent stopped for {node_id}");
        Ok(())
    }

    pub async fn run_with_shutdown(self, shutdown: oneshot::Receiver<()>) -> anyhow::Result<()> {
        overlay_transport::install_default_crypto_provider();

        let control_url = self
            .cfg
            .control_url
            .clone()
            .or_else(|| std::env::var("OVERLAY_CONTROL_URL").ok());
        let control_pin = self
            .cfg
            .control_pin
            .clone()
            .or_else(|| std::env::var("MEDIUM_CONTROL_PIN").ok());
        if let Some(control_url) = control_url.as_deref() {
            register_node_with_retry(control_url, control_pin.as_deref(), &self.registration)
                .await
                .with_context(|| {
                    format!("register node {} with control-plane", self.cfg.node_id)
                })?;
            println!("registered node {} with {}", self.cfg.node_id, control_url);
        }

        let shared_secret = self
            .cfg
            .shared_secret
            .clone()
            .or_else(|| std::env::var("OVERLAY_SHARED_SECRET").ok())
            .unwrap_or_else(|| "local-dev-secret".to_string());
        spawn_configured_connectors(&self.cfg, &shared_secret);
        run_tcp_proxy_with_shutdown(self.cfg, &shared_secret, shutdown, None).await
    }
}

pub fn prepare_agent(cfg: NodeConfig) -> PreparedAgent {
    PreparedAgent {
        registration: build_registration(&cfg),
        cfg,
    }
}

pub fn prepare_agent_from_path(path: impl AsRef<Path>) -> anyhow::Result<PreparedAgent> {
    let cfg = load_from_path(path)?;
    Ok(prepare_agent(cfg))
}

pub async fn register_node(
    control_url: &str,
    control_pin: Option<&str>,
    registration: &RegisterNodeRequest,
) -> anyhow::Result<()> {
    if let Some(control_pin) = control_pin {
        overlay_transport::pinned_http::post_json_no_content(
            &format!("{}/api/nodes/register", control_url.trim_end_matches('/')),
            control_pin,
            registration,
        )
        .await?;
        return Ok(());
    }

    reqwest::Client::new()
        .post(format!(
            "{}/api/nodes/register",
            control_url.trim_end_matches('/')
        ))
        .json(registration)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn register_node_with_retry(
    control_url: &str,
    control_pin: Option<&str>,
    registration: &RegisterNodeRequest,
) -> anyhow::Result<()> {
    let mut last_error = None;

    for _ in 0..30 {
        match register_node(control_url, control_pin, registration).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("registration failed")))
}
