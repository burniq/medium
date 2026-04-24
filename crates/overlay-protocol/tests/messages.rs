use overlay_protocol::{
    DeviceCatalogResponse, DeviceRecord, EndpointKind, NodeEndpoint, PublishedService,
    RegisterNodeRequest, ServiceKind, SessionOpenRequest, SshEndpoint,
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

#[test]
fn register_node_request_round_trips_versioned_components() {
    let request = RegisterNodeRequest {
        node_id: "node-home".into(),
        node_label: "Home".into(),
        endpoints: vec![NodeEndpoint {
            kind: EndpointKind::TcpProxy,
            schema_version: 1,
            addr: "127.0.0.1:17001".into(),
            priority: 10,
        }],
        services: vec![PublishedService {
            id: "svc_home_ssh".into(),
            kind: ServiceKind::Ssh,
            schema_version: 1,
            label: Some("Home SSH".into()),
            target: "127.0.0.1:2222".into(),
            user_name: Some("overlay".into()),
        }],
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: RegisterNodeRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.node_label, "Home");
    assert_eq!(parsed.endpoints[0].kind.as_str(), "tcp_proxy");
    assert_eq!(parsed.services[0].schema_version, 1);
    assert_eq!(parsed.services[0].user_name.as_deref(), Some("overlay"));
}
