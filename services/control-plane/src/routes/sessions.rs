use axum::Json;
use chrono::{Duration, Utc};
use overlay_protocol::{PeerCandidate, SessionAuthorization, SessionOpenGrant};

pub async fn open_session() -> Json<SessionOpenGrant> {
    Json(SessionOpenGrant {
        session_id: "sess_local".into(),
        service_id: "svc_home_openclaw".into(),
        home_node_id: "node-home".into(),
        relay_hint: Some("relay-local".into()),
        authorization: SessionAuthorization {
            token: "signed-local-dev-token".into(),
            expires_at: Utc::now() + Duration::minutes(2),
            candidates: vec![PeerCandidate {
                addr: "203.0.113.10:7001".into(),
            }],
        },
    })
}
