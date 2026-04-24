use linux_client::app::{normalize_device_label, summary, title};
use linux_client::paths::AppPaths;
use linux_client::run;
use linux_client::state::AppState;
use std::fs;

#[test]
fn title_matches_product_name() {
    assert_eq!(title(), "Medium");
}

#[test]
fn summary_marks_headless_role() {
    assert_eq!(summary(), "Medium CLI");
}

#[test]
fn summary_mentions_medium_name() {
    assert!(summary().contains("Medium"));
}

#[test]
fn normalize_device_label_trims_whitespace() {
    assert_eq!(normalize_device_label("  arch node  "), "arch node");
}

#[test]
fn info_command_returns_summary_output() -> anyhow::Result<()> {
    let output = run(vec!["medium".to_string(), "info".to_string()]).map_err(anyhow::Error::msg)?;
    assert_eq!(output, "Medium CLI");
    Ok(())
}

#[test]
fn run_supports_label_normalization() -> anyhow::Result<()> {
    let output = run(vec![
        "medium".to_string(),
        "normalize-label".to_string(),
        "  phone  ".to_string(),
    ])
    .map_err(anyhow::Error::msg)?;
    assert_eq!(output, "phone");
    Ok(())
}

#[test]
fn run_requires_config_flag() {
    let error = run(vec!["medium".to_string(), "run".to_string()]).unwrap_err();
    assert!(
        error.contains("usage: medium [join <invite> | pair --server <url> --device <name> | devices | ssh sync [--write-main-config] | proxy ssh --device <name> | run --config <path> | info | normalize-label <value>]")
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
        "medium".to_string(),
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
    let error = run(vec!["medium".to_string(), "bad".to_string()]).unwrap_err();
    assert!(
        error.contains("usage: medium [join <invite> | pair --server <url> --device <name> | devices | ssh sync [--write-main-config] | proxy ssh --device <name> | run --config <path> | info | normalize-label <value>]")
    );
}

#[test]
fn app_state_saves_under_state_directory() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());
    let state = AppState {
        server_url: "https://example.test".to_string(),
        device_name: "node-home".to_string(),
        bootstrap_code: "ABC123".to_string(),
        invite_version: 0,
    };

    state.save(&paths)?;

    assert!(paths.state_dir.is_dir());
    assert!(paths.state_path.is_file());
    assert!(!paths.app_config_dir.join("state.json").exists());
    Ok(())
}

#[test]
fn app_state_loads_legacy_overlay_state_and_migrates_it() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::for_linux_home(home.path());
    let legacy_state_path = home
        .path()
        .join(".config")
        .join("overlay")
        .join("state.json");
    let expected = AppState {
        server_url: "https://legacy.example.test".to_string(),
        device_name: "legacy-node".to_string(),
        bootstrap_code: "LEGACY123".to_string(),
        invite_version: 0,
    };

    fs::create_dir_all(legacy_state_path.parent().unwrap())?;
    fs::write(&legacy_state_path, serde_json::to_vec_pretty(&expected)?)?;

    let loaded = AppState::load(&paths)?;

    assert_eq!(loaded.server_url, expected.server_url);
    assert_eq!(loaded.device_name, expected.device_name);
    assert_eq!(loaded.bootstrap_code, expected.bootstrap_code);
    assert_eq!(loaded.invite_version, expected.invite_version);
    assert!(paths.state_path.is_file());
    assert_eq!(
        fs::read_to_string(&paths.state_path)?,
        fs::read_to_string(&legacy_state_path)?
    );
    Ok(())
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
