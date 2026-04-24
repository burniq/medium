use axum::{body::Body, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn health_route_returns_ok() {
    let app = control_plane::app::build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}
