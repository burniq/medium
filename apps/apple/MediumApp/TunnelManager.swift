#if os(iOS)
import Foundation
import NetworkExtension

@MainActor
final class TunnelManager: ObservableObject {
    @Published private(set) var status: NEVPNStatus = .invalid

    func saveOrLoadManager() async throws -> NETunnelProviderManager {
        let managers = try await NETunnelProviderManager.loadAllFromPreferences()
        let manager = managers.first ?? NETunnelProviderManager()
        let proto = NETunnelProviderProtocol()
        proto.providerBundleIdentifier = "io.burniq.medium.ios.PacketTunnel"
        proto.serverAddress = "Medium"
        manager.protocolConfiguration = proto
        manager.localizedDescription = "Medium"
        manager.isEnabled = true
        try await manager.saveToPreferences()
        try await manager.loadFromPreferences()
        status = manager.connection.status
        return manager
    }

    func start() async throws {
        let manager = try await saveOrLoadManager()
        try manager.connection.startVPNTunnel()
        status = manager.connection.status
    }

    func stop() async throws {
        let manager = try await saveOrLoadManager()
        manager.connection.stopVPNTunnel()
        status = manager.connection.status
    }
}
#endif
