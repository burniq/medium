use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
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
    #[serde(default = "default_service_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
struct ServicesConfig {
    #[serde(default)]
    services: Vec<ServiceConfig>,
}

pub fn load_from_path(path: impl AsRef<Path>) -> anyhow::Result<NodeConfig> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read node config {}", path.display()))?;
    let mut cfg: NodeConfig =
        toml::from_str(&raw).with_context(|| format!("parse node config {}", path.display()))?;
    if let Some(services_path) = services_path_for_node_config(path) {
        if services_path.is_file() {
            cfg.services = load_services_from_path(&services_path)?;
        }
    }
    Ok(cfg)
}

pub fn load_services_from_path(path: impl AsRef<Path>) -> anyhow::Result<Vec<ServiceConfig>> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read services config {}", path.display()))?;
    let cfg: ServicesConfig = toml::from_str(&raw)
        .with_context(|| format!("parse services config {}", path.display()))?;
    Ok(cfg.services)
}

pub fn services_path_for_node_config(path: impl AsRef<Path>) -> Option<PathBuf> {
    path.as_ref()
        .parent()
        .map(|parent| parent.join("services.toml"))
}

fn default_bind_addr() -> String {
    "127.0.0.1:17001".into()
}

fn default_ice_bind_addr() -> String {
    "0.0.0.0:17002".into()
}

fn default_service_enabled() -> bool {
    true
}
