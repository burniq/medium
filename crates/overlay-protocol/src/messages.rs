use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Https,
    Ssh,
}

impl ServiceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Ssh => "ssh",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOpenRequest {
    pub service_id: String,
    pub requester_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOpenGrant {
    pub session_id: String,
    pub service_id: String,
    pub home_node_id: String,
    pub relay_hint: Option<String>,
    pub authorization: SessionAuthorization,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedService {
    pub id: String,
    pub kind: ServiceKind,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterNodeRequest {
    pub node_id: String,
    pub services: Vec<PublishedService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCatalogResponse {
    pub devices: Vec<DeviceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub ssh: Option<SshEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshEndpoint {
    pub host: String,
    pub port: u16,
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCandidate {
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAuthorization {
    pub token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub candidates: Vec<PeerCandidate>,
}
