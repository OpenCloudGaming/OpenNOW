import CoreGraphics
import Foundation

// These presets select the streamed input size; MetalFX spatial has no quality knob.
enum MetalFXQualityPreset: String, Codable, CaseIterable, Identifiable {
    case manual, quality, balanced, performance
    var id: String { rawValue }
    var label: String { rawValue.capitalized }
    var inputScale: CGFloat? {
        switch self {
        case .manual: return nil
        case .quality: return 0.85
        case .balanced: return 2.0 / 3.0
        case .performance: return 0.5
        }
    }
}

// MARK: - Microphone

/// Mirrors Android's `MicrophoneMode`. The previous iOS build had a single `keepMicEnabled`
/// boolean, which cannot express push-to-talk.
enum MicrophoneMode: String, Codable, CaseIterable, Identifiable {
    case disabled
    case pushToTalk = "push-to-talk"
    case voiceActivity = "voice-activity"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .disabled: return "Off"
        case .pushToTalk: return "Push to talk"
        case .voiceActivity: return "Voice activity"
        }
    }

    var isEnabled: Bool { self != .disabled }
}

// MARK: - Touch routing

/// Whether fingers reach the host as real touch rather than being turned into a cursor.
///
/// `automatic` limits it to games known to react to a touch device, which is the whole feature for
/// most people. The other two exist because that list is maintained by hand and will lag reality.
enum NativeTouchMode: String, Codable, CaseIterable, Identifiable {
    case automatic
    case never
    case always

    var id: String { rawValue }

    var label: String {
        switch self {
        case .automatic: return "Automatic"
        case .never: return "Never"
        case .always: return "Always"
        }
    }

    var detail: String {
        switch self {
        case .automatic: return "Use touch only in games known to support it"
        case .never: return "Always send a mouse cursor instead"
        case .always: return "Send touch to every game"
        }
    }
}

/// The end state the touch routing actually resolves to, so the settings row can show it.
/// Android exposes the inputs and leaves the user to work out the result; showing the result is
/// the difference between a comprehensible screen and a confusing one.
enum ResolvedTouchMode: Equatable {
    case nativeTouch
    /// Touch is wanted, but this session was created without the digitizer that would receive it.
    /// The host decides that once, when the session starts, so the only way out is a new session.
    case nativeTouchUnavailable
    case trackpadCursor(directClick: Bool)
    case virtualGamepad
    case inert

    var label: String {
        switch self {
        case .nativeTouch: return "Native touch"
        case .nativeTouchUnavailable: return "Native touch — needs a new session"
        case .trackpadCursor(let directClick): return directClick ? "Trackpad, direct click" : "Trackpad cursor"
        case .virtualGamepad: return "Virtual gamepad"
        case .inert: return "Touch does nothing"
        }
    }
}

enum TouchJoystickMode: String, Codable, CaseIterable, Identifiable {
    case fixed
    case dynamic

    var id: String { rawValue }
    var label: String { self == .fixed ? "Fixed" : "Follow finger" }
}

enum TouchAimMode: String, Codable, CaseIterable, Identifiable {
    case lockJoystick
    case lockZone

    var id: String { rawValue }
    var label: String { self == .lockJoystick ? "Lock to stick" : "Lock to zone" }
}

enum TouchControllerStyle: String, Codable, CaseIterable, Identifiable {
    case solid
    case outline

    var id: String { rawValue }
    var label: String { self == .solid ? "Solid" : "Outline" }
}

// MARK: - Catalog backdrop

enum CatalogWallpaperPreset: String, Codable, CaseIterable, Identifiable {
    case colorfulAbstract = "colorful-abstract"
    case original
    case absoluteCinema = "absolute-cinema"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .colorfulAbstract: return "Colorful Abstract"
        case .original: return "Original"
        case .absoluteCinema: return "Absolute Cinema"
        }
    }
}

// MARK: - Stats HUD

/// The ten metrics the in-stream HUD can show. Defaults match Android so a user moving between
/// platforms sees the same four readouts.
struct StreamStatsMetrics: Codable, Equatable {
    var fps: Bool = true
    var ping: Bool = true
    var bitrate: Bool = false
    var battery: Bool = true
    var connection: Bool = true
    var resolution: Bool = false
    var codec: Bool = false
    var location: Bool = false
    var latency: Bool = false
    var packetLoss: Bool = false

    var enabledCount: Int {
        [fps, ping, bitrate, battery, connection, resolution, codec, location, latency, packetLoss]
            .filter { $0 }
            .count
    }

    /// A HUD with nothing in it is a bug the user cannot recover from without finding this screen
    /// again, so the last metric cannot be turned off — the HUD toggle is the way to hide it.
    var isMinimallyPopulated: Bool { enabledCount > 0 }

    static let `default` = StreamStatsMetrics()

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        fps = try c.decodeIfPresent(Bool.self, forKey: .fps) ?? true
        ping = try c.decodeIfPresent(Bool.self, forKey: .ping) ?? true
        bitrate = try c.decodeIfPresent(Bool.self, forKey: .bitrate) ?? false
        battery = try c.decodeIfPresent(Bool.self, forKey: .battery) ?? true
        connection = try c.decodeIfPresent(Bool.self, forKey: .connection) ?? true
        resolution = try c.decodeIfPresent(Bool.self, forKey: .resolution) ?? false
        codec = try c.decodeIfPresent(Bool.self, forKey: .codec) ?? false
        location = try c.decodeIfPresent(Bool.self, forKey: .location) ?? false
        latency = try c.decodeIfPresent(Bool.self, forKey: .latency) ?? false
        packetLoss = try c.decodeIfPresent(Bool.self, forKey: .packetLoss) ?? false
    }
}

// MARK: - Normalized point

/// Fraction of the available area, 0...1 on both axes. Used for the draggable in-stream keyboard
/// button so a saved position survives a rotation or a different device.
struct NormalizedPoint: Codable, Equatable {
    var x: Double
    var y: Double

    init(x: Double, y: Double) {
        self.x = Self.clamped(x, fallback: 1)
        self.y = Self.clamped(y, fallback: 0.5)
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        x = Self.clamped(try c.decodeIfPresent(Double.self, forKey: .x), fallback: 1)
        y = Self.clamped(try c.decodeIfPresent(Double.self, forKey: .y), fallback: 0.5)
    }

    private static func clamped(_ value: Double?, fallback: Double) -> Double {
        guard let value, value.isFinite else { return fallback }
        return min(max(value, 0), 1)
    }

    static let trailingCenter = NormalizedPoint(x: 1, y: 0.5)
}

// MARK: - Touch settings

/// Everything about touch input that is not a control's position or size — those live in
/// `TouchControlLayout`. Mirrors the non-geometry half of Android's `AndroidTouchSettings`.
struct TouchSettings: Codable, Equatable {
    var nativeTouchMode: NativeTouchMode = .automatic

    /// Scales the velocity of touch movement in native touch mode. Below 1 slows scroll and swipe
    /// gestures; above 1 speeds them up.
    var nativeTouchScrollScale: Double = 1.0

    /// Minimum movement in points before a move event is forwarded in native touch mode.
    /// Suppresses sensor jitter that would otherwise turn a tap into a micro-swipe.
    var nativeTouchJitterThreshold: Double = 8

    var joystickMode: TouchJoystickMode = .fixed
    var aimMode: TouchAimMode = .lockJoystick
    var joystickDeadZone: Double = 0
    var style: TouchControllerStyle = .solid

    /// Finger-mouse taps click where the finger lands rather than moving a cursor first.
    var mouseDirectClick: Bool = false

    var edgePadding: Double = 14
    var bottomPadding: Double = 10
    var leftOffsetX: Double = 0
    var leftOffsetY: Double = 0
    var rightOffsetX: Double = 0
    var rightOffsetY: Double = 0

    static let `default` = TouchSettings()

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        nativeTouchMode = try c.decodeIfPresent(NativeTouchMode.self, forKey: .nativeTouchMode) ?? .automatic
        nativeTouchScrollScale = try c.decodeIfPresent(Double.self, forKey: .nativeTouchScrollScale) ?? 1.0
        nativeTouchJitterThreshold = try c.decodeIfPresent(Double.self, forKey: .nativeTouchJitterThreshold) ?? 8
        joystickMode = try c.decodeIfPresent(TouchJoystickMode.self, forKey: .joystickMode) ?? .fixed
        aimMode = try c.decodeIfPresent(TouchAimMode.self, forKey: .aimMode) ?? .lockJoystick
        joystickDeadZone = try c.decodeIfPresent(Double.self, forKey: .joystickDeadZone) ?? 0
        style = try c.decodeIfPresent(TouchControllerStyle.self, forKey: .style) ?? .solid
        mouseDirectClick = try c.decodeIfPresent(Bool.self, forKey: .mouseDirectClick) ?? false
        edgePadding = try c.decodeIfPresent(Double.self, forKey: .edgePadding) ?? 14
        bottomPadding = try c.decodeIfPresent(Double.self, forKey: .bottomPadding) ?? 10
        leftOffsetX = try c.decodeIfPresent(Double.self, forKey: .leftOffsetX) ?? 0
        leftOffsetY = try c.decodeIfPresent(Double.self, forKey: .leftOffsetY) ?? 0
        rightOffsetX = try c.decodeIfPresent(Double.self, forKey: .rightOffsetX) ?? 0
        rightOffsetY = try c.decodeIfPresent(Double.self, forKey: .rightOffsetY) ?? 0
        normalize()
    }

    mutating func normalize() {
        nativeTouchScrollScale = min(max(nativeTouchScrollScale, 0.25), 2.0)
        nativeTouchJitterThreshold = min(max(nativeTouchJitterThreshold, 0), 24)
        joystickDeadZone = min(max(joystickDeadZone, 0), 0.3)
        edgePadding = min(max(edgePadding, 0), 72)
        bottomPadding = min(max(bottomPadding, 0), 120)
        leftOffsetX = min(max(leftOffsetX, -220), 220)
        leftOffsetY = min(max(leftOffsetY, -160), 160)
        rightOffsetX = min(max(rightOffsetX, -220), 220)
        rightOffsetY = min(max(rightOffsetY, -160), 160)
    }

    /// Resets only the offsets and paddings, leaving the routing choices alone —
    /// "reset layout" should not silently change whether touch reaches the game.
    mutating func resetLayout() {
        let fresh = TouchSettings()
        edgePadding = fresh.edgePadding
        bottomPadding = fresh.bottomPadding
        leftOffsetX = fresh.leftOffsetX
        leftOffsetY = fresh.leftOffsetY
        rightOffsetX = fresh.rightOffsetX
        rightOffsetY = fresh.rightOffsetY
    }
}

// MARK: - Stick maths

/// Shared by the on-screen sticks and, in spirit, by the controller cursor curve.
///
/// Kept out of the view so it can be tested: the correctness that matters here is arithmetic, and
/// a dead zone that clips instead of rescaling is a bug you only notice as "my stick feels short".
enum TouchStickMath {
    /// Rescales the remaining travel so the stick still reaches full deflection at the rim.
    /// Simply zeroing inside the threshold would cost the player the top of their range.
    static func applyDeadZone(x: CGFloat, y: CGFloat, deadZone: Double) -> (CGFloat, CGFloat) {
        let threshold = CGFloat(min(max(deadZone, 0), 0.9))
        guard threshold > 0 else { return (x, y) }
        let magnitude = hypot(x, y)
        guard magnitude > threshold else { return (0, 0) }
        let scaled = (magnitude - threshold) / (1 - threshold)
        let factor = scaled / magnitude
        return (x * factor, y * factor)
    }
}

// MARK: - Session timer

enum SessionTimerMode: Equatable {
    case countdown
    case stopwatch
}

/// Plan-derived session cap. Free accounts get a hard one-hour limit and therefore a countdown;
/// paid tiers get a much longer cap and a stopwatch, because counting down from eight hours is
/// anxiety with no information in it.
struct SmartSessionLimit: Equatable {
    let tierLabel: String
    let limitHours: Int
    let mode: SessionTimerMode

    static let free = SmartSessionLimit(tierLabel: "Free", limitHours: 1, mode: .countdown)
    static let performance = SmartSessionLimit(tierLabel: "Performance", limitHours: 6, mode: .stopwatch)
    static let ultimate = SmartSessionLimit(tierLabel: "Ultimate", limitHours: 8, mode: .stopwatch)

    static func forTier(_ membershipTier: String?) -> SmartSessionLimit {
        let normalized = (membershipTier ?? "")
            .uppercased()
            .components(separatedBy: CharacterSet.alphanumerics.inverted)
            .joined()
        if normalized.contains("ULTIMATE") || normalized.contains("RTX3080") { return .ultimate }
        if normalized.contains("PRIORITY") || normalized.contains("PERFORMANCE") || normalized.contains("FOUNDERS") {
            return .performance
        }
        return .free
    }

    var limitSeconds: Int { limitHours * 3600 }
}

/// Thresholds, in seconds remaining, at which the session clock surfaces itself.
let sessionWarningThresholdsSeconds: [Int] = [30 * 60, 10 * 60, 5 * 60, 3 * 60, 60]

func sessionWarningThresholdCrossed(previousRemaining: Int?, remaining: Int) -> Int? {
    guard let previous = previousRemaining else { return nil }
    return sessionWarningThresholdsSeconds
        .filter { previous > $0 && remaining <= $0 }
        .min()
}

// MARK: - Analytics consent

/// Three states, not two. "Not yet asked" has to be distinguishable from "asked and declined",
/// or the consent prompt reappears every launch.
enum AnalyticsConsent: Equatable {
    case notAsked
    case granted
    case declined

    init(asked: Bool, optOut: Bool) {
        guard asked else { self = .notAsked; return }
        self = optOut ? .declined : .granted
    }

    var isSharing: Bool { self == .granted }
}

// MARK: - App language

/// The fourteen locales Android ships app-owned translations for, plus a system-default entry.
/// The value is a BCP-47 tag; empty means "follow the system".
enum AppLanguage {
    static let systemDefault = ""

    static let supported: [(tag: String, label: String)] = [
        ("", "System default"),
        ("en", "English"),
        ("ar", "العربية"),
        ("de", "Deutsch"),
        ("es", "Español"),
        ("fr", "Français"),
        ("ja", "日本語"),
        ("ko", "한국어"),
        ("nl", "Nederlands"),
        ("pl", "Polski"),
        ("pt", "Português"),
        ("ro", "Română"),
        ("ru", "Русский"),
        ("tr", "Türkçe"),
        ("zh-Hans", "简体中文")
    ]

    static func label(for tag: String) -> String {
        supported.first { $0.tag == tag }?.label ?? tag
    }
}
