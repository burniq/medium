use overlay_protocol::{CandidateKind, PeerCandidate, SessionAuthorization, SessionOpenGrant};
use overlay_transport::session::{
    RelayHello, SessionHello, read_relay_hello, read_session_hello, session_alpn,
    write_relay_hello, write_session_hello,
};
use overlay_transport::udp_rendezvous::{
    UdpRendezvousMessage, handle_listener_message, send_message_to,
};
use overlay_transport::udp_session::{UdpSessionListener, UdpSessionStream};
use std::io::{Read, Write};
use std::net::UdpSocket;
use tokio::io::duplex;

#[test]
fn uses_overlay_alpn() {
    assert_eq!(session_alpn(), b"overlay/1");
}

#[tokio::test]
async fn relay_hello_round_trips_over_stream() {
    let (mut client, mut server) = duplex(1024);
    let expected = RelayHello::Client {
        node_id: "node-1".into(),
    };

    let writer = tokio::spawn(async move {
        write_relay_hello(&mut client, &expected).await.unwrap();
    });

    let actual = read_relay_hello(&mut server).await.unwrap();
    writer.await.unwrap();

    assert_eq!(
        actual,
        RelayHello::Client {
            node_id: "node-1".into()
        }
    );
}

#[test]
fn session_grant_contains_candidate_for_direct_connect() {
    let grant = SessionOpenGrant {
        session_id: "sess1".into(),
        service_id: "svc_openclaw".into(),
        node_id: "node-1".into(),
        relay_hint: None,
        authorization: SessionAuthorization {
            token: "token".into(),
            expires_at: chrono::Utc::now(),
            candidates: vec![PeerCandidate {
                kind: CandidateKind::DirectTcp,
                addr: "198.51.100.10:7001".into(),
                priority: 100,
            }],
            ice: None,
        },
    };

    assert_eq!(grant.authorization.candidates.len(), 1);
}

#[tokio::test]
async fn session_hello_round_trips_over_stream() {
    let (mut client, mut server) = duplex(1024);
    let expected = SessionHello {
        token: "signed-token".into(),
        service_id: "svc_ssh".into(),
    };

    let writer = tokio::spawn(async move {
        write_session_hello(&mut client, &expected).await.unwrap();
    });

    let actual = read_session_hello(&mut server).await.unwrap();
    writer.await.unwrap();

    assert_eq!(actual.service_id, "svc_ssh");
    assert_eq!(actual.token, "signed-token");
}

#[test]
fn udp_session_stream_round_trips_encrypted_bytes() {
    let listener_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let listener_addr = listener_socket.local_addr().unwrap();
    let listener = UdpSessionListener::new(listener_socket);

    let server = std::thread::spawn(move || {
        let accepted = listener.accept().unwrap();
        assert_eq!(accepted.hello.service_id, "svc_web");
        assert_eq!(accepted.hello.token, "signed-token");
        let mut stream = accepted.stream;
        let mut request = [0_u8; 4];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"ping");
        stream.write_all(b"pong").unwrap();
    });

    let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut client = UdpSessionStream::connect(
        client_socket,
        listener_addr,
        SessionHello {
            token: "signed-token".into(),
            service_id: "svc_web".into(),
        },
    )
    .unwrap();
    client.write_all(b"ping").unwrap();
    let mut response = [0_u8; 4];
    client.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"pong");

    server.join().unwrap();
}

#[test]
fn udp_session_connect_uses_peer_reflexive_punch_source() {
    let client_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let client_addr = client_socket.local_addr().unwrap();

    let observed_peer_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let observed_peer_addr = observed_peer_socket.local_addr().unwrap();

    let actual_peer_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let punch_socket = actual_peer_socket.try_clone().unwrap();
    let listener = UdpSessionListener::new(actual_peer_socket);

    let server = std::thread::spawn(move || {
        send_message_to(&punch_socket, client_addr, &UdpRendezvousMessage::Punch).unwrap();
        let accepted = listener.accept().unwrap();
        assert_eq!(accepted.hello.service_id, "svc_web");
        assert_eq!(accepted.hello.token, "signed-token");
    });

    UdpSessionStream::connect(
        client_socket,
        observed_peer_addr,
        SessionHello {
            token: "signed-token".into(),
            service_id: "svc_web".into(),
        },
    )
    .unwrap();

    server.join().unwrap();
}

#[test]
fn rendezvous_peer_message_triggers_punch_burst() {
    let listener_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let listener_addr = listener_socket.local_addr().unwrap();
    let peer_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let peer_addr = peer_socket.local_addr().unwrap();
    peer_socket
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .unwrap();

    send_message_to(
        &peer_socket,
        listener_addr,
        &UdpRendezvousMessage::Peer {
            addr: peer_addr.to_string(),
        },
    )
    .unwrap();

    let mut buffer = [0_u8; 1500];
    let (size, _) = listener_socket.recv_from(&mut buffer).unwrap();
    assert_eq!(
        handle_listener_message(&listener_socket, &buffer[..size])
            .unwrap()
            .unwrap(),
        peer_addr
    );

    let mut punches = 0;
    while let Ok((size, _)) = peer_socket.recv_from(&mut buffer) {
        if matches!(
            overlay_transport::udp_rendezvous::parse_message(&buffer[..size]).unwrap(),
            UdpRendezvousMessage::Punch
        ) {
            punches += 1;
        }
    }
    assert!(punches >= 3, "expected punch burst, got {punches}");
}
