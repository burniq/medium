import Foundation
import Network

enum ForegroundBrowserError: LocalizedError {
    case unsupportedServiceKind(String)
    case noUsableCandidate
    case noUsableCandidateWithReason(String)
    case invalidCandidateAddress(String)
    case listenerUnavailable
    case sessionHelloEncodingFailed
    case listenerFailed(String)

    var errorDescription: String? {
        switch self {
        case .unsupportedServiceKind(let kind):
            return "Foreground browser currently supports HTTP services only, got \(kind)."
        case .noUsableCandidate:
            return "Session grant has no direct TCP or relay TCP candidate usable from iOS foreground mode."
        case .noUsableCandidateWithReason(let reason):
            return "No Medium foreground candidate connected. \(reason)"
        case .invalidCandidateAddress(let address):
            return "Invalid Medium candidate address: \(address)."
        case .listenerUnavailable:
            return "Failed to start local foreground browser proxy."
        case .sessionHelloEncodingFailed:
            return "Failed to encode Medium session hello."
        case .listenerFailed(let message):
            return "Failed to start local foreground browser proxy: \(message)."
        }
    }
}

struct ForegroundBrowserSession: Identifiable, Equatable {
    let id: String
    let service: PublishedService
    let localURL: URL
}

struct ForegroundPipeState {
    private var completedDirections = Set<String>()
    private var bytesByDirection: [String: Int] = [:]

    mutating func addBytes(_ count: Int, direction: String) {
        bytesByDirection[direction, default: 0] += count
    }

    func bytes(for direction: String) -> Int {
        bytesByDirection[direction, default: 0]
    }

    mutating func markCompleted(_ direction: String) -> Bool {
        completedDirections.insert(direction)
        return completedDirections.count >= 2
    }
}

struct ForegroundBrowserResponseChunk {
    let data: Data
    let closeAfterSend: Bool
}

struct ForegroundPreparedStream {
    let stream: ForegroundByteStream
    let selectedIceCandidate: IceCandidate?
    let candidateDescription: String
}

struct ForegroundActiveByteStreamConnection {
    let browser: NWConnection
    let medium: ForegroundByteStream
    let serviceID: String
    let candidateDescription: String
}

final class ForegroundIdleStreamPool {
    private let maxPerService: Int
    private var streams: [String: [ForegroundPreparedStream]] = [:]

    init(maxPerService: Int) {
        self.maxPerService = maxPerService
    }

    func push(_ stream: ForegroundByteStream, serviceID: String) -> Bool {
        push(
            ForegroundPreparedStream(
                stream: stream,
                selectedIceCandidate: nil,
                candidateDescription: "warmed"
            ),
            serviceID: serviceID
        )
    }

    func push(_ prepared: ForegroundPreparedStream, serviceID: String) -> Bool {
        var entries = streams[serviceID] ?? []
        guard entries.count < maxPerService else {
            prepared.stream.close()
            return false
        }
        entries.append(prepared)
        streams[serviceID] = entries
        return true
    }

    func pop(serviceID: String) -> ForegroundPreparedStream? {
        guard var entries = streams[serviceID], !entries.isEmpty else {
            return nil
        }
        let stream = entries.removeFirst()
        streams[serviceID] = entries.isEmpty ? nil : entries
        return stream
    }

    func len(serviceID: String) -> Int {
        streams[serviceID]?.count ?? 0
    }

    func closeAll() {
        streams.values.flatMap { $0 }.forEach { $0.stream.close() }
        streams = [:]
    }
}

final class ForegroundWarmWaiters {
    typealias Waiter = (ForegroundPreparedStream?) -> Void

    private var waiters: [String: [Waiter]] = [:]

    func add(serviceID: String, waiter: @escaping Waiter) {
        waiters[serviceID, default: []].append(waiter)
    }

    func count(serviceID: String) -> Int {
        waiters[serviceID]?.count ?? 0
    }

    func resolve(serviceID: String, prepared: ForegroundPreparedStream) -> Bool {
        guard let serviceWaiters = waiters.removeValue(forKey: serviceID), !serviceWaiters.isEmpty else {
            return false
        }
        serviceWaiters[0](prepared)
        serviceWaiters.dropFirst().forEach { $0(nil) }
        return true
    }

    func reject(serviceID: String) {
        let serviceWaiters = waiters.removeValue(forKey: serviceID) ?? []
        serviceWaiters.forEach { $0(nil) }
    }

    func rejectAll() {
        let allWaiters = waiters.values.flatMap { $0 }
        waiters = [:]
        allWaiters.forEach { $0(nil) }
    }
}

struct ForegroundUdpConnectOutcome {
    let stream: ForegroundByteStream
    let selection: ForegroundUdpCandidateSelection
}

enum ForegroundUdpConnector {
    typealias Connect = (ForegroundUdpCandidateSelection) throws -> ForegroundByteStream

    static func connectFirstSuccessful(
        grant: SessionOpenGrant,
        iceChecklist: [ForegroundUdpCandidateSelection],
        connect: @escaping Connect
    ) throws -> ForegroundUdpConnectOutcome {
        guard !iceChecklist.isEmpty else {
            throw ForegroundUdpSessionError.noIceCandidate
        }

        let lock = NSLock()
        let done = DispatchSemaphore(value: 0)
        let group = DispatchGroup()
        var winner: ForegroundUdpConnectOutcome?
        var failures: [String] = []
        var completed = 0

        for selection in iceChecklist {
            group.enter()
            DispatchQueue.global(qos: .userInitiated).async {
                defer {
                    lock.lock()
                    completed += 1
                    let shouldSignalFailure = completed == iceChecklist.count && winner == nil
                    lock.unlock()
                    if shouldSignalFailure {
                        done.signal()
                    }
                    group.leave()
                }

                do {
                    let stream = try connect(selection)
                    lock.lock()
                    if winner == nil {
                        winner = ForegroundUdpConnectOutcome(stream: stream, selection: selection)
                        lock.unlock()
                        done.signal()
                    } else {
                        lock.unlock()
                        stream.close()
                    }
                } catch {
                    let reason = "\(selection.candidate.kind.rawValue) \(selection.addr): \(error.localizedDescription)"
                    lock.lock()
                    failures.append(reason)
                    lock.unlock()
                }
            }
        }

        done.wait()
        lock.lock()
        let result = winner
        let failureSummary = failures.joined(separator: "; ")
        lock.unlock()

        if let result {
            group.notify(queue: .global(qos: .utility)) {}
            return result
        }
        throw ForegroundBrowserError.noUsableCandidateWithReason(failureSummary)
    }
}

enum ForegroundSessionHello {
    static func makeFrame(for grant: SessionOpenGrant) throws -> Data {
        let payload = [
            "token": grant.authorization.token,
            "service_id": grant.serviceID
        ]
        var data = try JSONSerialization.data(withJSONObject: payload, options: [])
        guard let newline = "\n".data(using: .utf8) else {
            throw ForegroundBrowserError.sessionHelloEncodingFailed
        }
        data.append(newline)
        return data
    }
}

enum ForegroundRelayHello {
    static func makeFrame(for grant: SessionOpenGrant) throws -> Data {
        let payload = [
            "role": "client",
            "node_id": grant.nodeID
        ]
        var data = try JSONSerialization.data(withJSONObject: payload, options: [])
        guard let newline = "\n".data(using: .utf8) else {
            throw ForegroundBrowserError.sessionHelloEncodingFailed
        }
        data.append(newline)
        return data
    }
}

final class ForegroundBrowserProxy: @unchecked Sendable {
    private let queue = DispatchQueue(label: "io.burniq.medium.foreground-browser-proxy")
    private var listener: NWListener?
    private var service: PublishedService?
    private var grant: SessionOpenGrant?
    private var serviceCAPEM: String?
    private var activeConnections: [UUID: (browser: NWConnection, medium: NWConnection)] = [:]
    private var activeUdpConnections: [UUID: ForegroundActiveByteStreamConnection] = [:]
    private var completedBrowserConnections: [UUID: NWConnection] = [:]
    private var pipeStates: [UUID: ForegroundPipeState] = [:]
    private var responseTimeouts: [UUID: DispatchWorkItem] = [:]
    private var idleStreams = ForegroundIdleStreamPool(maxPerService: 1)
    private var warmingServices = Set<String>()
    private var warmWaiters = ForegroundWarmWaiters()
    private var selectedIceCandidates: [String: IceCandidate] = [:]
    private var iceBypassUntil: [String: Date] = [:]
    private var generation = 0
    private static let iceFailureBypassSeconds = 20.0

    deinit {
        stop()
    }

    static func localURLScheme(for service: PublishedService) -> String {
        service.kind == .https ? "https" : "http"
    }

    func start(service: PublishedService, grant: SessionOpenGrant, serviceCAPEM: String?) async throws -> URL {
        guard service.supportsForegroundBrowser else {
            throw ForegroundBrowserError.unsupportedServiceKind(service.kind.rawValue)
        }
        guard Self.bestCandidate(in: grant) != nil || !ForegroundUdpSession.iceChecklist(in: grant).isEmpty else {
            throw ForegroundBrowserError.noUsableCandidate
        }

        stop()
        generation += 1
        self.service = service
        self.grant = grant
        self.serviceCAPEM = serviceCAPEM
        iceBypassUntil = [:]

        let parameters = NWParameters.tcp
        parameters.allowLocalEndpointReuse = true
        let listener = try NWListener(using: parameters, on: .any)
        listener.newConnectionHandler = { [weak self] connection in
            self?.handleBrowserConnection(connection)
        }
        self.listener = listener
        let startup = ListenerStartup()
        listener.stateUpdateHandler = { state in
            switch state {
            case .ready:
                startup.succeed()
            case .failed(let error):
                startup.fail(ForegroundBrowserError.listenerFailed(error.localizedDescription))
            default:
                break
            }
        }
        listener.start(queue: queue)
        try await startup.wait()

        guard let port = listener.port else {
            throw ForegroundBrowserError.listenerUnavailable
        }
        let url = URL(string: "\(Self.localURLScheme(for: service))://127.0.0.1:\(port.rawValue)/")!
        print("Medium foreground proxy listening for \(service.id) at \(url.absoluteString)")
        ensureWarmedStream()
        return url
    }

    func stop() {
        generation += 1
        listener?.cancel()
        listener = nil
        serviceCAPEM = nil
        idleStreams.closeAll()
        warmingServices = []
        warmWaiters.rejectAll()
        activeConnections.values.forEach { pair in
            pair.browser.cancel()
            pair.medium.cancel()
        }
        activeUdpConnections.values.forEach { pair in
            pair.browser.cancel()
            pair.medium.close()
        }
        completedBrowserConnections.values.forEach { $0.cancel() }
        activeConnections = [:]
        activeUdpConnections = [:]
        iceBypassUntil = [:]
        completedBrowserConnections = [:]
        pipeStates = [:]
    }

    private func handleBrowserConnection(_ browser: NWConnection) {
        print("Medium foreground proxy accepted browser connection")
        guard let grant else {
            sendErrorPage(browser, status: "503 Service Unavailable", message: "Medium foreground proxy is not ready.")
            return
        }
        if let warmed = idleStreams.pop(serviceID: grant.serviceID) {
            print("Medium foreground proxy using warmed backend for \(grant.serviceID) via \(warmed.candidateDescription)")
            activatePreparedStream(browser, grant: grant, prepared: warmed, startBrowser: true)
            ensureWarmedStream()
            return
        }
        if service?.kind == .http, warmingServices.contains(grant.serviceID) {
            let currentGeneration = generation
            print("Medium foreground proxy waiting for warmed backend for \(grant.serviceID)")
            warmWaiters.add(serviceID: grant.serviceID) { [weak self] prepared in
                guard let self, self.generation == currentGeneration else {
                    prepared?.stream.close()
                    browser.cancel()
                    return
                }
                if let prepared {
                    print("Medium foreground proxy using just-warmed backend for \(grant.serviceID) via \(prepared.candidateDescription)")
                    self.activatePreparedStream(browser, grant: grant, prepared: prepared, startBrowser: true)
                } else {
                    print("Medium foreground proxy warmed backend unavailable for \(grant.serviceID), opening cold path")
                    self.handleBrowserConnectionCold(browser, grant: grant)
                }
            }
            return
        }
        handleBrowserConnectionCold(browser, grant: grant)
    }

    private func handleBrowserConnectionCold(_ browser: NWConnection, grant: SessionOpenGrant) {
        if shouldBypassIce(serviceID: grant.serviceID) {
            print("Medium foreground proxy skipping ICE UDP for \(grant.serviceID) after recent failure")
            handleBrowserConnectionViaTCP(browser, grant: grant)
            return
        }
        let iceChecklist = ForegroundUdpSession.iceChecklist(in: grant, preferred: selectedIceCandidates[grant.serviceID])
        if !iceChecklist.isEmpty {
            handleBrowserConnection(browser, grant: grant, iceChecklist: iceChecklist)
            return
        }
        handleBrowserConnectionViaTCP(browser, grant: grant)
    }

    private func shouldBypassIce(serviceID: String) -> Bool {
        guard let until = iceBypassUntil[serviceID] else {
            return false
        }
        if until > Date() {
            return true
        }
        iceBypassUntil[serviceID] = nil
        return false
    }

    private func handleBrowserConnection(_ browser: NWConnection, grant: SessionOpenGrant, iceChecklist: [ForegroundUdpCandidateSelection]) {
        let id = UUID()
        pipeStates[id] = ForegroundPipeState()
        browser.stateUpdateHandler = { [weak self] state in
            if case .failed = state {
                self?.close(id)
            }
        }
        browser.start(queue: queue)

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else {
                return
            }
            let currentService = self.service
            let currentServiceCAPEM = self.serviceCAPEM
            do {
                let outcome = try ForegroundUdpConnector.connectFirstSuccessful(
                    grant: grant,
                    iceChecklist: iceChecklist
                ) { iceSelection in
                    print("Medium foreground proxy trying ICE UDP candidate \(iceSelection.candidate.kind.rawValue) \(iceSelection.addr)")
                    return try ForegroundUdpSession.connect(grant: grant, selection: iceSelection)
                }
                let prepared = try Self.makePreparedStream(
                    for: grant,
                    service: currentService,
                    serviceCAPEM: currentServiceCAPEM,
                    medium: outcome.stream,
                    selectedIceCandidate: outcome.selection.candidate,
                    candidateDescription: "ice_udp \(outcome.selection.candidate.kind.rawValue) \(outcome.selection.addr)"
                )
                self.queue.async {
                    guard self.pipeStates[id] != nil else {
                        prepared.stream.close()
                        return
                    }
                    print("Medium foreground proxy connected ICE UDP candidate \(outcome.selection.candidate.kind.rawValue) \(outcome.selection.addr)")
                    self.selectedIceCandidates[grant.serviceID] = outcome.selection.candidate
                    self.iceBypassUntil[grant.serviceID] = nil
                    self.activatePreparedStream(browser, grant: grant, prepared: prepared, id: id, startBrowser: false)
                }
            } catch {
                print("Medium foreground proxy all ICE UDP candidates failed: \(error.localizedDescription)")
                self.queue.async {
                    guard self.pipeStates.removeValue(forKey: id) != nil else {
                        return
                    }
                    self.iceBypassUntil[grant.serviceID] = Date().addingTimeInterval(Self.iceFailureBypassSeconds)
                    self.handleBrowserConnectionViaTCP(browser, grant: grant, startBrowser: false)
                }
            }
        }
    }

    private func ensureWarmedStream() {
        guard let service,
              let grant,
              service.kind == .http,
              idleStreams.len(serviceID: grant.serviceID) == 0,
              !warmingServices.contains(grant.serviceID) else {
            return
        }
        let currentGeneration = generation
        let preferred = selectedIceCandidates[grant.serviceID]
        let skipIce = shouldBypassIce(serviceID: grant.serviceID)
        let currentServiceCAPEM = serviceCAPEM
        warmingServices.insert(grant.serviceID)

        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self else {
                return
            }
            do {
                let prepared = try Self.connectPreparedStream(
                    grant: grant,
                    service: service,
                    serviceCAPEM: currentServiceCAPEM,
                    preferredIceCandidate: preferred,
                    skipIce: skipIce
                )
                self.queue.async {
                    self.warmingServices.remove(grant.serviceID)
                    guard self.generation == currentGeneration,
                          self.grant?.sessionID == grant.sessionID else {
                        prepared.stream.close()
                        return
                    }
                    if let selected = prepared.selectedIceCandidate {
                        self.selectedIceCandidates[grant.serviceID] = selected
                        self.iceBypassUntil[grant.serviceID] = nil
                    }
                    if self.warmWaiters.resolve(serviceID: grant.serviceID, prepared: prepared) {
                        return
                    }
                    if self.idleStreams.push(prepared, serviceID: grant.serviceID) {
                        print("Medium foreground proxy warmed backend for \(grant.serviceID) via \(prepared.candidateDescription)")
                    }
                }
            } catch {
                self.queue.async {
                    self.warmingServices.remove(grant.serviceID)
                    guard self.generation == currentGeneration else {
                        return
                    }
                    self.warmWaiters.reject(serviceID: grant.serviceID)
                    print("Medium foreground proxy failed to warm backend for \(grant.serviceID): \(error.localizedDescription)")
                }
            }
        }
    }

    private func activatePreparedStream(
        _ browser: NWConnection,
        grant: SessionOpenGrant,
        prepared: ForegroundPreparedStream,
        startBrowser: Bool
    ) {
        let id = UUID()
        pipeStates[id] = ForegroundPipeState()
        browser.stateUpdateHandler = { [weak self] state in
            if case .failed = state {
                self?.close(id)
            }
        }
        activatePreparedStream(browser, grant: grant, prepared: prepared, id: id, startBrowser: startBrowser)
    }

    private func activatePreparedStream(
        _ browser: NWConnection,
        grant: SessionOpenGrant,
        prepared: ForegroundPreparedStream,
        id: UUID,
        startBrowser: Bool
    ) {
        if startBrowser {
            browser.start(queue: queue)
        }
        activeUdpConnections[id] = ForegroundActiveByteStreamConnection(
            browser: browser,
            medium: prepared.stream,
            serviceID: grant.serviceID,
            candidateDescription: prepared.candidateDescription
        )
        scheduleResponseTimeout(id, serviceID: grant.serviceID, candidateDescription: prepared.candidateDescription)
        pipeHTTPRequest(from: browser, to: prepared.stream, id: id, candidateDescription: prepared.candidateDescription)
    }

    private static func connectPreparedStream(
        grant: SessionOpenGrant,
        service: PublishedService?,
        serviceCAPEM: String?,
        preferredIceCandidate: IceCandidate?,
        skipIce: Bool
    ) throws -> ForegroundPreparedStream {
        if !skipIce {
            let iceChecklist = ForegroundUdpSession.iceChecklist(in: grant, preferred: preferredIceCandidate)
            if !iceChecklist.isEmpty {
                do {
                    let outcome = try ForegroundUdpConnector.connectFirstSuccessful(
                        grant: grant,
                        iceChecklist: iceChecklist
                    ) { iceSelection in
                        print("Medium foreground proxy warming ICE UDP candidate \(iceSelection.candidate.kind.rawValue) \(iceSelection.addr)")
                        return try ForegroundUdpSession.connect(grant: grant, selection: iceSelection)
                    }
                    return try makePreparedStream(
                        for: grant,
                        service: service,
                        serviceCAPEM: serviceCAPEM,
                        medium: outcome.stream,
                        selectedIceCandidate: outcome.selection.candidate,
                        candidateDescription: "ice_udp \(outcome.selection.candidate.kind.rawValue) \(outcome.selection.addr)"
                    )
                } catch {
                    print("Medium foreground proxy warm ICE UDP failed for \(grant.serviceID): \(error.localizedDescription)")
                }
            }
        }

        guard let candidate = bestCandidate(in: grant) else {
            throw ForegroundBrowserError.noUsableCandidate
        }
        let tcpStream = try connectTCPStream(grant: grant, candidate: candidate)
        return try makePreparedStream(
            for: grant,
            service: service,
            serviceCAPEM: serviceCAPEM,
            medium: tcpStream,
            selectedIceCandidate: nil,
            candidateDescription: "\(candidate.kind.rawValue) \(candidate.addr)"
        )
    }

    private static func connectTCPStream(grant: SessionOpenGrant, candidate: PeerCandidate) throws -> ForegroundTCPStream {
        let tcpStream = try ForegroundTCPStream.connect(to: candidate.addr)
        if candidate.kind == .relayTcp {
            try tcpStream.write(ForegroundRelayHello.makeFrame(for: grant))
            print("Medium foreground proxy sent relay hello for \(grant.nodeID)")
        }
        try tcpStream.write(ForegroundSessionHello.makeFrame(for: grant))
        print("Medium foreground proxy sent session hello for \(grant.serviceID) via \(candidate.kind.rawValue)")
        return tcpStream
    }

    private static func makePreparedStream(
        for grant: SessionOpenGrant,
        service: PublishedService?,
        serviceCAPEM: String?,
        medium: ForegroundByteStream,
        selectedIceCandidate: IceCandidate?,
        candidateDescription: String
    ) throws -> ForegroundPreparedStream {
        do {
            return ForegroundPreparedStream(
                stream: try makeForegroundStream(
                    for: grant,
                    service: service,
                    serviceCAPEM: serviceCAPEM,
                    medium: medium
                ),
                selectedIceCandidate: selectedIceCandidate,
                candidateDescription: candidateDescription
            )
        } catch {
            medium.close()
            throw error
        }
    }

    private func handleBrowserConnectionViaTCP(_ browser: NWConnection, grant: SessionOpenGrant, startBrowser: Bool = true) {
        guard let candidate = Self.bestCandidate(in: grant) else {
            sendErrorPage(browser, status: "502 Bad Gateway", message: "No usable Medium candidate is available.", startBrowser: startBrowser)
            return
        }

        if service?.kind == .http {
            connectForegroundTCPStream(browser, grant: grant, candidate: candidate, startBrowser: startBrowser)
            return
        }

        do {
            let endpoint = try tcpEndpoint(candidate.addr)
            let medium = NWConnection(to: endpoint, using: .tcp)
            let id = UUID()
            activeConnections[id] = (browser: browser, medium: medium)
            pipeStates[id] = ForegroundPipeState()
            scheduleResponseTimeout(id, serviceID: grant.serviceID, candidateDescription: "\(candidate.kind.rawValue) \(candidate.addr)")
            browser.stateUpdateHandler = { [weak self] state in
                if case .failed = state {
                    self?.close(id)
                }
            }
            medium.stateUpdateHandler = { [weak self] state in
                switch state {
                case .ready:
                    print("Medium foreground proxy connected Medium candidate \(candidate.addr)")
                    self?.sendHandshake(grant, candidate: candidate, to: medium) {
                        print("Medium foreground proxy sent session hello for \(grant.serviceID) via \(candidate.kind.rawValue)")
                        self?.pipe(from: browser, to: medium, id: id, direction: "browser->medium")
                        self?.pipe(from: medium, to: browser, id: id, direction: "medium->browser")
                    }
                case .failed(let error):
                    print("Medium foreground proxy candidate failed: \(error.localizedDescription)")
                    self?.close(id)
                default:
                    break
                }
            }
            if startBrowser {
                browser.start(queue: queue)
            }
            medium.start(queue: queue)
        } catch {
            sendErrorPage(browser, status: "502 Bad Gateway", message: error.localizedDescription, startBrowser: startBrowser)
        }
    }

    private func connectForegroundTCPStream(_ browser: NWConnection, grant: SessionOpenGrant, candidate: PeerCandidate, startBrowser: Bool) {
        let id = UUID()
        pipeStates[id] = ForegroundPipeState()
        browser.stateUpdateHandler = { [weak self] state in
            if case .failed = state {
                self?.close(id)
            }
        }
        if startBrowser {
            browser.start(queue: queue)
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else {
                return
            }
            let currentService = self.service
            let currentServiceCAPEM = self.serviceCAPEM
            do {
                let tcpStream = try Self.connectTCPStream(grant: grant, candidate: candidate)
                let prepared = try Self.makePreparedStream(
                    for: grant,
                    service: currentService,
                    serviceCAPEM: currentServiceCAPEM,
                    medium: tcpStream,
                    selectedIceCandidate: nil,
                    candidateDescription: "\(candidate.kind.rawValue) \(candidate.addr)"
                )
                self.queue.async {
                    guard self.pipeStates[id] != nil else {
                        prepared.stream.close()
                        return
                    }
                    print("Medium foreground proxy connected Medium TCP stream \(candidate.kind.rawValue) \(candidate.addr)")
                    self.activatePreparedStream(browser, grant: grant, prepared: prepared, id: id, startBrowser: false)
                }
            } catch {
                self.queue.async {
                    print("Medium foreground proxy TCP stream failed: \(error.localizedDescription)")
                    self.sendErrorPage(browser, status: "502 Bad Gateway", message: error.localizedDescription, startBrowser: false)
                    self.pipeStates.removeValue(forKey: id)
                }
            }
        }
    }

    private func sendHandshake(_ grant: SessionOpenGrant, candidate: PeerCandidate, to medium: NWConnection, then startPipes: @escaping () -> Void) {
        if candidate.kind == .relayTcp {
            sendRelayHello(grant, to: medium) { [weak self] in
                print("Medium foreground proxy sent relay hello for \(grant.nodeID)")
                self?.sendSessionHello(grant, to: medium, then: startPipes)
            }
        } else {
            sendSessionHello(grant, to: medium, then: startPipes)
        }
    }

    private func sendRelayHello(_ grant: SessionOpenGrant, to medium: NWConnection, then sendSession: @escaping () -> Void) {
        do {
            let frame = try ForegroundRelayHello.makeFrame(for: grant)
            medium.send(content: frame, completion: .contentProcessed { error in
                if error == nil {
                    sendSession()
                } else {
                    medium.cancel()
                }
            })
        } catch {
            medium.cancel()
        }
    }

    private func sendSessionHello(_ grant: SessionOpenGrant, to medium: NWConnection, then startPipes: @escaping () -> Void) {
        do {
            let frame = try ForegroundSessionHello.makeFrame(for: grant)
            medium.send(content: frame, completion: .contentProcessed { error in
                if error == nil {
                    startPipes()
                } else {
                    medium.cancel()
                }
            })
        } catch {
            medium.cancel()
        }
    }

    private func pipe(from source: NWConnection, to target: NWConnection, id: UUID, direction: String) {
        source.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] data, _, isComplete, error in
            guard let self else {
                return
            }
            if let data, !data.isEmpty {
                print("Medium foreground proxy \(direction) \(data.count) bytes")
                self.markPipeBytes(data.count, id: id, direction: direction)
                if direction == "medium->browser" {
                    self.cancelResponseTimeout(id)
                }
                target.send(content: self.rewriteHostHeaderIfNeeded(data), completion: .contentProcessed { sendError in
                    if sendError != nil {
                        print("Medium foreground proxy \(direction) send failed")
                        self.close(id)
                    }
                })
            }
            if isComplete || error != nil {
                if let error {
                    print("Medium foreground proxy \(direction) receive failed: \(error.localizedDescription)")
                    self.close(id)
                } else {
                    print("Medium foreground proxy \(direction) receive complete")
                    if direction == "medium->browser", self.pipeBytes(id, direction: direction) == 0 {
                        self.sendActiveErrorPage(
                            id,
                            status: "502 Bad Gateway",
                            message: "Medium path closed before the service returned a response. Check node and relay logs for this session."
                        )
                        return
                    }
                    if self.markPipeCompleted(id, direction: direction) {
                        self.close(id)
                    }
                }
                return
            }
            self.pipe(from: source, to: target, id: id, direction: direction)
        }
    }

    private func pipeHTTPRequest(
        from browser: NWConnection,
        to medium: ForegroundByteStream,
        id: UUID,
        candidateDescription: String,
        buffered: Data = Data()
    ) {
        browser.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] data, _, isComplete, error in
            guard let self else {
                return
            }
            if let error {
                print("Medium foreground proxy browser->\(candidateDescription) receive failed: \(error.localizedDescription)")
                self.close(id)
                return
            }
            var request = buffered
            if let data, !data.isEmpty {
                print("Medium foreground proxy browser->\(candidateDescription) \(data.count) bytes")
                self.markPipeBytes(data.count, id: id, direction: "browser->medium")
                request.append(self.rewriteHostHeaderIfNeeded(data))
            }
            if isComplete || self.isCompleteHTTPRequest(request) {
                DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                    do {
                        try medium.write(request)
                        self?.readByteStreamResponse(
                            from: medium,
                            to: browser,
                            id: id,
                            candidateDescription: candidateDescription
                        )
                    } catch {
                        self?.queue.async {
                            print("Medium foreground proxy \(candidateDescription) write failed: \(error.localizedDescription)")
                            self?.sendActiveErrorPage(id, status: "502 Bad Gateway", message: "Medium path write failed through \(candidateDescription): \(error.localizedDescription)")
                        }
                    }
                }
                return
            }
            self.pipeHTTPRequest(
                from: browser,
                to: medium,
                id: id,
                candidateDescription: candidateDescription,
                buffered: request
            )
        }
    }

    private func readByteStreamResponse(
        from medium: ForegroundByteStream,
        to browser: NWConnection,
        id: UUID,
        candidateDescription: String
    ) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var firstResponseChunk = true
            do {
                while true {
                    let data = try medium.read(maxLength: 64 * 1024)
                    if data.isEmpty {
                        self?.queue.async {
                            print("Medium foreground proxy \(candidateDescription)->browser receive complete")
                            if self?.pipeBytes(id, direction: "medium->browser") == 0 {
                                self?.sendActiveErrorPage(
                                    id,
                                    status: "502 Bad Gateway",
                                    message: "Medium path \(candidateDescription) closed before the service returned a response. Check node logs for this session."
                                )
                            } else {
                                self?.close(id)
                            }
                        }
                        return
                    }
                    let response: ForegroundBrowserResponseChunk
                    if firstResponseChunk {
                        firstResponseChunk = false
                        response = Self.browserResponseChunk(fromFirstChunk: data)
                    } else {
                        response = ForegroundBrowserResponseChunk(data: data, closeAfterSend: false)
                    }
                    self?.queue.async {
                        print("Medium foreground proxy \(candidateDescription)->browser \(response.data.count) bytes")
                        self?.markPipeBytes(response.data.count, id: id, direction: "medium->browser")
                        self?.cancelResponseTimeout(id)
                        let sendComplete = response.closeAfterSend
                        browser.send(content: response.data, contentContext: .defaultMessage, isComplete: sendComplete, completion: .contentProcessed { sendError in
                            if sendError != nil {
                                print("Medium foreground proxy \(candidateDescription)->browser send failed")
                                self?.close(id)
                            } else if response.closeAfterSend {
                                print("Medium foreground proxy closed synthetic one-shot response")
                                self?.finishUdpOneShot(id)
                            }
                        })
                    }
                    if response.closeAfterSend {
                        return
                    }
                }
            } catch {
                self?.queue.async {
                    print("Medium foreground proxy \(candidateDescription) read failed: \(error.localizedDescription)")
                    if self?.pipeBytes(id, direction: "medium->browser") == 0 {
                        self?.sendActiveErrorPage(id, status: "504 Gateway Timeout", message: "Medium path \(candidateDescription) did not return a response: \(error.localizedDescription)")
                    } else {
                        self?.close(id)
                    }
                }
            }
        }
    }

    static func browserResponseData(fromFirstChunk data: Data) -> Data {
        browserResponseChunk(fromFirstChunk: data).data
    }

    static func browserResponseChunk(fromFirstChunk data: Data) -> ForegroundBrowserResponseChunk {
        guard !startsWithHTTPStatusLine(data) else {
            return ForegroundBrowserResponseChunk(data: data, closeAfterSend: false)
        }
        let header = """
        HTTP/1.1 200 OK\r
        content-type: text/plain; charset=utf-8\r
        content-length: \(data.count)\r
        connection: keep-alive\r
        \r

        """
        var response = Data(header.utf8)
        response.append(data)
        return ForegroundBrowserResponseChunk(data: response, closeAfterSend: false)
    }

    private static func startsWithHTTPStatusLine(_ data: Data) -> Bool {
        data.starts(with: Data("HTTP/".utf8))
    }

    private func isCompleteHTTPRequest(_ data: Data) -> Bool {
        guard let request = String(data: data, encoding: .utf8),
              let headerEnd = request.range(of: "\r\n\r\n") else {
            return false
        }
        let headers = request[..<headerEnd.lowerBound]
        let bodyStart = headerEnd.upperBound
        let bodyBytes = request[bodyStart...].utf8.count
        for line in headers.split(separator: "\r\n") {
            let parts = line.split(separator: ":", maxSplits: 1)
            if parts.count == 2, parts[0].trimmingCharacters(in: .whitespaces).lowercased() == "content-length" {
                let expected = Int(parts[1].trimmingCharacters(in: .whitespaces)) ?? 0
                return bodyBytes >= expected
            }
        }
        return true
    }

    private func markPipeBytes(_ count: Int, id: UUID, direction: String) {
        guard var state = pipeStates[id] else {
            return
        }
        state.addBytes(count, direction: direction)
        pipeStates[id] = state
    }

    private func pipeBytes(_ id: UUID, direction: String) -> Int {
        pipeStates[id]?.bytes(for: direction) ?? 0
    }

    private func markPipeCompleted(_ id: UUID, direction: String) -> Bool {
        guard var state = pipeStates[id] else {
            return true
        }
        let shouldClose = state.markCompleted(direction)
        pipeStates[id] = state
        return shouldClose
    }

    private func scheduleResponseTimeout(_ id: UUID, serviceID: String, candidate: PeerCandidate) {
        scheduleResponseTimeout(id, serviceID: serviceID, candidateDescription: "\(candidate.kind.rawValue) \(candidate.addr)")
    }

    private func scheduleResponseTimeout(_ id: UUID, serviceID: String, candidateDescription: String) {
        let timeout = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            guard (self.activeConnections[id] != nil || self.activeUdpConnections[id] != nil),
                  self.pipeBytes(id, direction: "medium->browser") == 0 else {
                return
            }
            print("Medium foreground proxy response timeout for \(serviceID) via \(candidateDescription)")
            self.sendActiveErrorPage(
                id,
                status: "504 Gateway Timeout",
                message: "Medium did not receive a response from \(serviceID) through \(candidateDescription). Check relay and node logs for this session."
            )
        }
        responseTimeouts[id] = timeout
        queue.asyncAfter(deadline: .now() + 12, execute: timeout)
    }

    private func cancelResponseTimeout(_ id: UUID) {
        responseTimeouts.removeValue(forKey: id)?.cancel()
    }

    private static func makeForegroundStream(
        for grant: SessionOpenGrant,
        service: PublishedService?,
        serviceCAPEM: String?,
        medium: ForegroundByteStream
    ) throws -> ForegroundByteStream {
        guard service?.kind == .http else {
            return medium
        }
        guard serviceCAPEM?.isEmpty == false else {
            throw ForegroundSecureSessionError.invalidServiceCA
        }
        print("Medium foreground service TLS connecting for \(grant.serviceID) as \(service?.mediumHostname ?? grant.serviceID)")
        return try ForegroundSecureSession(
            transport: medium,
            serverName: service?.mediumHostname ?? "\(grant.serviceID).medium",
            serviceCAPEM: serviceCAPEM
        )
    }

    private func rewriteHostHeaderIfNeeded(_ data: Data) -> Data {
        guard let service,
              let request = String(data: data, encoding: .utf8),
              let range = request.range(of: "\r\nHost: ", options: [.caseInsensitive]) else {
            return data
        }
        let afterHost = request[range.upperBound...]
        guard let lineEnd = afterHost.range(of: "\r\n") else {
            return data
        }
        var rewritten = request
        rewritten.replaceSubrange(range.upperBound..<lineEnd.lowerBound, with: service.mediumHostname)
        return Data(rewritten.utf8)
    }

    private func close(_ id: UUID) {
        guard let pair = activeConnections.removeValue(forKey: id) else {
            if let udpPair = activeUdpConnections.removeValue(forKey: id) {
                pipeStates.removeValue(forKey: id)
                cancelResponseTimeout(id)
                udpPair.browser.cancel()
                udpPair.medium.close()
                if grant?.serviceID == udpPair.serviceID {
                    ensureWarmedStream()
                }
            } else {
                pipeStates.removeValue(forKey: id)
                cancelResponseTimeout(id)
            }
            return
        }
        pipeStates.removeValue(forKey: id)
        cancelResponseTimeout(id)
        pair.browser.cancel()
        pair.medium.cancel()
    }

    private func finishUdpOneShot(_ id: UUID) {
        guard let udpPair = activeUdpConnections.removeValue(forKey: id) else {
            pipeStates.removeValue(forKey: id)
            cancelResponseTimeout(id)
            return
        }
        pipeStates.removeValue(forKey: id)
        cancelResponseTimeout(id)
        udpPair.medium.close()
        if grant?.serviceID == udpPair.serviceID {
            ensureWarmedStream()
        }
        completedBrowserConnections[id] = udpPair.browser
        queue.asyncAfter(deadline: .now() + 2) { [weak self] in
            self?.completedBrowserConnections.removeValue(forKey: id)?.cancel()
        }
    }

    private func sendErrorPage(_ browser: NWConnection, status: String, message: String, startBrowser: Bool = true) {
        if startBrowser {
            browser.start(queue: queue)
        }
        browser.send(content: errorResponse(status: status, message: message), completion: .contentProcessed { _ in
            browser.cancel()
        })
    }

    private func sendActiveErrorPage(_ id: UUID, status: String, message: String) {
        guard let browser = activeConnections[id]?.browser ?? activeUdpConnections[id]?.browser else {
            return
        }
        browser.send(content: errorResponse(status: status, message: message), completion: .contentProcessed { [weak self] _ in
            self?.close(id)
        })
    }

    private func errorResponse(status: String, message: String) -> Data {
        let body = """
        <html><body style="font-family:-apple-system;background:#111;color:#eee;padding:24px">
        <h1>Medium foreground browser</h1>
        <p>\(escapeHTML(message))</p>
        </body></html>
        """
        let response = """
        HTTP/1.1 \(status)\r
        content-type: text/html; charset=utf-8\r
        content-length: \(Data(body.utf8).count)\r
        connection: close\r
        \r
        \(body)
        """
        return Data(response.utf8)
    }

    static func bestCandidate(in grant: SessionOpenGrant) -> PeerCandidate? {
        grant.authorization.candidates
            .filter { $0.kind == .directTcp || $0.kind == .relayTcp }
            .filter { isUsableForegroundCandidate($0.addr) }
            .sorted { $0.priority > $1.priority }
            .first
    }

    private static func isUsableForegroundCandidate(_ address: String) -> Bool {
        let host = address.split(separator: ":", maxSplits: 1).first.map(String.init) ?? address
        if let ipv4 = IPv4Address(host) {
            let octets = ipv4.rawValue
            return !(octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        }
        return true
    }

    private func tcpEndpoint(_ address: String) throws -> NWEndpoint {
        let parts = address.split(separator: ":", maxSplits: 1).map(String.init)
        guard parts.count == 2, let port = NWEndpoint.Port(parts[1]) else {
            throw ForegroundBrowserError.invalidCandidateAddress(address)
        }
        return .hostPort(host: NWEndpoint.Host(parts[0]), port: port)
    }

    private func escapeHTML(_ value: String) -> String {
        value
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }
}

private final class ListenerStartup {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Error>?
    private var result: Result<Void, Error>?

    func wait() async throws {
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            if let result {
                lock.unlock()
                continuation.resume(with: result)
                return
            }
            self.continuation = continuation
            lock.unlock()
        }
    }

    func succeed() {
        resume(.success(()))
    }

    func fail(_ error: Error) {
        resume(.failure(error))
    }

    private func resume(_ result: Result<Void, Error>) {
        lock.lock()
        if let continuation {
            self.continuation = nil
            lock.unlock()
            continuation.resume(with: result)
            return
        }
        if self.result == nil {
            self.result = result
        }
        lock.unlock()
    }
}
