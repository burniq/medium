use crate::client_api;
use anyhow::{Context, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_CONTROL_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_NODE_ID: &str = "node-home";
const DEFAULT_SSH_SERVICE_ID: &str = "svc_home_ssh";
const DEFAULT_SSH_TARGET: &str = "127.0.0.1:22";
const DEFAULT_SSH_USER: &str = "overlay";

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
    let mut config_errors = Vec::new();
    let control_url = match control_public_url(&bind_addr) {
        Ok(value) => value,
        Err(error) => {
            config_errors.push(error.to_string());
            String::new()
        }
    };
    let node_bind_addr = match home_node_bind_addr() {
        Ok(value) => value,
        Err(error) => {
            config_errors.push(error.to_string());
            String::new()
        }
    };
    if !config_errors.is_empty() {
        bail!(config_errors.join("; "));
    }
    init_control_at(&root, &bind_addr, &control_url, &node_bind_addr, reconfigure)
}

fn init_control_at(
    root: &Path,
    bind_addr: &str,
    control_url: &str,
    node_bind_addr: &str,
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
    let invite = client_api::format_join_invite(control_url, &bootstrap_token)?;

    write_control_config(
        &layout.control_config_path,
        bind_addr,
        control_url,
        &layout.database_path,
        &shared_secret,
    )?;
    write_home_node_config(
        &layout.node_config_path,
        DEFAULT_NODE_ID,
        node_bind_addr,
        DEFAULT_SSH_SERVICE_ID,
    )?;
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

fn control_public_url(bind_addr: &str) -> anyhow::Result<String> {
    if let Some(url) = env_string("OVERLAY_CONTROL_URL")
        .or_else(|| env_string("MEDIUM_CONTROL_PUBLIC_URL"))
    {
        return client_api::format_join_invite(&url, "bootstrap-placeholder")
            .map(|_| url)
            .map_err(|error| anyhow::anyhow!("invalid public control URL: {error}"));
    }

    let host = split_host_port(bind_addr)
        .map(|(host, _port)| host)
        .ok_or_else(|| anyhow::anyhow!("MEDIUM_CONTROL_BIND_ADDR must include host:port"))?;
    if is_unsuitable_public_host(host) {
        bail!(
            "OVERLAY_CONTROL_URL must be set to the public control URL when MEDIUM_CONTROL_BIND_ADDR uses {}",
            host
        );
    }

    Ok(format!("http://{bind_addr}"))
}

fn home_node_bind_addr() -> anyhow::Result<String> {
    env_string("MEDIUM_HOME_NODE_BIND_ADDR")
        .or_else(|| env_string("OVERLAY_HOME_NODE_BIND_ADDR"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "MEDIUM_HOME_NODE_BIND_ADDR must be set to the reachable home-node TCP proxy address"
            )
        })
}

fn write_control_config(
    path: &Path,
    bind_addr: &str,
    control_url: &str,
    database_path: &Path,
    shared_secret: &str,
) -> anyhow::Result<()> {
    let contents = format!(
        "# Generated by medium init-control\nOVERLAY_CONTROL_BIND_ADDR={bind_addr}\nOVERLAY_CONTROL_DATABASE_URL=sqlite://{}\nOVERLAY_CONTROL_URL={control_url}\nOVERLAY_SHARED_SECRET={shared_secret}\n",
        database_path.display()
    );
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

fn write_home_node_config(
    path: &Path,
    node_id: &str,
    bind_addr: &str,
    ssh_service_id: &str,
) -> anyhow::Result<()> {
    let contents = format!(
        "node_id = \"{node_id}\"\nnode_label = \"{node_id}\"\nbind_addr = \"{bind_addr}\"\n\n[[services]]\nid = \"{ssh_service_id}\"\nkind = \"ssh\"\ntarget = \"{DEFAULT_SSH_TARGET}\"\nuser_name = \"{DEFAULT_SSH_USER}\"\n"
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

fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.trim().is_empty())
}

fn split_host_port(value: &str) -> Option<(&str, &str)> {
    if let Some(rest) = value.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        return Some((host, port));
    }

    value.rsplit_once(':')
}

fn is_unsuitable_public_host(host: &str) -> bool {
    matches!(host, "0.0.0.0" | "::" | "127.0.0.1" | "::1" | "localhost")
}
