use home_node::config::NodeConfig;
use home_node::proxy::run_tcp_proxy_with_shutdown;
use overlay_crypto::issue_session_token;
use overlay_transport::session::{SessionHello, write_session_hello};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

#[tokio::test]
async fn proxy_forwards_tcp_stream_to_matching_service() {
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();

    let target_task = tokio::spawn(async move {
        let (mut stream, _) = target_listener.accept().await.unwrap();
        stream.write_all(b"SSH-2.0-OverlayTest\r\n").await.unwrap();
    });

    let cfg: NodeConfig = toml::from_str(&format!(
        r#"
node_id = "node-1"
bind_addr = "127.0.0.1:0"

[[services]]
id = "svc_ssh"
kind = "ssh"
target = "{target_addr}"
"#
    ))
    .unwrap();

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (bound_addr_tx, bound_addr_rx) = oneshot::channel();

    let proxy_task = tokio::spawn(async move {
        run_tcp_proxy_with_shutdown(cfg, "local-secret", shutdown_rx, Some(bound_addr_tx))
            .await
            .unwrap();
    });

    let bound_addr = bound_addr_rx.await.unwrap();
    let mut client = TcpStream::connect(bound_addr).await.unwrap();
    let hello = SessionHello {
        token: issue_session_token("local-secret", "sess-1", "svc_ssh", "node-1").unwrap(),
        service_id: "svc_ssh".into(),
    };
    write_session_hello(&mut client, &hello).await.unwrap();

    let mut banner = Vec::new();
    let mut reader = BufReader::new(client);
    reader.read_until(b'\n', &mut banner).await.unwrap();
    assert_eq!(banner, b"SSH-2.0-OverlayTest\r\n");

    let _ = shutdown_tx.send(());
    proxy_task.await.unwrap();
    target_task.await.unwrap();
}
