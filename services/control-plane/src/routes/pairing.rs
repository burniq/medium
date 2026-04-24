use axum::{
    Json,
    http::{HeaderMap, header},
};
use overlay_protocol::BootstrapInviteResponse;

const DEFAULT_CONTROL_AUTHORITY: &str = "127.0.0.1:8080";

pub async fn create_bootstrap_code(headers: HeaderMap) -> Json<BootstrapInviteResponse> {
    let control_url = control_url(&headers);
    Json(issue_bootstrap_invite(&control_url))
}

fn issue_bootstrap_invite(control_url: &str) -> BootstrapInviteResponse {
    let bootstrap_token = overlay_crypto::issue_bootstrap_code();
    let invite = format!("medium://join?v=1&control={control_url}&token={bootstrap_token}");

    BootstrapInviteResponse {
        invite,
        bootstrap_token,
        expires_at: None,
    }
}

fn control_url(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http");
    let authority = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_CONTROL_AUTHORITY.to_string());

    format!("{scheme}://{authority}")
}
