#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = relay::config::RelayConfig::default();
    tracing_subscriber::fmt::init();
    let shared_secret = cfg
        .shared_secret
        .ok_or_else(|| anyhow::anyhow!("MEDIUM_RELAY_SHARED_SECRET is required"))?;
    let mode = std::env::var("MEDIUM_RELAY_MODE").unwrap_or_else(|_| "tcp".into());
    tracing::info!(bind_addr = %cfg.bind_addr, %mode, "relay starting");
    if mode == "wss" {
        relay::run_wss_relay(&cfg.bind_addr, Some(shared_secret)).await
    } else {
        relay::run_tcp_relay(&cfg.bind_addr, Some(shared_secret)).await
    }
}
