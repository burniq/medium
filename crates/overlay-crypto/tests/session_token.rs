use overlay_crypto::{issue_session_token, verify_session_token};

#[test]
fn session_token_round_trips_with_shared_secret() {
    let token = issue_session_token("local-secret", "sess-1", "svc_ssh", "node-1").unwrap();
    let claims = verify_session_token("local-secret", &token).unwrap();

    assert_eq!(claims.session_id, "sess-1");
    assert_eq!(claims.service_id, "svc_ssh");
    assert_eq!(claims.node_id, "node-1");
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
