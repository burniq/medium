use linux_client::client_api;
use linux_client::state::invite::{Invite, parse_invite};

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

#[test]
fn rejects_invite_with_unsupported_version() {
    assert!(parse_invite("medium://join?v=2&control=http://127.0.0.1:8080&token=abc123").is_err());
}

#[tokio::test]
async fn join_rejects_malformed_control_url() {
    let invite = Invite {
        version: 1,
        control_url: "not-a-url".to_string(),
        bootstrap_token: "abc123".to_string(),
    };

    assert!(client_api::join(&invite).await.is_err());
}
