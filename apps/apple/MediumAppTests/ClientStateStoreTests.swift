import XCTest
@testable import Medium

final class ClientStateStoreTests: XCTestCase {
    func testMemoryStateStoreRoundTripsClientState() throws {
        let store = MemoryClientStateStore()
        let state = MediumClientState(
            controlURL: URL(string: "https://control.example.test")!,
            deviceName: "iphone",
            inviteVersion: 1,
            security: "pinned-tls",
            controlPin: "sha256:abc",
            serviceCAPEM: "-----BEGIN CERTIFICATE-----\nAQIDBA==\n-----END CERTIFICATE-----"
        )

        try store.save(state)

        XCTAssertEqual(try store.load(), state)
    }

    func testClientStateDecodesLegacyStateWithoutServiceCA() throws {
        let json = """
        {
          "controlURL": "https://control.example.test",
          "deviceName": "iphone",
          "inviteVersion": 1,
          "security": "pinned-tls",
          "controlPin": "sha256:abc"
        }
        """.data(using: .utf8)!

        let state = try JSONDecoder.medium.decode(MediumClientState.self, from: json)

        XCTAssertNil(state.serviceCAPEM)
    }
}
