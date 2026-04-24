use axum::{body::Body, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn devices_route_returns_catalog() {
    let app = control_plane::app::build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/devices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
    let devices = json.get("devices").unwrap().as_array().unwrap();

    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].get("name").unwrap(), "node-home");
    assert_eq!(
        devices[0].get("ssh").unwrap().get("service_id").unwrap(),
        "svc_home_ssh"
    );
    assert_eq!(devices[0].get("ssh").unwrap().get("port").unwrap(), 2222);
}
