use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    #[serde(default)]
    pub node_label: Option<String>,
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_ice_bind_addr")]
    pub ice_bind_addr: String,
    #[serde(default)]
    pub public_addr: Option<String>,
    #[serde(default)]
    pub ice_public_addr: Option<String>,
    #[serde(default)]
    pub ice_host_addrs: Vec<String>,
    #[serde(default)]
    pub control_url: Option<String>,
    #[serde(default)]
    pub control_pin: Option<String>,
    #[serde(default)]
    pub shared_secret: Option<String>,
    #[serde(default)]
    pub relay_addr: Option<String>,
    #[serde(default)]
    pub wss_relay_url: Option<String>,
    #[serde(default)]
    pub ice_relay_addr: Option<String>,
    #[serde(default)]
    pub service_ca_cert_pem: Option<String>,
    #[serde(default)]
    pub service_ca_key_pem: Option<String>,
    pub services: Vec<ServiceConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
    pub target: String,
    #[serde(default)]
    pub user_name: Option<String>,
}

pub fn load_from_path(path: impl AsRef<Path>) -> anyhow::Result<NodeConfig> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)?;
    let cfg = toml::from_str(&raw)?;
    Ok(cfg)
}

fn default_bind_addr() -> String {
    "127.0.0.1:17001".into()
}

fn default_ice_bind_addr() -> String {
    "0.0.0.0:17002".into()
}
