#[derive(Debug)]
pub struct RelayConfig {
    pub bind_addr: String,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:7001".into(),
        }
    }
}
