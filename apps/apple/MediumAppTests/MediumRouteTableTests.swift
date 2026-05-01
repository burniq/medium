import XCTest
@testable import Medium

final class MediumRouteTableTests: XCTestCase {
    func testAssignsStableVirtualAddressForService() throws {
        var table = MediumRouteTable(subnetBase: "100.96.0.0")

        let first = try table.assign(nodeID: "node-1", serviceID: "svc_web", port: 443)
        let second = try table.assign(nodeID: "node-1", serviceID: "svc_web", port: 443)

        XCTAssertEqual(first.address, second.address)
        XCTAssertEqual(first.hostname, "svc-web.node-1.medium")
    }

    func testSanitizesHostnameLabels() throws {
        var table = MediumRouteTable(subnetBase: "100.96.0.0")

        let service = try table.assign(nodeID: "Node_One", serviceID: "API_Service", port: 8443)

        XCTAssertEqual(service.hostname, "api-service.node-one.medium")
    }
}
