import CryptoKit
import Darwin
import Foundation

enum ForegroundUdpSessionError: LocalizedError {
    case noIceCandidate
    case invalidAddress(String)
    case socket(String)
    case handshakeTimeout(String)
    case invalidPacket
    case cryptoFailed

    var errorDescription: String? {
        switch self {
        case .noIceCandidate:
            return "Session grant has no UDP ICE candidate usable from iOS foreground mode."
        case .invalidAddress(let address):
            return "Invalid UDP candidate address: \(address)."
        case .socket(let message):
            return "UDP socket error: \(message)."
        case .handshakeTimeout(let peer):
            return "UDP session handshake timed out for \(peer)."
        case .invalidPacket:
            return "Invalid UDP session packet."
        case .cryptoFailed:
            return "UDP session crypto failed."
        }
    }
}

struct ForegroundUdpCandidateSelection {
    let candidate: IceCandidate
    let addr: String
}

final class ForegroundUdpSession {
    private enum PacketKind: UInt8 {
        case hello = 1
        case helloAck = 2
        case data = 3
        case ack = 4
        case close = 5
    }

    private struct Packet {
        let kind: PacketKind
        let seq: UInt64
        let payload: Data
    }

    private enum Direction: UInt8 {
        case clientToNode = 0
        case nodeToClient = 1
    }

    private static let magic = Data([0x4d, 0x44, 0x55, 0x31])
    private static let maxPayload = 1_100
    private static let retries = 5
    private static let handshakeDeadlineSeconds = 8.0
    private static let punchBurstPackets = 5
    private static let punchBurstIntervalMicros: useconds_t = 40_000

    private let fd: Int32
    private var peer: sockaddr_storage
    private var peerLen: socklen_t
    private let key: SymmetricKey
    private var sendSeq: UInt64 = 0
    private var recvSeq: UInt64 = 0
    private var pendingRead = Data()
    private let lock = NSLock()

    private init(fd: Int32, peer: sockaddr_storage, peerLen: socklen_t, token: String) {
        self.fd = fd
        self.peer = peer
        self.peerLen = peerLen
        self.key = SymmetricKey(data: Data(SHA256.hash(data: Data(token.utf8))))
    }

    deinit {
        close()
    }

    static func bestIceCandidate(in grant: SessionOpenGrant, preferred: IceCandidate? = nil) -> ForegroundUdpCandidateSelection? {
        iceChecklist(in: grant, preferred: preferred).first
    }

    static func iceChecklist(in grant: SessionOpenGrant, preferred: IceCandidate? = nil) -> [ForegroundUdpCandidateSelection] {
        guard let ice = grant.authorization.ice else {
            return []
        }
        return ice.candidates
            .filter { $0.transport.lowercased() == "udp" }
            .filter { !isUnusableCandidateAddress($0.addr) }
            .sorted { left, right in
                let leftPreferred = sameIceCandidate(left, preferred)
                let rightPreferred = sameIceCandidate(right, preferred)
                if leftPreferred != rightPreferred {
                    return leftPreferred
                }
                let leftRank = iceKindRank(left.kind)
                let rightRank = iceKindRank(right.kind)
                if leftRank != rightRank {
                    return leftRank < rightRank
                }
                if left.priority != right.priority {
                    return left.priority > right.priority
                }
                return left.foundation < right.foundation
            }
            .map { ForegroundUdpCandidateSelection(candidate: $0, addr: "\($0.addr):\($0.port)") }
    }

    static func connect(grant: SessionOpenGrant, selection: ForegroundUdpCandidateSelection) throws -> ForegroundUdpSession {
        let relayOrPeer = try resolveAddress(selection.addr)
        let fd = socket(relayOrPeer.storage.ss_family == sa_family_t(AF_INET6) ? AF_INET6 : AF_INET, SOCK_DGRAM, IPPROTO_UDP)
        guard fd >= 0 else {
            throw ForegroundUdpSessionError.socket(String(cString: strerror(errno)))
        }
        try setTimeout(fd, millis: 500)

        do {
            let peer: (sockaddr_storage, socklen_t)
            if selection.candidate.kind == .relay {
                peer = try resolvePeer(fd: fd, relay: relayOrPeer, grant: grant)
            } else {
                peer = (relayOrPeer.storage, relayOrPeer.length)
            }
            let session = ForegroundUdpSession(fd: fd, peer: peer.0, peerLen: peer.1, token: grant.authorization.token)
            try session.sendSessionHello(grant: grant)
            return session
        } catch {
            Darwin.close(fd)
            throw error
        }
    }

    func write(_ bytes: Data) throws {
        var offset = 0
        while offset < bytes.count {
            let size = min(Self.maxPayload, bytes.count - offset)
            let chunk = bytes.subdata(in: offset..<(offset + size))
            try writeChunk(chunk)
            offset += size
        }
    }

    func read(maxLength: Int = 64 * 1024) throws -> Data {
        if let pending = popPendingRead(maxLength: maxLength) {
            return pending
        }

        while true {
            let (packet, _) = try receivePacket()
            switch packet.kind {
            case .data:
                let plaintext = try decrypt(packet.payload, direction: .nodeToClient, seq: packet.seq)
                try sendPacket(Packet(kind: .ack, seq: packet.seq, payload: Data()))
                lock.lock()
                if packet.seq == recvSeq {
                    recvSeq += 1
                    pendingRead.append(plaintext)
                    let output = popPendingReadLocked(maxLength: maxLength)
                    lock.unlock()
                    return output
                }
                lock.unlock()
            case .close:
                return Data()
            case .ack, .hello, .helloAck:
                continue
            }
        }
    }

    func close() {
        _ = try? sendPacket(Packet(kind: .close, seq: sendSeq, payload: Data()))
        Darwin.close(fd)
    }

    private func sendSessionHello(grant: SessionOpenGrant) throws {
        print("Medium foreground UDP session hello for \(grant.serviceID)")
        let hello = try JSONSerialization.data(withJSONObject: [
            "token": grant.authorization.token,
            "service_id": grant.serviceID
        ])
        let packet = Packet(kind: .hello, seq: 0, payload: hello)
        let deadline = Date().addingTimeInterval(Self.handshakeDeadlineSeconds)
        while Date() < deadline {
            try sendPacket(packet)
            do {
                let (response, addr) = try receivePacket()
                if response.kind == .helloAck, response.seq == 0, Self.sameHost(addr, peer) {
                    peer = addr
                    peerLen = Self.sockaddrLength(addr)
                    return
                }
            } catch ForegroundUdpSessionError.socket {
                continue
            }
        }
        throw ForegroundUdpSessionError.handshakeTimeout(Self.describe(peer))
    }

    private func writeChunk(_ chunk: Data) throws {
        lock.lock()
        let seq = sendSeq
        lock.unlock()

        let packet = Packet(kind: .data, seq: seq, payload: try encrypt(chunk, direction: .clientToNode, seq: seq))
        var lastError: Error?
        for _ in 0..<Self.retries {
            try sendPacket(packet)
            do {
                try waitForAck(seq)
                lock.lock()
                sendSeq += 1
                lock.unlock()
                return
            } catch {
                lastError = error
            }
        }
        throw lastError ?? ForegroundUdpSessionError.handshakeTimeout(Self.describe(peer))
    }

    private func popPendingRead(maxLength: Int) -> Data? {
        lock.lock()
        defer { lock.unlock() }
        guard !pendingRead.isEmpty else {
            return nil
        }
        return popPendingReadLocked(maxLength: maxLength)
    }

    private func popPendingReadLocked(maxLength: Int) -> Data {
        let size = min(maxLength, pendingRead.count)
        let output = pendingRead.prefix(size)
        pendingRead.removeFirst(size)
        return Data(output)
    }

    private func waitForAck(_ seq: UInt64) throws {
        while true {
            let (packet, _) = try receivePacket()
            switch packet.kind {
            case .ack where packet.seq == seq:
                return
            case .data:
                let plaintext = try decrypt(packet.payload, direction: .nodeToClient, seq: packet.seq)
                try sendPacket(Packet(kind: .ack, seq: packet.seq, payload: Data()))
                lock.lock()
                pendingRead.append(plaintext)
                lock.unlock()
            case .close:
                throw ForegroundUdpSessionError.socket("remote closed UDP session")
            default:
                continue
            }
        }
    }

    private func encrypt(_ bytes: Data, direction: Direction, seq: UInt64) throws -> Data {
        do {
            let sealed = try ChaChaPoly.seal(bytes, using: key, nonce: nonce(direction: direction, seq: seq))
            return sealed.ciphertext + sealed.tag
        } catch {
            throw ForegroundUdpSessionError.cryptoFailed
        }
    }

    private func decrypt(_ bytes: Data, direction: Direction, seq: UInt64) throws -> Data {
        guard bytes.count >= 16 else {
            throw ForegroundUdpSessionError.cryptoFailed
        }
        do {
            let ciphertext = bytes.prefix(bytes.count - 16)
            let tag = bytes.suffix(16)
            let box = try ChaChaPoly.SealedBox(nonce: nonce(direction: direction, seq: seq), ciphertext: ciphertext, tag: tag)
            return try ChaChaPoly.open(box, using: key)
        } catch {
            throw ForegroundUdpSessionError.cryptoFailed
        }
    }

    private func nonce(direction: Direction, seq: UInt64) -> ChaChaPoly.Nonce {
        var bytes = [UInt8](repeating: 0, count: 12)
        bytes[0] = direction.rawValue
        var bigEndian = seq.bigEndian
        withUnsafeBytes(of: &bigEndian) { raw in
            bytes.replaceSubrange(4..<12, with: raw)
        }
        return try! ChaChaPoly.Nonce(data: bytes)
    }

    private func sendPacket(_ packet: Packet) throws {
        let encoded = try Self.encode(packet)
        try withUnsafePointer(to: &peer) { pointer in
            try pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                let sent = encoded.withUnsafeBytes { buffer in
                    Darwin.sendto(fd, buffer.baseAddress, encoded.count, 0, sockaddrPointer, peerLen)
                }
                if sent < 0 {
                    throw ForegroundUdpSessionError.socket(String(cString: strerror(errno)))
                }
            }
        }
    }

    private func receivePacket() throws -> (Packet, sockaddr_storage) {
        var buffer = [UInt8](repeating: 0, count: 1_500)
        var addr = sockaddr_storage()
        var length = socklen_t(MemoryLayout<sockaddr_storage>.size)
        let size = withUnsafeMutablePointer(to: &addr) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                Darwin.recvfrom(fd, &buffer, buffer.count, 0, sockaddrPointer, &length)
            }
        }
        guard size >= 0 else {
            throw ForegroundUdpSessionError.socket(String(cString: strerror(errno)))
        }
        return (try Self.decode(Data(buffer.prefix(size))), addr)
    }

    private static func encode(_ packet: Packet) throws -> Data {
        guard packet.payload.count <= UInt16.max else {
            throw ForegroundUdpSessionError.invalidPacket
        }
        var output = magic
        output.append(packet.kind.rawValue)
        var seq = packet.seq.bigEndian
        withUnsafeBytes(of: &seq) { output.append(contentsOf: $0) }
        var len = UInt16(packet.payload.count).bigEndian
        withUnsafeBytes(of: &len) { output.append(contentsOf: $0) }
        output.append(packet.payload)
        return output
    }

    private static func decode(_ bytes: Data) throws -> Packet {
        guard bytes.count >= 15, bytes.prefix(4) == magic else {
            throw ForegroundUdpSessionError.invalidPacket
        }
        guard let kind = PacketKind(rawValue: bytes[4]) else {
            throw ForegroundUdpSessionError.invalidPacket
        }
        let seq = bytes[5..<13].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        let len = bytes[13..<15].reduce(UInt16(0)) { ($0 << 8) | UInt16($1) }
        guard bytes.count == 15 + Int(len) else {
            throw ForegroundUdpSessionError.invalidPacket
        }
        return Packet(kind: kind, seq: seq, payload: bytes.suffix(Int(len)))
    }

    private static func resolvePeer(fd: Int32, relay: (storage: sockaddr_storage, length: socklen_t), grant: SessionOpenGrant) throws -> (sockaddr_storage, socklen_t) {
        let message = try JSONSerialization.data(withJSONObject: [
            "role": "client",
            "node_id": grant.nodeID,
            "token": grant.authorization.token
        ])
        try sendRaw(fd: fd, data: message, addr: relay.storage, length: relay.length)

        var buffer = [UInt8](repeating: 0, count: 1_500)
        let deadline = Date().addingTimeInterval(2.5)
        while Date() < deadline {
            var addr = sockaddr_storage()
            var length = socklen_t(MemoryLayout<sockaddr_storage>.size)
            let size = withUnsafeMutablePointer(to: &addr) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                    Darwin.recvfrom(fd, &buffer, buffer.count, 0, sockaddrPointer, &length)
                }
            }
            if size <= 0 {
                continue
            }
            guard sameSocketAddress(addr, relay.storage),
                  let json = try JSONSerialization.jsonObject(with: Data(buffer.prefix(size))) as? [String: Any],
                  json["role"] as? String == "peer",
                  let peerAddr = json["addr"] as? String else {
                continue
            }
            let peer = try resolveAddress(peerAddr)
            try sendPunchBurst(fd: fd, peer: peer)
            return (peer.storage, peer.length)
        }
        throw ForegroundUdpSessionError.handshakeTimeout("rendezvous \(describe(relay.storage))")
    }

    private static func sendPunchBurst(fd: Int32, peer: (storage: sockaddr_storage, length: socklen_t)) throws {
        let punch = try JSONSerialization.data(withJSONObject: ["role": "punch"])
        for index in 0..<punchBurstPackets {
            try sendRaw(fd: fd, data: punch, addr: peer.storage, length: peer.length)
            if index + 1 < punchBurstPackets {
                usleep(punchBurstIntervalMicros)
            }
        }
    }

    private static func sendRaw(fd: Int32, data: Data, addr: sockaddr_storage, length: socklen_t) throws {
        var mutableAddr = addr
        try withUnsafePointer(to: &mutableAddr) { pointer in
            try pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                let sent = data.withUnsafeBytes { buffer in
                    Darwin.sendto(fd, buffer.baseAddress, data.count, 0, sockaddrPointer, length)
                }
                if sent < 0 {
                    throw ForegroundUdpSessionError.socket(String(cString: strerror(errno)))
                }
            }
        }
    }

    private static func resolveAddress(_ value: String) throws -> (storage: sockaddr_storage, length: socklen_t) {
        guard let separator = value.lastIndex(of: ":") else {
            throw ForegroundUdpSessionError.invalidAddress(value)
        }
        let host = String(value[..<separator]).trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        let port = String(value[value.index(after: separator)...])
        var hints = addrinfo(
            ai_flags: 0,
            ai_family: AF_UNSPEC,
            ai_socktype: SOCK_DGRAM,
            ai_protocol: IPPROTO_UDP,
            ai_addrlen: 0,
            ai_canonname: nil,
            ai_addr: nil,
            ai_next: nil
        )
        var result: UnsafeMutablePointer<addrinfo>?
        guard getaddrinfo(host, port, &hints, &result) == 0, let result else {
            throw ForegroundUdpSessionError.invalidAddress(value)
        }
        defer { freeaddrinfo(result) }
        var storage = sockaddr_storage()
        memcpy(&storage, result.pointee.ai_addr, Int(result.pointee.ai_addrlen))
        return (storage, result.pointee.ai_addrlen)
    }

    private static func setTimeout(_ fd: Int32, millis: Int) throws {
        var timeout = timeval(
            tv_sec: __darwin_time_t(millis / 1_000),
            tv_usec: __darwin_suseconds_t((millis % 1_000) * 1_000)
        )
        guard setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size)) == 0,
              setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size)) == 0 else {
            throw ForegroundUdpSessionError.socket(String(cString: strerror(errno)))
        }
    }

    private static func iceKindRank(_ kind: IceCandidateKind) -> Int {
        switch kind {
        case .host: return 0
        case .srflx: return 1
        case .relay: return 2
        }
    }

    private static func sameIceCandidate(_ left: IceCandidate, _ right: IceCandidate?) -> Bool {
        guard let right else {
            return false
        }
        return left.transport.lowercased() == right.transport.lowercased()
            && left.kind == right.kind
            && left.addr == right.addr
            && left.port == right.port
    }

    private static func isUnusableCandidateAddress(_ host: String) -> Bool {
        if host == "0.0.0.0" || host == "127.0.0.1" || host == "::1" {
            return true
        }
        let parts = host.split(separator: ".").compactMap { Int($0) }
        return parts.count == 4 && parts[0] == 198 && (parts[1] == 18 || parts[1] == 19)
    }

    private static func sameHost(_ left: sockaddr_storage, _ right: sockaddr_storage) -> Bool {
        if left.ss_family != right.ss_family {
            return false
        }
        if left.ss_family == sa_family_t(AF_INET) {
            return withSockaddrIn(left).sin_addr.s_addr == withSockaddrIn(right).sin_addr.s_addr
        }
        if left.ss_family == sa_family_t(AF_INET6) {
            var leftAddr = withSockaddrIn6(left).sin6_addr
            var rightAddr = withSockaddrIn6(right).sin6_addr
            return memcmp(&leftAddr, &rightAddr, MemoryLayout<in6_addr>.size) == 0
        }
        return false
    }

    private static func sameSocketAddress(_ left: sockaddr_storage, _ right: sockaddr_storage) -> Bool {
        sameHost(left, right) && port(left) == port(right)
    }

    private static func port(_ addr: sockaddr_storage) -> UInt16 {
        if addr.ss_family == sa_family_t(AF_INET) {
            return UInt16(bigEndian: withSockaddrIn(addr).sin_port)
        }
        if addr.ss_family == sa_family_t(AF_INET6) {
            return UInt16(bigEndian: withSockaddrIn6(addr).sin6_port)
        }
        return 0
    }

    private static func sockaddrLength(_ addr: sockaddr_storage) -> socklen_t {
        addr.ss_family == sa_family_t(AF_INET6) ? socklen_t(MemoryLayout<sockaddr_in6>.size) : socklen_t(MemoryLayout<sockaddr_in>.size)
    }

    private static func withSockaddrIn(_ storage: sockaddr_storage) -> sockaddr_in {
        var copy = storage
        return withUnsafePointer(to: &copy) {
            $0.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee }
        }
    }

    private static func withSockaddrIn6(_ storage: sockaddr_storage) -> sockaddr_in6 {
        var copy = storage
        return withUnsafePointer(to: &copy) {
            $0.withMemoryRebound(to: sockaddr_in6.self, capacity: 1) { $0.pointee }
        }
    }

    private static func describe(_ storage: sockaddr_storage) -> String {
        var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
        var service = [CChar](repeating: 0, count: Int(NI_MAXSERV))
        var copy = storage
        let length = sockaddrLength(storage)
        let result = withUnsafePointer(to: &copy) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getnameinfo($0, length, &host, socklen_t(host.count), &service, socklen_t(service.count), NI_NUMERICHOST | NI_NUMERICSERV)
            }
        }
        if result == 0 {
            return "\(String(cString: host)):\(String(cString: service))"
        }
        return "unknown"
    }
}
