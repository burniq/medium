use linux_client::run_main;
use std::fs;
use std::path::{Path, PathBuf};
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should resolve")
}

fn write_mock_systemctl(path: &Path) -> anyhow::Result<()> {
    fs::write(
        path,
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"$MEDIUM_SYSTEMCTL_LOG\"\n",
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
async fn init_control_renders_units_and_enables_services() -> anyhow::Result<()> {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let systemctl_path = temp.path().join("mock-systemctl.sh");
    let systemctl_log = temp.path().join("systemctl.log");
    write_mock_systemctl(&systemctl_path)?;

    let _root = EnvGuard::set_path("MEDIUM_ROOT", temp.path());
    let _public_url = EnvGuard::set_str("OVERLAY_CONTROL_URL", "https://control.example.test");
    let _control_bind = EnvGuard::set_str("MEDIUM_CONTROL_BIND_ADDR", "0.0.0.0:8080");
    let _node_public = EnvGuard::set_str("MEDIUM_NODE_PUBLIC_ADDR", "198.51.100.24:17001");
    let _systemctl_bin = EnvGuard::set_path("MEDIUM_SYSTEMCTL_BIN", &systemctl_path);
    let _systemctl_log = EnvGuard::set_path("MEDIUM_SYSTEMCTL_LOG", &systemctl_log);

    let output = run_main(vec!["medium".to_string(), "init-control".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("init-control should return a summary");

    let control_unit_path = temp
        .path()
        .join("etc/systemd/system/medium-control-plane.service");
    let node_unit_path = temp
        .path()
        .join("etc/systemd/system/medium-node-agent.service");

    assert!(control_unit_path.is_file());
    assert!(node_unit_path.is_file());

    let control_unit = fs::read_to_string(&control_unit_path)?;
    assert!(control_unit.contains(&format!(
        "ExecStart={}",
        temp.path().join("usr/bin/control-plane").display()
    )));
    assert!(control_unit.contains(&format!(
        "Environment=OVERLAY_CONTROL_BIND_ADDR={}",
        "0.0.0.0:8080"
    )));
    assert!(control_unit.contains(&format!(
        "Environment=OVERLAY_CONTROL_DATABASE_URL=sqlite://{}",
        temp.path().join("var/lib/medium/control-plane.db").display()
    )));
    let control_config = fs::read_to_string(temp.path().join("etc/medium/control.toml"))?;
    let shared_secret_line = control_config
        .lines()
        .find(|line| line.starts_with("shared_secret = "))
        .expect("shared_secret should be present in control config");
    let shared_secret = shared_secret_line
        .trim_start_matches("shared_secret = \"")
        .trim_end_matches('"');
    assert!(control_unit.contains(&format!(
        "Environment=OVERLAY_SHARED_SECRET={shared_secret}"
    )));
    assert!(control_unit.contains(&format!(
        "WorkingDirectory={}",
        temp.path().join("var/lib/medium").display()
    )));
    assert!(!control_unit.contains("medium serve"));
    assert!(!control_unit.contains("MEDIUM_CONTROL_DATABASE_URL"));

    let node_unit = fs::read_to_string(&node_unit_path)?;
    assert!(node_unit.contains(&format!(
        "ExecStart={} --config {}",
        temp.path().join("usr/bin/node-agent").display(),
        temp.path().join("etc/medium/node.toml").display()
    )));
    assert!(node_unit.contains("Environment=OVERLAY_CONTROL_URL=https://control.example.test"));
    assert!(node_unit.contains(&format!(
        "Environment=OVERLAY_SHARED_SECRET={shared_secret}"
    )));
    assert!(!node_unit.contains("medium serve"));
    assert!(!node_unit.contains("http://127.0.0.1:8080"));

    let template_root = repo_root().join("packaging/systemd");
    assert!(template_root.join("medium-control-plane.service").is_file());
    assert!(template_root.join("medium-node-agent.service").is_file());

    let commands = fs::read_to_string(&systemctl_log)?;
    assert_eq!(
        commands.lines().collect::<Vec<_>>(),
        vec![
            "daemon-reload",
            "enable --now medium-control-plane.service",
            "enable --now medium-node-agent.service",
        ]
    );

    assert!(output.contains("initialized Medium control"));
    Ok(())
}
