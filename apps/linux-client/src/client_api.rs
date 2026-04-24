use crate::app;
use crate::state::AppState;
use crate::state::invite::Invite;
use overlay_protocol::{DeviceCatalogResponse, SessionOpenGrant, SessionOpenRequest};
use serde::Deserialize;

#[derive(Deserialize)]
struct BootstrapCodeResponse {
    code: String,
}

pub async fn pair(server_url: &str, device_name: &str) -> anyhow::Result<AppState> {
    let server_url = server_url.trim_end_matches('/');
    let url = format!("{server_url}/api/bootstrap-code");
    let response = reqwest::get(url).await?.error_for_status()?;
    let payload: BootstrapCodeResponse = response.json().await?;

    Ok(AppState {
        server_url: server_url.to_string(),
        device_name: device_name.to_string(),
        bootstrap_code: payload.code,
        invite_version: 0,
    })
}

pub async fn join(invite: &Invite) -> anyhow::Result<AppState> {
    Ok(AppState {
        server_url: invite.control_url.trim_end_matches('/').to_string(),
        device_name: local_device_name(),
        bootstrap_code: invite.bootstrap_token.clone(),
        invite_version: invite.version,
    })
}

pub async fn fetch_devices(state: &AppState) -> anyhow::Result<DeviceCatalogResponse> {
    let url = format!("{}/api/devices", state.server_url.trim_end_matches('/'));
    let response = reqwest::get(url).await?.error_for_status()?;
    Ok(response.json().await?)
}

fn local_device_name() -> String {
    std::env::var("MEDIUM_DEVICE_NAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .map(|value| app::normalize_device_label(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "medium-client".to_string())
}

pub async fn open_session(state: &AppState, service_id: &str) -> anyhow::Result<SessionOpenGrant> {
    let url = format!(
        "{}/api/sessions/open",
        state.server_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .get(url)
        .query(&SessionOpenRequest {
            service_id: service_id.to_string(),
            requester_device_id: state.device_name.clone(),
        })
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}
