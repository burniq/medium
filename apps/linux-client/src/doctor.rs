use super::install;
use crate::paths::AppPaths;
use crate::state::AppState;
use home_node::config::load_from_path as load_node_config;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

const CONTROL_SERVICE: &str = "medium-control-plane.service";
const NODE_SERVICE: &str = "medium-home-node.service";

pub struct DoctorReport {
    pub lines: Vec<String>,
}

impl DoctorReport {
    pub fn render(&self) -> String {
        self.lines.join("\n")
    }
}

pub fn inspect(paths: &AppPaths) -> anyhow::Result<DoctorReport> {
    let root = install::install_root();
    let config_dir = root.join("etc").join("medium");
    let state_dir = root.join("var").join("lib").join("medium");
    let control_config_path = config_dir.join("control.toml");
    let node_config_path = config_dir.join("node.toml");
    let database_path = state_dir.join("control-plane.db");
    let ssh = inspect_ssh(paths)?;

    let mut lines = Vec::new();
    lines.push(path_line("config-dir", &paths.app_config_dir));
    lines.push(path_line("state-dir", &paths.state_dir));
    lines.push(state_line(paths)?);
    lines.push(path_line("control-config", &control_config_path));
    lines.push(control_config_line(&control_config_path)?);
    lines.push(path_line("node-config", &node_config_path));
    lines.push(node_config_line(&node_config_path));
    lines.push(path_line("control-db", &database_path));
    lines.push(path_line(
        "control-plane-bin",
        &install::control_plane_binary_path(&root),
    ));
    lines.push(path_line(
        "home-node-bin",
        &install::home_node_binary_path(&root),
    ));
    lines.push(ssh.include_line());
    lines.push(ssh.managed_line());
    lines.push(service_line(CONTROL_SERVICE));
    lines.push(service_line(NODE_SERVICE));

    Ok(DoctorReport { lines })
}

fn path_line(label: &str, path: &Path) -> String {
    format!(
        "{label}: {} ({})",
        if path.exists() { "ok" } else { "missing" },
        path.display()
    )
}

fn control_config_line(path: &Path) -> anyhow::Result<String> {
    if !path.is_file() {
        return Ok("control-config-valid: missing".into());
    }

    let raw = fs::read_to_string(path)?;
    let values = parse_simple_toml_strings(&raw);
    let missing_fields = ["bind_addr", "database_url", "control_url", "shared_secret"]
        .into_iter()
        .filter(|field| values.get(*field).is_none_or(|value| value.trim().is_empty()))
        .collect::<Vec<_>>();

    if missing_fields.is_empty() {
        return Ok("control-config-valid: ok".into());
    }

    Ok(format!(
        "control-config-valid: invalid (missing {})",
        missing_fields.join(", ")
    ))
}

fn node_config_line(path: &Path) -> String {
    if !path.is_file() {
        return "node-config-valid: missing".into();
    }

    match load_node_config(path) {
        Ok(config) if !config.node_id.trim().is_empty() && !config.bind_addr.trim().is_empty() => {
            "node-config-valid: ok".into()
        }
        Ok(_) => "node-config-valid: invalid (missing node_id or bind_addr)".into(),
        Err(error) => format!("node-config-valid: invalid ({error})"),
    }
}

fn state_line(paths: &AppPaths) -> anyhow::Result<String> {
    match read_state(paths, &paths.state_path)? {
        Some(state) => Ok(format!(
            "join-state: ok (device {} via {})",
            state.device_name, state.server_url
        )),
        None => {
            let legacy_path = legacy_state_path(paths);
            match read_state(paths, &legacy_path)? {
                Some(state) => Ok(format!(
                    "join-state: ok (device {} via {} from legacy state {})",
                    state.device_name,
                    state.server_url,
                    legacy_path.display()
                )),
                None => Ok(format!("join-state: missing ({})", paths.state_path.display())),
            }
        }
    }
}

fn read_state(paths: &AppPaths, path: &Path) -> anyhow::Result<Option<AppState>> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };

    let state = serde_json::from_str::<AppState>(&raw).map_err(|error| {
        anyhow::anyhow!("invalid state at {}: {error}", display_state_path(paths, path))
    })?;
    Ok(Some(state))
}

fn display_state_path(paths: &AppPaths, path: &Path) -> String {
    if path == paths.state_path {
        paths.state_path.display().to_string()
    } else {
        path.display().to_string()
    }
}

fn legacy_state_path(paths: &AppPaths) -> std::path::PathBuf {
    paths
        .home_dir
        .join(".config")
        .join("overlay")
        .join("state.json")
}

fn parse_simple_toml_strings(raw: &str) -> std::collections::BTreeMap<String, String> {
    raw.lines()
        .filter_map(parse_simple_toml_string_line)
        .collect()
}

fn parse_simple_toml_string_line(line: &str) -> Option<(String, String)> {
    let line = line.split_once('#').map_or(line, |(before, _)| before).trim();
    if line.is_empty() || line.starts_with('[') {
        return None;
    }

    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || !value.starts_with('"') || !value.ends_with('"') || value.len() < 2 {
        return None;
    }

    Some((key.to_string(), value[1..value.len() - 1].to_string()))
}

struct SshInspection {
    include_status: SshStatus,
    managed_status: SshStatus,
}

impl SshInspection {
    fn include_line(&self) -> String {
        format!("ssh-include: {}", self.include_status.render())
    }

    fn managed_line(&self) -> String {
        format!("ssh-managed-config: {}", self.managed_status.render())
    }
}

enum SshStatus {
    Missing,
    Ok,
    Legacy(&'static str),
}

impl SshStatus {
    fn render(&self) -> String {
        match self {
            Self::Missing => "missing".into(),
            Self::Ok => "ok".into(),
            Self::Legacy(note) => format!("ok ({note})"),
        }
    }
}

fn inspect_ssh(paths: &AppPaths) -> anyhow::Result<SshInspection> {
    let raw = match fs::read_to_string(&paths.ssh_config_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(SshInspection {
                include_status: SshStatus::Missing,
                managed_status: managed_ssh_status(paths),
            });
        }
        Err(error) => return Err(error.into()),
    };

    let include_status = raw
        .lines()
        .map(str::trim)
        .find_map(|line| match line {
            "Include ~/.ssh/config.d/medium.conf" => Some(SshStatus::Ok),
            "Include ~/.ssh/config.d/overlay.conf" => Some(SshStatus::Legacy("legacy overlay.conf")),
            _ => None,
        })
        .unwrap_or(SshStatus::Missing);

    Ok(SshInspection {
        include_status,
        managed_status: managed_ssh_status(paths),
    })
}

fn managed_ssh_status(paths: &AppPaths) -> SshStatus {
    if paths.overlay_ssh_config_path.exists() {
        return SshStatus::Ok;
    }

    let legacy_path = paths.ssh_config_dir.join("overlay.conf");
    if legacy_path.exists() {
        return SshStatus::Legacy("legacy overlay.conf");
    }

    SshStatus::Missing
}

fn service_line(service: &str) -> String {
    let systemctl = install::systemctl_bin();
    let enabled = service_status(&systemctl, &["is-enabled", service]);
    let active = service_status(&systemctl, &["is-active", service]);

    match (enabled, active) {
        (Some(enabled), Some(active)) => format!("service {service}: {enabled}, {active}"),
        _ => format!("service {service}: unavailable"),
    }
}

fn service_status(systemctl: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(systemctl).args(args).output().ok()?;
    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !status.is_empty() {
        return Some(status);
    }
    if !output.status.success() {
        return None;
    }
    Some("ok".into())
}
