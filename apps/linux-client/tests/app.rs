use linux_client::app::{normalize_device_label, summary, title};
use linux_client::run;
use std::fs;

#[test]
fn title_matches_product_name() {
    assert_eq!(title(), "Overlay");
}

#[test]
fn summary_marks_headless_role() {
    assert_eq!(summary(), "Overlay CLI");
}

#[test]
fn normalize_device_label_trims_whitespace() {
    assert_eq!(normalize_device_label("  arch node  "), "arch node");
}

#[test]
fn info_command_returns_summary_output() -> anyhow::Result<()> {
    let output = run(vec!["overlay".to_string(), "info".to_string()]).map_err(anyhow::Error::msg)?;
    assert_eq!(output, "Overlay CLI");
    Ok(())
}

#[test]
fn run_supports_label_normalization() -> anyhow::Result<()> {
    let output = run(vec![
        "overlay".to_string(),
        "normalize-label".to_string(),
        "  phone  ".to_string(),
    ])
    .map_err(anyhow::Error::msg)?;
    assert_eq!(output, "phone");
    Ok(())
}

#[test]
fn run_requires_config_flag() {
    let error = run(vec!["overlay".to_string(), "run".to_string()]).unwrap_err();
    assert!(
        error.contains("usage: overlay [pair --server <url> --device <name> | devices | ssh sync [--write-main-config] | proxy ssh --device <name> | run --config <path> | info | normalize-label <value>]")
    );
}

#[test]
fn run_uses_default_agent_mode_with_config() -> anyhow::Result<()> {
    let config_path = write_config(
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
    )?;

    let output = run(vec![
        "overlay".to_string(),
        "run".to_string(),
        "--config".to_string(),
        config_path.display().to_string(),
    ])
    .map_err(anyhow::Error::msg)?;

    assert!(output.contains("agent ready for node-home"));
    assert!(output.contains("2 services"));
    assert!(output.contains("svc_home_openclaw:https@127.0.0.1:3000"));
    assert!(output.contains("svc_home_ssh:ssh@127.0.0.1:22"));
    Ok(())
}

#[test]
fn run_rejects_unknown_commands() {
    let error = run(vec!["overlay".to_string(), "bad".to_string()]).unwrap_err();
    assert!(
        error.contains("usage: overlay [pair --server <url> --device <name> | devices | ssh sync [--write-main-config] | proxy ssh --device <name> | run --config <path> | info | normalize-label <value>]")
    );
}

fn write_config(contents: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "overlay-linux-client-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::write(&path, contents)?;
    Ok(path)
}
