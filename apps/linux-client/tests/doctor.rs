use linux_client::run_main;
use linux_client::paths::AppPaths;
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
  "is-enabled medium-home-node.service")
    printf 'disabled\n'
    exit 1
    ;;
  "is-active medium-home-node.service")
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
    let _guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let home_dir = temp.path().join("home");
    let root_dir = temp.path().join("root");
    let systemctl_path = temp.path().join("unavailable-systemctl.sh");
    let paths = AppPaths::from_home(&home_dir);
    fs::create_dir_all(&home_dir)?;
    fs::create_dir_all(&root_dir)?;
    write_unavailable_systemctl(&systemctl_path)?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
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
    assert!(output.contains("service medium-home-node.service: unavailable"));
    assert!(output.contains(&paths.state_path.display().to_string()));
    Ok(())
}

#[tokio::test]
async fn doctor_reports_bootstrapped_files_and_service_statuses() -> anyhow::Result<()> {
    let _guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
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
    fs::write(&paths.overlay_ssh_config_path, "Host medium-laptop\n  HostName 198.51.100.20\n")?;
    fs::write(
        root_dir.join("etc/medium/control.toml"),
        "bind_addr = \"0.0.0.0:8080\"\ndatabase_url = \"sqlite:///tmp/control-plane.db\"\ncontrol_url = \"https://control.example.test\"\nshared_secret = \"secret\"\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/node.toml"),
        "node_id = \"node-home\"\nnode_label = \"node-home\"\nbind_addr = \"198.51.100.24:17001\"\n\n[[services]]\nid = \"svc_home_ssh\"\nkind = \"ssh\"\ntarget = \"127.0.0.1:22\"\nuser_name = \"overlay\"\n",
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
    assert!(output.contains(
        "service medium-control-plane.service: enabled, active"
    ));
    assert!(output.contains(
        "service medium-home-node.service: disabled, inactive"
    ));
    Ok(())
}

#[tokio::test]
async fn doctor_reads_legacy_state_and_ssh_without_migrating() -> anyhow::Result<()> {
    let _guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
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
    fs::write(&legacy_managed_path, "Host overlay-legacy\n  HostName 198.51.100.44\n")?;
    fs::write(
        root_dir.join("etc/medium/control.toml"),
        "bind_addr = \"0.0.0.0:8080\"\ndatabase_url = \"sqlite:///tmp/control-plane.db\"\ncontrol_url = \"https://control.example.test\"\nshared_secret = \"secret\"\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/node.toml"),
        "node_id = \"node-home\"\nnode_label = \"node-home\"\nbind_addr = \"198.51.100.24:17001\"\n\n[[services]]\nid = \"svc_home_ssh\"\nkind = \"ssh\"\ntarget = \"127.0.0.1:22\"\nuser_name = \"overlay\"\n",
    )?;
    fs::write(root_dir.join("var/lib/medium/control-plane.db"), [])?;

    let _home = EnvGuard::set_path("OVERLAY_HOME", &home_dir);
    let _root = EnvGuard::set_path("MEDIUM_ROOT", &root_dir);
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);

    let output = run_main(vec!["medium".to_string(), "doctor".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("doctor should return a report");

    assert!(output.contains(
        "join-state: ok (device legacy-laptop via https://legacy-control.example.test"
    ));
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
    let _guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
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
        "# bind_addr = \"0.0.0.0:8080\"\n# database_url = \"sqlite:///tmp/control-plane.db\"\ncontrol_url = \"https://control.example.test\"\nshared_secret = \"secret\"\n",
    )?;
    fs::write(
        root_dir.join("etc/medium/node.toml"),
        "# node_id = \"node-home\"\nnode_label = \"node-home\"\n",
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
