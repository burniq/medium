use crate::p2p_diag;
use crate::session::SessionHello;
use crate::udp_rendezvous;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

const MAGIC: &[u8; 4] = b"MDU1";
const HEADER_LEN: usize = 15;
const MAX_PACKET_LEN: usize = 1500;
const MAX_PAYLOAD_LEN: usize = 1100;
const RETRIES: usize = 5;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(8_000);
const HANDSHAKE_READ_TIMEOUT: Duration = Duration::from_millis(150);
const ACK_BURST_PACKETS: usize = 5;
const ACK_BURST_INTERVAL: Duration = Duration::from_millis(40);
const DUPLICATE_HANDSHAKE_WINDOW: Duration = Duration::from_secs(30);
const MAX_RECENT_HANDSHAKES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketKind {
    Hello = 1,
    HelloAck = 2,
    Data = 3,
    Ack = 4,
    Close = 5,
}

impl PacketKind {
    fn from_byte(value: u8) -> anyhow::Result<Self> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::HelloAck),
            3 => Ok(Self::Data),
            4 => Ok(Self::Ack),
            5 => Ok(Self::Close),
            _ => anyhow::bail!("unknown UDP session packet kind {value}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::HelloAck => "hello_ack",
            Self::Data => "data",
            Self::Ack => "ack",
            Self::Close => "close",
        }
    }
}

#[derive(Debug)]
struct Packet {
    kind: PacketKind,
    seq: u64,
    payload: Vec<u8>,
}

pub struct AcceptedUdpSession {
    pub peer_addr: SocketAddr,
    pub hello: SessionHello,
    pub stream: UdpSessionStream,
}

pub struct UdpSessionListener {
    accept_timeout: Duration,
    incoming_sessions: Mutex<mpsc::Receiver<anyhow::Result<AcceptedUdpSession>>>,
    running: Arc<AtomicBool>,
}

impl UdpSessionListener {
    pub fn new(socket: UdpSocket) -> Self {
        let accept_timeout = socket
            .read_timeout()
            .ok()
            .flatten()
            .unwrap_or(Duration::from_secs(3));
        let _ = socket.set_read_timeout(Some(accept_timeout));
        let (incoming_tx, incoming_rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        std::thread::spawn({
            let running = running.clone();
            move || run_listener_reader(socket, incoming_tx, running)
        });
        Self {
            accept_timeout,
            incoming_sessions: Mutex::new(incoming_rx),
            running,
        }
    }

    pub fn accept(&self) -> anyhow::Result<AcceptedUdpSession> {
        let receiver = self
            .incoming_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("UDP session incoming queue lock poisoned"))?;
        match receiver.recv_timeout(self.accept_timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err(std::io::Error::from(std::io::ErrorKind::TimedOut).into())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("UDP session listener stopped")
            }
        }
    }
}

impl Drop for UdpSessionListener {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn run_listener_reader(
    socket: UdpSocket,
    incoming_tx: mpsc::Sender<anyhow::Result<AcceptedUdpSession>>,
    running: Arc<AtomicBool>,
) {
    let mut recent_handshakes = VecDeque::new();
    let mut active_sessions: HashMap<SocketAddr, PacketInbox> = HashMap::new();
    let mut buffer = [0_u8; MAX_PACKET_LEN];
    while running.load(Ordering::Relaxed) {
        let (size, peer_addr) = match socket.recv_from(&mut buffer) {
            Ok(received) => received,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(error) => {
                let _ = incoming_tx.send(Err(error.into()));
                return;
            }
        };
        log_udp_source("node_udp_packet_received", peer_addr, &buffer[..size]);
        let packet = match decode_packet(&buffer[..size]) {
            Ok(packet) => packet,
            Err(_) => {
                log_rendezvous_control_packet(peer_addr, &buffer[..size]);
                let _ = udp_rendezvous::handle_listener_message(&socket, &buffer[..size]);
                continue;
            }
        };
        if packet.kind != PacketKind::Hello {
            if let Some(inbox) = active_sessions.get(&peer_addr) {
                inbox.push(packet);
            }
            continue;
        }
        let hello: SessionHello = match serde_json::from_slice(&packet.payload) {
            Ok(hello) => hello,
            Err(error) => {
                let _ = incoming_tx.send(Err(error.into()));
                continue;
            }
        };
        let key = SessionHandshakeKey {
            peer_addr,
            token: hello.token.clone(),
            service_id: hello.service_id.clone(),
        };
        let duplicate = is_duplicate_handshake(&mut recent_handshakes, key, Instant::now());
        tracing::info!(
            "{}",
            p2p_diag::line(
                "session_hello_received",
                if duplicate { "duplicate" } else { "ok" },
                [
                    ("service_id", hello.service_id.as_str()),
                    ("peer_addr", peer_addr.to_string().as_str()),
                ],
            )
        );
        let ack_result = send_ack_burst(
            &socket,
            peer_addr,
            &Packet {
                kind: PacketKind::HelloAck,
                seq: packet.seq,
                payload: Vec::new(),
            },
        );
        if let Err(error) = ack_result {
            let _ = incoming_tx.send(Err(error));
            continue;
        }
        tracing::info!(
            "{}",
            p2p_diag::line(
                "session_ack_sent",
                "ok",
                [
                    ("service_id", hello.service_id.as_str()),
                    ("peer_addr", peer_addr.to_string().as_str()),
                    ("packets", ACK_BURST_PACKETS.to_string().as_str()),
                ],
            )
        );
        if duplicate {
            continue;
        }
        let inbox = PacketInbox::new();
        active_sessions.insert(peer_addr, inbox.clone());
        let stream = match UdpSessionStream::accepted(
            match socket.try_clone() {
                Ok(socket) => socket,
                Err(error) => {
                    let _ = incoming_tx.send(Err(error.into()));
                    continue;
                }
            },
            peer_addr,
            hello.token.clone(),
            Duration::from_secs(3),
            Some(inbox),
        ) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = incoming_tx.send(Err(error));
                continue;
            }
        };
        if incoming_tx
            .send(Ok(AcceptedUdpSession {
                peer_addr,
                hello,
                stream,
            }))
            .is_err()
        {
            return;
        }
    }
}

fn is_duplicate_handshake(
    recent: &mut VecDeque<(SessionHandshakeKey, Instant)>,
    key: SessionHandshakeKey,
    now: Instant,
) -> bool {
    while recent
        .front()
        .is_some_and(|(_, seen_at)| now.duration_since(*seen_at) > DUPLICATE_HANDSHAKE_WINDOW)
    {
        recent.pop_front();
    }
    if recent.iter().any(|(recent_key, _)| recent_key == &key) {
        return true;
    }
    recent.push_back((key, now));
    while recent.len() > MAX_RECENT_HANDSHAKES {
        recent.pop_front();
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionHandshakeKey {
    peer_addr: SocketAddr,
    token: String,
    service_id: String,
}

#[derive(Clone)]
struct PacketInbox {
    inner: Arc<PacketInboxInner>,
}

struct PacketInboxInner {
    packets: Mutex<VecDeque<Packet>>,
    ready: Condvar,
}

impl PacketInbox {
    fn new() -> Self {
        Self {
            inner: Arc::new(PacketInboxInner {
                packets: Mutex::new(VecDeque::new()),
                ready: Condvar::new(),
            }),
        }
    }

    fn push(&self, packet: Packet) {
        if let Ok(mut packets) = self.inner.packets.lock() {
            packets.push_back(packet);
            self.inner.ready.notify_all();
        }
    }

    fn recv_matching<F>(&self, timeout: Duration, predicate: F) -> std::io::Result<Packet>
    where
        F: Fn(&Packet) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut packets = self
            .inner
            .packets
            .lock()
            .map_err(|_| std::io::Error::other("UDP session inbox lock poisoned"))?;
        loop {
            if let Some(index) = packets.iter().position(&predicate) {
                return Ok(packets.remove(index).expect("packet index checked"));
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            let wait_for = deadline.saturating_duration_since(now);
            let (next_packets, wait_result) = self
                .inner
                .ready
                .wait_timeout(packets, wait_for)
                .map_err(|_| std::io::Error::other("UDP session inbox lock poisoned"))?;
            packets = next_packets;
            if wait_result.timed_out() && !packets.iter().any(&predicate) {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
        }
    }
}

struct SessionState {
    send_seq: u64,
    recv_seq: u64,
    pending_read: VecDeque<u8>,
    pending_packets: BTreeMap<u64, Vec<u8>>,
}

pub struct UdpSessionStream {
    socket: UdpSocket,
    peer_addr: SocketAddr,
    cipher: ChaCha20Poly1305,
    state: Arc<Mutex<SessionState>>,
    inbox: Option<PacketInbox>,
    direction: Direction,
}

impl UdpSessionStream {
    pub fn connect(
        socket: UdpSocket,
        peer_addr: SocketAddr,
        hello: SessionHello,
    ) -> anyhow::Result<Self> {
        socket.set_read_timeout(Some(HANDSHAKE_READ_TIMEOUT))?;
        socket.set_write_timeout(Some(Duration::from_millis(500)))?;
        let payload = serde_json::to_vec(&hello)?;
        let packet = Packet {
            kind: PacketKind::Hello,
            seq: 0,
            payload,
        };
        let mut buffer = [0_u8; MAX_PACKET_LEN];
        let mut peer_addrs = vec![peer_addr];
        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        while Instant::now() < deadline {
            send_handshake_burst(&socket, &peer_addrs, &packet)?;
            match socket.recv_from(&mut buffer) {
                Ok((size, addr)) => {
                    log_udp_source("client_udp_packet_received", addr, &buffer[..size]);
                    let response = decode_packet(&buffer[..size]);
                    if is_handshake_ack_from_peer(&peer_addrs, addr, response.as_ref().ok()) {
                        if !peer_addrs.contains(&addr) {
                            tracing::info!(
                                "{}",
                                p2p_diag::line(
                                    "peer_reflexive_ack",
                                    "observed",
                                    [
                                        (
                                            "known_peer_addrs",
                                            join_socket_addrs(&peer_addrs).as_str()
                                        ),
                                        ("new_peer_addr", addr.to_string().as_str()),
                                    ],
                                )
                            );
                        }
                        return Self::new(
                            socket,
                            addr,
                            hello.token,
                            Duration::from_millis(100),
                            Direction::ClientToNode,
                            None,
                        );
                    }
                    if record_peer_reflexive_candidate(&mut peer_addrs, addr, &buffer[..size]) {
                        continue;
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!(
            "UDP session handshake timed out for {}",
            join_socket_addrs(&peer_addrs)
        )
    }

    fn accepted(
        socket: UdpSocket,
        peer_addr: SocketAddr,
        token: String,
        timeout: Duration,
        inbox: Option<PacketInbox>,
    ) -> anyhow::Result<Self> {
        Self::new(
            socket,
            peer_addr,
            token,
            timeout,
            Direction::NodeToClient,
            inbox,
        )
    }

    fn new(
        socket: UdpSocket,
        peer_addr: SocketAddr,
        token: String,
        timeout: Duration,
        direction: Direction,
        inbox: Option<PacketInbox>,
    ) -> anyhow::Result<Self> {
        socket.set_read_timeout(Some(timeout))?;
        socket.set_write_timeout(Some(timeout))?;
        let key_bytes = Sha256::digest(token.as_bytes());
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        Ok(Self {
            socket,
            peer_addr,
            cipher,
            state: Arc::new(Mutex::new(SessionState {
                send_seq: 0,
                recv_seq: 0,
                pending_read: VecDeque::new(),
                pending_packets: BTreeMap::new(),
            })),
            inbox,
            direction,
        })
    }

    pub fn set_poll_timeout(&self, timeout: Duration) -> anyhow::Result<()> {
        self.socket.set_read_timeout(Some(timeout))?;
        self.socket.set_write_timeout(Some(timeout))?;
        Ok(())
    }

    pub fn try_clone(&self) -> anyhow::Result<Self> {
        Ok(Self {
            socket: self.socket.try_clone()?,
            peer_addr: self.peer_addr,
            cipher: self.cipher.clone(),
            state: self.state.clone(),
            inbox: self.inbox.clone(),
            direction: self.direction,
        })
    }

    fn send_ack(&self, seq: u64) -> std::io::Result<()> {
        send_packet_to(
            &self.socket,
            self.peer_addr,
            &Packet {
                kind: PacketKind::Ack,
                seq,
                payload: Vec::new(),
            },
        )
        .map_err(std::io::Error::other)
    }

    fn wait_for_ack(&mut self, seq: u64) -> std::io::Result<()> {
        loop {
            match self.recv_session_packet(|packet| {
                matches!(
                    packet.kind,
                    PacketKind::Ack | PacketKind::Data | PacketKind::Close
                )
            }) {
                Ok(packet) => match packet.kind {
                    PacketKind::Ack if packet.seq == seq => return Ok(()),
                    PacketKind::Data => self.handle_data_packet(packet)?,
                    PacketKind::Close => return Err(std::io::ErrorKind::UnexpectedEof.into()),
                    _ => {}
                },
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn handle_data_packet(&mut self, packet: Packet) -> std::io::Result<()> {
        let seq = packet.seq;
        let plaintext = self
            .cipher
            .decrypt(
                Nonce::from_slice(&nonce_bytes(self.direction.peer(), seq)),
                packet.payload.as_ref(),
            )
            .map_err(|_| std::io::Error::other("UDP session decrypt failed"))?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("UDP session state lock poisoned"))?;

        if seq < state.recv_seq {
            drop(state);
            self.send_ack(seq)?;
            return Ok(());
        }

        if seq > state.recv_seq {
            state.pending_packets.entry(seq).or_insert(plaintext);
            drop(state);
            self.send_ack(seq)?;
            return Ok(());
        }

        state.pending_read.extend(plaintext);
        state.recv_seq += 1;
        while let Some(next_payload) = {
            let next_seq = state.recv_seq;
            state.pending_packets.remove(&next_seq)
        } {
            state.pending_read.extend(next_payload);
            state.recv_seq += 1;
        }
        drop(state);
        self.send_ack(seq)?;
        Ok(())
    }

    fn recv_session_packet<F>(&self, predicate: F) -> std::io::Result<Packet>
    where
        F: Fn(&Packet) -> bool,
    {
        if let Some(inbox) = &self.inbox {
            return inbox.recv_matching(self.timeout(), predicate);
        }

        let mut packet_buffer = [0_u8; MAX_PACKET_LEN];
        loop {
            match self.socket.recv_from(&mut packet_buffer) {
                Ok((size, addr)) if addr == self.peer_addr => {
                    let packet =
                        decode_packet(&packet_buffer[..size]).map_err(std::io::Error::other)?;
                    if predicate(&packet) {
                        return Ok(packet);
                    }
                }
                Ok(_) => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn timeout(&self) -> Duration {
        self.socket
            .read_timeout()
            .ok()
            .flatten()
            .unwrap_or(Duration::from_secs(3))
    }
}

impl Read for UdpSessionStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| std::io::Error::other("UDP session state lock poisoned"))?;
            if !state.pending_read.is_empty() {
                return drain_pending(&mut state.pending_read, buffer);
            }
        }

        loop {
            match self.recv_session_packet(|packet| {
                matches!(packet.kind, PacketKind::Data | PacketKind::Close)
            }) {
                Ok(packet) => match packet.kind {
                    PacketKind::Data => {
                        self.handle_data_packet(packet)?;
                        let mut state = self.state.lock().map_err(|_| {
                            std::io::Error::other("UDP session state lock poisoned")
                        })?;
                        if !state.pending_read.is_empty() {
                            return drain_pending(&mut state.pending_read, buffer);
                        }
                    }
                    PacketKind::Close => return Ok(0),
                    _ => {}
                },
                Err(error) => return Err(error),
            }
        }
    }
}

impl Write for UdpSessionStream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let chunk = &bytes[..bytes.len().min(MAX_PAYLOAD_LEN)];
        let seq = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("UDP session state lock poisoned"))?
            .send_seq;
        let ciphertext = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes(self.direction, seq)),
                chunk.as_ref(),
            )
            .map_err(|_| std::io::Error::other("UDP session encrypt failed"))?;
        let packet = Packet {
            kind: PacketKind::Data,
            seq,
            payload: ciphertext,
        };

        let mut last_error = None;
        for _ in 0..RETRIES {
            send_packet_to(&self.socket, self.peer_addr, &packet).map_err(std::io::Error::other)?;
            match self.wait_for_ack(seq) {
                Ok(()) => {
                    self.state
                        .lock()
                        .map_err(|_| std::io::Error::other("UDP session state lock poisoned"))?
                        .send_seq += 1;
                    return Ok(chunk.len());
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut =>
                {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error
            .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "missing ACK")))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    ClientToNode,
    NodeToClient,
}

impl Direction {
    fn byte(self) -> u8 {
        match self {
            Self::ClientToNode => 0,
            Self::NodeToClient => 1,
        }
    }

    fn peer(self) -> Self {
        match self {
            Self::ClientToNode => Self::NodeToClient,
            Self::NodeToClient => Self::ClientToNode,
        }
    }
}

fn is_handshake_ack_from_peer(
    peer_addrs: &[SocketAddr],
    packet_addr: SocketAddr,
    response: Option<&Packet>,
) -> bool {
    if !peer_addrs
        .iter()
        .any(|peer_addr| packet_addr.ip() == peer_addr.ip())
    {
        return false;
    }
    matches!(
        response,
        Some(Packet {
            kind: PacketKind::HelloAck,
            seq: 0,
            ..
        })
    )
}

fn log_udp_source(phase: &str, peer_addr: SocketAddr, payload: &[u8]) {
    let packet_kind = decode_packet(payload)
        .map(|packet| packet.kind.as_str().to_string())
        .or_else(|_| udp_rendezvous::parse_message(payload).map(rendezvous_message_kind))
        .unwrap_or_else(|_| "unknown".to_string());
    tracing::info!(
        "{}",
        p2p_diag::line(
            phase,
            "received",
            [
                ("peer_addr", peer_addr.to_string().as_str()),
                ("packet_kind", packet_kind.as_str()),
            ],
        )
    );
}

fn rendezvous_message_kind(message: udp_rendezvous::UdpRendezvousMessage) -> String {
    match message {
        udp_rendezvous::UdpRendezvousMessage::Node { .. } => "rendezvous_node",
        udp_rendezvous::UdpRendezvousMessage::Client { .. } => "rendezvous_client",
        udp_rendezvous::UdpRendezvousMessage::Registered { .. } => "rendezvous_registered",
        udp_rendezvous::UdpRendezvousMessage::Peer { .. } => "rendezvous_peer",
        udp_rendezvous::UdpRendezvousMessage::Punch => "rendezvous_punch",
    }
    .to_string()
}

fn log_rendezvous_control_packet(peer_addr: SocketAddr, payload: &[u8]) {
    if matches!(
        udp_rendezvous::parse_message(payload),
        Ok(udp_rendezvous::UdpRendezvousMessage::Punch)
    ) {
        tracing::info!(
            "{}",
            p2p_diag::line(
                "punch_received",
                "ok",
                [("peer_addr", peer_addr.to_string().as_str())],
            )
        );
    }
}

fn record_peer_reflexive_candidate(
    peer_addrs: &mut Vec<SocketAddr>,
    packet_addr: SocketAddr,
    payload: &[u8],
) -> bool {
    if !peer_addrs
        .iter()
        .any(|peer_addr| packet_addr.ip() == peer_addr.ip())
        || peer_addrs.contains(&packet_addr)
    {
        return false;
    }
    if !matches!(
        udp_rendezvous::parse_message(payload),
        Ok(udp_rendezvous::UdpRendezvousMessage::Punch)
    ) {
        return false;
    }
    tracing::info!(
        "{}",
        p2p_diag::line(
            "peer_reflexive_candidate",
            "observed",
            [
                ("known_peer_addrs", join_socket_addrs(peer_addrs).as_str()),
                ("new_peer_addr", packet_addr.to_string().as_str()),
            ],
        )
    );
    peer_addrs.push(packet_addr);
    true
}

fn join_socket_addrs(peer_addrs: &[SocketAddr]) -> String {
    peer_addrs
        .iter()
        .map(SocketAddr::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn nonce_bytes(direction: Direction, seq: u64) -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    nonce[0] = direction.byte();
    nonce[4..12].copy_from_slice(&seq.to_be_bytes());
    nonce
}

fn drain_pending(pending: &mut VecDeque<u8>, buffer: &mut [u8]) -> std::io::Result<usize> {
    let size = buffer.len().min(pending.len());
    for slot in buffer.iter_mut().take(size) {
        *slot = pending.pop_front().expect("pending length checked");
    }
    Ok(size)
}

fn send_packet_to(
    socket: &UdpSocket,
    peer_addr: SocketAddr,
    packet: &Packet,
) -> anyhow::Result<()> {
    let encoded = encode_packet(packet)?;
    socket.send_to(&encoded, peer_addr)?;
    Ok(())
}

fn send_handshake_burst(
    socket: &UdpSocket,
    peer_addrs: &[SocketAddr],
    packet: &Packet,
) -> anyhow::Result<()> {
    for peer_addr in peer_addrs {
        send_packet_to(socket, *peer_addr, packet)?;
    }
    tracing::info!(
        "{}",
        p2p_diag::line(
            "session_hello_burst_sent",
            "ok",
            [
                ("peer_addrs", join_socket_addrs(peer_addrs).as_str()),
                ("packets", peer_addrs.len().to_string().as_str()),
            ],
        )
    );
    Ok(())
}

fn send_ack_burst(
    socket: &UdpSocket,
    peer_addr: SocketAddr,
    packet: &Packet,
) -> anyhow::Result<()> {
    for index in 0..ACK_BURST_PACKETS {
        send_packet_to(socket, peer_addr, packet)?;
        if index + 1 < ACK_BURST_PACKETS {
            std::thread::sleep(ACK_BURST_INTERVAL);
        }
    }
    Ok(())
}

fn encode_packet(packet: &Packet) -> anyhow::Result<Vec<u8>> {
    if packet.payload.len() > u16::MAX as usize {
        anyhow::bail!("UDP session packet is too large");
    }
    let mut output = Vec::with_capacity(HEADER_LEN + packet.payload.len());
    output.extend_from_slice(MAGIC);
    output.push(packet.kind as u8);
    output.extend_from_slice(&packet.seq.to_be_bytes());
    output.extend_from_slice(&(packet.payload.len() as u16).to_be_bytes());
    output.extend_from_slice(&packet.payload);
    Ok(output)
}

fn decode_packet(bytes: &[u8]) -> anyhow::Result<Packet> {
    if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
        anyhow::bail!("invalid UDP session packet");
    }
    let kind = PacketKind::from_byte(bytes[4])?;
    let seq = u64::from_be_bytes(bytes[5..13].try_into()?);
    let len = u16::from_be_bytes(bytes[13..15].try_into()?) as usize;
    if bytes.len() != HEADER_LEN + len {
        anyhow::bail!("truncated UDP session packet");
    }
    Ok(Packet {
        kind,
        seq,
        payload: bytes[HEADER_LEN..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn connect_accepts_peer_reflexive_ack_source() {
        let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client_addr = client_socket.local_addr().unwrap();

        let observed_peer_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let observed_peer_addr = observed_peer_socket.local_addr().unwrap();

        let handshake_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ack_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let punch_socket = handshake_socket.try_clone().unwrap();

        let server = std::thread::spawn(move || {
            udp_rendezvous::send_message_to(
                &punch_socket,
                client_addr,
                &udp_rendezvous::UdpRendezvousMessage::Punch,
            )
            .unwrap();

            let mut buffer = [0_u8; MAX_PACKET_LEN];
            let (size, _) = handshake_socket.recv_from(&mut buffer).unwrap();
            let packet = decode_packet(&buffer[..size]).unwrap();
            assert_eq!(packet.kind, PacketKind::Hello);

            let ack = encode_packet(&Packet {
                kind: PacketKind::HelloAck,
                seq: 0,
                payload: Vec::new(),
            })
            .unwrap();
            ack_socket.send_to(&ack, client_addr).unwrap();
        });

        UdpSessionStream::connect(
            client_socket,
            observed_peer_addr,
            SessionHello {
                token: "signed-token".into(),
                service_id: "svc_web".into(),
                transport: None,
            },
        )
        .unwrap();

        server.join().unwrap();
    }

    #[test]
    fn connect_waits_for_ack_after_peer_reflexive_punch_burst() {
        let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client_addr = client_socket.local_addr().unwrap();

        let observed_peer_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let observed_peer_addr = observed_peer_socket.local_addr().unwrap();

        let handshake_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let punch_socket = handshake_socket.try_clone().unwrap();

        let server = std::thread::spawn(move || {
            for _ in 0..RETRIES {
                udp_rendezvous::send_message_to(
                    &punch_socket,
                    client_addr,
                    &udp_rendezvous::UdpRendezvousMessage::Punch,
                )
                .unwrap();
            }

            let mut buffer = [0_u8; MAX_PACKET_LEN];
            let (size, _) = handshake_socket.recv_from(&mut buffer).unwrap();
            let packet = decode_packet(&buffer[..size]).unwrap();
            assert_eq!(packet.kind, PacketKind::Hello);

            send_packet_to(
                &handshake_socket,
                client_addr,
                &Packet {
                    kind: PacketKind::HelloAck,
                    seq: 0,
                    payload: Vec::new(),
                },
            )
            .unwrap();
        });

        UdpSessionStream::connect(
            client_socket,
            observed_peer_addr,
            SessionHello {
                token: "signed-token".into(),
                service_id: "svc_web".into(),
                transport: None,
            },
        )
        .unwrap();

        server.join().unwrap();
    }

    #[test]
    fn listener_sends_ack_burst_for_session_hello() {
        let listener_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let listener_addr = listener_socket.local_addr().unwrap();
        let listener = UdpSessionListener::new(listener_socket);

        let server = std::thread::spawn(move || {
            let accepted = listener.accept().unwrap();
            assert_eq!(accepted.hello.service_id, "svc_web");
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        client_socket
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let payload = serde_json::to_vec(&SessionHello {
            token: "signed-token".into(),
            service_id: "svc_web".into(),
            transport: None,
        })
        .unwrap();
        send_packet_to(
            &client_socket,
            listener_addr,
            &Packet {
                kind: PacketKind::Hello,
                seq: 0,
                payload,
            },
        )
        .unwrap();

        let mut buffer = [0_u8; MAX_PACKET_LEN];
        let mut ack_count = 0;
        while let Ok((size, _)) = client_socket.recv_from(&mut buffer) {
            let packet = decode_packet(&buffer[..size]).unwrap();
            if packet.kind == PacketKind::HelloAck {
                ack_count += 1;
            }
        }

        assert!(ack_count >= 3, "expected ACK burst, got {ack_count}");
        server.join().unwrap();
    }

    #[test]
    fn listener_does_not_accept_duplicate_session_hello_as_new_session() {
        let listener_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        listener_socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        let listener_addr = listener_socket.local_addr().unwrap();
        let listener = UdpSessionListener::new(listener_socket);

        let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let payload = serde_json::to_vec(&SessionHello {
            token: "signed-token".into(),
            service_id: "svc_web".into(),
            transport: None,
        })
        .unwrap();
        let hello = Packet {
            kind: PacketKind::Hello,
            seq: 0,
            payload,
        };

        send_packet_to(&client_socket, listener_addr, &hello).unwrap();
        send_packet_to(&client_socket, listener_addr, &hello).unwrap();

        let accepted = listener.accept().unwrap();
        assert_eq!(accepted.hello.service_id, "svc_web");

        let duplicate = match listener.accept() {
            Ok(_) => panic!("duplicate hello was accepted as a new session"),
            Err(error) => error,
        };
        assert!(
            duplicate
                .chain()
                .find_map(|cause| cause.downcast_ref::<std::io::Error>())
                .is_some_and(|error| matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                )),
            "duplicate hello should be ACKed and ignored, got {duplicate:#}"
        );
    }

    #[test]
    fn listener_routes_active_session_data_while_accepting_new_sessions() {
        let listener_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        listener_socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        let listener_addr = listener_socket.local_addr().unwrap();
        let listener = Arc::new(UdpSessionListener::new(listener_socket));

        let accept_first = {
            let listener = listener.clone();
            std::thread::spawn(move || listener.accept().unwrap())
        };

        let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut client = UdpSessionStream::connect(
            client_socket,
            listener_addr,
            SessionHello {
                token: "signed-token".into(),
                service_id: "svc_web".into(),
                transport: None,
            },
        )
        .unwrap();
        let mut accepted = accept_first.join().unwrap().stream;

        let accept_second = {
            let listener = listener.clone();
            std::thread::spawn(move || listener.accept())
        };
        std::thread::sleep(Duration::from_millis(50));

        let read_active = std::thread::spawn(move || {
            let mut received = [0_u8; 4];
            std::io::Read::read_exact(&mut accepted, &mut received).unwrap();
            received
        });
        std::thread::sleep(Duration::from_millis(50));
        std::io::Write::write_all(&mut client, b"ping").unwrap();
        assert_eq!(&read_active.join().unwrap(), b"ping");

        let second = match accept_second.join().unwrap() {
            Ok(_) => panic!("listener accepted active session data as a new session"),
            Err(error) => error,
        };
        assert!(
            second
                .chain()
                .find_map(|cause| cause.downcast_ref::<std::io::Error>())
                .is_some_and(|error| matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                )),
            "listener should keep waiting for new sessions while routing active data"
        );
    }

    #[test]
    fn stream_buffers_out_of_order_data_without_reporting_eof() {
        let token = "signed-token";
        let node_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer_addr = peer_socket.local_addr().unwrap();
        let inbox = PacketInbox::new();
        let mut stream = UdpSessionStream::accepted(
            node_socket,
            peer_addr,
            token.to_string(),
            Duration::from_millis(25),
            Some(inbox.clone()),
        )
        .unwrap();

        inbox.push(encrypted_data_packet(
            token,
            Direction::ClientToNode,
            1,
            b"def",
        ));
        let mut first_read = [0_u8; 6];
        let first_result = stream.read(&mut first_read);
        assert!(
            matches!(&first_result, Err(error) if error.kind() == std::io::ErrorKind::TimedOut),
            "out-of-order data must wait for the missing sequence instead of EOF, got {first_result:?}"
        );

        inbox.push(encrypted_data_packet(
            token,
            Direction::ClientToNode,
            0,
            b"abc",
        ));
        let size = stream.read(&mut first_read).unwrap();

        assert_eq!(size, 6);
        assert_eq!(&first_read[..size], b"abcdef");
    }

    fn encrypted_data_packet(token: &str, direction: Direction, seq: u64, bytes: &[u8]) -> Packet {
        let key_bytes = Sha256::digest(token.as_bytes());
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        Packet {
            kind: PacketKind::Data,
            seq,
            payload: cipher
                .encrypt(Nonce::from_slice(&nonce_bytes(direction, seq)), bytes)
                .unwrap(),
        }
    }
}
