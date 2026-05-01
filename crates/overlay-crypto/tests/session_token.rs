use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use overlay_crypto::{
    SESSION_TOKEN_CLOCK_SKEW_MINUTES, SESSION_TOKEN_TTL_MINUTES, SessionTokenClaims,
    issue_session_token, verify_session_token,
};
use sha2::Sha256;

#[test]
fn session_token_round_trips_with_shared_secret() {
    let token = issue_session_token("local-secret", "sess-1", "svc_ssh", "node-1").unwrap();
    let claims = verify_session_token("local-secret", &token).unwrap();

    assert_eq!(claims.session_id, "sess-1");
    assert_eq!(claims.service_id, "svc_ssh");
    assert_eq!(claims.node_id, "node-1");
    assert!(claims.expires_at > Utc::now() + Duration::minutes(SESSION_TOKEN_TTL_MINUTES - 1));
}

#[test]
fn session_token_rejects_wrong_secret() {
    let token = issue_session_token("local-secret", "sess-1", "svc_ssh", "node-1").unwrap();
    let error = verify_session_token("other-secret", &token).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("invalid session token signature")
    );
}

#[test]
fn session_token_allows_small_clock_skew_after_expiry() {
    let token = signed_token(
        "local-secret",
        Utc::now() - Duration::minutes(SESSION_TOKEN_CLOCK_SKEW_MINUTES - 1),
    );

    verify_session_token("local-secret", &token).unwrap();
}

#[test]
fn session_token_rejects_expiry_beyond_clock_skew() {
    let token = signed_token(
        "local-secret",
        Utc::now() - Duration::minutes(SESSION_TOKEN_CLOCK_SKEW_MINUTES + 1),
    );

    let error = verify_session_token("local-secret", &token).unwrap_err();
    assert!(error.to_string().contains("session token expired"));
    assert!(error.to_string().contains("leeway_minutes="));
}

fn signed_token(shared_secret: &str, expires_at: chrono::DateTime<Utc>) -> String {
    let claims = SessionTokenClaims {
        session_id: "sess-1".into(),
        service_id: "svc_ssh".into(),
        node_id: "node-1".into(),
        expires_at,
    };
    let payload = serde_json::to_vec(&claims).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(shared_secret.as_bytes()).unwrap();
    mac.update(&payload);
    let signature = mac.finalize().into_bytes();
    let payload_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload);
    let signature_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, signature);
    format!("{payload_b64}.{signature_b64}")
}
