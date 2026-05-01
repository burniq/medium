import Foundation

struct MediumVirtualService: Equatable {
    let nodeID: String
    let serviceID: String
    let hostname: String
    let address: String
    let port: UInt16
}

enum MediumRouteTableError: Error, Equatable {
    case invalidSubnetBase(String)
    case addressPoolExhausted
}

struct MediumRouteTable {
    private var assignments: [String: MediumVirtualService] = [:]
    private var nextHost: UInt16 = 10

    let subnetBase: String

    init(subnetBase: String) {
        self.subnetBase = subnetBase
    }

    mutating func assign(nodeID: String, serviceID: String, port: UInt16) throws -> MediumVirtualService {
        let key = "\(nodeID)/\(serviceID)/\(port)"
        if let existing = assignments[key] {
            return existing
        }

        guard nextHost <= 254 else {
            throw MediumRouteTableError.addressPoolExhausted
        }

        let address = try virtualAddress(host: nextHost)
        let service = MediumVirtualService(
            nodeID: nodeID,
            serviceID: serviceID,
            hostname: "\(sanitizeLabel(serviceID)).\(sanitizeLabel(nodeID)).medium",
            address: address,
            port: port
        )

        nextHost += 1
        assignments[key] = service
        return service
    }

    private func virtualAddress(host: UInt16) throws -> String {
        let octets = subnetBase.split(separator: ".").compactMap { UInt8($0) }
        guard octets.count == 4 else {
            throw MediumRouteTableError.invalidSubnetBase(subnetBase)
        }

        return "\(octets[0]).\(octets[1]).\(octets[2]).\(host)"
    }

    private func sanitizeLabel(_ value: String) -> String {
        var result = ""
        var previousWasHyphen = false

        for scalar in value.lowercased().unicodeScalars {
            if isASCIIAlphanumeric(scalar) {
                result.unicodeScalars.append(scalar)
                previousWasHyphen = false
            } else if !previousWasHyphen {
                result.append("-")
                previousWasHyphen = true
            }
        }

        let trimmed = result.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return trimmed.isEmpty ? "service" : trimmed
    }

    private func isASCIIAlphanumeric(_ scalar: UnicodeScalar) -> Bool {
        ("a"..."z").contains(scalar) || ("0"..."9").contains(scalar)
    }
}
