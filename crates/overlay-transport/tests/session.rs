use overlay_protocol::{PeerCandidate, SessionAuthorization, SessionOpenGrant};
use overlay_transport::session::session_alpn;

#[test]
fn uses_overlay_alpn() {
    assert_eq!(session_alpn(), b"overlay/1");
}

#[test]
fn session_grant_contains_candidate_for_direct_connect() {
    let grant = SessionOpenGrant {
        session_id: "sess1".into(),
        service_id: "svc_home_openclaw".into(),
        home_node_id: "node-home".into(),
        relay_hint: None,
        authorization: SessionAuthorization {
            token: "token".into(),
            expires_at: chrono::Utc::now(),
            candidates: vec![PeerCandidate {
                addr: "198.51.100.10:7001".into(),
            }],
        },
    };

    assert_eq!(grant.authorization.candidates.len(), 1);
}
