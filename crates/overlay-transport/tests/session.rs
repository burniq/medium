use overlay_protocol::{PeerCandidate, SessionAuthorization, SessionOpenGrant};
use overlay_transport::session::{
    SessionHello, read_session_hello, session_alpn, write_session_hello,
};
use tokio::io::duplex;

#[test]
fn uses_overlay_alpn() {
    assert_eq!(session_alpn(), b"overlay/1");
}

#[test]
fn session_grant_contains_candidate_for_direct_connect() {
    let grant = SessionOpenGrant {
        session_id: "sess1".into(),
        service_id: "svc_openclaw".into(),
        node_id: "node-1".into(),
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

#[tokio::test]
async fn session_hello_round_trips_over_stream() {
    let (mut client, mut server) = duplex(1024);
    let expected = SessionHello {
        token: "signed-token".into(),
        service_id: "svc_ssh".into(),
    };

    let writer = tokio::spawn(async move {
        write_session_hello(&mut client, &expected).await.unwrap();
    });

    let actual = read_session_hello(&mut server).await.unwrap();
    writer.await.unwrap();

    assert_eq!(actual.service_id, "svc_ssh");
    assert_eq!(actual.token, "signed-token");
}
