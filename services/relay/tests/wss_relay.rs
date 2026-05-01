use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::test]
async fn wss_relay_pairs_client_with_waiting_node_by_node_id() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    drop(listener);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let relay = tokio::spawn(async move {
        relay::run_wss_relay_with_shutdown(
            &addr.to_string(),
            Some("relay-secret".into()),
            shutdown_rx,
        )
        .await
    });

    wait_tcp(addr).await?;

    let node_url = format!("ws://{addr}/medium/v1/relay");
    let client_url = node_url.clone();
    let (mut node_ws, _) = connect_async(node_url).await?;
    let (mut client_ws, _) = connect_async(client_url).await?;

    node_ws
        .send(Message::Text(
            r#"{"role":"node","node_id":"node-1","shared_secret":"relay-secret"}"#.into(),
        ))
        .await?;
    client_ws
        .send(Message::Text(
            r#"{"role":"client","node_id":"node-1"}"#.into(),
        ))
        .await?;

    client_ws
        .send(Message::Binary(b"ping".to_vec().into()))
        .await?;
    assert_eq!(&node_ws.next().await.unwrap()?.into_data()[..], b"ping");

    node_ws
        .send(Message::Binary(b"pong".to_vec().into()))
        .await?;
    assert_eq!(&client_ws.next().await.unwrap()?.into_data()[..], b"pong");

    shutdown_tx.send(()).ok();
    relay.await??;
    Ok(())
}

async fn wait_tcp(addr: std::net::SocketAddr) -> anyhow::Result<()> {
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    anyhow::bail!("relay did not start at {addr}")
}
