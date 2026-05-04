import Darwin
import Foundation

enum ForegroundTCPStreamError: LocalizedError {
    case invalidAddress(String)
    case connectFailed(String)
    case socket(String)

    var errorDescription: String? {
        switch self {
        case .invalidAddress(let address):
            return "Invalid TCP candidate address: \(address)."
        case .connectFailed(let address):
            return "Failed to connect Medium TCP candidate \(address)."
        case .socket(let message):
            return "TCP socket error: \(message)."
        }
    }
}

final class ForegroundTCPStream: ForegroundByteStream {
    private let fd: Int32
    private let lock = NSLock()
    private var closed = false

    private init(fd: Int32) {
        self.fd = fd
    }

    deinit {
        close()
    }

    static func connect(to address: String) throws -> ForegroundTCPStream {
        guard let separator = address.lastIndex(of: ":") else {
            throw ForegroundTCPStreamError.invalidAddress(address)
        }
        let host = String(address[..<separator]).trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        let port = String(address[address.index(after: separator)...])

        var hints = addrinfo(
            ai_flags: 0,
            ai_family: AF_UNSPEC,
            ai_socktype: SOCK_STREAM,
            ai_protocol: IPPROTO_TCP,
            ai_addrlen: 0,
            ai_canonname: nil,
            ai_addr: nil,
            ai_next: nil
        )
        var result: UnsafeMutablePointer<addrinfo>?
        guard getaddrinfo(host, port, &hints, &result) == 0, let result else {
            throw ForegroundTCPStreamError.invalidAddress(address)
        }
        defer { freeaddrinfo(result) }

        var cursor: UnsafeMutablePointer<addrinfo>? = result
        var lastError = ForegroundTCPStreamError.connectFailed(address)
        while let info = cursor {
            let fd = socket(info.pointee.ai_family, info.pointee.ai_socktype, info.pointee.ai_protocol)
            if fd >= 0 {
                do {
                    try configure(fd)
                    if Darwin.connect(fd, info.pointee.ai_addr, info.pointee.ai_addrlen) == 0 {
                        return ForegroundTCPStream(fd: fd)
                    }
                    lastError = .socket(String(cString: strerror(errno)))
                } catch {
                    lastError = .socket(error.localizedDescription)
                }
                Darwin.close(fd)
            }
            cursor = info.pointee.ai_next
        }
        throw lastError
    }

    func write(_ bytes: Data) throws {
        try bytes.withUnsafeBytes { rawBuffer in
            guard let base = rawBuffer.bindMemory(to: UInt8.self).baseAddress else {
                return
            }
            var offset = 0
            while offset < bytes.count {
                let sent = Darwin.write(fd, base.advanced(by: offset), bytes.count - offset)
                if sent < 0 {
                    throw ForegroundTCPStreamError.socket(String(cString: strerror(errno)))
                }
                if sent == 0 {
                    throw ForegroundTCPStreamError.socket("remote closed TCP stream")
                }
                offset += sent
            }
        }
    }

    func read(maxLength: Int = 64 * 1024) throws -> Data {
        var buffer = [UInt8](repeating: 0, count: maxLength)
        let size = Darwin.read(fd, &buffer, buffer.count)
        if size < 0 {
            throw ForegroundTCPStreamError.socket(String(cString: strerror(errno)))
        }
        if size == 0 {
            return Data()
        }
        return Data(buffer.prefix(size))
    }

    func close() {
        lock.lock()
        defer { lock.unlock() }
        guard !closed else {
            return
        }
        closed = true
        Darwin.shutdown(fd, SHUT_RDWR)
        Darwin.close(fd)
    }

    private static func configure(_ fd: Int32) throws {
        var timeout = timeval(tv_sec: 12, tv_usec: 0)
        guard setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size)) == 0,
              setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size)) == 0 else {
            throw ForegroundTCPStreamError.socket(String(cString: strerror(errno)))
        }
        var noSigPipe: Int32 = 1
        _ = setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &noSigPipe, socklen_t(MemoryLayout<Int32>.size))
    }
}
