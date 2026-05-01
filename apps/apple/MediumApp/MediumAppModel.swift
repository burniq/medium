import Foundation

@MainActor
final class MediumAppModel: ObservableObject {
    @Published var state: MediumClientState?
    @Published var devices: [DeviceRecord] = []
    @Published var selectedGrant: SessionOpenGrant?
    @Published var errorMessage: String?
    @Published var isLoading = false
    #if os(iOS)
    @Published var tunnelStatusText = "Tunnel not configured"
    let tunnelManager = TunnelManager()
    #endif

    private let store: ClientStateStore

    init(store: ClientStateStore = KeychainClientStateStore()) {
        self.store = store
        self.state = try? store.load()
    }

    func join(inviteText: String, deviceName: String) async {
        await run {
            let invite = try JoinInvite.parse(inviteText.trimmingCharacters(in: .whitespacesAndNewlines))
            let clientState = MediumClientState(
                controlURL: invite.controlURL,
                deviceName: deviceName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "iphone" : deviceName,
                inviteVersion: invite.version,
                security: invite.security,
                controlPin: invite.controlPin
            )
            try store.save(clientState)
            state = clientState
            devices = []
            selectedGrant = nil
        }
    }

    func refreshDevices() async {
        await run {
            guard let state else {
                throw MediumClientError.missingState
            }
            devices = try await makeClient(state: state).fetchDevices().devices
        }
    }

    func open(service: PublishedService) async {
        await run {
            guard let state else {
                throw MediumClientError.missingState
            }
            selectedGrant = try await makeClient(state: state).openSession(serviceID: service.id)
        }
    }

    func reset() {
        try? store.clear()
        state = nil
        devices = []
        selectedGrant = nil
        errorMessage = nil
    }

    #if os(iOS)
    func startTunnel() async {
        await run {
            try await tunnelManager.start()
            tunnelStatusText = "Tunnel requested"
        }
    }

    func stopTunnel() async {
        await run {
            try await tunnelManager.stop()
            tunnelStatusText = "Tunnel stopped"
        }
    }
    #endif

    private func run(_ operation: () async throws -> Void) async {
        isLoading = true
        errorMessage = nil
        do {
            try await operation()
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    private func makeClient(state: MediumClientState) -> MediumAPIClient {
        if state.security == "pinned-tls" {
            return MediumAPIClient(
                state: state,
                pinnedTransport: PinnedHTTPTransport(expectedPin: state.controlPin)
            )
        }
        return MediumAPIClient(state: state)
    }
}
