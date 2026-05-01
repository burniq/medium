@preconcurrency import Foundation
@preconcurrency import Network
import os

final class PinnedHTTPTransport {
    private let expectedPin: String
    private let logger = Logger(subsystem: "io.burniq.medium", category: "PinnedHTTP")

    init(expectedPin: String) {
        self.expectedPin = expectedPin.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    func data(for request: URLRequest) async throws -> Data {
        guard let url = request.url,
              url.scheme == "https",
              let host = url.host else {
            throw MediumClientError.invalidResponse
        }

        let port = UInt16(url.port ?? 443)
        let queue = DispatchQueue(label: "io.burniq.medium.pinned-http")
        let connection = NWConnection(
            host: NWEndpoint.Host(host),
            port: NWEndpoint.Port(rawValue: port) ?? .https,
            using: parameters(host: host, queue: queue)
        )

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let state = PinnedHTTPState()
                let timeout = DispatchWorkItem {
                    finish(
                        connection: connection,
                        state: state,
                        continuation: continuation,
                        result: .failure(URLError(.timedOut))
                    )
                }
                state.timeout = timeout

                queue.asyncAfter(deadline: .now() + (request.timeoutInterval > 0 ? request.timeoutInterval : 15), execute: timeout)

                connection.stateUpdateHandler = { [weak self] connectionState in
                    switch connectionState {
                    case .ready:
                        let requestData = self?.makeRequestData(for: request) ?? Data()
                        connection.send(content: requestData, completion: .contentProcessed { error in
                            if let error {
                                finish(
                                    connection: connection,
                                    state: state,
                                    continuation: continuation,
                                    result: .failure(error)
                                )
                                return
                            }
                            receive(
                                connection: connection,
                                state: state,
                                continuation: continuation
                            )
                        })
                    case .failed(let error):
                        finish(
                            connection: connection,
                            state: state,
                            continuation: continuation,
                            result: .failure(error)
                        )
                    default:
                        break
                    }
                }

                connection.start(queue: queue)
            }
        } onCancel: {
            connection.cancel()
        }
    }

    private func parameters(host: String, queue: DispatchQueue) -> NWParameters {
        let tls = NWProtocolTLS.Options()
        sec_protocol_options_set_tls_server_name(tls.securityProtocolOptions, host)
        sec_protocol_options_set_verify_block(tls.securityProtocolOptions, { [expectedPin, logger] _, secTrust, complete in
            let trust = sec_trust_copy_ref(secTrust).takeRetainedValue()
            guard let certificate = SecTrustGetCertificateAtIndex(trust, 0) else {
                logger.error("Pinned HTTP missing leaf certificate")
                complete(false)
                return
            }

            let actualPin = SHA256Pin.make(for: SecCertificateCopyData(certificate) as Data)
            guard actualPin == expectedPin else {
                logger.error("Pinned HTTP rejected certificate pin. expected=\(expectedPin, privacy: .public) actual=\(actualPin, privacy: .public)")
                complete(false)
                return
            }

            logger.debug("Pinned HTTP accepted certificate pin for \(host, privacy: .public)")
            complete(true)
        }, queue)

        let parameters = NWParameters(tls: tls)
        parameters.allowLocalEndpointReuse = true
        return parameters
    }

    private func makeRequestData(for request: URLRequest) -> Data {
        guard let url = request.url else {
            return Data()
        }

        var target = url.path.isEmpty ? "/" : url.path
        if let query = url.query, !query.isEmpty {
            target += "?\(query)"
        }

        let host = url.port.map { "\(url.host ?? ""):\($0)" } ?? (url.host ?? "")
        var lines = [
            "GET \(target) HTTP/1.1",
            "Host: \(host)",
            "Accept: \(request.value(forHTTPHeaderField: "Accept") ?? "application/json")",
            "Connection: close",
            "User-Agent: Medium/0.1"
        ]
        lines.append("")
        lines.append("")
        return lines.joined(separator: "\r\n").data(using: .utf8) ?? Data()
    }
}

private final class PinnedHTTPState: @unchecked Sendable {
    var response = Data()
    var completed = false
    var timeout: DispatchWorkItem?
}

private func receive(
    connection: NWConnection,
    state: PinnedHTTPState,
    continuation: CheckedContinuation<Data, Error>
) {
    connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { data, _, isComplete, error in
        if let error {
            finish(
                connection: connection,
                state: state,
                continuation: continuation,
                result: .failure(error)
            )
            return
        }

        if let data {
            state.response.append(data)
        }

        if isComplete {
            finish(
                connection: connection,
                state: state,
                continuation: continuation,
                result: parseHTTPResponse(state.response)
            )
            return
        }

        receive(
            connection: connection,
            state: state,
            continuation: continuation
        )
    }
}

private func finish(
    connection: NWConnection,
    state: PinnedHTTPState,
    continuation: CheckedContinuation<Data, Error>,
    result: Result<Data, Error>
) {
    guard !state.completed else {
        return
    }
    state.completed = true
    state.timeout?.cancel()
    connection.cancel()
    continuation.resume(with: result)
}

private func parseHTTPResponse(_ response: Data) -> Result<Data, Error> {
    let separator = Data("\r\n\r\n".utf8)
    guard let headerEnd = response.range(of: separator) else {
        return .failure(MediumClientError.invalidResponse)
    }

    let headerData = response[..<headerEnd.lowerBound]
    guard let headerText = String(data: headerData, encoding: .utf8),
          let statusLine = headerText.split(separator: "\r\n").first,
          let statusCode = Int(statusLine.split(separator: " ").dropFirst().first ?? "") else {
        return .failure(MediumClientError.invalidResponse)
    }

    guard (200..<300).contains(statusCode) else {
        return .failure(MediumClientError.invalidResponse)
    }

    return .success(Data(response[headerEnd.upperBound...]))
}
