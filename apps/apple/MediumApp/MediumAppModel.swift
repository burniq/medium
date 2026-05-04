import Foundation

@MainActor
final class MediumAppModel: ObservableObject {
    @Published var state: MediumClientState?
    @Published var devices: [DeviceRecord] = []
    @Published var selectedGrant: SessionOpenGrant?
    @Published var browserSession: ForegroundBrowserSession?
    @Published var errorMessage: String?
    @Published var isLoading = false
    private let foregroundBrowserProxy = ForegroundBrowserProxy()
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
            let baseState = MediumClientState(
                controlURL: invite.controlURL,
                deviceName: deviceName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "iphone" : deviceName,
                inviteVersion: invite.version,
                security: invite.security,
                controlPin: invite.controlPin,
                serviceCAPEM: nil
            )
            let serviceCAPEM = try await makeClient(state: baseState).fetchMediumCA()
            let clientState = MediumClientState(
                controlURL: baseState.controlURL,
                deviceName: baseState.deviceName,
                inviteVersion: baseState.inviteVersion,
                security: baseState.security,
                controlPin: baseState.controlPin,
                serviceCAPEM: serviceCAPEM
            )
            try store.save(clientState)
            state = clientState
            devices = []
            selectedGrant = nil
            browserSession = nil
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
            let currentState = try await ensureServiceCA(state)
            let grant = try await makeClient(state: currentState).openSession(serviceID: service.id)
            #if os(iOS)
            let localURL = try await foregroundBrowserProxy.start(
                service: service,
                grant: grant,
                serviceCAPEM: currentState.serviceCAPEM
            )
            browserSession = ForegroundBrowserSession(id: grant.sessionID, service: service, localURL: localURL)
            #else
            selectedGrant = grant
            #endif
        }
    }

    func closeBrowser() {
        foregroundBrowserProxy.stop()
        browserSession = nil
    }

    func reset() {
        try? store.clear()
        foregroundBrowserProxy.stop()
        state = nil
        devices = []
        selectedGrant = nil
        browserSession = nil
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

    private func ensureServiceCA(_ currentState: MediumClientState) async throws -> MediumClientState {
        if currentState.serviceCAPEM?.isEmpty == false {
            return currentState
        }
        let serviceCAPEM = try await makeClient(state: currentState).fetchMediumCA()
        let updated = MediumClientState(
            controlURL: currentState.controlURL,
            deviceName: currentState.deviceName,
            inviteVersion: currentState.inviteVersion,
            security: currentState.security,
            controlPin: currentState.controlPin,
            serviceCAPEM: serviceCAPEM
        )
        try store.save(updated)
        state = updated
        return updated
    }
}
