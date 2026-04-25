use overlay_crypto::verify_session_token;
use overlay_protocol::{
    EndpointKind, NodeEndpoint, PublishedService, RegisterNodeRequest, ServiceKind,
    SessionOpenRequest,
};

#[tokio::test]
async fn session_grant_contains_signed_token_and_candidate() {
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

    let settings = control_plane::routes::sessions::SessionSettings {
        registry: store,
        shared_secret: "local-secret".into(),
    };
    let grant = control_plane::routes::sessions::issue_session_grant(
        &SessionOpenRequest {
            service_id: "svc_ssh".into(),
            requester_device_id: "macbook".into(),
        },
        &settings,
    )
    .await
    .unwrap();

    assert_eq!(grant.authorization.candidates[0].addr, "127.0.0.1:17001");
    assert!(grant.relay_hint.is_none());
    let claims = verify_session_token("local-secret", &grant.authorization.token).unwrap();
    assert_eq!(claims.service_id, "svc_ssh");
    assert_eq!(claims.node_id, grant.node_id);
}
