import SwiftUI

struct BootstrapView: View {
    @StateObject private var model = MediumAppModel()

    var body: some View {
        NavigationStack {
            ZStack {
                MediumScreenBackground()
                if model.state == nil {
                    JoinView(model: model)
                } else {
                    ServicesView(model: model)
                }
            }
            .navigationTitle("Medium")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(MediumPalette.background, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
            .toolbarColorScheme(.dark, for: .navigationBar)
            #endif
            .alert("Medium", isPresented: Binding(
                get: { model.errorMessage != nil },
                set: { if !$0 { model.errorMessage = nil } }
            )) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(model.errorMessage ?? "")
            }
        }
    }
}

struct JoinView: View {
    @ObservedObject var model: MediumAppModel
    @State private var inviteText = ""
    @State private var deviceName = PlatformDevice.defaultName

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                MediumHero(
                    eyebrow: nil,
                    title: "Join",
                    subtitle: "Paste a medium://join invite. This device will store the control certificate pin locally."
                )

                MediumCard {
                    VStack(alignment: .leading, spacing: 14) {
                        FieldHeader(title: "Device", subtitle: "This name will identify the phone in your Medium network.")
                        TextField("Device name", text: $deviceName)
                            .textFieldStyle(.plain)
                            .font(.system(.body, design: .default).weight(.semibold))
                            .mediumInputChrome()
                            .autocorrectionDisabled()
                    }
                }

                MediumCard {
                    VStack(alignment: .leading, spacing: 14) {
                        FieldHeader(title: "Invite", subtitle: "Expected format starts with medium://join.")
                        TextEditor(text: $inviteText)
                            .font(.system(.callout, design: .monospaced))
                            .frame(minHeight: 190)
                            .scrollContentBackground(.hidden)
                            .mediumInputChrome()
                            .autocorrectionDisabled()
                    }
                }

                PrimaryActionButton(
                    title: model.isLoading ? "Joining..." : "Join Device",
                    systemImage: "link.badge.plus",
                    isDisabled: inviteText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.isLoading
                ) {
                    Task { await model.join(inviteText: inviteText, deviceName: deviceName) }
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 30)
            .padding(.bottom, 34)
        }
        .scrollIndicators(.hidden)
    }
}

struct ServicesView: View {
    @ObservedObject var model: MediumAppModel

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 18) {
                MediumHero(
                    eyebrow: nil,
                    title: "Services",
                    subtitle: "Published endpoints available to this node."
                )

                if let state = model.state {
                    MediumCard {
                        VStack(alignment: .leading, spacing: 14) {
                            SectionTitle("Control")
                            InfoRow(title: "URL", value: state.controlURL.absoluteString)
                            InfoRow(title: "Device", value: state.deviceName)
                        }
                    }
                }

                MediumCard {
                    VStack(alignment: .leading, spacing: 14) {
                        SectionTitle("Foreground Browser")
                        StatusPill(text: "Tap an HTTP service to open it inside Medium. This temporary mode works only while the app stays open.", systemImage: "globe.badge.chevron.backward")
                    }
                }

                if model.devices.isEmpty && !model.isLoading {
                    EmptyServicesCard()
                }

                ForEach(model.devices) { device in
                    MediumCard {
                        VStack(alignment: .leading, spacing: 14) {
                            SectionTitle(device.name)
                            if device.services.isEmpty {
                                Text("No published services")
                                    .font(.callout)
                                    .foregroundStyle(MediumPalette.secondaryText)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                            ForEach(device.services) { service in
                                ServiceCard(service: service, isEnabled: service.kind != .ssh) {
                                    Task { await model.open(service: service) }
                                }
                            }
                        }
                    }
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 28)
            .padding(.bottom, 34)
        }
        .scrollIndicators(.hidden)
        .refreshable {
            await model.refreshDevices()
        }
        .toolbar {
            ToolbarItem {
                Button("Reset", role: .destructive) {
                    model.reset()
                }
            }
            ToolbarItem {
                Button {
                    Task { await model.refreshDevices() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
            }
        }
        .sheet(item: Binding(
            get: { model.selectedGrant.map(SessionGrantSheet.init(grant:)) },
            set: { if $0 == nil { model.selectedGrant = nil } }
        )) { sheet in
            SessionGrantView(grant: sheet.grant)
        }
        .sheet(item: Binding(
            get: { model.browserSession },
            set: { if $0 == nil { model.closeBrowser() } }
        )) { session in
            ForegroundBrowserView(session: session) {
                model.closeBrowser()
            }
        }
        .task {
            if model.devices.isEmpty {
                await model.refreshDevices()
            }
        }
    }
}

struct ForegroundBrowserView: View {
    let session: ForegroundBrowserSession
    let onClose: () -> Void
    @State private var navigationError: String?

    var body: some View {
        NavigationStack {
            ZStack(alignment: .top) {
                MediumWebView(url: session.localURL) { message in
                    navigationError = message
                }
                .ignoresSafeArea(edges: .bottom)

                if let navigationError {
                    Text(navigationError)
                        .font(.footnote)
                        .foregroundStyle(MediumPalette.background)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                        .background(MediumPalette.accent)
                        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                        .padding(.top, 12)
                        .padding(.horizontal, 16)
                }
            }
            .navigationTitle(session.service.displayName)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(MediumPalette.background, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
            .toolbarColorScheme(.dark, for: .navigationBar)
            #endif
            .toolbar {
                ToolbarItem {
                    Button("Close") {
                        onClose()
                    }
                }
            }
        }
    }
}

struct SessionGrantSheet: Identifiable {
    let grant: SessionOpenGrant
    var id: String { grant.sessionID }
}

struct SessionGrantView: View {
    let grant: SessionOpenGrant

    var body: some View {
        NavigationStack {
            ZStack {
                MediumScreenBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 18) {
                MediumHero(
                    eyebrow: "Session",
                    title: "Service grant",
                    subtitle: "Connection candidates returned by the control plane."
                        )

                        MediumCard {
                            VStack(alignment: .leading, spacing: 14) {
                                SectionTitle("Session")
                                InfoRow(title: "Service", value: grant.serviceID)
                                InfoRow(title: "Node", value: grant.nodeID)
                                if let relayHint = grant.relayHint {
                                    InfoRow(title: "Relay", value: relayHint)
                                }
                            }
                        }

                        MediumCard {
                            VStack(alignment: .leading, spacing: 14) {
                                SectionTitle("Candidates")
                                ForEach(grant.authorization.candidates) { candidate in
                                    InfoRow(title: candidate.kind.rawValue, value: candidate.addr)
                                }
                            }
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.top, 24)
                    .padding(.bottom, 34)
                }
                .scrollIndicators(.hidden)
            }
            .navigationTitle("Service Session")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(MediumPalette.background, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
            .toolbarColorScheme(.dark, for: .navigationBar)
            #endif
        }
    }
}

private enum MediumPalette {
    static let background = Color(red: 0.027, green: 0.031, blue: 0.035)
    static let backgroundUpper = Color(red: 0.055, green: 0.063, blue: 0.070)
    static let ink = Color(red: 0.94, green: 0.95, blue: 0.94)
    static let secondaryText = Color(red: 0.58, green: 0.62, blue: 0.61)
    static let mutedText = Color(red: 0.39, green: 0.43, blue: 0.42)
    static let surface = Color(red: 0.085, green: 0.096, blue: 0.105)
    static let surfaceStrong = Color(red: 0.115, green: 0.128, blue: 0.136)
    static let stroke = Color(red: 0.22, green: 0.25, blue: 0.25)
    static let input = Color(red: 0.062, green: 0.070, blue: 0.077)
    static let inputStroke = Color(red: 0.26, green: 0.30, blue: 0.29)
    static let accent = Color(red: 0.70, green: 0.86, blue: 0.18)
    static let accentDark = Color(red: 0.55, green: 0.69, blue: 0.13)
    static let danger = Color(red: 0.68, green: 0.16, blue: 0.12)
}

private struct MediumScreenBackground: View {
    var body: some View {
        ZStack {
            LinearGradient(
                colors: [
                    MediumPalette.backgroundUpper,
                    MediumPalette.background
                ],
                startPoint: .top,
                endPoint: .bottom
            )

            VStack(spacing: 0) {
                Rectangle()
                    .fill(MediumPalette.accent.opacity(0.12))
                    .frame(height: 2)
                Spacer()
                Rectangle()
                    .fill(Color.white.opacity(0.035))
                    .frame(height: 1)
                    .padding(.bottom, 96)
            }
        }
        .ignoresSafeArea()
    }
}

private struct MediumHero: View {
    let eyebrow: String?
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let eyebrow {
                Text(eyebrow.uppercased())
                    .font(.system(size: 11, weight: .bold, design: .monospaced))
                    .tracking(2.0)
                    .foregroundStyle(MediumPalette.accent)
            }
            Text(title)
                .font(.system(size: 34, weight: .heavy, design: .default))
                .foregroundStyle(MediumPalette.ink)
                .fixedSize(horizontal: false, vertical: true)
            Text(subtitle)
                .font(.system(.callout, design: .default))
                .foregroundStyle(MediumPalette.secondaryText)
                .lineSpacing(1)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct MediumCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                Rectangle()
                    .fill(MediumPalette.surface)
                    .overlay(alignment: .top) {
                        Rectangle()
                            .fill(Color.white.opacity(0.045))
                            .frame(height: 1)
                    }
            )
            .overlay(Rectangle().stroke(MediumPalette.stroke, lineWidth: 1))
            .shadow(color: Color.black.opacity(0.24), radius: 18, x: 0, y: 10)
    }
}

private extension View {
    func mediumInputChrome() -> some View {
        self
            .padding(13)
            .background(
                Rectangle()
                    .fill(MediumPalette.input)
                    .overlay(alignment: .top) {
                        Rectangle()
                            .fill(Color.white.opacity(0.055))
                            .frame(height: 1)
                    }
            )
            .overlay(Rectangle().stroke(MediumPalette.inputStroke, lineWidth: 1))
    }
}

private struct FieldHeader: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(.headline, design: .default).weight(.bold))
                .foregroundStyle(MediumPalette.ink)
            Text(subtitle)
                .font(.system(.caption, design: .default))
                .foregroundStyle(MediumPalette.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct SectionTitle: View {
    let title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
        Text(title)
            .font(.system(.headline, design: .default).weight(.heavy))
            .foregroundStyle(MediumPalette.ink)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct StatusPill: View {
    let text: String
    let systemImage: String

    var body: some View {
        Label(text, systemImage: systemImage)
            .font(.system(.callout, design: .default).weight(.semibold))
            .foregroundStyle(MediumPalette.ink)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(MediumPalette.surfaceStrong)
            .overlay(Rectangle().stroke(MediumPalette.stroke, lineWidth: 1))
    }
}

private struct EmptyServicesCard: View {
    var body: some View {
        MediumCard {
            VStack(alignment: .leading, spacing: 10) {
                Image(systemName: "network")
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(MediumPalette.accent)
                Text("No services loaded")
                    .font(.headline)
                    .foregroundStyle(MediumPalette.ink)
                Text("Pull to refresh or tap Refresh after joining the network.")
                    .font(.callout)
                    .foregroundStyle(MediumPalette.secondaryText)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

private struct ServiceCard: View {
    let service: PublishedService
    let isEnabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(alignment: .top, spacing: 12) {
                ZStack {
                    Rectangle()
                        .fill(MediumPalette.surface)
                        .overlay(Rectangle().stroke(MediumPalette.stroke, lineWidth: 1))
                    Image(systemName: service.kind == .ssh ? "terminal" : "globe")
                        .font(.headline)
                        .foregroundStyle(MediumPalette.accent)
                }
                .frame(width: 44, height: 44)

                VStack(alignment: .leading, spacing: 7) {
                    Text(service.displayName)
                        .font(.headline)
                        .foregroundStyle(MediumPalette.ink)
                    Text(service.kind.rawValue.uppercased())
                        .font(.caption.weight(.bold))
                        .tracking(0.8)
                        .foregroundStyle(MediumPalette.accent)
                    Text(service.target)
                        .font(.caption.monospaced())
                        .foregroundStyle(MediumPalette.secondaryText)
                        .lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(MediumPalette.secondaryText.opacity(0.7))
                    .padding(.top, 4)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(MediumPalette.surfaceStrong)
            .overlay(Rectangle().stroke(MediumPalette.stroke, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
    }
}

private struct PrimaryActionButton: View {
    let title: String
    let systemImage: String
    let isDisabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .font(.headline)
                .foregroundStyle(isDisabled ? MediumPalette.mutedText : Color.black)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 16)
                .background(isDisabled ? MediumPalette.surfaceStrong : MediumPalette.accent)
                .overlay(Rectangle().stroke(isDisabled ? MediumPalette.stroke : MediumPalette.accent, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .disabled(isDisabled)
    }
}

private struct CompactActionButton: View {
    let title: String
    let systemImage: String
    let isPrimary: Bool
    let isDisabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .font(.callout.weight(.bold))
                .foregroundStyle(isPrimary ? Color.black : MediumPalette.ink)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 13)
                .background(isPrimary ? MediumPalette.accent : MediumPalette.surfaceStrong)
                .overlay(Rectangle().stroke(isPrimary ? MediumPalette.accent : MediumPalette.stroke, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .disabled(isDisabled)
        .opacity(isDisabled ? 0.55 : 1)
    }
}

private struct InfoRow: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(MediumPalette.secondaryText)
            Text(value)
                .font(.callout.monospaced())
                .foregroundStyle(MediumPalette.ink)
                .textSelection(.enabled)
                .lineLimit(4)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 4)
    }
}
