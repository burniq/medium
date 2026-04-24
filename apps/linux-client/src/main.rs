#[tokio::main]
async fn main() {
    match linux_client::run_main(std::env::args()).await {
        Ok(Some(output)) => println!("{output}"),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
