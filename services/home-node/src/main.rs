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
    overlay_transport::install_default_crypto_provider();
    overlay_transport::logging::init_tracing();
    home_node::agent::prepare_agent(cfg)
        .run_until_shutdown()
        .await
        .unwrap();
}
