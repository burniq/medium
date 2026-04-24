use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    #[serde(default)]
    pub node_label: Option<String>,
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    pub services: Vec<ServiceConfig>,
}

#[derive(Debug, Deserialize)]
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
