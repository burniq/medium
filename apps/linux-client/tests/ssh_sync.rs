use linux_client::paths::AppPaths;
use linux_client::ssh::sync_ssh_config;
use overlay_protocol::{DeviceRecord, SshEndpoint};
use std::fs;

fn sample_device() -> DeviceRecord {
    DeviceRecord {
        id: "node-home".into(),
        name: "node-home".into(),
        ssh: Some(SshEndpoint {
            service_id: "svc_home_ssh".into(),
            host: "127.0.0.1".into(),
            port: 2222,
            user: "overlay".into(),
        }),
    }
}

#[test]
fn sync_requires_confirmation_before_touching_main_config() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());

    let error = sync_ssh_config(&paths, &[sample_device()], false).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("re-run with --write-main-config")
    );
}

#[test]
fn ssh_sync_uses_medium_managed_file_name() {
    let paths = AppPaths::from_home("/tmp/example-home");
    assert!(paths.overlay_ssh_config_path.ends_with("medium.conf"));
}

#[test]
fn sync_migrates_legacy_overlay_managed_state() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());
    let legacy_overlay_path = paths.ssh_config_dir.join("overlay.conf");

    fs::create_dir_all(&paths.ssh_config_dir)?;
    fs::write(
        &paths.ssh_config_path,
        "Host *\n  ServerAliveInterval 60\nInclude ~/.ssh/config.d/overlay.conf\n",
    )?;
    fs::write(
        &legacy_overlay_path,
        "# Managed by overlay. DO NOT EDIT.\n\nHost node-home\n  ProxyCommand overlay proxy ssh --device node-home\n",
    )?;

    sync_ssh_config(&paths, &[sample_device()], true)?;

    let main_config = fs::read_to_string(&paths.ssh_config_path)?;
    assert!(main_config.contains("Include ~/.ssh/config.d/medium.conf"));
    assert!(!main_config.contains("Include ~/.ssh/config.d/overlay.conf"));

    let managed = fs::read_to_string(&paths.overlay_ssh_config_path)?;
    assert!(managed.contains("ProxyCommand medium proxy ssh --device node-home"));

    if legacy_overlay_path.exists() {
        let legacy = fs::read_to_string(&legacy_overlay_path)?;
        assert!(!legacy.contains("ProxyCommand overlay proxy ssh --device node-home"));
    }

    Ok(())
}

#[test]
fn sync_removes_stale_overlay_include_once_medium_include_exists() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());
    let legacy_overlay_path = paths.ssh_config_dir.join("overlay.conf");

    fs::create_dir_all(&paths.ssh_config_dir)?;
    fs::write(
        &paths.ssh_config_path,
        "Include ~/.ssh/config.d/overlay.conf\nInclude ~/.ssh/config.d/medium.conf\n",
    )?;
    fs::write(
        &legacy_overlay_path,
        "# Managed by overlay. DO NOT EDIT.\n\nHost node-home\n  ProxyCommand overlay proxy ssh --device node-home\n",
    )?;
    fs::write(
        &paths.overlay_ssh_config_path,
        "# Managed by medium. DO NOT EDIT.\n\nHost node-home\n  ProxyCommand medium proxy ssh --device node-home\n",
    )?;

    sync_ssh_config(&paths, &[sample_device()], false)?;

    let main_config = fs::read_to_string(&paths.ssh_config_path)?;
    assert!(main_config.contains("Include ~/.ssh/config.d/medium.conf"));
    assert!(!main_config.contains("Include ~/.ssh/config.d/overlay.conf"));

    if legacy_overlay_path.exists() {
        let legacy = fs::read_to_string(&legacy_overlay_path)?;
        assert!(!legacy.contains("ProxyCommand overlay proxy ssh --device node-home"));
    }

    Ok(())
}

#[test]
fn sync_preserves_user_owned_overlay_include() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());
    let legacy_overlay_path = paths.ssh_config_dir.join("overlay.conf");

    fs::create_dir_all(&paths.ssh_config_dir)?;
    fs::write(
        &paths.ssh_config_path,
        "Include ~/.ssh/config.d/overlay.conf\n",
    )?;
    fs::write(
        &legacy_overlay_path,
        "Host corp-bastion\n  HostName bastion.example.com\n  User alice\n",
    )?;

    sync_ssh_config(&paths, &[sample_device()], true)?;

    let main_config = fs::read_to_string(&paths.ssh_config_path)?;
    assert!(main_config.contains("Include ~/.ssh/config.d/overlay.conf"));
    assert!(main_config.contains("Include ~/.ssh/config.d/medium.conf"));

    let overlay_config = fs::read_to_string(&legacy_overlay_path)?;
    assert!(overlay_config.contains("Host corp-bastion"));
    assert!(!overlay_config.contains("Legacy overlay SSH config disabled by medium."));

    Ok(())
}

#[test]
fn sync_writes_include_and_managed_medium_config() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());

    let report = sync_ssh_config(&paths, &[sample_device()], true)?;
    assert!(report.main_config_updated);
    assert_eq!(report.hosts_written, 1);

    let main_config = fs::read_to_string(&paths.ssh_config_path)?;
    assert!(main_config.contains("Include ~/.ssh/config.d/medium.conf"));

    let managed = fs::read_to_string(&paths.overlay_ssh_config_path)?;
    assert!(managed.contains("Host node-home"));
    assert!(managed.contains("ProxyCommand medium proxy ssh --device node-home"));
    assert!(managed.contains("User overlay"));
    Ok(())
}

#[test]
fn sync_creates_backup_before_rewriting_managed_config() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());

    sync_ssh_config(&paths, &[sample_device()], true)?;
    let first = fs::read_to_string(&paths.overlay_ssh_config_path)?;
    assert!(first.contains("Host node-home"));

    let other = DeviceRecord {
        id: "node-lab".into(),
        name: "node-lab".into(),
        ssh: Some(SshEndpoint {
            service_id: "svc_lab_ssh".into(),
            host: "127.0.0.1".into(),
            port: 2201,
            user: "nikita".into(),
        }),
    };

    let report = sync_ssh_config(&paths, &[other], false)?;
    assert!(report.managed_backup_path.is_some());

    let managed = fs::read_to_string(&paths.overlay_ssh_config_path)?;
    assert!(managed.contains("Host node-lab"));
    assert!(!managed.contains("Host node-home"));

    let backup = fs::read_to_string(report.managed_backup_path.unwrap())?;
    assert!(backup.contains("Host node-home"));
    Ok(())
}
