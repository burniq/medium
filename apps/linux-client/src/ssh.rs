use crate::paths::AppPaths;
use overlay_protocol::DeviceRecord;
use std::path::{Path, PathBuf};

const MAIN_INCLUDE_LINE: &str = "Include ~/.ssh/config.d/overlay.conf";

#[derive(Debug)]
pub struct SyncReport {
    pub main_config_updated: bool,
    pub managed_backup_path: Option<PathBuf>,
    pub hosts_written: usize,
}

pub fn sync_ssh_config(
    paths: &AppPaths,
    devices: &[DeviceRecord],
    write_main_config: bool,
) -> anyhow::Result<SyncReport> {
    std::fs::create_dir_all(&paths.ssh_dir)?;
    std::fs::create_dir_all(&paths.ssh_config_dir)?;

    let main_config_updated = ensure_main_include(paths, write_main_config)?;
    let managed_backup_path = if paths.overlay_ssh_config_path.exists() {
        Some(backup_file(&paths.overlay_ssh_config_path)?)
    } else {
        None
    };

    let managed = render_managed_config(devices);
    atomic_write(&paths.overlay_ssh_config_path, managed.as_bytes())?;

    Ok(SyncReport {
        main_config_updated,
        managed_backup_path,
        hosts_written: devices.iter().filter(|device| device.ssh.is_some()).count(),
    })
}

fn ensure_main_include(paths: &AppPaths, write_main_config: bool) -> anyhow::Result<bool> {
    let current = if paths.ssh_config_path.exists() {
        std::fs::read_to_string(&paths.ssh_config_path)?
    } else {
        String::new()
    };

    if current.lines().any(|line| line.trim() == MAIN_INCLUDE_LINE) {
        return Ok(false);
    }

    if !write_main_config {
        anyhow::bail!(
            "main SSH config is missing overlay include; re-run with --write-main-config"
        );
    }

    if paths.ssh_config_path.exists() {
        let _ = backup_file(&paths.ssh_config_path)?;
    }

    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(MAIN_INCLUDE_LINE);
    next.push('\n');

    atomic_write(&paths.ssh_config_path, next.as_bytes())?;
    Ok(true)
}

fn render_managed_config(devices: &[DeviceRecord]) -> String {
    let mut out = String::from("# Managed by overlay. DO NOT EDIT.\n\n");

    for device in devices.iter().filter(|device| device.ssh.is_some()) {
        let ssh = device.ssh.as_ref().expect("filtered above");
        out.push_str(&format!(
            "# endpoint {host}:{port}\nHost {name}\n  HostName {name}\n  User {user}\n  ProxyCommand overlay proxy ssh --device {name}\n\n",
            host = ssh.host,
            port = ssh.port,
            name = device.name,
            user = ssh.user,
        ));
    }

    out
}

fn atomic_write(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension(format!("tmp-{}", timestamp_suffix()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn backup_file(path: &Path) -> anyhow::Result<PathBuf> {
    let backup = path.with_extension(format!("bak-{}", timestamp_suffix()));
    std::fs::copy(path, &backup)?;
    Ok(backup)
}

fn timestamp_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock drift")
        .as_nanos()
}
