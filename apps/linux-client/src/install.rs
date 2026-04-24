use crate::client_api;
use anyhow::{Context, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_CONTROL_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_NODE_ID: &str = "node-home";
const DEFAULT_SSH_SERVICE_ID: &str = "svc_home_ssh";
const DEFAULT_SSH_TARGET: &str = "127.0.0.1:22";

#[allow(dead_code)]
pub struct InitControlReport {
    pub control_config_path: PathBuf,
    pub node_config_path: PathBuf,
    pub database_path: PathBuf,
    pub invite: String,
}

struct InstallLayout {
    config_dir: PathBuf,
    state_dir: PathBuf,
    control_config_path: PathBuf,
    node_config_path: PathBuf,
    database_path: PathBuf,
}

impl InstallLayout {
    fn new(root: &Path) -> Self {
        let config_dir = root.join("etc").join("medium");
        let state_dir = root.join("var").join("lib").join("medium");

        Self {
            control_config_path: config_dir.join("control.toml"),
            node_config_path: config_dir.join("node.toml"),
            database_path: state_dir.join("control-plane.db"),
            config_dir,
            state_dir,
        }
    }

    fn is_bootstrapped(&self) -> bool {
        self.control_config_path.exists()
            || self.node_config_path.exists()
            || self.database_path.exists()
            || self.config_dir.exists()
            || self.state_dir.exists()
    }
}

pub fn init_control(reconfigure: bool) -> anyhow::Result<InitControlReport> {
    let root = install_root();
    let bind_addr = control_bind_addr();
    init_control_at(&root, &bind_addr, reconfigure)
}

fn init_control_at(
    root: &Path,
    bind_addr: &str,
    reconfigure: bool,
) -> anyhow::Result<InitControlReport> {
    let layout = InstallLayout::new(root);
    if layout.is_bootstrapped() && !reconfigure {
        bail!("Medium control is already initialized; rerun with --reconfigure to rewrite bootstrap files");
    }

    fs::create_dir_all(&layout.config_dir)
        .with_context(|| format!("create {}", layout.config_dir.display()))?;
    fs::create_dir_all(&layout.state_dir)
        .with_context(|| format!("create {}", layout.state_dir.display()))?;

    let shared_secret = make_token("medium-shared-secret");
    let bootstrap_token = make_token("medium-bootstrap");
    let control_url = format!("http://{bind_addr}");
    let invite = client_api::format_join_invite(&control_url, &bootstrap_token)?;

    write_control_config(
        &layout.control_config_path,
        bind_addr,
        &layout.database_path,
        &shared_secret,
    )?;
    write_home_node_config(&layout.node_config_path, DEFAULT_NODE_ID, DEFAULT_SSH_SERVICE_ID)?;
    touch_file(&layout.database_path)?;

    Ok(InitControlReport {
        control_config_path: layout.control_config_path,
        node_config_path: layout.node_config_path,
        database_path: layout.database_path,
        invite,
    })
}

fn install_root() -> PathBuf {
    std::env::var_os("MEDIUM_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn control_bind_addr() -> String {
    std::env::var("MEDIUM_CONTROL_BIND_ADDR").unwrap_or_else(|_| DEFAULT_CONTROL_BIND_ADDR.into())
}

fn write_control_config(
    path: &Path,
    bind_addr: &str,
    database_path: &Path,
    shared_secret: &str,
) -> anyhow::Result<()> {
    let contents = format!(
        "bind_addr = \"{bind_addr}\"\ndatabase_url = \"sqlite://{}\"\nshared_secret = \"{shared_secret}\"\n",
        database_path.display()
    );
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

fn write_home_node_config(path: &Path, node_id: &str, ssh_service_id: &str) -> anyhow::Result<()> {
    let contents = format!(
        "node_id = \"{node_id}\"\n\n[[services]]\nid = \"{ssh_service_id}\"\nkind = \"ssh\"\ntarget = \"{DEFAULT_SSH_TARGET}\"\n"
    );
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

fn touch_file(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }

    fs::write(path, []).with_context(|| format!("write {}", path.display()))
}

fn make_token(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}-{now:x}")
}
