import XCTest
@testable import Medium

final class MediumAPITests: XCTestCase {
    func testDecodesServiceCatalog() throws {
        let json = """
        {
          "devices": [
            {
              "id": "node-1",
              "name": "office-server",
              "ssh": null,
              "services": [
                {
                  "id": "svc_openclaw",
                  "kind": "https",
                  "schema_version": 1,
                  "label": "OpenClaw",
                  "target": "127.0.0.1:3000",
                  "user_name": null
                }
              ]
            }
          ]
        }
        """.data(using: .utf8)!

        let catalog = try JSONDecoder.medium.decode(DeviceCatalog.self, from: json)

        XCTAssertEqual(catalog.devices.first?.name, "office-server")
        XCTAssertEqual(catalog.devices.first?.services.first?.id, "svc_openclaw")
        XCTAssertEqual(catalog.devices.first?.services.first?.displayName, "OpenClaw")
    }

    func testDecodesHttpServiceForForegroundBrowser() throws {
        let json = """
        {
          "devices": [
            {
              "id": "node-1",
              "name": "office-server",
              "ssh": null,
              "services": [
                {
                  "id": "hello",
                  "kind": "http",
                  "schema_version": 1,
                  "label": "Hello",
                  "target": "127.0.0.1:8082",
                  "user_name": null
                }
              ]
            }
          ]
        }
        """.data(using: .utf8)!

        let catalog = try JSONDecoder.medium.decode(DeviceCatalog.self, from: json)
        let service = try XCTUnwrap(catalog.devices.first?.services.first)

        XCTAssertEqual(service.kind, .http)
        XCTAssertEqual(service.mediumHostname, "hello.medium")
    }

    func testDecodesHttpsServiceForForegroundBrowser() throws {
        let json = """
        {
          "devices": [
            {
              "id": "node-1",
              "name": "office-server",
              "ssh": null,
              "services": [
                {
                  "id": "openclaw",
                  "kind": "https",
                  "schema_version": 1,
                  "label": "OpenClaw",
                  "target": "127.0.0.1:8443",
                  "user_name": null
                }
              ]
            }
          ]
        }
        """.data(using: .utf8)!

        let catalog = try JSONDecoder.medium.decode(DeviceCatalog.self, from: json)
        let service = try XCTUnwrap(catalog.devices.first?.services.first)

        XCTAssertEqual(service.kind, .https)
        XCTAssertTrue(service.supportsForegroundBrowser)
        XCTAssertEqual(ForegroundBrowserProxy.localURLScheme(for: service), "https")
    }

    func testBuildsSessionOpenRequestURL() throws {
        let client = MediumAPIClient(state: MediumClientState(
            controlURL: URL(string: "https://control.example.test:7777")!,
            deviceName: "iphone",
            inviteVersion: 1,
            security: "pinned-tls",
            controlPin: "sha256:abc",
            serviceCAPEM: nil
        ))

        let request = try client.makeOpenSessionRequest(serviceID: "svc openclaw")

        XCTAssertEqual(request.url?.absoluteString, "https://control.example.test:7777/api/sessions/open?service_id=svc%20openclaw&requester_device_id=iphone")
    }

    func testMediumServiceCAPEMIsDecodedToDERBytes() throws {
        let pem = """
        -----BEGIN CERTIFICATE-----
        AQIDBA==
        -----END CERTIFICATE-----
        """

        let der = try ForegroundSecureSession.certificateDER(fromPEM: pem)

        XCTAssertEqual(Array(der), [1, 2, 3, 4])
    }

    func testDecodesSessionGrantWithWSSRelayCandidate() throws {
        let json = """
        {
          "session_id": "session-wss",
          "service_id": "svc_web",
          "node_id": "node-1",
          "relay_hint": "wss://relay.example.com/medium/v1/relay",
          "authorization": {
            "token": "token-wss",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": [
              {
                "kind": "direct_tcp",
                "addr": "198.51.100.10:17001",
                "priority": 100
              },
              {
                "kind": "wss_relay",
                "addr": "wss://relay.example.com/medium/v1/relay",
                "priority": 10
              }
            ]
          }
        }
        """.data(using: .utf8)!

        let grant = try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)

        XCTAssertEqual(grant.authorization.candidates.last?.kind, .wssRelay)
    }

    func testBuildsIOSForegroundSessionHelloFrameUsesDefaultMediumTransport() throws {
        let json = """
        {
          "session_id": "session-direct",
          "service_id": "hello",
          "node_id": "node-1",
          "relay_hint": null,
          "authorization": {
            "token": "token-direct",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": [
              {
                "kind": "direct_tcp",
                "addr": "198.51.100.10:17001",
                "priority": 100
              }
            ]
          }
        }
        """.data(using: .utf8)!
        let grant = try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)

        let frame = try ForegroundSessionHello.makeFrame(for: grant)
        let payload = try XCTUnwrap(String(data: frame, encoding: .utf8))

        XCTAssertTrue(payload.hasSuffix("\n"))
        XCTAssertTrue(payload.contains("\"token\":\"token-direct\""))
        XCTAssertTrue(payload.contains("\"service_id\":\"hello\""))
        XCTAssertFalse(payload.contains("\"transport\""))
    }

    func testBuildsForegroundRelayHelloFrame() throws {
        let json = """
        {
          "session_id": "session-relay",
          "service_id": "hello",
          "node_id": "node-1",
          "relay_hint": null,
          "authorization": {
            "token": "token-relay",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": [
              {
                "kind": "relay_tcp",
                "addr": "94.242.58.217:7001",
                "priority": 10
              }
            ]
          }
        }
        """.data(using: .utf8)!
        let grant = try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)

        let frame = try ForegroundRelayHello.makeFrame(for: grant)
        let payload = try XCTUnwrap(String(data: frame, encoding: .utf8))

        XCTAssertTrue(payload.hasSuffix("\n"))
        XCTAssertTrue(payload.contains("\"role\":\"client\""))
        XCTAssertTrue(payload.contains("\"node_id\":\"node-1\""))
    }

    func testSelectsForegroundIceUdpCandidateBeforeTcpFallback() throws {
        let json = """
        {
          "session_id": "session-ice",
          "service_id": "hello",
          "node_id": "node-1",
          "relay_hint": null,
          "authorization": {
            "token": "token-ice",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": [
              {
                "kind": "relay_tcp",
                "addr": "94.242.58.217:7001",
                "priority": 10
              }
            ],
            "ice": {
              "credentials": {
                "ufrag": "ufrag",
                "pwd": "pwd",
                "expires_at": "2099-01-01T00:00:00Z"
              },
              "candidates": [
                {
                  "foundation": "virtual",
                  "component": 1,
                  "transport": "udp",
                  "priority": 999,
                  "addr": "198.18.0.1",
                  "port": 17002,
                  "kind": "host",
                  "related_addr": null,
                  "related_port": null
                },
                {
                  "foundation": "relay-udp-1",
                  "component": 1,
                  "transport": "udp",
                  "priority": 10,
                  "addr": "94.242.58.217",
                  "port": 7001,
                  "kind": "relay",
                  "related_addr": null,
                  "related_port": null
                }
              ]
            }
          }
        }
        """.data(using: .utf8)!
        let grant = try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)

        let selection = try XCTUnwrap(ForegroundUdpSession.bestIceCandidate(in: grant))

        XCTAssertEqual(selection.addr, "94.242.58.217:7001")
        XCTAssertEqual(selection.candidate.kind, .relay)
    }

    func testForegroundPipeStateClosesAfterBothDirectionsComplete() {
        var state = ForegroundPipeState()

        XCTAssertFalse(state.markCompleted("browser->medium"))
        XCTAssertTrue(state.markCompleted("medium->browser"))
    }

    func testForegroundIceChecklistKeepsFallbackCandidates() throws {
        let json = """
        {
          "session_id": "session-ice",
          "service_id": "hello",
          "node_id": "node-1",
          "relay_hint": null,
          "authorization": {
            "token": "token-ice",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": [
              {
                "kind": "relay_tcp",
                "addr": "94.242.58.217:7001",
                "priority": 10
              }
            ],
            "ice": {
              "credentials": {
                "ufrag": "ufrag",
                "pwd": "pwd",
                "expires_at": "2099-01-01T00:00:00Z"
              },
              "candidates": [
                {
                  "foundation": "host-1",
                  "component": 1,
                  "transport": "udp",
                  "priority": 300,
                  "addr": "192.168.88.88",
                  "port": 17002,
                  "kind": "host",
                  "related_addr": null,
                  "related_port": null
                },
                {
                  "foundation": "srflx-1",
                  "component": 1,
                  "transport": "udp",
                  "priority": 200,
                  "addr": "203.0.113.10",
                  "port": 17002,
                  "kind": "srflx",
                  "related_addr": "192.168.88.88",
                  "related_port": 17002
                },
                {
                  "foundation": "relay-1",
                  "component": 1,
                  "transport": "udp",
                  "priority": 100,
                  "addr": "94.242.58.217",
                  "port": 7001,
                  "kind": "relay",
                  "related_addr": null,
                  "related_port": null
                }
              ]
            }
          }
        }
        """.data(using: .utf8)!
        let grant = try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)

        let checklist = ForegroundUdpSession.iceChecklist(in: grant)

        XCTAssertEqual(checklist.map(\.addr), [
            "192.168.88.88:17002",
            "203.0.113.10:17002",
            "94.242.58.217:7001"
        ])
    }

    func testForegroundIceChecklistPrefersPreviouslySelectedCandidate() throws {
        let json = """
        {
          "session_id": "session-ice",
          "service_id": "hello",
          "node_id": "node-1",
          "relay_hint": null,
          "authorization": {
            "token": "token-ice",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": [],
            "ice": {
              "credentials": {
                "ufrag": "ufrag",
                "pwd": "pwd",
                "expires_at": "2099-01-01T00:00:00Z"
              },
              "candidates": [
                {
                  "foundation": "host-1",
                  "component": 1,
                  "transport": "udp",
                  "priority": 300,
                  "addr": "192.168.88.88",
                  "port": 17002,
                  "kind": "host",
                  "related_addr": null,
                  "related_port": null
                },
                {
                  "foundation": "relay-1",
                  "component": 1,
                  "transport": "udp",
                  "priority": 100,
                  "addr": "94.242.58.217",
                  "port": 7001,
                  "kind": "relay",
                  "related_addr": null,
                  "related_port": null
                }
              ]
            }
          }
        }
        """.data(using: .utf8)!
        let grant = try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)
        let preferred = try XCTUnwrap(grant.authorization.ice?.candidates.last)

        let checklist = ForegroundUdpSession.iceChecklist(in: grant, preferred: preferred)

        XCTAssertEqual(checklist.first?.addr, "94.242.58.217:7001")
        XCTAssertEqual(checklist.first?.candidate.kind, .relay)
    }

    func testForegroundIdleStreamPoolReturnsOnlyOneWarmedStreamPerService() throws {
        let pool = ForegroundIdleStreamPool(maxPerService: 1)
        let first = FakeForegroundByteStream()
        let second = FakeForegroundByteStream()

        XCTAssertTrue(pool.push(first, serviceID: "hello"))
        XCTAssertFalse(pool.push(second, serviceID: "hello"))

        XCTAssertTrue(pool.pop(serviceID: "hello")?.stream === first)
        XCTAssertNil(pool.pop(serviceID: "hello"))
        XCTAssertFalse(first.closed)
        XCTAssertTrue(second.closed)
    }

    func testForegroundIceRaceReturnsFastestSuccessfulCandidate() throws {
        let slow = ForegroundUdpCandidateSelection(
            candidate: IceCandidate(
                foundation: "slow",
                component: 1,
                transport: "udp",
                priority: 300,
                addr: "192.168.88.88",
                port: 17002,
                kind: .host,
                relatedAddr: nil,
                relatedPort: nil
            ),
            addr: "192.168.88.88:17002"
        )
        let fast = ForegroundUdpCandidateSelection(
            candidate: IceCandidate(
                foundation: "fast",
                component: 1,
                transport: "udp",
                priority: 100,
                addr: "94.242.58.217",
                port: 7001,
                kind: .relay,
                relatedAddr: nil,
                relatedPort: nil
            ),
            addr: "94.242.58.217:7001"
        )

        let outcome = try ForegroundUdpConnector.connectFirstSuccessful(
            grant: Self.sessionGrant(),
            iceChecklist: [slow, fast]
        ) { selection in
            if selection.addr == slow.addr {
                Thread.sleep(forTimeInterval: 0.2)
            }
            return FakeForegroundByteStream()
        }

        XCTAssertEqual(outcome.selection.addr, fast.addr)
    }

    func testForegroundWarmWaitersGivePreparedStreamToOnlyFirstWaiter() {
        let waiters = ForegroundWarmWaiters()
        let stream = FakeForegroundByteStream()
        var firstReceived: ForegroundPreparedStream?
        var secondReceived: ForegroundPreparedStream?

        waiters.add(serviceID: "hello") { firstReceived = $0 }
        waiters.add(serviceID: "hello") { secondReceived = $0 }

        let consumed = waiters.resolve(
            serviceID: "hello",
            prepared: ForegroundPreparedStream(
                stream: stream,
                selectedIceCandidate: nil,
                candidateDescription: "relay_tcp 94.242.58.217:7001"
            )
        )

        XCTAssertTrue(consumed)
        XCTAssertTrue(firstReceived?.stream === stream)
        XCTAssertNil(secondReceived)
        XCTAssertEqual(waiters.count(serviceID: "hello"), 0)
    }

    func testForegroundPipeStateTracksDirectionBytes() {
        var state = ForegroundPipeState()

        XCTAssertEqual(state.bytes(for: "medium->browser"), 0)

        state.addBytes(128, direction: "medium->browser")
        state.addBytes(64, direction: "medium->browser")

        XCTAssertEqual(state.bytes(for: "medium->browser"), 192)
        XCTAssertEqual(state.bytes(for: "browser->medium"), 0)
    }

    func testForegroundBrowserWrapsRawServiceBodyAsHTTPResponse() throws {
        let response = ForegroundBrowserProxy.browserResponseChunk(fromFirstChunk: Data("hello\n".utf8))
        let text = try XCTUnwrap(String(data: response.data, encoding: .utf8))

        XCTAssertFalse(response.closeAfterSend)
        XCTAssertTrue(text.hasPrefix("HTTP/1.1 200 OK\r\n"))
        XCTAssertTrue(text.contains("content-length: 6\r\n"))
        XCTAssertTrue(text.contains("connection: keep-alive\r\n"))
        XCTAssertTrue(text.hasSuffix("\r\n\r\nhello\n"))
    }

    func testForegroundBrowserPreservesHTTPServiceResponse() throws {
        let original = Data("HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n".utf8)
        let response = ForegroundBrowserProxy.browserResponseChunk(fromFirstChunk: original)

        XCTAssertEqual(response.data, original)
        XCTAssertFalse(response.closeAfterSend)
    }

    func testMediumWebViewIgnoresFrameInterruptedNavigationError() {
        let error = NSError(domain: "WebKitErrorDomain", code: 102)

        XCTAssertFalse(MediumWebView.Coordinator.shouldReportNavigationError(error))
    }

    func testForegroundBrowserSkipsOverlayVirtualCandidate() throws {
        let json = """
        {
          "session_id": "session-direct",
          "service_id": "hello",
          "node_id": "node-1",
          "relay_hint": null,
          "authorization": {
            "token": "token-direct",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": [
              {
                "kind": "direct_tcp",
                "addr": "198.18.0.1:17001",
                "priority": 100
              },
              {
                "kind": "relay_tcp",
                "addr": "94.242.58.217:7001",
                "priority": 10
              }
            ]
          }
        }
        """.data(using: .utf8)!
        let grant = try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)

        let candidate = try XCTUnwrap(ForegroundBrowserProxy.bestCandidate(in: grant))

        XCTAssertEqual(candidate.addr, "94.242.58.217:7001")
    }

    private static func sessionGrant() throws -> SessionOpenGrant {
        let json = """
        {
          "session_id": "session-ice",
          "service_id": "hello",
          "node_id": "node-1",
          "relay_hint": null,
          "authorization": {
            "token": "token-ice",
            "expires_at": "2099-01-01T00:00:00Z",
            "candidates": []
          }
        }
        """.data(using: .utf8)!
        return try JSONDecoder.medium.decode(SessionOpenGrant.self, from: json)
    }
}

private final class FakeForegroundByteStream: ForegroundByteStream {
    private(set) var closed = false

    func write(_ bytes: Data) throws {}

    func read(maxLength: Int) throws -> Data {
        Data()
    }

    func close() {
        closed = true
    }
}
