import Foundation
import CryptoKit
import os

struct MediumAPIClient {
    let state: MediumClientState
    var session: URLSession = .shared
    var pinnedTransport: PinnedHTTPTransport?

    func fetchDevices() async throws -> DeviceCatalog {
        let request = try makeRequest(path: "/api/devices")
        let data = try await load(request)
        return try JSONDecoder.medium.decode(DeviceCatalog.self, from: data)
    }

    func fetchMediumCA() async throws -> String {
        let request = try makeRequest(path: "/api/medium-ca.pem")
        let data = try await load(request)
        guard let pem = String(data: data, encoding: .utf8), pem.contains("BEGIN CERTIFICATE") else {
            throw MediumClientError.invalidResponse
        }
        return pem
    }

    func openSession(serviceID: String) async throws -> SessionOpenGrant {
        let request = try makeOpenSessionRequest(serviceID: serviceID)
        let data = try await load(request)
        return try JSONDecoder.medium.decode(SessionOpenGrant.self, from: data)
    }

    func makeOpenSessionRequest(serviceID: String) throws -> URLRequest {
        var components = URLComponents(url: state.controlURL.mediumAppending(path: "/api/sessions/open"), resolvingAgainstBaseURL: false)
        components?.queryItems = [
            URLQueryItem(name: "service_id", value: serviceID),
            URLQueryItem(name: "requester_device_id", value: state.deviceName)
        ]
        guard let url = components?.url else {
            throw MediumClientError.invalidResponse
        }
        return makeJSONRequest(url: url)
    }

    private func makeRequest(path: String) throws -> URLRequest {
        makeJSONRequest(url: state.controlURL.mediumAppending(path: path))
    }

    private func makeJSONRequest(url: URL) -> URLRequest {
        var request = URLRequest(url: url)
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.timeoutInterval = 15
        return request
    }

    private func load(_ request: URLRequest) async throws -> Data {
        if let pinnedTransport {
            return try await pinnedTransport.data(for: request)
        }
        let (data, response) = try await session.data(for: request)
        try validate(response)
        return data
    }

    private func validate(_ response: URLResponse) throws {
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw MediumClientError.invalidResponse
        }
    }
}

private extension URL {
    func mediumAppending(path: String) -> URL {
        var url = self
        for component in path.split(separator: "/") {
            url.appendPathComponent(String(component))
        }
        return url
    }
}

final class PinnedControlSessionDelegate: NSObject, URLSessionDelegate, URLSessionTaskDelegate {
    private let logger = Logger(subsystem: "io.burniq.medium", category: "PinnedTLS")
    private let expectedPin: String

    init(expectedPin: String) {
        self.expectedPin = expectedPin.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        handle(challenge, completionHandler: completionHandler)
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        handle(challenge, completionHandler: completionHandler)
    }

    private func handle(
        _ challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust else {
            logger.debug("Pinned TLS skipped non-server-trust challenge: \(challenge.protectionSpace.authenticationMethod, privacy: .public)")
            completionHandler(.performDefaultHandling, nil)
            return
        }

        guard let trust = challenge.protectionSpace.serverTrust,
              let certificate = SecTrustGetCertificateAtIndex(trust, 0) else {
            logger.error("Pinned TLS missing server trust or leaf certificate")
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }

        let actualPin = SHA256Pin.make(for: SecCertificateCopyData(certificate) as Data)
        guard actualPin == expectedPin else {
            logger.error("Pinned TLS rejected certificate pin. expected=\(self.expectedPin, privacy: .public) actual=\(actualPin, privacy: .public)")
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }

        SecTrustSetAnchorCertificates(trust, [certificate] as CFArray)
        SecTrustSetAnchorCertificatesOnly(trust, true)

        var trustError: CFError?
        guard SecTrustEvaluateWithError(trust, &trustError) else {
            logger.error("Pinned TLS trust evaluation failed after pin match for \(challenge.protectionSpace.host, privacy: .public)")
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }

        logger.debug("Pinned TLS accepted certificate pin and trust for \(challenge.protectionSpace.host, privacy: .public)")
        completionHandler(.useCredential, URLCredential(trust: trust))
    }
}

enum SHA256Pin {
    static func make(for data: Data) -> String {
        let digest = SHA256.hash(data: data)
        return "sha256:" + digest.map { String(format: "%02x", $0) }.joined()
    }
}
