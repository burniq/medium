use crate::adapters::normalize_target_addr;
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
                target: normalize_target_addr(&svc.target),
            })
            .collect(),
    }
}
