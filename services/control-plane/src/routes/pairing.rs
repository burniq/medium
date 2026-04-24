use axum::{
    Json,
    http::{HeaderMap, header, uri::Authority},
};
use overlay_protocol::BootstrapInviteResponse;
use std::str::FromStr;

const DEFAULT_CONTROL_AUTHORITY: &str = "127.0.0.1:8080";

pub async fn create_bootstrap_code(headers: HeaderMap) -> Json<BootstrapInviteResponse> {
    let control_url = control_url(&headers);
    Json(issue_bootstrap_invite(&control_url))
}

fn issue_bootstrap_invite(control_url: &str) -> BootstrapInviteResponse {
    let bootstrap_token = overlay_crypto::issue_bootstrap_code();
    let invite = format!("medium://join?v=1&control={control_url}&token={bootstrap_token}");

    BootstrapInviteResponse {
        code: bootstrap_token.clone(),
        invite,
        bootstrap_token,
        expires_at: None,
    }
}

fn control_url(headers: &HeaderMap) -> String {
    let scheme = forwarded_scheme(headers);
    let authority = forwarded_authority(headers)
        .unwrap_or_else(|| DEFAULT_CONTROL_AUTHORITY.to_string());

    format!("{scheme}://{authority}")
}

fn forwarded_scheme(headers: &HeaderMap) -> &'static str {
    match headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
    {
        Some(value) if value.eq_ignore_ascii_case("https") => "https",
        Some(value) if value.eq_ignore_ascii_case("http") => "http",
        _ => "http",
    }
}

fn forwarded_authority(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::HOST)?.to_str().ok()?;
    let authority = raw.split(',').next()?.trim();
    let authority = Authority::from_str(authority).ok()?;

    Some(authority.as_str().to_string())
}
