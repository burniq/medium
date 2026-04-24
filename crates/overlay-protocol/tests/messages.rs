use overlay_protocol::{
    DeviceCatalogResponse, DeviceRecord, ServiceKind, SessionOpenRequest, SshEndpoint,
};

#[test]
fn session_open_request_round_trips_as_json() {
    let req = SessionOpenRequest {
        service_id: "svc_home_openclaw".into(),
        requester_device_id: "dev_phone".into(),
    };

    let json = serde_json::to_string(&req).unwrap();
    let parsed: SessionOpenRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.service_id, "svc_home_openclaw");
    assert_eq!(parsed.requester_device_id, "dev_phone");
    assert_eq!(ServiceKind::Https.as_str(), "https");
}

#[test]
fn device_catalog_round_trips_as_json() {
    let catalog = DeviceCatalogResponse {
        devices: vec![DeviceRecord {
            id: "node-home".into(),
            name: "node-home".into(),
            ssh: Some(SshEndpoint {
                service_id: "svc_home_ssh".into(),
                host: "127.0.0.1".into(),
                port: 2222,
                user: "overlay".into(),
            }),
        }],
    };

    let json = serde_json::to_string(&catalog).unwrap();
    let parsed: DeviceCatalogResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.devices.len(), 1);
    assert_eq!(parsed.devices[0].name, "node-home");
    assert_eq!(parsed.devices[0].ssh.as_ref().unwrap().service_id, "svc_home_ssh");
    assert_eq!(parsed.devices[0].ssh.as_ref().unwrap().port, 2222);
}
