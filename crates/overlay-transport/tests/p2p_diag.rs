use overlay_transport::p2p_diag;

#[test]
fn p2p_diag_line_formats_stable_key_value_output() {
    let line = p2p_diag::line(
        "session_hello",
        "failed",
        [
            ("session_id", "sess_1"),
            ("peer_addr", "5.138.235.64:65114"),
            ("reason", "UDP session handshake timed out"),
        ],
    );

    assert_eq!(
        line,
        "p2p_diag phase=session_hello result=failed session_id=sess_1 peer_addr=5.138.235.64:65114 reason=\"UDP session handshake timed out\""
    );
}
