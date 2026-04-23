use home_node::config::NodeConfig;

#[test]
fn parses_https_service_definition() {
    let raw = r#"
node_id = "node-home"

[[services]]
id = "svc_home_openclaw"
kind = "https"
target = "127.0.0.1:3000"
"#;

    let cfg: NodeConfig = toml::from_str(raw).unwrap();
    assert_eq!(cfg.services.len(), 1);
    assert_eq!(cfg.services[0].id, "svc_home_openclaw");
}
