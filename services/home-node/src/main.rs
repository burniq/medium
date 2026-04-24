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
    if let Ok(control_url) = std::env::var("OVERLAY_CONTROL_URL") {
        let registration = home_node::control::build_registration(&cfg);
        home_node::agent::register_node_with_retry(&control_url, &registration)
            .await
            .unwrap();
    }
    home_node::proxy::run_tcp_proxy(cfg, &shared_secret)
        .await
        .unwrap();
}
