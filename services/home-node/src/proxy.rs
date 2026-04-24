use crate::config::NodeConfig;
use overlay_crypto::verify_session_token;
use overlay_transport::session::read_session_hello;
use std::collections::HashMap;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

pub async fn run_tcp_proxy(cfg: NodeConfig, shared_secret: &str) -> anyhow::Result<()> {
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();
    run_tcp_proxy_with_shutdown(cfg, shared_secret, shutdown_rx, None).await
}

pub async fn run_tcp_proxy_with_shutdown(
    cfg: NodeConfig,
    shared_secret: &str,
    mut shutdown: oneshot::Receiver<()>,
    bound_addr_tx: Option<oneshot::Sender<std::net::SocketAddr>>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(&cfg.bind_addr).await?;
    if let Some(tx) = bound_addr_tx {
        let _ = tx.send(listener.local_addr()?);
    }

    let services = cfg
        .services
        .into_iter()
        .map(|service| (service.id, service.target))
        .collect::<HashMap<_, _>>();
    let node_id = cfg.node_id;
    let shared_secret = shared_secret.to_string();

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let services = services.clone();
                let node_id = node_id.clone();
                let shared_secret = shared_secret.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(stream, services, &node_id, &shared_secret).await {
                        tracing::warn!(%error, "proxy connection failed");
                    }
                });
            }
        }
    }

    Ok(())
}

async fn handle_connection(
    mut inbound: TcpStream,
    services: HashMap<String, String>,
    expected_node_id: &str,
    shared_secret: &str,
) -> anyhow::Result<()> {
    let hello = read_session_hello(&mut inbound).await?;
    let claims = verify_session_token(shared_secret, &hello.token)?;
    if claims.service_id != hello.service_id {
        anyhow::bail!("session service mismatch");
    }
    if claims.home_node_id != expected_node_id {
        anyhow::bail!("session home node mismatch");
    }

    let target = services
        .get(&hello.service_id)
        .ok_or_else(|| anyhow::anyhow!("unknown service {}", hello.service_id))?;
    let mut outbound = TcpStream::connect(target).await?;
    let _ = copy_bidirectional(&mut inbound, &mut outbound).await?;
    Ok(())
}
