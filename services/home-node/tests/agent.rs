use home_node::agent::prepare_agent;
use home_node::config::NodeConfig;

#[test]
fn startup_summary_lists_published_services() {
    let cfg: NodeConfig = toml::from_str(
        r#"
node_id = "node-home"

[[services]]
id = "svc_home_openclaw"
kind = "https"
target = "127.0.0.1:3000"

[[services]]
id = "svc_home_ssh"
kind = "ssh"
target = "127.0.0.1:22"
"#,
    )
    .unwrap();

    let agent = prepare_agent(cfg);
    let summary = agent.startup_summary();

    assert!(summary.contains("agent ready for node-home"));
    assert!(summary.contains("2 services"));
    assert!(summary.contains("svc_home_openclaw:https@127.0.0.1:3000"));
    assert!(summary.contains("svc_home_ssh:ssh@127.0.0.1:22"));
}
