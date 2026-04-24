use super::install;
use crate::paths::AppPaths;
use crate::state::AppState;
use std::fs;
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
    let ssh_include = ssh_include_present(&paths.ssh_config_path)?;

    let mut lines = Vec::new();
    lines.push(path_line("config-dir", &paths.app_config_dir));
    lines.push(path_line("state-dir", &paths.state_dir));
    lines.push(state_line(paths));
    lines.push(path_line("control-config", &control_config_path));
    lines.push(config_line(
        "control-config-valid",
        &control_config_path,
        &["bind_addr = ", "database_url = ", "control_url = ", "shared_secret = "],
    )?);
    lines.push(path_line("node-config", &node_config_path));
    lines.push(config_line(
        "node-config-valid",
        &node_config_path,
        &["node_id = ", "bind_addr = "],
    )?);
    lines.push(path_line("control-db", &database_path));
    lines.push(path_line(
        "control-plane-bin",
        &install::control_plane_binary_path(&root),
    ));
    lines.push(path_line(
        "home-node-bin",
        &install::home_node_binary_path(&root),
    ));
    lines.push(format!(
        "ssh-include: {}",
        if ssh_include { "ok" } else { "missing" }
    ));
    lines.push(path_line("ssh-managed-config", &paths.overlay_ssh_config_path));
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

fn config_line(label: &str, path: &Path, required_fields: &[&str]) -> anyhow::Result<String> {
    if !path.is_file() {
        return Ok(format!("{label}: missing"));
    }

    let raw = fs::read_to_string(path)?;
    let missing_fields = required_fields
        .iter()
        .copied()
        .filter(|field| !raw.contains(field))
        .collect::<Vec<_>>();

    if missing_fields.is_empty() {
        return Ok(format!("{label}: ok"));
    }

    Ok(format!(
        "{label}: invalid (missing {})",
        missing_fields.join(", ")
    ))
}

fn state_line(paths: &AppPaths) -> String {
    match AppState::load(paths) {
        Ok(state) => format!(
            "join-state: ok (device {} via {})",
            state.device_name, state.server_url
        ),
        Err(error) if is_missing_state(paths, &error) => {
            format!("join-state: missing ({})", paths.state_path.display())
        }
        Err(error) => format!("join-state: invalid ({error})"),
    }
}

fn is_missing_state(paths: &AppPaths, error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
        || error.to_string().contains(&paths.state_path.display().to_string())
}

fn ssh_include_present(path: &Path) -> anyhow::Result<bool> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };

    Ok(raw
        .lines()
        .map(str::trim)
        .any(|line| line == "Include ~/.ssh/config.d/medium.conf"))
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

    if output.status.success() {
        return Some("ok".into());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() { None } else { Some(stderr) }
}
