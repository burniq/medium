use home_node::config::NodeConfig;
use std::fs;

#[test]
fn parses_https_service_definition() {
    let raw = r#"
node_id = "node-1"
node_label = "Node"
control_url = "https://control.example.test"
control_pin = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
shared_secret = "medium-shared-secret-test"
relay_addr = "relay.example.test:7001"
wss_relay_url = "wss://relay.example.test/medium/v1/relay"

[[services]]
id = "svc_openclaw"
kind = "https"
label = "OpenClaw"
target = "127.0.0.1:3000"
"#;

    let cfg: NodeConfig = toml::from_str(raw).unwrap();
    assert_eq!(cfg.services.len(), 1);
    assert_eq!(cfg.node_label.as_deref(), Some("Node"));
    assert_eq!(
        cfg.control_url.as_deref(),
        Some("https://control.example.test")
    );
    assert_eq!(
        cfg.control_pin.as_deref(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(
        cfg.shared_secret.as_deref(),
        Some("medium-shared-secret-test")
    );
    assert_eq!(cfg.relay_addr.as_deref(), Some("relay.example.test:7001"));
    assert_eq!(
        cfg.wss_relay_url.as_deref(),
        Some("wss://relay.example.test/medium/v1/relay")
    );
    assert_eq!(cfg.services[0].id, "svc_openclaw");
    assert_eq!(cfg.services[0].label.as_deref(), Some("OpenClaw"));
}

#[test]
fn parses_embedded_medium_service_ca() {
    let raw = r#"
node_id = "node-1"
service_ca_cert_pem = """
-----BEGIN CERTIFICATE-----
test-cert
-----END CERTIFICATE-----
"""
service_ca_key_pem = """
-----BEGIN PRIVATE KEY-----
test-key
-----END PRIVATE KEY-----
"""

[[services]]
id = "hello"
kind = "http"
target = "127.0.0.1:8082"
"#;

    let cfg: NodeConfig = toml::from_str(raw).unwrap();
    assert!(
        cfg.service_ca_cert_pem
            .as_deref()
            .unwrap()
            .contains("test-cert")
    );
    assert!(
        cfg.service_ca_key_pem
            .as_deref()
            .unwrap()
            .contains("test-key")
    );
    assert_eq!(cfg.services[0].kind, "http");
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
node_id = "node-1"
node_label = "Node"

[[services]]
id = "svc_openclaw"
kind = "https"
target = "127.0.0.1:3000"
"#,
    )
    .unwrap();

    let cfg = home_node::config::load_from_path(&path).unwrap();
    assert_eq!(cfg.node_id, "node-1");
    assert_eq!(cfg.services.len(), 1);
}
