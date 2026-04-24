use crate::state::ControlState;
use axum::{Json, extract::State, http::StatusCode};
use overlay_protocol::RegisterNodeRequest;

pub async fn register_node(
    State(state): State<ControlState>,
    Json(request): Json<RegisterNodeRequest>,
) -> Result<StatusCode, StatusCode> {
    state
        .registry
        .register_node(&request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}
