use axum::Json;
use overlay_protocol::{DeviceCatalogResponse, DeviceRecord, SshEndpoint};

pub async fn list_devices() -> Json<DeviceCatalogResponse> {
    Json(device_catalog_from_env())
}

pub fn device_catalog_from_env() -> DeviceCatalogResponse {
    DeviceCatalogResponse {
        devices: vec![DeviceRecord {
            id: std::env::var("OVERLAY_HOME_NODE_ID").unwrap_or_else(|_| "node-home".into()),
            name: std::env::var("OVERLAY_HOME_NODE_NAME").unwrap_or_else(|_| "node-home".into()),
            ssh: Some(SshEndpoint {
                service_id: std::env::var("OVERLAY_SSH_SERVICE_ID")
                    .unwrap_or_else(|_| "svc_home_ssh".into()),
                host: "127.0.0.1".into(),
                port: 2222,
                user: std::env::var("OVERLAY_SSH_USER").unwrap_or_else(|_| "overlay".into()),
            }),
        }],
    }
}
