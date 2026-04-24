use axum::{
    body::{Body, to_bytes},
    http::{Request, header},
};
use overlay_crypto::issue_bootstrap_code;
use overlay_protocol::BootstrapInviteResponse;
use tower::ServiceExt;

#[test]
fn bootstrap_code_has_expected_prefix() {
    let code = issue_bootstrap_code();
    assert!(code.starts_with("ovr-"));
    assert!(code.len() > 12);
}

#[tokio::test]
async fn bootstrap_route_returns_medium_join_invite() {
    let app = control_plane::app::build_router(control_plane::state::ControlState {
        registry: control_plane::registry::RegistryStore::in_memory()
            .await
            .unwrap(),
        shared_secret: "local-test-secret".into(),
    });
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap-code")
                .header(header::HOST, "control.example.test")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: BootstrapInviteResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload.expires_at, None);
    assert!(payload.bootstrap_token.starts_with("ovr-"));
    assert_eq!(
        payload.invite,
        format!(
            "medium://join?v=1&control=https://control.example.test&token={}",
            payload.bootstrap_token
        )
    );
}
