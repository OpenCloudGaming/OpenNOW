import AuthenticationServices
import SwiftUI

struct LoginView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(spacing: 14) {
                        BrandLogoView(size: 72)
                        Text("OpenNOW")
                            .font(.largeTitle.bold())
                        Text("Cloud gaming, open source.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 24)
                }

                if let error = store.lastError {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(.red)
                    }
                }

                Section {
                    if store.providers.count > 1 {
                        Picker("Provider", selection: $store.settings.selectedProviderIdpId) {
                            ForEach(store.providers) { provider in
                                Text(provider.displayName).tag(provider.idpId)
                            }
                        }
                    }

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
                            Text(store.isAuthenticating ? "Connecting" : "Sign In with NVIDIA")
                        }
                    }
                    .disabled(store.isAuthenticating || !store.supportsNativeOAuth)
                }
            }
            .listStyle(.insetGrouped)
            .navigationTitle("Sign In")
        }
    }

    private func handleSignIn() {
        Haptics.medium()
        Task { await store.signIn() }
    }
}

#Preview {
    LoginView()
        .environmentObject(OpenNOWStore())
}
