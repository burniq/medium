use overlay_protocol::{
    EndpointKind, NodeEndpoint, PublishedService, RegisterNodeRequest, ServiceKind,
};

#[tokio::test]
async fn registry_returns_devices_from_registered_nodes() {
    let store = control_plane::registry::RegistryStore::in_memory()
        .await
        .unwrap();
    store
        .register_node(&RegisterNodeRequest {
            node_id: "node-1".into(),
            node_label: "Node".into(),
            endpoints: vec![NodeEndpoint {
                kind: EndpointKind::TcpProxy,
                schema_version: 1,
                addr: "127.0.0.1:17001".into(),
                priority: 10,
            }],
            services: vec![PublishedService {
                id: "svc_ssh".into(),
                kind: ServiceKind::Ssh,
                schema_version: 1,
                label: Some("Node SSH".into()),
                target: "127.0.0.1:2222".into(),
                user_name: Some("overlay".into()),
            }],
        })
        .await
        .unwrap();

    let catalog = store.list_devices().await.unwrap();

    assert_eq!(catalog.devices.len(), 1);
    assert_eq!(catalog.devices[0].name, "Node");
    assert_eq!(
        catalog.devices[0].ssh.as_ref().unwrap().service_id,
        "svc_ssh"
    );
    assert_eq!(catalog.devices[0].ssh.as_ref().unwrap().port, 17001);
    assert_eq!(catalog.devices[0].ssh.as_ref().unwrap().user, "overlay");
}

#[tokio::test]
async fn registry_resolves_service_route_for_session_open() {
    let store = control_plane::registry::RegistryStore::in_memory()
        .await
        .unwrap();
    store
        .register_node(&RegisterNodeRequest {
            node_id: "node-1".into(),
            node_label: "Node".into(),
            endpoints: vec![NodeEndpoint {
                kind: EndpointKind::TcpProxy,
                schema_version: 1,
                addr: "127.0.0.1:17001".into(),
                priority: 10,
            }],
            services: vec![PublishedService {
                id: "svc_ssh".into(),
                kind: ServiceKind::Ssh,
                schema_version: 1,
                label: Some("Node SSH".into()),
                target: "127.0.0.1:2222".into(),
                user_name: Some("overlay".into()),
            }],
        })
        .await
        .unwrap();

    let route = store.resolve_service_route("svc_ssh").await.unwrap();

    assert_eq!(route.node_id, "node-1");
    assert_eq!(route.tcp_addr, "127.0.0.1:17001");
    assert_eq!(route.user_name.as_deref(), Some("overlay"));
}
