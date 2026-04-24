use axum::{body::Body, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn health_route_returns_ok() {
    let app = control_plane::app::build_router(control_plane::state::ControlState {
        registry: control_plane::registry::RegistryStore::in_memory()
            .await
            .unwrap(),
        shared_secret: "local-test-secret".into(),
    });
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
