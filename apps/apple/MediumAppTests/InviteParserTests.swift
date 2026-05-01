import XCTest
@testable import Medium

final class InviteParserTests: XCTestCase {
    func testParsesPinnedTlsJoinInvite() throws {
        let invite = try JoinInvite.parse("medium://join?v=1&control=https://control.example.test:7777&security=pinned-tls&control_pin=sha256:abc")

        XCTAssertEqual(invite.version, 1)
        XCTAssertEqual(invite.controlURL.absoluteString, "https://control.example.test:7777")
        XCTAssertEqual(invite.security, "pinned-tls")
        XCTAssertEqual(invite.controlPin, "sha256:abc")
    }

    func testRejectsUnsupportedInviteScheme() {
        XCTAssertThrowsError(try JoinInvite.parse("https://control.example.test"))
    }
}
