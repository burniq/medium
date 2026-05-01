use medium_cli::paths::AppPaths;
use medium_cli::run_main;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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

    fn set_str(key: &'static str, value: &str) -> Self {
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

fn write_mock_systemctl(path: &Path) -> anyhow::Result<()> {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
case "$*" in
  "is-enabled medium-control-plane.service")
    printf 'enabled\n'
    ;;
  "is-active medium-control-plane.service")
    printf 'active\n'
    ;;
  "is-enabled medium-node-agent.service")
    printf 'disabled\n'
    exit 1
    ;;
  "is-active medium-node-agent.service")
    printf 'inactive\n'
    exit 3
    ;;
  *)
    printf 'unexpected %s\n' "$*" >&2
    exit 9
    ;;
esac
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn write_unavailable_systemctl(path: &Path) -> anyhow::Result<()> {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
printf 'not available\n' >&2
exit 1
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[tokio::test]
async fn doctor_reports_missing_client_and_server_state() -> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let root_dir = temp.path().join("root");
    let systemctl_path = temp.path().join("unavailable-systemctl.sh");
    let paths = AppPaths::from_home(&home_dir);
    fs::create_dir_all(&home_dir)?;
    fs::create_dir_all(&root_dir)?;
    write_unavailable_systemctl(&systemctl_path)?;

    let _overlay_home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _home = EnvGuard::set_path("HOME", &home_dir);
    let _root = EnvGuard::set_path("MEDIUM_ROOT", &root_dir);
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    assert!(output.contains("config-dir: missing"));
    assert!(output.contains("join-state: missing"));
    assert!(output.contains("control-config: missing"));
    assert!(output.contains("control-config-valid: missing"));
    assert!(output.contains("node-config-valid: missing"));
    assert!(output.contains("control-db: missing"));
    assert!(output.contains("ssh-managed-config: missing"));
    assert!(output.contains("service medium-control-plane.service: unavailable"));
    assert!(output.contains("service medium-node-agent.service: unavailable"));
    assert!(output.contains(&paths.state_path.display().to_string()));
    Ok(())
}

#[tokio::test]
async fn doctor_reports_bootstrapped_files_and_service_statuses() -> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let root_dir = temp.path().join("root");
    let systemctl_path = temp.path().join("mock-systemctl.sh");
    let paths = AppPaths::from_home(&home_dir);
    fs::create_dir_all(&paths.app_config_dir)?;
    fs::create_dir_all(&paths.state_dir)?;
    fs::create_dir_all(&paths.ssh_config_dir)?;
    fs::create_dir_all(root_dir.join("etc/medium"))?;
    fs::create_dir_all(root_dir.join("var/lib/medium"))?;
    write_mock_systemctl(&systemctl_path)?;

    fs::write(
        &paths.state_path,
        r#"{
  "server_url": "https://control.example.test",
  "device_name": "laptop",
  "bootstrap_code": "bootstrap-123",
  "invite_version": 1
}
"#,
    )?;
    fs::write(
        &paths.ssh_config_path,
        "Include ~/.ssh/config.d/medium.conf\nHost existing\n  HostName example.test\n",
    )?;
    fs::write(
        &paths.overlay_ssh_config_path,
        "Host medium-laptop\n  HostName 198.51.100.20\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/control.toml"),
        "bind_addr = \"0.0.0.0:7777\"\ndatabase_url = \"sqlite:///tmp/control-plane.db\"\ncontrol_url = \"https://control.example.test\"\nshared_secret = \"secret\"\ncontrol_pin = \"ctrl_pub_123\"\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/node.toml"),
        "node_id = \"node-1\"\nnode_label = \"node-1\"\nbind_addr = \"198.51.100.24:17001\"\n\n[[services]]\nid = \"svc_ssh\"\nkind = \"ssh\"\ntarget = \"127.0.0.1:22\"\nuser_name = \"overlay\"\n",
    )?;
    fs::write(root_dir.join("var/lib/medium/control-plane.db"), [])?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _root = EnvGuard::set_path("MEDIUM_ROOT", &root_dir);
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);
    let _public_url = EnvGuard::set_str("OVERLAY_CONTROL_URL", "https://control.example.test");

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    assert!(output.contains("config-dir: ok"));
    assert!(output.contains("join-state: ok (device laptop via https://control.example.test)"));
    assert!(output.contains("control-config: ok"));
    assert!(output.contains("control-config-valid: ok"));
    assert!(output.contains("node-config: ok"));
    assert!(output.contains("node-config-valid: ok"));
    assert!(output.contains("control-db: ok"));
    assert!(output.contains("ssh-include: ok"));
    assert!(output.contains("ssh-managed-config: ok"));
    assert!(output.contains("service medium-control-plane.service: enabled, active"));
    assert!(output.contains("service medium-node-agent.service: disabled, inactive"));
    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn doctor_uses_macos_application_support_and_usr_local_bins() -> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let paths = AppPaths::from_home(&home_dir);
    fs::create_dir_all(&paths.app_config_dir)?;
    fs::create_dir_all(&paths.state_dir)?;
    fs::write(
        paths.app_config_dir.join("node.toml"),
        "node_id = \"mac-node\"\nnode_label = \"mac-node\"\nbind_addr = \"127.0.0.1:17001\"\n\n[[services]]\nid = \"svc_ssh\"\nkind = \"ssh\"\ntarget = \"127.0.0.1:22\"\nuser_name = \"overlay\"\n",
    )?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _clear_root = EnvGuard::set_str("MEDIUM_ROOT", "");
    unsafe {
        std::env::remove_var("MEDIUM_ROOT");
    }

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    let expected_node_config = format!(
        "node-config: ok ({})",
        paths.app_config_dir.join("node.toml").display()
    );
    assert!(
        output.contains(&expected_node_config),
        "expected {expected_node_config:?} in output:\n{output}"
    );
    assert!(output.contains("node-config-valid: ok"));
    assert!(output.contains("control-plane-bin:"));
    assert!(output.contains("/usr/local/bin/control-plane"));
    assert!(output.contains("/usr/local/bin/node-agent"));
    Ok(())
}

#[tokio::test]
async fn doctor_reads_legacy_state_and_ssh_without_migrating() -> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let root_dir = temp.path().join("root");
    let systemctl_path = temp.path().join("mock-systemctl.sh");
    let paths = AppPaths::from_home(&home_dir);
    let legacy_state_path = home_dir.join(".config/overlay/state.json");
    let legacy_managed_path = home_dir.join(".ssh/config.d/overlay.conf");
    fs::create_dir_all(legacy_state_path.parent().expect("legacy state dir"))?;
    fs::create_dir_all(paths.ssh_config_dir.clone())?;
    fs::create_dir_all(root_dir.join("etc/medium"))?;
    fs::create_dir_all(root_dir.join("var/lib/medium"))?;
    write_mock_systemctl(&systemctl_path)?;

    fs::write(
        &legacy_state_path,
        r#"{
  "server_url": "https://legacy-control.example.test",
  "node_name": "legacy-laptop",
  "bootstrap_code": "legacy-bootstrap",
  "invite_version": 1
}
"#,
    )?;
    fs::write(
        &paths.ssh_config_path,
        "Include ~/.ssh/config.d/overlay.conf\nHost existing\n  HostName example.test\n",
    )?;
    fs::write(
        &legacy_managed_path,
        "# Managed by overlay. DO NOT EDIT.\n\nHost node-1\n  ProxyCommand overlay proxy ssh --device node-1\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/control.toml"),
        "bind_addr = \"0.0.0.0:7777\"\ndatabase_url = \"sqlite:///tmp/control-plane.db\"\ncontrol_url = \"https://control.example.test\"\nshared_secret = \"secret\"\ncontrol_pin = \"ctrl_pub_123\"\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/node.toml"),
        "node_id = \"node-1\"\nnode_label = \"node-1\"\nbind_addr = \"198.51.100.24:17001\"\n\n[[services]]\nid = \"svc_ssh\"\nkind = \"ssh\"\ntarget = \"127.0.0.1:22\"\nuser_name = \"overlay\"\n",
    )?;
    fs::write(root_dir.join("var/lib/medium/control-plane.db"), [])?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _root = EnvGuard::set_path("MEDIUM_ROOT", &root_dir);
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    assert!(
        output.contains(
            "join-state: ok (device legacy-laptop via https://legacy-control.example.test"
        )
    );
    assert!(output.contains("legacy state"));
    assert!(output.contains("ssh-include: ok (legacy overlay.conf)"));
    assert!(output.contains("ssh-managed-config: ok (legacy"));
    assert!(!paths.state_path.exists());
    assert!(legacy_state_path.is_file());
    assert!(!paths.overlay_ssh_config_path.exists());
    assert!(legacy_managed_path.is_file());
    Ok(())
}

#[tokio::test]
async fn doctor_reports_structurally_invalid_configs() -> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let root_dir = temp.path().join("root");
    let systemctl_path = temp.path().join("unavailable-systemctl.sh");
    let paths = AppPaths::from_home(&home_dir);
    fs::create_dir_all(&paths.app_config_dir)?;
    fs::create_dir_all(&paths.state_dir)?;
    fs::create_dir_all(root_dir.join("etc/medium"))?;
    fs::create_dir_all(root_dir.join("var/lib/medium"))?;
    write_unavailable_systemctl(&systemctl_path)?;

    fs::write(
        root_dir.join("etc/medium/control.toml"),
        "# bind_addr = \"0.0.0.0:7777\"\n# database_url = \"sqlite:///tmp/control-plane.db\"\ncontrol_url = \"https://control.example.test\"\nshared_secret = \"secret\"\ncontrol_pin = \"ctrl_pub_123\"\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/node.toml"),
        "# node_id = \"node-1\"\nnode_label = \"node-1\"\n",
    )?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _root = EnvGuard::set_path("MEDIUM_ROOT", &root_dir);
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    assert!(output.contains("control-config: ok"));
    assert!(output.contains("control-config-valid: invalid"));
    assert!(output.contains("node-config: ok"));
    assert!(output.contains("node-config-valid: invalid"));
    Ok(())
}

#[tokio::test]
async fn doctor_does_not_treat_user_owned_overlay_conf_as_legacy_managed_state()
-> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let root_dir = temp.path().join("root");
    let systemctl_path = temp.path().join("mock-systemctl.sh");
    let paths = AppPaths::from_home(&home_dir);
    let user_overlay_path = paths.ssh_config_dir.join("overlay.conf");
    fs::create_dir_all(&paths.app_config_dir)?;
    fs::create_dir_all(&paths.state_dir)?;
    fs::create_dir_all(&paths.ssh_config_dir)?;
    fs::create_dir_all(root_dir.join("etc/medium"))?;
    fs::create_dir_all(root_dir.join("var/lib/medium"))?;
    write_mock_systemctl(&systemctl_path)?;

    fs::write(
        &paths.ssh_config_path,
        "Include ~/.ssh/config.d/overlay.conf\nHost existing\n  HostName example.test\n",
    )?;
    fs::write(
        &user_overlay_path,
        "Host corp-bastion\n  HostName bastion.example.com\n  User alice\n",
    )?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _root = EnvGuard::set_path("MEDIUM_ROOT", &root_dir);
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    assert!(output.contains("ssh-include: missing"));
    assert!(output.contains("ssh-managed-config: missing"));
    assert!(!output.contains("legacy overlay.conf"));
    Ok(())
}

#[tokio::test]
async fn doctor_prefers_current_medium_ssh_state_over_legacy_overlay_state() -> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let root_dir = temp.path().join("root");
    let systemctl_path = temp.path().join("mock-systemctl.sh");
    let paths = AppPaths::from_home(&home_dir);
    let legacy_overlay_path = paths.ssh_config_dir.join("overlay.conf");
    fs::create_dir_all(&paths.app_config_dir)?;
    fs::create_dir_all(&paths.state_dir)?;
    fs::create_dir_all(&paths.ssh_config_dir)?;
    fs::create_dir_all(root_dir.join("etc/medium"))?;
    fs::create_dir_all(root_dir.join("var/lib/medium"))?;
    write_mock_systemctl(&systemctl_path)?;

    fs::write(
        &paths.ssh_config_path,
        "Include ~/.ssh/config.d/overlay.conf\nInclude ~/.ssh/config.d/medium.conf\n",
    )?;
    fs::write(
        &legacy_overlay_path,
        "# Managed by overlay. DO NOT EDIT.\n\nHost node-1\n  ProxyCommand overlay proxy ssh --device node-1\n",
    )?;
    fs::write(
        &paths.overlay_ssh_config_path,
        "# Managed by medium. DO NOT EDIT.\n\nHost node-1\n  ProxyCommand medium proxy ssh --device node-1\n",
    )?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _root = EnvGuard::set_path("MEDIUM_ROOT", &root_dir);
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    assert!(output.contains("ssh-include: ok"));
    assert!(output.contains("ssh-managed-config: ok"));
    assert!(!output.contains("ssh-include: ok (legacy overlay.conf)"));
    assert!(!output.contains("ssh-managed-config: ok (legacy overlay.conf)"));
    Ok(())
}
