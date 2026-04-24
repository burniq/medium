use axum::{Json, extract::Query, http::StatusCode};
use chrono::{Duration, Utc};
use overlay_crypto::issue_session_token;
use overlay_protocol::{PeerCandidate, SessionAuthorization, SessionOpenGrant, SessionOpenRequest};

pub async fn open_session(
    Query(request): Query<SessionOpenRequest>,
) -> Result<Json<SessionOpenGrant>, StatusCode> {
    let grant = issue_session_grant(&request, &SessionSettings::from_env())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(grant))
}

#[derive(Debug, Clone)]
pub struct SessionSettings {
    pub ssh_service_id: String,
    pub home_node_id: String,
    pub home_node_tcp_addr: String,
    pub shared_secret: String,
}

impl SessionSettings {
    pub fn from_env() -> Self {
        Self {
            ssh_service_id: std::env::var("OVERLAY_SSH_SERVICE_ID")
                .unwrap_or_else(|_| "svc_home_ssh".into()),
            home_node_id: std::env::var("OVERLAY_HOME_NODE_ID")
                .unwrap_or_else(|_| "node-home".into()),
            home_node_tcp_addr: std::env::var("OVERLAY_HOME_NODE_TCP_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:17001".into()),
            shared_secret: std::env::var("OVERLAY_SHARED_SECRET")
                .unwrap_or_else(|_| "local-dev-secret".into()),
        }
    }
}

pub fn issue_session_grant(
    request: &SessionOpenRequest,
    settings: &SessionSettings,
) -> anyhow::Result<SessionOpenGrant> {
    if request.service_id != settings.ssh_service_id {
        anyhow::bail!("unknown service");
    }

    let session_id = format!("sess_{}", uuid::Uuid::new_v4().simple());
    let token = issue_session_token(
        &settings.shared_secret,
        &session_id,
        &request.service_id,
        &settings.home_node_id,
    )?;

    Ok(SessionOpenGrant {
        session_id,
        service_id: request.service_id.clone(),
        home_node_id: settings.home_node_id.clone(),
        relay_hint: None,
        authorization: SessionAuthorization {
            token,
            expires_at: Utc::now() + Duration::minutes(2),
            candidates: vec![PeerCandidate {
                addr: settings.home_node_tcp_addr.clone(),
            }],
        },
    })
}
