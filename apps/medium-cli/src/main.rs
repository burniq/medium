#[tokio::main]
async fn main() {
    overlay_transport::install_default_crypto_provider();
    overlay_transport::logging::init_tracing();

    match medium_cli::run_main(std::env::args()).await {
        Ok(Some(output)) => println!("{output}"),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
