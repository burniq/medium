pub mod logging;
pub mod p2p_diag;
pub mod pinned_http;
pub mod session;
pub mod udp_rendezvous;
pub mod udp_session;

pub fn install_default_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}
