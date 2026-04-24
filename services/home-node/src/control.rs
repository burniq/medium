use crate::adapters::normalize_target_addr;
use overlay_protocol::{
    EndpointKind, NodeEndpoint, PublishedService, RegisterNodeRequest, ServiceKind,
};

use crate::config::NodeConfig;

pub fn build_registration(cfg: &NodeConfig) -> RegisterNodeRequest {
    RegisterNodeRequest {
        node_id: cfg.node_id.clone(),
        node_label: cfg
            .node_label
            .clone()
            .unwrap_or_else(|| cfg.node_id.clone()),
        endpoints: vec![NodeEndpoint {
            kind: EndpointKind::TcpProxy,
            schema_version: 1,
            addr: cfg.bind_addr.clone(),
            priority: 10,
        }],
        services: cfg
            .services
            .iter()
            .map(|svc| PublishedService {
                id: svc.id.clone(),
                kind: match svc.kind.as_str() {
                    "ssh" => ServiceKind::Ssh,
                    _ => ServiceKind::Https,
                },
                schema_version: 1,
                label: svc.label.clone(),
                target: normalize_target_addr(&svc.target),
                user_name: svc.user_name.clone(),
            })
            .collect(),
    }
}
