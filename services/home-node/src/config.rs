use serde::Deserialize;

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
