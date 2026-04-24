use linux_client::state::invite::parse_invite;

#[test]
fn parses_versioned_join_invite() {
    let invite =
        parse_invite("medium://join?v=1&control=http://127.0.0.1:8080&token=abc123").unwrap();

    assert_eq!(invite.version, 1);
    assert_eq!(invite.control_url, "http://127.0.0.1:8080");
    assert_eq!(invite.bootstrap_token, "abc123");
}

#[test]
fn rejects_invite_with_unknown_scheme() {
    assert!(parse_invite("overlay://join?v=1").is_err());
}
