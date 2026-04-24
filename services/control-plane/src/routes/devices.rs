use axum::Json;
use overlay_protocol::{DeviceCatalogResponse, DeviceRecord, SshEndpoint};

pub async fn list_devices() -> Json<DeviceCatalogResponse> {
    Json(DeviceCatalogResponse {
        devices: vec![DeviceRecord {
            id: "node-home".into(),
            name: "node-home".into(),
            ssh: Some(SshEndpoint {
                host: "127.0.0.1".into(),
                port: 2222,
                user: "overlay".into(),
            }),
        }],
    })
}
