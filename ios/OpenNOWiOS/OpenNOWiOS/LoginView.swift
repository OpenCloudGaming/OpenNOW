import AuthenticationServices
import CoreImage.CIFilterBuiltins
import SwiftUI

struct LoginView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var signInTask: Task<Void, Never>?
    @State private var showingProviderChooser = false
    private let qrContext = CIContext()

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 14) {
                        BrandLogoView(size: 68)
                        Text("Your games. Anywhere.")
                            .font(.system(size: 34, weight: .bold, design: .rounded))
                        Text("Sign in to GeForce NOW to browse your library and start playing.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 24)
                }

                if let error = store.authError {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(.red)
                    }
                }

                if let prompt = store.deviceLoginPrompt {
                    Section("Finish sign-in") {
                        ViewThatFits(in: .horizontal) {
                            HStack(alignment: .center, spacing: 24) {
                                qrImage(for: prompt).frame(width: 180, height: 180)
                                codeInstructions(prompt)
                            }
                            VStack(alignment: .leading, spacing: 16) {
                                qrImage(for: prompt).frame(width: 180, height: 180)
                                codeInstructions(prompt)
                            }
                        }
                        .padding(.vertical, 12)
                        HStack {
                            ProgressView()
                            Text(store.authenticationPhase ?? "Waiting for approval")
                                .foregroundStyle(.secondary)
                            Spacer()
                            Button("Cancel") { signInTask?.cancel() }
                        }
                    }
                } else {
                    Section {
                    Button {
                        showingProviderChooser = true
                    } label: {
                        HStack {
                            Label("Choose Provider", systemImage: "network")
                            Spacer()
                            Text(store.providers.first(where: { $0.idpId == store.settings.selectedProviderIdpId })?.displayName ?? "NVIDIA")
                                .foregroundStyle(.secondary)
                            Image(systemName: "chevron.right")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .disabled(store.isAuthenticating)

                    #if os(tvOS)
                    if !store.tvAuthLogs.isEmpty {
                        DisclosureGroup("Authentication Log") {
                            ForEach(Array(store.tvAuthLogs.enumerated()), id: \.offset) { _, line in
                                Text(line)
                                    .font(.footnote.monospaced())
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    #else
                    if !store.supportsNativeOAuth {
                        Label("Sign in is unavailable in this build.", systemImage: "lock.slash")
                            .foregroundStyle(.secondary)
                    }
                    #endif

                    Button {
                        handleSignIn()
                    } label: {
                        HStack {
                            if store.isAuthenticating {
                                ProgressView()
                            } else {
                                Image(systemName: store.supportsNativeOAuth ? "person.badge.key" : "lock.slash")
                            }
                            Text(store.isAuthenticating ? "Connecting" : "Sign In with \(selectedProviderName)")
                        }
                    }
                    .disabled(store.isAuthenticating || !store.supportsNativeOAuth)

                    if (store.providers.first { $0.idpId == store.settings.selectedProviderIdpId }?.code ?? "NVIDIA").uppercased() == "NVIDIA" {
                        Button {
                            Haptics.medium()
                            signInTask?.cancel()
                            signInTask = Task { await store.signInWithCode() }
                        } label: {
                            Label("Sign in with a QR code", systemImage: "qrcode")
                        }
                        .disabled(store.isAuthenticating)
                    }

                    if let phase = store.authenticationPhase {
                        Label(phase, systemImage: "hourglass")
                            .foregroundStyle(.secondary)
                    }
                    }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .navigationTitle("Sign In")
            .navigationBarTitleDisplayMode(.inline)
            .background {
                RadialGradient(colors: [brandAccent.opacity(0.13), .clear], center: .topLeading,
                    startRadius: 20, endRadius: 520).ignoresSafeArea()
            }
            .sheet(isPresented: $showingProviderChooser) {
                ProviderChooserSheet(
                    selectedProviderId: store.settings.selectedProviderIdpId,
                    onContinue: { provider in store.selectProvider(provider) }
                )
                .environmentObject(store)
            }
        }
    }

    private func handleSignIn() {
        Haptics.medium()
        signInTask?.cancel()
        signInTask = Task { await store.signIn() }
    }

    private var selectedProviderName: String {
        store.providers.first(where: { $0.idpId == store.settings.selectedProviderIdpId })?.displayName ?? "NVIDIA"
    }

    private func codeInstructions(_ prompt: DeviceLoginPrompt) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Scan with your phone or open the link, then enter this code.")
                .font(.subheadline).foregroundStyle(.secondary)
            Text(prompt.userCode)
                .font(.system(size: 28, weight: .bold, design: .monospaced))
                .tracking(2).textSelection(.enabled)
            Link(destination: prompt.verificationURL) {
                Label("Open NVIDIA sign-in", systemImage: "arrow.up.right.square")
            }
            Text("Keep this screen open while approving sign-in.")
                .font(.footnote).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func qrImage(for prompt: DeviceLoginPrompt) -> some View {
        Group {
            if let image = makeQRCode(prompt.verificationURL.absoluteString) {
                Image(uiImage: image).interpolation(.none).resizable().scaledToFit()
            } else {
                Image(systemName: "qrcode").resizable().scaledToFit()
            }
        }
        .padding(12)
        .background(.white, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .accessibilityLabel("QR code for NVIDIA sign-in")
    }

    private func makeQRCode(_ value: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(value.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage,
              let image = qrContext.createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: image)
    }
}

struct ProviderChooserSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: OpenNOWStore
    let selectedProviderId: String
    let onContinue: (LoginProvider) -> Void
    @State private var choiceId = ""

    private var choices: [LoginProvider] {
        store.providers
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(choices) { provider in
                        Button {
                            choiceId = provider.idpId
                        } label: {
                            HStack {
                                Text(provider.displayName)
                                    .foregroundStyle(.primary)
                                Spacer()
                                if choiceId == provider.idpId {
                                    Image(systemName: "checkmark.circle.fill")
                                        .foregroundStyle(.tint)
                                }
                            }
                        }
                    }
                } footer: {
                    Text("Choose the GeForce NOW service that holds your account.")
                }
            }
            .navigationTitle("Choose Provider")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Continue") {
                        guard let provider = choices.first(where: { $0.idpId == choiceId }) else { return }
                        onContinue(provider)
                        dismiss()
                    }
                    .disabled(!choices.contains(where: { $0.idpId == choiceId }))
                }
            }
            .task {
                choiceId = selectedProviderId
                await store.refreshProviders()
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }
}

#Preview {
    LoginView()
        .environmentObject(OpenNOWStore())
}
