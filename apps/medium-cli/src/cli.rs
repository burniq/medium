use crate::app;
use crate::client_api;
use crate::paths::AppPaths;
use crate::ssh::sync_ssh_config;
use crate::state::AppState;
use crate::state::invite;
#[path = "doctor.rs"]
mod doctor;
#[path = "install.rs"]
mod install;
use futures_util::{SinkExt, StreamExt};
use home_node::agent::prepare_agent_from_path;
use overlay_protocol::{CandidateKind, DeviceRecord, PeerCandidate, SessionOpenGrant};
use overlay_transport::session::{
    RelayHello, SessionHello, write_relay_hello, write_session_hello,
};
use sqlx::Row;
use std::path::PathBuf;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

const HELP: &str = r#"Medium CLI

Usage:
  medium <command> [options]

Bootstrap:
  medium init-control [--reconfigure]
      Initialize a control-plane host.
  medium init-node <node-invite> [--reconfigure]
      Initialize a node that publishes services.
  medium join <invite>
      Join this client to a Medium network.

Client:
  medium devices
      List devices and SSH endpoints visible to this joined client.

Control-plane diagnostics:
  medium control devices
      Read the control-plane registry and show registered nodes and services.
      Run this on a control-plane host, usually with sudo.
  medium control restart
      Restart the local systemd relay and control-plane services.
  medium doctor
      Inspect local config, state, binaries, and service status.

Node runtime:
  medium run [--config <path>]
      Run node-agent using ~/.medium/node.toml by default.

SSH and proxy:
  medium ssh sync [--write-main-config]
      Write managed SSH config entries for joined devices.
  medium proxy ssh --device <name>
      Open an SSH TCP proxy to a device through Medium session transport.

Maintenance:
  medium info
      Print product information.
  medium normalize-label <value>
      Normalize a node or device label.

Run `medium help <command>` is not supported yet.
"#;

enum Command {
    InitControl {
        reconfigure: bool,
    },
    InitNode {
        invite: String,
        reconfigure: bool,
    },
    Run {
        config_path: PathBuf,
    },
    Join {
        invite: String,
    },
    Pair {
        server_url: String,
        device_name: String,
    },
    Devices,
    ControlDevices,
    ControlRestart,
    Help,
    SshSync {
        write_main_config: bool,
    },
    ProxySsh {
        device_name: String,
    },
    Doctor,
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
            if !config_path.is_file() {
                return Err(format!(
                    "node config not found at {}",
                    config_path.display()
                ));
            }
            let agent = prepare_agent_from_path(config_path).map_err(|error| error.to_string())?;
            Ok(agent.startup_summary())
        }
        Command::Info => Ok(app::summary().to_string()),
        Command::Help => Ok(HELP.to_string()),
        Command::NormalizeLabel { value } => Ok(app::normalize_device_label(&value)),
        Command::InitControl { .. }
        | Command::InitNode { .. }
        | Command::Join { .. }
        | Command::Pair { .. }
        | Command::Devices
        | Command::ControlDevices
        | Command::ControlRestart
        | Command::SshSync { .. }
        | Command::ProxySsh { .. }
        | Command::Doctor => Err("command requires runtime context; use run_main".into()),
    }
}

pub async fn run_main<I>(args: I) -> Result<Option<String>, String>
where
    I: IntoIterator<Item = String>,
{
    match parse(args)? {
        Command::InitControl { reconfigure } => {
            let report = install::init_control(reconfigure).map_err(|error| error.to_string())?;
            Ok(Some(format!(
                "initialized Medium control at {} and generated invite {}\ngenerated node invite {}",
                report.control_config_path.display(),
                report.invite,
                report.node_invite
            )))
        }
        Command::InitNode {
            invite,
            reconfigure,
        } => {
            let report =
                install::init_node(&invite, reconfigure).map_err(|error| error.to_string())?;
            Ok(Some(format!(
                "initialized Medium node at {}",
                report.node_config_path.display()
            )))
        }
        Command::Run { config_path } => {
            if !config_path.is_file() {
                return Err(format!(
                    "node config not found at {}",
                    config_path.display()
                ));
            }
            let agent = prepare_agent_from_path(config_path).map_err(|error| error.to_string())?;
            agent
                .run_until_shutdown()
                .await
                .map_err(|error| format!("node-agent failed: {error:#}"))?;
            Ok(None)
        }
        Command::Join { invite } => {
            let paths = AppPaths::from_env().map_err(|error| error.to_string())?;
            let invite = invite::parse_invite(&invite).map_err(|error| error.to_string())?;
            let state = client_api::join(&invite)
                .await
                .map_err(|error| error.to_string())?;
            state.save(&paths).map_err(|error| error.to_string())?;
            Ok(Some(format!(
                "joined {} via {} using invite v{}",
                state.device_name, state.server_url, state.invite_version
            )))
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
            let state = AppState::load(&paths).map_err(|error| {
                format!(
                    "{}; `medium devices` is a client command, run `medium join <invite>` first or use `medium control devices` on a control-plane host",
                    error
                )
            })?;
            let devices = client_api::fetch_devices(&state)
                .await
                .map_err(|error| error.to_string())?;
            Ok(Some(render_devices(&devices.devices)))
        }
        Command::ControlDevices => {
            let report = render_control_devices()
                .await
                .map_err(|error| error.to_string())?;
            Ok(Some(report))
        }
        Command::ControlRestart => {
            let services =
                install::restart_control_services().map_err(|error| error.to_string())?;
            Ok(Some(format!("restarted {}", services.join(", "))))
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
            run_proxy(&state, &devices.devices, &device_name)
                .await
                .map_err(|error| error.to_string())?;
            Ok(None)
        }
        Command::Doctor => {
            let paths = AppPaths::from_env().map_err(|error| error.to_string())?;
            let report = doctor::inspect(&paths).map_err(|error| error.to_string())?;
            Ok(Some(report.render()))
        }
        Command::Info => Ok(Some(app::summary().to_string())),
        Command::Help => Ok(Some(HELP.to_string())),
        Command::NormalizeLabel { value } => Ok(Some(app::normalize_device_label(&value))),
    }
}

fn parse<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();

    match args.as_slice() {
        [_binary] => Ok(Command::Help),
        [_binary, command] if command == "help" || command == "--help" || command == "-h" => {
            Ok(Command::Help)
        }
        [_binary, command] if command == "init-control" => {
            Ok(Command::InitControl { reconfigure: false })
        }
        [_binary, command, flag] if command == "init-control" && flag == "--reconfigure" => {
            Ok(Command::InitControl { reconfigure: true })
        }
        [_binary, command, invite] if command == "init-node" => Ok(Command::InitNode {
            invite: invite.clone(),
            reconfigure: false,
        }),
        [_binary, command, invite, flag] if command == "init-node" && flag == "--reconfigure" => {
            Ok(Command::InitNode {
                invite: invite.clone(),
                reconfigure: true,
            })
        }
        [_binary, command, invite] if command == "join" => Ok(Command::Join {
            invite: invite.clone(),
        }),
        [_binary, command, flag, server_url, device_flag, device_name]
            if command == "pair" && flag == "--server" && device_flag == "--device" =>
        {
            Ok(Command::Pair {
                server_url: server_url.clone(),
                device_name: device_name.clone(),
            })
        }
        [_binary, command] if command == "devices" => Ok(Command::Devices),
        [_binary, first, second] if first == "control" && second == "devices" => {
            Ok(Command::ControlDevices)
        }
        [_binary, first, second] if first == "control" && second == "restart" => {
            Ok(Command::ControlRestart)
        }
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
        [_binary, command] if command == "doctor" => Ok(Command::Doctor),
        [_binary, command, flag, path] if command == "run" && flag == "--config" => {
            Ok(Command::Run {
                config_path: PathBuf::from(path),
            })
        }
        [_binary, command] if command == "run" => Ok(Command::Run {
            config_path: install::default_node_config_path(&install::install_root()),
        }),
        [_binary, command] if command == "info" => Ok(Command::Info),
        [_binary, command, value] if command == "normalize-label" => Ok(Command::NormalizeLabel {
            value: value.clone(),
        }),
        [_binary, command, ..] => Err(format!("unknown command: {command}\n\nRun: medium help")),
        [] => Err("missing argv[0]\n\nRun: medium help".to_string()),
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

async fn render_control_devices() -> anyhow::Result<String> {
    let root = install::install_root();
    let control_config_path = install::control_config_path(&root);
    let raw = std::fs::read_to_string(&control_config_path).map_err(|error| {
        anyhow::anyhow!(
            "control config not found at {}: {}",
            control_config_path.display(),
            error
        )
    })?;
    let database_url = parse_simple_toml_string(&raw, "database_url")
        .ok_or_else(|| anyhow::anyhow!("control config is missing database_url"))?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    let rows = match sqlx::query(
        r#"
        select
          n.id as node_id,
          n.label as node_label,
          n.last_seen_at as last_seen_at,
          ns.id as service_id,
          ns.kind as service_kind,
          ns.target as service_target,
          ns.label as service_label
        from nodes n
        left join node_services ns on ns.node_id = n.id
        order by n.id, ns.id
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) if error.to_string().contains("no such table") => {
            return Ok(
                "control registry is not initialized; start medium-control-plane first".to_string(),
            );
        }
        Err(error) => return Err(error.into()),
    };

    if rows.is_empty() {
        return Ok("no registered nodes".into());
    }

    let mut output = Vec::new();
    let mut current_node = String::new();
    let mut service_count = 0usize;
    for row in rows {
        let node_id: String = row.try_get("node_id")?;
        if node_id != current_node {
            if !current_node.is_empty() && service_count == 0 {
                output.push("  no published services".to_string());
            }
            current_node = node_id.clone();
            service_count = 0;
            let node_label: String = row.try_get("node_label")?;
            let last_seen_at: String = row.try_get("last_seen_at")?;
            output.push(format!("{node_label} ({node_id}) last_seen={last_seen_at}"));
        }

        let service_id: Option<String> = row.try_get("service_id")?;
        if let Some(service_id) = service_id {
            service_count += 1;
            let kind: String = row.try_get("service_kind")?;
            let target: String = row.try_get("service_target")?;
            let label: Option<String> = row.try_get("service_label")?;
            match label.filter(|label| label != &service_id) {
                Some(label) => {
                    output.push(format!("  {service_id} {kind} \"{label}\" -> {target}"))
                }
                None => output.push(format!("  {service_id} {kind} -> {target}")),
            }
        }
    }
    if service_count == 0 {
        output.push("  no published services".to_string());
    }

    Ok(output.join("\n"))
}

fn parse_simple_toml_string(raw: &str, wanted_key: &str) -> Option<String> {
    raw.lines().find_map(|line| {
        let line = line
            .split_once('#')
            .map_or(line, |(before, _)| before)
            .trim();
        let (key, value) = line.split_once('=')?;
        if key.trim() != wanted_key {
            return None;
        }
        let value = value.trim();
        if !value.starts_with('"') || !value.ends_with('"') || value.len() < 2 {
            return None;
        }
        Some(value[1..value.len() - 1].to_string())
    })
}

async fn run_proxy(
    state: &AppState,
    devices: &[DeviceRecord],
    device_name: &str,
) -> anyhow::Result<()> {
    let device = devices
        .iter()
        .find(|device| device.name == device_name)
        .ok_or_else(|| anyhow::anyhow!("unknown device {}", device_name))?;
    let ssh = device
        .ssh
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("device {} has no SSH endpoint", device_name))?;
    let grant = client_api::open_session(state, &ssh.service_id).await?;
    proxy_via_grant(&grant).await
}

async fn proxy_via_grant(grant: &SessionOpenGrant) -> anyhow::Result<()> {
    let mut last_error = None;

    for candidate in ordered_candidates(grant) {
        match connect_candidate(grant, &candidate).await {
            Ok(CandidateConnection::Tcp(stream)) => return pipe_stdio(stream).await,
            Ok(CandidateConnection::WssRelay(socket)) => return pipe_websocket_stdio(socket).await,
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("session grant has no candidates")))
}

pub fn ordered_candidates(grant: &SessionOpenGrant) -> Vec<PeerCandidate> {
    let mut candidates = grant.authorization.candidates.clone();
    candidates.sort_by(|left, right| right.priority.cmp(&left.priority));
    candidates
}

#[doc(hidden)]
pub fn candidate_order_for_test(grant: &SessionOpenGrant) -> Vec<CandidateKind> {
    ordered_candidates(grant)
        .into_iter()
        .map(|candidate| candidate.kind)
        .collect()
}

enum CandidateConnection {
    Tcp(TcpStream),
    WssRelay(WebSocketStream<MaybeTlsStream<TcpStream>>),
}

async fn connect_candidate(
    grant: &SessionOpenGrant,
    candidate: &PeerCandidate,
) -> anyhow::Result<CandidateConnection> {
    match candidate.kind {
        CandidateKind::DirectTcp => Ok(CandidateConnection::Tcp(
            connect_tcp_candidate(grant, &candidate.addr).await?,
        )),
        CandidateKind::RelayTcp => {
            let mut stream = TcpStream::connect(&candidate.addr).await?;
            write_relay_hello(
                &mut stream,
                &RelayHello::Client {
                    node_id: grant.node_id.clone(),
                },
            )
            .await?;
            write_session_hello(
                &mut stream,
                &SessionHello {
                    token: grant.authorization.token.clone(),
                    service_id: grant.service_id.clone(),
                    transport: None,
                },
            )
            .await?;
            Ok(CandidateConnection::Tcp(stream))
        }
        CandidateKind::WssRelay => connect_wss_relay_candidate(grant, candidate).await,
    }
}

async fn connect_wss_relay_candidate(
    grant: &SessionOpenGrant,
    candidate: &PeerCandidate,
) -> anyhow::Result<CandidateConnection> {
    let (mut socket, _) = connect_async(&candidate.addr).await?;
    socket
        .send(Message::Text(
            serde_json::to_string(&RelayHello::Client {
                node_id: grant.node_id.clone(),
            })?
            .into(),
        ))
        .await?;
    socket
        .send(Message::Binary(session_hello_frame(grant)?.into()))
        .await?;

    Ok(CandidateConnection::WssRelay(socket))
}

fn session_hello_frame(grant: &SessionOpenGrant) -> anyhow::Result<Vec<u8>> {
    let mut payload = serde_json::to_vec(&SessionHello {
        token: grant.authorization.token.clone(),
        service_id: grant.service_id.clone(),
        transport: None,
    })?;
    payload.push(b'\n');
    Ok(payload)
}

async fn connect_tcp_candidate(grant: &SessionOpenGrant, addr: &str) -> anyhow::Result<TcpStream> {
    let mut stream = TcpStream::connect(addr).await?;
    write_session_hello(
        &mut stream,
        &SessionHello {
            token: grant.authorization.token.clone(),
            service_id: grant.service_id.clone(),
            transport: None,
        },
    )
    .await?;

    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use overlay_protocol::PeerCandidate;
    use tokio::net::TcpListener;
    use tokio::time::{Duration, timeout};
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    #[tokio::test]
    async fn connect_candidate_sends_wss_relay_hello_then_session_hello_frame() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (observed_tx, observed_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let relay_hello = socket.next().await.unwrap().unwrap();
            let session_hello = socket.next().await.unwrap().unwrap();
            observed_tx.send((relay_hello, session_hello)).unwrap();
        });

        let grant: SessionOpenGrant = serde_json::from_str(
            r#"{"session_id":"session-wss","service_id":"svc_web","node_id":"node-1","relay_hint":"wss://relay.example.com/medium/v1/relay","authorization":{"token":"token-wss","expires_at":"2099-01-01T00:00:00Z","candidates":[]}}"#,
        )
        .unwrap();
        let candidate = PeerCandidate {
            kind: CandidateKind::WssRelay,
            addr: format!("ws://{addr}/medium/v1/relay"),
            priority: 10,
        };

        let connection = connect_candidate(&grant, &candidate).await;

        if let Err(error) = connection {
            panic!("{error}");
        }
        let (relay_hello, session_hello) = timeout(Duration::from_secs(1), observed_rx)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            relay_hello,
            Message::Text(r#"{"role":"client","node_id":"node-1"}"#.into())
        );
        assert_eq!(
            session_hello,
            Message::Binary(
                br#"{"token":"token-wss","service_id":"svc_web"}
"#
                .to_vec()
                .into()
            )
        );
    }
}

async fn pipe_stdio(stream: TcpStream) -> anyhow::Result<()> {
    let (mut read_half, mut write_half) = stream.into_split();
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    let stdin_to_net = tokio::spawn(async move {
        io::copy(&mut stdin, &mut write_half).await?;
        write_half.shutdown().await?;
        anyhow::Ok(())
    });
    let net_to_stdout = tokio::spawn(async move {
        io::copy(&mut read_half, &mut stdout).await?;
        stdout.flush().await?;
        anyhow::Ok(())
    });

    stdin_to_net.await??;
    net_to_stdout.await??;
    Ok(())
}

async fn pipe_websocket_stdio(
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> anyhow::Result<()> {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    let stdin_to_ws = async {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = stdin.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            ws_tx
                .send(Message::Binary(buffer[..read].to_vec().into()))
                .await?;
        }
        anyhow::Ok(())
    };

    let ws_to_stdout = async {
        while let Some(message) = ws_rx.next().await {
            match message? {
                Message::Binary(payload) => stdout.write_all(&payload).await?,
                Message::Close(_) => break,
                _ => {}
            }
        }
        stdout.flush().await?;
        anyhow::Ok(())
    };

    let (stdin_result, stdout_result) = tokio::join!(stdin_to_ws, ws_to_stdout);
    stdin_result?;
    stdout_result?;

    Ok(())
}
