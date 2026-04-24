#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let config_path = match (args.next().as_deref(), args.next()) {
        (Some("--config"), Some(path)) => path,
        _ => {
            eprintln!("usage: home-node --config <path>");
            std::process::exit(1);
        }
    };
    let cfg = home_node::config::load_from_path(&config_path).unwrap();
    let shared_secret =
        std::env::var("OVERLAY_SHARED_SECRET").unwrap_or_else(|_| "local-dev-secret".into());
    tracing_subscriber::fmt::init();
    home_node::proxy::run_tcp_proxy(cfg, &shared_secret)
        .await
        .unwrap();
}
