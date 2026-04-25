use crate::client_api;
use anyhow::{Context, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_CONTROL_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_NODE_ID: &str = "node-1";
const DEFAULT_SSH_SERVICE_ID: &str = "svc_ssh";
const DEFAULT_SSH_TARGET: &str = "127.0.0.1:22";
const DEFAULT_SSH_USER: &str = "overlay";
const CONTROL_PLANE_UNIT_TEMPLATE: &str =
    include_str!("../../../packaging/systemd/medium-control-plane.service");
const NODE_AGENT_UNIT_TEMPLATE: &str =
    include_str!("../../../packaging/systemd/medium-node-agent.service");

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
    systemd_unit_dir: PathBuf,
    control_config_path: PathBuf,
    node_config_path: PathBuf,
    database_path: PathBuf,
    control_unit_path: PathBuf,
    node_unit_path: PathBuf,
}

impl InstallLayout {
    fn new(root: &Path) -> Self {
        let config_dir = root.join("etc").join("medium");
        let state_dir = root.join("var").join("lib").join("medium");
        let systemd_unit_dir = root.join("etc").join("systemd").join("system");

        Self {
            control_config_path: config_dir.join("control.toml"),
            node_config_path: config_dir.join("node.toml"),
            database_path: state_dir.join("control-plane.db"),
            control_unit_path: systemd_unit_dir.join("medium-control-plane.service"),
            node_unit_path: systemd_unit_dir.join("medium-node-agent.service"),
            config_dir,
            state_dir,
            systemd_unit_dir,
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
    let node_addrs = match node_addrs() {
        Ok(value) => value,
        Err(error) => {
            config_errors.push(error.to_string());
            NodeAddrs {
                listen_addr: String::new(),
                public_addr: String::new(),
            }
        }
    };
    if !config_errors.is_empty() {
        bail!(config_errors.join("; "));
    }
    init_control_at(&root, &bind_addr, &control_url, &node_addrs, reconfigure)
}

fn init_control_at(
    root: &Path,
    bind_addr: &str,
    control_url: &str,
    node_addrs: &NodeAddrs,
    reconfigure: bool,
) -> anyhow::Result<InitControlReport> {
    let layout = InstallLayout::new(root);
    if layout.is_bootstrapped() && !reconfigure {
        bail!(
            "Medium control is already initialized; rerun with --reconfigure to rewrite bootstrap files"
        );
    }

    fs::create_dir_all(&layout.config_dir)
        .with_context(|| format!("create {}", layout.config_dir.display()))?;
    fs::create_dir_all(&layout.state_dir)
        .with_context(|| format!("create {}", layout.state_dir.display()))?;

    let shared_secret = make_token("medium-shared-secret");
    let control_key = make_token("medium-control-key");
    let invite = client_api::format_join_invite(control_url, &control_key)?;

    write_control_config(
        &layout.control_config_path,
        bind_addr,
        control_url,
        &layout.database_path,
        &shared_secret,
        &control_key,
    )?;
    write_home_node_config(
        &layout.node_config_path,
        DEFAULT_NODE_ID,
        &node_addrs.listen_addr,
        &node_addrs.public_addr,
        DEFAULT_SSH_SERVICE_ID,
    )?;
    touch_file(&layout.database_path)?;
    write_systemd_units(&layout, root, bind_addr, control_url, &shared_secret)?;
    maybe_enable_systemd_services(root)?;

    Ok(InitControlReport {
        control_config_path: layout.control_config_path,
        node_config_path: layout.node_config_path,
        database_path: layout.database_path,
        invite,
    })
}

pub(crate) fn install_root() -> PathBuf {
    std::env::var_os("MEDIUM_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn control_bind_addr() -> String {
    std::env::var("MEDIUM_CONTROL_BIND_ADDR").unwrap_or_else(|_| DEFAULT_CONTROL_BIND_ADDR.into())
}

fn control_public_url(bind_addr: &str) -> anyhow::Result<String> {
    if let Some(url) =
        env_string("OVERLAY_CONTROL_URL").or_else(|| env_string("MEDIUM_CONTROL_PUBLIC_URL"))
    {
        return client_api::format_join_invite(&url, "control-key-placeholder")
            .map(|_| url)
            .map_err(|error| anyhow::anyhow!("invalid public control URL: {error}"));
    }

    let host = split_host_port(bind_addr)
        .map(|(host, _port)| host)
        .ok_or_else(|| anyhow::anyhow!("MEDIUM_CONTROL_BIND_ADDR must include host:port"))?;
    if is_unsuitable_public_host(host) {
        bail!(
            "MEDIUM_CONTROL_PUBLIC_URL must be set to the public control URL when MEDIUM_CONTROL_BIND_ADDR uses {}",
            host
        );
    }

    Ok(format!("http://{bind_addr}"))
}

struct NodeAddrs {
    listen_addr: String,
    public_addr: String,
}

fn node_addrs() -> anyhow::Result<NodeAddrs> {
    if let Some(legacy_addr) = env_string("MEDIUM_HOME_NODE_BIND_ADDR")
        .or_else(|| env_string("OVERLAY_HOME_NODE_BIND_ADDR"))
    {
        return Ok(NodeAddrs {
            listen_addr: legacy_addr.clone(),
            public_addr: legacy_addr,
        });
    }

    let listen_addr =
        env_string("MEDIUM_NODE_LISTEN_ADDR").unwrap_or_else(|| "0.0.0.0:17001".to_string());
    let public_addr = if let Some(public_addr) = env_string("MEDIUM_NODE_PUBLIC_ADDR") {
        public_addr
    } else {
        let host = split_host_port(&listen_addr)
            .map(|(host, _port)| host)
            .ok_or_else(|| anyhow::anyhow!("MEDIUM_NODE_LISTEN_ADDR must include host:port"))?;
        if is_unsuitable_public_host(host) {
            bail!(
                "MEDIUM_NODE_PUBLIC_ADDR must be set when MEDIUM_NODE_LISTEN_ADDR uses {}",
                host
            );
        }
        listen_addr.clone()
    };

    Ok(NodeAddrs {
        listen_addr,
        public_addr,
    })
}

fn write_control_config(
    path: &Path,
    bind_addr: &str,
    control_url: &str,
    database_path: &Path,
    shared_secret: &str,
    control_key: &str,
) -> anyhow::Result<()> {
    let contents = format!(
        "# Generated by medium init-control\nbind_addr = \"{bind_addr}\"\ndatabase_url = \"sqlite://{}\"\ncontrol_url = \"{control_url}\"\nshared_secret = \"{shared_secret}\"\ncontrol_key = \"{control_key}\"\n",
        database_path.display()
    );
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

fn write_home_node_config(
    path: &Path,
    node_id: &str,
    bind_addr: &str,
    public_addr: &str,
    ssh_service_id: &str,
) -> anyhow::Result<()> {
    let contents = format!(
        "node_id = \"{node_id}\"\nnode_label = \"{node_id}\"\nbind_addr = \"{bind_addr}\"\npublic_addr = \"{public_addr}\"\n\n[[services]]\nid = \"{ssh_service_id}\"\nkind = \"ssh\"\ntarget = \"{DEFAULT_SSH_TARGET}\"\nuser_name = \"{DEFAULT_SSH_USER}\"\n"
    );
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

fn write_systemd_units(
    layout: &InstallLayout,
    root: &Path,
    bind_addr: &str,
    control_url: &str,
    shared_secret: &str,
) -> anyhow::Result<()> {
    fs::create_dir_all(&layout.systemd_unit_dir)
        .with_context(|| format!("create {}", layout.systemd_unit_dir.display()))?;
    fs::write(
        &layout.control_unit_path,
        render_control_plane_unit(root, layout, bind_addr, shared_secret),
    )
    .with_context(|| format!("write {}", layout.control_unit_path.display()))?;
    fs::write(
        &layout.node_unit_path,
        render_node_agent_unit(root, layout, control_url, shared_secret),
    )
    .with_context(|| format!("write {}", layout.node_unit_path.display()))?;
    Ok(())
}

fn render_control_plane_unit(
    root: &Path,
    layout: &InstallLayout,
    bind_addr: &str,
    shared_secret: &str,
) -> String {
    render_unit(
        CONTROL_PLANE_UNIT_TEMPLATE,
        &[
            (
                "{{CONTROL_PLANE_BIN}}",
                &control_plane_binary_path(root).display().to_string(),
            ),
            ("{{CONTROL_BIND_ADDR}}", bind_addr),
            (
                "{{DATABASE_URL}}",
                &format!("sqlite://{}", layout.database_path.display()),
            ),
            ("{{SHARED_SECRET}}", shared_secret),
            ("{{STATE_DIR}}", &layout.state_dir.display().to_string()),
        ],
    )
}

fn render_node_agent_unit(
    root: &Path,
    layout: &InstallLayout,
    control_url: &str,
    shared_secret: &str,
) -> String {
    render_unit(
        NODE_AGENT_UNIT_TEMPLATE,
        &[
            (
                "{{NODE_AGENT_BIN}}",
                &node_agent_binary_path(root).display().to_string(),
            ),
            (
                "{{NODE_CONFIG_PATH}}",
                &layout.node_config_path.display().to_string(),
            ),
            ("{{CONTROL_URL}}", control_url),
            ("{{SHARED_SECRET}}", shared_secret),
        ],
    )
}

fn render_unit(template: &str, replacements: &[(&str, &str)]) -> String {
    let mut rendered = template.to_string();
    for (needle, replacement) in replacements {
        rendered = rendered.replace(needle, replacement);
    }
    rendered
}

pub(crate) fn control_plane_binary_path(root: &Path) -> PathBuf {
    if root == Path::new("/") {
        PathBuf::from("/usr/bin/control-plane")
    } else {
        root.join("usr").join("bin").join("control-plane")
    }
}

pub(crate) fn node_agent_binary_path(root: &Path) -> PathBuf {
    if root == Path::new("/") {
        PathBuf::from("/usr/bin/node-agent")
    } else {
        root.join("usr").join("bin").join("node-agent")
    }
}

fn maybe_enable_systemd_services(root: &Path) -> anyhow::Result<()> {
    if root != Path::new("/") && env_string("MEDIUM_SYSTEMCTL_BIN").is_none() {
        return Ok(());
    }

    let systemctl = systemctl_bin();
    run_command(&systemctl, &["daemon-reload"])?;
    run_command(
        &systemctl,
        &["enable", "--now", "medium-control-plane.service"],
    )?;
    run_command(
        &systemctl,
        &["enable", "--now", "medium-node-agent.service"],
    )?;
    Ok(())
}

pub(crate) fn systemctl_bin() -> String {
    env_string("MEDIUM_SYSTEMCTL_BIN").unwrap_or_else(|| "systemctl".into())
}

fn run_command(command: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(command)
        .args(args)
        .output()
        .with_context(|| format!("run {} {}", command, args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!(
            "command failed: {} {} (status {})",
            command,
            args.join(" "),
            output.status
        );
    }

    bail!("command failed: {} {}: {}", command, args.join(" "), stderr);
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
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
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
