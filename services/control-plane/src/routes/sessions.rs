use crate::state::ControlState;
use axum::{Json, extract::Query, http::StatusCode};
use chrono::{Duration, Utc};
use overlay_crypto::issue_session_token;
use overlay_protocol::{PeerCandidate, SessionAuthorization, SessionOpenGrant, SessionOpenRequest};

pub async fn open_session(
    axum::extract::State(state): axum::extract::State<ControlState>,
    Query(request): Query<SessionOpenRequest>,
) -> Result<Json<SessionOpenGrant>, StatusCode> {
    let grant = issue_session_grant(
        &request,
        &SessionSettings {
            registry: state.registry.clone(),
            shared_secret: state.shared_secret.clone(),
        },
    )
    .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(grant))
}

#[derive(Debug, Clone)]
pub struct SessionSettings {
    pub registry: crate::registry::RegistryStore,
    pub shared_secret: String,
}

pub async fn issue_session_grant(
    request: &SessionOpenRequest,
    settings: &SessionSettings,
) -> anyhow::Result<SessionOpenGrant> {
    let route = settings.registry.resolve_service_route(&request.service_id).await?;
    let session_id = format!("sess_{}", uuid::Uuid::new_v4().simple());
    let token = issue_session_token(
        &settings.shared_secret,
        &session_id,
        &request.service_id,
        &route.node_id,
    )?;

    Ok(SessionOpenGrant {
        session_id,
        service_id: request.service_id.clone(),
        home_node_id: route.node_id,
        relay_hint: None,
        authorization: SessionAuthorization {
            token,
            expires_at: Utc::now() + Duration::minutes(2),
            candidates: vec![PeerCandidate {
                addr: route.tcp_addr,
            }],
        },
    })
}
