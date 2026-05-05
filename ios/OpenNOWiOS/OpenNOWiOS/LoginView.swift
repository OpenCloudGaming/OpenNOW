import AuthenticationServices
import SwiftUI

struct LoginView: View {
    @EnvironmentObject private var store: OpenNOWStore
    #if os(tvOS)
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    #endif

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            ambientOrbs

            ScrollView {
                VStack(spacing: 40) {
                    Spacer(minLength: 60)
                    brandHeader
                    loginCard
                    footerNote
                    Spacer(minLength: 40)
                }
                .padding(.horizontal, 28)
            }
        }
        .preferredColorScheme(.dark)
    }

    // MARK: - Ambient background orbs

    private var ambientOrbs: some View {
        ZStack {
            Circle()
                .fill(Color(red: 0.46, green: 0.72, blue: 0.0).opacity(0.25))
                .frame(width: 320, height: 320)
                .blur(radius: 80)
                .offset(x: -80, y: -180)
            Circle()
                .fill(Color(red: 0.0, green: 0.72, blue: 0.55).opacity(0.18))
                .frame(width: 260, height: 260)
                .blur(radius: 70)
                .offset(x: 120, y: 200)
        }
        .ignoresSafeArea()
    }

    // MARK: - Brand header

    private var brandHeader: some View {
        VStack(spacing: 12) {
            BrandLogoView(size: 82)
            .shadow(color: Color(red: 0.46, green: 0.72, blue: 0.0).opacity(0.6), radius: 20)

            Text("OpenNOW")
                .font(.system(size: 34, weight: .bold, design: .rounded))
                .foregroundStyle(.white)

            Text("Cloud gaming, open source.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(0.55))
        }
    }

    // MARK: - Login card

    private var loginCard: some View {
        VStack(spacing: 24) {
            VStack(spacing: 6) {
                Text("Sign In")
                    .font(.title2.bold())
                    .foregroundStyle(.white)
                Text("Connect your NVIDIA account to access GeForce NOW.")
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(0.55))
                    .multilineTextAlignment(.center)
            }

            if let error = store.lastError {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                    Text(error)
                        .font(.footnote)
                        .foregroundStyle(.red)
                        .multilineTextAlignment(.leading)
                    Spacer()
                }
                .padding(12)
                .background(.red.opacity(0.12), in: RoundedRectangle(cornerRadius: 10))
            }

            #if os(tvOS)
            VStack(alignment: .leading, spacing: 12) {
                Text("Apple TV sign-in is still being debugged here. Recent auth logs appear below.")
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(0.7))
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)

                if !store.tvAuthLogs.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Auth Logs")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white.opacity(0.65))
                            .textCase(.uppercase)
                            .kerning(0.5)

                        VStack(alignment: .leading, spacing: 6) {
                            ForEach(Array(store.tvAuthLogs.enumerated()), id: \.offset) { _, line in
                                Text(line)
                                    .font(.system(size: 13, weight: .medium, design: .monospaced))
                                    .foregroundStyle(.white.opacity(0.82))
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .background(.black.opacity(0.22), in: RoundedRectangle(cornerRadius: 12))
                    }
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity)
            .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
            #else
            if !store.supportsNativeOAuth {
                Text("This build still needs a native NVIDIA sign-in flow.")
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(0.7))
                    .multilineTextAlignment(.center)
                    .padding(12)
                    .frame(maxWidth: .infinity)
                    .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
            }
            #endif

            if store.providers.count > 1 {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Provider")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white.opacity(0.55))
                        .textCase(.uppercase)
                        .kerning(0.5)

                    Picker("Provider", selection: $store.settings.selectedProviderIdpId) {
                        ForEach(store.providers) { provider in
                            Text(provider.displayName).tag(provider.idpId)
                        }
                    }
                    .pickerStyle(.menu)
                    .tint(brandAccent)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
                }
            }

            Button {
                handleSignIn()
            } label: {
                HStack(spacing: 10) {
                    if store.isAuthenticating {
                        ProgressView()
                            .progressViewStyle(.circular)
                            .tint(.white)
                            .scaleEffect(0.9)
                    } else if store.supportsNativeOAuth {
                        Image(systemName: "bolt.fill")
                            .font(.body.weight(.semibold))
                    } else {
                        Image(systemName: "lock.slash")
                            .font(.body.weight(.semibold))
                    }
                    Text(
                        store.isAuthenticating
                            ? "Connecting…"
                            : (store.supportsNativeOAuth ? "Sign In with NVIDIA" : "Sign In Unavailable")
                    )
                        .font(.body.weight(.semibold))
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 16)
                .foregroundStyle(.white)
                .background(
                    Group {
                        if store.isAuthenticating {
                            LinearGradient(
                                colors: [brandAccent.opacity(0.6), Color(red: 0.0, green: 0.72, blue: 0.55).opacity(0.6)],
                                startPoint: .leading,
                                endPoint: .trailing
                            )
                        } else {
                            brandGradient
                        }
                    }
                )
                .clipShape(RoundedRectangle(cornerRadius: 14))
                .shadow(color: brandAccent.opacity(0.4), radius: store.isAuthenticating ? 0 : 12)
                .animation(.easeOut(duration: 0.2), value: store.isAuthenticating)
            }
            .disabled(store.isAuthenticating || !store.supportsNativeOAuth)
            .buttonStyle(.plain)
        }
        .padding(28)
        .background {
            if #available(iOS 26, *) {
                RoundedRectangle(cornerRadius: 24)
                    .fill(.regularMaterial)
                    .glassEffect(in: RoundedRectangle(cornerRadius: 24))
            } else {
                RoundedRectangle(cornerRadius: 24)
                    .fill(.white.opacity(0.08))
                    .overlay(
                        RoundedRectangle(cornerRadius: 24)
                            .strokeBorder(.white.opacity(0.12), lineWidth: 1)
                    )
            }
        }
    }

    // MARK: - Footer

    private var footerNote: some View {
        Text("Open-source cloud gaming client · Not affiliated with NVIDIA")
            .font(.caption2)
            .foregroundStyle(.white.opacity(0.3))
            .multilineTextAlignment(.center)
    }

    private func handleSignIn() {
        Haptics.medium()
        #if os(tvOS)
        Task {
            await store.signInOnTVOS { url, callbackScheme in
                try await webAuthenticationSession.authenticate(
                    using: url,
                    callbackURLScheme: callbackScheme,
                    preferredBrowserSession: .shared
                )
            }
        }
        #else
        Task { await store.signIn() }
        #endif
    }
}

#Preview {
    LoginView()
        .environmentObject(OpenNOWStore())
}
