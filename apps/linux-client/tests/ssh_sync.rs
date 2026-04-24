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
fn sync_writes_include_and_managed_overlay_config() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let paths = AppPaths::from_home(home.path());

    let report = sync_ssh_config(&paths, &[sample_device()], true)?;
    assert!(report.main_config_updated);
    assert_eq!(report.hosts_written, 1);

    let main_config = fs::read_to_string(&paths.ssh_config_path)?;
    assert!(main_config.contains("Include ~/.ssh/config.d/overlay.conf"));

    let managed = fs::read_to_string(&paths.overlay_ssh_config_path)?;
    assert!(managed.contains("Host node-home"));
    assert!(managed.contains("ProxyCommand overlay proxy ssh --device node-home"));
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
