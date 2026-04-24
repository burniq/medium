#[tokio::main]
async fn main() {
    let cfg = relay::config::RelayConfig::default();
    tracing_subscriber::fmt::init();
    tracing::info!(bind_addr = %cfg.bind_addr, "relay starting");
}
