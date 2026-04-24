use home_node::config::NodeConfig;
use std::fs;

#[test]
fn parses_https_service_definition() {
    let raw = r#"
node_id = "node-home"
node_label = "Home"

[[services]]
id = "svc_home_openclaw"
kind = "https"
label = "OpenClaw"
target = "127.0.0.1:3000"
"#;

    let cfg: NodeConfig = toml::from_str(raw).unwrap();
    assert_eq!(cfg.services.len(), 1);
    assert_eq!(cfg.node_label.as_deref(), Some("Home"));
    assert_eq!(cfg.services[0].id, "svc_home_openclaw");
    assert_eq!(cfg.services[0].label.as_deref(), Some("OpenClaw"));
}

#[test]
fn loads_node_config_from_file() {
    let path = std::env::temp_dir().join(format!(
        "overlay-home-node-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    fs::write(
        &path,
        r#"
node_id = "node-home"
node_label = "Home"

[[services]]
id = "svc_home_openclaw"
kind = "https"
target = "127.0.0.1:3000"
"#,
    )
    .unwrap();

    let cfg = home_node::config::load_from_path(&path).unwrap();
    assert_eq!(cfg.node_id, "node-home");
    assert_eq!(cfg.services.len(), 1);
}
