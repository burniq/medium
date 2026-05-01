import Foundation
#if os(iOS)
import UIKit
#endif

enum PlatformDevice {
    static var defaultName: String {
        #if os(iOS)
        UIDevice.current.name
        #else
        Host.current().localizedName ?? "medium-apple-client"
        #endif
    }
}
