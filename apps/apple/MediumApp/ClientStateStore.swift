import Foundation
import Security

protocol ClientStateStore {
    func load() throws -> MediumClientState?
    func save(_ state: MediumClientState) throws
    func clear() throws
}

final class MemoryClientStateStore: ClientStateStore {
    private var state: MediumClientState?

    func load() throws -> MediumClientState? {
        state
    }

    func save(_ state: MediumClientState) throws {
        self.state = state
    }

    func clear() throws {
        state = nil
    }
}

final class KeychainClientStateStore: ClientStateStore {
    private let service = "dev.homeworks.medium"
    private let account = "client-state"

    func load() throws -> MediumClientState? {
        var query = baseQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess, let data = item as? Data else {
            throw KeychainError.unhandled(status)
        }
        return try JSONDecoder.medium.decode(MediumClientState.self, from: data)
    }

    func save(_ state: MediumClientState) throws {
        let data = try JSONEncoder().encode(state)
        var query = baseQuery()
        query[kSecValueData as String] = data
        query[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly

        let status = SecItemAdd(query as CFDictionary, nil)
        if status == errSecDuplicateItem {
            let update: [String: Any] = [kSecValueData as String: data]
            let updateStatus = SecItemUpdate(baseQuery() as CFDictionary, update as CFDictionary)
            guard updateStatus == errSecSuccess else {
                throw KeychainError.unhandled(updateStatus)
            }
            return
        }
        guard status == errSecSuccess else {
            throw KeychainError.unhandled(status)
        }
    }

    func clear() throws {
        let status = SecItemDelete(baseQuery() as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainError.unhandled(status)
        }
    }

    private func baseQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
    }
}

enum KeychainError: LocalizedError, Equatable {
    case unhandled(OSStatus)

    var errorDescription: String? {
        switch self {
        case .unhandled(let status):
            return "Keychain operation failed with status \(status)."
        }
    }
}
