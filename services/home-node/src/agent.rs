use crate::config::{NodeConfig, load_from_path};
use crate::control::build_registration;
use anyhow::Context;
use overlay_protocol::RegisterNodeRequest;
use std::path::Path;

pub struct PreparedAgent {
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

    pub async fn run_until_shutdown(&self) -> anyhow::Result<()> {
        println!("{}", self.startup_summary());
        tokio::signal::ctrl_c()
            .await
            .context("failed while waiting for shutdown signal")?;
        println!("agent stopped for {}", self.registration.node_id);
        Ok(())
    }
}

pub fn prepare_agent(cfg: NodeConfig) -> PreparedAgent {
    PreparedAgent {
        registration: build_registration(&cfg),
    }
}

pub fn prepare_agent_from_path(path: impl AsRef<Path>) -> anyhow::Result<PreparedAgent> {
    let cfg = load_from_path(path)?;
    Ok(prepare_agent(cfg))
}

pub async fn register_node(
    control_url: &str,
    registration: &RegisterNodeRequest,
) -> anyhow::Result<()> {
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
    registration: &RegisterNodeRequest,
) -> anyhow::Result<()> {
    let mut last_error = None;

    for _ in 0..30 {
        match register_node(control_url, registration).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("registration failed")))
}
