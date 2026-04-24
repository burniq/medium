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
    let _guard = env_lock().lock().unwrap();
    let temp = tempfile::tempdir()?;
    let _root = EnvGuard::set("MEDIUM_ROOT", temp.path());

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
    assert!(control_config.contains("127.0.0.1:8080"));
    assert!(control_config.contains(&database_path.display().to_string()));

    let node_config = fs::read_to_string(&node_config_path)?;
    assert!(node_config.contains("node-home"));
    assert!(node_config.contains("svc_home_ssh"));

    assert!(output.contains("initialized Medium control"));
    assert!(output.contains("medium://join?v=1&control=http://127.0.0.1:8080&token="));
    Ok(())
}

#[tokio::test]
async fn init_control_refuses_existing_install_without_reconfigure() -> anyhow::Result<()> {
    let _guard = env_lock().lock().unwrap();
    let temp = tempfile::tempdir()?;
    let _root = EnvGuard::set("MEDIUM_ROOT", temp.path());

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
