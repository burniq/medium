import XCTest
@testable import OverlayMac

final class SecureStoreTests: XCTestCase {
    func testDeviceLabelKeyIsStable() {
        XCTAssertEqual(SecureStore.deviceLabelKey, "device_label")
    }
}
