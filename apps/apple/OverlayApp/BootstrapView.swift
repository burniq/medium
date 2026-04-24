import SwiftUI

struct BootstrapView: View {
    var body: some View {
        VStack(spacing: 16) {
            Text("Pair this device")
                .font(.title2.weight(.semibold))
            Text("Native Apple shell for the overlay client.")
                .foregroundStyle(.secondary)
        }
        .padding(24)
    }
}
