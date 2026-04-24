use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub services: Vec<ServiceConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceConfig {
    pub id: String,
    pub kind: String,
    pub target: String,
}

pub fn load_from_path(path: impl AsRef<Path>) -> anyhow::Result<NodeConfig> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)?;
    let cfg = toml::from_str(&raw)?;
    Ok(cfg)
}
