use overlay_crypto::issue_session_token;
use overlay_transport::udp_rendezvous::{
    UdpRendezvousMessage, parse_message, resolve_peer, send_node_register,
};
use std::net::UdpSocket;
use tokio::sync::oneshot;

#[tokio::test]
async fn udp_rendezvous_exchanges_reflexive_peer_addresses() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (addr_tx, addr_rx) = oneshot::channel();
    let relay = tokio::spawn(async move {
        relay::run_udp_rendezvous_with_shutdown(
            "127.0.0.1:0",
            Some("relay-secret".into()),
            shutdown_rx,
            Some(addr_tx),
        )
        .await
        .unwrap();
    });
    let relay_addr = addr_rx.await?;

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let node = UdpSocket::bind("127.0.0.1:0")?;
        node.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
        let node_addr = node.local_addr()?;
        send_node_register(&node, relay_addr, "node-1", "relay-secret")?;

        let mut buffer = [0_u8; 1500];
        let (size, _) = node.recv_from(&mut buffer)?;
        assert_eq!(
            parse_message(&buffer[..size])?,
            UdpRendezvousMessage::Registered {
                addr: node_addr.to_string()
            }
        );

        let client = UdpSocket::bind("127.0.0.1:0")?;
        client.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
        let token = issue_session_token("relay-secret", "sess-1", "svc_web", "node-1")?;
        let resolved = resolve_peer(&client, relay_addr, "node-1", &token)?;
        assert_eq!(resolved, node_addr);

        let (size, _) = node.recv_from(&mut buffer)?;
        assert_eq!(
            parse_message(&buffer[..size])?,
            UdpRendezvousMessage::Peer {
                addr: client.local_addr()?.to_string()
            }
        );
        Ok(())
    })
    .await??;

    let _ = shutdown_tx.send(());
    relay.await?;
    Ok(())
}

#[tokio::test]
async fn udp_rendezvous_rejects_client_with_wrong_session_token() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (addr_tx, addr_rx) = oneshot::channel();
    let relay = tokio::spawn(async move {
        relay::run_udp_rendezvous_with_shutdown(
            "127.0.0.1:0",
            Some("relay-secret".into()),
            shutdown_rx,
            Some(addr_tx),
        )
        .await
        .unwrap();
    });
    let relay_addr = addr_rx.await?;

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let node = UdpSocket::bind("127.0.0.1:0")?;
        send_node_register(&node, relay_addr, "node-1", "relay-secret")?;

        let client = UdpSocket::bind("127.0.0.1:0")?;
        client.set_read_timeout(Some(std::time::Duration::from_millis(200)))?;
        let token = issue_session_token("wrong-secret", "sess-1", "svc_web", "node-1")?;
        let error = resolve_peer(&client, relay_addr, "node-1", &token).unwrap_err();
        assert!(error.to_string().contains("timed out"));
        Ok(())
    })
    .await??;

    let _ = shutdown_tx.send(());
    relay.await?;
    Ok(())
}
