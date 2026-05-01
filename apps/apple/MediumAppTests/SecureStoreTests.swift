import XCTest
@testable import Medium

final class SecureStoreTests: XCTestCase {
    func testDeviceLabelKeyIsStable() {
        XCTAssertEqual(SecureStore.deviceLabelKey, "device_label")
    }
}
