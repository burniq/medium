use home_node::config::load_from_path;
use home_node::control::build_registration;
use linux_client::run_main;
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
    fn set(key: &'static str, value: &Path) -> Self {
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

#[tokio::test]
async fn init_control_creates_expected_paths_and_files() -> anyhow::Result<()> {
    let _guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let _root = EnvGuard::set("MEDIUM_ROOT", temp.path());
    let _public_url = EnvGuard::set_str("OVERLAY_CONTROL_URL", "https://control.example.test");
    let _control_bind = EnvGuard::set_str("MEDIUM_CONTROL_BIND_ADDR", "0.0.0.0:8080");
    let _node_bind = EnvGuard::set_str("MEDIUM_HOME_NODE_BIND_ADDR", "198.51.100.24:17001");

    let output = run_main(vec!["medium".to_string(), "init-control".to_string()])
        .await
        .map_err(anyhow::Error::msg)?
        .expect("init-control should return a summary");

    let control_config_path = temp.path().join("etc/medium/control.toml");
    let node_config_path = temp.path().join("etc/medium/node.toml");
    let database_path = temp.path().join("var/lib/medium/control-plane.db");

    assert!(control_config_path.is_file());
    assert!(node_config_path.is_file());
    assert!(database_path.is_file());

    let control_config = fs::read_to_string(&control_config_path)?;
    assert!(control_config.contains("bind_addr = \"0.0.0.0:8080\""));
    assert!(control_config.contains("control_url = \"https://control.example.test\""));
    assert!(control_config.contains("database_url = \"sqlite://"));
    assert!(control_config.contains("shared_secret = \""));
    assert!(control_config.contains(&format!(
        "database_url = \"sqlite://{}\"",
        database_path.display()
    )));

    let node_config = load_from_path(&node_config_path)?;
    assert_eq!(node_config.node_id, "node-home");
    assert_eq!(node_config.bind_addr, "198.51.100.24:17001");
    assert_eq!(node_config.services.len(), 1);
    assert_eq!(node_config.services[0].id, "svc_home_ssh");
    assert_eq!(node_config.services[0].user_name.as_deref(), Some("overlay"));

    let registration = build_registration(&node_config);
    assert_eq!(registration.endpoints.len(), 1);
    assert_eq!(registration.endpoints[0].addr, "198.51.100.24:17001");
    assert_eq!(
        registration.services[0].user_name.as_deref(),
        Some("overlay")
    );

    assert!(output.contains("initialized Medium control"));
    assert!(output.contains("medium://join?v=1&control=https://control.example.test&token="));
    Ok(())
}

#[tokio::test]
async fn init_control_refuses_existing_install_without_reconfigure() -> anyhow::Result<()> {
    let _guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let _root = EnvGuard::set("MEDIUM_ROOT", temp.path());
    let _public_url = EnvGuard::set_str("OVERLAY_CONTROL_URL", "https://control.example.test");
    let _node_bind = EnvGuard::set_str("MEDIUM_HOME_NODE_BIND_ADDR", "198.51.100.24:17001");

    run_main(vec!["medium".to_string(), "init-control".to_string()])
        .await
        .map_err(anyhow::Error::msg)?;

    let error = run_main(vec!["medium".to_string(), "init-control".to_string()])
        .await
        .unwrap_err();
    assert!(error.contains("--reconfigure"));

    let output = run_main(vec![
        "medium".to_string(),
        "init-control".to_string(),
        "--reconfigure".to_string(),
    ])
    .await
    .map_err(anyhow::Error::msg)?
    .expect("reconfigure should return a summary");
    assert!(output.contains("initialized Medium control"));
    Ok(())
}

#[tokio::test]
async fn init_control_requires_explicit_public_and_node_addresses_when_defaults_are_not_usable(
) -> anyhow::Result<()> {
    let _guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
    let temp = tempfile::tempdir()?;
    let _root = EnvGuard::set("MEDIUM_ROOT", temp.path());
    let _control_bind = EnvGuard::set_str("MEDIUM_CONTROL_BIND_ADDR", "0.0.0.0:8080");
    let _clear_public = EnvGuard::set_str("OVERLAY_CONTROL_URL", "");
    let _clear_node = EnvGuard::set_str("MEDIUM_HOME_NODE_BIND_ADDR", "");

    let error = run_main(vec!["medium".to_string(), "init-control".to_string()])
        .await
        .unwrap_err();

    assert!(error.contains("OVERLAY_CONTROL_URL"));
    assert!(error.contains("MEDIUM_HOME_NODE_BIND_ADDR"));
    Ok(())
}
