use crate::adapters::normalize_target_addr;
use overlay_protocol::{
    EndpointKind, NodeEndpoint, PublishedService, RegisterNodeRequest, ServiceKind,
};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};

use crate::config::NodeConfig;

pub fn build_registration(cfg: &NodeConfig) -> RegisterNodeRequest {
    let mut endpoints = vec![NodeEndpoint {
        kind: EndpointKind::TcpProxy,
        schema_version: 1,
        addr: cfg
            .public_addr
            .clone()
            .unwrap_or_else(|| cfg.bind_addr.clone()),
        priority: 10,
    }];
    endpoints.extend(build_ice_udp_endpoints(cfg));

    RegisterNodeRequest {
        node_id: cfg.node_id.clone(),
        node_label: cfg
            .node_label
            .clone()
            .unwrap_or_else(|| cfg.node_id.clone()),
        endpoints,
        services: cfg
            .services
            .iter()
            .filter(|svc| svc.enabled)
            .map(|svc| PublishedService {
                id: svc.id.clone(),
                kind: match svc.kind.as_str() {
                    "http" => ServiceKind::Http,
                    "https" => ServiceKind::Https,
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

pub fn build_ice_udp_endpoints(cfg: &NodeConfig) -> Vec<NodeEndpoint> {
    let port = ice_bind_port(&cfg.ice_bind_addr).unwrap_or(17002);
    let mut seen = HashSet::new();
    let mut endpoints = Vec::new();

    for addr in &cfg.ice_host_addrs {
        push_ice_endpoint(&mut endpoints, &mut seen, addr.clone(), 300);
    }

    for addr in discover_local_ice_addrs(port) {
        push_ice_endpoint(&mut endpoints, &mut seen, addr, 250);
    }

    push_ice_endpoint(
        &mut endpoints,
        &mut seen,
        effective_ice_public_addr(cfg),
        100,
    );
    endpoints
}

fn push_ice_endpoint(
    endpoints: &mut Vec<NodeEndpoint>,
    seen: &mut HashSet<String>,
    addr: String,
    priority: i32,
) {
    if !seen.insert(addr.clone()) {
        return;
    }
    endpoints.push(NodeEndpoint {
        kind: EndpointKind::IceUdp,
        schema_version: 1,
        addr,
        priority,
    });
}

fn effective_ice_public_addr(cfg: &NodeConfig) -> String {
    if let Some(addr) = &cfg.ice_public_addr {
        return addr.clone();
    }
    if let (Some(public_addr), Some(port)) = (&cfg.public_addr, ice_bind_port(&cfg.ice_bind_addr)) {
        if let Some(host) = addr_host(public_addr) {
            return format_host_port(&host, port);
        }
    }
    cfg.ice_bind_addr.clone()
}

fn ice_bind_port(addr: &str) -> Option<u16> {
    addr.parse::<SocketAddr>()
        .map(|addr| addr.port())
        .ok()
        .or_else(|| addr.rsplit_once(':')?.1.parse().ok())
}

fn addr_host(addr: &str) -> Option<String> {
    if let Ok(socket_addr) = addr.parse::<SocketAddr>() {
        return Some(socket_addr.ip().to_string());
    }
    let (host, _) = addr.rsplit_once(':')?;
    Some(host.trim_matches(['[', ']']).to_string())
}

fn format_host_port(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    }
}

fn discover_local_ice_addrs(port: u16) -> Vec<String> {
    discover_local_ips()
        .into_iter()
        .filter(|ip| is_usable_host_ip(*ip))
        .map(|ip| format_host_port(&ip.to_string(), port))
        .collect()
}

fn is_usable_host_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !(octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        }
        IpAddr::V6(ip) => {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_unicast_link_local()
                && !ip.is_multicast()
        }
    }
}

#[cfg(unix)]
fn discover_local_ips() -> Vec<IpAddr> {
    let mut ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut ifaddrs) } != 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut cursor = ifaddrs;
    while !cursor.is_null() {
        let ifaddr = unsafe { &*cursor };
        if !ifaddr.ifa_addr.is_null() {
            let family = unsafe { (*ifaddr.ifa_addr).sa_family as i32 };
            match family {
                libc::AF_INET => {
                    let sockaddr = unsafe { &*(ifaddr.ifa_addr as *const libc::sockaddr_in) };
                    out.push(IpAddr::from(sockaddr.sin_addr.s_addr.to_ne_bytes()));
                }
                libc::AF_INET6 => {
                    let sockaddr = unsafe { &*(ifaddr.ifa_addr as *const libc::sockaddr_in6) };
                    out.push(IpAddr::from(sockaddr.sin6_addr.s6_addr));
                }
                _ => {}
            }
        }
        cursor = ifaddr.ifa_next;
    }
    unsafe { libc::freeifaddrs(ifaddrs) };
    out
}

#[cfg(not(unix))]
fn discover_local_ips() -> Vec<IpAddr> {
    Vec::new()
}
