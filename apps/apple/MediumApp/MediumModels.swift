import Foundation

struct JoinInvite: Equatable {
    let version: Int
    let controlURL: URL
    let security: String
    let controlPin: String

    static func parse(_ raw: String) throws -> JoinInvite {
        guard let components = URLComponents(string: raw),
              components.scheme == "medium",
              components.host == "join" else {
            throw MediumClientError.invalidInvite("expected medium://join invite")
        }
        let query = Dictionary(uniqueKeysWithValues: (components.queryItems ?? []).map { ($0.name, $0.value ?? "") })
        guard query["v"] == "1" else {
            throw MediumClientError.invalidInvite("unsupported invite version")
        }
        guard let control = query["control"], let controlURL = URL(string: control), controlURL.host != nil else {
            throw MediumClientError.invalidInvite("missing control URL")
        }
        guard query["security"] == "pinned-tls" else {
            throw MediumClientError.invalidInvite("unsupported invite security")
        }
        guard let controlPin = query["control_pin"], !controlPin.isEmpty else {
            throw MediumClientError.invalidInvite("missing control pin")
        }
        return JoinInvite(version: 1, controlURL: controlURL, security: "pinned-tls", controlPin: controlPin)
    }
}

struct MediumClientState: Codable, Equatable {
    let controlURL: URL
    let deviceName: String
    let inviteVersion: Int
    let security: String
    let controlPin: String
}

struct DeviceCatalog: Decodable {
    let devices: [DeviceRecord]
}

struct DeviceRecord: Decodable, Identifiable {
    let id: String
    let name: String
    let services: [PublishedService]
}

struct PublishedService: Decodable, Identifiable, Equatable {
    let id: String
    let kind: ServiceKind
    let schemaVersion: Int
    let label: String?
    let target: String
    let userName: String?

    var displayName: String {
        label?.isEmpty == false ? label! : id
    }

    enum CodingKeys: String, CodingKey {
        case id
        case kind
        case schemaVersion = "schema_version"
        case label
        case target
        case userName = "user_name"
    }
}

enum ServiceKind: String, Codable {
    case https
    case ssh
}

struct SessionOpenGrant: Decodable, Equatable {
    let sessionID: String
    let serviceID: String
    let nodeID: String
    let relayHint: String?
    let authorization: SessionAuthorization

    enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case serviceID = "service_id"
        case nodeID = "node_id"
        case relayHint = "relay_hint"
        case authorization
    }
}

struct SessionAuthorization: Decodable, Equatable {
    let token: String
    let expiresAt: Date
    let candidates: [PeerCandidate]

    enum CodingKeys: String, CodingKey {
        case token
        case expiresAt = "expires_at"
        case candidates
    }
}

struct PeerCandidate: Decodable, Identifiable, Equatable {
    var id: String { "\(kind.rawValue)-\(addr)" }
    let kind: CandidateKind
    let addr: String
    let priority: Int
}

enum CandidateKind: String, Codable {
    case directTcp = "direct_tcp"
    case relayTcp = "relay_tcp"
    case wssRelay = "wss_relay"
}

enum MediumClientError: LocalizedError, Equatable {
    case invalidInvite(String)
    case missingState
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .invalidInvite(let message):
            return message
        case .missingState:
            return "Join this device before loading services."
        case .invalidResponse:
            return "Control plane returned an invalid response."
        }
    }
}

extension JSONDecoder {
    static var medium: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }
}
