use smoltcp::wire::{IpCidr, Ipv4Address};
use std::collections::{BTreeMap, VecDeque};

pub mod stack {
    use super::*;
    use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
    use smoltcp::socket::{tcp as smol_tcp, udp};
    use smoltcp::time::Instant;
    use smoltcp::wire::{HardwareAddress, IpAddress, IpEndpoint, IpListenEndpoint};

    const DNS_PORT: u16 = 53;
    const HTTP_PORT: u16 = 80;
    const HTTPS_PORT: u16 = 443;

    pub struct MediumStack<'a> {
        network: VirtualNetwork,
        iface: Interface,
        sockets: SocketSet<'a>,
        device: tun::PacketDevice,
        dns_handle: SocketHandle,
        tcp_slots: Vec<TcpSlot>,
        next_tcp_stream: u64,
    }

    struct TcpSlot {
        handle: SocketHandle,
        service_id: String,
        hostname: String,
        port: u16,
        stream_id: Option<String>,
    }

    impl MediumStack<'static> {
        pub fn new(network: VirtualNetwork) -> anyhow::Result<Self> {
            let mut device = tun::PacketDevice::new(1500);
            let config = Config::new(HardwareAddress::Ip);
            let mut iface = Interface::new(config, &mut device, Instant::ZERO);
            iface.update_ip_addrs(|ip_addrs| {
                ip_addrs
                    .push(IpCidr::new(network.dns_addr().into(), network.prefix_len()))
                    .unwrap();
                for service in network.services() {
                    ip_addrs
                        .push(IpCidr::new(IpAddress::Ipv4(service.addr), 32))
                        .unwrap();
                }
            });

            let mut sockets = SocketSet::new(Vec::new());
            let dns_handle = sockets.add(udp::Socket::new(
                udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; 8192]),
                udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; 8192]),
            ));
            let mut tcp_slots = Vec::with_capacity(network.services().len() * 2);
            for service in network.services() {
                for port in [HTTP_PORT, HTTPS_PORT] {
                    tcp_slots.push(TcpSlot {
                        handle: sockets.add(smol_tcp::Socket::new(
                            smol_tcp::SocketBuffer::new(vec![0; 64 * 1024]),
                            smol_tcp::SocketBuffer::new(vec![0; 64 * 1024]),
                        )),
                        service_id: service.id.clone(),
                        hostname: service.hostname.clone(),
                        port,
                        stream_id: None,
                    });
                }
            }

            Ok(Self {
                network,
                iface,
                sockets,
                device,
                dns_handle,
                tcp_slots,
                next_tcp_stream: 0,
            })
        }
    }

    impl MediumStack<'_> {
        pub fn push_tun_packet(&mut self, packet: Vec<u8>) {
            self.device.push_tun_packet(packet);
        }

        pub fn pop_tun_packet(&mut self) -> Option<Vec<u8>> {
            self.device.pop_tun_packet()
        }

        pub fn poll(&mut self, now_millis: i64) -> anyhow::Result<Vec<tcp::TcpPumpEvent>> {
            let now = Instant::from_millis(now_millis);
            self.ensure_dns_bound()?;
            self.ensure_tcp_listeners()?;
            self.iface.poll(now, &mut self.device, &mut self.sockets);
            self.pump_dns()?;
            let events = self.pump_tcp()?;
            self.iface.poll(now, &mut self.device, &mut self.sockets);
            Ok(events)
        }

        pub fn send_tcp(&mut self, stream_id: &str, bytes: &[u8]) -> anyhow::Result<usize> {
            let slot = self
                .tcp_slots
                .iter()
                .find(|slot| slot.stream_id.as_deref() == Some(stream_id))
                .ok_or_else(|| anyhow::anyhow!("unknown tcp stream: {stream_id}"))?;
            let socket = self.sockets.get_mut::<smol_tcp::Socket>(slot.handle);
            tcp::send(socket, bytes)
        }

        pub fn close_tcp(&mut self, stream_id: &str) -> anyhow::Result<()> {
            let slot = self
                .tcp_slots
                .iter_mut()
                .find(|slot| slot.stream_id.as_deref() == Some(stream_id))
                .ok_or_else(|| anyhow::anyhow!("unknown tcp stream: {stream_id}"))?;
            let socket = self.sockets.get_mut::<smol_tcp::Socket>(slot.handle);
            socket.close();
            slot.stream_id = None;
            Ok(())
        }

        fn ensure_dns_bound(&mut self) -> anyhow::Result<()> {
            let socket = self.sockets.get_mut::<udp::Socket>(self.dns_handle);
            if !socket.is_open() {
                socket.bind(DNS_PORT)?;
            }
            Ok(())
        }

        fn ensure_tcp_listeners(&mut self) -> anyhow::Result<()> {
            let desired_listeners = self
                .network
                .services()
                .iter()
                .flat_map(|service| {
                    [HTTP_PORT, HTTPS_PORT]
                        .into_iter()
                        .map(move |port| (service.id.clone(), IpAddress::Ipv4(service.addr), port))
                })
                .collect::<Vec<_>>();

            for slot in &mut self.tcp_slots {
                let Some(service) = self
                    .network
                    .services()
                    .iter()
                    .find(|service| service.id == slot.service_id)
                else {
                    continue;
                };
                let socket = self.sockets.get_mut::<smol_tcp::Socket>(slot.handle);
                if !socket.is_open() {
                    socket.listen(IpListenEndpoint {
                        addr: Some(IpAddress::Ipv4(service.addr)),
                        port: slot.port,
                    })?;
                }
            }

            for (service_id, addr, port) in desired_listeners {
                let has_listener = self.tcp_slots.iter().any(|slot| {
                    if slot.service_id != service_id
                        || slot.port != port
                        || slot.stream_id.is_some()
                    {
                        return false;
                    }
                    let socket = self.sockets.get::<smol_tcp::Socket>(slot.handle);
                    socket.state() == smol_tcp::State::Listen
                });
                if !has_listener {
                    let handle = self.sockets.add(smol_tcp::Socket::new(
                        smol_tcp::SocketBuffer::new(vec![0; 64 * 1024]),
                        smol_tcp::SocketBuffer::new(vec![0; 64 * 1024]),
                    ));
                    let socket = self.sockets.get_mut::<smol_tcp::Socket>(handle);
                    socket.listen(IpListenEndpoint {
                        addr: Some(addr),
                        port,
                    })?;
                    let hostname = self
                        .network
                        .services()
                        .iter()
                        .find(|service| service.id == service_id)
                        .map(|service| service.hostname.clone())
                        .unwrap_or_else(|| service_id.clone());
                    self.tcp_slots.push(TcpSlot {
                        handle,
                        service_id,
                        hostname,
                        port,
                        stream_id: None,
                    });
                }
            }
            Ok(())
        }

        fn pump_dns(&mut self) -> anyhow::Result<()> {
            let socket = self.sockets.get_mut::<udp::Socket>(self.dns_handle);
            while socket.can_recv() {
                let (response, remote) = {
                    let (data, endpoint) = socket.recv()?;
                    (dns::answer_query(data, &self.network), endpoint.endpoint)
                };
                if let Some(response) = response {
                    socket.send_slice(
                        &response,
                        IpEndpoint {
                            addr: remote.addr,
                            port: remote.port,
                        },
                    )?;
                }
            }
            Ok(())
        }

        fn pump_tcp(&mut self) -> anyhow::Result<Vec<tcp::TcpPumpEvent>> {
            let mut events = Vec::new();
            for slot in &mut self.tcp_slots {
                let socket = self.sockets.get_mut::<smol_tcp::Socket>(slot.handle);
                if socket.state() == smol_tcp::State::Established && slot.stream_id.is_none() {
                    self.next_tcp_stream += 1;
                    let stream_id = format!("tcp-{}", self.next_tcp_stream);
                    slot.stream_id = Some(stream_id.clone());
                    if slot.port == HTTPS_PORT {
                        events.push(tcp::TcpPumpEvent::Connected {
                            stream_id,
                            service_id: slot.service_id.clone(),
                        });
                    }
                }

                let Some(stream_id) = slot.stream_id.clone() else {
                    continue;
                };

                if slot.port == HTTP_PORT {
                    tcp::redirect_http_to_https(socket, &slot.hostname)?;
                } else {
                    events.extend(tcp::poll_socket(
                        socket,
                        &self.network,
                        &stream_id,
                        &slot.service_id,
                    )?);
                }

                if !socket.is_active() && !socket.may_recv() {
                    events.push(tcp::TcpPumpEvent::Closed {
                        stream_id,
                        service_id: slot.service_id.clone(),
                    });
                    slot.stream_id = None;
                }
            }
            Ok(events)
        }
    }
}

pub mod dns {
    use super::*;

    const TYPE_A: u16 = 1;
    const CLASS_IN: u16 = 1;
    const FLAG_RESPONSE_NO_ERROR: u16 = 0x8180;
    const FLAG_RESPONSE_NAME_ERROR: u16 = 0x8183;
    const TTL_SECONDS: u32 = 30;

    pub fn answer_query(packet: &[u8], network: &VirtualNetwork) -> Option<Vec<u8>> {
        let query = parse_query(packet)?;
        let addr = network
            .resolve_hostname(&query.name)
            .filter(|_| query.qtype == TYPE_A && query.qclass == CLASS_IN)
            .map(|service| service.addr);
        Some(emit_response(packet, &query, addr))
    }

    fn emit_response(request: &[u8], query: &DnsQuery, addr: Option<Ipv4Address>) -> Vec<u8> {
        let question_end = query.question_end;
        let mut response = Vec::with_capacity(question_end + if addr.is_some() { 16 } else { 0 });
        response.extend_from_slice(&request[..question_end]);
        response[2..4].copy_from_slice(
            &(if addr.is_some() {
                FLAG_RESPONSE_NO_ERROR
            } else {
                FLAG_RESPONSE_NAME_ERROR
            })
            .to_be_bytes(),
        );
        response[6..8].copy_from_slice(&(u16::from(addr.is_some())).to_be_bytes());
        response[8..10].copy_from_slice(&0_u16.to_be_bytes());
        response[10..12].copy_from_slice(&0_u16.to_be_bytes());

        if let Some(addr) = addr {
            response.extend_from_slice(&[0xc0, 0x0c]);
            response.extend_from_slice(&TYPE_A.to_be_bytes());
            response.extend_from_slice(&CLASS_IN.to_be_bytes());
            response.extend_from_slice(&TTL_SECONDS.to_be_bytes());
            response.extend_from_slice(&4_u16.to_be_bytes());
            response.extend_from_slice(&addr.octets());
        }

        response
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DnsQuery {
        name: String,
        qtype: u16,
        qclass: u16,
        question_end: usize,
    }

    fn parse_query(packet: &[u8]) -> Option<DnsQuery> {
        if packet.len() < 12 || u16::from_be_bytes([packet[4], packet[5]]) != 1 {
            return None;
        }
        let mut offset = 12;
        let mut labels = Vec::new();
        loop {
            let len = *packet.get(offset)? as usize;
            offset += 1;
            if len == 0 {
                break;
            }
            if len > 63 || offset + len > packet.len() {
                return None;
            }
            labels.push(std::str::from_utf8(&packet[offset..offset + len]).ok()?);
            offset += len;
        }
        if offset + 4 > packet.len() {
            return None;
        }
        let qtype = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let qclass = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
        Some(DnsQuery {
            name: labels.join(".").to_ascii_lowercase(),
            qtype,
            qclass,
            question_end: offset + 4,
        })
    }

    #[cfg(test)]
    pub(crate) fn query_a(transaction_id: u16, name: &str) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&transaction_id.to_be_bytes());
        packet.extend_from_slice(&0x0100_u16.to_be_bytes());
        packet.extend_from_slice(&1_u16.to_be_bytes());
        packet.extend_from_slice(&0_u16.to_be_bytes());
        packet.extend_from_slice(&0_u16.to_be_bytes());
        packet.extend_from_slice(&0_u16.to_be_bytes());
        for label in name.trim_end_matches('.').split('.') {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label.as_bytes());
        }
        packet.push(0);
        packet.extend_from_slice(&TYPE_A.to_be_bytes());
        packet.extend_from_slice(&CLASS_IN.to_be_bytes());
        packet
    }
}

pub mod tcp {
    use super::*;
    use smoltcp::socket::tcp;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum TcpPumpEvent {
        Connected {
            stream_id: String,
            service_id: String,
        },
        Received {
            stream_id: String,
            service_id: String,
            bytes: Vec<u8>,
        },
        Closed {
            stream_id: String,
            service_id: String,
        },
    }

    pub fn ensure_listening(socket: &mut tcp::Socket<'_>, port: u16) -> anyhow::Result<()> {
        if !socket.is_open() {
            socket.listen(port)?;
        }
        Ok(())
    }

    pub fn poll_socket(
        socket: &mut tcp::Socket<'_>,
        network: &VirtualNetwork,
        stream_id: &str,
        fallback_service_id: &str,
    ) -> anyhow::Result<Vec<TcpPumpEvent>> {
        let mut events = Vec::new();
        let service_id = socket
            .local_endpoint()
            .and_then(|endpoint| match endpoint.addr {
                smoltcp::wire::IpAddress::Ipv4(addr) => network.service_for_addr(addr),
                _ => None,
            })
            .map(|service| service.id.clone())
            .unwrap_or_else(|| fallback_service_id.to_string());

        if socket.can_recv() {
            let bytes = socket.recv(|buffer| (buffer.len(), buffer.to_vec()))?;
            events.push(TcpPumpEvent::Received {
                stream_id: stream_id.to_string(),
                service_id,
                bytes,
            });
        }

        Ok(events)
    }

    pub fn send(socket: &mut tcp::Socket<'_>, bytes: &[u8]) -> anyhow::Result<usize> {
        Ok(socket.send_slice(bytes)?)
    }

    pub fn redirect_http_to_https(
        socket: &mut tcp::Socket<'_>,
        hostname: &str,
    ) -> anyhow::Result<()> {
        if !socket.can_recv() {
            return Ok(());
        }
        let _ = socket.recv(|buffer| (buffer.len(), ()))?;
        if socket.can_send() {
            let response = format!(
                "HTTP/1.1 308 Permanent Redirect\r\nLocation: https://{hostname}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = socket.send_slice(response.as_bytes())?;
        }
        socket.close();
        Ok(())
    }
}

pub mod tun {
    use super::*;
    use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
    use smoltcp::time::Instant;

    #[derive(Debug, Default)]
    pub struct PacketDevice {
        rx: VecDeque<Vec<u8>>,
        tx: VecDeque<Vec<u8>>,
        mtu: usize,
    }

    impl PacketDevice {
        pub fn new(mtu: usize) -> Self {
            Self {
                rx: VecDeque::new(),
                tx: VecDeque::new(),
                mtu,
            }
        }

        pub fn push_tun_packet(&mut self, packet: Vec<u8>) {
            self.rx.push_back(packet);
        }

        pub fn pop_tun_packet(&mut self) -> Option<Vec<u8>> {
            self.tx.pop_front()
        }
    }

    impl Device for PacketDevice {
        type RxToken<'a>
            = TunRxToken
        where
            Self: 'a;
        type TxToken<'a>
            = TunTxToken<'a>
        where
            Self: 'a;

        fn receive(
            &mut self,
            _timestamp: Instant,
        ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
            self.rx
                .pop_front()
                .map(|packet| (TunRxToken { packet }, TunTxToken { tx: &mut self.tx }))
        }

        fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
            Some(TunTxToken { tx: &mut self.tx })
        }

        fn capabilities(&self) -> DeviceCapabilities {
            let mut capabilities = DeviceCapabilities::default();
            capabilities.medium = Medium::Ip;
            capabilities.max_transmission_unit = self.mtu;
            capabilities
        }
    }

    pub struct TunRxToken {
        packet: Vec<u8>,
    }

    impl RxToken for TunRxToken {
        fn consume<R, F>(self, f: F) -> R
        where
            F: FnOnce(&[u8]) -> R,
        {
            f(&self.packet)
        }
    }

    pub struct TunTxToken<'a> {
        tx: &'a mut VecDeque<Vec<u8>>,
    }

    impl TxToken for TunTxToken<'_> {
        fn consume<R, F>(self, len: usize, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            let mut packet = vec![0_u8; len];
            let result = f(&mut packet);
            self.tx.push_back(packet);
            result
        }
    }
}

const DEFAULT_SERVICE_BASE: [u8; 4] = [10, 88, 0, 10];
const DEFAULT_DNS_ADDR: [u8; 4] = [10, 88, 0, 1];
const DEFAULT_CLIENT_ADDR: [u8; 4] = [10, 88, 0, 2];
const DEFAULT_PREFIX_LEN: u8 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedService {
    pub id: String,
    pub label: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualService {
    pub id: String,
    pub hostname: String,
    pub addr: Ipv4Address,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct VirtualNetwork {
    client_addr: Ipv4Address,
    dns_addr: Ipv4Address,
    prefix_len: u8,
    services: Vec<VirtualService>,
    by_hostname: BTreeMap<String, usize>,
    by_addr: BTreeMap<Ipv4Address, usize>,
}

impl VirtualNetwork {
    pub fn new(services: &[PublishedService]) -> anyhow::Result<Self> {
        let client_addr = ipv4(DEFAULT_CLIENT_ADDR);
        let dns_addr = ipv4(DEFAULT_DNS_ADDR);
        let prefix_len = DEFAULT_PREFIX_LEN;
        let mut allocated = Vec::with_capacity(services.len());
        let mut by_hostname = BTreeMap::new();
        let mut by_addr = BTreeMap::new();

        for (index, service) in services.iter().enumerate() {
            if index > 200 {
                anyhow::bail!("too many services for default Medium virtual subnet");
            }
            let hostname = service_hostname(service.label.as_deref().unwrap_or(&service.id));
            let addr = service_addr(index)?;
            let virtual_service = VirtualService {
                id: service.id.clone(),
                hostname: hostname.clone(),
                addr,
                kind: service.kind.clone(),
            };
            by_hostname.insert(hostname, allocated.len());
            by_addr.insert(addr, allocated.len());
            allocated.push(virtual_service);
        }

        Ok(Self {
            client_addr,
            dns_addr,
            prefix_len,
            services: allocated,
            by_hostname,
            by_addr,
        })
    }

    pub fn interface_cidr(&self) -> IpCidr {
        IpCidr::new(self.client_addr.into(), self.prefix_len)
    }

    pub fn dns_addr(&self) -> Ipv4Address {
        self.dns_addr
    }

    pub fn prefix_len(&self) -> u8 {
        self.prefix_len
    }

    pub fn services(&self) -> &[VirtualService] {
        &self.services
    }

    pub fn resolve_hostname(&self, hostname: &str) -> Option<&VirtualService> {
        let hostname = hostname.trim_end_matches('.').to_ascii_lowercase();
        self.by_hostname
            .get(&hostname)
            .and_then(|index| self.services.get(*index))
    }

    pub fn service_for_addr(&self, addr: Ipv4Address) -> Option<&VirtualService> {
        self.by_addr
            .get(&addr)
            .and_then(|index| self.services.get(*index))
    }
}

pub fn service_hostname(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("service");
    }
    format!("{out}.medium")
}

fn service_addr(index: usize) -> anyhow::Result<Ipv4Address> {
    let mut bytes = DEFAULT_SERVICE_BASE;
    let host = u16::from(bytes[3]) + u16::try_from(index)?;
    if host > 254 {
        anyhow::bail!("service address outside default Medium virtual subnet");
    }
    bytes[3] = host as u8;
    Ok(ipv4(bytes))
}

fn ipv4(bytes: [u8; 4]) -> Ipv4Address {
    Ipv4Address::new(bytes[0], bytes[1], bytes[2], bytes[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_service_hostname() {
        assert_eq!(service_hostname("Hello World"), "hello-world.medium");
        assert_eq!(service_hostname("svc_openclaw"), "svc-openclaw.medium");
        assert_eq!(service_hostname("  "), "service.medium");
    }

    #[test]
    fn maps_services_to_stable_virtual_addresses() {
        let network = VirtualNetwork::new(&[
            PublishedService {
                id: "hello".into(),
                label: None,
                kind: "https".into(),
            },
            PublishedService {
                id: "svc_docs".into(),
                label: Some("Docs".into()),
                kind: "https".into(),
            },
        ])
        .unwrap();

        assert_eq!(
            network.interface_cidr(),
            IpCidr::new(Ipv4Address::new(10, 88, 0, 2).into(), 24)
        );
        assert_eq!(network.dns_addr(), Ipv4Address::new(10, 88, 0, 1));
        assert_eq!(network.services()[0].hostname, "hello.medium");
        assert_eq!(network.services()[0].addr, Ipv4Address::new(10, 88, 0, 10));
        assert_eq!(network.services()[1].hostname, "docs.medium");
        assert_eq!(
            network.resolve_hostname("docs.medium.").unwrap().id,
            "svc_docs"
        );
        assert_eq!(
            network
                .service_for_addr(Ipv4Address::new(10, 88, 0, 10))
                .unwrap()
                .id,
            "hello"
        );
    }

    #[test]
    fn packet_device_bridges_tun_packets_to_smoltcp_device() {
        use smoltcp::phy::{Device, RxToken, TxToken};
        use smoltcp::time::Instant;

        let mut device = tun::PacketDevice::new(1500);
        assert_eq!(device.capabilities().max_transmission_unit, 1500);

        device.push_tun_packet(vec![0x45, 0x00, 0x00, 0x14]);
        let (rx, tx) = device.receive(Instant::ZERO).expect("rx packet");
        let observed = rx.consume(|packet| packet.to_vec());
        assert_eq!(observed, vec![0x45, 0x00, 0x00, 0x14]);

        tx.consume(4, |packet| {
            packet.copy_from_slice(&[0x45, 0x00, 0x00, 0x15])
        });
        assert_eq!(device.pop_tun_packet(), Some(vec![0x45, 0x00, 0x00, 0x15]));
    }

    #[test]
    fn dns_answers_medium_hostname_with_virtual_service_addr() {
        let network = VirtualNetwork::new(&[PublishedService {
            id: "hello".into(),
            label: None,
            kind: "https".into(),
        }])
        .unwrap();
        let response = dns::answer_query(&dns::query_a(0x1234, "hello.medium"), &network)
            .expect("dns response");

        assert_eq!(&response[0..2], &0x1234_u16.to_be_bytes());
        assert_eq!(&response[2..4], &0x8180_u16.to_be_bytes());
        assert_eq!(&response[6..8], &1_u16.to_be_bytes());
        assert_eq!(&response[response.len() - 4..], &[10, 88, 0, 10]);
    }

    #[test]
    fn dns_returns_name_error_for_unknown_medium_hostname() {
        let network = VirtualNetwork::new(&[]).unwrap();
        let response = dns::answer_query(&dns::query_a(0x1234, "missing.medium"), &network)
            .expect("dns response");

        assert_eq!(&response[2..4], &0x8183_u16.to_be_bytes());
        assert_eq!(&response[6..8], &0_u16.to_be_bytes());
    }

    #[test]
    fn tcp_pump_opens_listening_socket() {
        let rx = smoltcp::socket::tcp::SocketBuffer::new(vec![0; 4096]);
        let tx = smoltcp::socket::tcp::SocketBuffer::new(vec![0; 4096]);
        let mut socket = smoltcp::socket::tcp::Socket::new(rx, tx);

        tcp::ensure_listening(&mut socket, 80).unwrap();

        assert!(socket.is_open());
        assert_eq!(socket.listen_endpoint().port, 80);
    }

    #[test]
    fn stack_answers_dns_query_from_tun_packet() {
        let network = VirtualNetwork::new(&[PublishedService {
            id: "hello".into(),
            label: None,
            kind: "https".into(),
        }])
        .unwrap();
        let query = dns::query_a(0x5678, "hello.medium");
        let mut stack = stack::MediumStack::new(network).unwrap();

        stack.push_tun_packet(udp_ipv4_packet(
            Ipv4Address::new(10, 88, 0, 99),
            Ipv4Address::new(10, 88, 0, 1),
            5353,
            53,
            &query,
        ));
        stack.poll(0).unwrap();
        let response = stack.pop_tun_packet().expect("dns response packet");
        let payload = udp_payload(&response);

        assert_eq!(&payload[0..2], &0x5678_u16.to_be_bytes());
        assert_eq!(&payload[2..4], &0x8180_u16.to_be_bytes());
        assert_eq!(&payload[payload.len() - 4..], &[10, 88, 0, 10]);
    }

    #[test]
    fn stack_emits_tcp_stream_events_for_service_connection() {
        use smoltcp::wire::{TcpControl, TcpSeqNumber};

        let network = VirtualNetwork::new(&[PublishedService {
            id: "hello".into(),
            label: None,
            kind: "http".into(),
        }])
        .unwrap();
        let mut stack = stack::MediumStack::new(network).unwrap();
        let client = Ipv4Address::new(10, 88, 0, 99);
        let service = Ipv4Address::new(10, 88, 0, 10);
        let client_port = 40000;
        let client_seq = TcpSeqNumber(1000);

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            client_port,
            443,
            client_seq,
            None,
            TcpControl::Syn,
            &[],
        ));
        let events = stack.poll(0).unwrap();
        assert!(events.is_empty());
        let syn_ack = stack.pop_tun_packet().expect("syn ack");
        let syn_ack = tcp_repr(&syn_ack);
        assert_eq!(syn_ack.control, TcpControl::Syn);
        assert_eq!(syn_ack.ack_number, Some(TcpSeqNumber(client_seq.0 + 1)));

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            client_port,
            443,
            TcpSeqNumber(client_seq.0 + 1),
            Some(TcpSeqNumber(syn_ack.seq_number.0 + 1)),
            TcpControl::None,
            &[],
        ));
        let events = stack.poll(1).unwrap();
        assert_eq!(
            events,
            vec![tcp::TcpPumpEvent::Connected {
                stream_id: "tcp-1".into(),
                service_id: "hello".into(),
            }]
        );

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            client_port,
            443,
            TcpSeqNumber(client_seq.0 + 1),
            Some(TcpSeqNumber(syn_ack.seq_number.0 + 1)),
            TcpControl::Psh,
            b"GET / HTTP/1.1\r\n\r\n",
        ));
        let events = stack.poll(2).unwrap();
        assert_eq!(
            events,
            vec![tcp::TcpPumpEvent::Received {
                stream_id: "tcp-1".into(),
                service_id: "hello".into(),
                bytes: b"GET / HTTP/1.1\r\n\r\n".to_vec(),
            }]
        );
    }

    #[test]
    fn stack_keeps_listening_after_first_https_connection_is_open() {
        use smoltcp::wire::{TcpControl, TcpSeqNumber};

        let network = VirtualNetwork::new(&[PublishedService {
            id: "hello".into(),
            label: None,
            kind: "http".into(),
        }])
        .unwrap();
        let mut stack = stack::MediumStack::new(network).unwrap();
        let client = Ipv4Address::new(10, 88, 0, 99);
        let service = Ipv4Address::new(10, 88, 0, 10);

        let first_syn_ack =
            establish_connection(&mut stack, client, service, 40000, TcpSeqNumber(1000), 0);
        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            40000,
            443,
            TcpSeqNumber(1001),
            Some(TcpSeqNumber(first_syn_ack.seq_number.0 + 1)),
            TcpControl::Psh,
            b"client hello",
        ));
        let first_events = stack.poll(2).unwrap();
        assert_eq!(
            first_events,
            vec![tcp::TcpPumpEvent::Received {
                stream_id: "tcp-1".into(),
                service_id: "hello".into(),
                bytes: b"client hello".to_vec(),
            }]
        );

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            40001,
            443,
            TcpSeqNumber(2000),
            None,
            TcpControl::Syn,
            &[],
        ));
        stack.poll(3).unwrap();
        let second_syn_ack = stack.pop_tun_packet().expect("second syn ack");
        let second_syn_ack = tcp_repr(&second_syn_ack);
        assert_eq!(second_syn_ack.control, TcpControl::Syn);
        assert_eq!(second_syn_ack.ack_number, Some(TcpSeqNumber(2001)));
    }

    #[test]
    fn stack_redirects_http_port_to_https_service_url() {
        use smoltcp::wire::{TcpControl, TcpSeqNumber};

        let network = VirtualNetwork::new(&[PublishedService {
            id: "hello".into(),
            label: None,
            kind: "http".into(),
        }])
        .unwrap();
        let mut stack = stack::MediumStack::new(network).unwrap();
        let client = Ipv4Address::new(10, 88, 0, 99);
        let service = Ipv4Address::new(10, 88, 0, 10);
        let client_port = 40000;
        let client_seq = TcpSeqNumber(1000);

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            client_port,
            80,
            client_seq,
            None,
            TcpControl::Syn,
            &[],
        ));
        stack.poll(0).unwrap();
        let syn_ack = stack.pop_tun_packet().expect("syn ack");
        let syn_ack = tcp_repr(&syn_ack);

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            client_port,
            80,
            TcpSeqNumber(client_seq.0 + 1),
            Some(TcpSeqNumber(syn_ack.seq_number.0 + 1)),
            TcpControl::Psh,
            b"GET / HTTP/1.1\r\nHost: hello.medium\r\n\r\n",
        ));
        let events = stack.poll(1).unwrap();
        assert!(events.is_empty());

        let mut payload = Vec::new();
        while let Some(packet) = stack.pop_tun_packet() {
            payload.extend_from_slice(tcp_repr(&packet).payload);
        }
        let response = String::from_utf8_lossy(&payload);
        assert!(response.contains("HTTP/1.1 308 Permanent Redirect"));
        assert!(response.contains("Location: https://hello.medium/"));
    }

    fn udp_ipv4_packet(
        src_addr: Ipv4Address,
        dst_addr: Ipv4Address,
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) -> Vec<u8> {
        use smoltcp::phy::ChecksumCapabilities;
        use smoltcp::wire::{IpProtocol, Ipv4Packet, Ipv4Repr, UdpPacket, UdpRepr};

        let udp = UdpRepr { src_port, dst_port };
        let ip = Ipv4Repr {
            src_addr,
            dst_addr,
            next_header: IpProtocol::Udp,
            payload_len: udp.header_len() + payload.len(),
            hop_limit: 64,
        };
        let mut packet = vec![0; ip.buffer_len() + udp.header_len() + payload.len()];
        ip.emit(
            &mut Ipv4Packet::new_unchecked(&mut packet),
            &ChecksumCapabilities::default(),
        );
        udp.emit(
            &mut UdpPacket::new_unchecked(&mut packet[ip.buffer_len()..]),
            &src_addr.into(),
            &dst_addr.into(),
            payload.len(),
            |buffer| buffer.copy_from_slice(payload),
            &ChecksumCapabilities::default(),
        );
        packet
    }

    fn udp_payload(packet: &[u8]) -> Vec<u8> {
        use smoltcp::phy::ChecksumCapabilities;
        use smoltcp::wire::{Ipv4Packet, Ipv4Repr, UdpPacket, UdpRepr};

        let ipv4 = Ipv4Packet::new_checked(packet).expect("ipv4 packet");
        let ipv4_repr =
            Ipv4Repr::parse(&ipv4, &ChecksumCapabilities::default()).expect("ipv4 repr");
        let udp_offset = ipv4_repr.buffer_len();
        let udp = UdpPacket::new_checked(&packet[udp_offset..]).expect("udp packet");
        UdpRepr::parse(
            &udp,
            &ipv4_repr.src_addr.into(),
            &ipv4_repr.dst_addr.into(),
            &ChecksumCapabilities::default(),
        )
        .expect("udp repr");
        udp.payload().to_vec()
    }

    fn tcp_ipv4_packet(
        src_addr: Ipv4Address,
        dst_addr: Ipv4Address,
        src_port: u16,
        dst_port: u16,
        seq_number: smoltcp::wire::TcpSeqNumber,
        ack_number: Option<smoltcp::wire::TcpSeqNumber>,
        control: smoltcp::wire::TcpControl,
        payload: &[u8],
    ) -> Vec<u8> {
        use smoltcp::phy::ChecksumCapabilities;
        use smoltcp::wire::{IpProtocol, Ipv4Packet, Ipv4Repr, TcpPacket, TcpRepr};

        let tcp = TcpRepr {
            src_port,
            dst_port,
            control,
            seq_number,
            ack_number,
            window_len: 4096,
            window_scale: None,
            max_seg_size: None,
            sack_permitted: false,
            sack_ranges: [None, None, None],
            timestamp: None,
            payload,
        };
        let ip = Ipv4Repr {
            src_addr,
            dst_addr,
            next_header: IpProtocol::Tcp,
            payload_len: tcp.buffer_len(),
            hop_limit: 64,
        };
        let mut packet = vec![0; ip.buffer_len() + tcp.buffer_len()];
        ip.emit(
            &mut Ipv4Packet::new_unchecked(&mut packet),
            &ChecksumCapabilities::default(),
        );
        tcp.emit(
            &mut TcpPacket::new_unchecked(&mut packet[ip.buffer_len()..]),
            &src_addr.into(),
            &dst_addr.into(),
            &ChecksumCapabilities::default(),
        );
        packet
    }

    fn tcp_repr(packet: &[u8]) -> smoltcp::wire::TcpRepr<'_> {
        use smoltcp::phy::ChecksumCapabilities;
        use smoltcp::wire::{Ipv4Packet, Ipv4Repr, TcpPacket, TcpRepr};

        let ipv4 = Ipv4Packet::new_checked(packet).expect("ipv4 packet");
        let ipv4_repr =
            Ipv4Repr::parse(&ipv4, &ChecksumCapabilities::default()).expect("ipv4 repr");
        let tcp_offset = ipv4_repr.buffer_len();
        let tcp = TcpPacket::new_checked(&packet[tcp_offset..]).expect("tcp packet");
        TcpRepr::parse(
            &tcp,
            &ipv4_repr.src_addr.into(),
            &ipv4_repr.dst_addr.into(),
            &ChecksumCapabilities::default(),
        )
        .expect("tcp repr")
    }

    fn establish_connection(
        stack: &mut stack::MediumStack<'_>,
        client: Ipv4Address,
        service: Ipv4Address,
        client_port: u16,
        client_seq: smoltcp::wire::TcpSeqNumber,
        now_millis: i64,
    ) -> smoltcp::wire::TcpRepr<'static> {
        use smoltcp::wire::TcpControl;

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            client_port,
            443,
            client_seq,
            None,
            TcpControl::Syn,
            &[],
        ));
        let events = stack.poll(now_millis).unwrap();
        assert!(events.is_empty());
        let syn_ack = stack.pop_tun_packet().expect("syn ack");
        let syn_ack = tcp_repr(&syn_ack);

        stack.push_tun_packet(tcp_ipv4_packet(
            client,
            service,
            client_port,
            443,
            smoltcp::wire::TcpSeqNumber(client_seq.0 + 1),
            Some(smoltcp::wire::TcpSeqNumber(syn_ack.seq_number.0 + 1)),
            TcpControl::None,
            &[],
        ));
        let events = stack.poll(now_millis + 1).unwrap();
        assert_eq!(
            events,
            vec![tcp::TcpPumpEvent::Connected {
                stream_id: format!("tcp-{}", client_port - 39999),
                service_id: "hello".into(),
            }]
        );

        smoltcp::wire::TcpRepr {
            src_port: syn_ack.src_port,
            dst_port: syn_ack.dst_port,
            control: syn_ack.control,
            seq_number: syn_ack.seq_number,
            ack_number: syn_ack.ack_number,
            window_len: syn_ack.window_len,
            window_scale: syn_ack.window_scale,
            max_seg_size: syn_ack.max_seg_size,
            sack_permitted: syn_ack.sack_permitted,
            sack_ranges: syn_ack.sack_ranges,
            timestamp: syn_ack.timestamp,
            payload: &[],
        }
    }
}
