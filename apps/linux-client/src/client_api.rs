use crate::app;
use crate::state::AppState;
use crate::state::invite::Invite;
use anyhow::{Context, bail};
use overlay_protocol::{DeviceCatalogResponse, SessionOpenGrant, SessionOpenRequest};
use serde::Deserialize;

#[derive(Deserialize)]
struct BootstrapCodeResponse {
    #[serde(default)]
    invite: String,
    #[serde(default)]
    bootstrap_token: String,
    #[serde(default)]
    code: String,
}

pub async fn pair(server_url: &str, device_name: &str) -> anyhow::Result<AppState> {
    let server_url = server_url.trim_end_matches('/');
    let url = format!("{server_url}/api/bootstrap-code");
    let response = reqwest::get(url).await?.error_for_status()?;
    let payload: BootstrapCodeResponse = response.json().await?;
    let bootstrap_code = if !payload.bootstrap_token.is_empty() {
        payload.bootstrap_token
    } else if !payload.code.is_empty() {
        payload.code
    } else if !payload.invite.is_empty() {
        String::new()
    } else {
        bail!("bootstrap response is missing a token");
    };

    Ok(AppState {
        server_url: server_url.to_string(),
        device_name: device_name.to_string(),
        bootstrap_code,
        invite_version: 0,
        control_key: String::new(),
    })
}

pub async fn join(invite: &Invite) -> anyhow::Result<AppState> {
    let server_url = normalize_control_url(&invite.control_url)?;

    Ok(AppState {
        server_url,
        device_name: local_device_name(),
        bootstrap_code: String::new(),
        invite_version: invite.version,
        control_key: invite.control_key.clone(),
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

fn normalize_control_url(raw: &str) -> anyhow::Result<String> {
    let url = reqwest::Url::parse(raw).with_context(|| format!("invalid control URL {raw}"))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => bail!("unsupported control URL scheme {scheme}"),
    }
    if url.host_str().is_none() {
        bail!("control URL must include a host");
    }

    Ok(url.as_str().trim_end_matches('/').to_string())
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

pub fn format_join_invite(control_url: &str, control_key: &str) -> anyhow::Result<String> {
    let control_url = normalize_control_url(control_url)?;
    if control_key.is_empty() {
        bail!("control key cannot be empty");
    }

    Ok(format!(
        "medium://join?v=1&control={control_url}&control_key={control_key}"
    ))
}
