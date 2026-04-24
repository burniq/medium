#[tokio::main]
async fn main() {
    let bind_addr =
        std::env::var("OVERLAY_CONTROL_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(listener, control_plane::app::build_router())
        .await
        .unwrap();
}
