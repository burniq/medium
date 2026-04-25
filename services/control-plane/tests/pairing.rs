use axum::{
    body::{Body, to_bytes},
    http::{Request, header},
};
use overlay_crypto::issue_bootstrap_code;
use overlay_protocol::BootstrapInviteResponse;
use serde::Deserialize;
use tower::ServiceExt;

#[derive(Deserialize)]
struct LegacyBootstrapCodeResponse {
    code: String,
}

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
    let legacy_payload: LegacyBootstrapCodeResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload.expires_at, None);
    assert!(payload.bootstrap_token.starts_with("ovr-"));
    assert!(payload.control_key.starts_with("medium-control-key-"));
    assert_eq!(legacy_payload.code, payload.bootstrap_token);
    assert_eq!(
        payload.invite,
        format!(
            "medium://join?v=1&control=https://control.example.test&control_key={}",
            payload.control_key
        )
    );
    assert!(!payload.invite.contains("token="));
}

#[tokio::test]
async fn bootstrap_route_ignores_invalid_forwarded_headers() {
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
                .header(header::HOST, "control.example.test/poison")
                .header("x-forwarded-proto", "javascript")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: BootstrapInviteResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        payload.invite,
        format!(
            "medium://join?v=1&control=http://127.0.0.1:8080&control_key={}",
            payload.control_key
        )
    );
}
