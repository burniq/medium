use relay::config::RelayConfig;

#[test]
fn default_bind_addr_is_set() {
    let cfg = RelayConfig::default();
    assert_eq!(cfg.bind_addr, "0.0.0.0:7001");
}
