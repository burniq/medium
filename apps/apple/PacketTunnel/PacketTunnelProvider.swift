import NetworkExtension
import os.log

final class PacketTunnelProvider: NEPacketTunnelProvider {
    private let logger = Logger(subsystem: "dev.homeworks.medium", category: "packet-tunnel")

    override func startTunnel(options: [String: NSObject]? = nil) async throws {
        logger.info("Medium packet tunnel starting")

        let settings = NEPacketTunnelNetworkSettings(tunnelRemoteAddress: "100.96.0.1")
        settings.ipv4Settings = NEIPv4Settings(
            addresses: ["100.96.0.2"],
            subnetMasks: ["255.240.0.0"]
        )
        settings.ipv4Settings?.includedRoutes = [
            NEIPv4Route(destinationAddress: "100.96.0.0", subnetMask: "255.240.0.0")
        ]
        settings.dnsSettings = NEDNSSettings(servers: ["100.96.0.1"])
        settings.dnsSettings?.matchDomains = ["medium"]

        try await setTunnelNetworkSettings(settings)
    }

    override func stopTunnel(with reason: NEProviderStopReason) async {
        logger.info("Medium packet tunnel stopped")
    }
}
