use crate::state::AppState;
use overlay_protocol::DeviceCatalogResponse;
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
    })
}

pub async fn fetch_devices(state: &AppState) -> anyhow::Result<DeviceCatalogResponse> {
    let url = format!("{}/api/devices", state.server_url.trim_end_matches('/'));
    let response = reqwest::get(url).await?.error_for_status()?;
    Ok(response.json().await?)
}
