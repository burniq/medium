import Foundation
import Security

enum ForegroundSecureSessionError: LocalizedError {
    case contextCreationFailed
    case invalidServiceCA
    case trustEvaluationFailed
    case tlsFailed(OSStatus, String)

    var errorDescription: String? {
        switch self {
        case .contextCreationFailed:
            return "Failed to create Medium service TLS context."
        case .invalidServiceCA:
            return "Medium service CA is missing or invalid."
        case .trustEvaluationFailed:
            return "Medium service TLS trust evaluation failed."
        case .tlsFailed(let status, let operation):
            return "Medium service TLS \(operation) failed with OSStatus \(status)."
        }
    }
}

protocol ForegroundByteStream: AnyObject {
    func write(_ bytes: Data) throws
    func read(maxLength: Int) throws -> Data
    func close()
}

extension ForegroundUdpSession: ForegroundByteStream {}

final class ForegroundSecureSession: ForegroundByteStream {
    private let transport: ForegroundByteStream
    private let context: SSLContext
    private let serviceCACertificate: SecCertificate?

    init(transport: ForegroundByteStream, serverName: String, serviceCAPEM: String?) throws {
        self.transport = transport
        self.serviceCACertificate = try serviceCAPEM.map { try Self.certificate(fromPEM: $0) }
        guard let context = SSLCreateContext(nil, .clientSide, .streamType) else {
            throw ForegroundSecureSessionError.contextCreationFailed
        }
        self.context = context

        try Self.check(SSLSetIOFuncs(context, secureTransportRead, secureTransportWrite), "configure I/O")
        try Self.check(SSLSetConnection(context, Unmanaged.passUnretained(self).toOpaque()), "bind I/O")
        try serverName.withCString { pointer in
            try Self.check(SSLSetPeerDomainName(context, pointer, serverName.utf8.count), "set SNI")
        }
        if serviceCACertificate != nil {
            try Self.check(SSLSetSessionOption(context, SSLSessionOption(rawValue: 0)!, true), "enable service CA trust")
        }
        try handshake()
    }

    deinit {
        SSLClose(context)
    }

    func write(_ bytes: Data) throws {
        try bytes.withUnsafeBytes { rawBuffer in
            guard let base = rawBuffer.bindMemory(to: UInt8.self).baseAddress else {
                return
            }
            var offset = 0
            while offset < bytes.count {
                var processed = 0
                let status = SSLWrite(context, base.advanced(by: offset), bytes.count - offset, &processed)
                if status == errSSLWouldBlock {
                    continue
                }
                try Self.check(status, "write")
                offset += processed
            }
        }
    }

    func read(maxLength: Int = 64 * 1024) throws -> Data {
        let deadline = Date().addingTimeInterval(12)
        while true {
            var buffer = [UInt8](repeating: 0, count: maxLength)
            var processed = 0
            let status = SSLRead(context, &buffer, buffer.count, &processed)
            if status == noErr {
                return Data(buffer.prefix(processed))
            }
            if status == errSSLClosedGraceful || status == errSSLClosedAbort {
                return Data()
            }
            if status == errSSLWouldBlock, Date() < deadline {
                continue
            }
            try Self.check(status, "read")
            return Data()
        }
    }

    func close() {
        SSLClose(context)
        transport.close()
    }

    fileprivate func transportRead(maxLength: Int) -> OSStatusData {
        do {
            return .data(try transport.read(maxLength: maxLength))
        } catch {
            return .status(errSSLWouldBlock)
        }
    }

    fileprivate func transportWrite(_ bytes: Data) -> OSStatus {
        do {
            try transport.write(bytes)
            return noErr
        } catch {
            return errSSLClosedAbort
        }
    }

    private func handshake() throws {
        let deadline = Date().addingTimeInterval(10)
        while true {
            let status = SSLHandshake(context)
            if status == noErr {
                print("Medium foreground service TLS connected")
                return
            }
            if status == errSSLPeerAuthCompleted {
                try evaluateServerTrust()
                continue
            }
            if status == errSSLWouldBlock, Date() < deadline {
                continue
            }
            throw ForegroundSecureSessionError.tlsFailed(status, "handshake")
        }
    }

    private func evaluateServerTrust() throws {
        guard let serviceCACertificate else {
            return
        }
        var trust: SecTrust?
        try Self.check(SSLCopyPeerTrust(context, &trust), "copy peer trust")
        guard let trust else {
            throw ForegroundSecureSessionError.trustEvaluationFailed
        }
        try Self.check(SecTrustSetAnchorCertificates(trust, [serviceCACertificate] as CFArray), "set Medium CA anchor")
        try Self.check(SecTrustSetAnchorCertificatesOnly(trust, true), "restrict Medium CA anchors")
        var trustError: CFError?
        guard SecTrustEvaluateWithError(trust, &trustError) else {
            throw ForegroundSecureSessionError.trustEvaluationFailed
        }
        print("Medium foreground service TLS accepted Medium CA")
    }

    private static func check(_ status: OSStatus, _ operation: String) throws {
        guard status == noErr else {
            throw ForegroundSecureSessionError.tlsFailed(status, operation)
        }
    }

    static func certificate(fromPEM pem: String) throws -> SecCertificate {
        let der = try certificateDER(fromPEM: pem)
        guard let certificate = SecCertificateCreateWithData(nil, der as CFData) else {
            throw ForegroundSecureSessionError.invalidServiceCA
        }
        return certificate
    }

    static func certificateDER(fromPEM pem: String) throws -> Data {
        let lines = pem
            .split(whereSeparator: \.isNewline)
            .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
        guard let begin = lines.firstIndex(of: "-----BEGIN CERTIFICATE-----"),
              let end = lines[(begin + 1)...].firstIndex(of: "-----END CERTIFICATE-----") else {
            throw ForegroundSecureSessionError.invalidServiceCA
        }
        let base64 = lines[(begin + 1)..<end].joined()
        guard let data = Data(base64Encoded: base64) else {
            throw ForegroundSecureSessionError.invalidServiceCA
        }
        return data
    }
}

private enum OSStatusData {
    case data(Data)
    case status(OSStatus)
}

private func secureTransportRead(
    connection: SSLConnectionRef,
    data: UnsafeMutableRawPointer,
    dataLength: UnsafeMutablePointer<Int>
) -> OSStatus {
    let session = Unmanaged<ForegroundSecureSession>
        .fromOpaque(connection)
        .takeUnretainedValue()
    switch session.transportRead(maxLength: dataLength.pointee) {
    case .data(let bytes):
        if bytes.isEmpty {
            dataLength.pointee = 0
            return errSSLClosedGraceful
        }
        let copiedBytes = min(bytes.count, dataLength.pointee)
        bytes.withUnsafeBytes { rawBuffer in
            if let base = rawBuffer.baseAddress, !bytes.isEmpty {
                data.copyMemory(from: base, byteCount: copiedBytes)
            }
        }
        dataLength.pointee = copiedBytes
        return noErr
    case .status(let status):
        dataLength.pointee = 0
        return status
    }
}

private func secureTransportWrite(
    connection: SSLConnectionRef,
    data: UnsafeRawPointer,
    dataLength: UnsafeMutablePointer<Int>
) -> OSStatus {
    let session = Unmanaged<ForegroundSecureSession>
        .fromOpaque(connection)
        .takeUnretainedValue()
    let bytes = Data(bytes: data, count: dataLength.pointee)
    let status = session.transportWrite(bytes)
    if status != noErr {
        dataLength.pointee = 0
    }
    return status
}
