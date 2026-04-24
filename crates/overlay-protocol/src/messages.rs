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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    TcpProxy,
}

impl EndpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TcpProxy => "tcp_proxy",
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
    pub schema_version: u32,
    pub label: Option<String>,
    pub target: String,
    pub user_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEndpoint {
    pub kind: EndpointKind,
    pub schema_version: u32,
    pub addr: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterNodeRequest {
    pub node_id: String,
    pub node_label: String,
    pub endpoints: Vec<NodeEndpoint>,
    pub services: Vec<PublishedService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCatalogResponse {
    pub devices: Vec<DeviceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BootstrapInviteResponse {
    pub invite: String,
    pub bootstrap_token: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub ssh: Option<SshEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshEndpoint {
    pub service_id: String,
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
