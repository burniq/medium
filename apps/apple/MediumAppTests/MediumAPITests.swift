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

    func testBuildsSessionOpenRequestURL() throws {
        let client = MediumAPIClient(state: MediumClientState(
            controlURL: URL(string: "https://control.example.test:7777")!,
            deviceName: "iphone",
            inviteVersion: 1,
            security: "pinned-tls",
            controlPin: "sha256:abc"
        ))

        let request = try client.makeOpenSessionRequest(serviceID: "svc openclaw")

        XCTAssertEqual(request.url?.absoluteString, "https://control.example.test:7777/api/sessions/open?service_id=svc%20openclaw&requester_device_id=iphone")
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
}
