use home_node::config::NodeConfig;
use home_node::control::build_registration;
use overlay_protocol::EndpointKind;

#[test]
fn registration_derives_ice_udp_endpoint_from_public_tcp_host() {
    let cfg: NodeConfig = toml::from_str(
        r#"
node_id = "node-1"
bind_addr = "0.0.0.0:17001"
public_addr = "198.51.100.10:17001"

[[services]]
id = "svc_web"
kind = "https"
target = "127.0.0.1:3000"
"#,
    )
    .unwrap();

    let registration = build_registration(&cfg);
    let ice = registration
        .endpoints
        .iter()
        .find(|endpoint| {
            endpoint.kind == EndpointKind::IceUdp && endpoint.addr == "198.51.100.10:17002"
        })
        .expect("ICE UDP endpoint should be registered");

    assert_eq!(ice.addr, "198.51.100.10:17002");
}

#[test]
fn registration_includes_configured_lan_ice_udp_endpoints_before_public_endpoint() {
    let cfg: NodeConfig = toml::from_str(
        r#"
node_id = "node-1"
bind_addr = "0.0.0.0:17001"
public_addr = "198.51.100.10:17001"
ice_host_addrs = ["192.168.1.44:17002", "[fd00::44]:17002"]

[[services]]
id = "svc_web"
kind = "https"
target = "127.0.0.1:3000"
"#,
    )
    .unwrap();

    let registration = build_registration(&cfg);
    let ice_addrs = registration
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.kind == EndpointKind::IceUdp)
        .map(|endpoint| endpoint.addr.as_str())
        .collect::<Vec<_>>();

    assert_eq!(&ice_addrs[..2], &["192.168.1.44:17002", "[fd00::44]:17002"]);
    assert!(ice_addrs.contains(&"198.51.100.10:17002"));
}
