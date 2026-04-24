use overlay_crypto::verify_session_token;
use overlay_protocol::SessionOpenRequest;

#[test]
fn session_grant_contains_signed_token_and_candidate() {
    let settings = control_plane::routes::sessions::SessionSettings {
        ssh_service_id: "svc_home_ssh".into(),
        home_node_id: "node-home".into(),
        home_node_tcp_addr: "127.0.0.1:17001".into(),
        shared_secret: "local-secret".into(),
    };
    let grant = control_plane::routes::sessions::issue_session_grant(
        &SessionOpenRequest {
            service_id: "svc_home_ssh".into(),
            requester_device_id: "macbook".into(),
        },
        &settings,
    )
    .unwrap();

    assert_eq!(grant.authorization.candidates[0].addr, "127.0.0.1:17001");
    assert!(grant.relay_hint.is_none());
    let claims = verify_session_token("local-secret", &grant.authorization.token).unwrap();
    assert_eq!(claims.service_id, "svc_home_ssh");
    assert_eq!(claims.home_node_id, grant.home_node_id);
}
