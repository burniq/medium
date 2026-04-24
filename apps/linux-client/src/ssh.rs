use crate::paths::AppPaths;
use overlay_protocol::DeviceRecord;
use std::path::{Path, PathBuf};

const MAIN_INCLUDE_LINE: &str = "Include ~/.ssh/config.d/medium.conf";
const LEGACY_MAIN_INCLUDE_LINE: &str = "Include ~/.ssh/config.d/overlay.conf";
const LEGACY_MANAGED_HEADER: &str = "# Managed by overlay.";
const LEGACY_PROXY_COMMAND: &str = "ProxyCommand overlay proxy ssh --device ";

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

    let legacy_overlay_is_managed = legacy_overlay_config_is_managed(paths)?;
    let managed_backup_path = backup_managed_state(paths, legacy_overlay_is_managed)?;
    let main_config_updated =
        ensure_main_include(paths, write_main_config, legacy_overlay_is_managed)?;

    let managed = render_managed_config(devices);
    atomic_write(&paths.overlay_ssh_config_path, managed.as_bytes())?;
    neutralize_legacy_overlay_config(paths)?;

    Ok(SyncReport {
        main_config_updated,
        managed_backup_path,
        hosts_written: devices.iter().filter(|device| device.ssh.is_some()).count(),
    })
}

fn ensure_main_include(
    paths: &AppPaths,
    write_main_config: bool,
    legacy_overlay_is_managed: bool,
) -> anyhow::Result<bool> {
    let current = if paths.ssh_config_path.exists() {
        std::fs::read_to_string(&paths.ssh_config_path)?
    } else {
        String::new()
    };

    let mut kept_lines = Vec::new();
    let mut has_medium_include = false;
    let mut needs_rewrite = false;

    for line in current.lines() {
        match line.trim() {
            LEGACY_MAIN_INCLUDE_LINE if legacy_overlay_is_managed => {
                needs_rewrite = true;
            }
            MAIN_INCLUDE_LINE if has_medium_include => {
                needs_rewrite = true;
            }
            MAIN_INCLUDE_LINE => {
                has_medium_include = true;
                kept_lines.push(line);
            }
            _ => kept_lines.push(line),
        }
    }

    if !has_medium_include && !write_main_config {
        anyhow::bail!(
            "main SSH config is missing medium include; re-run with --write-main-config"
        );
    }

    if !has_medium_include {
        kept_lines.push(MAIN_INCLUDE_LINE);
        needs_rewrite = true;
    }

    if !needs_rewrite {
        return Ok(false);
    }

    if paths.ssh_config_path.exists() {
        let _ = backup_file(&paths.ssh_config_path)?;
    }

    let mut next = kept_lines.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }

    atomic_write(&paths.ssh_config_path, next.as_bytes())?;
    Ok(true)
}

fn render_managed_config(devices: &[DeviceRecord]) -> String {
    let mut out = String::from("# Managed by medium. DO NOT EDIT.\n\n");

    for device in devices.iter().filter(|device| device.ssh.is_some()) {
        let ssh = device.ssh.as_ref().expect("filtered above");
        out.push_str(&format!(
            "# endpoint {host}:{port}\nHost {name}\n  HostName {name}\n  User {user}\n  ProxyCommand medium proxy ssh --device {name}\n\n",
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

fn backup_managed_state(
    paths: &AppPaths,
    legacy_overlay_is_managed: bool,
) -> anyhow::Result<Option<PathBuf>> {
    let mut managed_backup_path = None;

    if paths.overlay_ssh_config_path.exists() {
        managed_backup_path = Some(backup_file(&paths.overlay_ssh_config_path)?);
    }

    let legacy_overlay_path = legacy_overlay_ssh_config_path(paths);
    if legacy_overlay_is_managed && legacy_overlay_path.exists() {
        let backup = backup_file(&legacy_overlay_path)?;
        if managed_backup_path.is_none() {
            managed_backup_path = Some(backup);
        }
    }

    Ok(managed_backup_path)
}

fn neutralize_legacy_overlay_config(paths: &AppPaths) -> anyhow::Result<()> {
    let legacy_overlay_path = legacy_overlay_ssh_config_path(paths);
    if !legacy_overlay_path.exists() {
        return Ok(());
    }

    if !legacy_overlay_config_is_managed(paths)? {
        return Ok(());
    }

    let stub = "# Legacy overlay SSH config disabled by medium.\n# Managed entries moved to medium.conf.\n";
    atomic_write(&legacy_overlay_path, stub.as_bytes())?;
    Ok(())
}

fn legacy_overlay_ssh_config_path(paths: &AppPaths) -> PathBuf {
    paths.ssh_config_dir.join("overlay.conf")
}

fn is_legacy_overlay_managed_config(contents: &str) -> bool {
    contents.contains(LEGACY_MANAGED_HEADER) || contents.contains(LEGACY_PROXY_COMMAND)
}

fn legacy_overlay_config_is_managed(paths: &AppPaths) -> anyhow::Result<bool> {
    let legacy_overlay_path = legacy_overlay_ssh_config_path(paths);
    if !legacy_overlay_path.exists() {
        return Ok(false);
    }

    let current = std::fs::read_to_string(&legacy_overlay_path)?;
    Ok(is_legacy_overlay_managed_config(&current))
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
