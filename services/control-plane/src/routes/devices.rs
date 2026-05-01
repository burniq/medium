use crate::state::ControlState;
use axum::{Json, extract::State, http::StatusCode};
use overlay_protocol::DeviceCatalogResponse;

pub async fn list_devices(
    State(state): State<ControlState>,
) -> Result<Json<DeviceCatalogResponse>, StatusCode> {
    let catalog = state
        .registry
        .list_devices()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(catalog))
}
