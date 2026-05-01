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
            controlPin: "sha256:abc"
        )

        try store.save(state)

        XCTAssertEqual(try store.load(), state)
    }
}
