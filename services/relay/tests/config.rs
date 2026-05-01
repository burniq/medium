use relay::config::RelayConfig;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn default_bind_addr_is_set() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    unsafe {
        std::env::remove_var("MEDIUM_RELAY_BIND_ADDR");
        std::env::remove_var("MEDIUM_RELAY_SHARED_SECRET");
        std::env::remove_var("OVERLAY_SHARED_SECRET");
    }
    let cfg = RelayConfig::default();
    assert_eq!(cfg.bind_addr, "0.0.0.0:7001");
    assert_eq!(cfg.shared_secret, None);
}

#[test]
fn bind_addr_can_be_overridden() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    unsafe {
        std::env::set_var("MEDIUM_RELAY_BIND_ADDR", "127.0.0.1:7002");
    }
    let cfg = RelayConfig::default();
    assert_eq!(cfg.bind_addr, "127.0.0.1:7002");
    unsafe {
        std::env::remove_var("MEDIUM_RELAY_BIND_ADDR");
    }
}

#[test]
fn shared_secret_can_be_overridden() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    unsafe {
        std::env::set_var("MEDIUM_RELAY_SHARED_SECRET", "relay-secret");
    }
    let cfg = RelayConfig::default();
    assert_eq!(cfg.shared_secret.as_deref(), Some("relay-secret"));
    unsafe {
        std::env::remove_var("MEDIUM_RELAY_SHARED_SECRET");
    }
}
