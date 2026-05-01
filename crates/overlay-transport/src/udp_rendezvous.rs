use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use crate::p2p_diag;

const PUNCH_BURST_PACKETS: usize = 5;
const PUNCH_BURST_INTERVAL: Duration = Duration::from_millis(40);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum UdpRendezvousMessage {
    Node {
        node_id: String,
        shared_secret: String,
    },
    Client {
        node_id: String,
        token: String,
    },
    Registered {
        addr: String,
    },
    Peer {
        addr: String,
    },
    Punch,
}

pub fn send_message_to(
    socket: &UdpSocket,
    peer_addr: SocketAddr,
    message: &UdpRendezvousMessage,
) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(message)?;
    socket.send_to(&payload, peer_addr)?;
    Ok(())
}

pub fn parse_message(payload: &[u8]) -> anyhow::Result<UdpRendezvousMessage> {
    Ok(serde_json::from_slice(payload)?)
}

pub fn send_node_register(
    socket: &UdpSocket,
    relay_addr: SocketAddr,
    node_id: &str,
    shared_secret: &str,
) -> anyhow::Result<()> {
    send_message_to(
        socket,
        relay_addr,
        &UdpRendezvousMessage::Node {
            node_id: node_id.to_string(),
            shared_secret: shared_secret.to_string(),
        },
    )
}

pub fn resolve_peer(
    socket: &UdpSocket,
    relay_addr: SocketAddr,
    node_id: &str,
    token: &str,
) -> anyhow::Result<SocketAddr> {
    send_message_to(
        socket,
        relay_addr,
        &UdpRendezvousMessage::Client {
            node_id: node_id.to_string(),
            token: token.to_string(),
        },
    )?;
    let mut buffer = [0_u8; 1500];
    for _ in 0..5 {
        match socket.recv_from(&mut buffer) {
            Ok((size, addr)) if addr == relay_addr => {
                if let UdpRendezvousMessage::Peer { addr } = parse_message(&buffer[..size])? {
                    let peer_addr = addr.parse::<SocketAddr>()?;
                    tracing::info!(
                        "{}",
                        p2p_diag::line(
                            "peer_received",
                            "ok",
                            [("peer_addr", peer_addr.to_string().as_str())],
                        )
                    );
                    send_punch_burst(socket, peer_addr)?;
                    return Ok(peer_addr);
                }
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("UDP rendezvous resolve timed out for node {node_id}")
}

pub fn handle_listener_message(
    socket: &UdpSocket,
    payload: &[u8],
) -> anyhow::Result<Option<SocketAddr>> {
    match parse_message(payload)? {
        UdpRendezvousMessage::Peer { addr } => {
            let peer_addr = addr.parse::<SocketAddr>()?;
            tracing::info!(
                "{}",
                p2p_diag::line(
                    "peer_received",
                    "ok",
                    [("peer_addr", peer_addr.to_string().as_str())],
                )
            );
            send_punch_burst(socket, peer_addr)?;
            Ok(Some(peer_addr))
        }
        UdpRendezvousMessage::Punch
        | UdpRendezvousMessage::Registered { .. }
        | UdpRendezvousMessage::Node { .. }
        | UdpRendezvousMessage::Client { .. } => Ok(None),
    }
}

fn send_punch_burst(socket: &UdpSocket, peer_addr: SocketAddr) -> anyhow::Result<()> {
    for index in 0..PUNCH_BURST_PACKETS {
        send_message_to(socket, peer_addr, &UdpRendezvousMessage::Punch)?;
        if index + 1 < PUNCH_BURST_PACKETS {
            std::thread::sleep(PUNCH_BURST_INTERVAL);
        }
    }
    tracing::info!(
        "{}",
        p2p_diag::line(
            "punch_sent",
            "ok",
            [
                ("peer_addr", peer_addr.to_string().as_str()),
                ("packets", PUNCH_BURST_PACKETS.to_string().as_str()),
            ],
        )
    );
    Ok(())
}
