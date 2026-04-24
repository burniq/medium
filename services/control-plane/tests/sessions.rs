use chrono::Utc;
use overlay_protocol::{PeerCandidate, SessionAuthorization, SessionOpenGrant};

#[test]
fn session_grant_has_relay_hint_field() {
    let grant = SessionOpenGrant {
        session_id: "sess_1".into(),
        service_id: "svc_home_openclaw".into(),
        home_node_id: "node-home".into(),
        relay_hint: Some("relay-1".into()),
        authorization: SessionAuthorization {
            token: "signed-local-dev-token".into(),
            expires_at: Utc::now(),
            candidates: vec![PeerCandidate {
                addr: "198.51.100.10:7001".into(),
            }],
        },
    };

    assert_eq!(grant.relay_hint.as_deref(), Some("relay-1"));
}
