use overlay_transport::session::{RelayHello, read_relay_hello, write_relay_hello};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

#[tokio::test]
async fn relay_pairs_client_with_waiting_node_by_node_id() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (addr_tx, addr_rx) = oneshot::channel();
    let relay = tokio::spawn(async move {
        relay::run_tcp_relay_with_shutdown(
            "127.0.0.1:0",
            Some("relay-secret".into()),
            shutdown_rx,
            Some(addr_tx),
        )
        .await
        .unwrap();
    });
    let addr = addr_rx.await?;

    let mut node = TcpStream::connect(addr).await?;
    write_relay_hello(
        &mut node,
        &RelayHello::Node {
            node_id: "node-1".into(),
            shared_secret: "relay-secret".into(),
        },
    )
    .await?;

    let mut client = TcpStream::connect(addr).await?;
    write_relay_hello(
        &mut client,
        &RelayHello::Client {
            node_id: "node-1".into(),
        },
    )
    .await?;

    client.write_all(b"ping").await?;
    client.flush().await?;
    let mut inbound = [0_u8; 4];
    node.read_exact(&mut inbound).await?;
    assert_eq!(&inbound, b"ping");

    node.write_all(b"pong").await?;
    node.flush().await?;
    let mut outbound = [0_u8; 4];
    client.read_exact(&mut outbound).await?;
    assert_eq!(&outbound, b"pong");

    let _ = shutdown_tx.send(());
    relay.await?;
    Ok(())
}

#[tokio::test]
async fn relay_rejects_node_with_wrong_shared_secret() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (addr_tx, addr_rx) = oneshot::channel();
    let relay = tokio::spawn(async move {
        relay::run_tcp_relay_with_shutdown(
            "127.0.0.1:0",
            Some("relay-secret".into()),
            shutdown_rx,
            Some(addr_tx),
        )
        .await
        .unwrap();
    });
    let addr = addr_rx.await?;

    let mut node = TcpStream::connect(addr).await?;
    write_relay_hello(
        &mut node,
        &RelayHello::Node {
            node_id: "node-1".into(),
            shared_secret: "wrong-secret".into(),
        },
    )
    .await?;

    let mut buf = [0_u8; 1];
    let read =
        tokio::time::timeout(std::time::Duration::from_millis(200), node.read(&mut buf)).await??;
    assert_eq!(read, 0);

    let _ = shutdown_tx.send(());
    relay.await?;
    Ok(())
}

#[tokio::test]
async fn relay_hello_shape_is_role_tagged_json() -> anyhow::Result<()> {
    let (mut writer, mut reader) = tokio::io::duplex(1024);
    write_relay_hello(
        &mut writer,
        &RelayHello::Client {
            node_id: "node-1".into(),
        },
    )
    .await?;

    assert_eq!(
        read_relay_hello(&mut reader).await?,
        RelayHello::Client {
            node_id: "node-1".into()
        }
    );
    Ok(())
}
