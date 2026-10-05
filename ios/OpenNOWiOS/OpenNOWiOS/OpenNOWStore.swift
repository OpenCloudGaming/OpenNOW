import AuthenticationServices
import CryptoKit
import Foundation
import ImageIO
import Network
import OSLog
import Security
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
    var authClientId: String? = nil
}

struct DeviceLoginPrompt: Equatable, Sendable {
    let userCode: String
    let verificationURL: URL
    let expiresAt: Date
}

struct DebugLifecycleEvent: Identifiable {
    let id = UUID()
    let timestamp: Date
    let category: String
    let message: String
}

struct AuthSession: Codable, Equatable {
    let provider: LoginProvider
    let tokens: AuthTokens
    let user: UserProfile
}

struct SavedAccount: Identifiable, Codable, Equatable {
    var id: String { userId }
    let userId: String
    let displayName: String
    let email: String?
    let membershipTier: String
    let providerCode: String

    init(
        userId: String,
        displayName: String,
        email: String?,
        membershipTier: String,
        providerCode: String
    ) {
        self.userId = userId
        self.displayName = displayName
        self.email = email
        self.membershipTier = membershipTier
        self.providerCode = providerCode
    }

    init(session: AuthSession) {
        userId = session.user.userId
        displayName = session.user.displayName
        email = session.user.email
        membershipTier = session.user.membershipTier
        providerCode = session.provider.code
    }
}

struct PersistedAuthState: Codable, Equatable {
    var sessions: [AuthSession] = []
    var activeUserId: String?
    var selectedProvider: LoginProvider?

    var activeSession: AuthSession? {
        if let activeUserId,
           let session = sessions.first(where: { $0.user.userId == activeUserId }) {
            return session
        }
        return sessions.first
    }

    var savedAccounts: [SavedAccount] {
        sessions.map(SavedAccount.init(session:))
    }
}

private struct CachedCatalogSnapshot: Codable {
    let schemaVersion: Int
    let cachedAt: TimeInterval
    let vpcId: String
    let allGames: [CloudGame]
    let featuredGames: [CloudGame]
    let libraryGames: [CloudGame]
}

struct CloudGame: Identifiable, Codable, Equatable {
    let id: String
    let title: String
    let genre: String
    let platform: String
    let icon: String
    let imageUrl: String?
    var boxArtUrl: String? = nil
    var heroImageUrl: String? = nil
    var tvBannerUrl: String? = nil
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
    let catalogSectionId: String?
    let catalogSectionTitle: String?
    let contentRatings: [String]?
    var screenshotUrls: [String]? = nil

    func fillingMissingMetadata(from fallback: CloudGame?) -> CloudGame {
        guard let fallback else { return self }
        let mergedScreenshots = Self.mergingArtworkURLs(screenshotUrls, fallback.screenshotUrls)
        return CloudGame(
            id: id,
            title: title,
            genre: genre,
            platform: platform,
            icon: icon,
            imageUrl: imageUrl ?? fallback.imageUrl,
            boxArtUrl: boxArtUrl ?? fallback.boxArtUrl,
            heroImageUrl: heroImageUrl ?? fallback.heroImageUrl,
            tvBannerUrl: tvBannerUrl ?? fallback.tvBannerUrl,
            launchAppId: launchAppId,
            launchOptions: launchOptions,
            uuid: uuid,
            summary: summary ?? fallback.summary,
            longDescription: longDescription ?? fallback.longDescription,
            publisher: publisher ?? fallback.publisher,
            developer: developer ?? fallback.developer,
            releaseDate: releaseDate ?? fallback.releaseDate,
            featureLabels: featureLabels ?? fallback.featureLabels,
            tags: tags ?? fallback.tags,
            stores: stores,
            playType: playType,
            membershipTierLabel: membershipTierLabel,
            catalogSectionId: catalogSectionId,
            catalogSectionTitle: catalogSectionTitle,
            contentRatings: GFNContentRatingParser.merging(contentRatings, fallback.contentRatings),
            screenshotUrls: mergedScreenshots
        )
    }

    var catalogArtworkUrl: String? {
        for candidate in [boxArtUrl, imageUrl] {
            guard let candidate = candidate?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !candidate.isEmpty else {
                continue
            }
            if candidate.localizedCaseInsensitiveContains("img.nvidiagrid.net"),
               !candidate.localizedCaseInsensitiveContains("GAME_BOX_ART") {
                continue
            }
            return candidate
        }
        return nil
    }

    var detailsArtworkUrl: String? {
        screenshotUrls?.first ?? heroImageUrl ?? tvBannerUrl ?? imageUrl ?? boxArtUrl
    }

    var queueArtworkUrl: String? {
        tvBannerUrl ?? heroImageUrl ?? imageUrl ?? boxArtUrl
    }

    private static func mergingArtworkURLs(_ primary: [String]?, _ fallback: [String]?) -> [String]? {
        var seen = Set<String>()
        let merged = ((primary ?? []) + (fallback ?? [])).compactMap { raw -> String? in
            let value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !value.isEmpty, seen.insert(value).inserted else { return nil }
            return value
        }
        return merged.isEmpty ? nil : merged
    }
}

func gameMatchesCatalogSearch(_ game: CloudGame, query: String) -> Bool {
    let terms = query
        .split(whereSeparator: { $0.isWhitespace })
        .map(String.init)
    guard !terms.isEmpty else { return true }
    let searchableText = ([
        game.title,
        game.genre,
        game.platform,
        game.summary,
        game.longDescription,
        game.publisher,
        game.developer
    ].compactMap { $0 }
        + (game.featureLabels ?? [])
        + (game.tags ?? [])
        + (game.stores ?? [])
        + game.launchOptions.map(\.storefront))
        .joined(separator: " ")
    return terms.allSatisfy { searchableText.localizedCaseInsensitiveContains($0) }
}

enum GFNCatalogLabelParser {
    static func labels(from value: Any?) -> [String]? {
        let rawLabels: [String]
        if let list = value as? [String] {
            rawLabels = list
        } else if let list = value as? [[String: Any]] {
            rawLabels = list.compactMap { item in
                ["name", "label", "title", "displayName"]
                    .compactMap { normalizedString(item[$0]) }
                    .first
            }
        } else {
            return nil
        }

        var seen = Set<String>()
        let cleaned = rawLabels.compactMap(normalizedString).filter { seen.insert($0).inserted }
        return cleaned.isEmpty ? nil : cleaned
    }

    private static func normalizedString(_ value: Any?) -> String? {
        guard let value = value as? String else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}

enum GFNContentRatingParser {
    static func merging(_ primary: [String]?, _ fallback: [String]?) -> [String]? {
        var seen = Set<String>()
        let merged = (primary ?? []) + (fallback ?? [])
        let unique = merged.filter { seen.insert($0).inserted }
        return unique.isEmpty ? nil : unique
    }

    static func ageBadge(from labels: [String]?) -> String? {
        let ages = (labels ?? []).compactMap(ageValue(from:))
        guard let highest = ages.max() else { return nil }
        return "\(highest)+"
    }

    private static func ageValue(from label: String) -> Int? {
        let normalized = label
            .uppercased()
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return nil }

        if normalized.contains("ESRB") || normalized.contains("EVERYONE") ||
            normalized.contains("TEEN") || normalized.contains("MATURE") {
            if normalized.contains("MATURE") || normalized.contains("AO") { return 18 }
            if normalized.contains("TEEN") || normalized.range(of: #"\bT\b"#, options: .regularExpression) != nil { return 12 }
            if normalized.contains("E10") || normalized.contains("EVERYONE 10") { return 10 }
            if normalized.range(of: #"\bE\b"#, options: .regularExpression) != nil || normalized.contains("EVERYONE") { return 4 }
        }

        guard let range = normalized.range(of: #"\b([0-9]{1,2})\+?\b"#, options: .regularExpression) else {
            return nil
        }
        return Int(normalized[range].trimmingCharacters(in: CharacterSet(charactersIn: "+")))
    }

    static func labels(from value: Any?) -> [String]? {
        let entries: [Any]
        if let values = value as? [Any] {
            entries = values
        } else if let value {
            entries = [value]
        } else {
            return nil
        }

        var labels: [String] = []
        for entry in entries {
            if let text = normalizedString(entry) {
                labels.append(text)
                continue
            }
            guard let object = entry as? [String: Any] else { continue }

            let type = normalizedString(object["type"])
            let category = normalizedString(object["categoryKey"])
            let currentRating = [type, category].compactMap { $0 }.joined(separator: " ")
            if !currentRating.isEmpty {
                labels.append(currentRating)
            } else if let legacyLabel = ["name", "label", "title", "displayName"]
                .compactMap({ normalizedString(object[$0]) })
                .first {
                labels.append(legacyLabel)
            }

            for key in ["contentDescriptorKeys", "interactiveElementKeys"] {
                labels.append(contentsOf: (object[key] as? [String] ?? []).compactMap(humanizedKey))
            }
        }

        var seen = Set<String>()
        let unique = labels.filter { seen.insert($0).inserted }
        return unique.isEmpty ? nil : unique
    }

    private static func normalizedString(_ value: Any?) -> String? {
        guard let value = value as? String else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func humanizedKey(_ value: String) -> String? {
        normalizedString(value)?
            .lowercased()
            .split(separator: "_")
            .map { $0.prefix(1).uppercased() + $0.dropFirst() }
            .joined(separator: " ")
    }
}

struct StreamRegion: Identifiable, Codable, Equatable {
    var id: String { url }
    let name: String
    let url: String
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
    var sessionControlBaseUrl: String? = nil
    let clientId: String
    let deviceId: String
    var adState: SessionAdState?
    var gpuType: String? = nil
    var negotiatedStreamProfile: NegotiatedStreamProfile? = nil
    var requestedStreamingFeatures: StreamingFeatures? = nil
    var finalizedStreamingFeatures: StreamingFeatures? = nil
    /// Whether this session was requested with `appLaunchMode: touchFriendly`.
    /// This records the request, not host acknowledgment of a working digitizer.
    ///
    /// The host provisions its input devices when the session is created and never revisits them,
    /// so this cannot be recomputed later from the current settings — flipping the touch setting
    /// mid-session would otherwise produce a stream that sends well-formed touch packets into a
    /// session with nowhere to put them, with nothing on screen to explain the silence.
    ///
    /// `nil` means "created before this was recorded, or by something other than a launch or claim
    /// we performed"; those sessions keep the old behaviour of trusting the setting.
    var touchProvisioned: Bool? = nil
    var nativeRtspsEndpoints: [String]? = nil
}

struct RemoteSessionCandidate: Identifiable, Codable, Equatable {
    let id: String
    let appId: String?
    let status: Int
    let serverIp: String?
    let streamSettingsSignature: String?
    let resolution: String?
    let fps: Int?
    var colorQuality: StreamColorQuality? = nil
    var hdrEnabled: Bool? = nil
}

struct StorageAddon: Codable, Equatable {
    let type: String
    let sizeGb: Double?
    let usedGb: Double?
    let regionName: String?
    let regionCode: String?
    let status: String?
    let subType: String?
    let autoPayEnabled: Bool?

    var usageFraction: Double? {
        guard let sizeGb, let usedGb, sizeGb > 0 else { return nil }
        return min(max(usedGb / sizeGb, 0), 1)
    }
}

struct SubscriptionSnapshot: Codable, Equatable {
    let membershipTier: String
    let subscriptionType: String?
    let subscriptionSubType: String?
    let isGamePlayAllowed: Bool
    let isUnlimited: Bool
    let remainingHours: Double
    let totalHours: Double
    let storageAddon: StorageAddon?
}

struct AccountConnector: Identifiable, Codable, Equatable {
    var id: String { store }
    let store: String
    let label: String
    let supported: Bool
    let required: Bool
    let userDisplayName: String?
    let userIdentifier: String?
    let expiresInSeconds: Int?
    let syncedGameCount: Int?
    let syncState: String?
    let syncDate: String?

    var isLinked: Bool {
        userDisplayName?.isEmpty == false ||
            userIdentifier?.isEmpty == false ||
            expiresInSeconds != nil ||
            syncedGameCount != nil ||
            syncState?.isEmpty == false ||
            syncDate?.isEmpty == false
    }
}

enum StreamColorQuality: String, Codable, CaseIterable, Identifiable {
    case eightBit420 = "8bit_420"
    case eightBit444 = "8bit_444"
    case tenBit420 = "10bit_420"
    case tenBit444 = "10bit_444"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .eightBit420: return "8-bit 4:2:0"
        case .eightBit444: return "8-bit 4:4:4"
        case .tenBit420: return "10-bit 4:2:0"
        case .tenBit444: return "10-bit 4:4:4"
        }
    }

    var bitDepth: Int {
        switch self {
        case .tenBit420, .tenBit444: return 10
        case .eightBit420, .eightBit444: return 0
        }
    }

    var chromaFormat: Int {
        switch self {
        case .eightBit444, .tenBit444: return 2
        case .eightBit420, .tenBit420: return 0
        }
    }
}

enum StreamPreset: String, Codable, CaseIterable, Identifiable {
    case recommended
    case lowDataSaver = "low_data_saver"
    case medium
    case high
    case custom

    var id: String { rawValue }

    var label: String {
        switch self {
        case .recommended: return "Recommended"
        case .lowDataSaver: return "Low Data Saver"
        case .medium: return "Medium"
        case .high: return "High"
        case .custom: return "Custom"
        }
    }

    var detail: String {
        switch self {
        case .recommended: return "Tuned to this device and connection"
        case .lowDataSaver: return "720p · 30 fps · 12 Mb/s"
        case .medium: return "1080p · 60 fps · 35 Mb/s"
        case .high: return "Highest your plan allows"
        case .custom: return "Your own settings below"
        }
    }
}

enum AppLaunchPage: String, Codable, CaseIterable, Identifiable {
    case store
    case library

    var id: String { rawValue }
    var label: String { self == .store ? "Home" : "Library" }
}

enum StreamStatsStyle: String, Codable, CaseIterable, Identifiable {
    case compact
    case detailed

    var id: String { rawValue }
    var label: String { self == .compact ? "Compact" : "Detailed" }
}

enum StreamStatsPosition: String, Codable, CaseIterable, Identifiable {
    case left
    case center
    case right

    var id: String { rawValue }

    var label: String {
        switch self {
        case .left: return "Left"
        case .center: return "Center"
        case .right: return "Right"
        }
    }

    var alignment: Alignment {
        switch self {
        case .left: return .topLeading
        case .center: return .top
        case .right: return .topTrailing
        }
    }
}

struct StreamingFeatures: Codable, Equatable {
    var reflex: Bool?
    var bitDepth: Int?
    var cloudGsync: Bool?
    var chromaFormat: Int?
    var enabledL4S: Bool?
    var trueHdr: Bool?
}

struct NegotiatedStreamProfile: Codable, Equatable {
    var resolution: String?
    var fps: Int?
    var codec: String?
    var colorQuality: StreamColorQuality?
    var enableL4S: Bool?
    var enableCloudGsync: Bool?
    var enableReflex: Bool?
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

func isSessionAdsRequired(_ adState: SessionAdState?) -> Bool {
    adState?.sessionAdsRequired ?? (adState?.isAdsRequired == true)
}

func sessionAdItems(_ adState: SessionAdState?) -> [SessionAdInfo] {
    if let sessionAds = adState?.sessionAds, !sessionAds.isEmpty {
        return sessionAds
    }
    return adState?.ads ?? []
}

func shouldWaitForQueueAdPlayback(_ adState: SessionAdState?) -> Bool {
    isSessionAdsRequired(adState) && !sessionAdItems(adState).isEmpty
}

func mergeQueueAdState(
    previous: SessionAdState?,
    next: SessionAdState?,
    preserveMissingAdState: Bool = true
) -> SessionAdState? {
    guard let next else {
        return preserveMissingAdState ? previous : nil
    }

    let shouldRestorePreviousAds =
        preserveMissingAdState &&
        isSessionAdsRequired(next) &&
        (next.serverSentEmptyAds ?? false) &&
        sessionAdItems(next).isEmpty &&
        !sessionAdItems(previous).isEmpty

    guard shouldRestorePreviousAds else {
        return next
    }

    let restored = sessionAdItems(previous)
    return SessionAdState(
        isAdsRequired: next.isAdsRequired,
        sessionAdsRequired: next.sessionAdsRequired,
        isQueuePaused: next.isQueuePaused,
        gracePeriodSeconds: next.gracePeriodSeconds,
        message: next.message,
        sessionAds: restored,
        ads: previous?.ads.isEmpty == false ? (previous?.ads ?? restored) : restored,
        opportunity: next.opportunity,
        serverSentEmptyAds: next.serverSentEmptyAds
    )
}

func mergeQueueSessionState(
    previous: ActiveSession,
    next: ActiveSession,
    preserveMissingAdState: Bool = true
) -> ActiveSession {
    guard next.status != 2 && next.status != 3 else {
        return next
    }
    var merged = next
    merged.adState = mergeQueueAdState(
        previous: previous.adState,
        next: next.adState,
        preserveMissingAdState: preserveMissingAdState
    )
    if merged.mediaIp == nil {
        merged.mediaIp = previous.mediaIp
    }
    if merged.mediaPort <= 0 {
        merged.mediaPort = previous.mediaPort
    }
    return merged
}

func removeSessionAdItem(_ adState: SessionAdState?, adId: String) -> SessionAdState? {
    guard let adState else { return nil }
    return SessionAdState(
        isAdsRequired: adState.isAdsRequired,
        sessionAdsRequired: adState.sessionAdsRequired,
        isQueuePaused: adState.isQueuePaused,
        gracePeriodSeconds: adState.gracePeriodSeconds,
        message: adState.message,
        sessionAds: adState.sessionAds.filter { $0.adId != adId },
        ads: adState.ads.filter { $0.adId != adId },
        opportunity: adState.opportunity,
        serverSentEmptyAds: false
    )
}

/// Somewhere in Settings that another screen can ask to open.
enum SettingsRouteTarget: String, Equatable {
    case account
    case general
    case stream
    case input
    case interface
}

/// A launch the user asked for, held so it can be replayed after a confirmation.
struct PendingLaunchRequest: Equatable {
    let game: CloudGame
    let zoneUrl: String?
    let launchOption: GameLaunchOption?
}

/// A launch that would end a session already running on another game.
struct LaunchConflict: Identifiable, Equatable {
    let runningGame: CloudGame
    let request: PendingLaunchRequest

    var id: String { "\(runningGame.id)->\(request.game.id)" }

    var title: String { "End your \(runningGame.title) session?" }

    var message: String {
        "\(request.game.title) needs the rig that \(runningGame.title) is using. "
            + "Any unsaved progress in \(runningGame.title) will be lost."
    }

    /// Pure so the rule can be tested without standing up a store.
    ///
    /// Statuses 1–3 are queued, setting up and ready — all of them hold a rig. Anything else has
    /// already released it, and re-confirming would be a dialog with nothing behind it. Relaunching
    /// the *same* game is not a conflict: the launch path claims the existing session instead.
    static func between(active: ActiveSession?, request: PendingLaunchRequest) -> LaunchConflict? {
        guard let active, (1...3).contains(active.status) else { return nil }
        guard active.game.id != request.game.id else { return nil }
        return LaunchConflict(runningGame: active.game, request: request)
    }
}

private let appSettingsSessionReportDefaultVersion = 1

struct AppSettings: Codable, Equatable {
    var preferredRegion: String
    var preferredAspectRatio: String = "16:9"
    var preferredResolution: String
    var streamPreset: StreamPreset = .custom
    var preferredFPS: Int
    var preferredQuality: String
    var preferredCodec: String
    var nativeStreamerEnabled: Bool = false
    var experimentalNativeNVSTEnabled: Bool = false
    var experimentalDesktop444TouchEnabled: Bool = false
    var preferredColorQuality: String = StreamColorQuality.eightBit420.rawValue
    var hdrEnabled: Bool = false
    var maxBitrateMbps: Int
    var keyboardLayout: String
    var gameLanguage: String
    var sessionProxyEnabled: Bool = false
    var sessionProxyUrl: String = ""
    var enableL4S: Bool
    var enableCloudGsync: Bool
    var metal4Enabled: Bool = false
    var metalFXUpscalingEnabled: Bool = false
    var metalFXQualityPreset: MetalFXQualityPreset = .manual
    var streamSharpeningEnabled: Bool = false
    var streamSharpeningAmount: Double = 0.25
    var mouseSensitivity: Double = 1
    var mouseAcceleration: Int = 1
    var fingerMouseEnabled: Bool = true
    var phoneRumbleFallback: Bool = true
    var launchPage: AppLaunchPage = .store
    var posterSizeScale: Double = 1
    var compactGameCards: Bool = true
    var showGameStoreLabels: Bool = true
    var catalogWallpaperEnabled: Bool = false
    var catalogWallpaperFilename: String? = nil
    var streamTutorialCompleted: Bool = false
    var controllerTouchPromptDismissed: Bool = false
    var sessionCounterEnabled: Bool = true
    var ageRequirementAccepted: Bool = false
    var keepMicEnabled: Bool
    var showStatsOverlay: Bool
    var hideServerSelector: Bool
    var nerdMode: Bool = false
    var queueLiveActivitiesEnabled: Bool
    var selectedProviderIdpId: String
    var fortnitePrefersNativeTouch: Bool
    var touchControlLayouts: [String: TouchControlLayout]
    var streamerPreferences: StreamerPreferences
    var favoriteGameIds: [String]
    var defaultGameVariantIds: [String: String] = [:]

    // MARK: Parity fields
    //
    // Everything below closes a gap against `AppSettings` in the Android build
    // (`android/app/src/main/java/com/opencloudgaming/opennow/Models.kt`). Each one decodes with a
    // default so existing installs migrate without touching stored JSON.

    // Appearance
    var uiAccent: UIAccent = .openNow
    var expressiveUI: Bool = true
    var liveSelectedOutlines: Bool = true

    // Catalog presentation
    var showCardTitles: Bool = true
    /// True keeps the heart on every card (the behaviour iOS shipped with). False declutters
    /// the grid: the heart then appears only on games already favourited or on the focused card,
    /// and the long-press menu still reaches the toggle either way.
    var showFavoriteIconOnGameCards: Bool = true
    var catalogWallpaperPreset: CatalogWallpaperPreset = .colorfulAbstract

    // Stream HUD
    /// Apple's own Metal performance HUD, drawn by the OS over the video layer. Off by default:
    /// it is a developer tool, it overlaps the game, and OpenNOW's own stats overlay already
    /// reports the numbers a player cares about. Kept as a setting because it reports things
    /// ours cannot — GPU time and the actual present rate — which is worth having when
    /// diagnosing a stutter.
    var streamStatsMetrics: StreamStatsMetrics = .default
    var hideStreamButtons: Bool = false
    var streamKeyboardButtonPosition: NormalizedPoint = .trailingCenter
    var streamKeyboardClearConfirmationDisabled: Bool = false
    var showAntiAfkIndicator: Bool = false

    // Sessions
    var showSessionReportAfterStream: Bool = false
    var sessionReportDefaultVersion: Int = 0
    var sessionClockShowEveryMinutes: Int = 60
    var sessionClockShowDurationSeconds: Int = 30

    // Input
    var microphoneMode: MicrophoneMode = .disabled
    var mouseScrollSensitivity: Int = 30
    var controllerMouseEmulation: Bool = false
    var clipboardPasteEnabled: Bool = true
    var touch: TouchSettings = .default

    // Sound. Controller UI tones default off on iOS: the system already provides keyboard and
    // selection feedback, and a second sound layer reads as a bug rather than a feature.
    var controllerUISounds: Bool = false
    var streamIntroSound: Bool = false
    var queueReadySound: Bool = false

    // Privacy
    var analyticsOptOut: Bool = true
    var analyticsConsentAsked: Bool = false

    // Localization. Empty string means "follow the system".
    var appLanguage: String = AppLanguage.systemDefault

    var analyticsConsent: AnalyticsConsent {
        AnalyticsConsent(asked: analyticsConsentAsked, optOut: analyticsOptOut)
    }

    enum CodingKeys: String, CodingKey {
        case preferredRegion
        case preferredAspectRatio
        case preferredResolution
        case streamPreset
        case preferredFPS
        case preferredQuality
        case preferredCodec
        case nativeStreamerEnabled
        case experimentalNativeNVSTEnabled
        case experimentalDesktop444TouchEnabled
        case preferredColorQuality
        case hdrEnabled
        case maxBitrateMbps
        case keyboardLayout
        case gameLanguage
        case sessionProxyEnabled
        case sessionProxyUrl
        case enableL4S
        case enableCloudGsync
        case metal4Enabled
        case metalFXUpscalingEnabled
        case metalFXQualityPreset
        case streamSharpeningEnabled
        case streamSharpeningAmount
        case mouseSensitivity
        case mouseAcceleration
        case fingerMouseEnabled
        case phoneRumbleFallback
        case launchPage
        case posterSizeScale
        case compactGameCards
        case showGameStoreLabels
        case catalogWallpaperEnabled
        case catalogWallpaperFilename
        case streamTutorialCompleted
        case controllerTouchPromptDismissed
        case sessionCounterEnabled
        case ageRequirementAccepted
        case keepMicEnabled
        case showStatsOverlay
        case hideServerSelector
        case nerdMode
        case queueLiveActivitiesEnabled
        case selectedProviderIdpId
        case fortnitePrefersNativeTouch
        case touchControlLayouts
        case streamerPreferences
        case favoriteGameIds
        case defaultGameVariantIds
        case uiAccent
        case expressiveUI
        case liveSelectedOutlines
        case showCardTitles
        case showFavoriteIconOnGameCards
        case catalogWallpaperPreset
        case streamStatsMetrics
        case hideStreamButtons
        case streamKeyboardButtonPosition
        case streamKeyboardClearConfirmationDisabled
        case showAntiAfkIndicator
        case showSessionReportAfterStream
        case sessionReportDefaultVersion
        case sessionClockShowEveryMinutes
        case sessionClockShowDurationSeconds
        case microphoneMode
        case mouseScrollSensitivity
        case controllerMouseEmulation
        case clipboardPasteEnabled
        case touch
        case controllerUISounds
        case streamIntroSound
        case queueReadySound
        case analyticsOptOut
        case analyticsConsentAsked
        case appLanguage
    }

    init(
        preferredRegion: String,
        preferredResolution: String,
        preferredFPS: Int,
        preferredQuality: String,
        preferredCodec: String,
        nativeStreamerEnabled: Bool = false,
        maxBitrateMbps: Int,
        keyboardLayout: String,
        gameLanguage: String,
        enableL4S: Bool,
        enableCloudGsync: Bool,
        keepMicEnabled: Bool,
        showStatsOverlay: Bool,
        hideServerSelector: Bool,
        queueLiveActivitiesEnabled: Bool,
        selectedProviderIdpId: String,
        fortnitePrefersNativeTouch: Bool,
        touchControlLayouts: [String: TouchControlLayout],
        streamerPreferences: StreamerPreferences,
        favoriteGameIds: [String]
    ) {
        self.preferredRegion = preferredRegion
        self.preferredResolution = preferredResolution
        self.preferredFPS = preferredFPS
        self.preferredQuality = preferredQuality
        self.preferredCodec = preferredCodec
        self.nativeStreamerEnabled = nativeStreamerEnabled
        self.maxBitrateMbps = maxBitrateMbps
        self.keyboardLayout = keyboardLayout
        self.gameLanguage = gameLanguage
        self.enableL4S = enableL4S
        self.enableCloudGsync = enableCloudGsync
        self.keepMicEnabled = keepMicEnabled
        self.showStatsOverlay = showStatsOverlay
        self.hideServerSelector = hideServerSelector
        self.nerdMode = false
        self.queueLiveActivitiesEnabled = queueLiveActivitiesEnabled
        self.selectedProviderIdpId = selectedProviderIdpId
        self.fortnitePrefersNativeTouch = fortnitePrefersNativeTouch
        self.touchControlLayouts = touchControlLayouts
        self.streamerPreferences = streamerPreferences
        self.favoriteGameIds = favoriteGameIds
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        preferredRegion = try container.decodeIfPresent(String.self, forKey: .preferredRegion) ?? "Auto"
        preferredAspectRatio = try container.decodeIfPresent(String.self, forKey: .preferredAspectRatio) ?? "16:9"
        preferredResolution = try container.decodeIfPresent(String.self, forKey: .preferredResolution) ?? "Auto"
        streamPreset = try container.decodeIfPresent(StreamPreset.self, forKey: .streamPreset) ?? .custom
        preferredFPS = try container.decodeIfPresent(Int.self, forKey: .preferredFPS) ?? 60
        preferredQuality = try container.decodeIfPresent(String.self, forKey: .preferredQuality) ?? "Balanced"
        preferredCodec = try container.decodeIfPresent(String.self, forKey: .preferredCodec) ?? "Auto"
        nativeStreamerEnabled = try container.decodeIfPresent(Bool.self, forKey: .nativeStreamerEnabled) ?? false
        experimentalNativeNVSTEnabled = try container.decodeIfPresent(Bool.self, forKey: .experimentalNativeNVSTEnabled) ?? false
        experimentalDesktop444TouchEnabled = try container.decodeIfPresent(Bool.self, forKey: .experimentalDesktop444TouchEnabled) ?? false
        preferredColorQuality = try container.decodeIfPresent(String.self, forKey: .preferredColorQuality) ?? StreamColorQuality.eightBit420.rawValue
        hdrEnabled = try container.decodeIfPresent(Bool.self, forKey: .hdrEnabled) ?? false
        maxBitrateMbps = try container.decodeIfPresent(Int.self, forKey: .maxBitrateMbps) ?? 0
        keyboardLayout = try container.decodeIfPresent(String.self, forKey: .keyboardLayout) ?? "en-US"
        gameLanguage = try container.decodeIfPresent(String.self, forKey: .gameLanguage) ?? "en_US"
        sessionProxyEnabled = try container.decodeIfPresent(Bool.self, forKey: .sessionProxyEnabled) ?? false
        sessionProxyUrl = try container.decodeIfPresent(String.self, forKey: .sessionProxyUrl) ?? ""
        enableL4S = try container.decodeIfPresent(Bool.self, forKey: .enableL4S) ?? false
        enableCloudGsync = try container.decodeIfPresent(Bool.self, forKey: .enableCloudGsync) ?? false
        metal4Enabled = try container.decodeIfPresent(Bool.self, forKey: .metal4Enabled) ?? false
        metalFXUpscalingEnabled = try container.decodeIfPresent(Bool.self, forKey: .metalFXUpscalingEnabled) ?? false
        metalFXQualityPreset = try container.decodeIfPresent(MetalFXQualityPreset.self, forKey: .metalFXQualityPreset) ?? .manual
        streamSharpeningEnabled = try container.decodeIfPresent(Bool.self, forKey: .streamSharpeningEnabled) ?? false
        streamSharpeningAmount = try container.decodeIfPresent(Double.self, forKey: .streamSharpeningAmount) ?? 0.25
        mouseSensitivity = try container.decodeIfPresent(Double.self, forKey: .mouseSensitivity) ?? 1
        mouseAcceleration = try container.decodeIfPresent(Int.self, forKey: .mouseAcceleration) ?? 1
        fingerMouseEnabled = try container.decodeIfPresent(Bool.self, forKey: .fingerMouseEnabled) ?? true
        phoneRumbleFallback = try container.decodeIfPresent(Bool.self, forKey: .phoneRumbleFallback) ?? true
        launchPage = try container.decodeIfPresent(AppLaunchPage.self, forKey: .launchPage) ?? .store
        posterSizeScale = try container.decodeIfPresent(Double.self, forKey: .posterSizeScale) ?? 1
        compactGameCards = try container.decodeIfPresent(Bool.self, forKey: .compactGameCards) ?? true
        showGameStoreLabels = try container.decodeIfPresent(Bool.self, forKey: .showGameStoreLabels) ?? true
        catalogWallpaperEnabled = try container.decodeIfPresent(Bool.self, forKey: .catalogWallpaperEnabled) ?? false
        catalogWallpaperFilename = try container.decodeIfPresent(String.self, forKey: .catalogWallpaperFilename)
        streamTutorialCompleted = try container.decodeIfPresent(Bool.self, forKey: .streamTutorialCompleted) ?? false
        controllerTouchPromptDismissed = try container.decodeIfPresent(Bool.self, forKey: .controllerTouchPromptDismissed) ?? false
        sessionCounterEnabled = try container.decodeIfPresent(Bool.self, forKey: .sessionCounterEnabled) ?? true
        ageRequirementAccepted = try container.decodeIfPresent(Bool.self, forKey: .ageRequirementAccepted) ?? false
        keepMicEnabled = try container.decodeIfPresent(Bool.self, forKey: .keepMicEnabled) ?? false
        showStatsOverlay = try container.decodeIfPresent(Bool.self, forKey: .showStatsOverlay) ?? true
        hideServerSelector = try container.decodeIfPresent(Bool.self, forKey: .hideServerSelector) ?? false
        nerdMode = try container.decodeIfPresent(Bool.self, forKey: .nerdMode) ?? false
        queueLiveActivitiesEnabled = try container.decodeIfPresent(Bool.self, forKey: .queueLiveActivitiesEnabled) ?? true
        selectedProviderIdpId = try container.decodeIfPresent(String.self, forKey: .selectedProviderIdpId)
            ?? "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg"
        fortnitePrefersNativeTouch = try container.decodeIfPresent(Bool.self, forKey: .fortnitePrefersNativeTouch) ?? true
        touchControlLayouts = try container.decodeIfPresent([String: TouchControlLayout].self, forKey: .touchControlLayouts)
            ?? TouchControlLayout.defaultProfiles
        streamerPreferences = try container.decodeIfPresent(StreamerPreferences.self, forKey: .streamerPreferences)
            ?? .default
        favoriteGameIds = try container.decodeIfPresent([String].self, forKey: .favoriteGameIds) ?? []
        defaultGameVariantIds = try container.decodeIfPresent([String: String].self, forKey: .defaultGameVariantIds) ?? [:]

        uiAccent = try container.decodeIfPresent(UIAccent.self, forKey: .uiAccent) ?? .openNow
        expressiveUI = try container.decodeIfPresent(Bool.self, forKey: .expressiveUI) ?? true
        liveSelectedOutlines = try container.decodeIfPresent(Bool.self, forKey: .liveSelectedOutlines) ?? true
        showCardTitles = try container.decodeIfPresent(Bool.self, forKey: .showCardTitles) ?? true
        showFavoriteIconOnGameCards = try container.decodeIfPresent(Bool.self, forKey: .showFavoriteIconOnGameCards) ?? true
        catalogWallpaperPreset = try container.decodeIfPresent(CatalogWallpaperPreset.self, forKey: .catalogWallpaperPreset) ?? .colorfulAbstract
        streamStatsMetrics = try container.decodeIfPresent(StreamStatsMetrics.self, forKey: .streamStatsMetrics) ?? .default
        hideStreamButtons = try container.decodeIfPresent(Bool.self, forKey: .hideStreamButtons) ?? false
        streamKeyboardButtonPosition = try container.decodeIfPresent(NormalizedPoint.self, forKey: .streamKeyboardButtonPosition) ?? .trailingCenter
        streamKeyboardClearConfirmationDisabled = try container.decodeIfPresent(Bool.self, forKey: .streamKeyboardClearConfirmationDisabled) ?? false
        showAntiAfkIndicator = try container.decodeIfPresent(Bool.self, forKey: .showAntiAfkIndicator) ?? false
        showSessionReportAfterStream = try container.decodeIfPresent(Bool.self, forKey: .showSessionReportAfterStream) ?? false
        sessionReportDefaultVersion = try container.decodeIfPresent(Int.self, forKey: .sessionReportDefaultVersion) ?? 0
        sessionClockShowEveryMinutes = try container.decodeIfPresent(Int.self, forKey: .sessionClockShowEveryMinutes) ?? 60
        sessionClockShowDurationSeconds = try container.decodeIfPresent(Int.self, forKey: .sessionClockShowDurationSeconds) ?? 30
        // `keepMicEnabled` predates the three-state mode. Carry a true value forward as
        // voice-activity rather than silently turning someone's microphone off on upgrade.
        microphoneMode = try container.decodeIfPresent(MicrophoneMode.self, forKey: .microphoneMode)
            ?? (keepMicEnabled ? .voiceActivity : .disabled)
        mouseScrollSensitivity = try container.decodeIfPresent(Int.self, forKey: .mouseScrollSensitivity) ?? 30
        controllerMouseEmulation = try container.decodeIfPresent(Bool.self, forKey: .controllerMouseEmulation) ?? false
        clipboardPasteEnabled = try container.decodeIfPresent(Bool.self, forKey: .clipboardPasteEnabled) ?? true
        if let storedTouch = try container.decodeIfPresent(TouchSettings.self, forKey: .touch) {
            touch = storedTouch
        } else {
            // Installs that predate the touch-mode picker only had the Fortnite switch. Carry an
            // explicit "off" forward rather than silently re-enabling touch for someone who
            // turned it off on purpose.
            var seeded = TouchSettings.default
            if !fortnitePrefersNativeTouch { seeded.nativeTouchMode = .never }
            touch = seeded
        }
        controllerUISounds = try container.decodeIfPresent(Bool.self, forKey: .controllerUISounds) ?? false
        streamIntroSound = try container.decodeIfPresent(Bool.self, forKey: .streamIntroSound) ?? false
        queueReadySound = try container.decodeIfPresent(Bool.self, forKey: .queueReadySound) ?? false
        analyticsOptOut = try container.decodeIfPresent(Bool.self, forKey: .analyticsOptOut) ?? true
        analyticsConsentAsked = try container.decodeIfPresent(Bool.self, forKey: .analyticsConsentAsked) ?? false
        appLanguage = try container.decodeIfPresent(String.self, forKey: .appLanguage) ?? AppLanguage.systemDefault

        migrateLegacyTouchControlDefaults()
        normalizeStreamDefaults()
    }

    static let `default` = AppSettings(
        preferredRegion: "Auto",
        preferredResolution: "Auto",
        preferredFPS: 60,
        preferredQuality: "Balanced",
        preferredCodec: "Auto",
        maxBitrateMbps: 0,
        keyboardLayout: "en-US",
        gameLanguage: "en_US",
        enableL4S: false,
        enableCloudGsync: false,
        keepMicEnabled: false,
        showStatsOverlay: true,
        hideServerSelector: false,
        queueLiveActivitiesEnabled: true,
        selectedProviderIdpId: "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg",
        fortnitePrefersNativeTouch: true,
        touchControlLayouts: TouchControlLayout.defaultProfiles,
        streamerPreferences: .default,
        favoriteGameIds: []
    )

    func touchLayout(for profile: String) -> TouchControlLayout {
        touchControlLayouts[profile] ?? TouchControlLayout.preset(for: profile)
    }

    mutating func migrateLegacyTouchControlDefaults() {
        if touchControlLayouts["default"] == .legacyStandard || touchControlLayouts["default"] == .legacyShrunkStandard {
            touchControlLayouts["default"] = .standard
        }
        if touchControlLayouts["fortnite-mobile"] == .legacyFortniteMobile || touchControlLayouts["fortnite-mobile"] == .legacyShrunkFortniteMobile {
            touchControlLayouts["fortnite-mobile"] = .fortniteMobile
        }
    }

    mutating func normalizeStreamDefaults() {
        if sessionReportDefaultVersion < appSettingsSessionReportDefaultVersion {
            showSessionReportAfterStream = false
        }
        sessionReportDefaultVersion = appSettingsSessionReportDefaultVersion
        if !StreamSettingsResolver.aspectRatioOptions.contains(preferredAspectRatio) {
            preferredAspectRatio = "16:9"
        }
        preferredResolution = StreamSettingsResolver.normalizedResolution(
            preferredResolution,
            aspectRatio: preferredAspectRatio
        )
        if StreamColorQuality(rawValue: preferredColorQuality) == nil {
            preferredColorQuality = StreamColorQuality.eightBit420.rawValue
        }
        streamSharpeningAmount = min(max(streamSharpeningAmount, 0), 1)
        mouseSensitivity = min(max(mouseSensitivity, 0.25), 3)
        mouseAcceleration = min(max(mouseAcceleration, 0), 2)
        maxBitrateMbps = StreamSettingsResolver.normalizedBitratePreset(maxBitrateMbps)
        posterSizeScale = min(max(posterSizeScale, 0.75), 1.4)
        sessionProxyUrl = sessionProxyUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        mouseScrollSensitivity = min(max(mouseScrollSensitivity, 10), 100)
        sessionClockShowEveryMinutes = min(max(sessionClockShowEveryMinutes, 5), 240)
        sessionClockShowDurationSeconds = min(max(sessionClockShowDurationSeconds, 5), 120)
        touch.normalize()
        // Keep the legacy flag consistent with the picker so anything still reading it agrees.
        fortnitePrefersNativeTouch = touch.nativeTouchMode != .never
        // A HUD with every metric off cannot be recovered from inside the stream, so restore the
        // defaults rather than persisting an empty overlay.
        if !streamStatsMetrics.isMinimallyPopulated {
            streamStatsMetrics = .default
        }
        // Keep the legacy flag in step so any code still reading it stays correct.
        keepMicEnabled = microphoneMode.isEnabled
        if preferredRegion == "Auto" {
            preferredRegion = ""
        } else if !preferredRegion.isEmpty,
                  URL(string: preferredRegion)?.scheme?.lowercased() != "https" {
            // Older iOS builds stored display labels such as "US East" even
            // though launches require the server-provided region URL.
            preferredRegion = ""
        }
    }

    func safeVideoFallback() -> AppSettings {
        var fallback = self
        let capped = Self.cappedSafeVideoResolution(
            value: preferredResolution,
            aspectRatio: preferredAspectRatio
        )
        fallback.preferredResolution = capped.resolution
        fallback.preferredAspectRatio = capped.aspectRatio
        fallback.preferredFPS = min(preferredFPS, 60)
        if fallback.maxBitrateMbps > 0 {
            fallback.maxBitrateMbps = min(fallback.maxBitrateMbps, 75)
        }
        fallback.preferredCodec = "H264"
        fallback.preferredColorQuality = StreamColorQuality.eightBit420.rawValue
        fallback.hdrEnabled = false
        fallback.enableCloudGsync = false
        fallback.normalizeStreamDefaults()
        return fallback
    }

    private static func cappedSafeVideoResolution(value: String, aspectRatio: String) -> (resolution: String, aspectRatio: String) {
        let maxWidth = 1920
        let maxHeight = 1080
        let normalizedAspect = StreamSettingsResolver.normalizedAspectRatio(aspectRatio)
        let normalizedResolution = StreamSettingsResolver.normalizedResolution(
            value,
            aspectRatio: normalizedAspect
        )
        if let parsed = parseResolution(normalizedResolution),
           parsed.width <= maxWidth,
           parsed.height <= maxHeight {
            return (normalizedResolution, normalizedAspect)
        }

        let sameAspect = StreamSettingsResolver.resolutionChoices
            .filter { $0.aspectRatio == normalizedAspect && resolution($0.value, fitsWithinWidth: maxWidth, height: maxHeight) }
            .max { lhs, rhs in pixelCount(lhs.value) < pixelCount(rhs.value) }
        let fallback = StreamSettingsResolver.resolutionChoices
            .filter { resolution($0.value, fitsWithinWidth: maxWidth, height: maxHeight) }
            .max { lhs, rhs in
                let lhsPixels = pixelCount(lhs.value)
                let rhsPixels = pixelCount(rhs.value)
                if lhsPixels == rhsPixels {
                    return lhs.aspectRatio != "16:9" && rhs.aspectRatio == "16:9"
                }
                return lhsPixels < rhsPixels
            }
        let capped = sameAspect ?? fallback
        return (capped?.value ?? "1280x720", capped?.aspectRatio ?? "16:9")
    }

    private static func resolution(_ value: String, fitsWithinWidth maxWidth: Int, height maxHeight: Int) -> Bool {
        guard let parsed = parseResolution(value) else { return false }
        return parsed.width <= maxWidth && parsed.height <= maxHeight
    }

    private static func pixelCount(_ value: String) -> Int {
        guard let parsed = parseResolution(value) else { return 0 }
        return parsed.width * parsed.height
    }

    private static func parseResolution(_ value: String) -> (width: Int, height: Int)? {
        let parts = value.split(separator: "x", maxSplits: 1).map(String.init)
        guard parts.count == 2,
              let width = Int(parts[0]),
              let height = Int(parts[1]),
              width > 0,
              height > 0 else {
            return nil
        }
        return (width, height)
    }
}

enum CatalogWallpaperStorageError: LocalizedError {
    case invalidImage

    var errorDescription: String? {
        switch self {
        case .invalidImage:
            return "The selected file is not a supported image."
        }
    }
}

enum CatalogWallpaperStorage {
    private static let directoryName = "CatalogWallpapers"
    private static let managedFilenamePrefix = "catalog-wallpaper-"
    private static let maximumPixelDimension = 2_560

    static func storeSelectedImageData(_ data: Data) throws -> String {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else {
            throw CatalogWallpaperStorageError.invalidImage
        }
        let thumbnailOptions = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maximumPixelDimension
        ] as CFDictionary
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, thumbnailOptions),
              let encoded = UIImage(cgImage: image).jpegData(compressionQuality: 0.88) else {
            throw CatalogWallpaperStorageError.invalidImage
        }

        let filename = "\(managedFilenamePrefix)\(UUID().uuidString).jpg"
        let targetURL = try storageDirectory().appendingPathComponent(filename, isDirectory: false)
        try encoded.write(to: targetURL, options: .atomic)
        try? FileManager.default.setAttributes(
            [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
            ofItemAtPath: targetURL.path
        )
        return filename
    }

    static func wallpaperURL(for filename: String?) -> URL? {
        guard let filename, isManagedFilename(filename),
              let directory = try? storageDirectory() else {
            return nil
        }
        let url = directory.appendingPathComponent(filename, isDirectory: false)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        return url
    }

    static func pruneManagedWallpapers(keeping filename: String?) {
        guard let directory = try? storageDirectory(),
              let contents = try? FileManager.default.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: nil,
                options: [.skipsHiddenFiles]
              ) else {
            return
        }
        for url in contents {
            let candidate = url.lastPathComponent
            guard isManagedFilename(candidate), candidate != filename else { continue }
            try? FileManager.default.removeItem(at: url)
        }
    }

    private static func storageDirectory() throws -> URL {
        let applicationSupport = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = applicationSupport
            .appendingPathComponent("OpenNOW", isDirectory: true)
            .appendingPathComponent(directoryName, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication]
        )
        return directory
    }

    private static func isManagedFilename(_ filename: String) -> Bool {
        filename.hasPrefix(managedFilenamePrefix) &&
            filename.hasSuffix(".jpg") &&
            !filename.contains("/") &&
            !filename.contains("\\")
    }
}

struct StreamerPreferences: Codable, Equatable {
    var audioMuted: Bool
    var showStatsClock: Bool
    var showStatsBattery: Bool
    var showStatsCellular: Bool
    var touchControllerVisible: Bool
    var touchscreenModeEnabled: Bool
    var physicalControllerPassthrough: Bool
    var statsStyle: StreamStatsStyle
    var statsPosition: StreamStatsPosition
    var stretchStreamToFill: Bool

    enum CodingKeys: String, CodingKey {
        case audioMuted
        case showStatsClock
        case showStatsBattery
        case showStatsCellular
        case touchControllerVisible
        case touchscreenModeEnabled
        case physicalControllerPassthrough
        case statsStyle
        case statsPosition
        case stretchStreamToFill
    }

    init(
        audioMuted: Bool,
        showStatsClock: Bool,
        showStatsBattery: Bool,
        showStatsCellular: Bool,
        touchControllerVisible: Bool,
        touchscreenModeEnabled: Bool,
        physicalControllerPassthrough: Bool,
        statsStyle: StreamStatsStyle = .compact,
        statsPosition: StreamStatsPosition = .right,
        stretchStreamToFill: Bool = false
    ) {
        self.audioMuted = audioMuted
        self.showStatsClock = showStatsClock
        self.showStatsBattery = showStatsBattery
        self.showStatsCellular = showStatsCellular
        self.touchControllerVisible = touchControllerVisible
        self.touchscreenModeEnabled = touchscreenModeEnabled
        self.physicalControllerPassthrough = physicalControllerPassthrough
        self.statsStyle = statsStyle
        self.statsPosition = statsPosition
        self.stretchStreamToFill = stretchStreamToFill
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        audioMuted = try container.decodeIfPresent(Bool.self, forKey: .audioMuted) ?? Self.default.audioMuted
        showStatsClock = try container.decodeIfPresent(Bool.self, forKey: .showStatsClock) ?? Self.default.showStatsClock
        showStatsBattery = try container.decodeIfPresent(Bool.self, forKey: .showStatsBattery) ?? Self.default.showStatsBattery
        showStatsCellular = try container.decodeIfPresent(Bool.self, forKey: .showStatsCellular) ?? Self.default.showStatsCellular
        touchControllerVisible = try container.decodeIfPresent(Bool.self, forKey: .touchControllerVisible) ?? Self.default.touchControllerVisible
        touchscreenModeEnabled = try container.decodeIfPresent(Bool.self, forKey: .touchscreenModeEnabled) ?? Self.default.touchscreenModeEnabled
        physicalControllerPassthrough = try container.decodeIfPresent(Bool.self, forKey: .physicalControllerPassthrough) ?? Self.default.physicalControllerPassthrough
        statsStyle = try container.decodeIfPresent(StreamStatsStyle.self, forKey: .statsStyle) ?? Self.default.statsStyle
        statsPosition = try container.decodeIfPresent(StreamStatsPosition.self, forKey: .statsPosition) ?? Self.default.statsPosition
        stretchStreamToFill = try container.decodeIfPresent(Bool.self, forKey: .stretchStreamToFill) ?? Self.default.stretchStreamToFill
    }

    static let `default` = StreamerPreferences(
        audioMuted: false,
        showStatsClock: false,
        showStatsBattery: false,
        showStatsCellular: false,
        touchControllerVisible: false,
        touchscreenModeEnabled: false,
        physicalControllerPassthrough: true,
        statsStyle: .compact,
        statsPosition: .right,
        stretchStreamToFill: false
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
        leftStick: .init(x: 0.18, y: 0.77),
        rightCluster: .init(x: 0.83, y: 0.76),
        bottomCenter: .init(x: 0.50, y: 0.88)
    )

    static let fortniteMobile = TouchControlLayout(
        scale: 1,
        opacity: 0.52,
        buttonScale: 1,
        stickScale: 1,
        topLeft: .init(x: 0.16, y: 0.11),
        topCenter: .init(x: 0.50, y: 0.11),
        topRight: .init(x: 0.84, y: 0.11),
        leftStick: .init(x: 0.17, y: 0.77),
        rightCluster: .init(x: 0.84, y: 0.75),
        bottomCenter: .init(x: 0.50, y: 0.86)
    )

    static let legacyStandard = TouchControlLayout(
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

    static let legacyFortniteMobile = TouchControlLayout(
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

    static let legacyShrunkStandard = TouchControlLayout(
        scale: 0.70,
        opacity: 0.58,
        buttonScale: 1,
        stickScale: 1,
        topLeft: .init(x: 0.14, y: 0.12),
        topCenter: .init(x: 0.50, y: 0.12),
        topRight: .init(x: 0.86, y: 0.12),
        leftStick: .init(x: 0.18, y: 0.77),
        rightCluster: .init(x: 0.83, y: 0.76),
        bottomCenter: .init(x: 0.50, y: 0.88)
    )

    static let legacyShrunkFortniteMobile = TouchControlLayout(
        scale: 0.70,
        opacity: 0.52,
        buttonScale: 1.05,
        stickScale: 1,
        topLeft: .init(x: 0.16, y: 0.11),
        topCenter: .init(x: 0.50, y: 0.11),
        topRight: .init(x: 0.84, y: 0.11),
        leftStick: .init(x: 0.17, y: 0.77),
        rightCluster: .init(x: 0.84, y: 0.75),
        bottomCenter: .init(x: 0.50, y: 0.86)
    )

    static let defaultProfiles: [String: TouchControlLayout] = [
        "default": .standard,
        "fortnite-mobile": .fortniteMobile
    ]

    static func preset(for profile: String) -> TouchControlLayout {
        defaultProfiles[profile] ?? .standard
    }
}

struct StreamVideoProfile: Equatable {
    let width: Int
    let height: Int
    let fps: Int
    let maxBitrateKbps: Int

    var resolutionString: String {
        "\(width)x\(height)"
    }
}

enum StreamSettingsResolver {
    enum StreamResolutionPlan: Int, Comparable {
        case free = 0
        case priority = 1
        case ultimate = 2

        static func < (lhs: StreamResolutionPlan, rhs: StreamResolutionPlan) -> Bool {
            lhs.rawValue < rhs.rawValue
        }

        var label: String? {
            switch self {
            case .free: return nil
            case .priority: return "Priority"
            case .ultimate: return "Ultimate"
            }
        }
    }

    struct StreamResolutionChoice: Identifiable, Equatable {
        let value: String
        let aspectRatio: String
        let tier: String
        let requiredPlan: StreamResolutionPlan

        var id: String { value }

        var label: String {
            value.replacingOccurrences(of: "x", with: " x ")
        }
    }

    static let aspectRatioOptions = ["16:9", "16:10", "4:3", "5:4", "20:9", "21:9", "24:10", "32:9"]

    static let resolutionChoices: [StreamResolutionChoice] = [
        .init(value: "1280x720", aspectRatio: "16:9", tier: "720", requiredPlan: .free),
        .init(value: "1366x768", aspectRatio: "16:9", tier: "768", requiredPlan: .free),
        .init(value: "1600x900", aspectRatio: "16:9", tier: "900", requiredPlan: .free),
        .init(value: "1280x800", aspectRatio: "16:10", tier: "720", requiredPlan: .free),
        .init(value: "1440x900", aspectRatio: "16:10", tier: "900", requiredPlan: .free),
        .init(value: "1680x1050", aspectRatio: "16:10", tier: "1050", requiredPlan: .free),
        .init(value: "1920x1080", aspectRatio: "16:9", tier: "1080", requiredPlan: .free),
        .init(value: "1920x1200", aspectRatio: "16:10", tier: "1080", requiredPlan: .free),
        .init(value: "1024x768", aspectRatio: "4:3", tier: "768", requiredPlan: .free),
        .init(value: "1112x834", aspectRatio: "4:3", tier: "834", requiredPlan: .free),
        .init(value: "1600x1200", aspectRatio: "4:3", tier: "1080", requiredPlan: .free),
        .init(value: "1280x1024", aspectRatio: "5:4", tier: "1050", requiredPlan: .free),
        .init(value: "1600x720", aspectRatio: "20:9", tier: "720", requiredPlan: .free),
        .init(value: "1680x720", aspectRatio: "21:9", tier: "720", requiredPlan: .free),
        .init(value: "2400x1080", aspectRatio: "20:9", tier: "1080", requiredPlan: .priority),
        .init(value: "2560x1080", aspectRatio: "21:9", tier: "1080", requiredPlan: .priority),
        .init(value: "3840x1080", aspectRatio: "32:9", tier: "1080", requiredPlan: .priority),
        .init(value: "2560x1440", aspectRatio: "16:9", tier: "1440", requiredPlan: .priority),
        .init(value: "2560x1600", aspectRatio: "16:10", tier: "1440", requiredPlan: .priority),
        .init(value: "3200x1440", aspectRatio: "20:9", tier: "1440", requiredPlan: .priority),
        .init(value: "3440x1440", aspectRatio: "21:9", tier: "1440", requiredPlan: .priority),
        .init(value: "5120x1440", aspectRatio: "32:9", tier: "1440", requiredPlan: .priority),
        .init(value: "3840x1600", aspectRatio: "24:10", tier: "1440", requiredPlan: .priority),
        .init(value: "3840x2160", aspectRatio: "16:9", tier: "2160", requiredPlan: .ultimate),
        .init(value: "3456x2160", aspectRatio: "16:10", tier: "2160", requiredPlan: .ultimate),
        .init(value: "4800x2160", aspectRatio: "20:9", tier: "2160", requiredPlan: .ultimate),
        .init(value: "5120x2160", aspectRatio: "21:9", tier: "2160", requiredPlan: .ultimate),
        .init(value: "5120x2880", aspectRatio: "16:9", tier: "2880", requiredPlan: .ultimate)
    ]

    static let bitrateOptionsMbps = [0, 5, 10, 15, 20, 25, 30, 35, 40, 50, 60, 75, 100]

    static func normalizedBitratePreset(_ value: Int) -> Int {
        guard value > 0 else { return 0 }
        return bitrateOptionsMbps.dropFirst().min { left, right in
            abs(left - value) < abs(right - value)
        } ?? 100
    }

    static var resolutionOptions: [(value: String, label: String)] {
        resolutionOptions(for: "16:9")
    }

    static func choices(forAspectRatio aspectRatio: String) -> [StreamResolutionChoice] {
        let normalizedAspect = normalizedAspectRatio(aspectRatio)
        return resolutionChoices.filter { $0.aspectRatio == normalizedAspect }
    }

    static func metalFXResolution(preset: MetalFXQualityPreset, aspectRatio: String,
                                  displaySize: CGSize, stretch: Bool, membershipTier: String?) -> StreamResolutionChoice? {
        guard let scale = preset.inputScale else { return nil }
        return choices(forAspectRatio: aspectRatio).filter { choice in
            guard isResolutionAvailable(choice, membershipTier: membershipTier) else { return false }
            let source = pixelSize(choice.value)
            let target = NativeStreamVideoEffectsPolicy.presentationSize(source: source, display: displaySize, stretch: stretch)
            return NativeStreamVideoEffectsPolicy.upscaleSize(source: source, destination: target) != nil
        }.min { left, right in
            func distance(_ choice: StreamResolutionChoice) -> CGFloat {
                let source = pixelSize(choice.value)
                let target = NativeStreamVideoEffectsPolicy.presentationSize(source: source, display: displaySize, stretch: stretch)
                return abs(source.width / target.width - scale) + abs(source.height / target.height - scale)
            }
            return distance(left) < distance(right)
        }
    }

    static func pixelSize(_ resolution: String) -> CGSize {
        let values = resolution.split(separator: "x").compactMap { Double($0) }
        guard values.count == 2 else { return .zero }
        return CGSize(width: values[0], height: values[1])
    }

    static func resolutionChoice(value: String, aspectRatio: String) -> StreamResolutionChoice? {
        let normalizedAspect = normalizedAspectRatio(aspectRatio)
        return resolutionChoices.first { $0.value == value && $0.aspectRatio == normalizedAspect }
    }

    static func isResolutionAvailable(_ choice: StreamResolutionChoice, membershipTier: String?) -> Bool {
        plan(for: membershipTier) >= choice.requiredPlan
    }

    static func resolutionOptions(for aspectRatio: String) -> [(value: String, label: String)] {
        let choices = choices(forAspectRatio: aspectRatio)
            .map { choice in
                let suffix = choice.requiredPlan.label.map { " (\($0))" } ?? ""
                return (choice.value, "\(choice.label)\(suffix)")
            }
        return [("Auto", "Auto")] + choices
    }

    static let keyboardLayoutOptions: [(value: String, label: String)] = [
        ("en-US", "English (US)"),
        ("en-GB", "English (UK)"),
        ("tr-TR", "Turkish Q"),
        ("de-DE", "German"),
        ("fr-FR", "French"),
        ("es-ES", "Spanish"),
        ("es-MX", "Spanish (Latin America)"),
        ("it-IT", "Italian"),
        ("pt-PT", "Portuguese (Portugal)"),
        ("pt-BR", "Portuguese (Brazil)"),
        ("pl-PL", "Polish"),
        ("ru-RU", "Russian"),
        ("ja-JP", "Japanese"),
        ("ko-KR", "Korean"),
        ("zh-CN", "Chinese (Simplified)"),
        ("zh-TW", "Chinese (Traditional)")
    ]

    static let gameLanguageOptions: [(value: String, label: String)] = [
        ("en_US", "English (US)"),
        ("en_GB", "English (UK)"),
        ("de_DE", "German"),
        ("fr_FR", "French"),
        ("es_ES", "Spanish"),
        ("es_MX", "Spanish (Latin America)"),
        ("it_IT", "Italian"),
        ("pt_PT", "Portuguese (Portugal)"),
        ("pt_BR", "Portuguese (Brazil)"),
        ("ru_RU", "Russian"),
        ("pl_PL", "Polish"),
        ("tr_TR", "Turkish"),
        ("ar_SA", "Arabic"),
        ("ja_JP", "Japanese"),
        ("ko_KR", "Korean"),
        ("zh_CN", "Chinese (Simplified)"),
        ("zh_TW", "Chinese (Traditional)"),
        ("th_TH", "Thai"),
        ("vi_VN", "Vietnamese"),
        ("id_ID", "Indonesian"),
        ("cs_CZ", "Czech"),
        ("el_GR", "Greek"),
        ("hu_HU", "Hungarian"),
        ("ro_RO", "Romanian"),
        ("uk_UA", "Ukrainian"),
        ("nl_NL", "Dutch"),
        ("sv_SE", "Swedish"),
        ("da_DK", "Danish"),
        ("fi_FI", "Finnish"),
        ("no_NO", "Norwegian")
    ]

    static func profile(for settings: AppSettings) -> StreamVideoProfile {
        profile(for: settings, membershipTier: nil)
    }

    static func profile(for settings: AppSettings, membershipTier: String?) -> StreamVideoProfile {
        #if os(tvOS)
        return profile(
            for: settings,
            nativeBounds: .zero,
            nativeScale: 1,
            userInterfaceIdiom: .tv,
            membershipTier: membershipTier
        )
        #else
        return profile(
            for: settings,
            nativeBounds: UIScreen.main.nativeBounds,
            nativeScale: UIScreen.main.nativeScale,
            userInterfaceIdiom: UIDevice.current.userInterfaceIdiom,
            membershipTier: membershipTier
        )
        #endif
    }

    static func settings(
        _ settings: AppSettings,
        applying preset: StreamPreset,
        membershipTier: String? = nil
    ) -> AppSettings {
        var updated = settings
        updated.streamPreset = preset
        guard preset != .custom else { return updated }

        let aspect = normalizedAspectRatio(settings.preferredAspectRatio)
        let maxHeight: Int
        switch preset {
        case .custom: maxHeight = .max
        case .lowDataSaver: maxHeight = 800
        case .recommended, .medium: maxHeight = 1_200
        case .high: maxHeight = 1_600
        }
        let maxPlan = profilePlanLimit(for: membershipTier)
        let choice = choices(forAspectRatio: aspect)
            .filter { $0.requiredPlan <= maxPlan }
            .filter { (parseResolution($0.value)?.height ?? .max) <= maxHeight }
            .max { lhs, rhs in
                let lhsPixels = parseResolution(lhs.value).map { $0.width * $0.height } ?? 0
                let rhsPixels = parseResolution(rhs.value).map { $0.width * $0.height } ?? 0
                return lhsPixels < rhsPixels
            }
            ?? choices(forAspectRatio: aspect).first
            ?? resolutionChoices[0]
        updated.preferredAspectRatio = choice.aspectRatio
        updated.preferredResolution = choice.value
        updated.preferredColorQuality = StreamColorQuality.eightBit420.rawValue
        updated.hdrEnabled = false
        updated.enableCloudGsync = false
        switch preset {
        case .custom:
            break
        case .lowDataSaver:
            updated.preferredFPS = 30
            updated.maxBitrateMbps = 12
            updated.preferredQuality = "Data Saver"
        case .recommended, .medium:
            updated.preferredFPS = 60
            updated.maxBitrateMbps = 35
            updated.preferredQuality = "Balanced"
        case .high:
            // Apple displays currently top out at 120 Hz. Requesting 360 FPS
            // wastes bandwidth and decoder work without a visible benefit.
            updated.preferredFPS = maxPlan >= .ultimate ? 120 : 60
            updated.maxBitrateMbps = 75
            updated.preferredQuality = "Quality"
        }
        return updated
    }

    static func sessionSignature(for settings: AppSettings) -> String {
        sessionSignature(for: settings, profile: profile(for: settings, membershipTier: nil))
    }

    static func sessionSignature(for settings: AppSettings, profile: StreamVideoProfile) -> String {
        let color = colorQuality(for: settings).rawValue
        var signature = [
            "opennow-ios-stream-v2",
            "res=\(profile.width)x\(profile.height)",
            "fps=\(profile.fps)",
            "bitrate=\(profile.maxBitrateKbps / 1000)",
            "codec=\(settings.preferredCodec.uppercased())",
            "transport=\(settings.experimentalNativeNVSTEnabled ? "nvst" : "webrtc")",
            "color=\(color)",
            "hdr=\(settings.hdrEnabled ? 1 : 0)",
            "l4s=\(settings.enableL4S ? 1 : 0)",
            "gsync=\(settings.enableCloudGsync ? 1 : 0)",
            "keyboard=\(settings.keyboardLayout.trimmingCharacters(in: .whitespacesAndNewlines))",
            "language=\(settings.gameLanguage.trimmingCharacters(in: .whitespacesAndNewlines))"
        ]
        // Changing the allocation identity needs a new host session, even if
        // resolution, codec and the saved touch preference are unchanged.
        if requiresDesktopColorProvisioning(for: settings) {
            signature.append("provisioning=desktop-444-v3")
            signature.append("desktopTouch=\(settings.experimentalDesktop444TouchEnabled ? settings.touch.nativeTouchMode.rawValue : "off")")
        }
        return signature.joined(separator: ";")
    }

    static func profile(
        for settings: AppSettings,
        nativeBounds: CGRect,
        nativeScale: CGFloat,
        userInterfaceIdiom: UIUserInterfaceIdiom,
        membershipTier: String? = nil
    ) -> StreamVideoProfile {
        let aspectRatio = normalizedAspectRatio(settings.preferredAspectRatio)
        let maxPlan = profilePlanLimit(for: membershipTier)
        let fps = min(normalizedFPS(settings.preferredFPS), maxPlan >= .ultimate ? 120 : 60)
        let normalizedResolution = normalizedResolution(
            settings.preferredResolution,
            aspectRatio: aspectRatio,
            maxPlan: maxPlan
        )
        let requestedResolution = parseResolution(normalizedResolution)
        let base = requestedResolution ?? automaticResolution(
            settings: settings,
            aspectRatio: aspectRatio,
            nativeBounds: nativeBounds,
            nativeScale: nativeScale,
            userInterfaceIdiom: userInterfaceIdiom,
            maxPlan: maxPlan
        )
        let bitrateMbps = normalizedMaxBitrateMbps(settings.maxBitrateMbps)
            ?? automaticBitrateMbps(width: base.width, height: base.height, fps: fps, quality: settings.preferredQuality)
        return StreamVideoProfile(
            width: base.width,
            height: base.height,
            fps: fps,
            maxBitrateKbps: max(5_000, bitrateMbps * 1_000)
        )
    }

    static func normalizedKeyboardLayout(_ value: String) -> String {
        keyboardLayoutOptions.contains(where: { $0.value == value }) ? value : "en-US"
    }

    static func normalizedGameLanguage(_ value: String) -> String {
        gameLanguageOptions.contains(where: { $0.value == value }) ? value : "en_US"
    }

    static func normalizedAspectRatio(_ value: String) -> String {
        aspectRatioOptions.contains(value) ? value : "16:9"
    }

    static func normalizedResolution(_ value: String, aspectRatio: String) -> String {
        normalizedResolution(value, aspectRatio: aspectRatio, maxPlan: .ultimate)
    }

    static func normalizedResolution(
        _ value: String,
        aspectRatio: String,
        maxPlan: StreamResolutionPlan
    ) -> String {
        guard value != "Auto" else { return "Auto" }
        let normalizedAspect = normalizedAspectRatio(aspectRatio)
        if let exact = resolutionChoices.first(where: { $0.value == value && $0.aspectRatio == normalizedAspect }),
           exact.requiredPlan <= maxPlan {
            return value
        }
        if resolutionChoices.allSatisfy({ $0.value != value }),
           let parsed = parseResolution(value),
           customResolutionIsAvailable(width: parsed.width, height: parsed.height, maxPlan: maxPlan) {
            return "\(parsed.width)x\(parsed.height)"
        }
        let requestedTier = resolutionChoices.first(where: { $0.value == value })?.tier
            ?? resolutionTier(forHeight: parseResolution(value)?.height ?? 1080)
        if let preferred = preferredResolutionByTierAndAspect[requestedTier]?[normalizedAspect],
           resolutionChoices.contains(where: { $0.value == preferred && $0.requiredPlan <= maxPlan }) {
            return preferred
        }
        let requestedPixels = parseResolution(value).map { $0.width * $0.height } ?? (1920 * 1080)
        return resolutionChoices
            .filter { $0.aspectRatio == normalizedAspect && $0.requiredPlan <= maxPlan }
            .min { lhs, rhs in
                let lhsPixels = parseResolution(lhs.value).map { $0.width * $0.height } ?? 0
                let rhsPixels = parseResolution(rhs.value).map { $0.width * $0.height } ?? 0
                let lhsDistance = abs(lhsPixels - requestedPixels)
                let rhsDistance = abs(rhsPixels - requestedPixels)
                return lhsDistance == rhsDistance ? lhsPixels < rhsPixels : lhsDistance < rhsDistance
            }?
            .value ?? "1920x1080"
    }

    static func customResolutionIsAvailable(
        width: Int,
        height: Int,
        membershipTier: String?
    ) -> Bool {
        customResolutionIsAvailable(width: width, height: height, maxPlan: plan(for: membershipTier))
    }

    private static func customResolutionIsAvailable(
        width: Int,
        height: Int,
        maxPlan: StreamResolutionPlan
    ) -> Bool {
        guard width > 0, height > 0 else { return false }
        let available = resolutionChoices.filter { $0.requiredPlan <= maxPlan }
        guard !available.isEmpty else { return false }
        let parsed = available.compactMap { parseResolution($0.value) }
        guard let maxWidth = parsed.map(\.width).max(),
              let maxHeight = parsed.map(\.height).max(),
              let maxPixels = parsed.map({ $0.width * $0.height }).max() else {
            return false
        }
        return width <= maxWidth && height <= maxHeight && width * height <= maxPixels
    }

    static func plan(for membershipTier: String?) -> StreamResolutionPlan {
        let normalized = normalizedMembershipKey(membershipTier)
        if normalized.contains("ULTIMATE") || normalized.contains("RTX3080") {
            return .ultimate
        }
        if normalized.contains("PREMIUM")
            || normalized.contains("PRIORITY")
            || normalized.contains("PERFORMANCE")
            || normalized.contains("FOUNDERS") {
            return .priority
        }
        return .free
    }

    static func isHDRAvailable(subscription: SubscriptionSnapshot?, fallbackMembershipTier: String?) -> Bool {
        plan(for: subscription?.membershipTier ?? fallbackMembershipTier) >= .ultimate
    }

    static func colorQuality(for settings: AppSettings) -> StreamColorQuality {
        let color = StreamColorQuality(rawValue: settings.preferredColorQuality) ?? .eightBit420
        guard settings.hdrEnabled, color.bitDepth < 10 else { return color }
        return color == .eightBit444 ? .tenBit444 : .tenBit420
    }

    /// An explicit picker choice takes priority over the preceding HDR preference.
    /// Keep legacy decoding/launch promotion separate from new user selections.
    static func selectingColor(_ color: StreamColorQuality, in settings: AppSettings) -> AppSettings {
        var updated = settings
        updated.preferredColorQuality = color.rawValue
        updated.streamPreset = .custom
        if color.bitDepth < 10 { updated.hdrEnabled = false }
        if color.chromaFormat == 2 && updated.experimentalNativeNVSTEnabled {
            updated.preferredCodec = "H265"
        }
        return updated
    }

    static func selectingHDR(_ enabled: Bool, in settings: AppSettings,
                             h265Available: Bool) -> AppSettings {
        var updated = settings
        updated.hdrEnabled = enabled
        updated.streamPreset = .custom
        if enabled {
            updated.preferredColorQuality = colorQuality(for: updated).rawValue
            if h265Available { updated.preferredCodec = "H265" }
        }
        return updated
    }

    static func remoteColorMatches(color: StreamColorQuality?, hdr: Bool?, settings: AppSettings) -> Bool {
        (color == nil || color == colorQuality(for: settings)) && (hdr == nil || hdr == settings.hdrEnabled)
    }

    static func requiresDesktopColorProvisioning(for settings: AppSettings) -> Bool {
        settings.experimentalNativeNVSTEnabled && colorQuality(for: settings).chromaFormat == 2
    }

    private static func profilePlanLimit(for membershipTier: String?) -> StreamResolutionPlan {
        guard let membershipTier,
              !membershipTier.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return .ultimate
        }
        return plan(for: membershipTier)
    }

    private static func normalizedFPS(_ value: Int) -> Int {
        min(max(value, 30), 120)
    }

    private static func parseResolution(_ value: String) -> (width: Int, height: Int)? {
        guard value != "Auto" else { return nil }
        let parts = value.split(separator: "x", maxSplits: 1).map(String.init)
        guard parts.count == 2,
              let width = Int(parts[0]),
              let height = Int(parts[1]),
              width > 0,
              height > 0 else {
            return nil
        }
        return (width, height)
    }

    private static func automaticResolution(
        settings: AppSettings,
        aspectRatio: String,
        nativeBounds: CGRect,
        nativeScale: CGFloat,
        userInterfaceIdiom: UIUserInterfaceIdiom,
        maxPlan: StreamResolutionPlan
    ) -> (width: Int, height: Int) {
        let longSide = max(nativeBounds.width, nativeBounds.height)
        let shortSide = min(nativeBounds.width, nativeBounds.height)
        let supports1440 = longSide >= 2500 || shortSide >= 1400 || nativeScale >= 3.0
        let prefersQuality = settings.preferredQuality.caseInsensitiveCompare("Quality") == .orderedSame

        let tier: String
        if maxPlan >= .priority, userInterfaceIdiom == .pad, prefersQuality, supports1440 {
            tier = "1440"
        } else if userInterfaceIdiom == .pad || userInterfaceIdiom == .tv {
            tier = "1080"
        } else {
            tier = "720"
        }
        let value = [tier, "1080", "900", "720"]
            .compactMap { preferredResolutionByTierAndAspect[$0]?[aspectRatio] }
            .first { candidate in
                resolutionChoices.contains { $0.value == candidate && $0.requiredPlan <= maxPlan }
            }
            ?? preferredResolutionByTierAndAspect["1080"]?["16:9"]
            ?? "1920x1080"
        return parseResolution(value) ?? (1920, 1080)
    }

    private static func automaticBitrateMbps(width: Int, height: Int, fps: Int, quality: String) -> Int {
        let pixels = width * height
        let qualityKey = quality.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let base: Int

        if pixels >= 3840 * 2160 {
            base = qualityKey == "data saver" ? 35 : (qualityKey == "quality" ? 75 : 50)
        } else if pixels >= 2560 * 1440 {
            base = qualityKey == "data saver" ? 18 : (qualityKey == "quality" ? 36 : 28)
        } else if pixels >= 1920 * 1080 {
            base = qualityKey == "data saver" ? 12 : (qualityKey == "quality" ? 24 : 18)
        } else {
            base = qualityKey == "data saver" ? 9 : (qualityKey == "quality" ? 18 : 13)
        }

        let fpsAdjusted = fps > 60 ? Int((Double(base) * 1.35).rounded()) : base
        return min(max(fpsAdjusted, 5), 100)
    }

    private static func normalizedMaxBitrateMbps(_ value: Int) -> Int? {
        guard value > 0 else { return nil }
        return min(max(value, 5), 150)
    }

    private static func resolutionTier(forHeight height: Int) -> String {
        switch height {
        case 2600...: return "2880"
        case 2000...: return "2160"
        case 1320...: return "1440"
        case 1120...: return "1080"
        case 975...: return "1050"
        case 850...: return "900"
        case 800...: return "834"
        case 740...: return "768"
        default: return "720"
        }
    }

    private static let preferredResolutionByTierAndAspect: [String: [String: String]] = [
        "720": ["16:9": "1280x720", "16:10": "1280x800", "4:3": "1024x768", "20:9": "1600x720", "21:9": "1680x720"],
        "768": ["16:9": "1366x768", "4:3": "1024x768"],
        "834": ["4:3": "1112x834"],
        "900": ["16:9": "1600x900", "16:10": "1440x900"],
        "1050": ["16:10": "1680x1050", "5:4": "1280x1024"],
        "1080": ["16:9": "1920x1080", "16:10": "1920x1200", "4:3": "1600x1200", "20:9": "2400x1080", "21:9": "2560x1080", "32:9": "3840x1080"],
        "1440": ["16:9": "2560x1440", "16:10": "2560x1600", "20:9": "3200x1440", "21:9": "3440x1440", "24:10": "3840x1600", "32:9": "5120x1440"],
        "2160": ["16:9": "3840x2160", "16:10": "3456x2160", "20:9": "4800x2160", "21:9": "5120x2160"],
        "2880": ["16:9": "5120x2880"]
    ]
}

enum OpenNOWPlatform {
    #if os(tvOS)
    static let supportsNativeOAuth = true
    static let supportsEmbeddedStreamer = true
    static let displayName = "tvOS"
    static let authUnavailableReason = ""
    static let streamingUnavailableReason = ""
    #else
    static let supportsNativeOAuth = true
    static let supportsEmbeddedStreamer = true
    static let displayName = "iOS"
    static let authUnavailableReason = ""
    static let streamingUnavailableReason = ""
    #endif

    /// Hardware identifier such as `iPhone16,1`. `UIDevice.model` only ever says "iPhone", which
    /// is useless in a bug report — decoder behaviour differs sharply between generations.
    static let modelIdentifier: String = {
        #if targetEnvironment(simulator)
        return ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"] ?? "Simulator"
        #else
        var systemInfo = utsname()
        uname(&systemInfo)
        let identifier = withUnsafeBytes(of: &systemInfo.machine) { buffer -> String in
            let bytes = buffer.prefix { $0 != 0 }
            return String(decoding: bytes, as: UTF8.self)
        }
        return identifier.isEmpty ? "Unknown" : identifier
        #endif
    }()
}

private enum GFNConstants {
    static let serviceUrlsEndpoint = URL(string: "https://pcs.geforcenow.com/v1/serviceUrls")!
    static let tokenEndpoint = URL(string: "https://login.nvidia.com/token")!
    static let userInfoEndpoint = URL(string: "https://login.nvidia.com/userinfo")!
    static let authEndpoint = URL(string: "https://login.nvidia.com/authorize")!
    static let deviceAuthorizationEndpoint = URL(string: "https://login.nvidia.com/device/authorize")!
    static let clientTokenEndpoint = URL(string: "https://login.nvidia.com/client_token")!
    static let mesEndpoint = URL(string: "https://mes.geforcenow.com/v4/subscriptions")!
    static let graphQL = "https://games.geforce.com/graphql"
    static let accountLinkingBase = URL(string: "https://als.geforcenow.com/v1")!

    static let clientId = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ"
    static let deviceCodeClientId = "q61ddeJrVt7O90Nl-P-N7I36yctih4Ml6FyXLrb6j-U"
    static let accountLinkingClientId = "gfn-pc"
    static let accountLinkingRedirectUri = "http://localhost:2259/"
    static let scopes = "openid consent email tk_client age"
    static let defaultProvider = LoginProvider(
        idpId: "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg",
        code: "NVIDIA",
        displayName: "NVIDIA",
        streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
        priority: 0
    )
    static let lcarsClientId = "ec7e38d4-03af-4b58-b131-cfb0495903ab"
    static let gfnClientVersion = "2.0.88.129"
    static let panelsQueryHash = "46ec15f267a056e7d5e46e629efa929529e5e7542a4850faece90b9f8fa5f810"
    static let libraryWithTimeQueryHash = "7f54d6bbbf3b1c09d0e5264dfa36f0f4aaf5e2678f2089f0cbf0d4dda18c3af9"
    static let appMetadataQueryHash = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7"
    static let userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.88.129"
    /// The user agent the official GeForce NOW Android client sends for a touch session. It travels
    /// with the touch CloudMatch identity in `StreamDeviceProfile`; the two are one signal and must
    /// not be set independently.
    static let touchUserAgent = "GFN-PC/22.0 (Android-Generic-Touch 14) PGC/3.8 (6.36.38319306) okhttp/4.12.0"
    static let oauthRedirectUri = "http://localhost:2259"
    static let oauthCallbackScheme = "opennowios"
    static let oauthRedirectPort: UInt16 = 2259
    static let sessionModifyActionAdUpdate = 6
    static let streamSettingsMetadataKey = "OpenNOWStreamSettingsSignature"
    static let storageAddonType = "STORAGE"
    static let storageAddonSubType = "PERMANENT_STORAGE"
    static let totalStorageSizeAttribute = "TOTAL_STORAGE_SIZE_IN_GB"
    static let usedStorageSizeAttribute = "USED_STORAGE_SIZE_IN_GB"
    static let storageMetroRegionAttribute = "STORAGE_METRO_REGION"
    static let storageMetroRegionNameAttribute = "STORAGE_METRO_REGION_NAME"
    static let storageManagementURL = URL(string: "https://gfn.link/cloudstorage")!
    static let storageResetURL = URL(string: "https://gfn.link/resetstorage")!
    static let storageAddURL = URL(string: "https://gfn.link/addstorage")!
    static let accountHelpURL = URL(string: "https://gfn.link/5399")!
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

/// CloudMatch's control address is a queue endpoint, never a media/signaling address.
enum SessionControlRouting {
    static func baseURL(host rawHost: String?, port: Int?) -> String? {
        guard port == nil || port == 443 else { return nil }
        var host = rawHost?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? ""
        while host.hasSuffix(".") { host.removeLast() }
        let pattern = #"^np-[a-z0-9]+(?:-[a-z0-9]+)+\.(?:cloudmatchbeta|cloudmatch)\.nvidiagrid\.net$"#
        guard host.range(of: pattern, options: .regularExpression) != nil else { return nil }
        return "https://\(host)"
    }

    static func pollBase(for session: ActiveSession) -> String {
        guard let launchHost = URL(string: session.streamingBaseUrl)?.host?.lowercased(),
              launchHost.hasSuffix(".cloudmatchbeta.nvidiagrid.net")
                || launchHost.hasSuffix(".cloudmatch.nvidiagrid.net"),
              let controlBase = session.sessionControlBaseUrl,
              let controlURL = URL(string: controlBase),
              controlURL.scheme == "https",
              controlURL.port == nil || controlURL.port == 443,
              controlURL.path.isEmpty || controlURL.path == "/",
              controlURL.query == nil,
              controlURL.fragment == nil,
              let validated = baseURL(host: controlURL.host, port: controlURL.port) else {
            return session.streamingBaseUrl
        }
        return validated
    }
}

enum CloudMatchQueueStatus {
    static func isAbandoned(_ requestStatus: [String: Any]?) -> Bool {
        let code = requestStatus?["statusCode"] as? Int
        let description = (requestStatus?["statusDescription"] as? String)?.uppercased() ?? ""
        let unified = (requestStatus?["unifiedErrorCode"] as? String)?.uppercased() ?? ""
        return code == 69 || description.contains("SESSION_REQUEST_IN_QUEUE_ABANDONED") || unified == "4A8C300F"
    }
}

func normalizeGameStore(_ store: String) -> String {
    store
        .uppercased()
        .components(separatedBy: CharacterSet(charactersIn: " -"))
        .filter { !$0.isEmpty }
        .joined(separator: "_")
}

func gameStoreDisplayName(_ store: String) -> String {
    let parts = store
        .split(separator: ",")
        .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        .filter { !$0.isEmpty }
    let normalizedParts = parts.isEmpty ? [store.trimmingCharacters(in: .whitespacesAndNewlines)] : parts
    let labels = normalizedParts.map { part -> String in
        switch normalizeGameStore(part) {
        case "EPIC", "EGS", "EPIC_GAMES_STORE":
            return "Epic"
        case "STEAM":
            return "Steam"
        case "XBOX", "XBOX_GAME_PASS", "GAME_PASS":
            return "Xbox"
        case "MICROSOFT", "MICROSOFT_STORE":
            return "Microsoft Store"
        case "UBISOFT", "UBISOFT_CONNECT":
            return "Ubisoft Connect"
        case "EA", "EA_APP", "ORIGIN":
            return "EA app"
        case "GOG", "GOG_COM":
            return "GOG"
        case "BATTLENET", "BATTLE_NET", "BLIZZARD":
            return "Battle.net"
        case "RIOT", "RIOT_CLIENT", "RIOT_GAMES":
            return "Riot"
        case "ROCKSTAR", "ROCKSTAR_GAMES", "ROCKSTAR_GAMES_LAUNCHER":
            return "Rockstar"
        case "GOOGLE_PLAY", "PLAY_STORE", "ANDROID":
            return "Google Play"
        case "AMAZON", "AMAZON_GAMES":
            return "Amazon Games"
        default:
            let words = part
                .replacingOccurrences(of: "_", with: " ")
                .lowercased()
                .split(separator: " ")
                .map { String($0.prefix(1)).uppercased() + String($0.dropFirst()) }
            return words.joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "Unknown"
                : words.joined(separator: " ")
        }
    }
    return Array(Set(labels)).sorted().joined(separator: " / ")
}

func gameMetadataDisplayLabel(_ rawValue: String) -> String {
    let trimmed = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return "" }

    var normalized = trimmed
    let uppercase = normalized.uppercased()
    if uppercase.hasPrefix("GFN_") || uppercase.hasPrefix("GAME_") {
        normalized = String(normalized.dropFirst(4))
    }

    normalized = normalized
        .replacingOccurrences(of: "_", with: " ")
        .replacingOccurrences(of: "-", with: " ")
    normalized = normalized
        .split(whereSeparator: { $0.isWhitespace })
        .map(String.init)
        .joined(separator: " ")
        .trimmingCharacters(in: .whitespacesAndNewlines)

    let key = normalized
        .lowercased()
        .components(separatedBy: CharacterSet.alphanumerics.inverted)
        .filter { !$0.isEmpty }
        .joined()
    if key.isEmpty || key == "na" || key == "unknown" || key == "none" {
        return ""
    }

    switch key {
    case "readytoplay":
        return "Ready to Play"
    case "installtoplay":
        return "Install to Play"
    case "fullgame":
        return "Full Game"
    case "singleplayer":
        return "Single-player"
    case "multiplayer":
        return "Multiplayer"
    case "mousekeyboard", "keyboardmouse":
        return "Mouse and Keyboard"
    case "gamepad":
        return "Gamepad"
    case "dualshock4", "dualshock4gamepad":
        return "DualShock 4"
    case "dualsense", "dualsensegamepad":
        return "DualSense"
    case "nvidia":
        return "NVIDIA"
    default:
        break
    }

    let alwaysUppercase: Set<String> = ["pc", "vr", "dlc", "hdr", "rtx", "fps", "pvp", "pve", "mmo", "rpg"]
    return normalized
        .split(separator: " ")
        .map { word -> String in
            let lower = word.lowercased()
            if alwaysUppercase.contains(lower) {
                return lower.uppercased()
            }
            if lower == "ios" {
                return "iOS"
            }
            if lower == "macos" {
                return "macOS"
            }
            return String(lower.prefix(1)).uppercased() + String(lower.dropFirst())
        }
        .joined(separator: " ")
}

func gameMetadataDisplayLabels(_ rawValues: [String]) -> [String] {
    var seen: Set<String> = []
    var labels: [String] = []
    for rawValue in rawValues {
        let parts = rawValue
            .split(separator: ",")
            .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        for part in parts.isEmpty ? [rawValue] : parts {
            let label = gameMetadataDisplayLabel(part)
            guard !isNoisyGameMetadataLabel(label) else { continue }
            let key = label.lowercased()
            guard !seen.contains(key) else { continue }
            seen.insert(key)
            labels.append(label)
        }
    }
    return labels
}

private func isNoisyGameMetadataLabel(_ label: String) -> Bool {
    let normalized = label
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .lowercased()
        .components(separatedBy: CharacterSet.alphanumerics.inverted)
        .filter { !$0.isEmpty }
        .joined()
    return normalized.isEmpty || normalized == "na" || normalized == "n/a" || normalized == "unknown" || normalized == "none"
}

private func normalizedMembershipKey(_ membershipTier: String?) -> String {
    (membershipTier ?? "")
        .uppercased()
        .components(separatedBy: CharacterSet.alphanumerics.inverted)
        .joined()
}

private func membershipTierIsFree(_ membershipTier: String?) -> Bool {
    let normalized = normalizedMembershipKey(membershipTier)
    return normalized.isEmpty || normalized == "FREE" || normalized.contains("FREETIER")
}

private func membershipTierRequiresPaid(_ membershipTier: String?) -> Bool {
    let normalized = normalizedMembershipKey(membershipTier)
    guard !normalized.isEmpty else { return false }
    return normalized.contains("PREMIUM")
        || normalized.contains("PRIORITY")
        || normalized.contains("PERFORMANCE")
        || normalized.contains("FOUNDERS")
        || normalized.contains("ULTIMATE")
        || normalized.contains("RTX3080")
}

typealias TVOSOAuthPresenter = @MainActor @Sendable (_ url: URL, _ callbackScheme: String) async throws -> URL

enum SessionAdAction: String, Codable {
    case start
    case pause
    case resume
    case finish
    case cancel
}

/// Checked continuations trap the process if a framework callback races a synchronous failure.
/// `ASWebAuthenticationSession.start()` can return false while its completion handler is still
/// delivered, so every exit goes through this one-shot gate.
final class OAuthCompletionGate: @unchecked Sendable {
    private let lock = NSLock()
    private var completed = false

    func claim() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !completed else { return false }
        completed = true
        return true
    }
}

private final class OAuthWebAuthenticator: NSObject {
    private var session: ASWebAuthenticationSession?

    func authenticate(
        url: URL,
        callbackScheme: String,
        prefersEphemeralBrowserSession: Bool
    ) async throws -> URL {
        try await withCheckedThrowingContinuation { continuation in
            let completionGate = OAuthCompletionGate()
            let complete: @Sendable (Result<URL, Error>) -> Void = { result in
                guard completionGate.claim() else { return }
                continuation.resume(with: result)
            }
            TVAuthDiagnostics.record(
                "Preparing ASWebAuthenticationSession host=\(url.host ?? "unknown") callbackScheme=\(callbackScheme)"
            )
            let authSession: ASWebAuthenticationSession
            #if os(tvOS)
            authSession = ASWebAuthenticationSession(
                url: url,
                callbackURLScheme: nil
            ) { callbackURL, error in
                if let callbackURL {
                    TVAuthDiagnostics.record("Auth session completed with \(summarizeOAuthCallback(callbackURL))")
                    complete(.success(callbackURL))
                    return
                }
                let authError = error ?? NSError(
                    domain: "OpenNOWAuth", code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "Authentication cancelled"]
                )
                TVAuthDiagnostics.record("Auth session ended with error: \(authError.localizedDescription)")
                complete(.failure(authError))
            }
            #else
            authSession = ASWebAuthenticationSession(
                url: url,
                callbackURLScheme: callbackScheme
            ) { callbackURL, error in
                if let callbackURL {
                    TVAuthDiagnostics.record("Auth session completed with \(summarizeOAuthCallback(callbackURL))")
                    complete(.success(callbackURL))
                    return
                }
                let authError = error ?? NSError(
                    domain: "OpenNOWAuth", code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "Authentication cancelled"]
                )
                TVAuthDiagnostics.record("Auth session ended with error: \(authError.localizedDescription)")
                complete(.failure(authError))
            }
            authSession.presentationContextProvider = self
            authSession.prefersEphemeralWebBrowserSession = prefersEphemeralBrowserSession
            #endif
            self.session = authSession
            TVAuthDiagnostics.record("Starting ASWebAuthenticationSession canStart=\(authSession.canStart).")
            if !authSession.start() {
                TVAuthDiagnostics.record("ASWebAuthenticationSession failed to start.")
                complete(.failure(NSError(
                    domain: "OpenNOWAuth", code: 2,
                    userInfo: [NSLocalizedDescriptionKey: "Could not start sign-in session"])))
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

    func waitForCallback(port: UInt16, timeoutSeconds: TimeInterval = 300) async throws -> URL {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { [weak self] in
                guard let self else {
                    continuation.resume(
                        throwing: NSError(
                            domain: "OpenNOW.Auth",
                            code: 10,
                            userInfo: [NSLocalizedDescriptionKey: "OAuth callback listener was released."]
                        )
                    )
                    return
                }

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

/// Server-side values, chosen when the session is created. They decide which virtual input devices
/// the host sets up, which is why the choice cannot be revisited once the game is running.
///
/// `touchFriendly` is what makes the host present a digitizer. The official client gates its whole
/// touch pipeline on it — `enableTouchInput: appLaunchMode === AppLaunchMode.TouchFriendly` — so a
/// session created under any other mode silently ignores well-formed touch packets.
///
/// Ported from `GfnAppLaunchMode` in the Android build.
enum GFNAppLaunchMode: Int {
    case `default` = 1
    case gamepadFriendly = 2
    case touchFriendly = 3
}

/// Owns the allocation identity and requested input-device envelope together.
///
/// The touch identity is the combination the Android build validated: the
/// desktop-native streamer and client type (`NVIDIA-CLASSIC` / `NATIVE`) with an Android OS and a
/// `TABLET` device type. It requests the host-side digitizer and desktop resolutions, but live
/// iOS tests found its finalized color profile downgraded to 4:2:0 with HDR off. Native 4:4:4
/// therefore keeps the desktop identity. A Windows TABLET touch experiment also downgraded
/// color in build 141. Desktop touch provisioning remains experimental, not a validated host
/// capability. An earlier iOS guess — `IOS` / `MOBILE` / `GFN-MOBILE` plus two
/// invented metadata keys — provisioned no digitizer at all.
enum StreamDeviceProfile: Equatable {
    case desktop
    case touch
    /// Experimental Windows desktop identity with a native digitizer request.
    /// Color delivery and actual host touch support require a live comparison.
    case desktopTouch

    /// A bounded diagnostic of the input envelope requested, not a host capability assertion.
    func recordInputRequest(settings: AppSettings, keyboardMouseConnected: Bool, action: String) {
        #if os(iOS) && canImport(WebRTC)
        #endif
    }

    static func resolve(game: CloudGame, settings: AppSettings,
                        keyboardMouseConnected: Bool, touchProvisionedOverride: Bool? = nil) -> Self {
        // A live 5080h/B40 comparison delivered 4:2:0 and HDR off under the
        // Android touch profile, then 4:4:4 HDR under the desktop profile.
        // Native Windows touch is an explicit experiment; the default keeps the
        // verified desktop 4:4:4 path and finger mouse/controller input.
        if StreamSettingsResolver.requiresDesktopColorProvisioning(for: settings) {
            let touchRequested = touchProvisionedOverride
                ?? NativeTouchSupport.shouldProvisionNativeTouch(mode: settings.touch.nativeTouchMode,
                    game: game, keyboardMouseConnected: keyboardMouseConnected)
            return settings.experimentalDesktop444TouchEnabled && touchRequested ? .desktopTouch : .desktop
        }
        if let touchProvisionedOverride { return touchProvisionedOverride ? .touch : .desktop }
        return NativeTouchSupport.shouldProvisionNativeTouch(mode: settings.touch.nativeTouchMode,
            game: game, keyboardMouseConnected: keyboardMouseConnected) ? .touch : .desktop
    }

    var appLaunchMode: GFNAppLaunchMode {
        switch self {
        case .desktop: return .gamepadFriendly
        case .touch, .desktopTouch: return .touchFriendly
        }
    }

    var remoteControllersBitmap: Int {
        self == .desktop ? 1 : 0
    }

    var availableSupportedControllers: [Int] {
        self == .desktop ? [2] : []
    }

    var nvDeviceOS: String {
        switch self {
        case .desktop, .desktopTouch: return "WINDOWS"
        case .touch: return "ANDROID"
        }
    }

    var nvDeviceType: String {
        switch self {
        case .desktop, .desktopTouch: return "DESKTOP"
        case .touch: return "TABLET"
        }
    }

    /// The touch identity has to look like the client whose user agent it borrows, so make and
    /// model go back to that identity's own values rather than naming the iPhone.
    var nvDeviceMake: String {
        switch self {
        case .desktop, .desktopTouch: return "APPLE"
        case .touch: return "UNKNOWN"
        }
    }

    var nvDeviceModel: String {
        switch self {
        case .desktop, .desktopTouch: return OpenNOWPlatform.displayName
        case .touch: return "UNKNOWN"
        }
    }

    var userAgent: String {
        switch self {
        case .desktop, .desktopTouch: return GFNConstants.userAgent
        case .touch: return GFNConstants.touchUserAgent
        }
    }

    var clientPlatformName: String {
        switch self {
        case .desktop, .desktopTouch: return "windows"
        case .touch: return "android"
        }
    }

    /// Only the desktop identity persists in-game settings server-side. The touch identity is a
    /// borrowed one and must not write a phone session's choices back over the desktop profile.
    var persistsInGameSettings: Bool {
        switch self {
        case .desktop, .desktopTouch: return true
        case .touch: return false
        }
    }

    /// Both identities present themselves as the PC client; only the device fields differ. Sending
    /// `GFN-MOBILE` here was part of the same guess that failed.
    var clientIdentification: String { "GFN-PC" }
}

// Keep the launch/resume feature envelope aligned with the desktop CloudMatch client.
// Request HDR explicitly in addition to monitor/display data; the desktop has no HDR toggle.
enum CloudMatchStreamingFeatureRequest {
    static func normalizedColorQuality(bitDepth: Int?, chromaFormat: Int?) -> StreamColorQuality? {
        guard let bitDepth, [0, 1, 8, 10].contains(bitDepth) else { return nil }
        let tenBit = bitDepth == 1 || bitDepth == 10
        let fullChroma = chromaFormat == 1 || chromaFormat == 2
        switch (tenBit, fullChroma) {
        case (true, true): return .tenBit444
        case (true, false): return .tenBit420
        case (false, true): return .eightBit444
        case (false, false): return .eightBit420
        }
    }

    static func build(settings: AppSettings, profile: StreamVideoProfile,
                      bitDepth: Int, chromaFormat: Int) -> [String: Any] {
        [
            "reflex": settings.enableCloudGsync || profile.fps >= 120,
            // CloudMatch uses enums, unlike the literal bit count in the NVST SDP.
            "bitDepth": bitDepth == 10 ? 1 : 0,
            "trueHdr": settings.hdrEnabled,
            "cloudGsync": settings.enableCloudGsync,
            "enabledL4S": settings.enableL4S,
            "supportedHidDevices": 0,
            "profile": 0,
            "fallbackToLogicalResolution": false,
            "chromaFormat": chromaFormat == 2 ? 1 : 0,
            "prefilterMode": 0,
            "prefilterSharpness": 0,
            "prefilterNoiseReduction": 0,
            "hudStreamingMode": 0
        ]
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
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 30
        return URLSession(configuration: config)
    }()
    private let logger = Logger(subsystem: "OpenNOWiOS", category: "GFNAPI")

    private func request(
        url: URL,
        method: String = "GET",
        headers: [String: String],
        body: Data? = nil,
        sessionSettings: AppSettings? = nil,
        timeoutInterval: TimeInterval? = nil
    ) async throws -> (Data, HTTPURLResponse) {
        var req = URLRequest(url: url)
        req.httpMethod = method
        req.httpBody = body
        if let timeoutInterval {
            req.timeoutInterval = max(0.1, timeoutInterval)
        }
        for (k, v) in headers {
            req.setValue(v, forHTTPHeaderField: k)
        }
        let urlSession = Self.shouldUseSessionProxy(for: url, settings: sessionSettings)
            ? proxiedSession(settings: sessionSettings)
            : self.session
        let (data, response) = try await DiagnosticsHTTPRecorder.data(
            for: req,
            using: urlSession,
            source: "GFNAPI"
        )
        guard let http = response as? HTTPURLResponse else {
            throw NSError(domain: "OpenNOW.Network", code: -1, userInfo: [NSLocalizedDescriptionKey: "Invalid response"])
        }
        return (data, http)
    }

    private static func shouldUseSessionProxy(for url: URL, settings: AppSettings?) -> Bool {
        guard let settings,
              settings.sessionProxyEnabled,
              !settings.sessionProxyUrl.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              let host = url.host else {
            return false
        }
        return isZoneHostname(host)
    }

    private func proxiedSession(settings: AppSettings?) -> URLSession {
        guard let settings,
              let proxyURL = URL(string: settings.sessionProxyUrl.trimmingCharacters(in: .whitespacesAndNewlines)),
              let proxyHost = proxyURL.host else {
            return session
        }
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 30
        let scheme = (proxyURL.scheme ?? "http").lowercased()
        let port = proxyURL.port ?? (scheme == "https" ? 443 : 80)
        config.connectionProxyDictionary = [
            "HTTPEnable": true,
            "HTTPProxy": proxyHost,
            "HTTPPort": port,
            "HTTPSEnable": true,
            "HTTPSProxy": proxyHost,
            "HTTPSPort": port
        ]
        return URLSession(configuration: config)
    }

    private func parseJSON(_ data: Data) throws -> [String: Any] {
        guard let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw NSError(domain: "OpenNOW.Network", code: -2, userInfo: [NSLocalizedDescriptionKey: "Invalid JSON object"])
        }
        return obj
    }

    private func responseError(
        domain: String,
        response: HTTPURLResponse,
        data: Data,
        fallback: String
    ) -> NSError {
        let rawBody = String(data: data, encoding: .utf8) ?? ""
        let rawError = NSError(
            domain: domain,
            code: response.statusCode,
            userInfo: [NSLocalizedDescriptionKey: rawBody.isEmpty ? fallback : rawBody]
        )
        return NSError(
            domain: domain,
            code: response.statusCode,
            userInfo: [NSLocalizedDescriptionKey: OpenNOWErrorPresenter.message(for: rawError, fallback: fallback)]
        )
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
                headers: headersForOAuth()
            )
            guard (200..<300).contains(response.statusCode) else { return [GFNConstants.defaultProvider] }
            let json = try parseJSON(data)
            let gfnInfo = json["gfnServiceInfo"] as? [String: Any]
            let endpoints = gfnInfo?["gfnServiceEndpoints"] as? [[String: Any]] ?? []
            let providers = endpoints.compactMap { item -> LoginProvider? in
                guard let idp = item["idpId"] as? String, !idp.isEmpty,
                      let rawURL = item["streamingServiceUrl"] as? String,
                      let parsedURL = URLComponents(string: rawURL.trimmingCharacters(in: .whitespacesAndNewlines)),
                      parsedURL.scheme?.lowercased() == "https",
                      let host = parsedURL.host, !host.isEmpty,
                      !host.hasPrefix("."), !host.contains("..") else {
                    return nil
                }
                let rawCode = (item["loginProviderCode"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                let code = rawCode.isEmpty ? "NVIDIA" : rawCode
                let rawName = (item["loginProviderDisplayName"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                let name = rawName.isEmpty ? code : rawName
                let prio = item["loginProviderPriority"] as? Int ?? 0
                let display = (code == "BPC") ? "bro.game" : name
                let port = parsedURL.port.map { $0 == 443 ? "" : ":\($0)" } ?? ""
                let normalizedURL = "https://\(host)\(port)/"
                return LoginProvider(idpId: idp, code: code, displayName: display, streamingServiceUrl: normalizedURL, priority: prio)
            }.sorted { $0.priority == $1.priority ? $0.displayName < $1.displayName : $0.priority < $1.priority }
            return providers.isEmpty ? [GFNConstants.defaultProvider] : providers
        } catch {
            return [GFNConstants.defaultProvider]
        }
    }

    func login(
        with provider: LoginProvider,
        deviceId: String,
        forceAccountSelection: Bool = false
    ) async throws -> AuthSession {
        let pending = makePendingOAuthLogin(with: provider, deviceId: deviceId)
        let callbackServer = OAuthLoopbackServer()
        let callbackURL: URL
        do {
            #if os(tvOS)
            let callbackResult = try await awaitTVOSOAuthCallback(
                authUrl: pending.authURL,
                callbackServer: callbackServer,
                authenticate: { url, callbackScheme in
                    try await Self.performOAuthSession(
                        url: url,
                        callbackScheme: callbackScheme,
                        prefersEphemeralBrowserSession: forceAccountSelection
                    )
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
                callbackScheme: GFNConstants.oauthCallbackScheme,
                prefersEphemeralBrowserSession: forceAccountSelection
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

    func loginWithDeviceCode(
        with provider: LoginProvider,
        deviceId: String,
        onPrompt: @escaping @MainActor @Sendable (DeviceLoginPrompt) -> Void,
        onAuthorized: @escaping @MainActor @Sendable () -> Void
    ) async throws -> AuthSession {
        guard provider.code.uppercased() == "NVIDIA" else {
            throw NSError(domain: "OpenNOW.Auth", code: 20, userInfo: [
                NSLocalizedDescriptionKey: "Code sign-in is only available for NVIDIA accounts."
            ])
        }
        let challengeBody = URLQueryItemEncoder.encode([
            "client_id": GFNConstants.deviceCodeClientId,
            "scope": GFNConstants.scopes,
            "device_id": deviceId,
            "display_name": "OpenNOW iOS",
            "idp_id": provider.idpId
        ])
        let formHeaders = [
            "Accept": "application/json, text/plain, */*",
            "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
            "User-Agent": GFNConstants.userAgent
        ]
        let (challengeData, challengeResponse) = try await request(
            url: GFNConstants.deviceAuthorizationEndpoint,
            method: "POST",
            headers: formHeaders,
            body: challengeBody.data(using: .utf8)
        )
        guard (200..<300).contains(challengeResponse.statusCode) else {
            throw responseError(domain: "OpenNOW.Auth", response: challengeResponse, data: challengeData,
                fallback: "NVIDIA could not start code sign-in.")
        }
        let challenge = try parseJSON(challengeData)
        guard let deviceCode = challenge["device_code"] as? String, !deviceCode.isEmpty,
              let userCode = challenge["user_code"] as? String, !userCode.isEmpty,
              let verificationString = (challenge["verification_uri_complete"] as? String)
                ?? (challenge["verification_uri"] as? String)
                ?? (challenge["verification_url"] as? String),
              let verificationURL = URL(string: verificationString),
              verificationURL.scheme == "https" else {
            throw NSError(domain: "OpenNOW.Auth", code: 21, userInfo: [
                NSLocalizedDescriptionKey: "NVIDIA returned an incomplete sign-in code."
            ])
        }
        let expiresIn = max(1, (challenge["expires_in"] as? NSNumber)?.doubleValue ?? 600)
        let expiresAt = Date().addingTimeInterval(expiresIn)
        let prompt = DeviceLoginPrompt(userCode: userCode, verificationURL: verificationURL, expiresAt: expiresAt)
        await onPrompt(prompt)
        var interval = max(5, (challenge["interval"] as? NSNumber)?.intValue ?? 5)
        logger.info("Device code issued; waiting for user authorization.")

        while Date() < expiresAt {
            try Task.checkCancellation()
            try await Task.sleep(for: .seconds(interval))
            try Task.checkCancellation()
            let tokenBody = URLQueryItemEncoder.encode([
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": deviceCode,
                "client_id": GFNConstants.deviceCodeClientId
            ])
            let (data, response) = try await request(
                url: GFNConstants.tokenEndpoint,
                method: "POST",
                headers: formHeaders,
                body: tokenBody.data(using: .utf8)
            )
            let result = (try? parseJSON(data)) ?? [:]
            if (200..<300).contains(response.statusCode) {
                guard let accessToken = result["access_token"] as? String, !accessToken.isEmpty else {
                    throw NSError(domain: "OpenNOW.Auth", code: 22, userInfo: [
                        NSLocalizedDescriptionKey: "NVIDIA did not return an access token."
                    ])
                }
                var tokens = AuthTokens(
                    accessToken: accessToken,
                    refreshToken: result["refresh_token"] as? String,
                    idToken: result["id_token"] as? String,
                    expiresAt: Date().addingTimeInterval((result["expires_in"] as? NSNumber)?.doubleValue ?? 86400).timeIntervalSince1970,
                    clientToken: result["client_token"] as? String,
                    clientTokenExpiresAt: nil
                )
                tokens.authClientId = GFNConstants.deviceCodeClientId
                logger.info("Device code authorized; exchanging sign-in tokens.")
                await onAuthorized()
                return try await buildAuthSession(provider: provider, tokens: tokens)
            }
            switch result["error"] as? String {
            case "authorization_pending": continue
            case "slow_down": interval += 5
            case "access_denied":
                throw NSError(domain: "OpenNOW.Auth", code: 23, userInfo: [NSLocalizedDescriptionKey: "Device sign-in was cancelled."])
            case "expired_token":
                break
            default:
                throw responseError(domain: "OpenNOW.Auth", response: response, data: data,
                    fallback: "NVIDIA could not finish code sign-in.")
            }
            if result["error"] as? String == "expired_token" { break }
        }
        throw NSError(domain: "OpenNOW.Auth", code: 24, userInfo: [NSLocalizedDescriptionKey: "Device sign-in code expired."])
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
            body: tokenBody.data(using: String.Encoding.utf8),
            timeoutInterval: 90
        )
        TVAuthDiagnostics.record("Token exchange returned HTTP \(tokenResponse.statusCode).")
        guard tokenResponse.statusCode == 200 else {
            throw responseError(
                domain: "OpenNOW.Auth",
                response: tokenResponse,
                data: tokenData,
                fallback: "NVIDIA could not finish the token exchange."
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
        tokens.authClientId = GFNConstants.clientId

        return try await buildAuthSession(provider: pending.provider, tokens: tokens)
    }

    private func buildAuthSession(provider: LoginProvider, tokens initialTokens: AuthTokens) async throws -> AuthSession {
        var tokens = initialTokens
        TVAuthDiagnostics.record("Loading NVIDIA user profile.")
        var user = try await fetchUser(tokens: tokens)
        TVAuthDiagnostics.record("User profile loaded.")
        if let freshClientToken = try? await requestClientToken(accessToken: tokens.accessToken),
           !freshClientToken.token.isEmpty {
            let authClientId = tokens.authClientId
            tokens = AuthTokens(
                accessToken: tokens.accessToken,
                refreshToken: tokens.refreshToken,
                idToken: tokens.idToken,
                expiresAt: tokens.expiresAt,
                clientToken: freshClientToken.token,
                clientTokenExpiresAt: freshClientToken.expiresAt
            )
            tokens.authClientId = authClientId
            TVAuthDiagnostics.record("Client token refreshed.")
        }
        if let tier = try? await fetchMembershipTier(
            token: tokens.idToken ?? tokens.accessToken,
            userId: user.userId,
            streamingBaseUrl: provider.streamingServiceUrl
        ) {
            user.membershipTier = tier
            TVAuthDiagnostics.record("Membership tier loaded.")
        }

        TVAuthDiagnostics.record("Sign-in finished successfully.")
        return AuthSession(provider: provider, tokens: tokens, user: user)
    }

    @MainActor
    private static func performOAuthSession(
        url: URL,
        callbackScheme: String,
        prefersEphemeralBrowserSession: Bool = false
    ) async throws -> URL {
        let authenticator = OAuthWebAuthenticator()
        return try await authenticator.authenticate(
            url: url,
            callbackScheme: callbackScheme,
            prefersEphemeralBrowserSession: prefersEphemeralBrowserSession
        )
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

        if let clientToken = session.tokens.clientToken, !clientToken.isEmpty,
           session.tokens.clientTokenExpiresAt.map({ nowEpoch < $0 - (5 * 60) }) ?? true {
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
            "client_id": existing.tokens.authClientId ?? GFNConstants.clientId,
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
            throw responseError(
                domain: "OpenNOW.Auth",
                response: response,
                data: data,
                fallback: "The saved account session could not be refreshed."
            )
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
        var retainedTokens = newTokens
        retainedTokens.authClientId = existing.tokens.authClientId
        let user = (try? await fetchUser(tokens: retainedTokens)) ?? existing.user
        return AuthSession(provider: existing.provider, tokens: retainedTokens, user: user)
    }

    private func refreshWithOAuthToken(_ refreshToken: String, existing: AuthSession) async throws -> AuthSession {
        let body = URLQueryItemEncoder.encode([
            "grant_type": "refresh_token",
            "refresh_token": refreshToken,
            "client_id": existing.tokens.authClientId ?? GFNConstants.clientId
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
            throw responseError(
                domain: "OpenNOW.Auth",
                response: response,
                data: data,
                fallback: "The saved account session could not be refreshed."
            )
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
        var retainedTokens = newTokens
        retainedTokens.authClientId = existing.tokens.authClientId
        let user = (try? await fetchUser(tokens: retainedTokens)) ?? existing.user
        return AuthSession(provider: existing.provider, tokens: retainedTokens, user: user)
    }

    func fetchMainGames(session: AuthSession) async throws -> ([CloudGame], String, [StreamRegion]) {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let serverInfo = try await fetchServerInfo(token: token, streamingBaseUrl: session.provider.streamingServiceUrl)
        let vpcId = serverInfo.vpcId ?? "GFN-PC"
        let payload = try await fetchPanels(token: token, panelNames: ["MAIN"], vpcId: vpcId)
        let games = Self.flattenPanels(payload: payload)
        let enrichedGames = (try? await enrichGamesWithMetadata(token: token, vpcId: vpcId, games: games)) ?? games
        return (enrichedGames, vpcId, serverInfo.regions)
    }

    func fetchRegions(session: AuthSession) async throws -> [StreamRegion] {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        return try await fetchServerInfo(token: token, streamingBaseUrl: session.provider.streamingServiceUrl).regions
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
            throw responseError(
                domain: "OpenNOW.Subscription",
                response: response,
                data: data,
                fallback: "Subscription details could not be refreshed."
            )
        }
        let json = try parseJSON(data)
        let tier = (json["membershipTier"] as? String) ?? session.user.membershipTier
        let remaining = (json["remainingTimeInMinutes"] as? Double ?? 0) / 60.0
        let total = (json["totalTimeInMinutes"] as? Double ?? 0) / 60.0
        let state = json["currentSubscriptionState"] as? [String: Any]
        let allowed = state?["isGamePlayAllowed"] as? Bool ?? true
        let subscription = (json["subscription"] as? [String: Any]) ?? json
        let subscriptionType = json["type"] as? String
        let subscriptionSubType = json["subType"] as? String
        let isUnlimited = subscriptionSubType == "UNLIMITED"
        let storageAddon = ((subscription["addons"] as? [[String: Any]]) ?? [])
            .first(where: Self.isActivePersistentStorageAddon)
            .map(Self.parseStorageAddon)
        return SubscriptionSnapshot(
            membershipTier: tier,
            subscriptionType: subscriptionType,
            subscriptionSubType: subscriptionSubType,
            isGamePlayAllowed: allowed,
            isUnlimited: isUnlimited,
            remainingHours: remaining,
            totalHours: total,
            storageAddon: storageAddon
        )
    }

    private static func parseStorageAddon(_ addon: [String: Any]) -> StorageAddon {
        let attributes = ((addon["attributes"] as? [[String: Any]]) ?? [])
            .reduce(into: [String: String]()) { result, attribute in
                guard let key = attribute["key"] as? String else { return }
                if let text = attribute["textValue"] as? String {
                    result[key] = text
                } else if let number = attribute["numberValue"] {
                    result[key] = "\(number)"
                }
            }
        return StorageAddon(
            type: (addon["type"] as? String) ?? GFNConstants.storageAddonType,
            sizeGb: Double(attributes[GFNConstants.totalStorageSizeAttribute] ?? ""),
            usedGb: Double(attributes[GFNConstants.usedStorageSizeAttribute] ?? ""),
            regionName: attributes[GFNConstants.storageMetroRegionNameAttribute],
            regionCode: attributes[GFNConstants.storageMetroRegionAttribute],
            status: addon["status"] as? String,
            subType: addon["subType"] as? String,
            autoPayEnabled: addon["autoPayEnabled"] as? Bool
        )
    }

    private static func isActivePersistentStorageAddon(_ addon: [String: Any]) -> Bool {
        (addon["type"] as? String) == GFNConstants.storageAddonType &&
            (addon["subType"] as? String) == GFNConstants.storageAddonSubType &&
            (addon["status"] as? String) == "OK"
    }

    func fetchAccountConnectors(session: AuthSession) async throws -> [AccountConnector] {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let query = """
        query GetAccountConnectors($locale: String!, $stringsKey: [String]!) {
          appStoreDefinitions(language: $locale) {
            store
            label
            sortOrder
            features {
              __typename
              ... on AccountLinkingSso {
                supported
              }
              ... on AccountGamesSyncing {
                supported
              }
            }
            accountLinkingMetadata {
              isSupported
              isRequired
              label
            }
          }
          userAccount {
            storesData {
              store
              accountLinkingData {
                userDisplayName
                expiresIn
                userIdentifier
                accountSyncingData {
                  totalNumberOfSyncedGfnGames
                  syncState
                  syncDate
                }
              }
            }
          }
          clientStrings(language: $locale, keys: $stringsKey)
        }
        """
        let payload: [String: Any] = [
            "query": query,
            "variables": [
                "locale": "en_US",
                "stringsKey": []
            ] as [String: Any]
        ]
        let body = try JSONSerialization.data(withJSONObject: payload)
        let (data, response) = try await request(
            url: URL(string: GFNConstants.graphQL)!,
            method: "POST",
            headers: Self.desktopGraphQLHeaders(token: token, contentType: "application/json; charset=utf-8"),
            body: body
        )
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.AccountConnectors", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
        let json = try parseJSON(data)
        if let errors = json["errors"] as? [[String: Any]], !errors.isEmpty {
            let message = errors.compactMap { $0["message"] as? String }.joined(separator: "\n")
            throw NSError(
                domain: "OpenNOW.AccountConnectors",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: message.isEmpty ? "Account connector query failed" : message]
            )
        }
        let dataObject = json["data"] as? [String: Any] ?? [:]
        let userStores = (((dataObject["userAccount"] as? [String: Any])?["storesData"] as? [[String: Any]]) ?? [])
            .reduce(into: [String: [String: Any]]()) { result, store in
                guard let storeId = store["store"] as? String else { return }
                result[normalizeGameStore(storeId)] = store
            }
        var connectors = (((dataObject["appStoreDefinitions"] as? [[String: Any]]) ?? []).compactMap { raw -> AccountConnector? in
            guard let storeId = raw["store"] as? String, !storeId.isEmpty else { return nil }
            let metadata = raw["accountLinkingMetadata"] as? [String: Any]
            let featureSupported = ((raw["features"] as? [[String: Any]]) ?? []).contains { feature in
                (feature["supported"] as? Bool) == true &&
                    ((feature["__typename"] as? String) == "AccountLinkingSso" ||
                     (feature["__typename"] as? String) == "AccountGamesSyncing")
            }
            let supported = (metadata?["isSupported"] as? Bool) == true || featureSupported
            let normalizedStoreId = normalizeGameStore(storeId)
            guard supported || userStores[normalizedStoreId] != nil else { return nil }
            let linked = userStores[normalizedStoreId]?["accountLinkingData"] as? [String: Any]
            let sync = linked?["accountSyncingData"] as? [String: Any]
            return AccountConnector(
                store: storeId,
                label: (metadata?["label"] as? String) ?? (raw["label"] as? String) ?? gameStoreDisplayName(storeId),
                supported: supported,
                required: metadata?["isRequired"] as? Bool ?? false,
                userDisplayName: linked?["userDisplayName"] as? String,
                userIdentifier: linked?["userIdentifier"] as? String,
                expiresInSeconds: Self.toPositiveInt(linked?["expiresIn"]),
                syncedGameCount: Self.toPositiveInt(sync?["totalNumberOfSyncedGfnGames"]),
                syncState: sync?["syncState"] as? String,
                syncDate: sync?["syncDate"] as? String
            )
        })
        if !connectors.contains(where: { normalizeGameStore($0.store) == "STEAM" }) {
            let linked = userStores["STEAM"]?["accountLinkingData"] as? [String: Any]
            let sync = linked?["accountSyncingData"] as? [String: Any]
            connectors.append(
                AccountConnector(
                    store: "STEAM",
                    label: "Steam",
                    supported: true,
                    required: false,
                    userDisplayName: linked?["userDisplayName"] as? String,
                    userIdentifier: linked?["userIdentifier"] as? String,
                    expiresInSeconds: Self.toPositiveInt(linked?["expiresIn"]),
                    syncedGameCount: Self.toPositiveInt(sync?["totalNumberOfSyncedGfnGames"]),
                    syncState: sync?["syncState"] as? String,
                    syncDate: sync?["syncDate"] as? String
                )
            )
        }
        return connectors.sorted {
            if $0.isLinked != $1.isLinked { return $0.isLinked && !$1.isLinked }
            let lhsRank = Self.accountConnectorSortRank($0.store)
            let rhsRank = Self.accountConnectorSortRank($1.store)
            if lhsRank != rhsRank { return lhsRank < rhsRank }
            return $0.label.localizedCaseInsensitiveCompare($1.label) == .orderedAscending
        }
    }

    func accountConnectorLoginURL(store: String, session: AuthSession) async throws -> URL {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let platform = Self.accountLinkingPlatform(store)
        var components = URLComponents(url: GFNConstants.accountLinkingBase.appendingPathComponent("login_url"), resolvingAgainstBaseURL: false)!
        components.queryItems = [
            .init(name: "platform", value: platform),
            .init(name: "redirect_uri", value: GFNConstants.accountLinkingRedirectUri),
            .init(name: "client_id", value: GFNConstants.accountLinkingClientId)
        ]
        let (data, response) = try await request(
            url: components.url!,
            headers: Self.accountLinkingHeaders(token: token)
        )
        guard response.statusCode >= 200 && response.statusCode < 300 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.AccountConnectors", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
        let json = try parseJSON(data)
        guard let rawURL = json["login_url"] as? String, let url = URL(string: rawURL) else {
            throw NSError(domain: "OpenNOW.AccountConnectors", code: 2, userInfo: [NSLocalizedDescriptionKey: "Store connection did not return a login URL"])
        }
        return url
    }

    func disconnectAccountConnector(store: String, session: AuthSession) async throws {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let platform = Self.accountLinkingPlatform(store)
        let url = GFNConstants.accountLinkingBase
            .appendingPathComponent("linking")
            .appendingPathComponent(platform)
        let (data, response) = try await request(
            url: url,
            method: "DELETE",
            headers: Self.accountLinkingHeaders(token: token)
        )
        guard response.statusCode >= 200 && response.statusCode < 300 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.AccountConnectors", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
    }

    private static func desktopGraphQLHeaders(token: String, contentType: String) -> [String: String] {
        [
            "Accept": "application/json, text/plain, */*",
            "Content-Type": contentType,
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
    }

    private static func accountLinkingHeaders(token: String) -> [String: String] {
        [
            "Accept": "application/json, text/plain, */*",
            "Authorization": "Bearer \(token)",
            "Origin": "https://play.geforcenow.com",
            "Referer": "https://play.geforcenow.com/",
            "User-Agent": GFNConstants.userAgent
        ]
    }

    private static func accountLinkingPlatform(_ store: String) -> String {
        switch normalizeGameStore(store) {
        case "UBISOFT", "UBISOFT_CONNECT":
            return "UPLAY"
        case "BATTLE_NET", "BLIZZARD":
            return "BATTLENET"
        case "EPIC_GAMES", "EPIC_GAMES_STORE":
            return "EPIC"
        default:
            return normalizeGameStore(store).isEmpty ? store.uppercased() : normalizeGameStore(store)
        }
    }

    private static func accountConnectorSortRank(_ store: String) -> Int {
        switch normalizeGameStore(store) {
        case "STEAM":
            return 0
        case "EPIC", "EGS", "EPIC_GAMES_STORE":
            return 1
        case "XBOX", "XBOX_GAME_PASS", "GAME_PASS":
            return 2
        case "UBISOFT", "UBISOFT_CONNECT":
            return 3
        default:
            return 10
        }
    }

    func startSession(
        session: AuthSession,
        game: CloudGame,
        vpcId: String,
        settings: AppSettings,
        streamProfile: StreamVideoProfile,
        streamingBaseUrl: String? = nil,
        launchAppIdOverride: String? = nil,
        launcherName: String = "Auto",
        deviceId: String,
        accountLinked: Bool = true
    ) async throws -> ActiveSession {
        let resolvedLaunchAppId = launchAppIdOverride ?? game.launchAppId
        guard let launchAppId = resolvedLaunchAppId, !launchAppId.isEmpty else {
            throw NSError(domain: "OpenNOW.Session", code: 30, userInfo: [NSLocalizedDescriptionKey: "Selected game has no launch app ID"])
        }
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let baseSource = streamingBaseUrl ?? session.provider.streamingServiceUrl
        let base = baseSource.hasSuffix("/") ? String(baseSource.dropLast()) : baseSource
        let physicalInputConnected = NativeStreamPhysicalInput.keyboardOrMouseConnected
        let deviceProfile = StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: physicalInputConnected)
        deviceProfile.recordInputRequest(settings: settings, keyboardMouseConnected: physicalInputConnected,
            action: "create")
        let sessionQuery = URLQueryItemEncoder.encode([
            "keyboardLayout": StreamSettingsResolver.normalizedKeyboardLayout(settings.keyboardLayout),
            "languageCode": StreamSettingsResolver.normalizedGameLanguage(settings.gameLanguage)
        ])
        let url = URL(string: "\(base)/v2/session?\(sessionQuery)")!
        let body = Self.buildSessionBody(
            appId: launchAppId,
            title: game.title,
            settings: settings,
            profile: streamProfile,
            launcherName: launcherName,
            deviceProfile: deviceProfile,
            deviceHashId: deviceId,
            accountLinked: accountLinked
        )
        Logger(subsystem: "OpenNOWiOS", category: "GFNAPI").info(
            "Session create profile=\(streamProfile.width)x\(streamProfile.height) fps=\(streamProfile.fps) codec=\(settings.preferredCodec, privacy: .public) color=\(settings.preferredColorQuality, privacy: .public) hdr=\(settings.hdrEnabled) identity=\(deviceProfile.clientPlatformName, privacy: .public)"
        )
        let clientId = UUID().uuidString
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
            body: body,
            sessionSettings: settings
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
        let queue = Self.extractQueuePosition(sessionObj: sessionObj)
        let seatSetupStep = Self.extractSeatSetupStep(sessionObj: sessionObj)
        let control = sessionObj["sessionControlInfo"] as? [String: Any]
        let controlBase = SessionControlRouting.baseURL(host: control?["ip"] as? String, port: control?["port"] as? Int)
        let serverIp = Self.extractServerIp(sessionObj: sessionObj) ?? normalizedEndpointHost(from: control?["ip"])
        let mediaConnectionInfo = Self.extractMediaConnectionInfo(sessionObj: sessionObj)
        let signaling = Self.resolveSignaling(sessionObj: sessionObj, fallbackServerIp: serverIp)
        let iceServers = Self.extractIceServers(sessionObj: sessionObj)
        let adState = Self.extractAdState(sessionObj: sessionObj)
        var active = ActiveSession(
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
            adState: adState,
            gpuType: sessionObj["gpuType"] as? String,
            negotiatedStreamProfile: Self.extractNegotiatedStreamProfile(sessionObj: sessionObj),
            requestedStreamingFeatures: Self.extractStreamingFeatures(
                (sessionObj["sessionRequestData"] as? [String: Any])?["requestedStreamingFeatures"] as? [String: Any]
            ),
            finalizedStreamingFeatures: Self.extractStreamingFeatures(sessionObj["finalizedStreamingFeatures"] as? [String: Any]),
            touchProvisioned: deviceProfile.appLaunchMode == .touchFriendly,
            nativeRtspsEndpoints: NativeStreamNVSTConfiguration.endpoints(sessionObj: sessionObj, fallbackHost: serverIp)
        )
        active.sessionControlBaseUrl = controlBase
        return active
    }

    func pollSession(
        session: AuthSession,
        activeSession: ActiveSession,
        settings: AppSettings? = nil
    ) async throws -> ActiveSession {
        let primaryBase = SessionControlRouting.pollBase(for: activeSession)
        do {
            return try await pollSession(session: session, activeSession: activeSession, base: primaryBase, settings: settings)
        } catch {
            // A control host can become unavailable. The canonical zone remains a safe fallback.
            // Terminal provider queue errors must not be retried through another host.
            guard primaryBase != activeSession.streamingBaseUrl,
                  (error as NSError).domain != "OpenNOW.Queue",
                  !(error is CancellationError) else {
                throw error
            }
            return try await pollSession(session: session, activeSession: activeSession, base: activeSession.streamingBaseUrl, settings: settings)
        }
    }

    private func pollSession(
        session: AuthSession,
        activeSession: ActiveSession,
        base: String,
        settings: AppSettings?
    ) async throws -> ActiveSession {
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
            ),
            sessionSettings: settings
        )
        let parsedResponse = try? parseJSON(data)
        let requestStatus = parsedResponse?["requestStatus"] as? [String: Any]
        if CloudMatchQueueStatus.isAbandoned(requestStatus) {
            throw NSError(domain: "OpenNOW.Queue", code: 69, userInfo: [
                NSLocalizedDescriptionKey: "The cloud provider ended this queue request. Start the game again to join a new queue."
            ])
        }
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.Session", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }

        let json = try parsedResponse ?? parseJSON(data)
        let statusCode = requestStatus?["statusCode"] as? Int ?? 0
        guard statusCode == 1 else {
            let description = requestStatus?["statusDescription"] as? String ?? "Session poll failed"
            throw NSError(domain: "OpenNOW.Session", code: statusCode, userInfo: [NSLocalizedDescriptionKey: description])
        }

        let sessionObj = json["session"] as? [String: Any] ?? [:]
        let status = sessionObj["status"] as? Int ?? activeSession.status
        let queue = Self.extractQueuePosition(sessionObj: sessionObj)
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
        let control = sessionObj["sessionControlInfo"] as? [String: Any]
        updated.sessionControlBaseUrl = SessionControlRouting.baseURL(
            host: control?["ip"] as? String,
            port: control?["port"] as? Int
        )
        updated.mediaIp = mediaConnectionInfo.ip ?? updated.mediaIp
        updated.mediaPort = mediaConnectionInfo.port > 0 ? mediaConnectionInfo.port : updated.mediaPort
        let nativeEndpoints = NativeStreamNVSTConfiguration.endpoints(sessionObj: sessionObj, fallbackHost: serverIp)
        if !nativeEndpoints.isEmpty { updated.nativeRtspsEndpoints = nativeEndpoints }
        updated.signalingServer = signaling.server ?? updated.signalingServer
        updated.signalingUrl = signaling.url ?? updated.signalingUrl
        if !iceServers.isEmpty {
            updated.iceServers = iceServers
        }
        updated.adState = adState
        updated.gpuType = sessionObj["gpuType"] as? String ?? updated.gpuType
        updated.negotiatedStreamProfile = Self.extractNegotiatedStreamProfile(sessionObj: sessionObj) ?? updated.negotiatedStreamProfile
        updated.requestedStreamingFeatures = Self.extractStreamingFeatures(
            (sessionObj["sessionRequestData"] as? [String: Any])?["requestedStreamingFeatures"] as? [String: Any]
        ) ?? updated.requestedStreamingFeatures
        updated.finalizedStreamingFeatures = Self.extractStreamingFeatures(sessionObj["finalizedStreamingFeatures"] as? [String: Any]) ?? updated.finalizedStreamingFeatures
        return mergeQueueSessionState(previous: activeSession, next: updated)
    }

    func reportSessionAd(
        session: AuthSession,
        activeSession: ActiveSession,
        adId: String,
        action: SessionAdAction,
        watchedTimeInMs: Int? = nil,
        pausedTimeInMs: Int? = nil,
        cancelReason: String? = nil,
        errorInfo: String? = nil,
        settings: AppSettings? = nil
    ) async throws -> ActiveSession {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let base = SessionControlRouting.pollBase(for: activeSession)
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
            body: body,
            sessionSettings: settings
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
        updated.queuePosition = Self.extractQueuePosition(sessionObj: sessionObj) ?? updated.queuePosition
        updated.seatSetupStep = Self.extractSeatSetupStep(sessionObj: sessionObj) ?? updated.seatSetupStep
        let adControl = sessionObj["sessionControlInfo"] as? [String: Any]
        updated.sessionControlBaseUrl = SessionControlRouting.baseURL(
            host: adControl?["ip"] as? String, port: adControl?["port"] as? Int
        ) ?? updated.sessionControlBaseUrl
        updated.adState = Self.extractAdState(sessionObj: sessionObj)
        updated.gpuType = sessionObj["gpuType"] as? String ?? updated.gpuType
        updated.negotiatedStreamProfile = Self.extractNegotiatedStreamProfile(sessionObj: sessionObj) ?? updated.negotiatedStreamProfile
        updated.requestedStreamingFeatures = Self.extractStreamingFeatures(
            (sessionObj["sessionRequestData"] as? [String: Any])?["requestedStreamingFeatures"] as? [String: Any]
        ) ?? updated.requestedStreamingFeatures
        updated.finalizedStreamingFeatures = Self.extractStreamingFeatures(sessionObj["finalizedStreamingFeatures"] as? [String: Any]) ?? updated.finalizedStreamingFeatures
        return updated
    }

    func stopSession(session: AuthSession, activeSession: ActiveSession) async throws {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let base = SessionControlRouting.pollBase(for: activeSession)
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
        vpcId: String,
        settings: AppSettings? = nil,
        deviceId: String? = nil
    ) async throws -> [RemoteSessionCandidate] {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let base = Self.normalizedStreamingBase(streamingBaseUrl, vpcId: vpcId)
        let requestDeviceId = deviceId?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
            ? deviceId!
            : UUID().uuidString
        let url = URL(string: "\(base)/v2/session")!
        let (data, response) = try await request(
            url: url,
            method: "GET",
            headers: Self.cloudMatchHeaders(
                token: token,
                clientId: UUID().uuidString,
                deviceId: requestDeviceId,
                includeOrigin: false
            ),
            sessionSettings: settings
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
            let sessionRequestData = item["sessionRequestData"] as? [String: Any]
            let appId = sessionRequestData?["appId"].flatMap { "\($0)" }
            let serverIp = Self.extractServerIp(sessionObj: item)
            let streamSettingsSignature = Self.metadataValue(
                key: GFNConstants.streamSettingsMetadataKey,
                in: sessionRequestData?["metaData"] as? [[String: Any]]
            )
            let negotiatedProfile = Self.extractNegotiatedStreamProfile(sessionObj: item)
            return RemoteSessionCandidate(
                id: sessionId,
                appId: appId,
                status: status,
                serverIp: serverIp,
                streamSettingsSignature: streamSettingsSignature,
                resolution: negotiatedProfile?.resolution,
                fps: negotiatedProfile?.fps,
                colorQuality: negotiatedProfile?.colorQuality,
                hdrEnabled: Self.toBoolean((item["finalizedStreamingFeatures"] as? [String: Any])?["trueHdr"])
            )
        }
    }

    func claimSession(
        session: AuthSession,
        candidate: RemoteSessionCandidate,
        game: CloudGame,
        streamingBaseUrl: String,
        vpcId: String,
        settings: AppSettings,
        deviceId: String,
        touchProvisionedOverride: Bool? = nil
    ) async throws -> ActiveSession {
        let token = session.tokens.idToken ?? session.tokens.accessToken
        let clientId = UUID().uuidString
        let claimDeviceId = deviceId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? UUID().uuidString : deviceId
        let streamProfile = StreamSettingsResolver.profile(for: settings, membershipTier: session.user.membershipTier)
        // Claiming repeats the session request body. During recovery, retain the input device
        // profile chosen when this allocation was created instead of letting a transient hot-plug
        // silently change a touch session into a controller session (or vice versa).
        let physicalInputConnected = NativeStreamPhysicalInput.keyboardOrMouseConnected
        let deviceProfile = StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: physicalInputConnected,
            touchProvisionedOverride: touchProvisionedOverride)
        deviceProfile.recordInputRequest(settings: settings, keyboardMouseConnected: physicalInputConnected,
            action: "claim")
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
                        deviceId: claimDeviceId,
                        includeOrigin: false,
                        deviceProfile: deviceProfile
                    ),
                    sessionSettings: settings
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
                    deviceId: claimDeviceId,
                    includeOrigin: false,
                    deviceProfile: deviceProfile
                    ),
                    sessionSettings: settings
                )
            let validationJSON = validationResponse.statusCode == 200 ? (try? parseJSON(validationData)) ?? [:] : [:]
            validationSessionObj = validationJSON["session"] as? [String: Any] ?? [:]
            preClaimStatus = validationSessionObj["status"] as? Int
        } catch {
        }

        var claimJSON: [String: Any] = [:]
        if preClaimStatus != 1 {
            let sessionQuery = URLQueryItemEncoder.encode([
                "keyboardLayout": StreamSettingsResolver.normalizedKeyboardLayout(settings.keyboardLayout),
                "languageCode": StreamSettingsResolver.normalizedGameLanguage(settings.gameLanguage)
            ])
            let claimURL = URL(string: "https://\(effectiveServerIp)/v2/session/\(candidate.id)?\(sessionQuery)")!
            let claimBody = Self.buildClaimBody(
                sessionId: candidate.id,
                appId: candidate.appId ?? game.launchAppId ?? "0",
                settings: settings,
                profile: streamProfile,
                deviceProfile: deviceProfile,
                deviceHashId: claimDeviceId
            )

            let (claimData, claimResponse) = try await request(
                url: claimURL,
                method: "PUT",
                headers: Self.cloudMatchHeaders(
                    token: token,
                    clientId: clientId,
                    deviceId: claimDeviceId,
                    includeOrigin: true,
                    deviceProfile: deviceProfile
                ),
                body: claimBody,
                sessionSettings: settings
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
            queuePosition: Self.extractQueuePosition(sessionObj: resolvedSessionObj),
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
            deviceId: claimDeviceId,
            adState: Self.extractAdState(sessionObj: resolvedSessionObj),
            gpuType: resolvedSessionObj["gpuType"] as? String,
            negotiatedStreamProfile: Self.extractNegotiatedStreamProfile(sessionObj: resolvedSessionObj),
            requestedStreamingFeatures: Self.extractStreamingFeatures(
                (resolvedSessionObj["sessionRequestData"] as? [String: Any])?["requestedStreamingFeatures"] as? [String: Any]
            ),
            finalizedStreamingFeatures: Self.extractStreamingFeatures(resolvedSessionObj["finalizedStreamingFeatures"] as? [String: Any]),
            touchProvisioned: deviceProfile.appLaunchMode == .touchFriendly,
            nativeRtspsEndpoints: NativeStreamNVSTConfiguration.endpoints(sessionObj: resolvedSessionObj, fallbackHost: claimServerIp)
        )
        let claimedControl = (resolvedSessionObj["sessionControlInfo"] as? [String: Any])
            ?? (validationSessionObj["sessionControlInfo"] as? [String: Any])
        active.sessionControlBaseUrl = SessionControlRouting.baseURL(
            host: claimedControl?["ip"] as? String, port: claimedControl?["port"] as? Int
        )

        for _ in 0..<45 {
            let polled = try await pollSession(session: session, activeSession: active, settings: settings)
            active = polled
            if active.status == 2 || active.status == 3 {
                return active
            }
            try? await Task.sleep(for: .seconds(1))
        }
        return active
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

    private static func metadataValue(key: String, in metadata: [[String: Any]]?) -> String? {
        guard let item = metadata?.first(where: {
            guard let itemKey = $0["key"] as? String else { return false }
            return itemKey.caseInsensitiveCompare(key) == .orderedSame
        }) else {
            return nil
        }
        guard let rawValue = item["value"], !(rawValue is NSNull) else {
            return nil
        }
        let value = String(describing: rawValue).trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    private static func extractMediaConnectionInfo(sessionObj: [String: Any]) -> (ip: String?, port: Int) {
        let connections = sessionObj["connectionInfo"] as? [[String: Any]] ?? []

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

        // Desktop only treats usage=2/17 as WebRTC media endpoints. Other
        // usages can be RTSPS/signaling fallbacks and must not drive WebRTC ICE.
        for usage in [2, 17] {
            if let candidate = connections.first(where: { ($0["usage"] as? Int) == usage }) {
                let ip = extractIp(from: candidate)
                let port = extractPort(from: candidate)
                if ip != nil, port > 0 {
                    return (ip, port)
                }
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

    private static func toNonnegativeInt(_ value: Any?) -> Int? {
        if let number = value as? Int, number >= 0 { return number }
        if let text = value as? String, let number = Int(text), number >= 0 { return number }
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
            serverSentEmptyAds: sessionObj["sessionAds"] == nil || sessionObj["sessionAds"] is NSNull
        )
    }

    private static func extractQueuePosition(sessionObj: [String: Any]) -> Int? {
        toPositiveInt(sessionObj["queuePosition"])
            ?? toPositiveInt((sessionObj["seatSetupInfo"] as? [String: Any])?["queuePosition"])
            ?? toPositiveInt((sessionObj["sessionProgress"] as? [String: Any])?["queuePosition"])
            ?? toPositiveInt((sessionObj["progressInfo"] as? [String: Any])?["queuePosition"])
    }

    private static func extractStreamingFeatures(_ features: [String: Any]?) -> StreamingFeatures? {
        guard let features else { return nil }
        let normalized = StreamingFeatures(
            reflex: toBoolean(features["reflex"]),
            bitDepth: toNonnegativeInt(features["bitDepth"]),
            cloudGsync: toBoolean(features["cloudGsync"]),
            chromaFormat: toNonnegativeInt(features["chromaFormat"]),
            enabledL4S: toBoolean(features["enabledL4S"]),
            trueHdr: toBoolean(features["trueHdr"])
        )
        if normalized.reflex == nil,
           normalized.bitDepth == nil,
           normalized.cloudGsync == nil,
           normalized.chromaFormat == nil,
           normalized.enabledL4S == nil,
           normalized.trueHdr == nil {
            return nil
        }
        return normalized
    }

    private static func extractNegotiatedStreamProfile(sessionObj: [String: Any]) -> NegotiatedStreamProfile? {
        let requested = (sessionObj["sessionRequestData"] as? [String: Any])?["requestedStreamingFeatures"] as? [String: Any]
        let finalized = sessionObj["finalizedStreamingFeatures"] as? [String: Any]
        let monitor = ((sessionObj["sessionRequestData"] as? [String: Any])?["clientRequestMonitorSettings"] as? [[String: Any]])?.first
            ?? (sessionObj["monitorSettings"] as? [[String: Any]])?.first
        let width = toPositiveInt(monitor?["widthInPixels"])
        let height = toPositiveInt(monitor?["heightInPixels"])
        let bitDepth = toNonnegativeInt(finalized?["bitDepth"]) ?? toNonnegativeInt(requested?["bitDepth"])
        let chromaFormat = toNonnegativeInt(finalized?["chromaFormat"]) ?? toNonnegativeInt(requested?["chromaFormat"])
        let colorQuality = CloudMatchStreamingFeatureRequest.normalizedColorQuality(
            bitDepth: bitDepth, chromaFormat: chromaFormat
        )
        let resolution = width.flatMap { width in
            height.map { "\(width)x\($0)" }
        }
        let normalized = NegotiatedStreamProfile(
            resolution: resolution,
            fps: toPositiveInt(monitor?["framesPerSecond"]),
            codec: toOptionalString(sessionObj["codec"]) ?? toOptionalString(finalized?["codec"]) ?? toOptionalString(requested?["codec"]),
            colorQuality: colorQuality,
            enableL4S: toBoolean(finalized?["enabledL4S"]) ?? toBoolean(requested?["enabledL4S"]),
            enableCloudGsync: toBoolean(finalized?["cloudGsync"]) ?? toBoolean(requested?["cloudGsync"]),
            enableReflex: toBoolean(finalized?["reflex"]) ?? toBoolean(requested?["reflex"])
        )
        if normalized.resolution == nil,
           normalized.fps == nil,
           normalized.codec == nil,
           normalized.colorQuality == nil,
           normalized.enableL4S == nil,
           normalized.enableCloudGsync == nil,
           normalized.enableReflex == nil {
            return nil
        }
        return normalized
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

    private func fetchServerInfo(token: String, streamingBaseUrl: String) async throws -> (vpcId: String?, regions: [StreamRegion]) {
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
        let regions = metadata.compactMap { entry -> StreamRegion? in
            guard let key = entry["key"] as? String,
                  key != "gfn-regions",
                  !key.starts(with: "gfn-"),
                  let rawValue = entry["value"] as? String,
                  var components = URLComponents(string: rawValue),
                  components.scheme?.lowercased() == "https",
                  components.host != nil else {
                return nil
            }
            if components.path.isEmpty {
                components.path = "/"
            } else if !components.path.hasSuffix("/") {
                components.path += "/"
            }
            guard let normalizedURL = components.url?.absoluteString else { return nil }
            guard !StreamZonePolicy.isBlocked(key), !StreamZonePolicy.isBlocked(normalizedURL) else {
                return nil
            }
            return StreamRegion(name: key, url: normalizedURL)
        }
        .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        return (vpcId, regions)
    }

    private func fetchPanels(token: String, panelNames: [String], vpcId: String) async throws -> [String: Any] {
        let variablesData: [String: Any] = [
            "vpcId": vpcId,
            "locale": "en_US",
            "panelNames": panelNames
        ]
        let variables = String(data: try JSONSerialization.data(withJSONObject: variablesData), encoding: .utf8) ?? "{}"
        let panelHash = panelNames.contains("LIBRARY")
            ? GFNConstants.libraryWithTimeQueryHash : GFNConstants.panelsQueryHash
        let extensionsData: [String: Any] = ["persistedQuery": ["sha256Hash": panelHash]]
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

    private func fetchAppMetadata(
        token: String,
        appIds: [String],
        vpcId: String,
        timeoutInterval: TimeInterval? = nil
    ) async throws -> [String: Any] {
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
            ],
            timeoutInterval: timeoutInterval
        )
        guard response.statusCode == 200 else {
            let text = String(data: data, encoding: .utf8) ?? "unknown"
            throw NSError(domain: "OpenNOW.GameMetadata", code: response.statusCode, userInfo: [NSLocalizedDescriptionKey: text])
        }
        return try parseJSON(data)
    }

    private func fetchAppMetadataWithRegistryRetry(
        token: String,
        appIds: [String],
        vpcId: String,
        deadline: Date,
        allowSplit: Bool = true
    ) async throws -> [String: Any] {
        var lastPayload: [String: Any] = [:]
        var lastRegistryError: Error?
        for attempt in 0..<3 {
            let remainingTime = deadline.timeIntervalSinceNow
            guard remainingTime > 0 else {
                throw NSError(
                    domain: "OpenNOW.GameMetadata",
                    code: 2,
                    userInfo: [NSLocalizedDescriptionKey: "Catalog metadata refresh deadline exceeded"]
                )
            }
            let payload: [String: Any]
            do {
                payload = try await fetchAppMetadata(
                    token: token,
                    appIds: appIds,
                    vpcId: vpcId,
                    timeoutInterval: min(10, remainingTime)
                )
            } catch {
                guard error.localizedDescription.localizedCaseInsensitiveContains("PersistedQueryNotFound") ||
                        error.localizedDescription.localizedCaseInsensitiveContains("PERSISTED_QUERY_NOT_FOUND") else {
                    throw error
                }
                lastRegistryError = error
                if attempt < 2 {
                    try await Task.sleep(nanoseconds: UInt64(attempt + 1) * 150_000_000)
                }
                continue
            }
            lastPayload = payload
            let errors = payload["errors"] as? [[String: Any]] ?? []
            let registryMiss = errors.contains { error in
                let message = (error["message"] as? String) ?? ""
                let code = (error["extensions"] as? [String: Any])?["code"] as? String ?? ""
                return message.localizedCaseInsensitiveContains("PersistedQueryNotFound") ||
                    code.localizedCaseInsensitiveContains("PERSISTED_QUERY_NOT_FOUND")
            }
            guard registryMiss else { return payload }
            if attempt < 2 {
                try await Task.sleep(nanoseconds: UInt64(attempt + 1) * 150_000_000)
            }
        }

        if allowSplit, appIds.count > 1 {
            let midpoint = appIds.count / 2
            let subchunks = [Array(appIds[..<midpoint]), Array(appIds[midpoint...])]
            var combinedItems: [[String: Any]] = []
            var successfulSubchunks = 0
            for subchunk in subchunks where !subchunk.isEmpty {
                let payload: [String: Any]
                do {
                    payload = try await fetchAppMetadataWithRegistryRetry(
                        token: token,
                        appIds: subchunk,
                        vpcId: vpcId,
                        deadline: deadline,
                        allowSplit: false
                    )
                } catch {
                    logger.warning("Catalog metadata subchunk unavailable size=\(subchunk.count, privacy: .public)")
                    continue
                }
                let errors = payload["errors"] as? [[String: Any]] ?? []
                guard errors.isEmpty else {
                    logger.warning("Catalog metadata subchunk unavailable size=\(subchunk.count, privacy: .public)")
                    continue
                }
                successfulSubchunks += 1
                let data = payload["data"] as? [String: Any]
                let apps = data?["apps"] as? [String: Any]
                combinedItems.append(contentsOf: apps?["items"] as? [[String: Any]] ?? [])
            }
            if successfulSubchunks > 0 {
                return ["data": ["apps": ["items": combinedItems]]]
            }
        }
        if let lastRegistryError {
            throw lastRegistryError
        }
        return lastPayload
    }

    private func enrichGamesWithMetadata(token: String, vpcId: String, games: [CloudGame]) async throws -> [CloudGame] {
        let appIds = Array(Set(games.compactMap(\.uuid)))
        guard !appIds.isEmpty else { return games }

        var metadataById: [String: [String: Any]] = [:]
        var lastError: Error?
        let deadline = Date().addingTimeInterval(20)
        let chunkSize = 40
        for start in stride(from: 0, to: appIds.count, by: chunkSize) {
            guard Date() < deadline else {
                logger.warning("Catalog metadata enrichment stopped at its 20-second deadline")
                break
            }
            let chunk = Array(appIds[start..<min(start + chunkSize, appIds.count)])
            do {
                let payload = try await fetchAppMetadataWithRegistryRetry(
                    token: token,
                    appIds: chunk,
                    vpcId: vpcId,
                    deadline: deadline
                )
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
            } catch {
                lastError = error
                logger.warning(
                    "Catalog metadata chunk failed size=\(chunk.count, privacy: .public) error=\(error.localizedDescription, privacy: .public)"
                )
            }
        }

        if metadataById.isEmpty, let lastError {
            throw lastError
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

    /// Searches the whole catalog server-side.
    ///
    /// The panels query only returns curated sections, so the local filter over `allGames` could
    /// never find a game NVIDIA had not put in a row — which is most of the catalog. Android has
    /// always searched server-side; this is the same `apps(searchQuery:)` query, posted as plain
    /// GraphQL rather than a persisted one because there is no published hash for it.
    ///
    /// The result is fed through `flattenPanels` by wrapping it in a one-section panel, so search
    /// results are parsed by exactly the same code as the catalog and cannot drift from it.
    func searchCatalog(token: String, vpcId: String, query: String, limit: Int = 60) async throws -> [CloudGame] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }

        let document = """
        query GetSearchFilterResults($vpcId: String!, $locale: String!, $sortString: String!, $fetchCount: Int!, $cursor: String!, $searchString: String!, $filters: AppFilterFields!) {
          apps(vpcId: $vpcId, language: $locale, orderBy: $sortString, first: $fetchCount, after: $cursor, searchQuery: $searchString, filters: $filters) {
            numberReturned
            pageInfo { hasNextPage endCursor }
            items {
              id
              title
              shortDescription
              longDescription
              publisherName
              images { KEY_ART GAME_BOX_ART TV_BANNER HERO_IMAGE SCREENSHOTS }
              variants { id appStore supportedControls gfn { status library { status selected lastPlayedDate } } }
              gfn { playType playabilityState minimumMembershipTierLabel }
              genres { name }
              contentRatings { name }
            }
          }
        }
        """

        let body: [String: Any] = [
            "query": document,
            "variables": [
                "vpcId": vpcId,
                "locale": "en_US",
                "sortString": "itemMetadata.relevance:DESC,sortName:ASC",
                "fetchCount": limit,
                "cursor": "",
                "searchString": trimmed,
                "filters": [String: Any]()
            ]
        ]

        var request = URLRequest(url: URL(string: GFNConstants.graphQL)!)
        request.httpMethod = "POST"
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        for (key, value) in [
            "Accept": "application/json, text/plain, */*",
            "Content-Type": "application/json",
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
        ] {
            request.setValue(value, forHTTPHeaderField: key)
        }

        let (data, response) = try await DiagnosticsHTTPRecorder.data(
            for: request,
            using: URLSession.shared,
            source: "catalog.search"
        )
        guard (response as? HTTPURLResponse)?.statusCode == 200 else {
            throw NSError(
                domain: "OpenNOW.Catalog",
                code: (response as? HTTPURLResponse)?.statusCode ?? -1,
                userInfo: [NSLocalizedDescriptionKey: "Catalog search failed."]
            )
        }

        let payload = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        guard let items = ((payload["data"] as? [String: Any])?["apps"] as? [String: Any])?["items"] as? [[String: Any]] else {
            return []
        }
        return Self.flattenPanels(payload: Self.searchResultsAsPanelPayload(items))
    }

    /// Reshapes a flat `apps.items` array into the panel envelope `flattenPanels` expects, so one
    /// parser serves both paths.
    private static func searchResultsAsPanelPayload(_ items: [[String: Any]]) -> [String: Any] {
        [
            "data": [
                "panels": [
                    [
                        "sections": [
                            [
                                "id": "SEARCH",
                                "title": "Search results",
                                "items": items.map { ["__typename": "GameItem", "app": $0] }
                            ]
                        ]
                    ]
                ]
            ]
        ]
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
                let sectionId = toOptionalString(section["id"])
                let sectionTitle = toOptionalString(section["title"])
                    ?? toOptionalString(section["name"])
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
                    let images = app["images"] as? [String: Any]
                    let boxArtUrl = optimizedImageURL(
                        (images?["GAME_BOX_ART"] as? String)
                            ?? (images?["KEY_ART"] as? String)
                            ?? (images?["HERO_IMAGE"] as? String)
                            ?? (images?["TV_BANNER"] as? String)
                    )
                    let heroImageUrl = optimizedImageURL(
                        (images?["HERO_IMAGE"] as? String)
                            ?? (images?["TV_BANNER"] as? String)
                            ?? (images?["KEY_ART"] as? String)
                            ?? (images?["GAME_BOX_ART"] as? String)
                    )
                    let tvBannerUrl = optimizedImageURL(
                        (images?["TV_BANNER"] as? String)
                            ?? (images?["HERO_IMAGE"] as? String)
                            ?? (images?["KEY_ART"] as? String)
                            ?? (images?["GAME_BOX_ART"] as? String)
                    )
                    let screenshotUrls = imageURLs(from: images?["SCREENSHOTS"])
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
                            imageUrl: boxArtUrl,
                            boxArtUrl: boxArtUrl,
                            heroImageUrl: heroImageUrl,
                            tvBannerUrl: tvBannerUrl,
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
                            membershipTierLabel: metadata.membershipTierLabel,
                            catalogSectionId: sectionId,
                            catalogSectionTitle: sectionTitle,
                            contentRatings: GFNContentRatingParser.labels(from: app["contentRatings"]),
                            screenshotUrls: screenshotUrls
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
        let images = app["images"] as? [String: Any]
        let boxArtUrl = optimizedImageURL(
            (images?["GAME_BOX_ART"] as? String)
                ?? (images?["KEY_ART"] as? String)
                ?? (images?["HERO_IMAGE"] as? String)
                ?? (images?["TV_BANNER"] as? String)
        ) ?? game.boxArtUrl ?? game.imageUrl
        let heroImageUrl = optimizedImageURL(
            (images?["HERO_IMAGE"] as? String)
                ?? (images?["TV_BANNER"] as? String)
                ?? (images?["KEY_ART"] as? String)
                ?? (images?["GAME_BOX_ART"] as? String)
        ) ?? game.heroImageUrl
        let tvBannerUrl = optimizedImageURL(
            (images?["TV_BANNER"] as? String)
                ?? (images?["HERO_IMAGE"] as? String)
                ?? (images?["KEY_ART"] as? String)
                ?? (images?["GAME_BOX_ART"] as? String)
        ) ?? game.tvBannerUrl
        let screenshotUrls = mergedImageURLs(
            imageURLs(from: images?["SCREENSHOTS"]),
            game.screenshotUrls
        )

        return CloudGame(
            id: game.id,
            title: toOptionalString(app["title"]) ?? game.title,
            genre: game.genre,
            platform: game.platform,
            icon: game.icon,
            imageUrl: boxArtUrl,
            boxArtUrl: boxArtUrl,
            heroImageUrl: heroImageUrl,
            tvBannerUrl: tvBannerUrl,
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
            membershipTierLabel: metadata.membershipTierLabel ?? game.membershipTierLabel,
            catalogSectionId: game.catalogSectionId,
            catalogSectionTitle: game.catalogSectionTitle,
            contentRatings: GFNContentRatingParser.merging(
                GFNContentRatingParser.labels(from: app["contentRatings"]),
                game.contentRatings
            ),
            screenshotUrls: screenshotUrls
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
        GFNCatalogLabelParser.labels(from: value)
    }

    private static func imageURLs(from value: Any?) -> [String]? {
        let rawURLs: [String]
        if let urls = value as? [String] {
            rawURLs = urls
        } else if let items = value as? [[String: Any]] {
            rawURLs = items.compactMap { item in
                ["url", "imageUrl", "mediaUrl", "src"]
                    .compactMap { toOptionalString(item[$0]) }
                    .first
            }
        } else if let url = toOptionalString(value) {
            rawURLs = [url]
        } else {
            return nil
        }
        return mergedImageURLs(rawURLs, nil)
    }

    private static func mergedImageURLs(_ primary: [String]?, _ fallback: [String]?) -> [String]? {
        var seen = Set<String>()
        let merged = ((primary ?? []) + (fallback ?? [])).compactMap { raw -> String? in
            let value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !value.isEmpty, seen.insert(value).inserted else { return nil }
            return value
        }
        return merged.isEmpty ? nil : merged
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
            "User-Agent": deviceProfile.userAgent,
            "Authorization": "GFNJWT \(token)",
            "Content-Type": "application/json",
            "nv-browser-type": "CHROME",
            "nv-client-id": clientId,
            "nv-client-streamer": "NVIDIA-CLASSIC",
            "nv-client-type": "NATIVE",
            "nv-client-version": GFNConstants.gfnClientVersion,
            "nv-device-make": deviceProfile.nvDeviceMake,
            "nv-device-model": deviceProfile.nvDeviceModel,
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
        settings: AppSettings,
        profile: StreamVideoProfile,
        launcherName: String,
        deviceProfile: StreamDeviceProfile,
        deviceHashId: String,
        accountLinked: Bool
    ) -> Data {
        let hdrEnabled = settings.hdrEnabled
        let colorQuality = StreamSettingsResolver.colorQuality(for: settings)
        let bitDepth = hdrEnabled || colorQuality.bitDepth == 10 ? 10 : 0
        let chromaFormat = colorQuality.chromaFormat
        let hdrDisplayDataValue: Any = hdrEnabled ? hdrDisplayData() : NSNull()
        let hdrCapabilitiesValue: Any = hdrEnabled ? hdrCapabilities() : NSNull()
        let metadata: [[String: String]] = [
            ["key": "SubSessionId", "value": UUID().uuidString],
            ["key": "wssignaling", "value": "1"],
            ["key": "GSStreamerType", "value": "WebRTC"],
            ["key": "networkType", "value": "Unknown"],
            ["key": "ClientImeSupport", "value": "0"],
            ["key": "preferredLauncher", "value": launcherName],
            ["key": "clientPhysicalResolution", "value": "{\"horizontalPixels\":\(profile.width),\"verticalPixels\":\(profile.height)}"],
            ["key": GFNConstants.streamSettingsMetadataKey, "value": StreamSettingsResolver.sessionSignature(for: settings, profile: profile)],
            ["key": "surroundAudioInfo", "value": "2"]
        ]
        let body: [String: Any] = [
            "sessionRequestData": [
                "appId": appId,
                "internalTitle": title,
                "availableSupportedControllers": deviceProfile.availableSupportedControllers,
                "networkTestSessionId": NSNull(),
                "parentSessionId": NSNull(),
                "clientIdentification": deviceProfile.clientIdentification,
                "deviceHashId": deviceHashId,
                "clientVersion": "30.0",
                "sdkVersion": "1.0",
                "streamerVersion": 1,
                "clientPlatformName": deviceProfile.clientPlatformName,
                "clientRequestMonitorSettings": [[
                    "monitorId": 0,
                    "positionX": 0,
                    "positionY": 0,
                    "widthInPixels": profile.width,
                    "heightInPixels": profile.height,
                    "framesPerSecond": profile.fps,
                    "sdrHdrMode": hdrEnabled ? 1 : 0,
                    "displayData": hdrDisplayDataValue,
                    "hdr10PlusGamingData": NSNull(),
                    "dpi": 100
                ]],
                "useOps": true,
                "audioMode": 2,
                "metaData": metadata,
                "sdrHdrMode": hdrEnabled ? 1 : 0,
                "clientDisplayHdrCapabilities": hdrCapabilitiesValue,
                "surroundAudioInfo": 0,
                "remoteControllersBitmap": deviceProfile.remoteControllersBitmap,
                "clientTimezoneOffset": TimeZone.current.secondsFromGMT() * 1000,
                "enhancedStreamMode": 1,
                "appLaunchMode": deviceProfile.appLaunchMode.rawValue,
                "secureRTSPSupported": false,
                "partnerCustomData": "",
                "accountLinked": accountLinked,
                "enablePersistingInGameSettings": deviceProfile.persistsInGameSettings,
                "userAge": 26,
                "requestedStreamingFeatures": requestedStreamingFeatures(
                    settings: settings,
                    profile: profile,
                    bitDepth: bitDepth,
                    chromaFormat: chromaFormat,
                    hdrEnabled: hdrEnabled
                )
            ]
        ]
        return (try? JSONSerialization.data(withJSONObject: NativeStreamNVSTConfiguration.requestBody(body, enabled: settings.experimentalNativeNVSTEnabled))) ?? Data()
    }

    private static func buildClaimBody(
        sessionId: String,
        appId: String,
        settings: AppSettings,
        profile: StreamVideoProfile,
        deviceProfile: StreamDeviceProfile,
        deviceHashId: String
    ) -> Data {
        let hdrEnabled = settings.hdrEnabled
        let colorQuality = StreamSettingsResolver.colorQuality(for: settings)
        let bitDepth = hdrEnabled || colorQuality.bitDepth == 10 ? 10 : 0
        let chromaFormat = colorQuality.chromaFormat
        let hdrDisplayDataValue: Any = hdrEnabled ? hdrDisplayData() : NSNull()
        let hdrCapabilitiesValue: Any = hdrEnabled ? hdrCapabilities() : NSNull()
        let metadata: [[String: String]] = [
            ["key": "SubSessionId", "value": UUID().uuidString],
            ["key": "wssignaling", "value": "1"],
            ["key": "GSStreamerType", "value": "WebRTC"],
            ["key": "networkType", "value": "Unknown"],
            ["key": "ClientImeSupport", "value": "0"],
            ["key": "clientPhysicalResolution", "value": "{\"horizontalPixels\":\(profile.width),\"verticalPixels\":\(profile.height)}"],
            ["key": GFNConstants.streamSettingsMetadataKey, "value": StreamSettingsResolver.sessionSignature(for: settings, profile: profile)],
            ["key": "surroundAudioInfo", "value": "2"]
        ]
        let body: [String: Any] = [
            "action": 2,
            "data": "RESUME",
            "sessionRequestData": [
                "audioMode": 2,
                "remoteControllersBitmap": deviceProfile.remoteControllersBitmap,
                "sdrHdrMode": hdrEnabled ? 1 : 0,
                "networkTestSessionId": NSNull(),
                "availableSupportedControllers": deviceProfile.availableSupportedControllers,
                "clientVersion": "30.0",
                "deviceHashId": deviceHashId,
                "internalTitle": NSNull(),
                "clientPlatformName": deviceProfile.clientPlatformName,
                "clientRequestMonitorSettings": [[
                    "monitorId": 0,
                    "positionX": 0,
                    "positionY": 0,
                    "widthInPixels": profile.width,
                    "heightInPixels": profile.height,
                    "framesPerSecond": profile.fps,
                    "sdrHdrMode": hdrEnabled ? 1 : 0,
                    "displayData": hdrDisplayDataValue,
                    "hdr10PlusGamingData": NSNull(),
                    "dpi": 100
                ]],
                "metaData": metadata,
                "surroundAudioInfo": 0,
                "clientTimezoneOffset": TimeZone.current.secondsFromGMT() * 1000,
                "clientIdentification": deviceProfile.clientIdentification,
                "parentSessionId": NSNull(),
                "appId": Int(appId) ?? 0,
                "streamerVersion": 1,
                "appLaunchMode": deviceProfile.appLaunchMode.rawValue,
                "sdkVersion": "1.0",
                "enhancedStreamMode": 1,
                "useOps": true,
                "clientDisplayHdrCapabilities": hdrCapabilitiesValue,
                "accountLinked": true,
                "partnerCustomData": "",
                "enablePersistingInGameSettings": deviceProfile.persistsInGameSettings,
                "secureRTSPSupported": false,
                "userAge": 26,
                "requestedStreamingFeatures": requestedStreamingFeatures(
                    settings: settings,
                    profile: profile,
                    bitDepth: bitDepth,
                    chromaFormat: chromaFormat,
                    hdrEnabled: hdrEnabled
                )
            ],
            "metaData": []
        ]
        return (try? JSONSerialization.data(withJSONObject: NativeStreamNVSTConfiguration.requestBody(body, enabled: settings.experimentalNativeNVSTEnabled))) ?? Data()
    }

    private static func requestedStreamingFeatures(
        settings: AppSettings,
        profile: StreamVideoProfile,
        bitDepth: Int,
        chromaFormat: Int,
        hdrEnabled: Bool
    ) -> [String: Any] {
        CloudMatchStreamingFeatureRequest.build(
            settings: settings, profile: profile, bitDepth: bitDepth, chromaFormat: chromaFormat
        )
    }

    private static func hdrDisplayData() -> [String: Any] {
        [
            "desiredContentMaxLuminance": 1000,
            "desiredContentMinLuminance": 0,
            "desiredContentMaxFrameAverageLuminance": 500
        ]
    }

    private static func hdrCapabilities() -> [String: Any] {
        [
            "version": 1,
            "hdrEdrSupportedFlagsInUint32": 1,
            "staticMetadataDescriptorId": 0
        ]
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

private enum AuthKeychainStore {
    private static let service = "com.opencloudgaming.opennow.auth"
    private static let account = "OpenNOW.iOS.authState"

    static func load() -> Data? {
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecReturnData: true,
            kSecMatchLimit: kSecMatchLimitOne
        ]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else {
            return nil
        }
        return result as? Data
    }

    @discardableResult
    static func save(_ data: Data) -> Bool {
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account
        ]
        let attributes: [CFString: Any] = [
            kSecValueData: data,
            kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        ]
        let updateStatus = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if updateStatus == errSecSuccess {
            return true
        }
        guard updateStatus == errSecItemNotFound else { return false }
        var item = query
        attributes.forEach { item[$0.key] = $0.value }
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func delete() {
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account
        ]
        SecItemDelete(query as CFDictionary)
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
    @Published private(set) var subscription: SubscriptionSnapshot?
    @Published private(set) var savedAccounts: [SavedAccount] = []
    @Published private(set) var accountConnectors: [AccountConnector] = []
    @Published private(set) var availableRegions: [StreamRegion] = []
    @Published private(set) var loadingAccountConnectors = false
    @Published private(set) var connectorActionStore: String?
    @Published private(set) var switchingAccountUserId: String?
    @Published var settings: AppSettings
    @Published var searchText = "" {
        didSet {
            guard searchText != oldValue else { return }
            scheduleCatalogSearch()
        }
    }

    /// Games the server returned for the current query that are not in the local catalog.
    @Published private(set) var remoteSearchResults: [CloudGame] = []
    @Published private(set) var isSearchingCatalog = false
    @Published var controllerConnected = true
    @Published var isAuthenticating = false
    @Published private(set) var deviceLoginPrompt: DeviceLoginPrompt?
    @Published private(set) var authenticationPhase: String?
    @Published var isLoadingGames = false
    @Published var isLaunchingSession = false
    @Published var showStreamLoading: Bool = false
    @Published var queueOverlayVisible: Bool = false
    @Published var streamSession: ActiveSession?
    @Published var lastError: String?
    @Published private(set) var authError: String?
    @Published private(set) var accountError: String?
    @Published private(set) var catalogError: String?
    @Published private(set) var sessionError: String?
    @Published var isBootstrapping: Bool = true
    @Published private(set) var activeStreamSettings: AppSettings?

    /// Set once a stream ends and cleared when the report sheet is dismissed. Nil the rest of the
    /// time, which is what the presentation binding keys off.
    @Published private(set) var sessionReport: SessionReport?

    /// Observed queue movement for the current launch. Drives the trend line and the estimated
    /// wait; both disappear when it has nothing honest to report.
    @Published private(set) var queueTrend = QueueTrendEstimator()
    @Published private(set) var displayQueuePosition: Int?

    /// A launch that is waiting on the user to confirm ending the session it would replace.
    @Published private(set) var pendingLaunchConflict: LaunchConflict?

    /// The last failure, classified. `lastError` still carries the same text for anything that
    /// only needs a string; this adds the recovery action, which is the part worth showing.
    @Published private(set) var lastFailure: OpenNOWFailure?

    /// Cross-tab navigation request, mirroring Android's `SettingsRouteTarget`. Set by anything
    /// that wants to send the user to a specific settings page — an empty state that knows the
    /// fix, a deep link, an error with a remedy — and consumed once by the settings screen.
    @Published var pendingSettingsRoute: SettingsRouteTarget?
    #if os(tvOS)
    @Published private(set) var tvAuthLogs: [String] = []
    #endif

    private let api = GFNAPIClient()
    private let logger = Logger(subsystem: "OpenNOWiOS", category: "Session")
    private var debugEvents: [DebugLifecycleEvent] = []
    private let defaults = UserDefaults.standard
    private var authSession: AuthSession?
    private var authGeneration = UUID()
    private var sessionPollTask: Task<Void, Never>?
    private var launchTask: Task<Void, Never>?
    private var sessionReportAccumulator: StreamSessionReportAccumulator?
    private var sessionReportSessionId: String?
    private var queueTrendSessionId: String?
    /// Mirrors the Keychain-backed auth state so a persist that would write identical bytes can
    /// be skipped. See `persistAuthState`.
    private var persistedAuthState: PersistedAuthState?
    private var catalogSearchTask: Task<Void, Never>?
    private var accountRefreshTask: Task<Void, Never>?
    private var backgroundRefreshingAccountUserId: String?
    #if os(tvOS)
    private var sessionPollBackgroundTaskActive = false
    #else
    private var sessionPollBackgroundTaskId: UIBackgroundTaskIdentifier = .invalid
    #endif
    private var cachedVpcId: String = "GFN-PC"
    private var remoteSessionsSnapshotLoaded = false
    private var adReportStateById: [String: SessionAdAction] = [:]
    private var adStartedAtById: [String: Date] = [:]
    private var reopenToken: UUID = UUID()
    private var currentScenePhase: ScenePhase = .active
    private var sessionPollBackgroundAllowanceConsumed = false
    #if os(tvOS)
    private var tvAuthLogObserver: NSObjectProtocol?
    #endif

    private let settingsKey = "OpenNOW.iOS.settings"
    private let authStateKey = "OpenNOW.iOS.authState"
    private let authSessionKey = "OpenNOW.iOS.authSession"
    private let activeSessionSnapshotKey = "OpenNOW.iOS.activeSession"
    private let activeStreamSettingsKey = "OpenNOW.iOS.activeStreamSettings"
    private let deviceIdKey = "OpenNOW.iOS.deviceId"
    private let catalogCacheKeyPrefix = "OpenNOW.iOS.catalog"
    private let accountCacheKeyPrefix = "OpenNOW.iOS.accountSnapshot"
    private let legacyLibraryGamesCacheKeyPrefix = "OpenNOW.iOS.libraryGames"
    private let setupPhaseTimeoutSeconds: TimeInterval = 90
    private let sessionRestoreMaxAgeSeconds: TimeInterval = 12 * 60 * 60

    init() {
        var loadedSettings = Self.loadSettings(from: defaults) ?? .default
        if loadedSettings.preferredCodec == "HEVC" {
            loadedSettings.preferredCodec = "H265"
        }
        let removedBlockedPreferredRegion = StreamZonePolicy.isBlocked(loadedSettings.preferredRegion)
        if removedBlockedPreferredRegion {
            loadedSettings.preferredRegion = ""
        }
        loadedSettings.normalizeStreamDefaults()
        settings = loadedSettings
        let retainedWallpaperFilename = loadedSettings.catalogWallpaperFilename
        Task.detached(priority: .utility) {
            CatalogWallpaperStorage.pruneManagedWallpapers(keeping: retainedWallpaperFilename)
        }
        let loadedAuthState = Self.loadAuthState(from: defaults)
        persistedAuthState = loadedAuthState
        authSession = loadedAuthState.activeSession
        savedAccounts = loadedAuthState.savedAccounts
        if let selectedProvider = loadedAuthState.selectedProvider {
            settings.selectedProviderIdpId = selectedProvider.idpId
        } else if let provider = authSession?.provider {
            settings.selectedProviderIdpId = provider.idpId
        }
        activeSession = Self.loadActiveSession(from: defaults)
        if let restoredSession = activeSession,
           StreamZonePolicy.isBlocked(restoredSession.streamingBaseUrl)
            || StreamZonePolicy.isBlocked(restoredSession.serverIp)
            || StreamZonePolicy.isBlocked(restoredSession.zone) {
            activeSession = nil
            defaults.removeObject(forKey: activeSessionSnapshotKey)
            defaults.removeObject(forKey: activeStreamSettingsKey)
        }
        #if DEBUG
        if activeSession?.id == "debug-queue-preview" {
            activeSession = nil
            defaults.removeObject(forKey: activeSessionSnapshotKey)
            defaults.removeObject(forKey: activeStreamSettingsKey)
        }
        #endif
        activeStreamSettings = activeSession == nil ? nil : Self.loadActiveStreamSettings(from: defaults)
        user = authSession?.user
        if let authSession {
            hydrateCachedCatalog(for: authSession)
            hydrateCachedAccount(for: authSession)
        }
        showStreamLoading = activeSession != nil
        if removedBlockedPreferredRegion {
            persistSettings()
        }
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
        sessionPollTask?.cancel()
        #if os(tvOS)
        if let tvAuthLogObserver {
            NotificationCenter.default.removeObserver(tvAuthLogObserver)
        }
        #endif
    }

    var supportsNativeOAuth: Bool { OpenNOWPlatform.supportsNativeOAuth }
    var supportsEmbeddedStreamer: Bool { OpenNOWPlatform.supportsEmbeddedStreamer }
    var currentStreamerSettings: AppSettings { activeStreamSettings ?? settings }

    private func nativeLaunchSettings(for requestedSettings: AppSettings, context: String) -> AppSettings {
        let resolved = NativeStreamLaunchSettingsResolver.resolve(requestedSettings)
        if let reason = resolved.reason {
            logger.notice(
                "Adjusted native stream settings context=\(context, privacy: .public) reason=\(reason, privacy: .public) requestedCodec=\(requestedSettings.preferredCodec, privacy: .public) resolvedCodec=\(resolved.settings.preferredCodec, privacy: .public)"
            )
        }
        return resolved.settings
    }
    var cloudStorageManagementURL: URL { GFNConstants.storageManagementURL }
    var cloudStorageResetURL: URL { GFNConstants.storageResetURL }
    var cloudStorageAddURL: URL { GFNConstants.storageAddURL }
    var accountHelpURL: URL { GFNConstants.accountHelpURL }

    var diagnosticsReport: String {
        let profile = StreamSettingsResolver.profile(
            for: currentStreamerSettings,
            membershipTier: subscription?.membershipTier ?? user?.membershipTier
        )
        let codecReport = NativeStreamCodecProbe.report()
        let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown"
        let buildNumber = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "unknown"
        let provider = authSession?.provider.code ?? "signed-out"
        let tier = subscription?.membershipTier ?? user?.membershipTier ?? "unknown"
        let active = activeSession
        let negotiated = active?.negotiatedStreamProfile
        let process = ProcessInfo.processInfo
        let proxyHost = URL(string: settings.sessionProxyUrl)?.host ?? (settings.sessionProxyEnabled ? "configured" : "off")
        let adState = effectiveAdState
        let regionSummary = availableRegions
            .prefix(30)
            .map { "\($0.name)=\(URL(string: $0.url)?.host ?? $0.url)" }
            .joined(separator: ",")
        var lines = [
            "OpenNOW iOS diagnostics",
            "generatedAt=\(ISO8601DateFormatter().string(from: Date()))",
            "app.version=\(appVersion) build=\(buildNumber) bundle=\(Bundle.main.bundleIdentifier ?? "unknown")",
            "Distribution: App Store compatible build; no self-updater",
            "device.model=\(UIDevice.current.model) platform=\(OpenNOWPlatform.displayName) os=\(UIDevice.current.systemName) \(UIDevice.current.systemVersion)",
            "device.locale=\(Locale.current.identifier) timezone=\(TimeZone.current.identifier) lowPower=\(process.isLowPowerModeEnabled) thermal=\(process.thermalState.rawValue)",
            "device.memoryBytes=\(process.physicalMemory) processors=\(process.processorCount) activeProcessors=\(process.activeProcessorCount)",
            "privacy.microphoneDescriptionPresent=\(Bundle.main.object(forInfoDictionaryKey: "NSMicrophoneUsageDescription") != nil) cameraDescriptionPresent=\(Bundle.main.object(forInfoDictionaryKey: "NSCameraUsageDescription") != nil)",
            "account.signedIn=\(authSession != nil) provider=\(provider) tier=\(tier) savedAccounts=\(savedAccounts.count)",
            "catalog.games=\(allGames.count) library=\(libraryGames.count) regions=\(availableRegions.count) connectors=\(accountConnectors.count)",
            "catalog.regionHosts=\(regionSummary.isEmpty ? "none" : regionSummary)",
            "ui.bootstrapping=\(isBootstrapping) loadingGames=\(isLoadingGames) launching=\(isLaunchingSession) queueOverlay=\(queueOverlayVisible) loadingSurface=\(showStreamLoading)",
            "session.present=\(active != nil) id=\(active?.id ?? "none") status=\(active?.status ?? -1) queue=\(active?.queuePosition ?? -1) seatSetup=\(active?.seatSetupStep ?? -1)",
            "session.game=\(active?.game.title ?? "none") zone=\(active?.zone ?? "none") base=\(active.flatMap { URL(string: $0.streamingBaseUrl)?.host } ?? "none")",
            "session.server=\(active?.serverIp ?? "none") media=\(active?.mediaIp ?? "none"):\(active?.mediaPort ?? 0) signaling=\(active?.signalingServer ?? "none")",
            "session.adsRequired=\(isSessionAdsRequired(adState)) ads=\(sessionAdItems(adState).count) queuePaused=\(adState?.isQueuePaused ?? false) activeAd=\(activeQueueAd?.adId ?? "none")",
            "requested.resolution=\(profile.width)x\(profile.height) fps=\(profile.fps) bitrateKbps=\(profile.maxBitrateKbps) codec=\(currentStreamerSettings.preferredCodec) quality=\(currentStreamerSettings.preferredQuality)",
            "requested.aspect=\(currentStreamerSettings.preferredAspectRatio) color=\(currentStreamerSettings.preferredColorQuality) hdr=\(currentStreamerSettings.hdrEnabled) l4s=\(currentStreamerSettings.enableL4S) gsync=\(currentStreamerSettings.enableCloudGsync)",
            "requested.region=\(currentStreamerSettings.preferredRegion.isEmpty ? "automatic" : currentStreamerSettings.preferredRegion) proxy=\(proxyHost)",
            "negotiated.resolution=\(negotiated?.resolution ?? "unknown") fps=\(negotiated?.fps.map(String.init) ?? "unknown") codec=\(negotiated?.codec ?? "unknown") color=\(negotiated?.colorQuality?.rawValue ?? "unknown")",
            "input.keyboard=\(settings.keyboardLayout) language=\(settings.gameLanguage) fingerMouse=\(settings.fingerMouseEnabled) sensitivity=\(settings.mouseSensitivity) acceleration=\(settings.mouseAcceleration) phoneRumble=\(settings.phoneRumbleFallback)",
            "streamer.audioMuted=\(settings.streamerPreferences.audioMuted) stats=\(settings.showStatsOverlay) statsStyle=\(settings.streamerPreferences.statsStyle.rawValue) statsPosition=\(settings.streamerPreferences.statsPosition.rawValue)",
            "streamer.touchVisible=\(settings.streamerPreferences.touchControllerVisible) touchscreen=\(settings.streamerPreferences.touchscreenModeEnabled) controllerPassthrough=\(settings.streamerPreferences.physicalControllerPassthrough) stretch=\(settings.streamerPreferences.stretchStreamToFill)",
            "codec.summary=\(codecReport.summary)",
            String(format: "telemetry.fps=%d pingMs=%d packetLoss=%.2f bitrateMbps=%.1f", telemetry.fps, telemetry.pingMs, telemetry.packetLossPercent, telemetry.bitrateMbps),
            "lastError=\(lastError ?? "none")"
        ]
        for capability in codecReport.capabilities {
            lines.append(
                "codec.\(capability.codec.rawValue)=hardwareDecode:\(capability.videoToolboxHardwareDecode) webRTC:\(capability.webRTCSupported) launchSafe:\(capability.launchSafe) profiles:\(capability.webRTCProfileSummary.joined(separator: " | "))"
            )
        }
        return DiagnosticsSanitizer.sanitize(lines.joined(separator: "\n"))
    }

    func debugEventSnapshot() -> [DebugLifecycleEvent] { debugEvents }

    private func recordDebugEvent(_ category: String, _ message: String) {
        let oneLine = message.replacingOccurrences(of: "\n", with: " ")
        let redacted = String(DiagnosticsSanitizer.sanitize(oneLine).prefix(640))
        debugEvents.append(DebugLifecycleEvent(timestamp: .now, category: category, message: redacted))
        if debugEvents.count > 140 { debugEvents.removeFirst(debugEvents.count - 140) }
        logger.debug("[\(category, privacy: .public)] \(redacted, privacy: .public)")
    }

    var diagnosticsExportFileName: String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return "opennow-ios-logs-\(formatter.string(from: Date()))"
    }

    func makeDiagnosticsExport() async -> String {
        let stateSnapshot = diagnosticsReport
        let eventFormatter = ISO8601DateFormatter()
        let eventTimeline = debugEvents.map {
            "\(eventFormatter.string(from: $0.timestamp)) [\($0.category)] \($0.message)"
        }.joined(separator: "\n")
        let unifiedLogs = await Task.detached(priority: .utility) {
            Self.currentProcessUnifiedLogs()
        }.value
        let apiTrace = await DiagnosticsHTTPTraceStore.shared.export()
        return DiagnosticsSanitizer.sanitize(
            """
            \(stateSnapshot)

            privacy
            strictRedaction=true
            redacted=credentials,tokens,cookies,oauthCodes,emailAddresses,userPaths,deviceIds,userIds,sessionIds,requestIds,ipAddresses,longOpaqueValues
            apiTraceRetention=boundedInMemoryOnly
            pasteRetention=7days

            apiTrace
            \(apiTrace)

            lifecycleEvents
            \(eventTimeline.isEmpty ? "entries=0" : eventTimeline)

            unifiedLogs
            \(unifiedLogs)
            """
        ) + "\n"
    }

    func uploadDiagnosticsPaste() async throws -> URL {
        try await DiagnosticsPasteClient.upload(await makeDiagnosticsExport())
    }

    func diagnosticsClipboardSummary(pasteURL: URL) -> String {
        let profile = StreamSettingsResolver.profile(
            for: currentStreamerSettings,
            membershipTier: subscription?.membershipTier ?? user?.membershipTier
        )
        let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown"
        let buildNumber = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "unknown"
        let provider = authSession?.provider.code ?? "signed-out"
        let tier = subscription?.membershipTier ?? user?.membershipTier ?? "unknown"
        let error = DiagnosticsSanitizer.sanitize(lastError ?? "none")
        let shortError = error.count > 240 ? String(error.prefix(240)) + "…" : error
        return """
        OpenNOW iOS \(appVersion) (\(buildNumber))
        Distribution: App Store compatible build; no self-updater
        Platform: \(OpenNOWPlatform.displayName) — \(UIDevice.current.systemName) \(UIDevice.current.systemVersion)
        Provider/Tier: \(provider) / \(tier)
        Requested: \(profile.width)x\(profile.height) @ \(profile.fps) FPS — \(currentStreamerSettings.preferredCodec) — \(currentStreamerSettings.preferredColorQuality)
        Session: status \(activeSession?.status ?? -1), queue \(activeSession?.queuePosition ?? -1), seat \(activeSession?.seatSetupStep ?? -1)
        Error: \(shortError)
        Redaction: strict; paste expires after 7 days
        Full redacted log paste:
        \(pasteURL.absoluteString)
        """
    }

    nonisolated private static func currentProcessUnifiedLogs() -> String {
        do {
            let store = try OSLogStore(scope: .currentProcessIdentifier)
            let position = store.position(date: Date().addingTimeInterval(-6 * 60 * 60))
            let logs = Array(try store.getEntries(at: position))
                .compactMap { $0 as? OSLogEntryLog }
                .filter { $0.subsystem == "OpenNOWiOS" }
                .suffix(800)
            guard !logs.isEmpty else { return "entries=0" }

            let formatter = ISO8601DateFormatter()
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            var lines = ["entries=\(logs.count) max=800 windowHours=6"]
            lines.append(contentsOf: logs.map { entry in
                "\(formatter.string(from: entry.date)) level=\(entry.level.rawValue) [\(entry.category)] \(DiagnosticsSanitizer.sanitize(entry.composedMessage))"
            })
            return lines.joined(separator: "\n")
        } catch {
            return "unavailable=\(error.localizedDescription)"
        }
    }

    func bootstrap() async {
        guard isBootstrapping else { return }

        if providers.isEmpty {
            providers = [GFNConstants.defaultProvider]
        }
        if settings.selectedProviderIdpId.isEmpty {
            settings.selectedProviderIdpId = providers.first?.idpId ?? GFNConstants.defaultProvider.idpId
            persistSettings()
        }

        syncTrackedSessionSurface()
        isBootstrapping = false

        Task {
            await refreshProviders()
        }
        if authSession != nil {
            if let userId = authSession?.user.userId {
                scheduleAccountRefresh(for: userId)
                Task {
                    restoreTrackedSessionIfNeeded()
                }
            } else {
                Task {
                    restoreTrackedSessionIfNeeded()
                }
            }
        }
    }

    func refreshProviders() async {
        let fetchedProviders = await api.fetchProviders()
        providers = fetchedProviders.isEmpty ? [GFNConstants.defaultProvider] : fetchedProviders
        if settings.selectedProviderIdpId.isEmpty {
            settings.selectedProviderIdpId = providers.first?.idpId ?? GFNConstants.defaultProvider.idpId
            persistSettings()
        }
    }

    func queueRegions() async throws -> [StreamRegion] {
        if !availableRegions.isEmpty { return availableRegions }
        guard let session = authSession else { return [] }
        let generation = authGeneration
        let refreshed = try await api.refreshSession(session)
        try Task.checkCancellation()
        guard generation == authGeneration, authSession?.user.userId == session.user.userId else {
            throw CancellationError()
        }
        let regions = try await api.fetchRegions(session: refreshed).filter {
            !StreamZonePolicy.isBlocked($0.url) && !StreamZonePolicy.isBlocked($0.name)
        }
        try Task.checkCancellation()
        guard generation == authGeneration, authSession?.user.userId == session.user.userId else {
            throw CancellationError()
        }
        authSession = refreshed
        user = refreshed.user
        persistAuthSession(refreshed)
        availableRegions = regions
        return regions
    }

    func selectProvider(_ provider: LoginProvider) {
        settings.selectedProviderIdpId = provider.idpId
        persistSettings()
    }

    #if DEBUG
    func installDebugQueuePreview(position: Int) {
        if activeSession?.id == "debug-queue-preview" {
            activeSession?.queuePosition = max(1, position)
            syncTrackedSessionSurface()
            return
        }
        let heroUrl = "https://cdn.cloudflare.steamstatic.com/steam/apps/714010/library_hero.jpg"
        let boxArtUrl = "https://cdn.cloudflare.steamstatic.com/steam/apps/714010/library_600x900.jpg"
        let game = CloudGame(
            id: "debug-aimlabs",
            title: "Aimlabs",
            genre: "Action",
            platform: "Steam",
            icon: "scope",
            imageUrl: boxArtUrl,
            boxArtUrl: boxArtUrl,
            heroImageUrl: heroUrl,
            tvBannerUrl: heroUrl,
            launchAppId: "714010",
            launchOptions: [GameLaunchOption(storefront: "STEAM", appId: "714010", supportedControls: ["GAMEPAD", "KEYBOARD_MOUSE"])],
            uuid: "debug-aimlabs",
            summary: "Training and warmup",
            longDescription: nil,
            publisher: "Statespace",
            developer: "Statespace",
            releaseDate: nil,
            featureLabels: ["Gamepad", "Keyboard & Mouse"],
            tags: ["Action"],
            stores: ["STEAM"],
            playType: "Single Player",
            membershipTierLabel: nil,
            catalogSectionId: nil,
            catalogSectionTitle: nil,
            contentRatings: ["ESRB T"]
        )
        activeSession = ActiveSession(
            id: "debug-queue-preview",
            game: game,
            startedAt: Date(),
            status: 1,
            queuePosition: max(1, position),
            seatSetupStep: 1,
            serverIp: nil,
            mediaIp: nil,
            mediaPort: 0,
            signalingServer: nil,
            signalingUrl: nil,
            iceServers: [],
            zone: "NP-DAL-01",
            streamingBaseUrl: "https://preview.invalid",
            clientId: "preview",
            deviceId: "preview",
            adState: nil
        )
        activeStreamSettings = settings
        settings.queueLiveActivitiesEnabled = true
        isBootstrapping = false
        showStreamLoading = true
        queueOverlayVisible = true
        defaults.removeObject(forKey: activeSessionSnapshotKey)
        defaults.removeObject(forKey: activeStreamSettingsKey)
        Task {
            await QueueLiveActivityManager.shared.sync(
                sessionId: "debug-queue-preview",
                gameTitle: game.title,
                storeName: "Steam",
                state: QueueActivityAttributes.ContentState(
                    phase: .queued,
                    headline: "In queue",
                    detail: position == 1 ? "A cloud rig is nearly ready" : "Queue #\(max(1, position))",
                    queueLabel: position == 1 ? "Soon" : "Q\(max(1, position))",
                    queuePosition: max(1, position)
                )
            )
        }
    }
    #endif

    func signIn(forceAccountSelection: Bool = false) async {
        guard !isAuthenticating else { return }
        lastError = nil
        authError = nil
        let generation = authGeneration
        isAuthenticating = true
        authenticationPhase = "Opening secure sign-in"
        recordDebugEvent("auth", "Opening browser sign-in")
        defer {
            if generation == authGeneration {
                isAuthenticating = false
                authenticationPhase = nil
            }
        }
        #if os(tvOS)
        clearTVAuthLogs()
        #endif

        guard supportsNativeOAuth else {
            lastError = OpenNOWPlatform.authUnavailableReason
            authError = lastError
            return
        }

        let provider = providers.first(where: { $0.idpId == settings.selectedProviderIdpId }) ?? GFNConstants.defaultProvider
        do {
            let session = try await api.login(
                with: provider,
                deviceId: persistentDeviceId(),
                forceAccountSelection: forceAccountSelection
            )
            guard generation == authGeneration else { return }
            persistAuthSession(session)
            activateAccount(session, hydrateCache: true)
            scheduleAccountRefresh(for: session.user.userId)
            recordDebugEvent("auth", "Browser sign-in completed")
        } catch {
            guard generation == authGeneration else { return }
            TVAuthDiagnostics.record("Store sign-in failed: \(error.localizedDescription)")
            lastError = "Sign in failed: \(OpenNOWErrorPresenter.message(for: error, fallback: "NVIDIA could not complete sign-in."))"
            authError = lastError
            recordDebugEvent("auth", "Browser sign-in failed: \(error.localizedDescription)")
        }
    }

    func signInWithCode() async {
        guard !isAuthenticating else { return }
        lastError = nil
        authError = nil
        let generation = authGeneration
        deviceLoginPrompt = nil
        isAuthenticating = true
        authenticationPhase = "Requesting sign-in code"
        recordDebugEvent("auth", "Requesting device sign-in code")
        defer {
            if generation == authGeneration {
                isAuthenticating = false
                deviceLoginPrompt = nil
                authenticationPhase = nil
            }
        }
        let provider = providers.first(where: { $0.idpId == settings.selectedProviderIdpId }) ?? GFNConstants.defaultProvider
        do {
            let session = try await api.loginWithDeviceCode(
                with: provider,
                deviceId: persistentDeviceId(),
                onPrompt: { [weak self] prompt in
                    guard self?.authGeneration == generation else { return }
                    self?.deviceLoginPrompt = prompt
                    self?.authenticationPhase = "Waiting for approval"
                    self?.recordDebugEvent("auth", "Device code shown; waiting for approval")
                },
                onAuthorized: { [weak self] in
                    guard self?.authGeneration == generation else { return }
                    self?.authenticationPhase = "Getting sign-in tokens"
                    self?.recordDebugEvent("auth", "Device code approved; getting sign-in tokens")
                }
            )
            guard generation == authGeneration else { return }
            persistAuthSession(session)
            activateAccount(session, hydrateCache: true)
            scheduleAccountRefresh(for: session.user.userId)
            recordDebugEvent("auth", "Device sign-in completed")
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            guard generation == authGeneration else { return }
            logger.error("Device sign-in failed: \(error.localizedDescription, privacy: .private)")
            lastError = "Sign in failed: \(OpenNOWErrorPresenter.message(for: error, fallback: "NVIDIA could not complete code sign-in."))"
            authError = lastError
            recordDebugEvent("auth", "Device sign-in failed: \(error.localizedDescription)")
        }
    }

    #if os(tvOS)
    func signInOnTVOS(authenticate: @escaping TVOSOAuthPresenter) async {
        lastError = nil
        authError = nil
        let generation = authGeneration
        isAuthenticating = true
        defer { if generation == authGeneration { isAuthenticating = false } }
        clearTVAuthLogs()

        let provider = providers.first(where: { $0.idpId == settings.selectedProviderIdpId }) ?? GFNConstants.defaultProvider
        do {
            let session = try await api.login(
                with: provider,
                deviceId: persistentDeviceId(),
                authenticate: authenticate
            )
            guard generation == authGeneration else { return }
            persistAuthSession(session)
            activateAccount(session, hydrateCache: true)
            scheduleAccountRefresh(for: session.user.userId)
        } catch {
            guard generation == authGeneration else { return }
            TVAuthDiagnostics.record("Store sign-in failed: \(error.localizedDescription)")
            lastError = "Sign in failed: \(OpenNOWErrorPresenter.message(for: error, fallback: "NVIDIA could not complete sign-in."))"
            authError = lastError
        }
    }
    #endif

    func signOut() {
        let currentUserId = authSession?.user.userId
        Task { await NotificationManager.shared.cancelSessionNotifications() }
        var state = currentAuthState()
        if let currentUserId {
            state.sessions.removeAll { $0.user.userId == currentUserId }
            removeCachedCatalog(forUserId: currentUserId)
            removeCachedAccount(forUserId: currentUserId)
        } else {
            state.sessions.removeAll()
            removeAllCachedCatalog()
            removeAllCachedAccounts()
        }
        state.activeUserId = state.sessions.first?.user.userId
        state.selectedProvider = state.sessions.first?.provider ?? state.selectedProvider
        persistAuthState(state)
        if let nextSession = state.activeSession {
            activateAccount(nextSession, hydrateCache: true)
            scheduleAccountRefresh(for: nextSession.user.userId)
        } else {
            cancelAccountRefresh()
            clearAccountScopedState()
            user = nil
            authSession = nil
        }
    }

    func signOutAll() {
        authGeneration = UUID()
        isAuthenticating = false
        deviceLoginPrompt = nil
        authenticationPhase = nil
        launchTask?.cancel()
        launchTask = nil
        catalogSearchTask?.cancel()
        catalogSearchTask = nil
        cancelAccountRefresh()
        sessionPollTask?.cancel()
        sessionPollTask = nil
        Task { await NotificationManager.shared.cancelSessionNotifications() }
        persistAuthState(PersistedAuthState())
        defaults.removeObject(forKey: authStateKey)
        defaults.removeObject(forKey: authSessionKey)
        removeAllCachedCatalog()
        removeAllCachedAccounts()
        clearAccountScopedState()
        user = nil
        authSession = nil
        lastError = nil
        authError = nil
        accountError = nil
        catalogError = nil
        sessionError = nil
        lastFailure = nil
    }

    func switchAccount(to userId: String) async {
        guard authSession?.user.userId != userId else {
            scheduleAccountRefresh(for: userId)
            return
        }
        switchingAccountUserId = userId
        defer {
            if switchingAccountUserId == userId {
                switchingAccountUserId = nil
            }
        }
        await Task.yield()
        var state = currentAuthState()
        guard let nextSession = state.sessions.first(where: { $0.user.userId == userId }) else { return }
        state.activeUserId = nextSession.user.userId
        state.selectedProvider = nextSession.provider
        persistAuthState(state)
        activateAccount(nextSession, hydrateCache: true)
        scheduleAccountRefresh(for: nextSession.user.userId)
    }

    private func activateAccount(_ session: AuthSession, hydrateCache: Bool) {
        cancelAccountRefresh()
        clearAccountScopedState()
        authSession = session
        user = session.user
        settings.selectedProviderIdpId = session.provider.idpId
        persistSettings()
        if hydrateCache {
            hydrateCachedCatalog(for: session)
            hydrateCachedAccount(for: session)
        }
        lastError = nil
    }

    private func scheduleAccountRefresh(for userId: String) {
        accountRefreshTask?.cancel()
        backgroundRefreshingAccountUserId = userId
        accountRefreshTask = Task { [weak self] in
            guard let self else { return }
            await self.refreshCatalog()
            guard self.backgroundRefreshingAccountUserId == userId else { return }
            self.backgroundRefreshingAccountUserId = nil
            self.accountRefreshTask = nil
        }
    }

    private func cancelAccountRefresh() {
        accountRefreshTask?.cancel()
        accountRefreshTask = nil
        backgroundRefreshingAccountUserId = nil
    }

    private func clearAccountScopedState() {
        isLoadingGames = false
        allGames = []
        featuredGames = []
        libraryGames = []
        resumableSessions = []
        activeSession = nil
        activeStreamSettings = nil
        remoteSessionsSnapshotLoaded = false
        setStreamSession(nil, reason: "signOut")
        subscription = nil
        sessionPollTask?.cancel()
        endSessionPollBackgroundTask()
        showStreamLoading = false
        queueOverlayVisible = false
        adReportStateById = [:]
        adStartedAtById = [:]
        accountConnectors = []
        availableRegions = []
        cachedVpcId = "GFN-PC"
        loadingAccountConnectors = false
        connectorActionStore = nil
        lastError = nil
        authError = nil
        accountError = nil
        catalogError = nil
        sessionError = nil
        lastFailure = nil
        syncTrackedSessionSurface()
    }

    func refreshCatalog() async {
        guard let session = authSession else { return }
        catalogError = nil
        let requestedUserId = session.user.userId
        let requestedGeneration = authGeneration
        isLoadingGames = true
        defer {
            if accountIsCurrent(requestedUserId, generation: requestedGeneration) {
                isLoadingGames = false
            }
        }

        do {
            let refreshed = try await api.refreshSession(session)
            guard accountIsCurrent(requestedUserId, generation: requestedGeneration) else { return }
            if allGames.isEmpty || libraryGames.isEmpty {
                hydrateCachedCatalog(for: refreshed, onlyMissing: true)
            }

            let (fetchedMainGames, vpcId, regions) = try await api.fetchMainGames(session: refreshed)
            let mainGames = preservingCatalogMetadata(in: fetchedMainGames, from: allGames)
            let fetchedLibrary = try await api.fetchLibraryGames(session: refreshed, vpcId: vpcId)
            let library = preservingCatalogMetadata(in: fetchedLibrary, from: libraryGames + allGames)
            let filteredRegions = regions.filter {
                !StreamZonePolicy.isBlocked($0.url) && !StreamZonePolicy.isBlocked($0.name)
            }

            guard accountIsCurrent(requestedUserId, generation: requestedGeneration) else { return }
            let catalogSession = AuthSession(
                provider: refreshed.provider,
                tokens: refreshed.tokens,
                user: refreshed.user
            )
            authSession = catalogSession
            user = refreshed.user
            persistAuthSession(catalogSession)
            cachedVpcId = vpcId
            availableRegions = filteredRegions
            allGames = mainGames
            featuredGames = Array(mainGames.prefix(8))
            libraryGames = library
            persistCachedCatalog(
                allGames: mainGames,
                featuredGames: featuredGames,
                libraryGames: library,
                vpcId: vpcId,
                for: catalogSession
            )
            isLoadingGames = false
            lastError = nil

            var accountWarnings: [String] = []
            var refreshedSubscription = subscription
            do {
                refreshedSubscription = try await api.fetchSubscription(session: refreshed, vpcId: vpcId)
            } catch {
                accountWarnings.append(
                    OpenNOWErrorPresenter.message(for: error, fallback: "Subscription details could not be refreshed.")
                )
            }
            var refreshedConnectors = accountConnectors
            do {
                refreshedConnectors = try await api.fetchAccountConnectors(session: refreshed)
            } catch {
                accountWarnings.append(
                    OpenNOWErrorPresenter.message(for: error, fallback: "Store connections could not be refreshed.")
                )
            }

            let streamSettings = nativeLaunchSettings(for: activeStreamSettings ?? settings, context: "refreshCatalog")
            var refreshedRemoteSessions: [RemoteSessionCandidate]?
            do {
                refreshedRemoteSessions = try await api.fetchActiveSessions(
                    session: refreshed,
                    streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                    vpcId: vpcId,
                    settings: streamSettings,
                    deviceId: persistentDeviceId()
                )
            } catch {
                logger.warning("Could not refresh remote sessions during catalog refresh error=\(error.localizedDescription, privacy: .public)")
            }

            guard accountIsCurrent(requestedUserId, generation: requestedGeneration) else { return }
            var updatedUser = refreshed.user
            if let refreshedSubscription {
                updatedUser.membershipTier = refreshedSubscription.membershipTier
            }
            let updatedSession = AuthSession(provider: refreshed.provider, tokens: refreshed.tokens, user: updatedUser)
            authSession = updatedSession
            user = updatedUser
            persistAuthSession(updatedSession)

            subscription = refreshedSubscription
            accountConnectors = refreshedConnectors
            persistCachedAccount(for: updatedSession)
            if let refreshedRemoteSessions {
                resumableSessions = refreshedRemoteSessions.filter {
                    remoteSessionIsLaunchable($0) && remoteSessionIsAllowed($0)
                }
                remoteSessionsSnapshotLoaded = true
            } else {
                remoteSessionsSnapshotLoaded = false
            }
            if !accountWarnings.isEmpty {
                logger.warning(
                    "Catalog loaded while account details were partially unavailable: \(accountWarnings.joined(separator: " "), privacy: .public)"
                )
            }
            lastError = nil
            catalogError = nil
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            // Pull-to-refresh can cancel an in-flight request; treat as non-failure.
            return
        } catch {
            guard accountIsCurrent(requestedUserId, generation: requestedGeneration) else { return }
            if allGames.isEmpty || libraryGames.isEmpty {
                hydrateCachedCatalog(for: session, onlyMissing: true)
            }
            hydrateCachedAccount(for: session, onlyMissing: true)
            let message = OpenNOWErrorPresenter.message(
                for: error,
                fallback: "Account data could not be refreshed."
            )
            if allGames.isEmpty && libraryGames.isEmpty {
                lastError = "Account refresh failed: \(message)"
                catalogError = lastError
            } else {
                logger.warning("Using cached catalog after refresh failure: \(message, privacy: .public)")
                lastError = nil
                catalogError = nil
            }
        }
    }

    private func accountIsCurrent(_ userId: String, generation: UUID? = nil) -> Bool {
        authSession?.user.userId == userId && (generation == nil || generation == authGeneration)
    }

    private func preservingCatalogMetadata(in games: [CloudGame], from fallbackGames: [CloudGame]) -> [CloudGame] {
        let byID = Dictionary(fallbackGames.map { ($0.id, $0) }, uniquingKeysWith: { current, _ in current })
        let byUUID = Dictionary(
            fallbackGames.compactMap { game in game.uuid.map { ($0, game) } },
            uniquingKeysWith: { current, _ in current }
        )
        return games.map { game in
            let fallback = byID[game.id] ?? game.uuid.flatMap { byUUID[$0] }
            return game.fillingMissingMetadata(from: fallback)
        }
    }

    func refreshAccountConnectors() async {
        guard let session = authSession else { return }
        accountError = nil
        let requestedUserId = session.user.userId
        loadingAccountConnectors = true
        defer {
            if accountIsCurrent(requestedUserId) {
                loadingAccountConnectors = false
            }
        }
        do {
            let refreshed = try await api.refreshSession(session)
            let connectors = try await api.fetchAccountConnectors(session: refreshed)
            guard accountIsCurrent(requestedUserId) else { return }
            authSession = refreshed
            user = refreshed.user
            persistAuthSession(refreshed)
            accountConnectors = connectors
            persistCachedAccount(for: refreshed)
            lastError = nil
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            guard accountIsCurrent(requestedUserId) else { return }
            hydrateCachedAccount(for: session, onlyMissing: true)
            lastError = "Failed to load account connections: \(OpenNOWErrorPresenter.message(for: error, fallback: "Store connections could not be refreshed."))"
            accountError = lastError
        }
    }

    func connectAccountConnector(_ connector: AccountConnector, openURL: @escaping (URL) -> Void) async {
        guard let session = authSession else { return }
        accountError = nil
        let requestedUserId = session.user.userId
        connectorActionStore = connector.store
        defer { connectorActionStore = nil }
        do {
            let refreshed = try await api.refreshSession(session)
            guard accountIsCurrent(requestedUserId) else { return }
            authSession = refreshed
            user = refreshed.user
            persistAuthSession(refreshed)
            let url = try await api.accountConnectorLoginURL(store: connector.store, session: refreshed)
            guard accountIsCurrent(requestedUserId) else { return }
            openURL(url)
            scheduleAccountConnectorRefreshAfterLinking()
            lastError = nil
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            guard accountIsCurrent(requestedUserId) else { return }
            lastError = "Failed to connect \(connector.label): \(OpenNOWErrorPresenter.message(for: error, fallback: "The store connection could not be started."))"
            accountError = lastError
        }
    }

    func disconnectAccountConnector(_ connector: AccountConnector) async {
        guard let session = authSession else { return }
        accountError = nil
        let requestedUserId = session.user.userId
        connectorActionStore = connector.store
        defer { connectorActionStore = nil }
        do {
            let refreshed = try await api.refreshSession(session)
            guard accountIsCurrent(requestedUserId) else { return }
            authSession = refreshed
            user = refreshed.user
            persistAuthSession(refreshed)
            try await api.disconnectAccountConnector(store: connector.store, session: refreshed)
            let connectors = try await api.fetchAccountConnectors(session: refreshed)
            guard accountIsCurrent(requestedUserId) else { return }
            accountConnectors = connectors
            persistCachedAccount(for: refreshed)
            lastError = nil
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            guard accountIsCurrent(requestedUserId) else { return }
            lastError = "Failed to disconnect \(connector.label): \(OpenNOWErrorPresenter.message(for: error, fallback: "The store connection could not be removed."))"
            accountError = lastError
        }
    }

    private func scheduleAccountConnectorRefreshAfterLinking() {
        Task { [weak self] in
            let delays: [UInt64] = [5, 10, 10, 10, 10, 10]
            for seconds in delays {
                try? await Task.sleep(for: .seconds(seconds))
                guard let self else { return }
                await self.refreshAccountConnectors()
            }
        }
    }

    func launch(
        game: CloudGame,
        zoneUrl: String? = nil,
        launchOption: GameLaunchOption? = nil,
        settingsOverride: AppSettings? = nil
    ) async {
        sessionError = nil
        guard supportsEmbeddedStreamer else {
            lastError = OpenNOWPlatform.streamingUnavailableReason
            sessionError = lastError
            return
        }
        var launchSettings = settingsOverride ?? settings
        launchSettings.normalizeStreamDefaults()
        launchSettings = nativeLaunchSettings(for: launchSettings, context: "launch")
        guard let session = authSession else {
            lastError = "Sign in first."
            sessionError = lastError
            return
        }
        let generation = authGeneration
        if let launchRestriction = launchRestrictionMessage(for: game) {
            lastError = launchRestriction
            return
        }
        if StreamZonePolicy.isBlocked(zoneUrl) {
            lastError = StreamZonePolicy.blockedZoneMessage
            return
        }
        isLaunchingSession = true
        showStreamLoading = true
        queueOverlayVisible = true
        defer { isLaunchingSession = false }
        do {
            logger.info("Launch requested game=\(game.title, privacy: .public) zoneUrl=\(zoneUrl ?? "default", privacy: .public)")
            let refreshed = try await api.refreshSession(session)
            try Task.checkCancellation()
            guard generation == authGeneration else { throw CancellationError() }
            authSession = refreshed
            user = refreshed.user
            persistAuthSession(refreshed)

            let effectiveLaunchOption = effectiveLaunchOption(for: game, requested: launchOption)
            let launchAppId = effectiveLaunchOption?.appId ?? game.launchAppId
            guard let launchAppId, !launchAppId.isEmpty else {
                throw NSError(
                    domain: "OpenNOW.Session",
                    code: 30,
                    userInfo: [NSLocalizedDescriptionKey: "Selected game has no launch app ID"]
                )
            }

            let deviceId = persistentDeviceId()
            let storedRegion = launchSettings.preferredRegion.trimmingCharacters(in: .whitespacesAndNewlines)
            let configuredRegion = StreamZonePolicy.isBlocked(storedRegion) ? "" : storedRegion
            let baseUrl = zoneUrl
                ?? (configuredRegion.isEmpty ? nil : configuredRegion)
                ?? refreshed.provider.streamingServiceUrl
            guard !StreamZonePolicy.isBlocked(baseUrl) else {
                throw NSError(
                    domain: "OpenNOW.StreamZonePolicy",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: StreamZonePolicy.blockedZoneMessage]
                )
            }
            let activeCandidates: [RemoteSessionCandidate]
            do {
                activeCandidates = try await api.fetchActiveSessions(
                    session: refreshed,
                    streamingBaseUrl: baseUrl,
                    vpcId: cachedVpcId,
                    settings: launchSettings,
                    deviceId: deviceId
                )
                remoteSessionsSnapshotLoaded = true
            } catch {
                activeCandidates = []
                remoteSessionsSnapshotLoaded = false
                logger.warning("Could not refresh active sessions before launch error=\(error.localizedDescription, privacy: .public)")
            }
            try Task.checkCancellation()
            guard generation == authGeneration else { throw CancellationError() }

            let compatibleCandidates = compatibleRemoteSessions(activeCandidates, settings: launchSettings, session: refreshed)
            resumableSessions = activeCandidates.filter {
                remoteSessionIsLaunchable($0) && remoteSessionIsAllowed($0)
            }
            let staleLaunchCandidate = activeCandidates.first {
                $0.appId == launchAppId
                    && remoteSessionIsAllowed($0)
                    && remoteSessionIsLaunchable($0)
                    && !remoteSession($0, matchesStreamSettings: launchSettings, session: refreshed)
            }
            let readyCandidate = compatibleCandidates.first {
                $0.appId == launchAppId && $0.serverIp != nil && ($0.status == 2 || $0.status == 3)
            }
            let launchingCandidate = compatibleCandidates.first {
                $0.appId == launchAppId && $0.status == 1
            } ?? compatibleCandidates.first {
                ($0.appId == nil || $0.appId?.isEmpty == true) && $0.status == 1
            }

            var started: ActiveSession
            if let readyCandidate {
                started = try await api.claimSession(
                    session: refreshed,
                    candidate: readyCandidate,
                    game: game,
                    streamingBaseUrl: baseUrl,
                    vpcId: cachedVpcId,
                    settings: launchSettings,
                    deviceId: deviceId
                )
            } else if let launchingCandidate {
                let pending = ActiveSession(
                    id: launchingCandidate.id,
                    game: game,
                    startedAt: .now,
                    status: launchingCandidate.status,
                    queuePosition: nil,
                    seatSetupStep: nil,
                    serverIp: launchingCandidate.serverIp,
                    mediaIp: nil,
                    mediaPort: 0,
                    signalingServer: nil,
                    signalingUrl: nil,
                    iceServers: [],
                    zone: cachedVpcId,
                    streamingBaseUrl: baseUrl,
                    clientId: UUID().uuidString,
                    deviceId: deviceId,
                    adState: nil
                )
                let hydrated = (try? await api.pollSession(
                    session: refreshed,
                    activeSession: pending,
                    settings: launchSettings
                )) ?? pending
                started = mergeQueueSessionState(previous: pending, next: hydrated)
            } else {
                if let staleLaunchCandidate {
                    do {
                        try await api.stopRemoteSession(
                            session: refreshed,
                            candidate: staleLaunchCandidate,
                            streamingBaseUrl: baseUrl,
                            vpcId: cachedVpcId
                        )
                        resumableSessions.removeAll { $0.id == staleLaunchCandidate.id }
                        logger.info("Stopped stale session before relaunch id=\(staleLaunchCandidate.id, privacy: .public)")
                    } catch {
                        logger.warning("Could not stop stale session before relaunch id=\(staleLaunchCandidate.id, privacy: .public) error=\(error.localizedDescription, privacy: .public)")
                    }
                }
                func startNewSession(using settings: AppSettings) async throws -> ActiveSession {
                    try await api.startSession(
                        session: refreshed,
                        game: game,
                        vpcId: cachedVpcId,
                        settings: settings,
                        streamProfile: StreamSettingsResolver.profile(
                            for: settings,
                            membershipTier: subscription?.membershipTier ?? user?.membershipTier
                        ),
                        streamingBaseUrl: baseUrl,
                        launchAppIdOverride: launchAppId,
                        launcherName: effectiveLaunchOption?.storefront ?? "Auto",
                        deviceId: deviceId,
                        accountLinked: shouldSendAccountLinked(game: game, launchOption: effectiveLaunchOption)
                    )
                }
                started = try await startNewSession(using: launchSettings)
            }
            try Task.checkCancellation()
            guard generation == authGeneration else { throw CancellationError() }
            activeSession = started
            activeStreamSettings = launchSettings
            adReportStateById = [:]
            adStartedAtById = [:]
                startSessionTasks()
            syncTrackedSessionSurface()
            logger.info("Session started id=\(started.id, privacy: .public) status=\(started.status) queue=\(started.queuePosition ?? -1)")
            lastError = nil
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            logger.error("Session launch failed error=\(error.localizedDescription, privacy: .public)")
            showStreamLoading = false
            queueOverlayVisible = false
            syncTrackedSessionSurface()
            report(error, context: .launch)
        }
    }

    func scheduleLaunch(game: CloudGame, zoneUrl: String? = nil, launchOption: GameLaunchOption? = nil) {
        let request = PendingLaunchRequest(game: game, zoneUrl: zoneUrl, launchOption: launchOption)
        // Starting a second game silently discards the rig the first one is holding, which on the
        // free tier also burns the hour. Ask first — this is the one confirmation in the launch
        // path that is worth the interruption.
        if let conflict = launchConflict(for: request) {
            pendingLaunchConflict = conflict
            return
        }
        performScheduledLaunch(request)
    }

    /// The live session a new launch would displace, if there is one.
    func launchConflict(for request: PendingLaunchRequest) -> LaunchConflict? {
        LaunchConflict.between(active: activeSession, request: request)
    }

    func confirmPendingLaunch() {
        guard let conflict = pendingLaunchConflict else { return }
        pendingLaunchConflict = nil
        launchTask?.cancel()
        launchTask = Task {
            await self.endSession()
            guard !Task.isCancelled else { return }
            await self.launch(
                game: conflict.request.game,
                zoneUrl: conflict.request.zoneUrl,
                launchOption: conflict.request.launchOption
            )
        }
    }

    func cancelPendingLaunch() {
        pendingLaunchConflict = nil
    }

    /// Records a failure in both channels at once.
    private func report(_ error: Error, context: OpenNOWFailure.Context) {
        let failure = OpenNOWFailure.classify(error, context: context)
        lastFailure = failure
        lastError = failure.message
        switch context {
        case .launch:
            catalogError = failure.message
            sessionError = failure.message
        case .session:
            sessionError = failure.message
        case .catalog:
            catalogError = failure.message
        case .account:
            accountError = failure.message
        }
        logger.error(
            "failure kind=\(failure.kind.rawValue, privacy: .public) context=\(String(describing: context), privacy: .public)"
        )
    }

    /// Debounced so a five-letter word is one request, not five.
    private func scheduleCatalogSearch() {
        catalogSearchTask?.cancel()
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)

        // Under three characters the server returns most of the catalog, which is slower and less
        // useful than the local list already on screen.
        guard query.count >= 3 else {
            remoteSearchResults = []
            isSearchingCatalog = false
            return
        }

        isSearchingCatalog = true
        catalogSearchTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 350 * NSEC_PER_MSEC)
            guard !Task.isCancelled, let self else { return }
            await self.runCatalogSearch(query: query)
        }
    }

    private func runCatalogSearch(query: String) async {
        defer { isSearchingCatalog = false }
        guard let session = authSession else { return }
        do {
            let results = try await api.searchCatalog(
                token: session.tokens.idToken ?? session.tokens.accessToken,
                vpcId: cachedVpcId,
                query: query
            )
            // The query may have moved on while the request was in flight.
            guard !Task.isCancelled,
                  searchText.trimmingCharacters(in: .whitespacesAndNewlines) == query else { return }
            remoteSearchResults = results
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            // A failed search is not worth a banner: the local results are still on screen and
            // still useful. It is logged for diagnostics and nothing else.
            logger.notice("catalog search failed query=\(query.count) chars error=\(error.localizedDescription, privacy: .public)")
        }
    }

    func clearFailure() {
        lastFailure = nil
        lastError = nil
        sessionError = nil
        catalogError = nil
    }

    func clearCatalogError() {
        catalogError = nil
        if lastFailure == nil { lastError = nil }
    }

    /// Runs the recovery the banner offered. The store owns what each one means, so the view
    /// never has to know how to retry a catalog load.
    func performRecovery(_ recovery: OpenNOWFailure.Recovery) {
        let context = lastFailure
        clearFailure()
        switch recovery {
        case .retry:
            Task { await refreshCatalog() }
        case .signIn:
            Task { await bootstrap() }
        case .changeServer:
            pendingSettingsRoute = .stream
        case .seePlans:
            pendingSettingsRoute = .account
        case .reportProblem, .none:
            _ = context
        }
    }

    private func performScheduledLaunch(_ request: PendingLaunchRequest) {
        launchTask?.cancel()
        launchTask = Task {
            await self.launch(
                game: request.game,
                zoneUrl: request.zoneUrl,
                launchOption: request.launchOption
            )
        }
    }

    func handleIncomingURL(_ url: URL) {
        guard Self.isOpenNOWDeepLink(url) else { return }
        let parts = Self.deepLinkParts(from: url)
        let action = parts.first?.lowercased() ?? ""
        let query = Self.deepLinkQueryItems(from: url)

        switch action {
        case "launch", "play":
            guard let appId = parts.dropFirst().first ?? query["appid"] ?? query["appId"] ?? query["id"],
                  !appId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                lastError = "Deep link launch needs an app ID."
                return
            }
            scheduleDeepLinkLaunch(
                appId: appId,
                storefront: query["store"] ?? query["launcher"],
                settingsOverride: deepLinkSettingsOverride(from: query)
            )
        case "resume", "continue", "jumpback":
            jumpBackToSession()
        case "refresh":
            launchTask?.cancel()
            launchTask = Task { await self.refreshCatalog() }
        case "stop", "terminate", "end":
            launchTask?.cancel()
            launchTask = Task { await self.terminateAllSessions(reason: "deepLink.stop") }
        default:
            if let appId = parts.first, appId.rangeOfCharacter(from: CharacterSet.alphanumerics.inverted) == nil {
                scheduleDeepLinkLaunch(
                    appId: appId,
                    storefront: query["store"] ?? query["launcher"],
                    settingsOverride: deepLinkSettingsOverride(from: query)
                )
            }
        }
    }

    private func scheduleDeepLinkLaunch(appId: String, storefront: String?, settingsOverride: AppSettings?) {
        launchTask?.cancel()
        launchTask = Task { [weak self] in
            guard let self else { return }
            if self.authSession == nil {
                self.lastError = "Sign in before launching from a link."
                return
            }
            if self.allGames.isEmpty {
                await self.refreshCatalog()
            }
            guard let target = self.deepLinkLaunchTarget(appId: appId, storefront: storefront) else {
                self.lastError = "No catalog entry matches app ID \(appId)."
                return
            }
            await self.launch(
                game: target.game,
                zoneUrl: nil,
                launchOption: target.launchOption,
                settingsOverride: settingsOverride
            )
        }
    }

    private func deepLinkSettingsOverride(from query: [String: String]) -> AppSettings? {
        var override = settings
        var hasOverride = false

        if let rawCodec = query["codec"] ?? query["videoCodec"],
           let codec = NativeStreamVideoCodec.normalized(rawCodec) {
            override.preferredCodec = codec.rawValue
            hasOverride = true
        }

        if let rawResolution = query["resolution"],
           let choice = StreamSettingsResolver.resolutionChoices.first(where: {
               $0.value.caseInsensitiveCompare(rawResolution.trimmingCharacters(in: .whitespacesAndNewlines)) == .orderedSame
           }) {
            override.streamPreset = .custom
            override.preferredAspectRatio = choice.aspectRatio
            override.preferredResolution = choice.value
            hasOverride = true
        }

        if let rawFPS = query["fps"],
           let fps = Int(rawFPS),
           [30, 60, 120].contains(fps) {
            override.streamPreset = .custom
            override.preferredFPS = fps
            hasOverride = true
        }

        guard hasOverride else { return nil }
        override.normalizeStreamDefaults()
        return override
    }

    private func deepLinkLaunchTarget(appId: String, storefront: String?) -> (game: CloudGame, launchOption: GameLaunchOption?)? {
        let normalizedAppId = appId.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedStore = storefront.map(normalizeGameStore)
        var seenGameIds = Set<String>()
        for game in allGames + libraryGames + featuredGames {
            guard seenGameIds.insert(game.id).inserted else { continue }
            let matchingOption = game.launchOptions.first {
                $0.appId.caseInsensitiveCompare(normalizedAppId) == .orderedSame
                    && (normalizedStore == nil || normalizeGameStore($0.storefront) == normalizedStore)
            }
            if let matchingOption {
                return (game, matchingOption)
            }
            if game.launchAppId?.caseInsensitiveCompare(normalizedAppId) == .orderedSame || game.id.caseInsensitiveCompare(normalizedAppId) == .orderedSame {
                return (game, nil)
            }
        }
        return nil
    }

    private static func isOpenNOWDeepLink(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased() else { return false }
        return scheme == "opennow" || scheme == "opennowios"
    }

    private static func deepLinkParts(from url: URL) -> [String] {
        var parts: [String] = []
        if let host = url.host(percentEncoded: false), !host.isEmpty {
            parts.append(host)
        }
        parts.append(contentsOf: url.pathComponents.filter { $0 != "/" })
        return parts.map { $0.removingPercentEncoding ?? $0 }
    }

    private static func deepLinkQueryItems(from url: URL) -> [String: String] {
        guard let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else { return [:] }
        var result: [String: String] = [:]
        for item in items {
            result[item.name] = item.value
        }
        return result
    }

    func refreshRemoteSessions() async {
        guard let session = authSession else { return }
        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            let streamSettings = nativeLaunchSettings(for: activeStreamSettings ?? settings, context: "refreshRemoteSessions")
            let remoteSessions = try await api.fetchActiveSessions(
                session: refreshed,
                streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                vpcId: cachedVpcId,
                settings: streamSettings,
                deviceId: persistentDeviceId()
            )
            resumableSessions = remoteSessions.filter {
                remoteSessionIsLaunchable($0) && remoteSessionIsAllowed($0)
            }
            remoteSessionsSnapshotLoaded = true
        } catch {
            remoteSessionsSnapshotLoaded = false
            report(error, context: .session)
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
            report(error, context: .session)
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
        guard remoteSessionIsAllowed(candidate) else {
            lastError = StreamZonePolicy.blockedZoneMessage
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
            let requestedSettings = nativeLaunchSettings(for: settings, context: "resumeSession")
            let streamSettings = streamSettingsByAdoptingRemoteProfile(
                candidate,
                base: requestedSettings,
                session: refreshed
            )
            let claimed = try await api.claimSession(
                session: refreshed,
                candidate: candidate,
                game: game,
                streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                vpcId: cachedVpcId,
                settings: streamSettings,
                deviceId: persistentDeviceId()
            )
            activeSession = claimed
            activeStreamSettings = streamSettings
            adReportStateById = [:]
            adStartedAtById = [:]
                startSessionTasks()
            syncTrackedSessionSurface()
            logger.info("Session resumed id=\(claimed.id, privacy: .public) status=\(claimed.status) queue=\(claimed.queuePosition ?? -1)")
            lastError = nil
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            logger.error("Resume session failed error=\(error.localizedDescription, privacy: .public)")
            showStreamLoading = false
            queueOverlayVisible = false
            syncTrackedSessionSurface()
            report(error, context: .session)
        }
    }

    func scheduleResume(candidate: RemoteSessionCandidate) {
        launchTask?.cancel()
        launchTask = Task { await self.resumeSession(candidate: candidate) }
    }

    var primaryRemoteJumpBackSession: RemoteSessionCandidate? {
        let activeId = activeSession?.id
        return resumableSessions.first {
            $0.id != activeId && ($0.status == 1 || $0.status == 2 || $0.status == 3)
        }
    }

    var canJumpBackToSession: Bool {
        activeSession != nil || primaryRemoteJumpBackSession != nil
    }

    func jumpBackToSession() {
        if let active = activeSession {
            restoreActiveSessionSurface(active)
            return
        }

        if let candidate = primaryRemoteJumpBackSession {
            scheduleResume(candidate: candidate)
            return
        }

        launchTask?.cancel()
        launchTask = Task { [weak self] in
            guard let self else { return }
            await self.refreshRemoteSessions()
            if let candidate = self.primaryRemoteJumpBackSession {
                self.scheduleResume(candidate: candidate)
            }
        }
    }

    func endSession() async {
        launchTask?.cancel()
        launchTask = nil
        showStreamLoading = false
        queueOverlayVisible = false
        await NotificationManager.shared.cancelSessionNotifications()
        guard let session = authSession, let active = activeSession else {
            clearLocalSessionState(reason: "endSession.noActiveSession")
            return
        }
        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            try await api.stopSession(session: refreshed, activeSession: active)
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            // The session is being torn down either way; a cancelled stop is not a failure.
        } catch {
            lastError = "Stop session failed: \(OpenNOWErrorPresenter.message(for: error, fallback: "The session could not be stopped."))"
        }
        clearLocalSessionState(reason: "endSession.completed")
        resumableSessions.removeAll { $0.id == active.id }
    }

    private func terminateAllSessions(reason: String) async {
        showStreamLoading = false
        queueOverlayVisible = false
        await NotificationManager.shared.cancelSessionNotifications()

        guard let session = authSession else {
            clearLocalSessionState(reason: reason)
            return
        }

        var failures: [String] = []
        do {
            let refreshed = try await api.refreshSession(session)
            authSession = refreshed
            persistAuthSession(refreshed)
            let activeToStop = activeSession
            if let activeToStop {
                do {
                    try await api.stopSession(session: refreshed, activeSession: activeToStop)
                } catch {
                    failures.append(activeToStop.id)
                }
            }

            let remoteSessions = (try? await api.fetchActiveSessions(
                session: refreshed,
                streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                vpcId: cachedVpcId,
                settings: settings,
                deviceId: persistentDeviceId()
            )) ?? []
            for candidate in remoteSessions where candidate.id != activeToStop?.id {
                do {
                    try await api.stopRemoteSession(
                        session: refreshed,
                        candidate: candidate,
                        streamingBaseUrl: refreshed.provider.streamingServiceUrl,
                        vpcId: cachedVpcId
                    )
                } catch {
                    failures.append(candidate.id)
                }
            }
        } catch {
            failures.append("refresh")
        }

        clearLocalSessionState(reason: reason)
        resumableSessions = []
        lastError = failures.isEmpty ? nil : "Could not stop every remote session: \(failures.joined(separator: ", "))"
    }

    private func clearLocalSessionState(reason: String) {
        finalizeSessionReport(reason: reason)
        if let id = activeSession?.id { QueueReadyAlert.forget(sessionId: id) }
        activeSession = nil
        activeStreamSettings = nil
        setStreamSession(nil, reason: reason)
        adReportStateById = [:]
        adStartedAtById = [:]
        sessionPollTask?.cancel()
        endSessionPollBackgroundTask()
        syncTrackedSessionSurface()
    }

    func minimizeQueueOverlay() {
        withAnimation(.easeInOut(duration: 0.32)) {
            queueOverlayVisible = false
        }
    }

    func maximizeQueueOverlay() {
        guard let active = activeSession else { return }
        restoreActiveSessionSurface(active)
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
        let completedAdDurationMs = activeQueueAd?.adLengthInSeconds.map { Int(round($0 * 1000)) }
            ?? activeQueueAd?.durationMs
        adReportStateById[adId] = .finish
        if var active = activeSession {
            active.adState = removeSessionAdItem(active.adState, adId: adId)
            activeSession = active
            syncTrackedSessionSurface()
        }
        Task {
            await reportQueueAdAction(
                adId: adId,
                action: .finish,
                watchedTimeInMs: max(0, watchedTimeInMs),
                completedAdDurationMs: completedAdDurationMs
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
        finalizeSessionReport(reason: "dismissStreamer")
        setStreamSession(nil, reason: "dismissStreamer")
        // Note: poll task stays cancelled (stopped in handoff).
        // The queue banner reflects last known activeSession state.
        // User can reopen via the banner tap.
    }

    // MARK: - Session report
    //
    // The accumulator is a plain object rather than published state: it is written to about once a
    // second for as long as the session lasts, and invalidating the view hierarchy at that rate
    // over live video is exactly the cost the HUD is designed to avoid.

    /// Begins collecting telemetry for a stream. Safe to call again for the same session — a
    /// reconnect must not throw away the measurements taken before it.
    func beginSessionReport(for session: ActiveSession) {
        guard sessionReportSessionId != session.id else { return }
        sessionReportSessionId = session.id
        let settings = currentStreamerSettings
        let tier = subscription?.membershipTier ?? user?.membershipTier
        let eligible = StreamSettingsResolver.profile(for: settings, membershipTier: tier)
        sessionReportAccumulator = StreamSessionReportAccumulator(
            launchProfile: StreamReportLaunchProfile(
                gameTitle: session.game.title,
                selectedProfile: StreamSettingsResolver.profile(for: settings, membershipTier: nil),
                eligibleProfile: eligible,
                initialProfile: eligible,
                requestedCodec: settings.preferredCodec,
                eligibleCodec: settings.preferredCodec,
                hdrRequested: settings.hdrEnabled
            )
        )
    }

    func recordStreamRuntimeSample(_ sample: StreamRuntimeSample) {
        sessionReportAccumulator?.record(sample)
    }

    func recordStreamRecovery(reason: String) {
        let settings = currentStreamerSettings
        let tier = subscription?.membershipTier ?? user?.membershipTier
        sessionReportAccumulator?.recordRecovery(
            reason: reason,
            profile: StreamSettingsResolver.profile(for: settings, membershipTier: tier),
            codec: settings.preferredCodec
        )
    }

    /// Turns whatever was collected into a report. Produces nothing when the session never
    /// delivered a measurable frame — an empty report is worse than no report.
    func finalizeSessionReport(reason: String) {
        guard let accumulator = sessionReportAccumulator else { return }
        sessionReportAccumulator = nil
        sessionReportSessionId = nil
        guard settings.showSessionReportAfterStream, let report = accumulator.finish() else { return }
        logger.notice(
            "session report reason=\(reason, privacy: .public) score=\(report.score) samples=\(report.sampleCount)"
        )
        sessionReport = report
    }

    /// Persists a settings change made from inside the stream.
    ///
    /// Only the fields the control panel can actually reach are copied across. Taking the whole
    /// object would let a stale snapshot — the streamer holds one from when the session started —
    /// silently revert anything changed in Settings while the game was running.
    func applyStreamerSettings(_ updated: AppSettings) {
        var next = settings
        next.streamStatsMetrics = updated.streamStatsMetrics
        next.metal4Enabled = updated.metal4Enabled
        next.metalFXUpscalingEnabled = updated.metalFXUpscalingEnabled
        next.touch = updated.touch
        next.mouseSensitivity = updated.mouseSensitivity
        next.mouseScrollSensitivity = updated.mouseScrollSensitivity
        next.controllerMouseEmulation = updated.controllerMouseEmulation
        next.streamKeyboardClearConfirmationDisabled = updated.streamKeyboardClearConfirmationDisabled
        guard next != settings else { return }
        settings = next
        persistSettings()
    }

    // MARK: - Report a problem

    /// Everything the app already knows that a maintainer would otherwise have to ask for.
    ///
    /// Collected at the moment the form opens rather than at submit, so the numbers describe the
    /// session the user is complaining about and not the idle state they returned to.
    func bugReportPreflightDeck() -> BugReportPreflightDeck {
        var items: [BugReportPreflightItem] = []

        #if canImport(UIKit)
        let device = UIDevice.current
        items.append(BugReportPreflightItem(
            label: "Device",
            value: "\(OpenNOWPlatform.modelIdentifier), \(device.systemName) \(device.systemVersion)"
        ))
        #endif

        items.append(BugReportPreflightItem(
            label: "App",
            value: "\(Self.appVersionName) (\(Self.appVersionCode))"
        ))

        let profile = StreamSettingsResolver.profile(
            for: currentStreamerSettings,
            membershipTier: subscription?.membershipTier ?? user?.membershipTier
        )
        items.append(BugReportPreflightItem(
            label: "Stream",
            value: "\(profile.resolutionString) · \(profile.fps) fps · \(currentStreamerSettings.preferredCodec)"
        ))

        if let session = activeSession ?? streamSession {
            items.append(BugReportPreflightItem(label: "Game", value: session.game.title))
            if !session.zone.isEmpty {
                items.append(BugReportPreflightItem(label: "Server", value: session.zone))
            }
        }

        if let report = sessionReport {
            items.append(BugReportPreflightItem(
                label: "Last session",
                value: "\(report.durationSeconds / 60) min, score \(report.score)"
            ))
        }

        let codecReport = NativeStreamCodecProbe.report()
        let codecSummary = NativeStreamVideoCodec.allCases
            .map { codec in
                let usable = codecReport.capability(for: codec)?.launchSafe == true
                return "\(codec.rawValue.uppercased()) \(usable ? "✓" : "✗")"
            }
            .joined(separator: " ")
        items.append(BugReportPreflightItem(label: "Decoders", value: codecSummary))

        items.append(BugReportPreflightItem(
            label: "Debug log",
            value: "Redacted diagnostics attached"
        ))

        items.append(BugReportPreflightItem(
            label: "Membership",
            value: subscription?.membershipTier ?? user?.membershipTier ?? "Unknown"
        ))

        var deck = BugReportPreflightDeck(items: items)
        if let known = knownIssueMatch(profile: profile, report: sessionReport) {
            deck.items.append(known)
        }
        return deck
    }

    /// A short, hand-maintained list of things already on the board. Matching one does not block
    /// the report — it tells the user before they spend time writing, and lets them say "no, mine
    /// is different", which is the signal worth capturing.
    private func knownIssueMatch(
        profile: StreamVideoProfile,
        report: SessionReport?
    ) -> BugReportPreflightItem? {
        if let report, report.deliveredResolution != nil,
           report.deliveredResolution != report.requestedResolution,
           (report.packetLossPercent ?? 0) > 1.0 {
            return BugReportPreflightItem(
                label: "Known issue",
                value: "Resolution drops on a weak connection",
                kind: .knownIssue(key: "ios-resolution-fallback-weak-link")
            )
        }
        if currentStreamerSettings.hdrEnabled,
           NativeStreamCodecProbe.report().capability(for: .h265)?.launchSafe != true {
            return BugReportPreflightItem(
                label: "Known issue",
                value: "HDR requested without a usable H.265 decoder",
                kind: .knownIssue(key: "ios-hdr-without-hevc")
            )
        }
        return nil
    }

    func submitBugReport(_ draft: BugReportDraft, deck: BugReportPreflightDeck) async -> Result<String, Error> {
        guard let reporterId = BugReportReporter.reporterId(stableDeviceId: persistentDeviceId()) else {
            return .failure(BugReportError.invalid("This installation has no reporting ID yet. Restart OpenNOW and try again."))
        }
        let diagnostics = await makeDiagnosticsExport()
        let diagnosticsAttachment = BugReportAttachment(
            fileName: "\(diagnosticsExportFileName).txt",
            contentType: "text/plain; charset=utf-8",
            data: Data(diagnostics.utf8)
        )
        let submission = BugReportSubmission(
            title: draft.title,
            description: draft.description,
            versionName: Self.appVersionName,
            versionCode: Self.appVersionCode,
            reporterId: reporterId,
            metadata: BugReportMetadata.build(deck: deck, overridesKnownIssue: draft.overridesKnownIssue),
            attachments: [diagnosticsAttachment]
        )
        do {
            let reference = try await BugReportClient.upload(submission)
            logger.notice("bug report accepted reference=\(reference, privacy: .public)")
            return .success(reference)
        } catch {
            logger.error("bug report failed error=\(error.localizedDescription, privacy: .public)")
            return .failure(error)
        }
    }

    static var appVersionName: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0"
    }

    static var appVersionCode: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "1"
    }

    /// Records the user's answer to the one-time analytics prompt. Both answers are terminal —
    /// the prompt never reappears, which is the only reason it is acceptable to block on it.
    func recordAnalyticsConsent(sharing: Bool) {
        settings.analyticsConsentAsked = true
        settings.analyticsOptOut = !sharing
        persistSettings()
    }

    func dismissSessionReport(disableFutureReports: Bool = false) {
        if disableFutureReports {
            settings.showSessionReportAfterStream = false
            persistSettings()
        }
        sessionReport = nil
    }

    private func restoreActiveSessionSurface(_ session: ActiveSession) {
        guard activeSession?.id == session.id else { return }
        if canReopenStreamer {
            reopenStreamer()
            return
        }
        launchTask?.cancel()
        withAnimation(.easeInOut(duration: 0.32)) {
            showStreamLoading = true
            queueOverlayVisible = true
        }
        if sessionPollTask == nil {
            startSessionTasks()
        } else {
            syncTrackedSessionSurface()
        }
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
        if phase == .active {
            sessionPollBackgroundAllowanceConsumed = false
            endSessionPollBackgroundTask()
        } else if activeSession != nil {
            refreshSessionPollBackgroundTask()
        }
        syncTrackedSessionSurface()
    }

    func persistSettings() {
        var normalized = settings
        normalized.normalizeStreamDefaults()
        if normalized != settings {
            settings = normalized
        }
        if let encoded = try? JSONEncoder().encode(normalized) {
            defaults.set(encoded, forKey: settingsKey)
        }
    }

    func setStreamColor(_ color: StreamColorQuality) {
        saveVideoSelection(StreamSettingsResolver.selectingColor(color,in:settings))
    }

    func setStreamHDR(_ enabled: Bool) {
        let h265Available = enabled && NativeStreamCodecProbe.report().capability(for:.h265)?.launchSafe == true
        saveVideoSelection(StreamSettingsResolver.selectingHDR(enabled,in:settings,h265Available:h265Available))
    }

    private func saveVideoSelection(_ updated: AppSettings) {
        guard updated != settings else { return }
        // Publish the complete choice once and persist before leaving the control.
        // The active session keeps its allocation snapshot until a fresh launch.
        settings = updated
        persistSettings()
    }

    func refreshTrackedSessionSurface() {
        syncTrackedSessionSurface()
    }

    func isFavorite(_ game: CloudGame) -> Bool {
        settings.favoriteGameIds.contains(game.id)
    }

    func toggleFavorite(_ game: CloudGame) {
        if let index = settings.favoriteGameIds.firstIndex(of: game.id) {
            settings.favoriteGameIds.remove(at: index)
        } else {
            settings.favoriteGameIds.append(game.id)
        }
        persistSettings()
    }

    func launchOptions(for game: CloudGame) -> [GameLaunchOption] {
        let options = game.launchOptions.filter { !$0.appId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        if !options.isEmpty {
            return options
        }
        if let launchAppId = game.launchAppId, !launchAppId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return [GameLaunchOption(storefront: game.platform.isEmpty ? "Auto" : game.platform, appId: launchAppId, supportedControls: nil)]
        }
        return []
    }

    func defaultLaunchOption(for game: CloudGame) -> GameLaunchOption? {
        guard let defaultId = settings.defaultGameVariantIds[game.id] else { return nil }
        return launchOptions(for: game).first { $0.id == defaultId || $0.appId == defaultId }
    }

    func setDefaultGameVariant(game: CloudGame, option: GameLaunchOption?) {
        if let option {
            settings.defaultGameVariantIds[game.id] = option.id
        } else {
            settings.defaultGameVariantIds.removeValue(forKey: game.id)
        }
        persistSettings()
    }

    func launchRestrictionMessage(for game: CloudGame) -> String? {
        guard membershipTierRequiresPaid(game.membershipTierLabel), isFreeTierUser else {
            return nil
        }
        let requiredTier = game.membershipTierLabel.map(gameMetadataDisplayLabel) ?? "Premium"
        let article = requiredTier.first.map { "AEIOU".contains(String($0).uppercased()) ? "an" : "a" } ?? "a"
        return "\(game.title) requires \(article) \(requiredTier) GeForce NOW membership."
    }

    func clearFavorites() {
        settings.favoriteGameIds.removeAll()
        persistSettings()
    }

    func clearDefaultGameVariants() {
        settings.defaultGameVariantIds.removeAll()
        persistSettings()
    }

    func resetSettings() {
        settings = .default
        settings.normalizeStreamDefaults()
        persistSettings()
        CatalogWallpaperStorage.pruneManagedWallpapers(keeping: nil)
        refreshTrackedSessionSurface()
    }

    func resetApp() {
        Task { await NotificationManager.shared.cancelSessionNotifications() }
        launchTask?.cancel()
        launchTask = nil
        sessionPollTask?.cancel()
        endSessionPollBackgroundTask()
        clearImageCache()

        persistAuthState(PersistedAuthState())
        defaults.removeObject(forKey: authStateKey)
        defaults.removeObject(forKey: authSessionKey)
        defaults.removeObject(forKey: activeSessionSnapshotKey)
        defaults.removeObject(forKey: activeStreamSettingsKey)
        defaults.removeObject(forKey: deviceIdKey)
        removeAllCachedCatalog()
        removeAllCachedAccounts()
        cancelAccountRefresh()

        authSession = nil
        user = nil
        savedAccounts = []
        searchText = ""
        isAuthenticating = false
        isLoadingGames = false
        isLaunchingSession = false
        telemetry = SessionTelemetry(pingMs: 0, fps: 0, packetLossPercent: 0, bitrateMbps: 0)
        lastError = nil
        cachedVpcId = "GFN-PC"
        settings = .default
        settings.normalizeStreamDefaults()
        persistSettings()
        CatalogWallpaperStorage.pruneManagedWallpapers(keeping: nil)
        clearAccountScopedState()
    }

    func clearImageCache() {
        OpenNOWImageCache.shared.removeAll()
        OpenNOWImageCache.removeAllPersistentImages()
        URLCache.shared.removeAllCachedResponses()
    }

    func updateTouchControlLayout(_ layout: TouchControlLayout, profile: String) {
        settings.touchControlLayouts[profile] = layout
        persistSettings()
    }

    func updateStreamerPreferences(_ preferences: StreamerPreferences) {
        settings.streamerPreferences = preferences
        if activeStreamSettings != nil {
            activeStreamSettings?.streamerPreferences = preferences
            syncTrackedSessionSurface()
        }
        persistSettings()
    }

    func setStreamTutorialCompleted(_ completed: Bool) {
        let activeStreamNeedsUpdate = activeStreamSettings.map { $0.streamTutorialCompleted != completed } ?? false
        guard settings.streamTutorialCompleted != completed || activeStreamNeedsUpdate else {
            return
        }
        settings.streamTutorialCompleted = completed
        if activeStreamSettings != nil {
            activeStreamSettings?.streamTutorialCompleted = completed
            syncTrackedSessionSurface()
        }
        persistSettings()
    }

    func setControllerTouchPromptDismissed(_ dismissed: Bool) {
        let activeStreamNeedsUpdate = activeStreamSettings.map { $0.controllerTouchPromptDismissed != dismissed } ?? false
        guard settings.controllerTouchPromptDismissed != dismissed || activeStreamNeedsUpdate else {
            return
        }
        settings.controllerTouchPromptDismissed = dismissed
        if activeStreamSettings != nil {
            activeStreamSettings?.controllerTouchPromptDismissed = dismissed
            syncTrackedSessionSurface()
        }
        persistSettings()
    }

    func updateStreamSharpening(enabled: Bool, amount: Double) {
        let normalizedAmount = min(max(amount, 0), 1)
        settings.streamSharpeningEnabled = enabled
        settings.streamSharpeningAmount = normalizedAmount
        if activeStreamSettings != nil {
            activeStreamSettings?.streamSharpeningEnabled = enabled
            activeStreamSettings?.streamSharpeningAmount = normalizedAmount
            syncTrackedSessionSurface()
        }
        persistSettings()
    }

    func updateFingerMouseEnabled(_ enabled: Bool) {
        let activeStreamNeedsUpdate = activeStreamSettings.map { $0.fingerMouseEnabled != enabled } ?? false
        guard settings.fingerMouseEnabled != enabled || activeStreamNeedsUpdate else {
            return
        }
        settings.fingerMouseEnabled = enabled
        if activeStreamSettings != nil {
            activeStreamSettings?.fingerMouseEnabled = enabled
            syncTrackedSessionSurface()
        }
        persistSettings()
    }

    func updatePhoneRumbleFallback(_ enabled: Bool) {
        let activeStreamNeedsUpdate = activeStreamSettings.map { $0.phoneRumbleFallback != enabled } ?? false
        guard settings.phoneRumbleFallback != enabled || activeStreamNeedsUpdate else {
            return
        }
        settings.phoneRumbleFallback = enabled
        if activeStreamSettings != nil {
            activeStreamSettings?.phoneRumbleFallback = enabled
            syncTrackedSessionSurface()
        }
        persistSettings()
    }

    func updateStreamStatsOverlayVisible(_ visible: Bool) {
        settings.showStatsOverlay = visible
        if activeStreamSettings != nil {
            activeStreamSettings?.showStatsOverlay = visible
            syncTrackedSessionSurface()
        }
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
        guard authProviderCode?.caseInsensitiveCompare("NVIDIA") == .orderedSame,
              isFreeTierUser,
              let host = effectiveProvider.flatMap({ URL(string: $0.streamingServiceUrl)?.host })?.lowercased() else {
            return false
        }
        return host.hasSuffix(".nvidiagrid.net")
    }

    var shouldPresentPrintedWasteQueue: Bool {
        shouldUsePrintedWasteQueue && !settings.hideServerSelector
    }

    /// Seconds since the active session started, or nil when there is no session.
    var sessionElapsedSeconds: Int? {
        activeSession.map { max(0, Int(Date().timeIntervalSince($0.startedAt))) }
    }

    func formattedSessionElapsed() -> String {
        Self.elapsedLabel(sessionElapsedSeconds ?? 0)
    }

    /// One owner for H:MM:SS, rather than the same arithmetic written out at each surface.
    static func elapsedLabel(_ seconds: Int) -> String {
        let hours = seconds / 3_600
        let minutes = (seconds % 3_600) / 60
        let remainingSeconds = seconds % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, remainingSeconds)
        }
        return String(format: "%02d:%02d", minutes, remainingSeconds)
    }

    /// Local matches first, then anything only the server knew about.
    ///
    /// Local results appear instantly as you type; the server's arrive a moment later and fill in
    /// the long tail. Ordering them this way means the list never reshuffles under a thumb that
    /// is already reaching for a result.
    var filteredCatalogGames: [CloudGame] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        let local = allGames.filter { gameMatchesCatalogSearch($0, query: query) }
        guard !query.isEmpty, !remoteSearchResults.isEmpty else { return local }
        var seen = Set(local.map { catalogStableGameKey($0) })
        let extra = remoteSearchResults.filter { seen.insert(catalogStableGameKey($0)).inserted }
        return local + extra
    }

    var favoriteGames: [CloudGame] {
        guard !settings.favoriteGameIds.isEmpty else { return [] }
        var gamesById: [String: CloudGame] = [:]
        for game in allGames + libraryGames {
            gamesById[game.id] = game
        }
        return settings.favoriteGameIds.compactMap { gamesById[$0] }
    }

    private func effectiveLaunchOption(for game: CloudGame, requested: GameLaunchOption?) -> GameLaunchOption? {
        if let requested {
            return requested
        }
        return defaultLaunchOption(for: game)
    }

    private func shouldSendAccountLinked(game: CloudGame, launchOption: GameLaunchOption?) -> Bool {
        let playType = game.playType?.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        if playType == "INSTALL_TO_PLAY" {
            return false
        }
        let launcher = launchOption?.storefront.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        if launcher == "XBOX" || launcher == "MICROSOFT" {
            return false
        }
        return true
    }

    private func compatibleRemoteSessions(
        _ candidates: [RemoteSessionCandidate],
        settings: AppSettings,
        session: AuthSession
    ) -> [RemoteSessionCandidate] {
        candidates.filter {
            remoteSessionIsAllowed($0)
                && remoteSession($0, matchesStreamSettings: settings, session: session)
        }
    }

    private func remoteSessionIsAllowed(_ candidate: RemoteSessionCandidate) -> Bool {
        !StreamZonePolicy.isBlocked(candidate.serverIp)
    }

    private func remoteSession(
        _ candidate: RemoteSessionCandidate,
        matchesStreamSettings settings: AppSettings,
        session: AuthSession
    ) -> Bool {
        let profile = StreamSettingsResolver.profile(
            for: settings,
            membershipTier: subscription?.membershipTier ?? session.user.membershipTier
        )
        let expectedSignature = StreamSettingsResolver.sessionSignature(for: settings, profile: profile)
        let expectedResolution = "\(profile.width)x\(profile.height)"
        guard candidate.streamSettingsSignature?.trimmingCharacters(in: .whitespacesAndNewlines) == expectedSignature else {
            return false
        }
        guard candidate.resolution?.trimmingCharacters(in: .whitespacesAndNewlines) == expectedResolution else {
            return false
        }
        // A signature describes the request; the finalized host format may have
        // downgraded it. Reusing that seat perpetuates 8-bit SDR after requesting HDR.
        guard StreamSettingsResolver.remoteColorMatches(color: candidate.colorQuality,
            hdr: candidate.hdrEnabled, settings: settings) else { return false }
        return candidate.fps == profile.fps
    }

    private func streamSettingsByAdoptingRemoteProfile(
        _ candidate: RemoteSessionCandidate,
        base: AppSettings,
        session: AuthSession
    ) -> AppSettings {
        var adopted = base
        let membershipTier = subscription?.membershipTier ?? session.user.membershipTier

        if let resolution = candidate.resolution?.trimmingCharacters(in: .whitespacesAndNewlines),
           !resolution.isEmpty {
            let parts = resolution.split(separator: "x", maxSplits: 1)
            if parts.count == 2,
               let width = Int(parts[0]),
               let height = Int(parts[1]),
               StreamSettingsResolver.customResolutionIsAvailable(
                width: width,
                height: height,
                membershipTier: membershipTier
               ) {
                adopted.preferredResolution = "\(width)x\(height)"
                if let known = StreamSettingsResolver.resolutionChoices.first(where: { $0.value == adopted.preferredResolution }) {
                    adopted.preferredAspectRatio = known.aspectRatio
                }
            }
        }

        if let fps = candidate.fps {
            let planLimit = StreamSettingsResolver.plan(for: membershipTier) >= .ultimate ? 120 : 60
            adopted.preferredFPS = min(max(fps, 30), planLimit)
        }
        adopted.streamPreset = .custom
        adopted.normalizeStreamDefaults()
        return nativeLaunchSettings(for: adopted, context: "adoptRemoteProfile")
    }

    private func remoteSessionIsLaunchable(_ candidate: RemoteSessionCandidate) -> Bool {
        candidate.status == 1 || candidate.status == 2 || candidate.status == 3
    }

    private func startSessionTasks() {
        recordDebugEvent("queue", "Starting session poll status=\(activeSession?.status ?? -1) queue=\(activeSession?.queuePosition ?? -1)")
        setStreamSession(nil, reason: "startSessionTasks.reset")
        if activeSession != nil, activeStreamSettings == nil {
            activeStreamSettings = settings
        }
        reopenToken = UUID()
        sessionPollTask?.cancel()
        endSessionPollBackgroundTask()

        telemetry = SessionTelemetry(pingMs: 0, fps: 0, packetLossPercent: 0, bitrateMbps: 0)
        // No 1 Hz ticker here any more. `sessionElapsedSeconds` was `@Published` and reassigned
        // every second for the whole life of a session, which published a change through the store
        // to *every* observing view — the catalog and the queue screen included — so that one
        // `LabeledContent` on a Settings page nobody was looking at could count up. Elapsed time is
        // derived from `activeSession.startedAt` on demand instead, and the surfaces that show it
        // ticking use `Text(_:style: .timer)`, which the system animates locally.

        sessionPollTask = Task { [weak self] in
            guard let self else { return }
            let generation = self.authGeneration
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
                    try Task.checkCancellation()
                    guard generation == self.authGeneration else { break }
                    self.authSession = refreshed
                    self.persistAuthSession(refreshed)
                    let polled = try await self.api.pollSession(
                        session: refreshed,
                        activeSession: active,
                        settings: self.currentStreamerSettings
                    )
                    try Task.checkCancellation()
                    guard generation == self.authGeneration else { break }
                    consecutivePollFailures = 0
                    if self.sessionError?.hasPrefix("Session poll failed:") == true {
                        self.sessionError = nil
                    }
                    self.activeSession = polled
                    if polled.status != active.status || polled.queuePosition != active.queuePosition
                        || polled.sessionControlBaseUrl != active.sessionControlBaseUrl {
                        self.recordDebugEvent("queue", "Session updated status=\(polled.status) queue=\(polled.queuePosition ?? -1) control=\(URL(string: polled.sessionControlBaseUrl ?? "")?.host ?? "zone")")
                    }
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
                            self.sessionError = self.lastError
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
                        self.recordDebugEvent("stream", "Session ready for streamer handoff status=\(handoffSession.status)")
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
                            self.sessionError = self.lastError
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
                } catch where OpenNOWErrorPresenter.isCancellation(error) {
                    // The loop is being torn down, or a poll was superseded. Neither is a failure,
                    // and both used to end the session on a banner reading "cancelled".
                    break
                } catch where (error as NSError).domain == "OpenNOW.Queue" {
                    self.logger.notice("CloudMatch ended the queue request. Clearing the local session.")
                    self.clearLocalSessionState(reason: "queue.abandoned")
                    self.showStreamLoading = false
                    self.queueOverlayVisible = false
                    self.resumableSessions.removeAll { $0.id == active.id }
                    self.lastError = error.localizedDescription
                    self.catalogError = self.lastError
                    self.recordDebugEvent("queue", "Provider ended queue request")
                    await NotificationManager.shared.cancelSessionNotifications()
                    break
                } catch {
                    consecutivePollFailures += 1
                    self.logger.error("Session poll failed attempt=\(consecutivePollFailures) error=\(error.localizedDescription, privacy: .public)")
                    self.recordDebugEvent("queue", "Poll failed attempt=\(consecutivePollFailures) error=\(error.localizedDescription)")
                    // One dropped poll is normal on a phone changing networks. Only say so once the
                    // failures are consistent enough to mean something.
                    if consecutivePollFailures >= 3 {
                        self.lastError = "Session poll failed: \(OpenNOWErrorPresenter.message(for: error, fallback: "The session status could not be refreshed."))"
                        self.sessionError = self.lastError
                    }
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
        guard !StreamZonePolicy.isBlocked(session.streamingBaseUrl),
              !StreamZonePolicy.isBlocked(session.serverIp),
              !StreamZonePolicy.isBlocked(session.zone) else {
            return false
        }
        // Match desktop behavior: allow connect on status 2 or 3.
        // Keep signaling non-empty checks below to avoid premature handoff.
        guard session.status == 2 || session.status == 3 else { return false }
        if (session.adState?.sessionAdsRequired ?? session.adState?.isAdsRequired ?? false) {
            return false
        }
        if let queuePosition = session.queuePosition, queuePosition > 1 {
            return false
        }
        if (activeStreamSettings ?? settings).experimentalNativeNVSTEnabled {
            return !(session.nativeRtspsEndpoints ?? []).isEmpty
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
        guard session.mediaPort > 0 else { return true }
        if normalizedEndpointHost(from: session.mediaIp) != nil {
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
            let requestedStreamSettings = currentStreamerSettings
            let deviceId = persistentDeviceId()
            let baseUrl = refreshed.provider.streamingServiceUrl
            let activeCandidates = try await api.fetchActiveSessions(
                session: refreshed,
                streamingBaseUrl: baseUrl,
                vpcId: cachedVpcId,
                settings: requestedStreamSettings,
                deviceId: deviceId
            )
            let compatibleCandidates = activeCandidates.filter {
                remoteSession($0, matchesStreamSettings: requestedStreamSettings, session: refreshed)
            }
            let gameAppIds = Set(
                ([session.game.launchAppId] + session.game.launchOptions.map(\.appId))
                    .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
            )
            let sameGameCandidates = activeCandidates.filter { candidate in
                guard remoteSessionIsLaunchable(candidate) else { return false }
                if candidate.id == session.id { return true }
                guard let appId = candidate.appId else { return false }
                return gameAppIds.contains(appId)
            }
            let candidatePool = compatibleCandidates + sameGameCandidates.filter { candidate in
                !compatibleCandidates.contains(where: { $0.id == candidate.id })
            }
            let readyCandidate = candidatePool.first {
                $0.id == session.id && ($0.status == 2 || $0.status == 3) && $0.serverIp?.isEmpty == false
            } ?? candidatePool.first {
                guard let appId = $0.appId else { return false }
                return gameAppIds.contains(appId) && ($0.status == 2 || $0.status == 3) && $0.serverIp?.isEmpty == false
            }
            let launchingCandidate = candidatePool.first {
                $0.id == session.id && remoteSessionIsLaunchable($0)
            } ?? candidatePool.first {
                guard let appId = $0.appId else { return false }
                return gameAppIds.contains(appId) && remoteSessionIsLaunchable($0)
            }

            let candidate: RemoteSessionCandidate
            if let readyCandidate {
                candidate = readyCandidate
            } else if let launchingCandidate {
                let pollingSettings = streamSettingsByAdoptingRemoteProfile(
                    launchingCandidate,
                    base: requestedStreamSettings,
                    session: refreshed
                )
                var latest = ActiveSession(
                    id: launchingCandidate.id,
                    game: session.game,
                    startedAt: .now,
                    status: launchingCandidate.status,
                    queuePosition: nil,
                    seatSetupStep: nil,
                    serverIp: launchingCandidate.serverIp,
                    mediaIp: nil,
                    mediaPort: 0,
                    signalingServer: nil,
                    signalingUrl: nil,
                    iceServers: [],
                    zone: cachedVpcId,
                    streamingBaseUrl: baseUrl,
                    clientId: UUID().uuidString,
                    deviceId: deviceId,
                    adState: nil
                )
                activeSession = latest
                for attempt in 0..<45 {
                    if isReadyForStreamer(latest) {
                        break
                    }
                    logger.info(
                        "Reopen polling active candidate id=\(latest.id, privacy: .public) status=\(latest.status) attempt=\(attempt + 1) signalingServer=\(latest.signalingServer ?? "nil", privacy: .public) signalingUrl=\(latest.signalingUrl ?? "nil", privacy: .public) mediaIp=\(latest.mediaIp ?? "nil", privacy: .public) mediaPort=\(latest.mediaPort)"
                    )
                    try await Task.sleep(for: .seconds(1))
                    latest = try await api.pollSession(session: refreshed, activeSession: latest, settings: pollingSettings)
                    activeSession = mergeQueueSessionState(previous: activeSession ?? latest, next: latest)
                }
                guard isReadyForStreamer(latest), let serverIp = latest.serverIp, !serverIp.isEmpty else {
                    activeSession = latest
                    lastError = "Session is still preparing its stream endpoint. Try reopening again in a moment."
                    sessionError = lastError
                    return
                }
                candidate = RemoteSessionCandidate(
                    id: latest.id,
                    appId: launchingCandidate.appId,
                    status: latest.status,
                    serverIp: serverIp,
                    streamSettingsSignature: launchingCandidate.streamSettingsSignature,
                    resolution: launchingCandidate.resolution ?? latest.negotiatedStreamProfile?.resolution,
                    fps: launchingCandidate.fps ?? latest.negotiatedStreamProfile?.fps
                )
            } else {
                activeSession = nil
                activeStreamSettings = nil
                syncTrackedSessionSurface()
                lastError = "No active session for this game is available to reconnect."
                sessionError = lastError
                return
            }

            let streamSettings = streamSettingsByAdoptingRemoteProfile(
                candidate,
                base: requestedStreamSettings,
                session: refreshed
            )

            let claimed = try await api.claimSession(
                session: refreshed,
                candidate: candidate,
                game: session.game,
                streamingBaseUrl: baseUrl,
                vpcId: cachedVpcId,
                settings: streamSettings,
                deviceId: deviceId,
                touchProvisionedOverride: session.touchProvisioned
            )
            activeSession = claimed
            activeStreamSettings = streamSettings
            setStreamSession(claimed, reason: "reopenStreamer.claimed")
            lastError = nil
            sessionError = nil
        } catch where OpenNOWErrorPresenter.isCancellation(error) {
            return
        } catch {
            logger.error(
                "Reopen refresh failed id=\(session.id, privacy: .public) error=\(error.localizedDescription, privacy: .public)"
            )
            lastError = "Failed to reconnect session: \(error.localizedDescription)"
            sessionError = lastError
        }
    }

    private var isFreeTierUser: Bool {
        membershipTierIsFree(subscription?.membershipTier ?? user?.membershipTier)
    }

    private func reportQueueAdAction(
        adId: String,
        action: SessionAdAction,
        watchedTimeInMs: Int? = nil,
        completedAdDurationMs: Int? = nil,
        cancelReason: String? = nil,
        errorInfo: String? = nil
    ) async {
        guard let session = authSession, let active = activeSession else { return }
        let pausedTimeInMs: Int? = {
            guard let startedAt = adStartedAtById[adId], action == .finish || action == .cancel else {
                return nil
            }
            let elapsed = max(0, Int(Date().timeIntervalSince(startedAt) * 1000))
            let adDurationMs = completedAdDurationMs
                ?? activeQueueAd?.adLengthInSeconds.map { Int(round($0 * 1000)) }
                ?? activeQueueAd?.durationMs
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
                errorInfo: errorInfo,
                settings: settings
            )
            if activeSession?.id == updated.id {
                let terminalAction = action == .finish || action == .cancel
                var previous = active
                if terminalAction {
                    previous.adState = removeSessionAdItem(previous.adState, adId: adId)
                }
                activeSession = mergeQueueSessionState(
                    previous: previous,
                    next: updated,
                    preserveMissingAdState: !terminalAction
                )
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
        guard currentScenePhase != .active,
              !sessionPollBackgroundAllowanceConsumed,
              sessionPollBackgroundTaskId == .invalid else {
            return
        }
        sessionPollBackgroundAllowanceConsumed = true
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
        if Date().timeIntervalSince(persisted.startedAt) > sessionRestoreMaxAgeSeconds {
            logger.notice("Discarding stale persisted session id=\(persisted.id, privacy: .public)")
            activeSession = nil
            showStreamLoading = false
            queueOverlayVisible = false
            syncTrackedSessionSurface()
            return
        }
        if !resumableSessions.contains(where: { $0.id == persisted.id }) {
            if remoteSessionsSnapshotLoaded {
                logger.notice(
                    "Discarding persisted session id=\(persisted.id, privacy: .public) because it is absent from refreshed active-session list."
                )
                clearLocalSessionState(reason: "restoreTrackedSessionIfNeeded.missingRemote")
                return
            }
            logger.notice(
                "Persisted session id=\(persisted.id, privacy: .public) not present in active-session list; keeping it pollable for jump back."
            )
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
        let active = activeSession
        updateQueueTrend(for: active)
        if let active {
            QueueReadyAlert.announceIfNeeded(
                sessionId: active.id,
                isReady: isReadyForStreamer(active) || active.status == 3,
                enabled: settings.queueReadySound
            )
        }
        persistActiveSession(active)
        persistActiveStreamSettings(active == nil ? nil : activeStreamSettings)
        let state = settings.queueLiveActivitiesEnabled ? active.flatMap(queueActivityState(for:)) : nil
        Task {
            await QueueLiveActivityManager.shared.sync(
                sessionId: active?.id,
                gameTitle: active?.game.title,
                storeName: active.map { queueActivityStoreName(for: $0) },
                state: state
            )
        }
    }

    /// Feeds the trend estimator, and resets it when the queue is no longer the thing on screen.
    /// A stale trend from the previous launch is worse than no trend at all.
    private func updateQueueTrend(for session: ActiveSession?) {
        guard let session, session.status < 2, let position = session.queuePosition else {
            displayQueuePosition = nil
            queueTrendSessionId = nil
            if !queueTrend.samples.isEmpty {
                queueTrend.reset()
            }
            return
        }
        if queueTrendSessionId != session.id {
            queueTrendSessionId = session.id
            displayQueuePosition = nil
            queueTrend.reset()
        }
        // Android keeps the lowest displayed position for a session so delayed polls cannot
        // make the queue numeral jump backwards. The trend must use the same visible series.
        let stablePosition = min(displayQueuePosition ?? position, position)
        displayQueuePosition = stablePosition
        queueTrend.record(position: stablePosition)
    }

    private func queueActivityStoreName(for session: ActiveSession) -> String {
        let store = session.game.platform.trimmingCharacters(in: .whitespacesAndNewlines)
        if !store.isEmpty {
            return gameStoreDisplayName(store)
        }
        if let firstStore = session.game.stores?.first?.trimmingCharacters(in: .whitespacesAndNewlines), !firstStore.isEmpty {
            return gameStoreDisplayName(firstStore)
        }
        return "Store"
    }

    private func queueActivityState(for session: ActiveSession) -> QueueActivityAttributes.ContentState? {
        if streamSession != nil {
            return nil
        }
        if isReadyForStreamer(session) || session.status == 3 {
            return QueueActivityAttributes.ContentState(
                phase: .ready,
                headline: "Ready to play",
                detail: "Tap to jump back in",
                queueLabel: "Ready",
                queuePosition: session.queuePosition,
                progress: 1
            )
        }
        if isInQueuePhase(session) {
            let detail: String
            let queueLabel: String
            if let queue = session.queuePosition {
                // The Live Activity is glanced at from the lock screen, so the estimate matters
                // more here than anywhere: it is the difference between checking every minute and
                // putting the phone down. It only appears when the estimator has earned it.
                let position = queue == 1 ? "Next in queue" : "Queue #\(queue)"
                if let estimate = queueTrend.estimate() {
                    detail = "\(position) · \(estimate.label)"
                } else if case .holding = queueTrend.trend() {
                    detail = "\(position) · holding"
                } else {
                    detail = position
                }
                queueLabel = queue == 1 ? "Soon" : "Q\(queue)"
            } else {
                detail = "Waiting in queue"
                queueLabel = "Queue"
            }
            return QueueActivityAttributes.ContentState(
                phase: .queued,
                headline: session.queuePosition == 1 ? "You're next" : "In queue",
                detail: detail,
                queueLabel: queueLabel,
                queuePosition: session.queuePosition,
                progress: queueTrend.progress(),
                waitStartedAt: session.startedAt
            )
        }
        if isInSetupPhase(session) || session.status == 2 {
            return QueueActivityAttributes.ContentState(
                phase: .waiting,
                headline: "Preparing your rig",
                detail: session.seatSetupStep.map { "Step \($0) of 4" } ?? "Almost there",
                queueLabel: "Setup",
                queuePosition: nil,
                progress: session.seatSetupStep.map { min(max(Double($0) / 4, 0.25), 0.95) },
                waitStartedAt: session.startedAt
            )
        }
        return nil
    }

    private func resolveGameForRemoteSession(_ candidate: RemoteSessionCandidate) -> CloudGame? {
        if let appId = candidate.appId {
            if let fromAll = allGames.first(where: { $0.launchAppId == appId || $0.launchOptions.contains(where: { $0.appId == appId }) }) {
                return fromAll
            }
            if let fromLibrary = libraryGames.first(where: { $0.launchAppId == appId || $0.launchOptions.contains(where: { $0.appId == appId }) }) {
                return fromLibrary
            }
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
        if let session {
            beginSessionReport(for: session)
        }
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

    private static func loadAuthState(from defaults: UserDefaults) -> PersistedAuthState {
        if let data = AuthKeychainStore.load(),
           let state = try? JSONDecoder().decode(PersistedAuthState.self, from: data) {
            return normalizedAuthState(state)
        }

        let migratedState: PersistedAuthState
        if let data = defaults.data(forKey: "OpenNOW.iOS.authState"),
           let state = try? JSONDecoder().decode(PersistedAuthState.self, from: data) {
            migratedState = normalizedAuthState(state)
        } else if let legacySession = loadAuthSession(from: defaults) {
            migratedState = PersistedAuthState(
                sessions: [legacySession],
                activeUserId: legacySession.user.userId,
                selectedProvider: legacySession.provider
            )
        } else {
            return PersistedAuthState()
        }
        if !migratedState.sessions.isEmpty,
           let encoded = try? JSONEncoder().encode(migratedState),
           AuthKeychainStore.save(encoded) {
            defaults.removeObject(forKey: "OpenNOW.iOS.authState")
            defaults.removeObject(forKey: "OpenNOW.iOS.authSession")
        }
        return migratedState
    }

    private static func normalizedAuthState(_ state: PersistedAuthState) -> PersistedAuthState {
        var seen = Set<String>()
        let sessions = state.sessions.filter { session in
            guard !session.user.userId.isEmpty, !seen.contains(session.user.userId) else { return false }
            seen.insert(session.user.userId)
            return true
        }
        let activeUserId = state.activeUserId.flatMap { id in
            sessions.contains(where: { $0.user.userId == id }) ? id : nil
        } ?? sessions.first?.user.userId
        return PersistedAuthState(
            sessions: sessions,
            activeUserId: activeUserId,
            selectedProvider: state.selectedProvider ?? sessions.first(where: { $0.user.userId == activeUserId })?.provider
        )
    }

    private static func loadActiveSession(from defaults: UserDefaults) -> ActiveSession? {
        guard let data = defaults.data(forKey: "OpenNOW.iOS.activeSession") else { return nil }
        return try? JSONDecoder().decode(ActiveSession.self, from: data)
    }

    private static func loadActiveStreamSettings(from defaults: UserDefaults) -> AppSettings? {
        guard let data = defaults.data(forKey: "OpenNOW.iOS.activeStreamSettings") else { return nil }
        return try? JSONDecoder().decode(AppSettings.self, from: data)
    }

    private func hydrateCachedCatalog(for session: AuthSession, onlyMissing: Bool = false) {
        if let snapshot = loadCachedCatalog(for: session) {
            if !onlyMissing || allGames.isEmpty {
                allGames = snapshot.allGames
                featuredGames = snapshot.featuredGames.isEmpty ? Array(snapshot.allGames.prefix(8)) : snapshot.featuredGames
            }
            if !onlyMissing || libraryGames.isEmpty {
                libraryGames = snapshot.libraryGames
            }
            if !snapshot.vpcId.isEmpty {
                cachedVpcId = snapshot.vpcId
            }
            return
        }

        let legacyLibraryGames = loadLegacyCachedLibraryGames(for: session)
        if !legacyLibraryGames.isEmpty && (!onlyMissing || libraryGames.isEmpty) {
            libraryGames = legacyLibraryGames
        }
    }

    private func loadCachedCatalog(for session: AuthSession) -> CachedCatalogSnapshot? {
        guard let data = defaults.data(forKey: catalogCacheKey(for: session.user.userId)) else {
            return nil
        }
        return try? JSONDecoder().decode(CachedCatalogSnapshot.self, from: data)
    }

    private func hydrateCachedAccount(for session: AuthSession, onlyMissing: Bool = false) {
        guard let snapshot = loadCachedAccount(for: session) else { return }
        if !onlyMissing || subscription == nil {
            subscription = snapshot.subscription
        }
        if !onlyMissing || accountConnectors.isEmpty {
            accountConnectors = snapshot.accountConnectors
        }
        if !onlyMissing || availableRegions.isEmpty {
            availableRegions = snapshot.availableRegions
        }
        if !snapshot.vpcId.isEmpty {
            cachedVpcId = snapshot.vpcId
        }
        if accountIsCurrent(session.user.userId) {
            var cachedUser = session.user
            cachedUser.membershipTier = snapshot.membershipTier
            let cachedSession = AuthSession(
                provider: session.provider,
                tokens: session.tokens,
                user: cachedUser
            )
            authSession = cachedSession
            user = cachedUser
        }
    }

    private func loadCachedAccount(for session: AuthSession) -> CachedAccountSnapshot? {
        guard let data = defaults.data(forKey: accountCacheKey(for: session.user.userId)) else {
            return nil
        }
        return try? JSONDecoder().decode(CachedAccountSnapshot.self, from: data)
    }

    private func persistCachedAccount(for session: AuthSession) {
        let snapshot = CachedAccountSnapshot(
            schemaVersion: 1,
            cachedAt: Date().timeIntervalSince1970,
            membershipTier: subscription?.membershipTier ?? user?.membershipTier ?? session.user.membershipTier,
            subscription: subscription,
            accountConnectors: accountConnectors,
            availableRegions: availableRegions,
            vpcId: cachedVpcId
        )
        guard let encoded = try? JSONEncoder().encode(snapshot) else { return }
        defaults.set(encoded, forKey: accountCacheKey(for: session.user.userId))
    }

    private func persistCachedCatalog(
        allGames: [CloudGame],
        featuredGames: [CloudGame],
        libraryGames: [CloudGame],
        vpcId: String,
        for session: AuthSession
    ) {
        let snapshot = CachedCatalogSnapshot(
            schemaVersion: 1,
            cachedAt: Date().timeIntervalSince1970,
            vpcId: vpcId,
            allGames: allGames,
            featuredGames: featuredGames,
            libraryGames: libraryGames
        )
        guard let encoded = try? JSONEncoder().encode(snapshot) else { return }
        defaults.set(encoded, forKey: catalogCacheKey(for: session.user.userId))
        defaults.removeObject(forKey: legacyLibraryGamesCacheKey(for: session.user.userId))
    }

    private func loadLegacyCachedLibraryGames(for session: AuthSession) -> [CloudGame] {
        guard let data = defaults.data(forKey: legacyLibraryGamesCacheKey(for: session.user.userId)) else {
            return []
        }
        return (try? JSONDecoder().decode([CloudGame].self, from: data)) ?? []
    }

    private func removeCachedCatalog(forUserId userId: String) {
        defaults.removeObject(forKey: catalogCacheKey(for: userId))
        defaults.removeObject(forKey: legacyLibraryGamesCacheKey(for: userId))
    }

    private func removeCachedAccount(forUserId userId: String) {
        defaults.removeObject(forKey: accountCacheKey(for: userId))
    }

    private func removeAllCachedCatalog() {
        let prefixes = [
            "\(catalogCacheKeyPrefix).",
            "\(legacyLibraryGamesCacheKeyPrefix)."
        ]
        for key in defaults.dictionaryRepresentation().keys where prefixes.contains(where: { key.hasPrefix($0) }) {
            defaults.removeObject(forKey: key)
        }
    }

    private func removeAllCachedAccounts() {
        let prefix = "\(accountCacheKeyPrefix)."
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(prefix) {
            defaults.removeObject(forKey: key)
        }
    }

    private func catalogCacheKey(for userId: String) -> String {
        "\(catalogCacheKeyPrefix).\(cacheDigest(for: userId))"
    }

    private func accountCacheKey(for userId: String) -> String {
        "\(accountCacheKeyPrefix).\(cacheDigest(for: userId))"
    }

    private func legacyLibraryGamesCacheKey(for userId: String) -> String {
        "\(legacyLibraryGamesCacheKeyPrefix).\(cacheDigest(for: userId))"
    }

    private func cacheDigest(for value: String) -> String {
        SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    private func persistAuthSession(_ session: AuthSession) {
        var state = currentAuthState()
        state.sessions.removeAll { $0.user.userId == session.user.userId }
        state.sessions.insert(session, at: 0)
        state.activeUserId = session.user.userId
        state.selectedProvider = session.provider
        persistAuthState(state)
    }

    /// What the Keychain currently holds, mirrored in memory.
    ///
    /// Reading it back was two IPC round-trips to `securityd` plus a full JSON decode, and the
    /// session poll asked for it every two seconds for the whole life of a queue and a stream.
    private func currentAuthState() -> PersistedAuthState {
        if let persistedAuthState { return persistedAuthState }
        let loaded = Self.loadAuthState(from: defaults)
        persistedAuthState = loaded
        return loaded
    }

    private func persistAuthState(_ state: PersistedAuthState) {
        let normalized = Self.normalizedAuthState(state)
        // The poll loop re-persists the auth session on every tick, and `refreshSession` returns
        // the same tokens until they are near expiry — so in the steady state this was encoding
        // every saved account and writing the Keychain twice a minute, on the main actor, to store
        // bytes it had just read. Rewriting identical state is not persistence, it is a stall: it
        // is what made a 30 fps progress bar stutter and image decodes queue behind it.
        guard normalized != persistedAuthState else { return }
        if normalized.sessions.isEmpty {
            AuthKeychainStore.delete()
        } else {
            guard let encoded = try? JSONEncoder().encode(normalized),
                  AuthKeychainStore.save(encoded) else {
                lastError = "Could not securely save account credentials in Keychain."
                return
            }
        }
        persistedAuthState = normalized
        savedAccounts = normalized.savedAccounts
        defaults.removeObject(forKey: authStateKey)
        defaults.removeObject(forKey: authSessionKey)
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

    private func persistActiveStreamSettings(_ settings: AppSettings?) {
        guard let settings else {
            defaults.removeObject(forKey: activeStreamSettingsKey)
            return
        }
        if let encoded = try? JSONEncoder().encode(settings) {
            defaults.set(encoded, forKey: activeStreamSettingsKey)
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
