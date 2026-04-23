use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct BootstrapCodeResponse {
    pub code: String,
}

pub async fn create_bootstrap_code() -> Json<BootstrapCodeResponse> {
    Json(BootstrapCodeResponse {
        code: overlay_crypto::issue_bootstrap_code(),
    })
}
