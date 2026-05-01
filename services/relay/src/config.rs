#[derive(Debug)]
pub struct RelayConfig {
    pub bind_addr: String,
    pub shared_secret: Option<String>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            bind_addr: std::env::var("MEDIUM_RELAY_BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:7001".into()),
            shared_secret: std::env::var("MEDIUM_RELAY_SHARED_SECRET")
                .or_else(|_| std::env::var("OVERLAY_SHARED_SECRET"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }
}
