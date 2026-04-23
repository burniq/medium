use overlay_protocol::{PublishedService, RegisterNodeRequest, ServiceKind};

use crate::config::NodeConfig;

pub fn build_registration(cfg: &NodeConfig) -> RegisterNodeRequest {
    RegisterNodeRequest {
        node_id: cfg.node_id.clone(),
        services: cfg
            .services
            .iter()
            .map(|svc| PublishedService {
                id: svc.id.clone(),
                kind: match svc.kind.as_str() {
                    "ssh" => ServiceKind::Ssh,
                    _ => ServiceKind::Https,
                },
                target: svc.target.clone(),
            })
            .collect(),
    }
}
