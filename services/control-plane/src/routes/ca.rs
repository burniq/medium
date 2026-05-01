use crate::state::ControlState;
use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use overlay_protocol::{ServiceCertificateRequest, ServiceCertificateResponse};

pub async fn medium_ca(State(state): State<ControlState>) -> impl IntoResponse {
    match state.service_ca_cert_pem {
        Some(cert) => ([(header::CONTENT_TYPE, "application/x-pem-file")], cert).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn issue_service_certificate(
    State(state): State<ControlState>,
    Json(request): Json<ServiceCertificateRequest>,
) -> Result<Json<ServiceCertificateResponse>, StatusCode> {
    if request.shared_secret != state.shared_secret {
        return Err(StatusCode::FORBIDDEN);
    }
    if request.hostnames.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let cert = state
        .service_ca_cert_pem
        .as_deref()
        .ok_or(StatusCode::NOT_FOUND)?;
    let key = state
        .service_ca_key_pem
        .as_deref()
        .ok_or(StatusCode::NOT_FOUND)?;
    let identity = overlay_crypto::issue_service_tls_identity(cert, key, &request.hostnames)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ServiceCertificateResponse {
        cert_pem: identity.cert_pem,
        key_pem: identity.key_pem,
    }))
}
