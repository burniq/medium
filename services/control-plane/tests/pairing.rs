use overlay_crypto::issue_bootstrap_code;

#[test]
fn bootstrap_code_has_expected_prefix() {
    let code = issue_bootstrap_code();
    assert!(code.starts_with("ovr-"));
    assert!(code.len() > 12);
}
