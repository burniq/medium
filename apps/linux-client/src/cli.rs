use crate::app;
use crate::client_api;
use crate::paths::AppPaths;
use crate::ssh::sync_ssh_config;
use crate::state::AppState;
use home_node::agent::prepare_agent_from_path;
use overlay_protocol::DeviceRecord;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

const USAGE: &str = "usage: overlay [pair --server <url> --device <name> | devices | ssh sync [--write-main-config] | proxy ssh --device <name> | run --config <path> | info | normalize-label <value>]";

enum Command {
    Run {
        config_path: PathBuf,
    },
    Pair {
        server_url: String,
        device_name: String,
    },
    Devices,
    SshSync {
        write_main_config: bool,
    },
    ProxySsh {
        device_name: String,
    },
    Info,
    NormalizeLabel {
        value: String,
    },
}

pub fn run<I>(args: I) -> Result<String, String>
where
    I: IntoIterator<Item = String>,
{
    match parse(args)? {
        Command::Run { config_path } => {
            let agent = prepare_agent_from_path(config_path).map_err(|error| error.to_string())?;
            Ok(agent.startup_summary())
        }
        Command::Info => Ok(app::summary().to_string()),
        Command::NormalizeLabel { value } => Ok(app::normalize_device_label(&value)),
        Command::Pair { .. }
        | Command::Devices
        | Command::SshSync { .. }
        | Command::ProxySsh { .. } => Err("command requires runtime context; use run_main".into()),
    }
}

pub async fn run_main<I>(args: I) -> Result<Option<String>, String>
where
    I: IntoIterator<Item = String>,
{
    match parse(args)? {
        Command::Run { config_path } => {
            let agent = prepare_agent_from_path(config_path).map_err(|error| error.to_string())?;
            agent
                .run_until_shutdown()
                .await
                .map_err(|error| error.to_string())?;
            Ok(None)
        }
        Command::Pair {
            server_url,
            device_name,
        } => {
            let paths = AppPaths::from_env().map_err(|error| error.to_string())?;
            let state = client_api::pair(&server_url, &device_name)
                .await
                .map_err(|error| error.to_string())?;
            state.save(&paths).map_err(|error| error.to_string())?;
            Ok(Some(format!(
                "paired {} with {} using bootstrap code {}",
                state.device_name, state.server_url, state.bootstrap_code
            )))
        }
        Command::Devices => {
            let paths = AppPaths::from_env().map_err(|error| error.to_string())?;
            let state = AppState::load(&paths).map_err(|error| error.to_string())?;
            let devices = client_api::fetch_devices(&state)
                .await
                .map_err(|error| error.to_string())?;
            Ok(Some(render_devices(&devices.devices)))
        }
        Command::SshSync { write_main_config } => {
            let paths = AppPaths::from_env().map_err(|error| error.to_string())?;
            let state = AppState::load(&paths).map_err(|error| error.to_string())?;
            let devices = client_api::fetch_devices(&state)
                .await
                .map_err(|error| error.to_string())?;
            let report = sync_ssh_config(&paths, &devices.devices, write_main_config)
                .map_err(|error| error.to_string())?;
            Ok(Some(format!(
                "synced {} SSH hosts into {}{}",
                report.hosts_written,
                paths.overlay_ssh_config_path.display(),
                if report.main_config_updated {
                    " and updated ~/.ssh/config"
                } else {
                    ""
                }
            )))
        }
        Command::ProxySsh { device_name } => {
            let paths = AppPaths::from_env().map_err(|error| error.to_string())?;
            let state = AppState::load(&paths).map_err(|error| error.to_string())?;
            let devices = client_api::fetch_devices(&state)
                .await
                .map_err(|error| error.to_string())?;
            run_proxy(&devices.devices, &device_name).map_err(|error| error.to_string())?;
            Ok(None)
        }
        Command::Info => Ok(Some(app::summary().to_string())),
        Command::NormalizeLabel { value } => Ok(Some(app::normalize_device_label(&value))),
    }
}

fn parse<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();

    match args.as_slice() {
        [_binary, command, flag, server_url, device_flag, device_name]
            if command == "pair" && flag == "--server" && device_flag == "--device" =>
        {
            Ok(Command::Pair {
                server_url: server_url.clone(),
                device_name: device_name.clone(),
            })
        }
        [_binary, command] if command == "devices" => Ok(Command::Devices),
        [_binary, first, second] if first == "ssh" && second == "sync" => Ok(Command::SshSync {
            write_main_config: false,
        }),
        [_binary, first, second, flag]
            if first == "ssh" && second == "sync" && flag == "--write-main-config" =>
        {
            Ok(Command::SshSync {
                write_main_config: true,
            })
        }
        [_binary, first, second, flag, device_name]
            if first == "proxy" && second == "ssh" && flag == "--device" =>
        {
            Ok(Command::ProxySsh {
                device_name: device_name.clone(),
            })
        }
        [_binary, command, flag, path] if command == "run" && flag == "--config" => {
            Ok(Command::Run {
                config_path: PathBuf::from(path),
            })
        }
        [_binary, command] if command == "info" => Ok(Command::Info),
        [_binary, command, value] if command == "normalize-label" => Ok(Command::NormalizeLabel {
            value: value.clone(),
        }),
        _ => Err(USAGE.to_string()),
    }
}

fn render_devices(devices: &[DeviceRecord]) -> String {
    devices
        .iter()
        .map(|device| match &device.ssh {
            Some(ssh) => format!("{} ssh {}@{}:{}", device.name, ssh.user, ssh.host, ssh.port),
            None => format!("{} no-ssh", device.name),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_proxy(devices: &[DeviceRecord], device_name: &str) -> anyhow::Result<()> {
    let device = devices
        .iter()
        .find(|device| device.name == device_name)
        .ok_or_else(|| anyhow::anyhow!("unknown device {}", device_name))?;
    let ssh = device
        .ssh
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("device {} has no SSH endpoint", device_name))?;

    let status = ProcessCommand::new("nc")
        .arg(&ssh.host)
        .arg(ssh.port.to_string())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("nc exited with status {}", status)
    }
}
