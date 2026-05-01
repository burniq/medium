use home_node::agent::prepare_agent;
use home_node::config::NodeConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

#[test]
fn startup_summary_lists_published_services() {
    let cfg: NodeConfig = toml::from_str(
        r#"
node_id = "node-1"

[[services]]
id = "svc_openclaw"
kind = "https"
target = "127.0.0.1:3000"

[[services]]
id = "svc_ssh"
kind = "ssh"
target = "127.0.0.1:22"
"#,
    )
    .unwrap();

    let agent = prepare_agent(cfg);
    let summary = agent.startup_summary();

    assert!(summary.contains("agent ready for node-1"));
    assert!(summary.contains("2 services"));
    assert!(summary.contains("svc_openclaw:https@127.0.0.1:3000"));
    assert!(summary.contains("svc_ssh:ssh@127.0.0.1:22"));
}

#[tokio::test]
async fn run_registers_node_with_configured_control_plane() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let control_url = format!("http://{}", listener.local_addr()?);
    let (registered_tx, mut registered_rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                let headers_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let headers = String::from_utf8_lossy(&request[..headers_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length: ")
                            .or_else(|| line.strip_prefix("Content-Length: "))
                    })
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if request.len() >= headers_end + content_length {
                    break;
                }
            }
        }
        registered_tx.send(request).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    });

    let cfg: NodeConfig = toml::from_str(&format!(
        r#"
node_id = "node-1"
bind_addr = "127.0.0.1:0"
control_url = "{control_url}"
shared_secret = "medium-shared-secret-test"

[[services]]
id = "svc_web"
kind = "https"
target = "127.0.0.1:3000"
"#
    ))?;
    let agent = prepare_agent(cfg);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let run = tokio::spawn(async move { agent.run_with_shutdown(shutdown_rx).await });

    let request = tokio::time::timeout(std::time::Duration::from_secs(2), registered_rx.recv())
        .await?
        .expect("node should register");
    let request = String::from_utf8(request)?;
    assert!(request.starts_with("POST /api/nodes/register "));
    assert!(request.contains(r#""node_id":"node-1""#));
    assert!(request.contains(r#""id":"svc_web""#));

    let _ = shutdown_tx.send(());
    run.await??;
    Ok(())
}
