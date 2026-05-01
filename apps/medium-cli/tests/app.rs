use medium_cli::app::{normalize_device_label, summary, title};
use medium_cli::paths::AppPaths;
use medium_cli::state::AppState;
use medium_cli::{run, run_main};
use std::fs;
use std::path::Path;

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
fn help_command_returns_grouped_cli_help() -> anyhow::Result<()> {
    let output = run(vec!["medium".to_string(), "help".to_string()]).map_err(anyhow::Error::msg)?;

    assert!(output.starts_with("Medium CLI\n"));
    assert!(output.contains("Usage:\n  medium <command> [options]"));
    assert!(output.contains("Bootstrap:"));
    assert!(output.contains("  medium init-control [--reconfigure]"));
    assert!(output.contains("Control-plane diagnostics:"));
    assert!(output.contains("  medium control devices"));
    assert!(output.contains("Client:"));
    assert!(output.contains("  medium devices"));
    assert!(output.contains("Run `medium help <command>` is not supported yet."));
    assert!(!output.contains("usage: medium ["));
    Ok(())
}

#[test]
fn help_flags_return_grouped_cli_help() -> anyhow::Result<()> {
    for flag in ["--help", "-h"] {
        let output =
            run(vec!["medium".to_string(), flag.to_string()]).map_err(anyhow::Error::msg)?;
        assert!(output.contains("Usage:\n  medium <command> [options]"));
        assert!(output.contains("Node runtime:"));
    }
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
fn run_without_config_uses_default_node_config_path() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let _root = EnvGuard::set_path("MEDIUM_ROOT", temp.path());
    let error = run(vec!["medium".to_string(), "run".to_string()]).unwrap_err();
    let expected = temp
        .path()
        .join("etc/medium/node.toml")
        .display()
        .to_string();
    assert!(
        error.contains(&expected),
        "expected {expected:?} in error {error:?}"
    );
    assert!(!error.contains("usage: medium"));
    Ok(())
}

#[test]
fn run_uses_default_agent_mode_with_config() -> anyhow::Result<()> {
    let config_path = write_config(
        r#"
node_id = "node-1"

[[services]]
id = "svc_openclaw"
kind = "https"
target = "127.0.0.1:3000"

[[services]]
id = "svc_ssh"
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

    assert!(output.contains("agent ready for node-1"));
    assert!(output.contains("2 services"));
    assert!(output.contains("svc_openclaw:https@127.0.0.1:3000"));
    assert!(output.contains("svc_ssh:ssh@127.0.0.1:22"));
    Ok(())
}

#[tokio::test]
async fn run_reports_http_service_certificate_setup_failures() -> anyhow::Result<()> {
    let config_path = write_config(
        r#"
node_id = "node-1"
bind_addr = "127.0.0.1:0"

[[services]]
id = "hello"
kind = "http"
target = "127.0.0.1:8082"
"#,
    )?;

    let error = run_main(vec![
        "medium".to_string(),
        "run".to_string(),
        "--config".to_string(),
        config_path.display().to_string(),
    ])
    .await
    .unwrap_err();

    assert!(error.contains("node-agent failed"));
    assert!(error.contains("http service hello"));
    assert!(error.contains("issue Medium service TLS certificate"));
    assert!(error.contains("sudo medium init-control --reconfigure"));
    Ok(())
}

#[test]
fn run_rejects_unknown_commands() {
    let error = run(vec!["medium".to_string(), "bad".to_string()]).unwrap_err();
    assert!(error.contains("unknown command: bad"));
    assert!(error.contains("Run: medium help"));
    assert!(!error.contains("Usage:\n  medium <command> [options]"));
    assert!(!error.contains("pair --server <url> --device <name>"));
}

#[test]
fn app_state_saves_under_state_directory() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());
    let state = AppState {
        server_url: "https://example.test".to_string(),
        device_name: "node-1".to_string(),
        bootstrap_code: "ABC123".to_string(),
        invite_version: 0,
        security: String::new(),
        control_pin: String::new(),
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
        security: String::new(),
        control_pin: String::new(),
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
        "overlay-medium-cli-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::write(&path, contents)?;
    Ok(path)
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set_path(key: &'static str, value: &Path) -> Self {
        let previous = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
