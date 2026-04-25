import AuthenticationServices
import CryptoKit
import Foundation
import Network
import OSLog
import SwiftUI
import UIKit

struct UserProfile: Codable, Equatable {
    let userId: String
    var displayName: String
    var email: String?
    var membershipTier: String
}

struct LoginProvider: Codable, Equatable, Identifiable {
    var id: String { idpId }
    let idpId: String
    let code: String
    let displayName: String
    let streamingServiceUrl: String
    let priority: Int
}

struct AuthTokens: Codable, Equatable {
    let accessToken: String
    let refreshToken: String?
    let idToken: String?
    let expiresAt: TimeInterval
    let clientToken: String?
    let clientTokenExpiresAt: TimeInterval?
}

struct AuthSession: Codable, Equatable {
    let provider: LoginProvider
    let tokens: AuthTokens
    let user: UserProfile
}

struct CloudGame: Identifiable, Codable, Equatable {
    let id: String
    let title: String
    let genre: String
    let platform: String
    let icon: String
    let imageUrl: String?
    let launchAppId: String?
    let launchOptions: [GameLaunchOption]
    let uuid: String?
    let summary: String?
    let longDescription: String?
    let publisher: String?
    let developer: String?
    let releaseDate: String?
    let featureLabels: [String]?
    let tags: [String]?
    let stores: [String]?
    let playType: String?
    let membershipTierLabel: String?
}

struct GameLaunchOption: Identifiable, Codable, Equatable {
    var id: String { "\(storefront)-\(appId)" }
    let storefront: String
    let appId: String
    let supportedControls: [String]?
}

struct SessionTelemetry: Codable, Equatable {
    var pingMs: Int
    var fps: Int
    var packetLossPercent: Double
    var bitrateMbps: Double
}

struct IceServerConfig: Codable, Equatable {
    let urls: [String]
    let username: String?
    let credential: String?
}

struct ActiveSession: Identifiable, Codable, Equatable {
    let id: String
    let game: CloudGame
    let startedAt: Date
    var status: Int
    var queuePosition: Int?
    var seatSetupStep: Int?
    var serverIp: String?
    var mediaIp: String?
    var mediaPort: Int
    var signalingServer: String?
    var signalingUrl: String?
    var iceServers: [IceServerConfig]
    let zone: String
    let streamingBaseUrl: String
    let clientId: String
    let deviceId: String
    var adState: SessionAdState?
}

struct RemoteSessionCandidate: Identifiable, Codable, Equatable {
    let id: String
    let appId: String?
    let status: Int
    let serverIp: String?
}

struct SubscriptionSnapshot: Codable, Equatable {
    let membershipTier: String
    let isGamePlayAllowed: Bool
    let isUnlimited: Bool
    let remainingHours: Double
    let totalHours: Double
}

struct SessionAdMediaFile: Codable, Equatable {
    let mediaFileUrl: String?
    let encodingProfile: String?
}

struct SessionOpportunityInfo: Codable, Equatable {
    let state: String?
    let queuePaused: Bool?
    let gracePeriodSeconds: Int?
    let message: String?
    let title: String?
    let description: String?
}

struct SessionAdInfo: Codable, Equatable, Identifiable {
    let adId: String
    let state: Int?
    let adState: Int?
    let adUrl: String?
    let mediaUrl: String?
    let adMediaFiles: [SessionAdMediaFile]
    let clickThroughUrl: String?
    let adLengthInSeconds: Double?
    let durationMs: Int?
    let title: String?
    let description: String?

    var id: String { adId }
}

struct SessionAdState: Codable, Equatable {
    let isAdsRequired: Bool
    let sessionAdsRequired: Bool?
    let isQueuePaused: Bool?
    let gracePeriodSeconds: Int?
    let message: String?
    let sessionAds: [SessionAdInfo]
    let ads: [SessionAdInfo]
    let opportunity: SessionOpportunityInfo?
    let serverSentEmptyAds: Bool?
}

struct AppSettings: Codable, Equatable {
    var preferredRegion: String
    var preferredFPS: Int
    var preferredQuality: String
    var preferredCodec: String
    var keepMicEnabled: Bool
    var showStatsOverlay: Bool
    var selectedProviderIdpId: String
    var fortnitePrefersNativeTouch: Bool
    var touchControlLayouts: [String: TouchControlLayout]
    var streamerPreferences: StreamerPreferences

    enum CodingKeys: String, CodingKey {
        case preferredRegion
        case preferredFPS
        case preferredQuality
        case preferredCodec
        case keepMicEnabled
        case showStatsOverlay
        case selectedProviderIdpId
        case fortnitePrefersNativeTouch
        case touchControlLayouts
        case streamerPreferences
    }

    init(
        preferredRegion: String,
        preferredFPS: Int,
        preferredQuality: String,
        preferredCodec: String,
        keepMicEnabled: Bool,
        showStatsOverlay: Bool,
        selectedProviderIdpId: String,
        fortnitePrefersNativeTouch: Bool,
        touchControlLayouts: [String: TouchControlLayout],
        streamerPreferences: StreamerPreferences
    ) {
        self.preferredRegion = preferredRegion
        self.preferredFPS = preferredFPS
        self.preferredQuality = preferredQuality
        self.preferredCodec = preferredCodec
        self.keepMicEnabled = keepMicEnabled
        self.showStatsOverlay = showStatsOverlay
        self.selectedProviderIdpId = selectedProviderIdpId
        self.fortnitePrefersNativeTouch = fortnitePrefersNativeTouch
        self.touchControlLayouts = touchControlLayouts
        self.streamerPreferences = streamerPreferences
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        preferredRegion = try container.decodeIfPresent(String.self, forKey: .preferredRegion) ?? "Auto"
        preferredFPS = try container.decodeIfPresent(Int.self, forKey: .preferredFPS) ?? 60
        preferredQuality = try container.decodeIfPresent(String.self, forKey: .preferredQuality) ?? "Balanced"
        preferredCodec = try container.decodeIfPresent(String.self, forKey: .preferredCodec) ?? "Auto"
        keepMicEnabled = try container.decodeIfPresent(Bool.self, forKey: .keepMicEnabled) ?? false
        showStatsOverlay = try container.decodeIfPresent(Bool.self, forKey: .showStatsOverlay) ?? true
        selectedProviderIdpId = try container.decodeIfPresent(String.self, forKey: .selectedProviderIdpId)
            ?? "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg"
        fortnitePrefersNativeTouch = try container.decodeIfPresent(Bool.self, forKey: .fortnitePrefersNativeTouch) ?? true
        touchControlLayouts = try container.decodeIfPresent([String: TouchControlLayout].self, forKey: .touchControlLayouts)
            ?? TouchControlLayout.defaultProfiles
        streamerPreferences = try container.decodeIfPresent(StreamerPreferences.self, forKey: .streamerPreferences)
            ?? .default
    }

    static let `default` = AppSettings(
        preferredRegion: "Auto",
        preferredFPS: 60,
        preferredQuality: "Balanced",
        preferredCodec: "Auto",
        keepMicEnabled: false,
        showStatsOverlay: true,
        selectedProviderIdpId: "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg",
        fortnitePrefersNativeTouch: true,
        touchControlLayouts: TouchControlLayout.defaultProfiles,
        streamerPreferences: .default
    )

    func touchLayout(for profile: String) -> TouchControlLayout {
        touchControlLayouts[profile] ?? TouchControlLayout.preset(for: profile)
    }
}

struct StreamerPreferences: Codable, Equatable {
    var audioMuted: Bool
    var showStatsClock: Bool
    var showStatsBattery: Bool
    var touchControllerVisible: Bool
    var physicalControllerPassthrough: Bool

    enum CodingKeys: String, CodingKey {
        case audioMuted
        case showStatsClock
        case showStatsBattery
        case touchControllerVisible
        case physicalControllerPassthrough
    }

    init(
        audioMuted: Bool,
        showStatsClock: Bool,
        showStatsBattery: Bool,
        touchControllerVisible: Bool,
        physicalControllerPassthrough: Bool
    ) {
        self.audioMuted = audioMuted
        self.showStatsClock = showStatsClock
        self.showStatsBattery = showStatsBattery
        self.touchControllerVisible = touchControllerVisible
        self.physicalControllerPassthrough = physicalControllerPassthrough
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        audioMuted = try container.decodeIfPresent(Bool.self, forKey: .audioMuted) ?? Self.default.audioMuted
        showStatsClock = try container.decodeIfPresent(Bool.self, forKey: .showStatsClock) ?? Self.default.showStatsClock
        showStatsBattery = try container.decodeIfPresent(Bool.self, forKey: .showStatsBattery) ?? Self.default.showStatsBattery
        touchControllerVisible = try container.decodeIfPresent(Bool.self, forKey: .touchControllerVisible) ?? Self.default.touchControllerVisible
        physicalControllerPassthrough = try container.decodeIfPresent(Bool.self, forKey: .physicalControllerPassthrough) ?? Self.default.physicalControllerPassthrough
    }

    static let `default` = StreamerPreferences(
        audioMuted: true,
        showStatsClock: false,
        showStatsBattery: false,
        touchControllerVisible: false,
        physicalControllerPassthrough: true
    )
}

struct TouchControlPoint: Codable, Equatable {
    var x: Double
    var y: Double

    enum CodingKeys: String, CodingKey {
        case x
        case y
    }

    init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        x = try container.decodeIfPresent(Double.self, forKey: .x) ?? 0.5
        y = try container.decodeIfPresent(Double.self, forKey: .y) ?? 0.5
    }
}

struct TouchControlLayout: Codable, Equatable {
    var scale: Double
    var opacity: Double
    var buttonScale: Double
    var stickScale: Double
    var topLeft: TouchControlPoint
    var topCenter: TouchControlPoint
    var topRight: TouchControlPoint
    var leftStick: TouchControlPoint
    var rightCluster: TouchControlPoint
    var bottomCenter: TouchControlPoint

    enum CodingKeys: String, CodingKey {
        case scale
        case opacity
        case buttonScale
        case stickScale
        case topLeft
        case topCenter
        case topRight
        case leftStick
        case rightCluster
        case bottomCenter
    }

    init(
        scale: Double,
        opacity: Double,
        buttonScale: Double,
        stickScale: Double,
        topLeft: TouchControlPoint,
        topCenter: TouchControlPoint,
        topRight: TouchControlPoint,
        leftStick: TouchControlPoint,
        rightCluster: TouchControlPoint,
        bottomCenter: TouchControlPoint
    ) {
        self.scale = scale
        self.opacity = opacity
        self.buttonScale = buttonScale
        self.stickScale = stickScale
        self.topLeft = topLeft
        self.topCenter = topCenter
        self.topRight = topRight
        self.leftStick = leftStick
        self.rightCluster = rightCluster
        self.bottomCenter = bottomCenter
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let fallback = Self.standard
        scale = try container.decodeIfPresent(Double.self, forKey: .scale) ?? fallback.scale
        opacity = try container.decodeIfPresent(Double.self, forKey: .opacity) ?? fallback.opacity
        buttonScale = try container.decodeIfPresent(Double.self, forKey: .buttonScale) ?? fallback.buttonScale
        stickScale = try container.decodeIfPresent(Double.self, forKey: .stickScale) ?? fallback.stickScale
        topLeft = try container.decodeIfPresent(TouchControlPoint.self, forKey: .topLeft) ?? fallback.topLeft
        topCenter = try container.decodeIfPresent(TouchControlPoint.self, forKey: .topCenter) ?? fallback.topCenter
        topRight = try container.decodeIfPresent(TouchControlPoint.self, forKey: .topRight) ?? fallback.topRight
        leftStick = try container.decodeIfPresent(TouchControlPoint.self, forKey: .leftStick) ?? fallback.leftStick
        rightCluster = try container.decodeIfPresent(TouchControlPoint.self, forKey: .rightCluster) ?? fallback.rightCluster
        bottomCenter = try container.decodeIfPresent(TouchControlPoint.self, forKey: .bottomCenter) ?? fallback.bottomCenter
    }

    static let standard = TouchControlLayout(
        scale: 1,
        opacity: 0.58,
        buttonScale: 1,
        stickScale: 1,
        topLeft: .init(x: 0.14, y: 0.12),
        topCenter: .init(x: 0.50, y: 0.12),
        topRight: .init(x: 0.86, y: 0.12),
        leftStick: .init(x: 0.18, y: 0.80),
        rightCluster: .init(x: 0.83, y: 0.79),
        bottomCenter: .init(x: 0.50, y: 0.92)
    )

    static let fortniteMobile = TouchControlLayout(
        scale: 1.05,
        opacity: 0.52,
        buttonScale: 1.05,
        stickScale: 1,
        topLeft: .init(x: 0.16, y: 0.11),
        topCenter: .init(x: 0.50, y: 0.11),
        topRight: .init(x: 0.84, y: 0.11),
        leftStick: .init(x: 0.17, y: 0.81),
        rightCluster: .init(x: 0.84, y: 0.78),
        bottomCenter: .init(x: 0.50, y: 0.91)
    )

    static let defaultProfiles: [String: TouchControlLayout] = [
        "default": .standard,
        "fortnite-mobile": .fortniteMobile
    ]

    static func preset(for profile: String) -> TouchControlLayout {
        defaultProfiles[profile] ?? .standard
    }
}

enum OpenNOWPlatform {
    #if os(tvOS)
    static let supportsNativeOAuth = true
    static let supportsEmbeddedStreamer = false
    static let displayName = "tvOS"
    static let authUnavailableReason = ""
    static let streamingUnavailableReason = "Apple TV streaming still needs a native tvOS player."
    #else
    static let supportsNativeOAuth = true
    static let supportsEmbeddedStreamer = true
    static let displayName = "iOS"
    static let authUnavailableReason = ""
    static let streamingUnavailableReason = ""
    #endif
}

private enum GFNConstants {
    static let serviceUrlsEndpoint = URL(string: "https://pcs.geforcenow.com/v1/serviceUrls")!
    static let tokenEndpoint = URL(string: "https://login.nvidia.com/token")!
    static let userInfoEndpoint = URL(string: "https://login.nvidia.com/userinfo")!
    static let authEndpoint = URL(string: "https://login.nvidia.com/authorize")!
    static let clientTokenEndpoint = URL(string: "https://login.nvidia.com/client_token")!
    static let mesEndpoint = URL(string: "https://mes.geforcenow.com/v4/subscriptions")!
    static let graphQL = "https://games.geforce.com/graphql"

    static let clientId = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ"
    static let scopes = "openid consent email tk_client age"
    static let defaultProvider = LoginProvider(
        idpId: "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg",
        code: "NVIDIA",
        displayName: "NVIDIA",
        streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
        priority: 0
    )
    static let lcarsClientId = "ec7e38d4-03af-4b58-b131-cfb0495903ab"
    static let gfnClientVersion = "2.0.80.173"
    static let panelsQueryHash = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0"
    static let appMetadataQueryHash = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7"
    static let userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173"
    static let oauthRedirectUri = "http://localhost:2259"
    static let oauthCallbackScheme = "opennowios"
    static let oauthRedirectPort: UInt16 = 2259
    static let sessionModifyActionAdUpdate = 6
}

private enum TVAuthDiagnostics {
    static let notificationName = Notification.Name("OpenNOW.TVAuthDiagnostics")

    #if os(tvOS)
    private static let logger = Logger(subsystem: "OpenNOWiOS", category: "TVAuth")

    static func record(_ message: String) {
        logger.notice("\(message, privacy: .public)")
        NotificationCenter.default.post(
            name: notificationName,
            object: nil,
            userInfo: ["message": message]
        )
    }
    #else
    static func record(_ message: String) {}
    #endif
}

private func oauthCallbackQueryItems(from url: URL) -> [URLQueryItem] {
    URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
}

private func oauthCallbackHasResult(_ url: URL) -> Bool {
    let queryItems = oauthCallbackQueryItems(from: url)
    let hasCode = queryItems.contains(where: { $0.name == "code" && !($0.value ?? "").isEmpty })
    let hasError = queryItems.contains(where: { $0.name == "error" && !($0.value ?? "").isEmpty })
    return hasCode || hasError
}

private func summarizeOAuthCallback(_ url: URL) -> String {
    let components = URLComponents(url: url, resolvingAgainstBaseURL: false)
    let scheme = components?.scheme ?? url.scheme ?? "unknown"
    let host = components?.host ?? url.host ?? "-"
    let rawPath = components?.path ?? url.path
    let path = rawPath.isEmpty ? "/" : rawPath
    let queryKeys = (components?.queryItems ?? [])
        .map(\.name)
        .sorted()
        .joined(separator: ",")
    let querySummary = queryKeys.isEmpty ? "none" : queryKeys
    return "\(scheme)://\(host)\(path) queryKeys=[\(querySummary)]"
}

private func normalizedEndpointHost(from value: Any?) -> String? {
    if let stringValue = value as? String {
        return normalizedEndpointHost(from: stringValue)
    }
    if let stringValues = value as? [String] {
        return stringValues.compactMap { normalizedEndpointHost(from: $0) }.first
    }
    return nil
}

private func normalizedEndpointHost(from stringValue: String?) -> String? {
    guard var raw = stringValue?.trimmingCharacters(in: .whitespacesAndNewlines), !raw.isEmpty else {
        return nil
    }
    let urlSchemes: Set<String> = ["http", "https", "ws", "wss", "rtsp", "rtsps"]
    if let components = URLComponents(string: raw),
       let scheme = components.scheme?.lowercased(),
       urlSchemes.contains(scheme) {
        return validEndpointHost(components.host)
    }

    if raw.hasPrefix("[") {
        guard let end = raw.firstIndex(of: "]") else { return nil }
        let hostStart = raw.index(after: raw.startIndex)
        return validEndpointHost(String(raw[hostStart..<end]))
    }

    if let separator = raw.firstIndex(where: { $0 == "/" || $0 == "?" || $0 == "#" }) {
        raw = String(raw[..<separator])
    }
    guard !raw.contains("@") else { return nil }

    let colonCount = raw.filter { $0 == ":" }.count
    if colonCount == 1, let separator = raw.lastIndex(of: ":") {
        let portStart = raw.index(after: separator)
        let port = raw[portStart...]
        if !port.isEmpty, port.allSatisfy({ $0.isNumber }) {
            raw = String(raw[..<separator])
        }
    }

    return validEndpointHost(raw)
}

private func validEndpointHost(_ host: String?) -> String? {
    guard let host else { return nil }
    let normalized = host.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !normalized.isEmpty,
          normalized.rangeOfCharacter(from: .whitespacesAndNewlines) == nil else {
        return nil
    }

    if normalized.contains(":") {
        let allowedIPv6 = CharacterSet(charactersIn: "0123456789abcdefABCDEF:.")
        guard normalized.unicodeScalars.allSatisfy({ allowedIPv6.contains($0) }) else {
            return nil
        }
        return normalized
    }

    let allowedHost = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.-")
    guard normalized.unicodeScalars.allSatisfy({ allowedHost.contains($0) }),
          !normalized.hasPrefix("."),
          !normalized.hasSuffix("."),
          !normalized.hasPrefix("-"),
          !normalized.hasSuffix("-"),
          !normalized.contains("..") else {
        return nil
    }

    let labels = normalized.split(separator: ".", omittingEmptySubsequences: false)
    guard labels.allSatisfy({ !$0.isEmpty && !$0.hasPrefix("-") && !$0.hasSuffix("-") }) else {
        return nil
    }
    return normalized
}

typealias TVOSOAuthPresenter = @MainActor @Sendable (_ url: URL, _ callbackScheme: String) async throws -> URL

enum SessionAdAction: String, Codable {
    case start
    case pause
    case resume
    case finish
    case cancel
}

private final class OAuthWebAuthenticator: NSObject {
    private var session: ASWebAuthenticationSession?

    func authenticate(url: URL, callbackScheme: String) async throws -> URL {
        try await withCheckedThrowingContinuation { continuation in
            TVAuthDiagnostics.record(
                "Preparing ASWebAuthenticationSession host=\(url.host ?? "unknown") callbackScheme=\(callbackScheme)"
            )
            let authSession: ASWebAuthenticationSession
            #if os(tvOS)
            authSession = ASWebAuthenticationSession(
                url: url,
                callback: .customScheme(callbackScheme)
            ) { callbackURL, error in
                if let callbackURL {
                    TVAuthDiagnostics.record("Auth session completed with \(summarizeOAuthCallback(callbackURL))")
                    continuation.resume(returning: callbackURL)
                    return
                }
                let authError = error ?? NSError(
                    domain: "OpenNOWAuth", code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "Authentication cancelled"]
                )
                TVAuthDiagnostics.record("Auth session ended with error: \(authError.localizedDescription)")
                continuation.resume(throwing: authError)
            }
            #else
            authSession = ASWebAuthenticationSession(
                url: url,
                callbackURLScheme: callbackScheme
            ) { callbackURL, error in
                if let callbackURL {
                    TVAuthDiagnostics.record("Auth session completed with \(summarizeOAuthCallback(callbackURL))")
                    continuation.resume(returning: callbackURL)
                    return
                }
                let authError = error ?? NSError(
                    domain: "OpenNOWAuth", code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "Authentication cancelled"]
                )
                TVAuthDiagnostics.record("Auth session ended with error: \(authError.localizedDescription)")
                continuation.resume(throwing: authError)
            }
            authSession.presentationContextProvider = self
            authSession.prefersEphemeralWebBrowserSession = false
            #endif
            self.session = authSession
            TVAuthDiagnostics.record("Starting ASWebAuthenticationSession.")
            if !authSession.start() {
                TVAuthDiagnostics.record("ASWebAuthenticationSession failed to start.")
                continuation.resume(throwing: NSError(
                    domain: "OpenNOWAuth", code: 2,
                    userInfo: [NSLocalizedDescriptionKey: "Could not start sign-in session"]))
            }
        }
    }

    func cancel() {
        #if !os(tvOS)
        session?.cancel()
        #endif
        session = nil
    }
}

#if !os(tvOS)
extension OAuthWebAuthenticator: ASWebAuthenticationPresentationContextProviding {
    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        if Thread.isMainThread {
            return MainActor.assumeIsolated {
                Self.activePresentationAnchor()
            }
        }

        return DispatchQueue.main.sync {
            MainActor.assumeIsolated {
                Self.activePresentationAnchor()
            }
        }
    }

    @MainActor
    private static func activePresentationAnchor() -> ASPresentationAnchor {
        let foregroundScene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        let scene = foregroundScene ?? UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first

        if let keyWindow = scene?.windows.first(where: { $0.isKeyWindow }) {
            return keyWindow
        }
        if let window = scene?.windows.first {
            return window
        }

        return ASPresentationAnchor()
    }
}
#endif

private final class OAuthLoopbackServer: @unchecked Sendable {
    private let queue = DispatchQueue(label: "OpenNOW.OAuthLoopback")
    private var listener: NWListener?
    private var continuation: CheckedContinuation<URL, Error>?
    private var didComplete = false

    func waitForCallback(port: UInt16, timeoutSeconds: TimeInterval = 120) async throws -> URL {
        try await withCheckedThrowingContinuation { continuation in
            queue.async {
                guard !self.didComplete else {
                    continuation.resume(
                        throwing: NSError(
                            domain: "OpenNOW.Auth",
                            code: 10,
                            userInfo: [NSLocalizedDescriptionKey: "OAuth callback listener already completed."]
                        )
                    )
                    return
                }

                self.continuation = continuation
                do {
                    let listener = try NWListener(using: .tcp, on: NWEndpoint.Port(rawValue: port)!)
                    self.listener = listener

                    listener.stateUpdateHandler = { [weak self] state in
                        guard let self else { return }
                        if case .failed(let error) = state {
                            TVAuthDiagnostics.record("Loopback listener failed: \(error.localizedDescription)")
                            self.complete(with: .failure(error))
                        }
                    }

                    listener.newConnectionHandler = { [weak self] connection in
                        self?.handle(connection)
                    }

                    listener.start(queue: self.queue)
                    TVAuthDiagnostics.record("Loopback listener started on http://localhost:\(port)")

                    self.queue.asyncAfter(deadline: .now() + timeoutSeconds) { [weak self] in
                        self?.complete(
                            with: .failure(
                                NSError(
                                    domain: "OpenNOW.Auth",
                                    code: 11,
                                    userInfo: [NSLocalizedDescriptionKey: "Timed out waiting for OAuth callback."]
                                )
                            )
                        )
                    }
                } catch {
                    self.complete(with: .failure(error))
                }
            }
        }
    }

    private func handle(_ connection: NWConnection) {
        connection.start(queue: queue)
        connection.receive(minimumIncompleteLength: 1, maximumLength: 8192) { [weak self] data, _, _, _ in
            guard let self else { return }
            let requestText = data.flatMap { String(data: $0, encoding: .utf8) } ?? ""
            let firstLine = requestText
                .split(separator: "\n", maxSplits: 1)
                .first
                .map { String($0) } ?? ""
            let path = firstLine
                .split(separator: " ", omittingEmptySubsequences: true)
                .dropFirst()
                .first
                .map { String($0) } ?? "/"

            let callbackURL = URL(string: "http://localhost\(path)") ?? URL(string: GFNConstants.oauthRedirectUri)!
            let queryItems = URLComponents(url: callbackURL, resolvingAgainstBaseURL: false)?.queryItems ?? []
            let hasCode = queryItems.contains(where: { $0.name == "code" && !($0.value ?? "").isEmpty })
            let hasError = queryItems.contains(where: { $0.name == "error" && !($0.value ?? "").isEmpty })

            if hasCode || hasError {
                TVAuthDiagnostics.record("Loopback callback received \(summarizeOAuthCallback(callbackURL))")
                var redirectComponents = URLComponents()
                redirectComponents.scheme = "opennowios"
                redirectComponents.host = "callback"
                redirectComponents.queryItems = queryItems
                let redirectTarget = redirectComponents.url?.absoluteString ?? "opennowios://callback"

                let httpResponse = "HTTP/1.1 302 Found\r\nLocation: \(redirectTarget)\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                connection.send(content: Data(httpResponse.utf8), completion: .contentProcessed { _ in
                    connection.cancel()
                })
                self.complete(with: .success(callbackURL))
            } else {
                TVAuthDiagnostics.record("Loopback request ignored \(summarizeOAuthCallback(callbackURL))")
                let httpResponse = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                connection.send(content: Data(httpResponse.utf8), completion: .contentProcessed { _ in
                    connection.cancel()
                })
            }
        }
    }

    private func complete(with result: Result<URL, Error>) {
        guard !didComplete else { return }
        didComplete = true
        listener?.cancel()
        listener = nil
        guard let continuation else { return }
        self.continuation = nil
        continuation.resume(with: result)
    }

    func stop() {
        complete(
            with: .failure(
                NSError(
                    domain: "OpenNOW.Auth",
                    code: 12,
                    userInfo: [NSLocalizedDescriptionKey: "OAuth callback listener stopped."]
                )
            )
        )
    }
}

private final class GFNTLSDelegate: NSObject, URLSessionDelegate {
    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let serverTrust = challenge.protectionSpace.serverTrust,
              challenge.protectionSpace.host.hasSuffix("nvidiagrid.net") else {
            completionHandler(.performDefaultHandling, nil)
            return
        }
        completionHandler(.useCredential, URLCredential(trust: serverTrust))
    }
}

private enum StreamDeviceProfile {
    case desktop
    case mobileTouch

    var nvDeviceOS: String {
        switch self {
        case .desktop:
            return "WINDOWS"
        case .mobileTouch:
            return "IOS"
        }
    }

    var nvDeviceType: String {
        switch self {
        case .desktop:
            return "DESKTOP"
        case .mobileTouch:
            return "MOBILE"
        }
    }

    var clientPlatformName: String {
        switch self {
        case .desktop:
            return "windows"
        case .mobileTouch:
            return "ios"
        }
    }

    var clientIdentification: String {
        switch self {
        case .desktop:
            return "GFN-PC"
        case .mobileTouch:
            return "GFN-MOBILE"
        }
    }

    var metadata: [[String: String]] {
        switch self {
        case .desktop:
            return []
        case .mobileTouch:
            return [
                ["key": "MobileTouchInput", "value": "1"],
                ["key": "InputDeviceClass", "value": "touch"]
            ]
        }
    }
}

private actor GFNAPIClient {
    #if os(tvOS)
    private enum OAuthCallbackSource: String {
        case authSession = "ASWebAuthenticationSession"
        case loopbackServer = "localhost callback"
    }

    private actor OAuthCallbackCoordinator {
        private var didResolve = false
        private var failures: [String] = []
        private let continuation: CheckedContinuation<(source: OAuthCallbackSource, url: URL), Error>

        init(continuation: CheckedContinuation<(source: OAuthCallbackSource, url: URL), Error>) {
            self.continuation = continuation
        }

        func succeed(source: OAuthCallbackSource, url: URL) -> Bool {
            guard !didResolve else { return false }
            didResolve = true
            continuation.resume(returning: (source, url))
            return true
        }

        func fail(source: OAuthCallbackSource, error: Error) -> Bool {
            guard !didResolve else { return false }
            failures.append("\(source.rawValue): \(error.localizedDescription)")
            if failures.count == 2 {
                didResolve = true
                continuation.resume(
                    throwing: NSError(
                        domain: "OpenNOW.Auth",
                        code: 13,
                        userInfo: [NSLocalizedDescriptionKey: failures.joined(separator: " | ")]
                    )
                )
            }
            return true
        }
    }
    #endif

    private struct PendingOAuthLogin {
        let provider: LoginProvider
        let redirectUri: String
        let codeVerifier: String
        let authURL: URL
    }

    private let session: URLSession = {
        let delegate = GFNTLSDelegate()
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 30
        return URLSession(configuration: config, delegate: delegate, delegateQueue: nil)
    }()

    private func request(
        url: URL,
        method: String = "GET",
        headers: [String: String],
        body: Data? = nil
    ) async throws -> (Data, HTTPURLResponse) {
        var req = URLRequest(url: url)
        req.httpMethod = method
        req.httpBody = body
        for (k, v) in headers {
            req.setValue(v, forHTTPHeaderField: k)
        }
        let (data, response) = try await self.session.data(for: req)
        guard let http = response as? HTTPURLResponse else {
            throw NSError(domain: "OpenNOW.Network", code: -1, userInfo: [NSLocalizedDescriptionKey: "Invalid response"])
        }
        return (data, http)
    }

    private func parseJSON(_ data: Data) throws -> [String: Any] {
        guard let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw NSError(domain: "OpenNOW.Network", code: -2, userInfo: [NSLocalizedDescriptionKey: "Invalid JSON object"])
        }
        return obj
    }

    private func headersForOAuth() -> [String: String] {
        [
            "Origin": "https://nvfile",
            "Referer": "https://nvfile/",
            "Accept": "application/json, text/plain, */*",
            "User-Agent": GFNConstants.userAgent
        ]
    }

    func fetchProviders() async -> [LoginProvider] {
        do {
            let (data, response) = try await request(
                url: GFNConstants.serviceUrlsEndpoint,
                headers: ["Accept": "application/json", "User-Agent": GFNConstants.userAgent]
            )
            guard response.statusCode == 200 else { return [GFNConstants.defaultProvider] }
            let json = try parseJSON(data)
            let gfnInfo = json["gfnServiceInfo"] as? [String: Any]
            let endpoints = gfnInfo?["gfnServiceEndpoints"] as? [[String: Any]] ?? []
            let providers = endpoints.compactMap { item -> LoginProvider? in
                guard let idp = item["idpId"] as? String,
                      let code = item["loginProviderCode"] as? String,
                      let name = item["loginProviderDisplayName"] as? String,
                      let url = item["streamingServiceUrl"] as? String else {
                    return nil
                }
                let prio = item["loginProviderPriority"] as? Int ?? 0
                let display = (code == "BPC") ? "bro.game" : name
                let normalizedURL = url.hasSuffix("/") ? url : "\(url)/"
                return LoginProvider(idpId: idp, code: code, displayName: display, streamingServiceUrl: normalizedURL, priority: prio)
            }.sorted { $0.priority < $1.priority }
            return providers.isEmpty ? [GFNConstants.defaultProvider] : providers
        } catch {
            return [GFNConstants.defaultProvider]
        }
    }

    func login(with provider: LoginProvider, deviceId: String) async throws -> AuthSession {
        let pending = makePendingOAuthLogin(with: provider, deviceId: deviceId)
        let callbackServer = OAuthLoopbackServer()
        let callbackURL: URL
        do {
            #if os(tvOS)
            let callbackResult = try await awaitTVOSOAuthCallback(
                authUrl: pending.authURL,
                callbackServer: callbackServer,
                authenticate: { url, callbackScheme in
                    try await Self.performOAuthSession(url: url, callbackScheme: callbackScheme)
                }
            )
            callbackURL = callbackResult.url
            TVAuthDiagnostics.record(
                "Continuing with \(callbackResult.source.rawValue) using \(summarizeOAuthCallback(callbackURL))"
            )
            if callbackResult.source == .loopbackServer {
                TVAuthDiagnostics.record(
                    "If the sign-in sheet stays open on Apple TV, dismiss it with Back or Menu after returning to OpenNOW."
                )
            }
            #else
            let serverTask = Task<URL, Error> {
                try await callbackServer.waitForCallback(port: GFNConstants.oauthRedirectPort)
            }
            callbackURL = try await Self.performOAuthSession(
                url: pending.authURL,
                callbackScheme: GFNConstants.oauthCallbackScheme
            )
            serverTask.cancel()
            #endif
        } catch {
            callbackServer.stop()
            TVAuthDiagnostics.record("Sign-in callback failed: \(error.localizedDescription)")
            throw error
        }
        return try await completeOAuthLogin(pending, callbackURL: callbackURL)
    }

    #if os(tvOS)
    func login(
        with provider: LoginProvider,
        deviceId: String,
        authenticate: @escaping TVOSOAuthPresenter
    ) async throws -> AuthSession {
        let pending = makePendingOAuthLogin(with: provider, deviceId: deviceId)
        let callbackServer = OAuthLoopbackServer()
        do {
            let callbackResult = try await awaitTVOSOAuthCallback(
                authUrl: pending.authURL,
                callbackServer: callbackServer,
                authenticate: authenticate
            )
            let callbackURL = callbackResult.url
            TVAuthDiagnostics.record(
                "Continuing with \(callbackResult.source.rawValue) using \(summarizeOAuthCallback(callbackURL))"
            )
            if callbackResult.source == .loopbackServer {
                TVAuthDiagnostics.record(
                    "If the sign-in sheet stays open on Apple TV, dismiss it with Back or Menu after returning to OpenNOW."
                )
            }
            return try await completeOAuthLogin(pending, callbackURL: callbackURL)
        } catch {
            callbackServer.stop()
            TVAuthDiagnostics.record("Sign-in callback failed: \(error.localizedDescription)")
            throw error
        }
    }
    #endif

    private func makePendingOAuthLogin(with provider: LoginProvider, deviceId: String) -> PendingOAuthLogin {
        let pkce = Self.generatePKCE()
        let nonce = UUID().uuidString.replacingOccurrences(of: "-", with: "")
        let redirectUri = GFNConstants.oauthRedirectUri
        TVAuthDiagnostics.record(
            "Starting sign-in provider=\(provider.displayName) redirect=\(redirectUri)"
        )
        var authComponents = URLComponents(url: GFNConstants.authEndpoint, resolvingAgainstBaseURL: false)!
        authComponents.queryItems = [
            .init(name: "response_type", value: "code"),
            .init(name: "device_id", value: deviceId),
            .init(name: "scope", value: GFNConstants.scopes),
            .init(name: "client_id", value: GFNConstants.clientId),
            .init(name: "redirect_uri", value: redirectUri),
            .init(name: "ui_locales", value: "en_US"),
            .init(name: "nonce", value: nonce),
            .init(name: "prompt", value: "select_account"),
            .init(name: "code_challenge", value: pkce.challenge),
            .init(name: "code_challenge_method", value: "S256"),
            .init(name: "idp_id", value: provider.idpId)
        ]

        return PendingOAuthLogin(
            provider: provider,
            redirectUri: redirectUri,
            codeVerifier: pkce.verifier,
            authURL: authComponents.url!
        )
    }

    private func completeOAuthLogin(_ pending: PendingOAuthLogin, callbackURL: URL) async throws -> AuthSession {
        let callbackQueryItems = oauthCallbackQueryItems(from: callbackURL)
        if let oauthError = callbackQueryItems.first(where: { $0.name == "error" })?.value {
            let oauthErrorDescription =
                callbackQueryItems.first(where: { $0.name == "error_description" })?.value ??
                "Authentication failed."
            TVAuthDiagnostics.record("OAuth provider returned error=\(oauthError)")
            throw NSError(
                domain: "OpenNOW.Auth",
                code: 8,
                userInfo: [NSLocalizedDescriptionKey: "\(oauthError): \(oauthErrorDescription)"]
            )
        }
        guard let authCode = callbackQueryItems.first(where: { $0.name == "code" })?.value else {
            TVAuthDiagnostics.record("Callback did not include an authorization code.")
            throw NSError(
                domain: "OpenNOW.Auth",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "Sign-in callback did not include an authorization code"]
            )
        }

        let tokenBody = URLQueryItemEncoder.encode([
            "grant_type": "authorization_code",
            "code": authCode,
            "redirect_uri": pending.redirectUri,
            "code_verifier": pending.codeVerifier
        ])

        var tokenHeaders = headersForOAuth()
        tokenHeaders["Content-Type"] = "application/x-www-form-urlencoded; charset=UTF-8"
        TVAuthDiagnostics.record("Exchanging authorization code for tokens.")
        let (tokenData, tokenResponse) = try await request(
            url: GFNConstants.tokenEndpoint,
            method: "POST",
            headers: tokenHeaders,
            body: tokenBody.data(using: String.Encoding.utf8)
        )
        TVAuthDiagnostics.record("Token exchange returned HTTP \(tokenResponse.statusCode).")
        guard tokenResponse.statusCode == 200 else {
            let body = String(data: tokenData, encoding: .utf8) ?? "unknown error"
            throw NSError(
                domain: "OpenNOW.Auth",
                code: tokenResponse.statusCode,
                userInfo: [NSLocalizedDescriptionKey: "Token exchange failed: \(body)"]
            )
        }

        let tokenJSON = try parseJSON(tokenData)
        guard let accessToken = tokenJSON["access_token"] as? String else {
            TVAuthDiagnostics.record("OAuth token response was missing access_token.")
            throw NSError(
                domain: "OpenNOW.Auth",
                code: 4,
                userInfo: [NSLocalizedDescriptionKey: "OAuth response missing access token"]
            )
        }
        let refreshToken = tokenJSON["refresh_token"] as? String
        let idToken = tokenJSON["id_token"] as? String
        let expiresIn = tokenJSON["expires_in"] as? Double ?? 86400
        var tokens = AuthTokens(
            accessToken: accessToken,
            refreshToken: refreshToken,
            idToken: idToken,
            expiresAt: Date().addingTimeInterval(expiresIn).timeIntervalSince1970,
            clientToken: nil,
            clientTokenExpiresAt: nil
        )

        TVAuthDiagnostics.record("Loading NVIDIA user profile.")
        var user = try await fetchUser(tokens: tokens)
        TVAuthDiagnostics.record("User profile loaded.")
        if let freshClientToken = try? await requestClientToken(accessToken: accessToken) {
            tokens = AuthTokens(
                accessToken: tokens.accessToken,
                refreshToken: tokens.refreshToken,
                idToken: tokens.idToken,
                expiresAt: tokens.expiresAt,
                clientToken: freshClientToken.token,
                clientTokenExpiresAt: freshClientToken.expiresAt
            )
            TVAuthDiagnostics.record("Client token refreshed.")
        }
        if let tier = try? await fetchMembershipTier(
            token: idToken ?? accessToken,
            userId: user.userId,
            streamingBaseUrl: pending.provider.streamingServiceUrl
        ) {
            user.membershipTier = tier
            TVAuthDiagnostics.record("Membership tier loaded.")
        }

        TVAuthDiagnostics.record("Sign-in finished successfully.")
        return AuthSession(provider: pending.provider, tokens: tokens, user: user)
    }

    @MainActor
    private static func performOAuthSession(url: URL, callbackScheme: String) async throws -> URL {
        let authenticator = OAuthWebAuthenticator()
        return try await authenticator.authenticate(url: url, callbackScheme: callbackScheme)
    }

    #if os(tvOS)
    private func awaitTVOSOAuthCallback(
        authUrl: URL,
        callbackServer: OAuthLoopbackServer,
        authenticate: @escaping TVOSOAuthPresenter
    ) async throws -> (source: OAuthCallbackSource, url: URL) {
        try await withCheckedThrowingContinuation { continuation in
            let coordinator = OAuthCallbackCoordinator(continuation: continuation)

            Task {
                do {
                    let callbackURL = try await callbackServer.waitForCallback(port: GFNConstants.oauthRedirectPort)
                    guard oauthCallbackHasResult(callbackURL) else {
                        let error = NSError(
                            domain: "OpenNOW.Auth",
                            code: 14,
                            userInfo: [NSLocalizedDescriptionKey: "Loopback callback was missing expected OAuth parameters."]
                        )
                        if await coordinator.fail(source: .loopbackServer, error: error) {
                            TVAuthDiagnostics.record(
                                "Loopback callback missing OAuth parameters: \(summarizeOAuthCallback(callbackURL))"
                            )
                        }
                        return
                    }
                    if await coordinator.succeed(source: .loopbackServer, url: callbackURL) {
                        TVAuthDiagnostics.record("Loopback callback won the race.")
                    }
                } catch {
                    if await coordinator.fail(source: .loopbackServer, error: error) {
                        TVAuthDiagnostics.record("Loopback listener ended: \(error.localizedDescription)")
                    }
                }
            }

            Task { @MainActor in
                do {
                    TVAuthDiagnostics.record("Launching tvOS WebAuthenticationSession.")
                    let callbackURL = try await authenticate(authUrl, GFNConstants.oauthCallbackScheme)
                    guard oauthCallbackHasResult(callbackURL) else {
                        let error = NSError(
                            domain: "OpenNOW.Auth",
                            code: 15,
                            userInfo: [NSLocalizedDescriptionKey: "ASWebAuthenticationSession returned an unexpected callback URL."]
                        )
                        if await coordinator.fail(source: .authSession, error: error) {
                            TVAuthDiagnostics.record(
                                "Auth session callback missing OAuth parameters: \(summarizeOAuthCallback(callbackURL))"
                            )
                        }
                        return
                    }
                    if await coordinator.succeed(source: .authSession, url: callbackURL) {
                        TVAuthDiagnostics.record("ASWebAuthenticationSession won the race.")
                        callbackServer.stop()
                    }
                } catch {
                    if await coordinator.fail(source: .authSession, error: error) {
                        TVAuthDiagnostics.record("Auth session ended before localhost callback: \(error.localizedDescription)")
                    }
                }
            }
        }
    }
    #endif

    func refreshSession(_ session: AuthSession) async throws -> AuthSession {
        let nowEpoch = Date().timeIntervalSince1970
        guard nowEpoch >= session.tokens.expiresAt - (10 * 60) else {
            return session
        }

        if let clientToken = session.tokens.clientToken,
           let expiry = session.tokens.clientTokenExpiresAt,
           nowEpoch < expiry - (5 * 60) {
            if let refreshed = try? await refreshWithClientToken(clientToken, userId: session.user.userId, existing: session) {
                return refreshed
            }
        }

        if let refreshToken = session.tokens.refreshToken {
            return try await refreshWithOAuthToken(refreshToken, existing: session)
        }

        return session
    }

    private func refreshWithClientToken(_ clientToken: String, userId: String, existing: AuthSession) async throws -> AuthSession {
        let body = URLQueryItemEncoder.encode([
            "grant_type": "urn:ietf:params:oauth:grant-type:client_token",
            "client_token": clientToken,
            "client_id": GFNConstants.clientId,
            "sub": userId
        ])
        var headers = headersForOAuth()
        headers["Content-Type"] = "application/x-www-form-urlencoded; charset=UTF-8"
        let (data, response) = try await request(
            url: GFNConstants.tokenEndpoint,
            method: "POST",
            headers: headers,
            body: body.data(using: .utf8)
        )
        guard response.statusCode == 200 else {
            throw NSError(domain: "OpenNOW.Auth", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: "client_token refresh failed"])
        }
        let json = try parseJSON(data)
        guard let accessToken = json["access_token"] as? String else {
            throw NSError(domain: "OpenNOW.Auth", code: 4, userInfo: [NSLocalizedDescriptionKey: "client_token response missing access_token"])
        }
        let newRefresh = (json["refresh_token"] as? String) ?? existing.tokens.refreshToken
        let newIdToken = (json["id_token"] as? String) ?? existing.tokens.idToken
        let expiresIn = (json["expires_in"] as? Double) ?? 86400
        var newClientToken = existing.tokens.clientToken
        var newClientTokenExpiry = existing.tokens.clientTokenExpiresAt
        if let fresh = try? await requestClientToken(accessToken: accessToken) {
            newClientToken = fresh.token
            newClientTokenExpiry = fresh.expiresAt
        }
        let newTokens = AuthTokens(
            accessToken: accessToken,
            refreshToken: newRefresh,
            idToken: newIdToken,
            expiresAt: Date().addingTimeInterval(expiresIn).timeIntervalSince1970,
            clientToken: newClientToken,
            clientTokenExpiresAt: newClientTokenExpiry
        )
        let user = (try? await fetchUser(tokens: newTokens)) ?? existing.user
        return AuthSession(provider: existing.provider, tokens: newTokens, user: user)
    }

    private func refreshWithOAuthToken(_ refreshToken: String, existing: AuthSession) async throws -> AuthSession {
        let body = URLQueryItemEncoder.encode([
            "grant_type": "refresh_token",
            "refresh_token": refreshToken,
            "client_id": GFNConstants.clientId
        ])
        var headers = headersForOAuth()
        headers["Content-Type"] = "application/x-www-form-urlencoded; charset=UTF-8"
        let (data, response) = try await request(
            url: GFNConstants.tokenEndpoint,
            method: "POST",
            headers: headers,
            body: body.data(using: .utf8)
        )
        guard response.statusCode == 200 else {
            throw NSError(domain: "OpenNOW.Auth", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: "refresh_token exchange failed"])
        }
        let json = try parseJSON(data)
        guard let accessToken = json["access_token"] as? String else {
            throw NSError(domain: "OpenNOW.Auth", code: 4, userInfo: [NSLocalizedDescriptionKey: "refresh_token response missing access_token"])
        }
        let newRefresh = (json["refresh_token"] as? String) ?? refreshToken
        let newIdToken = (json["id_token"] as? String) ?? existing.tokens.idToken
        let expiresIn = (json["expires_in"] as? Double) ?? 86400
        var newClientToken = existing.tokens.clientToken
        var newClientTokenExpiry = existing.tokens.clientTokenExpiresAt
        if let fresh = try? await requestClientToken(accessToken: accessToken) {
            newClientToken = fresh.token
            newClientTokenExpiry = fresh.expiresAt
        }
        let newTokens = AuthTokens(
            accessToken: accessToken,
            refreshToken: newRefresh,
            idToken: newIdToken,
            expiresAt: Date().addingTimeInterval(expiresIn).timeIntervalSince1970,
            clientToken: newClientToken,
            clientTokenExpiresAt: newClientTokenExpiry
        )
        let user = (try? await fetchUser(tokens: newTokens)) ?? existing.user
        return AuthSession(provider: existing.provider, tokens: newTokens, user: user)
    }

    func fetchMainGames(session: AuthSession) async throws -> ([CloudGame], String) {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let serverInfo = try await fetchServerInfo(token: token, streamingBaseUrl: session.provider.streamingServiceUrl)
        let vpcId = serverInfo.vpcId ?? "GFN-PC"
        let payload = try await fetchPanels(token: token, panelNames: ["MAIN"], vpcId: vpcId)
        let games = Self.flattenPanels(payload: payload)
        let enrichedGames = (try? await enrichGamesWithMetadata(token: token, vpcId: vpcId, games: games)) ?? games
        return (enrichedGames, vpcId)
    }

    func fetchLibraryGames(session: AuthSession, vpcId: String) async throws -> [CloudGame] {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let payload = try await fetchPanels(token: token, panelNames: ["LIBRARY"], vpcId: vpcId)
        let games = Self.flattenPanels(payload: payload)
        return (try? await enrichGamesWithMetadata(token: token, vpcId: vpcId, games: games)) ?? games
    }

    func fetchSubscription(session: AuthSession, vpcId: String) async throws -> SubscriptionSnapshot {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        var components = URLComponents(url: GFNConstants.mesEndpoint, resolvingAgainstBaseURL: false)!
        components.queryItems = [
            .init(name: "serviceName", value: "gfn_pc"),
            .init(name: "languageCode", value: "en_US"),
            .init(name: "vpcId", value: vpcId),
            .init(name: "userId", value: session.user.userId)
        ]
        let (data, response) = try await request(
            url: components.url!,
            headers: [
                "Authorization": "GFNJWT \(token)",
                "Accept": "application/json",
                "nv-client-id": GFNConstants.lcarsClientId,
                "nv-client-type": "NATIVE",
                "nv-client-version": GFNConstants.gfnClientVersion,
                "nv-client-streamer": "NVIDIA-CLASSIC",
                "nv-device-os": "WINDOWS",
                "nv-device-type": "DESKTOP"
            ]
        )
        guard response.statusCode == 200 else {
            throw NSError(domain: "OpenNOW.Subscription", code: response.statusCode, userInfo: nil)
        }
        let json = try parseJSON(data)
        let tier = (json["membershipTier"] as? String) ?? session.user.membershipTier
        let remaining = (json["remainingTimeInMinutes"] as? Double ?? 0) / 60.0
        let total = (json["totalTimeInMinutes"] as? Double ?? 0) / 60.0
        let state = json["currentSubscriptionState"] as? [String: Any]
        let allowed = state?["isGamePlayAllowed"] as? Bool ?? true
        let isUnlimited = (json["subType"] as? String) == "UNLIMITED"
        return SubscriptionSnapshot(
            membershipTier: tier,
            isGamePlayAllowed: allowed,
            isUnlimited: isUnlimited,
            remainingHours: remaining,
            totalHours: total
        )
    }

    func startSession(
        session: AuthSession,
        game: CloudGame,
        vpcId: String,
        settings: AppSettings,
        streamingBaseUrl: String? = nil,
        launchAppIdOverride: String? = nil,
        launcherName: String = "Auto"
    ) async throws -> ActiveSession {
        let resolvedLaunchAppId = launchAppIdOverride ?? game.launchAppId
        guard let launchAppId = resolvedLaunchAppId, !launchAppId.isEmpty else {
            throw NSError(domain: "OpenNOW.Session", code: 30, userInfo: [NSLocalizedDescriptionKey: "Selected game has no launch app ID"])
        }
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let baseSource = streamingBaseUrl ?? session.provider.streamingServiceUrl
        let base = baseSource.hasSuffix("/") ? String(baseSource.dropLast()) : baseSource
        let deviceProfile = Self.streamDeviceProfile(for: game.title, settings: settings)
        let url = URL(string: "\(base)/v2/session?keyboardLayout=en-US&languageCode=en_US")!
        let body = Self.buildSessionBody(
            appId: launchAppId,
            title: game.title,
            fps: settings.preferredFPS,
            launcherName: launcherName,
            deviceProfile: deviceProfile
        )
        let clientId = UUID().uuidString
        let deviceId = UUID().uuidString
        let (data, response) = try await request(
            url: url,
            method: "POST",
            headers: Self.cloudMatchHeaders(
                token: token,
                clientId: clientId,
                deviceId: deviceId,
                includeOrigin: true,
                deviceProfile: deviceProfile
            ),
            body: body
        )
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.Session", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
        let json = try parseJSON(data)
        let requestStatus = json["requestStatus"] as? [String: Any]
        let statusCode = requestStatus?["statusCode"] as? Int ?? 0
        guard statusCode == 1 else {
            let description = requestStatus?["statusDescription"] as? String ?? "Session create failed"
            throw NSError(domain: "OpenNOW.Session", code: statusCode, userInfo: [NSLocalizedDescriptionKey: description])
        }
        let sessionObj = json["session"] as? [String: Any] ?? [:]
        let sessionId = sessionObj["sessionId"] as? String ?? UUID().uuidString
        let status = sessionObj["status"] as? Int ?? 1
        let queue = sessionObj["queuePosition"] as? Int
        let seatSetupStep = Self.extractSeatSetupStep(sessionObj: sessionObj)
        let control = sessionObj["sessionControlInfo"] as? [String: Any]
        let serverIp = Self.extractServerIp(sessionObj: sessionObj) ?? normalizedEndpointHost(from: control?["ip"])
        let mediaConnectionInfo = Self.extractMediaConnectionInfo(sessionObj: sessionObj)
        let signaling = Self.resolveSignaling(sessionObj: sessionObj, fallbackServerIp: serverIp)
        let iceServers = Self.extractIceServers(sessionObj: sessionObj)
        let adState = Self.extractAdState(sessionObj: sessionObj)
        return ActiveSession(
            id: sessionId,
            game: game,
            startedAt: .now,
            status: status,
            queuePosition: queue,
            seatSetupStep: seatSetupStep,
            serverIp: serverIp,
            mediaIp: mediaConnectionInfo.ip,
            mediaPort: mediaConnectionInfo.port,
            signalingServer: signaling.server,
            signalingUrl: signaling.url,
            iceServers: iceServers,
            zone: Self.extractZoneId(from: base, fallback: vpcId),
            streamingBaseUrl: base,
            clientId: clientId,
            deviceId: deviceId,
            adState: adState
        )
    }

    func pollSession(session: AuthSession, activeSession: ActiveSession) async throws -> ActiveSession {
        let primaryBase = Self.resolvePollBase(streamingBaseUrl: activeSession.streamingBaseUrl, serverIp: activeSession.serverIp)
        do {
            return try await pollSession(session: session, activeSession: activeSession, base: primaryBase)
        } catch {
            // Some zones intermittently fail when polling through the resolved host (e.g. direct IP).
            // Retry once through the canonical zone base before surfacing the failure.
            guard primaryBase != activeSession.streamingBaseUrl else {
                throw error
            }
            return try await pollSession(session: session, activeSession: activeSession, base: activeSession.streamingBaseUrl)
        }
    }

    private func pollSession(session: AuthSession, activeSession: ActiveSession, base: String) async throws -> ActiveSession {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let url = URL(string: "\(base)/v2/session/\(activeSession.id)")!
        let (data, response) = try await request(
            url: url,
            method: "GET",
            headers: Self.cloudMatchHeaders(
                token: token,
                clientId: activeSession.clientId,
                deviceId: activeSession.deviceId,
                includeOrigin: false
            )
        )
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.Session", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }

        let json = try parseJSON(data)
        let requestStatus = json["requestStatus"] as? [String: Any]
        let statusCode = requestStatus?["statusCode"] as? Int ?? 0
        guard statusCode == 1 else {
            let description = requestStatus?["statusDescription"] as? String ?? "Session poll failed"
            throw NSError(domain: "OpenNOW.Session", code: statusCode, userInfo: [NSLocalizedDescriptionKey: description])
        }

        let sessionObj = json["session"] as? [String: Any] ?? [:]
        let status = sessionObj["status"] as? Int ?? activeSession.status
        let queue = (sessionObj["queuePosition"] as? Int) ??
            ((sessionObj["seatSetupInfo"] as? [String: Any])?["queuePosition"] as? Int)
        let seatSetupStep = Self.extractSeatSetupStep(sessionObj: sessionObj)
        let serverIp = Self.extractServerIp(sessionObj: sessionObj) ?? activeSession.serverIp
        let mediaConnectionInfo = Self.extractMediaConnectionInfo(sessionObj: sessionObj)
        let signaling = Self.resolveSignaling(sessionObj: sessionObj, fallbackServerIp: serverIp ?? activeSession.signalingServer)
        let iceServers = Self.extractIceServers(sessionObj: sessionObj)
        let adState = Self.extractAdState(sessionObj: sessionObj)

        var updated = activeSession
        updated.status = status
        updated.queuePosition = queue
        updated.seatSetupStep = seatSetupStep
        updated.serverIp = serverIp
        updated.mediaIp = mediaConnectionInfo.ip ?? updated.mediaIp
        updated.mediaPort = mediaConnectionInfo.port > 0 ? mediaConnectionInfo.port : updated.mediaPort
        updated.signalingServer = signaling.server ?? updated.signalingServer
        updated.signalingUrl = signaling.url ?? updated.signalingUrl
        if !iceServers.isEmpty {
            updated.iceServers = iceServers
        }
        // Always replace ad state with the latest poll value. When the backend omits
        // ad fields after queue/setup, we must clear stale ad requirements so the
        // streamer handoff can proceed.
        updated.adState = adState
        return updated
    }

    func reportSessionAd(
        session: AuthSession,
        activeSession: ActiveSession,
        adId: String,
        action: SessionAdAction,
        watchedTimeInMs: Int? = nil,
        pausedTimeInMs: Int? = nil,
        cancelReason: String? = nil,
        errorInfo: String? = nil
    ) async throws -> ActiveSession {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let base = Self.resolvePollBase(streamingBaseUrl: activeSession.streamingBaseUrl, serverIp: activeSession.serverIp)
        let url = URL(string: "\(base)/v2/session/\(activeSession.id)")!
        let actionCodes: [SessionAdAction: Int] = [
            .start: 1,
            .pause: 2,
            .resume: 3,
            .finish: 4,
            .cancel: 5
        ]
        var adUpdate: [String: Any] = [
            "adId": adId,
            "adAction": actionCodes[action] ?? 1,
            "clientTimestamp": Int(Date().timeIntervalSince1970)
        ]
        if let watchedTimeInMs {
            adUpdate["watchedTimeInMs"] = max(0, watchedTimeInMs)
        }
        if let pausedTimeInMs {
            adUpdate["pausedTimeInMs"] = max(0, pausedTimeInMs)
        }
        if let cancelReason, !cancelReason.isEmpty {
            adUpdate["cancelReason"] = cancelReason
        }
        if let errorInfo, !errorInfo.isEmpty {
            adUpdate["errorInfo"] = errorInfo
        }
        let requestBody: [String: Any] = [
            "action": GFNConstants.sessionModifyActionAdUpdate,
            "adUpdates": [adUpdate]
        ]
        let body = try JSONSerialization.data(withJSONObject: requestBody)
        let (data, response) = try await request(
            url: url,
            method: "PUT",
            headers: Self.cloudMatchHeaders(
                token: token,
                clientId: activeSession.clientId,
                deviceId: activeSession.deviceId,
                includeOrigin: true
            ),
            body: body
        )
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.SessionAd", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
        let json = try parseJSON(data)
        let requestStatus = json["requestStatus"] as? [String: Any]
        let statusCode = requestStatus?["statusCode"] as? Int ?? 0
        guard statusCode == 1 else {
            let description = requestStatus?["statusDescription"] as? String ?? "Ad update failed"
            throw NSError(domain: "OpenNOW.SessionAd", code: statusCode, userInfo: [NSLocalizedDescriptionKey: description])
        }
        let sessionObj = json["session"] as? [String: Any] ?? [:]
        var updated = activeSession
        updated.status = sessionObj["status"] as? Int ?? updated.status
        updated.queuePosition = (sessionObj["queuePosition"] as? Int) ??
            ((sessionObj["seatSetupInfo"] as? [String: Any])?["queuePosition"] as? Int) ??
            updated.queuePosition
        updated.seatSetupStep = Self.extractSeatSetupStep(sessionObj: sessionObj) ?? updated.seatSetupStep
        // Keep ad state in sync with server response; clear stale queue-ad state when omitted.
        updated.adState = Self.extractAdState(sessionObj: sessionObj)
        return updated
    }

    func stopSession(session: AuthSession, activeSession: ActiveSession) async throws {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let base = Self.resolvePollBase(streamingBaseUrl: activeSession.streamingBaseUrl, serverIp: activeSession.serverIp)
        let url = URL(string: "\(base)/v2/session/\(activeSession.id)")!
        let (_, response) = try await request(
            url: url,
            method: "DELETE",
            headers: Self.cloudMatchHeaders(
                token: token,
                clientId: activeSession.clientId,
                deviceId: activeSession.deviceId,
                includeOrigin: false
            )
        )
        guard response.statusCode == 200 || response.statusCode == 204 else {
            throw NSError(domain: "OpenNOW.Session", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: "Failed to stop session"])
        }
    }

    func stopRemoteSession(
        session: AuthSession,
        candidate: RemoteSessionCandidate,
        streamingBaseUrl: String,
        vpcId: String
    ) async throws {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let targetHost = Self.remoteSessionTargetHost(
            serverIp: candidate.serverIp,
            streamingBaseUrl: streamingBaseUrl,
            vpcId: vpcId
        )
        let url = URL(string: "https://\(targetHost)/v2/session/\(candidate.id)")!
        let (_, response) = try await request(
            url: url,
            method: "DELETE",
            headers: Self.cloudMatchHeaders(
                token: token,
                clientId: UUID().uuidString,
                deviceId: UUID().uuidString,
                includeOrigin: false
            )
        )
        guard response.statusCode == 200 || response.statusCode == 204 else {
            throw NSError(domain: "OpenNOW.Session", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: "Failed to end remote session"])
        }
    }

    func fetchActiveSessions(
        session: AuthSession,
        streamingBaseUrl: String,
        vpcId: String
    ) async throws -> [RemoteSessionCandidate] {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let base = Self.normalizedStreamingBase(streamingBaseUrl, vpcId: vpcId)
        let url = URL(string: "\(base)/v2/session")!
        let (data, response) = try await request(
            url: url,
            method: "GET",
            headers: Self.cloudMatchHeaders(
                token: token,
                clientId: UUID().uuidString,
                deviceId: UUID().uuidString,
                includeOrigin: false
            )
        )
        guard response.statusCode == 200 else { return [] }
        let json = try parseJSON(data)
        let requestStatus = json["requestStatus"] as? [String: Any]
        guard (requestStatus?["statusCode"] as? Int) == 1 else { return [] }
        let sessions = json["sessions"] as? [[String: Any]] ?? []
        return sessions.compactMap { item in
            let status = item["status"] as? Int ?? 0
            guard status == 1 || status == 2 || status == 3 else { return nil }
            guard let sessionId = item["sessionId"] as? String else { return nil }
            let appId = (item["sessionRequestData"] as? [String: Any])?["appId"].flatMap { "\($0)" }
            let serverIp = Self.extractServerIp(sessionObj: item)
            return RemoteSessionCandidate(id: sessionId, appId: appId, status: status, serverIp: serverIp)
        }
    }

    func claimSession(
        session: AuthSession,
        candidate: RemoteSessionCandidate,
        game: CloudGame,
        streamingBaseUrl: String,
        vpcId: String,
        settings: AppSettings
    ) async throws -> ActiveSession {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let clientId = UUID().uuidString
        let deviceId = UUID().uuidString
        let deviceProfile = Self.streamDeviceProfile(for: game.title, settings: settings)
        let zoneBase = Self.normalizedStreamingBase(streamingBaseUrl, vpcId: vpcId)
        var effectiveServerIp = Self.remoteSessionTargetHost(
            serverIp: candidate.serverIp,
            streamingBaseUrl: zoneBase,
            vpcId: vpcId
        )

        if Self.isZoneHostname(effectiveServerIp) {
            do {
                let preflightURL = URL(string: "https://\(effectiveServerIp)/v2/session/\(candidate.id)")!
                let (prefetchData, prefetchResponse) = try await request(
                    url: preflightURL,
                    method: "GET",
                    headers: Self.cloudMatchHeaders(
                        token: token,
                        clientId: clientId,
                        deviceId: deviceId,
                        includeOrigin: false,
                        deviceProfile: deviceProfile
                    )
                )
                if prefetchResponse.statusCode == 200,
                   let prefetchJSON = try? parseJSON(prefetchData),
                   let prefetchSession = prefetchJSON["session"] as? [String: Any],
                   let realIp = Self.extractServerIp(sessionObj: prefetchSession),
                   !realIp.isEmpty {
                    effectiveServerIp = realIp
                }
            } catch {
            }
        }

        var validationSessionObj: [String: Any] = [:]
        var preClaimStatus: Int?
        do {
            let validationURL = URL(string: "https://\(effectiveServerIp)/v2/session/\(candidate.id)")!
            let (validationData, validationResponse) = try await request(
                url: validationURL,
                method: "GET",
                headers: Self.cloudMatchHeaders(
                    token: token,
                    clientId: clientId,
                    deviceId: deviceId,
                    includeOrigin: false,
                    deviceProfile: deviceProfile
                )
            )
            let validationJSON = validationResponse.statusCode == 200 ? (try? parseJSON(validationData)) ?? [:] : [:]
            validationSessionObj = validationJSON["session"] as? [String: Any] ?? [:]
            preClaimStatus = validationSessionObj["status"] as? Int
        } catch {
        }

        var claimJSON: [String: Any] = [:]
        if preClaimStatus != 1 {
            let claimURL = URL(string: "https://\(effectiveServerIp)/v2/session/\(candidate.id)?keyboardLayout=en-US&languageCode=en_US")!
            let claimBody = Self.buildClaimBody(
                sessionId: candidate.id,
                appId: candidate.appId ?? game.launchAppId ?? "0",
                settings: settings,
                deviceProfile: deviceProfile
            )

            let (claimData, claimResponse) = try await request(
                url: claimURL,
                method: "PUT",
                headers: Self.cloudMatchHeaders(
                    token: token,
                    clientId: clientId,
                    deviceId: deviceId,
                    includeOrigin: true,
                    deviceProfile: deviceProfile
                ),
                body: claimBody
            )
            guard claimResponse.statusCode == 200 else {
                let text = String(data: claimData, encoding: .utf8) ?? "unknown"
                throw NSError(domain: "OpenNOW.Session", code: claimResponse.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
            }
            claimJSON = (try? parseJSON(claimData)) ?? [:]
        }

        let claimSessionObj = claimJSON["session"] as? [String: Any] ?? [:]
        let resolvedSessionObj = claimSessionObj.isEmpty ? validationSessionObj : claimSessionObj
        let claimServerIp = Self.extractServerIp(sessionObj: claimSessionObj)
            ?? Self.extractServerIp(sessionObj: validationSessionObj)
            ?? candidate.serverIp
            ?? effectiveServerIp
        let mediaConnectionInfo = Self.extractMediaConnectionInfo(sessionObj: resolvedSessionObj)
        let signaling = Self.resolveSignaling(sessionObj: resolvedSessionObj, fallbackServerIp: claimServerIp)
        let iceServers = Self.extractIceServers(sessionObj: resolvedSessionObj)

        var active = ActiveSession(
            id: candidate.id,
            game: game,
            startedAt: .now,
            status: (resolvedSessionObj["status"] as? Int) ?? candidate.status,
            queuePosition: nil,
            seatSetupStep: Self.extractSeatSetupStep(sessionObj: resolvedSessionObj),
            serverIp: claimServerIp,
            mediaIp: mediaConnectionInfo.ip,
            mediaPort: mediaConnectionInfo.port,
            signalingServer: signaling.server ?? claimServerIp,
            signalingUrl: signaling.url ?? "wss://\(claimServerIp):443/nvst/",
            iceServers: iceServers,
            zone: vpcId,
            streamingBaseUrl: zoneBase,
            clientId: clientId,
            deviceId: deviceId,
            adState: Self.extractAdState(sessionObj: resolvedSessionObj)
        )

        for _ in 0..<45 {
            let polled = try await pollSession(session: session, activeSession: active)
            active = polled
            if active.status == 2 || active.status == 3 {
                return active
            }
            try? await Task.sleep(for: .seconds(1))
        }
        return active
    }

    private static func resolvePollBase(streamingBaseUrl: String, serverIp: String?) -> String {
        guard let serverIp, !serverIp.isEmpty else { return streamingBaseUrl }
        if streamingBaseUrl.contains("cloudmatchbeta.nvidiagrid.net") && !serverIp.contains("cloudmatchbeta.nvidiagrid.net") {
            return "https://\(serverIp)"
        }
        return streamingBaseUrl
    }

    private static func normalizedStreamingBase(_ streamingBaseUrl: String, vpcId: String) -> String {
        let trimmed = streamingBaseUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        if let host = URL(string: trimmed)?.host, !host.isEmpty {
            return trimmed.hasSuffix("/") ? String(trimmed.dropLast()) : trimmed
        }
        return "https://\(vpcId.lowercased()).cloudmatchbeta.nvidiagrid.net"
    }

    private static func remoteSessionTargetHost(serverIp: String?, streamingBaseUrl: String, vpcId: String) -> String {
        if let serverIp = normalizedEndpointHost(from: serverIp) {
            return serverIp
        }
        if let host = normalizedEndpointHost(from: URL(string: normalizedStreamingBase(streamingBaseUrl, vpcId: vpcId))?.host) {
            return host
        }
        return "\(vpcId.lowercased()).cloudmatchbeta.nvidiagrid.net"
    }

    private static func isZoneHostname(_ value: String) -> Bool {
        let normalized = value.lowercased()
        return normalized.contains("cloudmatchbeta.nvidiagrid.net") || normalized.contains("cloudmatch.nvidiagrid.net")
    }

    private static func extractZoneId(from streamingBaseUrl: String, fallback: String) -> String {
        guard let host = URL(string: streamingBaseUrl)?.host,
              let zoneId = host.split(separator: ".").first,
              !zoneId.isEmpty else {
            return fallback
        }
        return String(zoneId).uppercased()
    }

    private static func extractServerIp(sessionObj: [String: Any]) -> String? {
        if let connections = sessionObj["connectionInfo"] as? [[String: Any]] {
            if let usage14 = connections.first(where: { ($0["usage"] as? Int) == 14 }) {
                if let ip = normalizedEndpointHost(from: usage14["ip"]) {
                    return ip
                }
                if let resourceHost = normalizedEndpointHost(from: usage14["resourcePath"]) {
                    return resourceHost
                }
            }
            if let any = connections.first {
                if let ip = normalizedEndpointHost(from: any["ip"]) {
                    return ip
                }
            }
        }
        if let control = sessionObj["sessionControlInfo"] as? [String: Any] {
            if let ip = normalizedEndpointHost(from: control["ip"]) {
                return ip
            }
        }
        return nil
    }

    private static func extractMediaConnectionInfo(sessionObj: [String: Any]) -> (ip: String?, port: Int) {
        let connections = sessionObj["connectionInfo"] as? [[String: Any]] ?? []
        let fallbackServerIp = extractServerIp(sessionObj: sessionObj)

        func extractIp(from connection: [String: Any]) -> String? {
            if let ip = normalizedEndpointHost(from: connection["ip"]) {
                return ip
            }
            if let host = normalizedEndpointHost(from: connection["resourcePath"]) {
                return host
            }
            return nil
        }

        func extractPort(from connection: [String: Any]) -> Int {
            if let port = connection["port"] as? Int, port > 0 {
                return port
            }
            if let resourcePath = connection["resourcePath"] as? String,
               let parsedPort = URL(string: resourcePath.replacingOccurrences(of: "rtsps://", with: "https://").replacingOccurrences(of: "rtsp://", with: "http://"))?.port,
               parsedPort > 0 {
                return parsedPort
            }
            return 0
        }

        // Mirror CloudMatch media priority order used by desktop:
        // usage=2 -> usage=17 -> usage=1 -> usage=14 (highest port, server fallback).
        for usage in [2, 17, 1] {
            if let candidate = connections.first(where: { ($0["usage"] as? Int) == usage }) {
                let ip = extractIp(from: candidate)
                let port = extractPort(from: candidate)
                if ip != nil, port > 0 {
                    return (ip, port)
                }
            }
        }

        let usage14Connections = connections
            .filter { ($0["usage"] as? Int) == 14 }
            .sorted { extractPort(from: $0) > extractPort(from: $1) }

        for candidate in usage14Connections {
            let ip = extractIp(from: candidate) ?? fallbackServerIp
            let port = extractPort(from: candidate)
            if ip != nil, port > 0 {
                return (ip, port)
            }
        }

        return (nil, 0)
    }

    private static func extractSeatSetupStep(sessionObj: [String: Any]) -> Int? {
        let seatSetupInfo = sessionObj["seatSetupInfo"] as? [String: Any]
        if let step = seatSetupInfo?["seatSetupStep"] as? Int {
            return step
        }
        if let stepDouble = seatSetupInfo?["seatSetupStep"] as? Double, stepDouble.isFinite {
            return Int(stepDouble)
        }
        if let stepString = seatSetupInfo?["seatSetupStep"] as? String, let step = Int(stepString) {
            return step
        }
        return nil
    }

    private static func toPositiveInt(_ value: Any?) -> Int? {
        if let intValue = value as? Int, intValue > 0 {
            return intValue
        }
        if let doubleValue = value as? Double, doubleValue.isFinite {
            let normalized = Int(doubleValue)
            return normalized > 0 ? normalized : nil
        }
        if let stringValue = value as? String,
           let parsed = Int(stringValue.trimmingCharacters(in: .whitespacesAndNewlines)),
           parsed > 0 {
            return parsed
        }
        return nil
    }

    private static func toBoolean(_ value: Any?) -> Bool? {
        if let boolValue = value as? Bool {
            return boolValue
        }
        if let intValue = value as? Int {
            return intValue != 0
        }
        if let doubleValue = value as? Double {
            return doubleValue != 0
        }
        if let stringValue = value as? String {
            let normalized = stringValue.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            if normalized == "true" || normalized == "1" {
                return true
            }
            if normalized == "false" || normalized == "0" {
                return false
            }
        }
        return nil
    }

    private static func toOptionalString(_ value: Any?) -> String? {
        guard let stringValue = value as? String else { return nil }
        let trimmed = stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func normalizeSessionAdInfo(ad: [String: Any], index: Int) -> SessionAdInfo? {
        let adId = toOptionalString(ad["adId"]) ?? "ad-\(index + 1)"
        let mediaFiles = (ad["adMediaFiles"] as? [[String: Any]] ?? []).compactMap { item -> SessionAdMediaFile? in
            let mediaFileUrl = toOptionalString(item["mediaFileUrl"])
            let encodingProfile = toOptionalString(item["encodingProfile"])
            guard mediaFileUrl != nil || encodingProfile != nil else { return nil }
            return SessionAdMediaFile(mediaFileUrl: mediaFileUrl, encodingProfile: encodingProfile)
        }
        let adLengthInSeconds = ad["adLengthInSeconds"] as? Double
        let durationMs = adLengthInSeconds.map { Int(round($0 * 1000)) }
            ?? toPositiveInt(ad["durationMs"])
            ?? toPositiveInt(ad["durationInMs"])
        let state = ad["state"] as? Int
        let adState = ad["adState"] as? Int
        let adUrl = toOptionalString(ad["adUrl"])
        let mediaUrl = toOptionalString(ad["mediaUrl"]) ?? toOptionalString(ad["videoUrl"]) ?? toOptionalString(ad["url"])
        let clickThroughUrl = toOptionalString(ad["clickThroughUrl"])
        let title = toOptionalString(ad["title"])
        let description = toOptionalString(ad["description"])
        return SessionAdInfo(
            adId: adId,
            state: state,
            adState: adState,
            adUrl: adUrl,
            mediaUrl: mediaUrl,
            adMediaFiles: mediaFiles,
            clickThroughUrl: clickThroughUrl,
            adLengthInSeconds: adLengthInSeconds,
            durationMs: durationMs,
            title: title,
            description: description
        )
    }

    private static func extractAdState(sessionObj: [String: Any]) -> SessionAdState? {
        let sessionAdsRequired = toBoolean(sessionObj["sessionAdsRequired"])
            ?? toBoolean(sessionObj["isAdsRequired"])
            ?? toBoolean((sessionObj["sessionProgress"] as? [String: Any])?["isAdsRequired"])
            ?? toBoolean((sessionObj["progressInfo"] as? [String: Any])?["isAdsRequired"])
        let adsRaw = sessionObj["sessionAds"] as? [[String: Any]] ?? []
        let ads = adsRaw.enumerated().compactMap { normalizeSessionAdInfo(ad: $0.element, index: $0.offset) }
        let opportunityRaw = sessionObj["opportunity"] as? [String: Any]
        let opportunity = opportunityRaw.map {
            SessionOpportunityInfo(
                state: toOptionalString($0["state"]),
                queuePaused: toBoolean($0["queuePaused"]),
                gracePeriodSeconds: toPositiveInt($0["gracePeriodSeconds"]),
                message: toOptionalString($0["message"]),
                title: toOptionalString($0["title"]),
                description: toOptionalString($0["description"])
            )
        }
        let queuePaused = opportunity?.queuePaused ?? {
            guard let state = opportunity?.state else { return nil }
            return state.lowercased() == "graceperiodstart"
        }()
        let effectiveAdsRequired = sessionAdsRequired ?? !ads.isEmpty
        let message = opportunity?.message ?? opportunity?.description ?? (queuePaused == true ? "Resume ads to stay in queue." : nil)
        if !effectiveAdsRequired, ads.isEmpty, queuePaused != true, message == nil {
            return nil
        }
        return SessionAdState(
            isAdsRequired: effectiveAdsRequired,
            sessionAdsRequired: sessionAdsRequired,
            isQueuePaused: queuePaused,
            gracePeriodSeconds: opportunity?.gracePeriodSeconds,
            message: message,
            sessionAds: ads,
            ads: ads,
            opportunity: opportunity,
            serverSentEmptyAds: sessionObj["sessionAds"] is NSNull
        )
    }

    private static func resolveSignaling(sessionObj: [String: Any], fallbackServerIp: String?) -> (server: String?, url: String?) {
        let connections = sessionObj["connectionInfo"] as? [[String: Any]] ?? []
        let signalingConnection = connections.first(where: { ($0["usage"] as? Int) == 14 }) ?? connections.first
        let resourcePath = signalingConnection?["resourcePath"] as? String ?? "/nvst/"
        let serverIp = normalizedEndpointHost(from: fallbackServerIp) ?? extractServerIp(sessionObj: sessionObj)
        guard let serverIp, !serverIp.isEmpty else {
            return (nil, nil)
        }

        if resourcePath.hasPrefix("rtsps://") || resourcePath.hasPrefix("rtsp://") {
            if let host = normalizedEndpointHost(from: resourcePath) {
                return (host, "wss://\(host)/nvst/")
            }
            return (serverIp, "wss://\(serverIp):443/nvst/")
        }
        if resourcePath.hasPrefix("wss://"), let host = normalizedEndpointHost(from: resourcePath) {
            return (host, resourcePath)
        }
        if resourcePath.hasPrefix("/") {
            return (serverIp, "wss://\(serverIp):443\(resourcePath)")
        }
        return (serverIp, "wss://\(serverIp):443/nvst/")
    }

    private static func extractIceServers(sessionObj: [String: Any]) -> [IceServerConfig] {
        let config = sessionObj["iceServerConfiguration"] as? [String: Any]
        let raw = config?["iceServers"] as? [[String: Any]] ?? []
        let servers = raw.compactMap { entry -> IceServerConfig? in
            let urlsValue = entry["urls"]
            let urls: [String]
            if let list = urlsValue as? [String] {
                urls = list
            } else if let single = urlsValue as? String {
                urls = [single]
            } else {
                urls = []
            }
            guard !urls.isEmpty else { return nil }
            return IceServerConfig(
                urls: urls,
                username: entry["username"] as? String,
                credential: entry["credential"] as? String
            )
        }
        if !servers.isEmpty {
            return servers
        }
        return [
            IceServerConfig(urls: ["stun:stun.l.google.com:19302"], username: nil, credential: nil),
            IceServerConfig(urls: ["stun:stun1.l.google.com:19302"], username: nil, credential: nil)
        ]
    }

    private func fetchServerInfo(token: String, streamingBaseUrl: String) async throws -> (vpcId: String?, regions: [String]) {
        let normalized = streamingBaseUrl.hasSuffix("/") ? streamingBaseUrl : "\(streamingBaseUrl)/"
        let (data, response) = try await request(
            url: URL(string: "\(normalized)v2/serverInfo")!,
            headers: [
                "Accept": "application/json",
                "Authorization": "GFNJWT \(token)",
                "nv-client-id": GFNConstants.lcarsClientId,
                "nv-client-type": "BROWSER",
                "nv-client-version": GFNConstants.gfnClientVersion,
                "nv-client-streamer": "WEBRTC",
                "nv-device-os": "WINDOWS",
                "nv-device-type": "DESKTOP",
                "User-Agent": GFNConstants.userAgent
            ]
        )
        guard response.statusCode == 200 else {
            return (nil, [])
        }
        let json = try parseJSON(data)
        let requestStatus = json["requestStatus"] as? [String: Any]
        let vpcId = requestStatus?["serverId"] as? String
        let metadata = json["metaData"] as? [[String: Any]] ?? []
        let regionNames = metadata.compactMap { $0["key"] as? String }.filter { !$0.starts(with: "gfn-") && $0 != "gfn-regions" }
        return (vpcId, regionNames)
    }

    private func fetchPanels(token: String, panelNames: [String], vpcId: String) async throws -> [String: Any] {
        let variablesData: [String: Any] = [
            "vpcId": vpcId,
            "locale": "en_US",
            "panelNames": panelNames
        ]
        let variables = String(data: try JSONSerialization.data(withJSONObject: variablesData), encoding: .utf8) ?? "{}"
        let extensionsData: [String: Any] = ["persistedQuery": ["sha256Hash": GFNConstants.panelsQueryHash]]
        let extensions = String(data: try JSONSerialization.data(withJSONObject: extensionsData), encoding: .utf8) ?? "{}"
        var components = URLComponents(string: GFNConstants.graphQL)!
        components.queryItems = [
            .init(name: "requestType", value: panelNames.contains("LIBRARY") ? "panels/Library" : "panels/MainV2"),
            .init(name: "extensions", value: extensions),
            .init(name: "huId", value: UUID().uuidString.replacingOccurrences(of: "-", with: "")),
            .init(name: "variables", value: variables)
        ]
        let (data, response) = try await request(
            url: components.url!,
            headers: [
                "Accept": "application/json, text/plain, */*",
                "Content-Type": "application/graphql",
                "Origin": "https://play.geforcenow.com",
                "Referer": "https://play.geforcenow.com/",
                "Authorization": "GFNJWT \(token)",
                "nv-client-id": GFNConstants.lcarsClientId,
                "nv-client-type": "NATIVE",
                "nv-client-version": GFNConstants.gfnClientVersion,
                "nv-client-streamer": "NVIDIA-CLASSIC",
                "nv-device-os": "WINDOWS",
                "nv-device-type": "DESKTOP",
                "nv-browser-type": "CHROME",
                "User-Agent": GFNConstants.userAgent
            ]
        )
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.Games", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
        return try parseJSON(data)
    }

    private func fetchAppMetadata(token: String, appIds: [String], vpcId: String) async throws -> [String: Any] {
        let uniqueAppIds = Array(Set(appIds.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }))
        guard !uniqueAppIds.isEmpty else {
            return ["data": ["apps": ["items": []]]]
        }
        let variablesData: [String: Any] = [
            "vpcId": vpcId,
            "locale": "en_US",
            "appIds": uniqueAppIds
        ]
        let variables = String(data: try JSONSerialization.data(withJSONObject: variablesData), encoding: .utf8) ?? "{}"
        let extensionsData: [String: Any] = ["persistedQuery": ["sha256Hash": GFNConstants.appMetadataQueryHash]]
        let extensions = String(data: try JSONSerialization.data(withJSONObject: extensionsData), encoding: .utf8) ?? "{}"
        var components = URLComponents(string: GFNConstants.graphQL)!
        components.queryItems = [
            .init(name: "requestType", value: "appMetaData"),
            .init(name: "extensions", value: extensions),
            .init(name: "huId", value: UUID().uuidString.replacingOccurrences(of: "-", with: "")),
            .init(name: "variables", value: variables)
        ]
        let (data, response) = try await request(
            url: components.url!,
            headers: [
                "Accept": "application/json, text/plain, */*",
                "Content-Type": "application/graphql",
                "Origin": "https://play.geforcenow.com",
                "Referer": "https://play.geforcenow.com/",
                "Authorization": "GFNJWT \(token)",
                "nv-client-id": GFNConstants.lcarsClientId,
                "nv-client-type": "NATIVE",
                "nv-client-version": GFNConstants.gfnClientVersion,
                "nv-client-streamer": "NVIDIA-CLASSIC",
                "nv-device-os": "WINDOWS",
                "nv-device-type": "DESKTOP",
                "nv-device-make": "UNKNOWN",
                "nv-device-model": "UNKNOWN",
                "nv-browser-type": "CHROME",
                "User-Agent": GFNConstants.userAgent
            ]
        )
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.GameMetadata", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
        return try parseJSON(data)
    }

    private func enrichGamesWithMetadata(token: String, vpcId: String, games: [CloudGame]) async throws -> [CloudGame] {
        let appIds = Array(Set(games.compactMap(\.uuid)))
        guard !appIds.isEmpty else { return games }

        var metadataById: [String: [String: Any]] = [:]
        let chunkSize = 40
        for start in stride(from: 0, to: appIds.count, by: chunkSize) {
            let chunk = Array(appIds[start..<min(start + chunkSize, appIds.count)])
            let payload = try await fetchAppMetadata(token: token, appIds: chunk, vpcId: vpcId)
            if let errors = payload["errors"] as? [[String: Any]], !errors.isEmpty {
                let message = errors.compactMap { $0["message"] as? String }.joined(separator: ", ")
                throw NSError(domain: "OpenNOW.GameMetadata", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
            }
            let data = payload["data"] as? [String: Any]
            let apps = data?["apps"] as? [String: Any]
            let items = apps?["items"] as? [[String: Any]] ?? []
            for app in items {
                if let id = app["id"] as? String {
                    metadataById[id] = app
                }
            }
        }

        return games.map { game in
            guard let uuid = game.uuid, let app = metadataById[uuid] else { return game }
            return Self.mergeGameMetadata(game: game, app: app)
        }
    }

    private func fetchUser(tokens: AuthTokens) async throws -> UserProfile {
        let jwtPayload = Self.decodeJWTPayload(token: tokens.idToken ?? tokens.accessToken)
        if let sub = jwtPayload["sub"] as? String {
            let email = jwtPayload["email"] as? String
            let emailDisplayName = email?
                .split(separator: "@", maxSplits: 1)
                .first
                .map { String($0) }
            let displayName = (jwtPayload["preferred_username"] as? String) ?? emailDisplayName ?? "User"
            return UserProfile(userId: sub, displayName: displayName, email: email, membershipTier: (jwtPayload["gfn_tier"] as? String) ?? "FREE")
        }

        let (data, response) = try await request(
            url: GFNConstants.userInfoEndpoint,
            headers: [
                "Authorization": "Bearer \(tokens.accessToken)",
                "Origin": "https://nvfile",
                "Accept": "application/json",
                "User-Agent": GFNConstants.userAgent
            ]
        )
        guard response.statusCode == 200 else {
            throw NSError(domain: "OpenNOW.Auth", code: response.statusCode)
        }
        let json = try parseJSON(data)
        guard let sub = json["sub"] as? String else {
            throw NSError(domain: "OpenNOW.Auth", code: 7)
        }
        let email = json["email"] as? String
        let emailDisplayName = email?
            .split(separator: "@", maxSplits: 1)
            .first
            .map { String($0) }
        let displayName = (json["preferred_username"] as? String) ?? emailDisplayName ?? "User"
        return UserProfile(userId: sub, displayName: displayName, email: email, membershipTier: "FREE")
    }

    private func requestClientToken(accessToken: String) async throws -> (token: String, expiresAt: TimeInterval) {
        let (data, response) = try await request(
            url: GFNConstants.clientTokenEndpoint,
            headers: [
                "Authorization": "Bearer \(accessToken)",
                "Origin": "https://nvfile",
                "Accept": "application/json, text/plain, */*",
                "User-Agent": GFNConstants.userAgent
            ]
        )
        guard response.statusCode == 200 else {
            throw NSError(domain: "OpenNOW.Auth", code: response.statusCode)
        }
        let json = try parseJSON(data)
        let token = json["client_token"] as? String ?? ""
        let expiresIn = json["expires_in"] as? Double ?? 3600
        return (token, Date().addingTimeInterval(expiresIn).timeIntervalSince1970)
    }

    private func fetchMembershipTier(token: String, userId: String, streamingBaseUrl: String) async throws -> String {
        let serverInfo = try await fetchServerInfo(token: token, streamingBaseUrl: streamingBaseUrl)
        let vpcId = serverInfo.vpcId ?? "NP-AMS-08"
        var components = URLComponents(url: GFNConstants.mesEndpoint, resolvingAgainstBaseURL: false)!
        components.queryItems = [
            .init(name: "serviceName", value: "gfn_pc"),
            .init(name: "languageCode", value: "en_US"),
            .init(name: "vpcId", value: vpcId),
            .init(name: "userId", value: userId)
        ]
        let (data, response) = try await request(
            url: components.url!,
            headers: [
                "Authorization": "GFNJWT \(token)",
                "Accept": "application/json",
                "nv-client-id": GFNConstants.lcarsClientId,
                "nv-client-type": "NATIVE",
                "nv-client-version": GFNConstants.gfnClientVersion,
                "nv-client-streamer": "NVIDIA-CLASSIC",
                "nv-device-os": "WINDOWS",
                "nv-device-type": "DESKTOP"
            ]
        )
        guard response.statusCode == 200 else { return "FREE" }
        let json = try parseJSON(data)
        return (json["membershipTier"] as? String) ?? "FREE"
    }

    private static func flattenPanels(payload: [String: Any]) -> [CloudGame] {
        guard let data = payload["data"] as? [String: Any],
              let panels = data["panels"] as? [[String: Any]] else {
            return []
        }
        var seen = Set<String>()
        var out: [CloudGame] = []
        for panel in panels {
            let sections = panel["sections"] as? [[String: Any]] ?? []
            for section in sections {
                let items = section["items"] as? [[String: Any]] ?? []
                for item in items {
                    guard let type = item["__typename"] as? String, type == "GameItem",
                          let app = item["app"] as? [String: Any] else { continue }
                    guard let appId = app["id"] as? String,
                          let title = app["title"] as? String else { continue }
                    let variants = app["variants"] as? [[String: Any]] ?? []
                    let selectedVariant = variants.first(where: { (($0["gfn"] as? [String: Any])?["library"] as? [String: Any])?["selected"] as? Bool == true }) ?? variants.first
                    let selectedVariantId = selectedVariant?["id"] as? String
                    let numericVariant = variants.compactMap { $0["id"] as? String }.first(where: { Int($0) != nil })
                    let launchAppId = [selectedVariantId, numericVariant, appId].compactMap { $0 }.first(where: { Int($0) != nil })
                    var launchOptions: [GameLaunchOption] = []
                    var seenLaunchOptionIds = Set<String>()
                    for variant in variants {
                        guard let variantId = variant["id"] as? String,
                              Int(variantId) != nil else {
                            continue
                        }
                        let storefront = ((variant["appStore"] as? String) ?? "Auto")
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                        let option = GameLaunchOption(
                            storefront: storefront.isEmpty ? "Auto" : storefront,
                            appId: variantId,
                            supportedControls: toOptionalStringArray(variant["supportedControls"])
                        )
                        if seenLaunchOptionIds.insert(option.id).inserted {
                            launchOptions.append(option)
                        }
                    }
                    if let launchAppId {
                        let selectedStore = ((selectedVariant?["appStore"] as? String) ?? "Auto")
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                        let defaultOption = GameLaunchOption(
                            storefront: selectedStore.isEmpty ? "Auto" : selectedStore,
                            appId: launchAppId,
                            supportedControls: toOptionalStringArray(selectedVariant?["supportedControls"])
                        )
                        if seenLaunchOptionIds.insert(defaultOption.id).inserted {
                            launchOptions.insert(defaultOption, at: 0)
                        }
                    }
                    let store = launchOptions.first?.storefront ?? (selectedVariant?["appStore"] as? String) ?? "Unknown"
                    let id = "\(appId):\(selectedVariantId ?? "default")"
                    let imageUrl: String? = {
                        guard let images = app["images"] as? [String: Any] else { return nil }
                        let raw = (images["GAME_BOX_ART"] as? String)
                            ?? (images["TV_BANNER"] as? String)
                            ?? (images["HERO_IMAGE"] as? String)
                        return optimizedImageURL(raw)
                    }()
                    if seen.contains(id) { continue }
                    seen.insert(id)
                    let genre = (((app["genres"] as? [[String: Any]])?.first)?["name"] as? String) ?? "Cloud Game"
                    let metadata = extractGameMetadata(
                        app: app,
                        selectedVariant: selectedVariant,
                        launchOptions: launchOptions
                    )
                    let icon: String = {
                        let lower = title.lowercased()
                        if lower.contains("fortnite") { return "bolt.fill" }
                        if lower.contains("apex") { return "scope" }
                        if lower.contains("cyberpunk") { return "sparkles.tv" }
                        if lower.contains("sky") { return "globe.americas.fill" }
                        if lower.contains("call of duty") { return "target" }
                        return "gamecontroller.fill"
                    }()
                    out.append(
                        CloudGame(
                            id: id,
                            title: title,
                            genre: genre,
                            platform: store,
                            icon: icon,
                            imageUrl: imageUrl,
                            launchAppId: launchAppId,
                            launchOptions: launchOptions,
                            uuid: appId,
                            summary: metadata.summary,
                            longDescription: metadata.longDescription,
                            publisher: metadata.publisher,
                            developer: metadata.developer,
                            releaseDate: metadata.releaseDate,
                            featureLabels: metadata.featureLabels,
                            tags: metadata.tags,
                            stores: metadata.stores,
                            playType: metadata.playType,
                            membershipTierLabel: metadata.membershipTierLabel
                        )
                    )
                }
            }
        }
        return out
    }

    private static func extractGameMetadata(
        app: [String: Any],
        selectedVariant: [String: Any]?,
        launchOptions: [GameLaunchOption]
    ) -> (
        summary: String?,
        longDescription: String?,
        publisher: String?,
        developer: String?,
        releaseDate: String?,
        featureLabels: [String]?,
        tags: [String]?,
        stores: [String]?,
        playType: String?,
        membershipTierLabel: String?
    ) {
        let summary = toOptionalString(app["description"])
            ?? toOptionalString(app["shortDescription"])
            ?? toOptionalString(app["tagLine"])
            ?? toOptionalString(app["synopsis"])
        let longDescription = toOptionalString(app["longDescription"])
            ?? toOptionalString(app["fullDescription"])
            ?? toOptionalString(app["localizedDescription"])
        let publisher = toOptionalString(app["publisher"])
            ?? toOptionalString((app["publisherInfo"] as? [String: Any])?["name"])
            ?? toOptionalString((app["publisherInfo"] as? [String: Any])?["displayName"])
        let developer = toOptionalString(app["developer"])
            ?? toOptionalString((app["developerInfo"] as? [String: Any])?["name"])
            ?? toOptionalString((app["studio"] as? [String: Any])?["name"])
        let releaseDateRaw = toOptionalString(app["releaseDate"])
            ?? toOptionalString(app["releaseDateTime"])
            ?? toOptionalString(selectedVariant?["releaseDate"])
        let releaseDate = formatReleaseDate(releaseDateRaw)
        let genreNames = ((app["genres"] as? [[String: Any]]) ?? [])
            .compactMap { toOptionalString($0["name"]) }
        let tagNames = toOptionalStringArray(app["tags"])
            ?? toOptionalStringArray(app["keywords"])
        let featureLabels = extractFeatureLabels(app: app)
        let mergedTags = Array(Set((genreNames + (tagNames ?? [])))).sorted()
        let stores = Array(
            Set(launchOptions.map(\.storefront).filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty })
        )
        let gfn = app["gfn"] as? [String: Any]
        return (
            summary: summary,
            longDescription: longDescription,
            publisher: publisher,
            developer: developer,
            releaseDate: releaseDate,
            featureLabels: featureLabels.isEmpty ? nil : featureLabels,
            tags: mergedTags.isEmpty ? nil : mergedTags,
            stores: stores.isEmpty ? nil : stores.sorted(),
            playType: toOptionalString(gfn?["playType"]),
            membershipTierLabel: toOptionalString(gfn?["minimumMembershipTierLabel"])
        )
    }

    private static func mergeGameMetadata(game: CloudGame, app: [String: Any]) -> CloudGame {
        let metadata = extractGameMetadata(
            app: app,
            selectedVariant: selectedVariant(for: game, app: app),
            launchOptions: launchOptions(for: game, app: app)
        )
        let imageUrl: String? = {
            guard let images = app["images"] as? [String: Any] else { return game.imageUrl }
            let raw = (images["GAME_BOX_ART"] as? String)
                ?? (images["TV_BANNER"] as? String)
                ?? (images["HERO_IMAGE"] as? String)
            return optimizedImageURL(raw) ?? game.imageUrl
        }()

        return CloudGame(
            id: game.id,
            title: toOptionalString(app["title"]) ?? game.title,
            genre: game.genre,
            platform: game.platform,
            icon: game.icon,
            imageUrl: imageUrl,
            launchAppId: game.launchAppId,
            launchOptions: launchOptions(for: game, app: app),
            uuid: game.uuid,
            summary: metadata.summary ?? game.summary,
            longDescription: metadata.longDescription ?? game.longDescription,
            publisher: metadata.publisher ?? game.publisher,
            developer: metadata.developer ?? game.developer,
            releaseDate: metadata.releaseDate ?? game.releaseDate,
            featureLabels: metadata.featureLabels ?? game.featureLabels,
            tags: metadata.tags ?? game.tags,
            stores: metadata.stores ?? game.stores,
            playType: metadata.playType ?? game.playType,
            membershipTierLabel: metadata.membershipTierLabel ?? game.membershipTierLabel
        )
    }

    private static func selectedVariant(for game: CloudGame, app: [String: Any]) -> [String: Any]? {
        let variants = app["variants"] as? [[String: Any]] ?? []
        if let selected = variants.first(where: { (($0["gfn"] as? [String: Any])?["library"] as? [String: Any])?["selected"] as? Bool == true }) {
            return selected
        }
        if let launchAppId = game.launchAppId,
           let matching = variants.first(where: { ($0["id"] as? String) == launchAppId }) {
            return matching
        }
        return variants.first
    }

    private static func launchOptions(for game: CloudGame, app: [String: Any]) -> [GameLaunchOption] {
        let variants = app["variants"] as? [[String: Any]] ?? []
        guard !variants.isEmpty else { return game.launchOptions }
        var options: [GameLaunchOption] = []
        var seen = Set<String>()
        for variant in variants {
            guard let variantId = variant["id"] as? String,
                  Int(variantId) != nil else {
                continue
            }
            let storefront = ((variant["appStore"] as? String) ?? "Auto")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let option = GameLaunchOption(
                storefront: storefront.isEmpty ? "Auto" : storefront,
                appId: variantId,
                supportedControls: toOptionalStringArray(variant["supportedControls"])
            )
            if seen.insert(option.id).inserted {
                options.append(option)
            }
        }
        return options.isEmpty ? game.launchOptions : options
    }

    private static func extractFeatureLabels(app: [String: Any]) -> [String] {
        let buckets = [
            app["features"],
            app["gameFeatures"],
            app["appFeatures"],
            app["tags"]
        ]
        let labels = buckets.flatMap { toOptionalStringArray($0) ?? [] }
        return Array(Set(labels)).sorted()
    }

    private static func toOptionalStringArray(_ value: Any?) -> [String]? {
        if let list = value as? [String] {
            let cleaned = list.compactMap { toOptionalString($0) }
            return cleaned.isEmpty ? nil : cleaned
        }
        if let list = value as? [[String: Any]] {
            let cleaned = list.compactMap {
                toOptionalString($0["name"]) ?? toOptionalString($0["title"]) ?? toOptionalString($0["label"])
            }
            return cleaned.isEmpty ? nil : cleaned
        }
        return nil
    }

    private static func formatReleaseDate(_ raw: String?) -> String? {
        guard let raw else { return nil }
        let iso = ISO8601DateFormatter()
        if let date = iso.date(from: raw) {
            let formatter = DateFormatter()
            formatter.locale = .autoupdatingCurrent
            formatter.dateStyle = .medium
            return formatter.string(from: date)
        }
        if let seconds = TimeInterval(raw) {
            let date = Date(timeIntervalSince1970: seconds > 1_000_000_000_000 ? seconds / 1000 : seconds)
            let formatter = DateFormatter()
            formatter.locale = .autoupdatingCurrent
            formatter.dateStyle = .medium
            return formatter.string(from: date)
        }
        return raw
    }

    private static func optimizedImageURL(_ raw: String?) -> String? {
        guard let raw, !raw.isEmpty else { return nil }
        guard raw.contains("img.nvidiagrid.net") else { return raw }
        return "\(raw)"
    }

    private static func cloudMatchHeaders(
        token: String,
        clientId: String,
        deviceId: String,
        includeOrigin: Bool,
        deviceProfile: StreamDeviceProfile = .desktop
    ) -> [String: String] {
        var headers: [String: String] = [
            "User-Agent": GFNConstants.userAgent,
            "Authorization": "GFNJWT \(token)",
            "Content-Type": "application/json",
            "nv-browser-type": "CHROME",
            "nv-client-id": clientId,
            "nv-client-streamer": "NVIDIA-CLASSIC",
            "nv-client-type": "NATIVE",
            "nv-client-version": GFNConstants.gfnClientVersion,
            "nv-device-make": "APPLE",
            "nv-device-model": UIDevice.current.model,
            "nv-device-os": deviceProfile.nvDeviceOS,
            "nv-device-type": deviceProfile.nvDeviceType,
            "x-device-id": deviceId
        ]
        if includeOrigin {
            headers["Origin"] = "https://play.geforcenow.com"
            headers["Referer"] = "https://play.geforcenow.com/"
        }
        return headers
    }

    private static func buildSessionBody(
        appId: String,
        title: String,
        fps: Int,
        launcherName: String,
        deviceProfile: StreamDeviceProfile
    ) -> Data {
        let metadata: [[String: String]] = [
            ["key": "SubSessionId", "value": UUID().uuidString],
            ["key": "wssignaling", "value": "1"],
            ["key": "GSStreamerType", "value": "WebRTC"],
            ["key": "networkType", "value": "Unknown"],
            ["key": "ClientImeSupport", "value": "0"],
            ["key": "preferredLauncher", "value": launcherName],
            ["key": "clientPhysicalResolution", "value": "{\"horizontalPixels\":1920,\"verticalPixels\":1080}"],
            ["key": "surroundAudioInfo", "value": "2"]
        ] + deviceProfile.metadata
        let body: [String: Any] = [
            "sessionRequestData": [
                "appId": appId,
                "internalTitle": title,
                "availableSupportedControllers": [],
                "networkTestSessionId": NSNull(),
                "parentSessionId": NSNull(),
                "clientIdentification": deviceProfile.clientIdentification,
                "deviceHashId": UUID().uuidString,
                "clientVersion": "30.0",
                "sdkVersion": "1.0",
                "streamerVersion": 1,
                "clientPlatformName": deviceProfile.clientPlatformName,
                "clientRequestMonitorSettings": [[
                    "widthInPixels": 1920,
                    "heightInPixels": 1080,
                    "framesPerSecond": fps,
                    "sdrHdrMode": 0,
                    "displayData": [
                        "desiredContentMaxLuminance": 0,
                        "desiredContentMinLuminance": 0,
                        "desiredContentMaxFrameAverageLuminance": 0
                    ],
                    "dpi": 100
                ]],
                "useOps": true,
                "audioMode": 2,
                "metaData": metadata,
                "sdrHdrMode": 0,
                "surroundAudioInfo": 0,
                "remoteControllersBitmap": 0,
                "clientTimezoneOffset": -TimeZone.current.secondsFromGMT() * 1000,
                "enhancedStreamMode": 1,
                "appLaunchMode": 1,
                "secureRTSPSupported": false,
                "partnerCustomData": "",
                "accountLinked": true,
                "enablePersistingInGameSettings": true,
                "userAge": 26,
                "requestedStreamingFeatures": [
                    "reflex": fps >= 120,
                    "bitDepth": 0,
                    "cloudGsync": false,
                    "enabledL4S": false,
                    "mouseMovementFlags": 0,
                    "trueHdr": false,
                    "profile": 0,
                    "chromaFormat": 0,
                    "prefilterMode": 0,
                    "prefilterSharpness": 0,
                    "prefilterNoiseReduction": 0,
                    "hudStreamingMode": 0,
                    "sdrColorSpace": 2,
                    "hdrColorSpace": 0
                ]
            ]
        ]
        return (try? JSONSerialization.data(withJSONObject: body)) ?? Data()
    }

    private static func buildClaimBody(
        sessionId: String,
        appId: String,
        settings: AppSettings,
        deviceProfile: StreamDeviceProfile
    ) -> Data {
        let metadata: [[String: String]] = [
            ["key": "SubSessionId", "value": UUID().uuidString],
            ["key": "wssignaling", "value": "1"],
            ["key": "GSStreamerType", "value": "WebRTC"],
            ["key": "networkType", "value": "Unknown"],
            ["key": "ClientImeSupport", "value": "0"]
        ] + deviceProfile.metadata
        let body: [String: Any] = [
            "action": 2,
            "data": "RESUME",
            "sessionRequestData": [
                "audioMode": 2,
                "remoteControllersBitmap": 0,
                "sdrHdrMode": 0,
                "networkTestSessionId": NSNull(),
                "availableSupportedControllers": [],
                "clientVersion": "30.0",
                "deviceHashId": UUID().uuidString,
                "internalTitle": NSNull(),
                "clientPlatformName": deviceProfile.clientPlatformName,
                "metaData": metadata,
                "surroundAudioInfo": 0,
                "clientTimezoneOffset": -TimeZone.current.secondsFromGMT() * 1000,
                "clientIdentification": deviceProfile.clientIdentification,
                "parentSessionId": NSNull(),
                "appId": Int(appId) ?? 0,
                "streamerVersion": 1,
                "appLaunchMode": 1,
                "sdkVersion": "1.0",
                "enhancedStreamMode": 1,
                "useOps": true,
                "clientDisplayHdrCapabilities": NSNull(),
                "accountLinked": true,
                "partnerCustomData": "",
                "enablePersistingInGameSettings": true,
                "secureRTSPSupported": false,
                "userAge": 26,
                "requestedStreamingFeatures": [
                    "reflex": settings.preferredFPS >= 120,
                    "bitDepth": 0,
                    "cloudGsync": false,
                    "enabledL4S": false,
                    "profile": 0,
                    "fallbackToLogicalResolution": false,
                    "chromaFormat": 0,
                    "prefilterMode": 0,
                    "hudStreamingMode": 0
                ]
            ],
            "metaData": []
        ]
        return (try? JSONSerialization.data(withJSONObject: body)) ?? Data()
    }

    private static func streamDeviceProfile(for gameTitle: String, settings: AppSettings) -> StreamDeviceProfile {
        guard settings.fortnitePrefersNativeTouch else { return .desktop }
        let normalized = gameTitle.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if normalized.contains("fortnite") {
            return .mobileTouch
        }
        return .desktop
    }

    private static func generatePKCE() -> (verifier: String, challenge: String) {
        let verifier = (0..<64).map { _ in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~".randomElement()! }
            .map(String.init).joined()
        let challengeData = Data(verifier.utf8)
        let hash = SHA256.hash(data: challengeData)
        let challenge = Data(hash).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
        return (verifier, challenge)
    }

    private static func decodeJWTPayload(token: String) -> [String: Any] {
        let parts = token.split(separator: ".")
        guard parts.count == 3 else { return [:] }
        var payload = String(parts[1])
        payload = payload.replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        while payload.count % 4 != 0 { payload.append("=") }
        guard let data = Data(base64Encoded: payload),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return [:]
        }
        return object
    }
}

private enum URLQueryItemEncoder {
    static func encode(_ values: [String: String]) -> String {
        var components = URLComponents()
        components.queryItems = values.map { URLQueryItem(name: $0.key, value: $0.value) }
        return components.percentEncodedQuery ?? ""
    }
}

@MainActor
final class OpenNOWStore: ObservableObject {
    @Published private(set) var user: UserProfile?
    @Published private(set) var providers: [LoginProvider] = []
    @Published private(set) var allGames: [CloudGame] = []
    @Published private(set) var featuredGames: [CloudGame] = []
    @Published private(set) var libraryGames: [CloudGame] = []
    @Published private(set) var activeSession: ActiveSession?
    @Published private(set) var resumableSessions: [RemoteSessionCandidate] = []
    @Published private(set) var telemetry = SessionTelemetry(pingMs: 0, fps: 0, packetLossPercent: 0, bitrateMbps: 0)
    @Published private(set) var sessionElapsedSeconds = 0
    @Published private(set) var subscription: SubscriptionSnapshot?
    @Published var settings: AppSettings
    @Published var searchText = ""
    @Published var micEnabled = false
    @Published var recordingEnabled = false
    @Published var controllerConnected = true
    @Published var isAuthenticating = false
    @Published var isLoadingGames = false
    @Published var isLaunchingSession = false
    @Published var showStreamLoading: Bool = false
    @Published var queueOverlayVisible: Bool = false
    @Published var streamSession: ActiveSession?
    @Published var lastError: String?
    @Published var isBootstrapping: Bool = true
    #if os(tvOS)
    @Published private(set) var tvAuthLogs: [String] = []
    #endif

    private let api = GFNAPIClient()
    private let logger = Logger(subsystem: "OpenNOWiOS", category: "Session")
    private let defaults = UserDefaults.standard
    private var authSession: AuthSession?
    private var telemetryTask: Task<Void, Never>?
    private var sessionPollTask: Task<Void, Never>?
    private var launchTask: Task<Void, Never>?
    #if os(tvOS)
    private var sessionPollBackgroundTaskActive = false
    #else
    private var sessionPollBackgroundTaskId: UIBackgroundTaskIdentifier = .invalid
    #endif
    private var cachedVpcId: String = "GFN-PC"
    private var adReportStateById: [String: SessionAdAction] = [:]
    private var adStartedAtById: [String: Date] = [:]
    private var reopenToken: UUID = UUID()
    private var currentScenePhase: ScenePhase = .active
    #if os(tvOS)
    private var tvAuthLogObserver: NSObjectProtocol?
    #endif

    private let settingsKey = "OpenNOW.iOS.settings"
    private let authSessionKey = "OpenNOW.iOS.authSession"
    private let activeSessionSnapshotKey = "OpenNOW.iOS.activeSession"
    private let deviceIdKey = "OpenNOW.iOS.deviceId"
    private let setupPhaseTimeoutSeconds: TimeInterval = 90

    init() {
        var loadedSettings = Self.loadSettings(from: defaults) ?? .default
        if loadedSettings.preferredCodec == "HEVC" {
            loadedSettings.preferredCodec = "H265"
        }
        settings = loadedSettings
        authSession = Self.loadAuthSession(from: defaults)
        activeSession = Self.loadActiveSession(from: defaults)
        user = authSession?.user
        showStreamLoading = activeSession != nil
        syncTrackedSessionSurface()
        #if os(tvOS)
        tvAuthLogObserver = NotificationCenter.default.addObserver(
            forName: TVAuthDiagnostics.notificationName,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let message = notification.userInfo?["message"] as? String else { return }
            Task { @MainActor [weak self] in
                self?.appendTVAuthLog(message)
            }
        }
        #endif
    }

    deinit {
        launchTask?.cancel()
        telemetryTask?.cancel()
        sessionPollTask?.cancel()
        #if os(tvOS)
        if let tvAuthLogObserver {
            NotificationCenter.default.removeObserver(tvAuthLogObserver)
        }
        #endif
    }

    var supportsNativeOAuth: Bool { OpenNOWPlatform.supportsNativeOAuth }
    var supportsEmbeddedStreamer: Bool { OpenNOWPlatform.supportsEmbeddedStreamer }

    func bootstrap() async {
        defer { isBootstrapping = false }
        await NotificationManager.shared.requestPermission()
        providers = await api.fetchProviders()
        if settings.selectedProviderIdpId.isEmpty {
            settings.selectedProviderIdpId = providers.first?.idpId ?? GFNConstants.defaultProvider.idpId
            persistSettings()
        }

        if let existing = authSession {
            if let refreshed = try? await api.refreshSession(existing) {
                authSession = refreshed
                user = refreshed.user
                persistAuthSession(refreshed)
                await refreshCatalog()
                restoreTrackedSessionIfNeeded()
            }
        }
        syncTrackedSessionSurface()
    }

    func signIn() async {
        lastError = nil
        isAuthenticating = true
        defer { isAuthenticating = false }
        #if os(tvOS)
        clearTVAuthLogs()
        #endif

        guard supportsNativeOAuth else {
            lastError = OpenNOWPlatform.authUnavailableReason
            return
        }

        let provider = providers.first(where: { $0.idpId == settings.selectedProviderIdpId }) ?? GFNConstants.defaultProvider
        do {
            let session = try await api.login(with: provider, deviceId: persistentDeviceId())
            authSession = session
            user = session.user
            persistAuthSession(session)
            await refreshCatalog()
        } catch {
            TVAuthDiagnostics.record("Store sign-in failed: \(error.localizedDescription)")
            lastError = "Sign in failed: \(error.localizedDescription)"
        }
    }

    #if os(tvOS)
    func signInOnTVOS(authenticate: @escaping TVOSOAuthPresenter) async {
        lastError = nil
        isAuthenticating = true
        defer { isAuthenticating = false }
        clearTVAuthLogs()

        let provider = providers.first(where: { $0.idpId == settings.selectedProviderIdpId }) ?? GFNConstants.defaultProvider
        do {
            let session = try await api.login(
                with: provider,
                deviceId: persistentDeviceId(),
                authenticate: authenticate
            )
            authSession = session
            user = session.user
            persistAuthSession(session)
            await refreshCatalog()
        } catch {
            TVAuthDiagnostics.record("Store sign-in failed: \(error.localizedDescription)")
            lastError = "Sign in failed: \(error.localizedDescription)"
        }
    }
    #endif

    func signOut() {
        Task { await NotificationManager.shared.cancelSessionNotifications() }
        user = nil
        authSession = nil
        allGames = []
        featuredGames = []
        libraryGames = []
        resumableSessions = []
        activeSession = nil
        setStreamSession(nil, reason: "signOut")
        subscription = nil
        telemetryTask?.cancel()
        sessionPollTask?.cancel()
        endSessionPollBackgroundTask()
        sessionElapsedSeconds = 0
        showStreamLoading = false
        queueOverlayVisible = false
        adReportStateById = [:]
        adStartedAtById = [:]
        defaults.removeObject(forKey: authSessionKey)
        syncTrackedSessionSurface()
    }

    func refreshCatalog() async {
        guard let session = authSession else { return }
        isLoadingGames = true
        defer { isLoadingGames = false }

        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            user = refreshed.user
            persistAuthSession(refreshed)

            let (mainGames, vpcId) = try await api.fetchMainGames(session: refreshed)
            cachedVpcId = vpcId
            let library = try await api.fetchLibraryGames(session: refreshed, vpcId: vpcId)
            let sub = try? await api.fetchSubscription(session: refreshed, vpcId: vpcId)

            allGames = mainGames
            featuredGames = Array(mainGames.prefix(8))
            libraryGames = library
            subscription = sub
            resumableSessions = (try? await api.fetchActiveSessions(
                session: refreshed,
                streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                vpcId: vpcId
            )) ?? []
            if let sub {
                user?.membershipTier = sub.membershipTier
            }
            lastError = nil
        } catch is CancellationError {
            // Pull-to-refresh can cancel an in-flight request; treat as non-failure.
            return
        } catch let nsError as NSError
            where nsError.domain == NSURLErrorDomain && nsError.code == NSURLErrorCancelled {
            return
        } catch {
            lastError = "Failed to load games: \(error.localizedDescription)"
        }
    }

    func launch(game: CloudGame, zoneUrl: String? = nil, launchOption: GameLaunchOption? = nil) async {
        guard supportsEmbeddedStreamer else {
            lastError = OpenNOWPlatform.streamingUnavailableReason
            return
        }
        guard let session = authSession else {
            lastError = "Sign in first."
            return
        }
        isLaunchingSession = true
        showStreamLoading = true
        queueOverlayVisible = true
        defer { isLaunchingSession = false }
        do {
            logger.info("Launch requested game=\(game.title, privacy: .public) zoneUrl=\(zoneUrl ?? "default", privacy: .public)")
            let started = try await api.startSession(
                session: session,
                game: game,
                vpcId: cachedVpcId,
                settings: settings,
                streamingBaseUrl: zoneUrl,
                launchAppIdOverride: launchOption?.appId,
                launcherName: launchOption?.storefront ?? "Auto"
            )
            activeSession = started
            adReportStateById = [:]
            adStartedAtById = [:]
            sessionElapsedSeconds = 0
            startSessionTasks()
            syncTrackedSessionSurface()
            logger.info("Session started id=\(started.id, privacy: .public) status=\(started.status) queue=\(started.queuePosition ?? -1)")
            lastError = nil
        } catch is CancellationError {
            return
        } catch {
            logger.error("Session launch failed error=\(error.localizedDescription, privacy: .public)")
            showStreamLoading = false
            queueOverlayVisible = false
            syncTrackedSessionSurface()
            lastError = "Session launch failed: \(error.localizedDescription)"
        }
    }

    func scheduleLaunch(game: CloudGame, zoneUrl: String? = nil, launchOption: GameLaunchOption? = nil) {
        launchTask?.cancel()
        launchTask = Task { await self.launch(game: game, zoneUrl: zoneUrl, launchOption: launchOption) }
    }

    func refreshRemoteSessions() async {
        guard let session = authSession else { return }
        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            resumableSessions = try await api.fetchActiveSessions(
                session: refreshed,
                streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                vpcId: cachedVpcId
            )
        } catch {
            lastError = "Could not fetch active sessions: \(error.localizedDescription)"
        }
    }

    func endRemoteSession(candidate: RemoteSessionCandidate) async {
        guard let session = authSession else {
            lastError = "Sign in first."
            return
        }
        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            try await api.stopRemoteSession(
                session: refreshed,
                candidate: candidate,
                streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                vpcId: cachedVpcId
            )
            if activeSession?.id == candidate.id {
                await endSession()
            } else {
                resumableSessions.removeAll { $0.id == candidate.id }
                await refreshRemoteSessions()
            }
            lastError = nil
        } catch {
            lastError = "Could not end session: \(error.localizedDescription)"
        }
    }

    func resumeSession(candidate: RemoteSessionCandidate) async {
        guard supportsEmbeddedStreamer else {
            lastError = OpenNOWPlatform.streamingUnavailableReason
            return
        }
        guard let session = authSession else {
            lastError = "Sign in first."
            return
        }
        guard let game = resolveGameForRemoteSession(candidate) else {
            lastError = "Unable to match this remote session to a known game."
            return
        }
        isLaunchingSession = true
        showStreamLoading = true
        queueOverlayVisible = true
        defer { isLaunchingSession = false }
        do {
            logger.info("Resume requested candidateId=\(candidate.id, privacy: .public) status=\(candidate.status)")
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            let claimed = try await api.claimSession(
                session: refreshed,
                candidate: candidate,
                game: game,
                streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                vpcId: cachedVpcId,
                settings: settings
            )
            activeSession = claimed
            adReportStateById = [:]
            adStartedAtById = [:]
            sessionElapsedSeconds = 0
            startSessionTasks()
            syncTrackedSessionSurface()
            logger.info("Session resumed id=\(claimed.id, privacy: .public) status=\(claimed.status) queue=\(claimed.queuePosition ?? -1)")
            lastError = nil
        } catch is CancellationError {
            return
        } catch {
            logger.error("Resume session failed error=\(error.localizedDescription, privacy: .public)")
            showStreamLoading = false
            queueOverlayVisible = false
            syncTrackedSessionSurface()
            lastError = "Failed to resume session: \(error.localizedDescription)"
        }
    }

    func scheduleResume(candidate: RemoteSessionCandidate) {
        launchTask?.cancel()
        launchTask = Task { await self.resumeSession(candidate: candidate) }
    }

    func endSession() async {
        launchTask?.cancel()
        launchTask = nil
        showStreamLoading = false
        queueOverlayVisible = false
        await NotificationManager.shared.cancelSessionNotifications()
        guard let session = authSession, let active = activeSession else {
            activeSession = nil
            setStreamSession(nil, reason: "endSession.noActiveSession")
            telemetryTask?.cancel()
            sessionPollTask?.cancel()
            endSessionPollBackgroundTask()
            sessionElapsedSeconds = 0
            syncTrackedSessionSurface()
            return
        }
        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            try await api.stopSession(session: refreshed, activeSession: active)
        } catch {
            lastError = "Stop session failed: \(error.localizedDescription)"
        }
        activeSession = nil
        setStreamSession(nil, reason: "endSession.completed")
        adReportStateById = [:]
        adStartedAtById = [:]
        telemetryTask?.cancel()
        sessionPollTask?.cancel()
        endSessionPollBackgroundTask()
        sessionElapsedSeconds = 0
        syncTrackedSessionSurface()
    }

    func minimizeQueueOverlay() {
        withAnimation(.easeInOut(duration: 0.32)) {
            queueOverlayVisible = false
        }
    }

    func maximizeQueueOverlay() {
        guard activeSession != nil else { return }
        withAnimation(.easeInOut(duration: 0.32)) {
            showStreamLoading = true
            queueOverlayVisible = true
        }
    }

    var canReopenStreamer: Bool {
        guard supportsEmbeddedStreamer else { return false }
        guard let active = activeSession else { return false }
        return streamSession == nil && isReadyForStreamer(active)
    }

    var effectiveAdState: SessionAdState? {
        guard let active = activeSession else { return nil }
        if let adState = active.adState {
            return adState
        }
        guard isFreeTierUser else { return nil }
        guard active.status == 1, (active.queuePosition ?? 0) >= 1 else { return nil }
        return SessionAdState(
            isAdsRequired: true,
            sessionAdsRequired: true,
            isQueuePaused: nil,
            gracePeriodSeconds: nil,
            message: "Free-tier queue ads begin as soon as you enter queue.",
            sessionAds: [],
            ads: [],
            opportunity: SessionOpportunityInfo(
                state: nil,
                queuePaused: nil,
                gracePeriodSeconds: nil,
                message: "Free-tier queue ads begin as soon as you enter queue.",
                title: nil,
                description: nil
            ),
            serverSentEmptyAds: true
        )
    }

    var activeQueueAd: SessionAdInfo? {
        let ads = effectiveAdState?.sessionAds ?? effectiveAdState?.ads ?? []
        return ads.first
    }

    func reportQueueAdStarted(adId: String) {
        let lastAction = adReportStateById[adId]
        if lastAction == .start || lastAction == .resume || lastAction == .finish || lastAction == .cancel {
            return
        }
        adStartedAtById[adId] = adStartedAtById[adId] ?? Date()
        let action: SessionAdAction = (lastAction == .pause) ? .resume : .start
        adReportStateById[adId] = action
        Task { await reportQueueAdAction(adId: adId, action: action) }
    }

    func reportQueueAdPaused(adId: String) {
        let lastAction = adReportStateById[adId]
        guard lastAction == .start || lastAction == .resume else { return }
        adReportStateById[adId] = .pause
        Task { await reportQueueAdAction(adId: adId, action: .pause) }
    }

    func reportQueueAdFinished(adId: String, watchedTimeInMs: Int) {
        let lastAction = adReportStateById[adId]
        guard lastAction != .finish && lastAction != .cancel else { return }
        adReportStateById[adId] = .finish
        Task {
            await reportQueueAdAction(
                adId: adId,
                action: .finish,
                watchedTimeInMs: max(0, watchedTimeInMs)
            )
        }
    }

    func reportQueueAdError(adId: String, message: String) {
        let lastAction = adReportStateById[adId]
        guard lastAction != .finish && lastAction != .cancel else { return }
        adReportStateById[adId] = .cancel
        Task {
            await reportQueueAdAction(
                adId: adId,
                action: .cancel,
                cancelReason: "error",
                errorInfo: message
            )
        }
    }

    func dismissStreamer() {
        setStreamSession(nil, reason: "dismissStreamer")
        // Note: poll task stays cancelled (stopped in handoff).
        // QueueStatusPill reflects last known activeSession state.
        // User can reopen via pill tap → reopenStreamer().
    }

    func reopenStreamer() {
        guard supportsEmbeddedStreamer else {
            lastError = OpenNOWPlatform.streamingUnavailableReason
            return
        }
        guard let active = activeSession, isReadyForStreamer(active) else { return }
        launchTask?.cancel()
        launchTask = Task { await self.reopenCurrentSession(active) }
    }

    /// Schedule a streamer reopen with a 0.8s delay.
    /// Automatically aborts if a new session starts before the delay completes.
    func scheduleStreamerReopen() {
        guard supportsEmbeddedStreamer else { return }
        let token = UUID()
        reopenToken = token
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
            guard let self, self.reopenToken == token else { return }
            self.reopenStreamer()
        }
    }

    func handleScenePhase(_ phase: ScenePhase) {
        currentScenePhase = phase
        syncTrackedSessionSurface()
    }

    func persistSettings() {
        if let encoded = try? JSONEncoder().encode(settings) {
            defaults.set(encoded, forKey: settingsKey)
        }
    }

    func updateTouchControlLayout(_ layout: TouchControlLayout, profile: String) {
        settings.touchControlLayouts[profile] = layout
        persistSettings()
    }

    func updateStreamerPreferences(_ preferences: StreamerPreferences) {
        settings.streamerPreferences = preferences
        persistSettings()
    }

    private var effectiveProvider: LoginProvider? {
        if let provider = authSession?.provider {
            return provider
        }
        return providers.first(where: { $0.idpId == settings.selectedProviderIdpId }) ?? providers.first
    }

    var authProviderCode: String? {
        effectiveProvider?.code
    }

    var shouldUsePrintedWasteQueue: Bool {
        authProviderCode == "NVIDIA"
    }

    func formattedSessionElapsed() -> String {
        let hours = sessionElapsedSeconds / 3600
        let minutes = (sessionElapsedSeconds % 3600) / 60
        let seconds = sessionElapsedSeconds % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        }
        return String(format: "%02d:%02d", minutes, seconds)
    }

    var filteredCatalogGames: [CloudGame] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return allGames }
        return allGames.filter {
            $0.title.localizedCaseInsensitiveContains(query) ||
            $0.genre.localizedCaseInsensitiveContains(query) ||
            $0.platform.localizedCaseInsensitiveContains(query)
        }
    }

    private func startSessionTasks() {
        setStreamSession(nil, reason: "startSessionTasks.reset")
        reopenToken = UUID()
        telemetryTask?.cancel()
        sessionPollTask?.cancel()
        endSessionPollBackgroundTask()

        telemetryTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                if let active = self.activeSession {
                    let now = Date().timeIntervalSince1970
                    let ping = 10 + Int((sin(now) + 1) * 8)
                    let fps = max(30, self.settings.preferredFPS - Int((cos(now * 0.8) + 1) * 2))
                    let loss = max(0.0, min(1.0, (sin(now * 0.35) + 1) * 0.16))
                    let bitrate = max(8.0, min(75.0, 30.0 + sin(now * 0.4) * 12.0))
                    self.telemetry = SessionTelemetry(pingMs: ping, fps: fps, packetLossPercent: loss, bitrateMbps: bitrate)
                    self.sessionElapsedSeconds = Int(Date().timeIntervalSince(active.startedAt))
                }
                try? await Task.sleep(for: .seconds(1))
            }
        }

        sessionPollTask = Task { [weak self] in
            guard let self else { return }
            var previousStatus = self.activeSession?.status
            var consecutivePollFailures = 0
            var setupTimeoutStartedAt: Date?
            var setupTimeoutNotified = false
            var readyPollStreak = 0
            var readySince: Date?
            var loggedReadyForStreamer = false
            var dismissedOverlayAfterReady = false
            while !Task.isCancelled {
                guard let session = self.authSession, let active = self.activeSession else {
                    try? await Task.sleep(for: .seconds(2))
                    continue
                }
                self.refreshSessionPollBackgroundTask()
                do {
                    let refreshed = try await self.api.refreshSession(session)
                    self.authSession = refreshed
                    self.persistAuthSession(refreshed)
                    let polled = try await self.api.pollSession(session: refreshed, activeSession: active)
                    consecutivePollFailures = 0
                    self.activeSession = polled
                    self.syncTrackedSessionSurface()
                    // Keep the presented streamer session stable while polling continues.
                    // Replacing the fullScreenCover item every poll can trigger reconnect churn.
                    self.logger.info(
                        "Poll id=\(polled.id, privacy: .public) status=\(polled.status) queue=\(polled.queuePosition ?? -1) showOverlay=\(self.showStreamLoading) signalingServer=\(polled.signalingServer ?? "nil", privacy: .public) signalingUrl=\(polled.signalingUrl ?? "nil", privacy: .public) mediaIp=\(polled.mediaIp ?? "nil", privacy: .public) mediaPort=\(polled.mediaPort)"
                    )
                    let readyForStreamer = self.isReadyForStreamer(polled)
                    if readyForStreamer {
                        readyPollStreak += 1
                        if readySince == nil {
                            readySince = Date()
                        }
                    } else {
                        readyPollStreak = 0
                        readySince = nil
                    }

                    let requiredReadyPollStreak = (polled.status == 2) ? 3 : 2
                    // Status=2 sessions can still be warming transport; hold briefly
                    // before first handoff to reduce early connection churn.
                    let requiredReadyHoldSeconds: TimeInterval = (polled.status == 2) ? 5 : 3
                    let readyHoldElapsed = readySince.map { Date().timeIntervalSince($0) } ?? 0
                    if readyPollStreak >= requiredReadyPollStreak
                        && readyHoldElapsed >= requiredReadyHoldSeconds
                        && !loggedReadyForStreamer
                    {
                        if !self.supportsEmbeddedStreamer {
                            self.logger.notice(
                                "Session ready but embedded streamer unavailable on \(OpenNOWPlatform.displayName, privacy: .public)."
                            )
                            self.lastError = OpenNOWPlatform.streamingUnavailableReason
                            self.showStreamLoading = false
                            self.queueOverlayVisible = false
                            self.syncTrackedSessionSurface()
                            self.sessionPollTask?.cancel()
                            continue
                        }
                        let handoffSession = await self.prepareSessionForStreamer(polled)
                        self.logger.notice(
                            "Session ready for streamer handoff id=\(handoffSession.id, privacy: .public) status=\(handoffSession.status) readyStreak=\(readyPollStreak) readyHoldSeconds=\(Int(readyHoldElapsed)). Presenting iOS streamer."
                        )
                        if self.activeSession?.id == handoffSession.id {
                            self.activeSession = handoffSession
                        }
                        self.setStreamSession(handoffSession, reason: "sessionPollTask.handoffReady")
                        loggedReadyForStreamer = true
                        self.sessionPollTask?.cancel()
                    } else if !readyForStreamer {
                        loggedReadyForStreamer = false
                        dismissedOverlayAfterReady = false
                    }
                    if polled.status == 2 && previousStatus == 1 {
                        await NotificationManager.shared.sendQueueSetupNotification(gameTitle: polled.game.title)
                    }
                    if polled.status == 3 && previousStatus != 3 {
                        await NotificationManager.shared.sendQueueReadyNotification(gameTitle: polled.game.title)
                    }
                    previousStatus = polled.status
                    if self.isInSetupPhase(polled) {
                        if setupTimeoutStartedAt == nil {
                            setupTimeoutStartedAt = Date()
                        } else if let startedAt = setupTimeoutStartedAt,
                                  Date().timeIntervalSince(startedAt) >= self.setupPhaseTimeoutSeconds,
                                  !setupTimeoutNotified {
                            self.logger.error("Setup phase timeout exceeded for session id=\(polled.id, privacy: .public)")
                            self.lastError = "Session setup is taking longer than expected. Please retry."
                            setupTimeoutNotified = true
                        }
                    } else {
                        setupTimeoutStartedAt = nil
                        setupTimeoutNotified = false
                    }
                    if !readyForStreamer && !self.queueOverlayVisible {
                        // If the user minimized during queue/setup, keep the loading
                        // experience reopenable so they can return to it.
                        self.showStreamLoading = true
                    }
                    if loggedReadyForStreamer && self.queueOverlayVisible && !dismissedOverlayAfterReady {
                        // Close the full-screen queue overlay so streamer can present,
                        // but keep the compact top indicator alive.
                        self.queueOverlayVisible = false
                        self.showStreamLoading = true
                        dismissedOverlayAfterReady = true
                    }
                } catch {
                    consecutivePollFailures += 1
                    self.logger.error("Session poll failed attempt=\(consecutivePollFailures) error=\(error.localizedDescription, privacy: .public)")
                    self.lastError = "Session poll failed: \(error.localizedDescription)"
                }
                try? await Task.sleep(for: .seconds(2))
            }
            self.endSessionPollBackgroundTask()
        }
    }

    private func isInQueuePhase(_ session: ActiveSession) -> Bool {
        if (session.adState?.sessionAdsRequired ?? session.adState?.isAdsRequired ?? false), session.status == 1 {
            return true
        }
        guard session.status == 1 else { return false }
        if session.seatSetupStep == 1 {
            return true
        }
        return (session.queuePosition ?? 0) > 1
    }

    private func isInSetupPhase(_ session: ActiveSession) -> Bool {
        !isInQueuePhase(session) && session.status == 1
    }

    private func isReadyForStreamer(_ session: ActiveSession) -> Bool {
        // Match desktop behavior: allow connect on status 2 or 3.
        // Keep signaling non-empty checks below to avoid premature handoff.
        guard session.status == 2 || session.status == 3 else { return false }
        if (session.adState?.sessionAdsRequired ?? session.adState?.isAdsRequired ?? false) {
            return false
        }
        if let queuePosition = session.queuePosition, queuePosition > 1 {
            return false
        }
        return hasUsableSignalingEndpoint(session) && hasUsableMediaEndpoint(session)
    }

    private func hasUsableSignalingEndpoint(_ session: ActiveSession) -> Bool {
        let signalingUrl = session.signalingUrl?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !signalingUrl.isEmpty, let parsed = URL(string: signalingUrl), normalizedEndpointHost(from: signalingUrl) != nil {
            let scheme = (parsed.scheme ?? "").lowercased()
            if scheme == "wss" || scheme == "ws" || scheme == "rtsps" || scheme == "rtsp" || scheme == "https" {
                return true
            }
        }

        let signalingServer = session.signalingServer?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard !signalingServer.isEmpty else { return false }
        if let parsed = URL(string: "https://\(signalingServer)"),
           normalizedEndpointHost(from: parsed.host) != nil {
            return true
        }
        return normalizedEndpointHost(from: signalingServer) != nil
    }

    private func hasUsableMediaEndpoint(_ session: ActiveSession) -> Bool {
        guard session.mediaPort > 0 else { return false }
        if normalizedEndpointHost(from: session.mediaIp) != nil {
            return true
        }
        if normalizedEndpointHost(from: session.serverIp) != nil {
            return true
        }
        if normalizedEndpointHost(from: session.signalingServer) != nil {
            return true
        }
        return false
    }

    private func prepareSessionForStreamer(_ session: ActiveSession) async -> ActiveSession {
        guard session.status == 2 else { return session }
        // Refresh auth token so it's valid for the upcoming signaling connection.
        // Do NOT claim/migrate the session — the polled server has already handled
        // this session throughout queue/setup and is ready to serve WebRTC offers.
        // Migrating to a different server (via claimSession PUT) moves to a cold
        // server that hasn't set up its WebRTC endpoint and won't offer in time.
        // This matches desktop behavior: connect signaling directly to the polled server.
        guard let currentAuth = authSession else { return session }
        do {
            let refreshed = try await api.refreshSession(currentAuth)
            authSession = refreshed
            persistAuthSession(refreshed)
        } catch {
            logger.error(
                "Pre-handoff auth refresh failed id=\(session.id, privacy: .public) error=\(error.localizedDescription, privacy: .public)"
            )
        }
        logger.info(
            "Pre-handoff ready id=\(session.id, privacy: .public) signalingServer=\(session.signalingServer ?? "nil", privacy: .public) signalingUrl=\(session.signalingUrl ?? "nil", privacy: .public) mediaIp=\(session.mediaIp ?? "nil", privacy: .public)"
        )
        return session
    }

    private func reopenCurrentSession(_ session: ActiveSession) async {
        guard let currentAuth = authSession else {
            lastError = "Sign in first."
            return
        }

        do {
            let refreshed = try await api.refreshSession(currentAuth)
            authSession = refreshed
            persistAuthSession(refreshed)
            let polled = try await api.pollSession(session: refreshed, activeSession: session)
            let handoff = await prepareSessionForStreamer(polled)

            guard activeSession?.id == session.id else { return }
            activeSession = handoff
            setStreamSession(handoff, reason: "reopenStreamer.refreshed")
            lastError = nil
        } catch is CancellationError {
            return
        } catch {
            logger.error(
                "Reopen refresh failed id=\(session.id, privacy: .public) error=\(error.localizedDescription, privacy: .public)"
            )
            lastError = "Failed to reconnect session: \(error.localizedDescription)"
        }
    }

    private var isFreeTierUser: Bool {
        let tier = (subscription?.membershipTier ?? user?.membershipTier)?.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        return ((tier?.isEmpty) != nil) || tier == "FREE"
    }

    private func reportQueueAdAction(
        adId: String,
        action: SessionAdAction,
        watchedTimeInMs: Int? = nil,
        cancelReason: String? = nil,
        errorInfo: String? = nil
    ) async {
        guard let session = authSession, let active = activeSession else { return }
        let pausedTimeInMs: Int? = {
            guard let startedAt = adStartedAtById[adId], action == .finish || action == .cancel else {
                return nil
            }
            let elapsed = max(0, Int(Date().timeIntervalSince(startedAt) * 1000))
            let adDurationMs = (activeQueueAd?.adLengthInSeconds.map { Int(round($0 * 1000)) }) ?? activeQueueAd?.durationMs
            guard let adDurationMs, elapsed > adDurationMs else { return 0 }
            return elapsed - adDurationMs
        }()
        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            let updated = try await api.reportSessionAd(
                session: refreshed,
                activeSession: active,
                adId: adId,
                action: action,
                watchedTimeInMs: watchedTimeInMs,
                pausedTimeInMs: pausedTimeInMs,
                cancelReason: cancelReason,
                errorInfo: errorInfo
            )
            if activeSession?.id == updated.id {
                activeSession = updated
            }
            if action == .finish || action == .cancel {
                adStartedAtById.removeValue(forKey: adId)
            }
        } catch {
            logger.error(
                "Ad report failed action=\(action.rawValue, privacy: .public) adId=\(adId, privacy: .public) error=\(error.localizedDescription, privacy: .public)"
            )
        }
    }

    private func refreshSessionPollBackgroundTask() {
        #if os(tvOS)
        sessionPollBackgroundTaskActive = true
        #else
        endSessionPollBackgroundTask()
        sessionPollBackgroundTaskId = UIApplication.shared.beginBackgroundTask(withName: "OpenNOW.SessionPoll") { [weak self] in
            Task { @MainActor [weak self] in
                self?.endSessionPollBackgroundTask()
            }
        }
        #endif
    }

    private func endSessionPollBackgroundTask() {
        #if os(tvOS)
        sessionPollBackgroundTaskActive = false
        #else
        guard sessionPollBackgroundTaskId != .invalid else { return }
        UIApplication.shared.endBackgroundTask(sessionPollBackgroundTaskId)
        sessionPollBackgroundTaskId = .invalid
        #endif
    }

    private func restoreTrackedSessionIfNeeded() {
        guard let persisted = activeSession else {
            syncTrackedSessionSurface()
            return
        }
        guard resumableSessions.contains(where: { $0.id == persisted.id }) else {
            activeSession = nil
            showStreamLoading = false
            queueOverlayVisible = false
            syncTrackedSessionSurface()
            return
        }
        guard sessionPollTask == nil else {
            syncTrackedSessionSurface()
            return
        }
        showStreamLoading = true
        queueOverlayVisible = false
        startSessionTasks()
        syncTrackedSessionSurface()
    }

    private func syncTrackedSessionSurface() {
        persistActiveSession(activeSession)
        let active = activeSession
        let state = active.flatMap(queueActivityState(for:))
        Task {
            await QueueLiveActivityManager.shared.sync(
                sessionId: active?.id,
                gameTitle: active?.game.title,
                state: state
            )
        }
    }

    private func queueActivityState(for session: ActiveSession) -> QueueActivityAttributes.ContentState? {
        if streamSession != nil && currentScenePhase == .active {
            return nil
        }
        if isReadyForStreamer(session) || session.status == 3 {
            return QueueActivityAttributes.ContentState(
                phase: .ready,
                headline: "Session ready",
                detail: "Tap to jump back in",
                queueLabel: "Ready",
                queuePosition: session.queuePosition
            )
        }
        if isInQueuePhase(session) {
            let detail: String
            let queueLabel: String
            if let queue = session.queuePosition {
                detail = queue == 1 ? "Next in queue" : "Queue #\(queue)"
                queueLabel = queue == 1 ? "Soon" : "Q\(queue)"
            } else {
                detail = "Waiting in queue"
                queueLabel = "Queue"
            }
            return QueueActivityAttributes.ContentState(
                phase: .queued,
                headline: "In queue",
                detail: detail,
                queueLabel: queueLabel,
                queuePosition: session.queuePosition
            )
        }
        if isInSetupPhase(session) || session.status == 2 {
            return QueueActivityAttributes.ContentState(
                phase: .waiting,
                headline: "Allocating your rig",
                detail: "Setting up \(session.game.title)",
                queueLabel: "Wait",
                queuePosition: session.queuePosition
            )
        }
        return nil
    }

    private func resolveGameForRemoteSession(_ candidate: RemoteSessionCandidate) -> CloudGame? {
        if let appId = candidate.appId {
            if let fromAll = allGames.first(where: { $0.launchAppId == appId }) { return fromAll }
            if let fromLibrary = libraryGames.first(where: { $0.launchAppId == appId }) { return fromLibrary }
        }
        return featuredGames.first ?? allGames.first ?? libraryGames.first
    }

    /// Resolved catalog entry for a remote session (for Jump back in, Session list, etc.).
    func gameForRemoteSession(_ candidate: RemoteSessionCandidate) -> CloudGame? {
        resolveGameForRemoteSession(candidate)
    }

    private func streamSessionChangeCallsite() -> String {
        // Keep this compact for console readability while still surfacing
        // which flow is clearing/presenting the streamer item binding.
        Thread.callStackSymbols
            .dropFirst(2)
            .prefix(5)
            .map { $0.replacingOccurrences(of: "\t", with: " ") }
            .joined(separator: " <- ")
    }

    private func setStreamSession(_ session: ActiveSession?, reason: String) {
        let oldId = streamSession?.id ?? "nil"
        let newId = session?.id ?? "nil"
        guard oldId != newId else { return }
        logger.notice(
            "streamSession reason=\(reason, privacy: .public) \(oldId, privacy: .public) -> \(newId, privacy: .public) callsite=\(self.streamSessionChangeCallsite(), privacy: .public)"
        )
        streamSession = session
        syncTrackedSessionSurface()
    }

    #if os(tvOS)
    func clearTVAuthLogs() {
        tvAuthLogs.removeAll(keepingCapacity: true)
    }

    private func appendTVAuthLog(_ message: String) {
        tvAuthLogs.append(message)
        if tvAuthLogs.count > 60 {
            tvAuthLogs.removeFirst(tvAuthLogs.count - 60)
        }
    }
    #endif

    private static func loadSettings(from defaults: UserDefaults) -> AppSettings? {
        guard let data = defaults.data(forKey: "OpenNOW.iOS.settings") else { return nil }
        return try? JSONDecoder().decode(AppSettings.self, from: data)
    }

    private static func loadAuthSession(from defaults: UserDefaults) -> AuthSession? {
        guard let data = defaults.data(forKey: "OpenNOW.iOS.authSession") else { return nil }
        return try? JSONDecoder().decode(AuthSession.self, from: data)
    }

    private static func loadActiveSession(from defaults: UserDefaults) -> ActiveSession? {
        guard let data = defaults.data(forKey: "OpenNOW.iOS.activeSession") else { return nil }
        return try? JSONDecoder().decode(ActiveSession.self, from: data)
    }

    private func persistAuthSession(_ session: AuthSession) {
        if let encoded = try? JSONEncoder().encode(session) {
            defaults.set(encoded, forKey: authSessionKey)
        }
    }

    private func persistActiveSession(_ session: ActiveSession?) {
        guard let session else {
            defaults.removeObject(forKey: activeSessionSnapshotKey)
            return
        }
        if let encoded = try? JSONEncoder().encode(session) {
            defaults.set(encoded, forKey: activeSessionSnapshotKey)
        }
    }

    private func persistentDeviceId() -> String {
        if let existing = defaults.string(forKey: deviceIdKey), !existing.isEmpty {
            return existing
        }
        let generated = SHA256.hash(data: Data(UUID().uuidString.utf8)).compactMap { String(format: "%02x", $0) }.joined()
        defaults.set(generated, forKey: deviceIdKey)
        return generated
    }
}
