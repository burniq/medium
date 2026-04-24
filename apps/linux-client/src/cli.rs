use crate::app;
use home_node::agent::prepare_agent_from_path;
use std::path::PathBuf;

const USAGE: &str = "usage: linux-client [run --config <path> | info | normalize-label <value>]";

enum Command {
    Run { config_path: PathBuf },
    Info,
    NormalizeLabel { value: String },
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
