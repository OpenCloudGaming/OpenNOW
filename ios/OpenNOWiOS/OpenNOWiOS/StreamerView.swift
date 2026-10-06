import Foundation
import SwiftUI

#if os(iOS)
import UIKit
#endif

#if os(iOS) && canImport(WebRTC)
import AVFoundation
import Accelerate
import AVKit
import CoreImage
import MetalKit
import Network
@preconcurrency import WebRTC
import os
#endif

#if os(iOS) && canImport(WebRTC)
enum NativeStreamAudioSessionPolicy {
    static func category(enableMic: Bool) -> AVAudioSession.Category {
        .playback
    }

    static func mode(enableMic: Bool) -> AVAudioSession.Mode {
        .moviePlayback
    }

    static func options(enableMic: Bool) -> AVAudioSession.CategoryOptions {
        // `.playback` already routes to AirPlay and Bluetooth A2DP. Apple only permits explicitly
        // setting `.allowAirPlay` with `.playAndRecord`; passing it here can make `setCategory`
        // reject the entire media session before the output engine starts.
        []
    }
}

func nativeStreamShouldUseFilteredRenderer(
    osMajorVersion: Int,
    videoCodec: NativeStreamVideoCodec,
    streamSharpeningEnabled: Bool,
    isSimulator: Bool
) -> Bool {
    // VideoToolbox HEVC frames can reach RTCMTLVideoView without producing a picture
    // on older iOS devices. Use the same surface that restores video with sharpening,
    // independently of whether the user wants the sharpening effect.
    isSimulator || osMajorVersion >= 26 || videoCodec == .h265 || streamSharpeningEnabled
}

enum NativeStreamTransportPolicy {
    static let offerTimeout: TimeInterval = 12
    static let iceDisconnectedGrace: TimeInterval = 3.5
    static let allowsTCPCandidates = true
}

enum NativeStreamLivenessAction: Equatable {
    case none
    case requestKeyframe(stalledFor: TimeInterval, attempt: Int)
    case restartTransport(stalledFor: TimeInterval)
}

struct NativeStreamLivenessWatchdog {
    private let keyframeAfter: TimeInterval
    private let keyframeInterval: TimeInterval
    private let restartAfter: TimeInterval
    private var lastProgressAt: TimeInterval?
    private var lastBytesReceived: Int?
    private var lastFramesDecoded: Int?
    private var lastKeyframeRequestAt: TimeInterval?
    private var keyframeAttempts = 0
    private(set) var latestObservationProgressed = false

    init(
        keyframeAfter: TimeInterval = 5,
        keyframeInterval: TimeInterval = 2.5,
        restartAfter: TimeInterval = 10
    ) {
        self.keyframeAfter = keyframeAfter
        self.keyframeInterval = keyframeInterval
        self.restartAfter = restartAfter
    }

    mutating func reset() {
        lastProgressAt = nil
        lastBytesReceived = nil
        lastFramesDecoded = nil
        lastKeyframeRequestAt = nil
        keyframeAttempts = 0
        latestObservationProgressed = false
    }

    mutating func markConnected(now: TimeInterval) {
        lastProgressAt = now
        lastKeyframeRequestAt = nil
        keyframeAttempts = 0
    }

    mutating func observe(
        now: TimeInterval,
        bytesReceived: Int?,
        framesDecoded: Int?,
        connected: Bool
    ) -> NativeStreamLivenessAction {
        latestObservationProgressed = false
        guard connected else {
            reset()
            return .none
        }

        let progressed: Bool
        if let framesDecoded {
            progressed = lastFramesDecoded.map { framesDecoded > $0 } ?? (framesDecoded > 0)
        } else if let bytesReceived {
            progressed = lastBytesReceived.map { bytesReceived > $0 } ?? (bytesReceived > 0)
        } else {
            progressed = false
        }
        if let bytesReceived {
            lastBytesReceived = bytesReceived
        }
        if let framesDecoded {
            lastFramesDecoded = framesDecoded
        }
        if progressed {
            latestObservationProgressed = true
            lastProgressAt = now
            lastKeyframeRequestAt = nil
            keyframeAttempts = 0
            return .none
        }

        guard let lastProgressAt else {
            self.lastProgressAt = now
            return .none
        }
        let stalledFor = now - lastProgressAt
        if stalledFor >= restartAfter {
            reset()
            return .restartTransport(stalledFor: stalledFor)
        }
        let keyframeDue = lastKeyframeRequestAt.map { now - $0 >= keyframeInterval } ?? true
        if stalledFor >= keyframeAfter, keyframeDue {
            lastKeyframeRequestAt = now
            keyframeAttempts += 1
            return .requestKeyframe(stalledFor: stalledFor, attempt: keyframeAttempts)
        }
        return .none
    }
}

struct NativeStreamRecoveryProgressTracker {
    private static let stableSampleCount = 3
    private var consecutiveProgressSamples = 0
    private var stable = false

    mutating func observe(progressed: Bool) -> Bool {
        guard !stable else { return false }
        guard progressed else {
            consecutiveProgressSamples = 0
            return false
        }
        consecutiveProgressSamples += 1
        guard consecutiveProgressSamples >= Self.stableSampleCount else { return false }
        stable = true
        return true
    }
}

/// Requires sustained proof that packets are arriving near the requested rate while decoded
/// output falls behind and decoding exceeds one frame budget. This keeps network loss and a slow
/// cloud game from being misdiagnosed as a local decoder failure.
struct NativeStreamDecoderRecoveryGate {
    private let badSamplesBeforeRecovery: Int
    private let minimumReceivedRatio: Double
    private let maximumDecodedRatio: Double
    private let minimumDecodeBudgetRatio: Double
    private var badSamples = 0
    private var recoveryIssued = false

    init(
        badSamplesBeforeRecovery: Int = 5,
        minimumReceivedRatio: Double = 0.85,
        maximumDecodedRatio: Double = 0.80,
        minimumDecodeBudgetRatio: Double = 1.10
    ) {
        precondition(badSamplesBeforeRecovery > 0)
        self.badSamplesBeforeRecovery = badSamplesBeforeRecovery
        self.minimumReceivedRatio = minimumReceivedRatio
        self.maximumDecodedRatio = maximumDecodedRatio
        self.minimumDecodeBudgetRatio = minimumDecodeBudgetRatio
    }

    mutating func reset() {
        badSamples = 0
        recoveryIssued = false
    }

    mutating func observe(
        receivedFPS: Int?,
        decodedFPS: Int?,
        decodeMs: Double?,
        requestedFPS: Int,
        advancedCodecActive: Bool,
        recoveryEligible: Bool
    ) -> Bool {
        guard advancedCodecActive, recoveryEligible, !recoveryIssued else {
            badSamples = 0
            return false
        }
        let frameBudgetMs = 1_000.0 / Double(max(requestedFPS, 1))
        let overloaded: Bool
        if let receivedFPS, let decodedFPS, let decodeMs {
            overloaded = Double(receivedFPS) >= Double(requestedFPS) * minimumReceivedRatio
                && Double(decodedFPS) <= Double(receivedFPS) * maximumDecodedRatio
                && decodeMs >= frameBudgetMs * minimumDecodeBudgetRatio
        } else {
            overloaded = false
        }
        badSamples = overloaded ? badSamples + 1 : 0
        guard badSamples >= badSamplesBeforeRecovery else { return false }
        badSamples = 0
        recoveryIssued = true
        return true
    }
}

/// Arms during sustained raw packet loss, then asks for one clean keyframe after the path becomes
/// healthy. Recovery waits for congestion to clear and never changes settings or restarts media.
struct NativeStreamPacketLossRecoveryGate {
    private let badSamplesBeforeArmed: Int
    private let lossThresholdPercent: Double
    private let minimumPacketSample: Int
    private let cooldownSamples: Int
    private var consecutiveBadSamples = 0
    private var recoveryArmed = false
    private var remainingCooldownSamples = 0

    init(
        badSamplesBeforeArmed: Int = 2,
        lossThresholdPercent: Double = 5,
        minimumPacketSample: Int = 100,
        cooldownSamples: Int = 15
    ) {
        precondition(badSamplesBeforeArmed > 0)
        precondition((0...100).contains(lossThresholdPercent))
        precondition(minimumPacketSample > 0)
        precondition(cooldownSamples >= 0)
        self.badSamplesBeforeArmed = badSamplesBeforeArmed
        self.lossThresholdPercent = lossThresholdPercent
        self.minimumPacketSample = minimumPacketSample
        self.cooldownSamples = cooldownSamples
    }

    mutating func reset() {
        consecutiveBadSamples = 0
        recoveryArmed = false
        remainingCooldownSamples = 0
    }

    mutating func observe(
        lostDelta: Int?,
        receivedDelta: Int?,
        recoveryEligible: Bool
    ) -> Bool {
        guard recoveryEligible else {
            consecutiveBadSamples = 0
            recoveryArmed = false
            return false
        }
        guard let lostDelta, let receivedDelta,
              lostDelta >= 0, receivedDelta >= 0 else { return false }
        let total = lostDelta + receivedDelta
        guard total >= minimumPacketSample else { return false }
        if remainingCooldownSamples > 0 { remainingCooldownSamples -= 1 }

        let lossPercent = Double(lostDelta) / Double(total) * 100
        if lossPercent >= lossThresholdPercent {
            consecutiveBadSamples += 1
            if consecutiveBadSamples >= badSamplesBeforeArmed { recoveryArmed = true }
            return false
        }

        consecutiveBadSamples = 0
        guard recoveryArmed, remainingCooldownSamples == 0 else { return false }
        recoveryArmed = false
        remainingCooldownSamples = cooldownSamples
        return true
    }
}

enum NativeStreamSignalingFailureDisposition: Equatable {
    case retryTransport
    case retrySignaling
    case recoverSession
    case sessionEnded
}

func nativeStreamSignalingFailureDisposition(_ message: String) -> NativeStreamSignalingFailureDisposition {
    let value = message.lowercased()
    if value.contains("http=410") || value.contains("410 gone") { return .sessionEnded }
    if value.contains("http=404") || value.contains("404 not found") { return .recoverSession }
    let transientStatuses = ["http=429", "http=500", "http=502", "http=503", "http=504"]
    if transientStatuses.contains(where: value.contains) || value.contains("service unavailable") {
        return .retrySignaling
    }
    return .retryTransport
}

func nativeStreamShouldPreserveMediaAfterSignalingFailure(
    _ disposition: NativeStreamSignalingFailureDisposition,
    mediaConnected: Bool
) -> Bool {
    guard mediaConnected else { return false }
    return disposition == .retryTransport || disposition == .retrySignaling
}
#endif

enum StreamSessionTimerMode: Equatable {
    case countdown
    case stopwatch
}

struct StreamSessionLimit: Equatable {
    let tierLabel: String
    let limitHours: Int
    let mode: StreamSessionTimerMode

    var limitSeconds: Int {
        limitHours * 60 * 60
    }
}

struct StreamSessionTimerSnapshot: Equatable {
    let elapsedSeconds: Int
    let remainingSeconds: Int
    let progress: Double

    var isWarning: Bool {
        remainingSeconds <= 10 * 60
    }
}

let streamSessionWarningThresholdsSeconds = [
    30 * 60,
    10 * 60,
    5 * 60,
    3 * 60,
    60
]

func streamSessionLimit(for membershipTier: String?) -> StreamSessionLimit {
    switch StreamSettingsResolver.plan(for: membershipTier) {
    case .free:
        return StreamSessionLimit(tierLabel: "Free", limitHours: 1, mode: .countdown)
    case .priority:
        return StreamSessionLimit(tierLabel: "Performance", limitHours: 6, mode: .stopwatch)
    case .ultimate:
        return StreamSessionLimit(tierLabel: "Ultimate", limitHours: 8, mode: .stopwatch)
    }
}

func streamSessionTimerSnapshot(
    limit: StreamSessionLimit,
    startedAt: Date,
    now: Date = Date()
) -> StreamSessionTimerSnapshot {
    let elapsedSeconds = max(0, Int(now.timeIntervalSince(startedAt)))
    let remainingSeconds = max(0, limit.limitSeconds - elapsedSeconds)
    let progress = limit.limitSeconds > 0
        ? min(max(Double(elapsedSeconds) / Double(limit.limitSeconds), 0), 1)
        : 0
    return StreamSessionTimerSnapshot(
        elapsedSeconds: elapsedSeconds,
        remainingSeconds: remainingSeconds,
        progress: progress
    )
}

struct StreamSessionWarningTracker: Equatable {
    private(set) var previousRemainingSeconds: Int?
    private(set) var warnedThresholds: Set<Int> = []

    mutating func nextWarning(remainingSeconds: Int) -> Int? {
        defer { previousRemainingSeconds = remainingSeconds }
        guard let previousRemainingSeconds else { return nil }
        guard let crossedThreshold = streamSessionWarningThresholdsSeconds
            .filter({ previousRemainingSeconds > $0 && remainingSeconds <= $0 })
            .min() else {
            return nil
        }
        return warnedThresholds.insert(crossedThreshold).inserted ? crossedThreshold : nil
    }
}

func streamTouchLayoutProfile(gameTitle: String, settings: AppSettings) -> String {
    NativeTouchSupport.touchLayoutProfile(gameTitle: gameTitle, settings: settings)
}

#if os(iOS) && canImport(WebRTC)
fileprivate enum NativeStreamNetworkTransport: Equatable, Sendable {
    case wifi
    case cellular
    case ethernet
    case other
    case offline

    init(path: NWPath) {
        guard path.status == .satisfied else {
            self = .offline
            return
        }
        if path.usesInterfaceType(.wifi) {
            self = .wifi
        } else if path.usesInterfaceType(.cellular) {
            self = .cellular
        } else if path.usesInterfaceType(.wiredEthernet) {
            self = .ethernet
        } else {
            self = .other
        }
    }

    var shortLabel: String {
        switch self {
        case .wifi: return "Wi-Fi"
        case .cellular: return "Cell"
        case .ethernet: return "LAN"
        case .other: return "Net"
        case .offline: return "Off"
        }
    }

    var accessibilityLabel: String {
        switch self {
        case .wifi: return "Network: Wi-Fi"
        case .cellular: return "Network: Cellular"
        case .ethernet: return "Network: Ethernet"
        case .other: return "Network: Other"
        case .offline: return "Network: Offline"
        }
    }

    var systemImage: String {
        switch self {
        case .wifi: return "wifi"
        case .cellular: return "antenna.radiowaves.left.and.right"
        case .ethernet: return "cable.connector.horizontal"
        case .other: return "network"
        case .offline: return "network.slash"
        }
    }

    /// The coarser classification the session report reasons about. "Other" and "offline" both
    /// become unknown: neither tells the report anything it can turn into advice.
    var sessionNetworkKind: SessionNetworkKind {
        switch self {
        case .wifi: return .wifi
        case .cellular: return .cellular
        case .ethernet: return .wired
        case .other, .offline: return .unknown
        }
    }
}
#endif

/// Fires only when the server hands back something different from what was asked for.
///
/// Silence is the normal case and the point of the whole thing: a notice that appears on every
/// session teaches people to ignore it, so this one is worth reading precisely because it is rare.
struct StreamModeChangeNotice: Equatable, Identifiable {
    let requested: String
    let delivered: String
    let reason: Reason

    enum Reason: Equatable {
        case serverNegotiated
        case runtimeChange
        case safeRecovery(String)
    }

    var id: String { "\(requested)->\(delivered)" }

    var message: String {
        switch reason {
        case .serverNegotiated:
            return "Server chose \(delivered) — your \(requested) request wasn't available."
        case .runtimeChange:
            return "The game switched to \(delivered); you asked for \(requested)."
        case .safeRecovery(let why):
            return "Dropped to \(delivered) to keep the stream up. \(why)"
        }
    }

    /// Compares two geometries, ignoring the noise CloudMatch emits before video actually starts.
    static func between(
        requested: StreamVideoProfile,
        deliveredResolution: String?,
        reason: Reason
    ) -> StreamModeChangeNotice? {
        guard let deliveredResolution,
              let parts = parsedResolution(deliveredResolution),
              parts.width >= 320, parts.height >= 180 else { return nil }
        guard parts.width != requested.width || parts.height != requested.height else { return nil }
        return StreamModeChangeNotice(
            requested: "\(requested.width)x\(requested.height)",
            delivered: "\(parts.width)x\(parts.height)",
            reason: reason
        )
    }

    private static func parsedResolution(_ value: String) -> (width: Int, height: Int)? {
        let parts = value.split(separator: "x", maxSplits: 1).map(String.init)
        guard parts.count == 2, let w = Int(parts[0]), let h = Int(parts[1]), w > 0, h > 0 else { return nil }
        return (w, h)
    }
}

fileprivate enum NativeStreamGuidanceSheet: String, Identifiable {
    case streamTutorial
    case controllerTouchPrompt

    var id: String { rawValue }
}

fileprivate enum NativeStreamInputMode: Equatable {
    case nativeTouch
    case keyboardMouse
}

fileprivate enum NativeStreamInputModePrompt: String, Identifiable, Equatable {
    case switchToKeyboardMouse
    case switchToNativeTouch

    var id: String { rawValue }
    var title: String {
        switch self {
        case .switchToKeyboardMouse: return "Keyboard or mouse detected"
        case .switchToNativeTouch: return "Keyboard and mouse disconnected"
        }
    }
    var message: String {
        switch self {
        case .switchToKeyboardMouse: return "You’re using native touch. Switch to keyboard and mouse for this stream?"
        case .switchToNativeTouch: return "You’re using keyboard and mouse. Switch back to native touch for this stream?"
        }
    }
    var switchLabel: String {
        switch self {
        case .switchToKeyboardMouse: return "Switch to Keyboard & Mouse"
        case .switchToNativeTouch: return "Switch to Native Touch"
        }
    }
    var stayLabel: String {
        switch self {
        case .switchToKeyboardMouse: return "Stay on Native Touch"
        case .switchToNativeTouch: return "Keep Keyboard & Mouse"
        }
    }
}

struct StreamerView: View {
    #if os(iOS) && canImport(WebRTC)
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var coordinator: NativeStreamCoordinator
    @State private var controlsPanelExpanded = false

    init(
        session: ActiveSession,
        settings: AppSettings,
        membershipTier: String? = nil,
        onTouchLayoutChange: @escaping (String, TouchControlLayout) -> Void,
        onStreamerPreferencesChange: @escaping (StreamerPreferences) -> Void,
        onStreamSharpeningChange: @escaping (Bool, Double) -> Void,
        onFingerMouseEnabledChange: @escaping (Bool) -> Void,
        onPhoneRumbleFallbackChange: @escaping (Bool) -> Void,
        onStreamTutorialCompleted: @escaping () -> Void = {},
        onControllerTouchPromptDismissed: @escaping () -> Void = {},
        onStatsOverlayChange: @escaping (Bool) -> Void = { _ in },
        onTransportStable: @escaping () -> Void = {},
        onSelectedVideoProfileRetry: @escaping (String) -> Void,
        onNativeFallbackRequiresFreshEndpoint: @escaping (String) -> Void,
        onRuntimeSample: @escaping (StreamRuntimeSample) -> Void = { _ in },
        onSettingsChange: @escaping (AppSettings) -> Void = { _ in },
        onBuildBugReportDeck: @escaping () -> BugReportPreflightDeck = { BugReportPreflightDeck() },
        onSubmitBugReport: @escaping (BugReportDraft, BugReportPreflightDeck) async -> Result<String, Error> = { _, _ in
            .failure(BugReportError.invalid("Bug reporting isn't available in this build."))
        },
        onClose: @escaping () -> Void,
        onRetry: (() -> Void)? = nil
    ) {
        _coordinator = StateObject(
            wrappedValue: NativeStreamCoordinator(
                session: session,
                settings: settings,
                membershipTier: membershipTier,
                onTouchLayoutChange: onTouchLayoutChange,
                onStreamerPreferencesChange: onStreamerPreferencesChange,
                onStreamSharpeningChange: onStreamSharpeningChange,
                onFingerMouseEnabledChange: onFingerMouseEnabledChange,
                onPhoneRumbleFallbackChange: onPhoneRumbleFallbackChange,
                onStreamTutorialCompleted: onStreamTutorialCompleted,
                onControllerTouchPromptDismissed: onControllerTouchPromptDismissed,
                onStatsOverlayChange: onStatsOverlayChange,
                onTransportStable: onTransportStable,
                onSelectedVideoProfileRetry: onSelectedVideoProfileRetry,
                onRuntimeSample: onRuntimeSample,
                onSettingsChange: onSettingsChange,
                onBuildBugReportDeck: onBuildBugReportDeck,
                onSubmitBugReport: onSubmitBugReport,
                onClose: onClose,
                onRetry: onRetry
            )
        )
        _ = onNativeFallbackRequiresFreshEndpoint
    }

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                Color.black
                    .ignoresSafeArea()

                NativeStreamVideoView(coordinator: coordinator).equatable()
                    .ignoresSafeArea()

                NativeStreamTouchCaptureView(
                    inputBridge: coordinator.inputBridge,
                    inputEnabled: coordinator.fingerMouseCaptureEnabled,
                    nativeTouchEnabled: coordinator.nativeTouchCaptureEnabled,
                    geometry: { [weak coordinator] in
                        coordinator?.currentTouchGeometry() ?? NativeStreamTouchGeometry()
                    },
                    onZoomGesture: coordinator.applyFingerMouseZoom
                )
                    .ignoresSafeArea()

                if coordinator.shouldShowVirtualController {
                    NativeStreamVirtualControllerOverlay(
                        inputBridge: coordinator.inputBridge,
                        layout: coordinator.touchLayout,
                        touchSettings: coordinator.liveSettings.touch,
                        editing: coordinator.touchLayoutEditing,
                        inputEnabled: coordinator.virtualControllerInputEnabled,
                        onPositionChange: coordinator.setTouchLayoutPosition,
                        onHide: { coordinator.setTouchControllerVisible(false) },
                        onReset: coordinator.resetTouchLayout,
                        onDoneEditing: coordinator.endTouchLayoutEditing
                    )
                    .padding(.horizontal, max(12, proxy.safeAreaInsets.leading + 12))
                    .padding(.bottom, max(10, proxy.safeAreaInsets.bottom + 8))
                    .transition(.opacity)
                }

                VStack {
                    // Three stats positions, so the HUD can be moved off whichever corner the
                    // current game already uses. The overlay buttons take the opposite side.
                    HStack(alignment: .top, spacing: 10) {
                        let statsPosition = coordinator.streamerPreferences.statsPosition

                        if coordinator.showStatsOverlay, statsPosition == .left {
                            streamStatsPill
                        }

                        if statsPosition != .left {
                            streamOverlayButtons
                        }

                        Spacer(minLength: 10)

                        if coordinator.showStatsOverlay, statsPosition == .center {
                            streamStatsPill
                            Spacer(minLength: 10)
                        }

                        if statsPosition == .left {
                            streamOverlayButtons
                        }

                        if coordinator.showStatsOverlay, statsPosition == .right {
                            streamStatsPill
                        }
                    }
                    .padding(.top, max(22, proxy.safeAreaInsets.top + 8))
                    .padding(.horizontal, max(12, proxy.safeAreaInsets.leading + 12))

                    if let notice = coordinator.modeChangeNotice {
                        NativeStreamModePill(notice: notice) {
                            coordinator.dismissModeChangeNotice()
                        }
                        .padding(.horizontal, max(12, proxy.safeAreaInsets.leading + 12))
                        .padding(.top, 8)
                        .transition(.opacity.combined(with: .move(edge: .top)))
                    }

                    Spacer()

                    if coordinator.antiAfkActive {
                        NativeStreamAntiAfkIndicator()
                            .padding(.leading, max(12, proxy.safeAreaInsets.leading + 12))
                            .padding(.bottom, max(10, proxy.safeAreaInsets.bottom + 8))
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .top)
                .animation(.easeInOut(duration: 0.22), value: coordinator.modeChangeNotice)

                if coordinator.showStatusOverlay {
                    NativeStreamStatusOverlay(
                        status: coordinator.statusText,
                        detail: coordinator.detailText
                    )
                    .frame(maxWidth: min(proxy.size.width - 48, 420))
                    .allowsHitTesting(false)
                }

                if coordinator.controlsPanelVisible {
                    Color.black.opacity(0.24)
                        .ignoresSafeArea()
                        .onTapGesture { coordinator.dismissControlsPanelFromBackdrop() }

                    VStack {
                        if !controlsPanelExpanded { Spacer() }
                        NativeStreamControlsPanel(coordinator: coordinator, expanded: $controlsPanelExpanded)
                            .frame(width: controlsPanelExpanded ? proxy.size.width : min(proxy.size.width * 0.94, 760),
                                   height: controlsPanelExpanded ? proxy.size.height : min(proxy.size.height * 0.78, 700))
                            .padding(.bottom, controlsPanelExpanded ? 0 : max(14, proxy.safeAreaInsets.bottom + 10))
                    }
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                if let sessionWarningText = coordinator.sessionWarningText {
                    VStack {
                        Spacer()
                        NativeStreamSessionWarningBanner(message: sessionWarningText)
                            .frame(maxWidth: min(proxy.size.width - 32, 420))
                            .padding(.horizontal, 16)
                            .padding(.bottom, max(24, proxy.safeAreaInsets.bottom + 18))
                    }
                    .allowsHitTesting(false)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .zIndex(50)
                }
            }
            .animation(.easeInOut(duration: 0.18), value: coordinator.controlsPanelVisible)
            .animation(.easeInOut(duration: 0.18), value: coordinator.showStatsOverlay)
            .animation(.easeInOut(duration: 0.18), value: coordinator.shouldShowVirtualController)
            .animation(.easeInOut(duration: 0.18), value: coordinator.sessionWarningText)
            .task(id: coordinator.sessionID) {
                coordinator.handleScenePhase(scenePhase)
                coordinator.start(viewportSize: proxy.size)
            }
            .onAppear { StreamOrientation.setStreaming(true) }
            .onChangeCompat(of: scenePhase) { newPhase in
                coordinator.handleScenePhase(newPhase)
            }
            .onChangeCompat(of: proxy.size) { newSize in
                coordinator.updateViewportSize(newSize)
            }
            .onChangeCompat(of: coordinator.controlsPanelVisible) { visible in
                if !visible { controlsPanelExpanded = false }
            }
            .onDisappear {
                coordinator.handleViewDisappear(scenePhase: scenePhase)
                StreamOrientation.setStreaming(false)
            }
            .statusBarHidden(false)
        }
        .background(NativeStreamPointerLockPreference(requested:
            NativeStreamPointerCapturePolicy.shouldCapture(videoActive: coordinator.videoActive && !coordinator.showStatusOverlay,
                sceneActive: scenePhase == .active, controlsVisible: coordinator.controlsPanelVisible,
                editing: coordinator.touchLayoutEditing, guidanceVisible: coordinator.presentedGuidanceSheet != nil
                    || coordinator.inputModePrompt != nil,
                pipActive: coordinator.isPictureInPictureActive)))
        .sheet(item: $coordinator.presentedGuidanceSheet) { destination in
            switch destination {
            case .streamTutorial:
                NativeStreamTutorialSheet(coordinator: coordinator)
            case .controllerTouchPrompt:
                NativeControllerTouchPromptSheet(coordinator: coordinator)
            }
        }
        .alert(item: $coordinator.inputModePrompt) { prompt in
            Alert(
                title: Text(prompt.title),
                message: Text(prompt.message),
                primaryButton: .default(Text(prompt.switchLabel)) {
                    coordinator.acceptInputModePrompt(prompt)
                },
                secondaryButton: .cancel(Text(prompt.stayLabel)) {
                    coordinator.dismissInputModePrompt()
                }
            )
        }
        .onChangeCompat(of: coordinator.controlsPanelVisible) { _ in
            coordinator.presentPendingInputModePromptIfPossible()
        }
        .onChangeCompat(of: coordinator.presentedGuidanceSheet) { _ in
            coordinator.presentPendingInputModePromptIfPossible()
        }
    }

    @ViewBuilder
    private var streamStatsPill: some View {
        NativeStreamStatsPill(
            gameTitle: coordinator.gameTitle,
            status: coordinator.statusText,
            snapshot: coordinator.statsSnapshot,
            preferences: coordinator.streamerPreferences,
            metrics: coordinator.statsMetrics,
            deviceStatus: coordinator.deviceStatus,
            style: coordinator.statsDisplayStyle,
            sessionDurationText: coordinator.sessionDurationText
        )
        .transition(.opacity.combined(with: .move(edge: .top)))
    }

    private var streamOverlayButtons: some View {
        HStack(spacing: 10) {
            if coordinator.pictureInPictureAvailable {
                NativeStreamOverlayButton(
                    systemImage: coordinator.isPictureInPictureActive ? "pip.exit" : "pip.enter",
                    label: coordinator.isPictureInPictureActive ? "Stop Picture in Picture" : "Start Picture in Picture"
                ) {
                    coordinator.togglePictureInPicture()
                }
            }

            NativeStreamOverlayButton(systemImage: "slider.horizontal.3", label: "Stream controls") {
                coordinator.toggleControlsPanel()
            }
        }
    }
    #else
    private let session: ActiveSession
    private let onClose: () -> Void

    init(
        session: ActiveSession,
        settings: AppSettings,
        membershipTier: String? = nil,
        onTouchLayoutChange: @escaping (String, TouchControlLayout) -> Void,
        onStreamerPreferencesChange: @escaping (StreamerPreferences) -> Void,
        onStreamSharpeningChange: @escaping (Bool, Double) -> Void,
        onFingerMouseEnabledChange: @escaping (Bool) -> Void,
        onPhoneRumbleFallbackChange: @escaping (Bool) -> Void,
        onStreamTutorialCompleted: @escaping () -> Void = {},
        onControllerTouchPromptDismissed: @escaping () -> Void = {},
        onStatsOverlayChange: @escaping (Bool) -> Void = { _ in },
        onTransportStable: @escaping () -> Void = {},
        onSelectedVideoProfileRetry: @escaping (String) -> Void,
        onNativeFallbackRequiresFreshEndpoint: @escaping (String) -> Void,
        onRuntimeSample: @escaping (StreamRuntimeSample) -> Void = { _ in },
        onSettingsChange: @escaping (AppSettings) -> Void = { _ in },
        onBuildBugReportDeck: @escaping () -> BugReportPreflightDeck = { BugReportPreflightDeck() },
        onSubmitBugReport: @escaping (BugReportDraft, BugReportPreflightDeck) async -> Result<String, Error> = { _, _ in
            .failure(BugReportError.invalid("Bug reporting isn't available in this build."))
        },
        onClose: @escaping () -> Void,
        onRetry: (() -> Void)? = nil
    ) {
        self.session = session
        self.onClose = onClose
        _ = settings
        _ = membershipTier
        _ = onTouchLayoutChange
        _ = onStreamerPreferencesChange
        _ = onStreamSharpeningChange
        _ = onFingerMouseEnabledChange
        _ = onPhoneRumbleFallbackChange
        _ = onStreamTutorialCompleted
        _ = onControllerTouchPromptDismissed
        _ = onStatsOverlayChange
        _ = onTransportStable
        _ = onSelectedVideoProfileRetry
        _ = onNativeFallbackRequiresFreshEndpoint
        _ = onRuntimeSample
        _ = onSettingsChange
        _ = onBuildBugReportDeck
        _ = onSubmitBugReport
        _ = onRetry
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            VStack(spacing: 16) {
                Image(systemName: "display.trianglebadge.exclamationmark")
                    .font(.largeTitle)
                Text("Streaming Unavailable")
                    .font(.headline)
                Text("\(session.game.title) is ready, but this platform build does not include the iOS WebRTC runtime.")
                    .font(.subheadline)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)
                Button("Close") {
                    onClose()
                }
                .buttonStyle(.borderedProminent)
            }
            .padding()
            .foregroundStyle(.white)
        }
    }
    #endif
}

#if os(iOS) && canImport(WebRTC)
private struct NativeStreamStatsSnapshot: Equatable {
    var codec = "--"
    var effects = ""
    var presentationRates: NativeStreamPresentationRates?
    var colorMode = "--"
    var requestedColor = ""
    var resolution = "--"
    var fps: Int?
    var bitrateKbps: Int?
    var pingMs: Int?
    var decoded = 0
    var rendered = 0
    var dropped = 0
    var lossPercent = 0.0
    var jitterMs: Int?
    /// Input-to-output decode latency for the last window, in milliseconds.
    var decodeMs: Double?
    /// The zone the session is running in, for the HUD's Server metric.
    var serverLabel: String?
    var gpuLabel: String?
    var targetFps: Int = 60
    var inputSummary = "r0/p0"
    var detail = ""

    static let empty = NativeStreamStatsSnapshot()

    var decodeText: String {
        decodeMs.map { String(format: "%.1fms", $0) } ?? "--"
    }

    var lossText: String {
        String(format: "%.1f%%", lossPercent)
    }

    var fpsText: String {
        fps.map(String.init) ?? "--"
    }

    var bitrateText: String {
        guard let bitrateKbps else { return "--" }
        if bitrateKbps >= 1000 {
            return String(format: "%.1f Mbps", Double(bitrateKbps) / 1000.0)
        }
        return "\(bitrateKbps) Kbps"
    }

    var compactBitrateText: String {
        guard let bitrateKbps else { return "--" }
        if bitrateKbps >= 1000 {
            return String(format: "%.1fM", Double(bitrateKbps) / 1000.0)
        }
        return "\(bitrateKbps)K"
    }

    var pingText: String {
        pingMs.map { "\($0)ms" } ?? "--"
    }
}

private struct NativeStreamDeviceStatus: Equatable {
    var timeText: String = "--:--"
    var batteryPercent: Int?
    var batteryState: UIDevice.BatteryState = .unknown
    var networkTransport: NativeStreamNetworkTransport = .offline

    static func current(
        networkTransport: NativeStreamNetworkTransport = .offline
    ) -> NativeStreamDeviceStatus {
        UIDevice.current.isBatteryMonitoringEnabled = true
        let level = UIDevice.current.batteryLevel
        let percent = level >= 0 ? Int((level * 100).rounded()) : nil
        return NativeStreamDeviceStatus(
            timeText: timeFormatter.string(from: Date()),
            batteryPercent: percent,
            batteryState: UIDevice.current.batteryState,
            networkTransport: networkTransport
        )
    }

    var batterySymbol: String {
        if batteryState == .charging || batteryState == .full {
            return "battery.100.bolt"
        }
        guard let batteryPercent else { return "battery.100" }
        switch batteryPercent {
        case 76...100: return "battery.100"
        case 51...75: return "battery.75"
        case 26...50: return "battery.50"
        case 1...25: return "battery.25"
        default: return "battery.0"
        }
    }

    private static let timeFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.timeStyle = .short
        formatter.dateStyle = .none
        return formatter
    }()
}

/// The one overlay that is allowed to interrupt gameplay, because it explains something the
/// player would otherwise blame on their connection.
private struct NativeStreamModePill: View {
    let notice: StreamModeChangeNotice
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "rectangle.on.rectangle.angled")
                .font(.caption)
                .accessibilityHidden(true)
            Text(notice.message)
                .font(.caption)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 8)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .font(.caption2.weight(.bold))
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Dismiss")
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(maxWidth: 460, alignment: .leading)
        .background(.black.opacity(0.72), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(OpenNOWPalette.statusFair.opacity(0.6), lineWidth: 1)
        )
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isStaticText)
    }
}

/// A twelve-point dot, bottom-left, present only while the keepalive is running. Deliberately
/// nothing more: it exists so a player can tell the input they did not make came from OpenNOW.
private struct NativeStreamAntiAfkIndicator: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulsing = false

    var body: some View {
        Circle()
            .fill(OpenNOWPalette.statusNotice)
            .frame(width: 12, height: 12)
            .opacity(pulsing ? 0.35 : 0.9)
            .animation(
                reduceMotion ? nil : .easeInOut(duration: 1.4).repeatForever(autoreverses: true),
                value: pulsing
            )
            .onAppear { pulsing = !reduceMotion }
            .accessibilityLabel("Anti-AFK is keeping the session awake")
    }
}

private struct NativeStreamStatusOverlay: View {
    let status: String
    let detail: String

    var body: some View {
        VStack(spacing: 8) {
            ProgressView()
                .tint(.white)
            Text(status)
                .font(.headline)
            if !detail.isEmpty {
                Text(detail)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(0.72))
                    .multilineTextAlignment(.center)
                    .lineLimit(3)
            }
        }
        .foregroundStyle(.white)
        .padding(16)
        .background(.black.opacity(0.58), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

private struct NativeStreamSessionWarningBanner: View {
    let message: String

    var body: some View {
        Label {
            Text(message)
                .font(.subheadline.weight(.semibold))
        } icon: {
            Image(systemName: "clock.badge.exclamationmark")
                .foregroundStyle(.orange)
        }
        .foregroundStyle(.primary)
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.orange.opacity(0.28), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.24), radius: 12, y: 4)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(message)
    }
}

private struct NativeStreamOverlayButton: View {
    let systemImage: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 17, weight: .semibold))
                .frame(width: 42, height: 42)
        }
        .buttonStyle(.plain)
        .foregroundStyle(.white)
        .background(.black.opacity(0.52), in: Circle())
        .overlay(Circle().stroke(Color.white.opacity(0.12), lineWidth: 1))
        .accessibilityLabel(label)
    }
}

/// The in-stream stats HUD.
///
/// Two rules govern everything here. First, a reading in the normal range renders in plain white —
/// tinting the healthy case is what makes the unhealthy one hard to spot. Second, the whole view
/// reads a throttled snapshot, never the live stats stream: it must not re-render on the video's
/// timeline, because a HUD that costs four percent of a frame budget costs the player two frames
/// a second.
private struct NativeStreamStatsPill: View {
    let gameTitle: String
    let status: String
    let snapshot: NativeStreamStatsSnapshot
    let preferences: StreamerPreferences
    let metrics: StreamStatsMetrics
    let deviceStatus: NativeStreamDeviceStatus
    let style: StreamStatsStyle
    let sessionDurationText: String?

    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor

    private struct Readout: Identifiable {
        let id: String
        let label: String
        let compact: String
        let detailed: String
        var level: StreamQualityLevel = .good
        var symbol: String?
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Group {
                if style == .compact {
                    compactPill
                } else {
                    detailedPanel
                }
            }
            if let rates = snapshot.presentationRates {
                Text(rates.label).font(.caption2.monospacedDigit()).foregroundStyle(.white)
                    .padding(.horizontal, 10).padding(.vertical, 4)
                    .background(.black.opacity(0.50), in: Capsule())
            }
            if !snapshot.effects.isEmpty {
                Text(snapshot.effects).font(.caption2.monospacedDigit()).foregroundStyle(.white)
                    .padding(.horizontal, 10).padding(.vertical, 4)
                    .background(.black.opacity(0.50), in: Capsule())
            }
        }
        // The HUD sits under a thumb and updates every second, which makes it hostile to
        // VoiceOver's swipe navigation. The same numbers are readable in the control panel.
        .accessibilityHidden(true)
    }

    private var compactPill: some View {
        ViewThatFits(in: .horizontal) {
            readoutRow(readouts, spacing: 8)
            readoutRow(Array(readouts.prefix(4)), spacing: 7)
            readoutRow(Array(readouts.prefix(2)), spacing: 6)
        }
        .font(.caption2.weight(.semibold).monospacedDigit())
        .lineLimit(1)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.black.opacity(0.50), in: Capsule())
        .overlay(Capsule().stroke(Color.white.opacity(0.10), lineWidth: 1))
    }

    private func readoutRow(_ items: [Readout], spacing: CGFloat) -> some View {
        HStack(spacing: spacing) {
            if let sessionDurationText {
                Text(sessionDurationText).foregroundStyle(.white)
            }
            ForEach(items) { item in
                HStack(spacing: 3) {
                    if let symbol = item.symbol {
                        Image(systemName: symbol)
                    }
                    if differentiateWithoutColor, let glyph = item.level.glyph, item.level != .good {
                        Image(systemName: glyph)
                    }
                    Text(item.compact)
                }
                .foregroundStyle(item.level.tint ?? Color.white)
            }
        }
    }

    private var detailedPanel: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(gameTitle)
                .font(.caption.weight(.bold))
                .lineLimit(1)

            ForEach(readouts) { item in
                HStack(spacing: 8) {
                    Text(item.label)
                        .foregroundStyle(.white.opacity(0.66))
                    Spacer(minLength: 12)
                    if differentiateWithoutColor, let glyph = item.level.glyph, item.level != .good {
                        Image(systemName: glyph).font(.caption2)
                    }
                    Text(item.detailed)
                        .foregroundStyle(item.level.tint ?? Color.white)
                }
                .font(.caption2.monospacedDigit())
                .lineLimit(1)
            }

            if let sessionDurationText {
                HStack(spacing: 8) {
                    Label(sessionDurationText, systemImage: "stopwatch")
                    if status != "Streaming" { Text(status).lineLimit(1) }
                    if preferences.audioMuted { Text("muted") }
                }
                .font(.caption2)
                .foregroundStyle(.white.opacity(0.66))
                .padding(.top, 2)
            }
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .frame(minWidth: 168, maxWidth: 300, alignment: .leading)
        .background(.black.opacity(0.70), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.white.opacity(0.10), lineWidth: 1))
    }

    /// Built in a fixed order so the HUD never reshuffles between samples — a readout that moves
    /// is a readout nobody can glance at.
    private var readouts: [Readout] {
        var items: [Readout] = []

        if metrics.fps {
            let level = snapshot.fps.map {
                StreamQuality.frameRate(Double($0), targetFps: snapshot.targetFps)
            } ?? .good
            items.append(Readout(
                id: "fps",
                label: "Frame rate",
                compact: "\(snapshot.fpsText) fps",
                detailed: "\(snapshot.fpsText) / \(snapshot.targetFps)",
                level: level
            ))
        }
        if metrics.ping {
            let level = snapshot.pingMs.map(StreamQuality.latency) ?? .good
            items.append(Readout(
                id: "ping",
                label: "Ping",
                compact: snapshot.pingText,
                detailed: snapshot.pingText,
                level: level
            ))
        }
        if metrics.latency {
            let level = snapshot.decodeMs.map {
                StreamQuality.decode($0, targetFps: snapshot.targetFps, actualFps: snapshot.fps.map(Double.init))
            } ?? .good
            items.append(Readout(
                id: "decode",
                label: "Decode",
                compact: snapshot.decodeText,
                detailed: snapshot.decodeText,
                level: level
            ))
        }
        if metrics.bitrate {
            items.append(Readout(
                id: "bitrate",
                label: "Bitrate",
                compact: snapshot.compactBitrateText,
                detailed: snapshot.bitrateText
            ))
        }
        if metrics.packetLoss {
            items.append(Readout(
                id: "loss",
                label: "Loss",
                compact: snapshot.lossText,
                detailed: snapshot.lossText,
                level: StreamQuality.packetLoss(snapshot.lossPercent)
            ))
        }
        if metrics.resolution {
            items.append(Readout(
                id: "resolution",
                label: "Resolution",
                compact: snapshot.resolution,
                detailed: snapshot.resolution
            ))
        }
        if metrics.codec {
            items.append(Readout(
                id: "codec",
                label: "Codec",
                compact: snapshot.codec,
                detailed: snapshot.codec
            ))
        }
        if metrics.codec {
            items.append(Readout(id: "color", label: "Color", compact: snapshot.colorMode + (snapshot.requestedColor.hasSuffix("HDR") && snapshot.colorMode.contains("SDR") ? " (HDR requested)" : ""),
                                 detailed: snapshot.requestedColor.isEmpty ? snapshot.colorMode : "Received: " + snapshot.colorMode + " · Requested: " + snapshot.requestedColor))
        }
        if metrics.location, let server = snapshot.serverLabel, !server.isEmpty {
            items.append(Readout(
                id: "server",
                label: "Server",
                compact: server,
                detailed: server
            ))
        }
        if let gpu = snapshot.gpuLabel, !gpu.isEmpty {
            items.append(Readout(id: "gpu", label: "GPU", compact: gpu, detailed: gpu))
        }
        if metrics.connection {
            items.append(Readout(
                id: "network",
                label: "Network",
                compact: deviceStatus.networkTransport.shortLabel,
                detailed: deviceStatus.networkTransport.shortLabel,
                symbol: deviceStatus.networkTransport.systemImage
            ))
        }
        if metrics.battery {
            let percent = deviceStatus.batteryPercent.map { "\($0)%" } ?? "--"
            items.append(Readout(
                id: "battery",
                label: "Battery",
                compact: percent,
                detailed: percent,
                // Below 20% and not charging is worth flagging: the stream is the reason.
                level: batteryLevel,
                symbol: deviceStatus.batterySymbol
            ))
        }
        if preferences.showStatsClock {
            items.append(Readout(
                id: "clock",
                label: "Time",
                compact: deviceStatus.timeText,
                detailed: deviceStatus.timeText
            ))
        }
        return items
    }

    private var batteryLevel: StreamQualityLevel {
        guard deviceStatus.batteryState != .charging, deviceStatus.batteryState != .full,
              let percent = deviceStatus.batteryPercent else { return .good }
        switch percent {
        case ..<10: return .poor
        case ..<20: return .fair
        default: return .good
        }
    }
}

private struct NativeStreamTutorialSheet: View {
    @ObservedObject var coordinator: NativeStreamCoordinator
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Image(systemName: "slider.horizontal.3")
                        .font(.system(size: 44, weight: .semibold))
                        .foregroundStyle(.tint)
                        .accessibilityHidden(true)

                    VStack(alignment: .leading, spacing: 6) {
                        Text("Step 1 of 2")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.tint)
                        Text("Open Stream Controls")
                            .font(.title2.bold())
                        Text("Stream controls let you manage audio, video, input, Picture in Picture, and your touch layout without ending the session.")
                            .foregroundStyle(.secondary)
                    }

                    Label("Use the sliders button at the top of the stream.", systemImage: "1.circle.fill")
                    Label("The stream keeps running while controls are open.", systemImage: "2.circle.fill")
                    Label("Choose Done to return to gameplay.", systemImage: "3.circle.fill")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(20)
            }
            .safeAreaInset(edge: .bottom) {
                HStack(spacing: 10) {
                    Button("Skip Tutorial") {
                        coordinator.skipStreamTutorial()
                        dismiss()
                    }
                    .buttonStyle(.bordered)

                    Button("Show Stream Controls") {
                        coordinator.beginStreamTutorialControlsStep()
                        dismiss()
                    }
                    .buttonStyle(.borderedProminent)
                }
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .background(.bar)
            }
            .navigationTitle("Stream Tutorial")
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.medium, .large])
        .interactiveDismissDisabled()
    }
}

private struct NativeControllerTouchPromptSheet: View {
    @ObservedObject var coordinator: NativeStreamCoordinator
    @Environment(\.dismiss) private var dismiss
    @State private var doNotShowAgain = false

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 18) {
                Label("Controller Connected", systemImage: "gamecontroller.fill")
                    .font(.title2.bold())
                    .foregroundStyle(.tint)

                Text("OpenNOW hides the on-screen controller while a physical controller is connected. You can keep it hidden or show both for this stream.")
                    .foregroundStyle(.secondary)

                Toggle("Don't show this again", isOn: $doNotShowAgain)

                VStack(spacing: 10) {
                    Button("Show Both") {
                        coordinator.resolveControllerTouchPrompt(
                            showTouchControls: true,
                            doNotShowAgain: doNotShowAgain
                        )
                        dismiss()
                    }
                    .buttonStyle(.borderedProminent)
                    .frame(maxWidth: .infinity)

                    Button("Keep Touch Controls Hidden") {
                        coordinator.resolveControllerTouchPrompt(
                            showTouchControls: false,
                            doNotShowAgain: doNotShowAgain
                        )
                        dismiss()
                    }
                    .buttonStyle(.bordered)
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(20)
            .navigationTitle("Input")
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.medium])
        .interactiveDismissDisabled()
    }
}

private struct NativeStreamTutorialDoneCallout: View {
    @ObservedObject var coordinator: NativeStreamCoordinator

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "checkmark.circle.fill")
                .font(.title2)
                .foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 2) {
                Text("Step 2 of 2")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.tint)
                Text("Choose Done above to return to your game.")
                    .font(.subheadline.weight(.semibold))
            }
            Spacer(minLength: 8)
            Button("Skip") {
                coordinator.skipStreamTutorial()
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(12)
        .background(Color.accentColor.opacity(0.12), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Color.accentColor.opacity(0.35), lineWidth: 1)
        )
    }
}

/// S7 — the in-stream control centre.
///
/// Five pages rather than one long scroll. The single-column version the iOS build shipped with
/// meant reaching the touch-layout sliders took a dozen swipes over a live game; grouping by task
/// keeps every page to roughly one screen. The Main page carries only what you actually reach for
/// mid-session — audio, stats, the session clock, the way out.
private struct NativeStreamControlsPanel: View {
    @ObservedObject var coordinator: NativeStreamCoordinator
    @Binding var expanded: Bool
    @State private var page: Page = .main
    @State private var keyboardText = ""
    @State private var keyboardPresented = false
    @State private var bugReportDeck: BugReportPreflightDeck?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private enum Page: Hashable {
        case main
        case statsHUD
        case touchControls
        case mouseMode

        var title: String {
            switch self {
            case .main: return "Stream Controls"
            case .statsHUD: return "Status Bar"
            case .touchControls: return "Touch Controls"
            case .mouseMode: return "Mouse Mode"
            }
        }

        /// Second line under the title. On Main it is the game; elsewhere it says what the page
        /// is for, which is the pattern the Android panel uses.
        func subtitle(gameTitle: String) -> String {
            switch self {
            case .main: return gameTitle
            case .statsHUD: return "Choose what the overlay shows"
            case .touchControls: return "Size, style and placement"
            case .mouseMode: return "How touch and sticks reach the game"
            }
        }

        var order: Int {
            switch self {
            case .main: return 0
            case .statsHUD: return 1
            case .touchControls: return 2
            case .mouseMode: return 3
            }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    switch page {
                    case .main: mainPage
                    case .statsHUD: statsPage
                    case .touchControls: touchPage
                    case .mouseMode: mousePage
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 14)
                // Forward going in, back coming out — the same directional cue Android uses, so
                // the panel reads as a stack rather than a set of unrelated screens.
                .transition(reduceMotion ? .opacity : .asymmetric(
                    insertion: .move(edge: page == .main ? .leading : .trailing).combined(with: .opacity),
                    removal: .move(edge: page == .main ? .trailing : .leading).combined(with: .opacity)
                ))
                .id(page)
            }
            .scrollIndicators(.visible)
            .clipped()
        }
        // Fixed dark: this sits on live video, where the system's light grouped colour would put
        // a near-white slab over the game for anyone not in dark mode.
        .foregroundStyle(OpenNOWPalette.textOnDark)
        .environment(\.colorScheme, .dark)
        .background(
            OpenNOWPalette.panelOverVideo,
            in: RoundedRectangle(cornerRadius: expanded ? 0 : 16, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: expanded ? 0 : 16, style: .continuous)
                .strokeBorder(OpenNOWPalette.hairlineOverVideo, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: expanded ? 0 : 16, style: .continuous))
        .shadow(color: .black.opacity(0.24), radius: 16, y: 8)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.18), value: page)
        .sheet(isPresented: $keyboardPresented) {
            NativeStreamKeyboardSheet(coordinator: coordinator, text: $keyboardText)
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
        }
        .sheet(item: $bugReportDeck) { deck in
            BugReportView(deck: deck) { draft in
                await coordinator.submitBugReport(draft, deck: deck)
            }
        }
    }

    // MARK: Header

    private var header: some View {
        VStack(alignment: .leading, spacing: 10) {
            if coordinator.tutorialDoneCalloutVisible, page == .main {
                NativeStreamTutorialDoneCallout(coordinator: coordinator)
            }

            HStack(spacing: 8) {
                if page != .main {
                    // A labelled pill rather than a bare chevron, as on Android: on a panel over
                    // live video the word is what makes it unmistakable at a glance.
                    NativeStreamPanelPillButton(action: { page = .main }) {
                        Label("Back", systemImage: "chevron.left")
                            .labelStyle(.titleAndIcon)
                    }
                }

                VStack(alignment: .leading, spacing: 1) {
                    Text(page.title)
                        .font(.subheadline.weight(.bold))
                        .lineLimit(1)
                    Text(page.subtitle(gameTitle: coordinator.gameTitle))
                        .font(StreamPanelStyle.supporting)
                        .foregroundStyle(StreamPanelStyle.supportingColor)
                        .lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                if page == .main {
                    NativeStreamPanelPillButton(action: { keyboardPresented = true }) {
                        Image(systemName: "keyboard")
                    }
                    .accessibilityLabel("Keyboard")

                    NativeStreamPanelPillButton(action: { coordinator.close() }) {
                        Text("Exit")
                    }
                }

                NativeStreamPanelPillButton(action: { withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.22)) { expanded.toggle() } }) {
                    Image(systemName: expanded ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                }
                .accessibilityLabel(expanded ? "Collapse controls" : "Expand controls")

                NativeStreamPanelPillButton(prominent: true, action: { coordinator.finishControlsPanel() }) {
                    Text("Done")
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 14)
    }

    // MARK: Main

    private var mainPage: some View {
        Group {
            NativeStreamPanelSection(title: "Session") {
                if let sessionDurationText = coordinator.sessionDurationText {
                    NativeStreamSessionTimeRow(
                        value: sessionDurationText,
                        progress: coordinator.sessionTimerProgress,
                        isWarning: coordinator.sessionTimerWarningActive
                    )
                }
                NativeStreamToggleRow(
                    title: "Audio",
                    value: coordinator.streamerPreferences.audioMuted ? "Muted" : "On",
                    isOn: Binding(
                        get: { !coordinator.streamerPreferences.audioMuted },
                        set: { coordinator.setAudioEnabled($0) }
                    )
                )
                NativeStreamToggleRow(
                    title: "Stream stats",
                    value: coordinator.showStatsOverlay ? "On" : "Off",
                    isOn: Binding(
                        get: { coordinator.showStatsOverlay },
                        set: { coordinator.setStatsOverlayVisible($0) }
                    )
                )
                if coordinator.pictureInPictureAvailable {
                    NativeStreamActionRow(
                        title: "Picture in Picture",
                        value: coordinator.isPictureInPictureActive ? "Active" : "Ready",
                        actionLabel: coordinator.isPictureInPictureActive ? "Stop" : "Start"
                    ) {
                        coordinator.togglePictureInPicture()
                    }
                }
                HStack(spacing: 8) {
                    NativeStreamKeyButton(title: "Esc") { coordinator.sendVirtualKey(.escape) }
                    NativeStreamKeyButton(title: "Enter") { coordinator.sendVirtualKey(.enter) }
                    NativeStreamKeyButton(title: "Delete") { coordinator.sendVirtualKey(.backspace) }
                }
                if coordinator.liveSettings.clipboardPasteEnabled {
                    NativeStreamActionRow(
                        title: "Paste",
                        value: "Types the clipboard into the game",
                        actionLabel: "Paste"
                    ) {
                        coordinator.pasteClipboardIntoStream()
                    }
                }
            }

            NativeStreamPanelSection(title: "More") {
                NativeStreamActionRow(title: "Stats & HUD", value: "\(coordinator.statsMetrics.enabledCount) metrics", actionLabel: "Open") {
                    page = .statsHUD
                }
                NativeStreamActionRow(title: "Touch controls", value: coordinator.streamerPreferences.touchControllerVisible ? "Shown" : "Hidden", actionLabel: "Open") {
                    page = .touchControls
                }
                NativeStreamActionRow(title: "Mouse & touch input", value: coordinator.resolvedTouchModeLabel, actionLabel: "Open") {
                    page = .mouseMode
                }
                NativeStreamActionRow(title: "Report a problem", value: "Attaches this session", actionLabel: "Open") {
                    bugReportDeck = coordinator.bugReportPreflightDeck()
                }
            }

            NativeStreamPanelSection(title: "Picture") {
                NativeStreamToggleRow(title: "Metal 4 rendering",
                    value: coordinator.liveSettings.metal4Enabled ? "On" : "Off",
                    isOn: Binding(get: { coordinator.liveSettings.metal4Enabled },
                        set: { value in coordinator.updateLiveSettings { $0.metal4Enabled = value } }))
                NativeStreamToggleRow(title: "MetalFX upscaling",
                    value: coordinator.liveSettings.metalFXUpscalingEnabled ? "On" : "Off",
                    isOn: Binding(get: { coordinator.liveSettings.metalFXUpscalingEnabled },
                        set: { value in coordinator.updateLiveSettings { $0.metalFXUpscalingEnabled = value } }))
                if let rates = coordinator.statsSnapshot.presentationRates {
                    Text(rates.label).font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                }
                if !coordinator.statsSnapshot.effects.isEmpty {
                    Text(coordinator.statsSnapshot.effects).font(.footnote).foregroundStyle(.secondary)
                }
                NativeStreamToggleRow(
                    title: "Stream sharpening",
                    value: coordinator.streamSharpeningEnabled ? "On" : "Off",
                    isOn: Binding(
                        get: { coordinator.streamSharpeningEnabled },
                        set: { coordinator.setStreamSharpeningEnabled($0) }
                    )
                )
                if coordinator.streamSharpeningEnabled {
                    NativeStreamSliderRow(
                        title: "Sharpness",
                        value: Binding(
                            get: { coordinator.streamSharpeningAmount },
                            set: { coordinator.setStreamSharpeningAmount($0) }
                        ),
                        range: 0...1
                    )
                }
                NativeStreamToggleRow(
                    title: "Stretch to fill",
                    value: coordinator.streamerPreferences.stretchStreamToFill ? "Fill" : "Fit",
                    isOn: Binding(
                        get: { coordinator.streamerPreferences.stretchStreamToFill },
                        set: { coordinator.setStretchStreamToFill($0) }
                    )
                )
                NativeStreamInfoRow(title: "Codec", value: coordinator.selectedCodecLabel)
                NativeStreamInfoRow(title: "Resolution", value: coordinator.profileLabel)
            }

        }
    }

    // MARK: Stats & HUD

    private var statsPage: some View {
        Group {
            NativeStreamPanelSection(title: "Appearance") {
                NativeStreamActionRow(title: "Style", value: coordinator.statsDisplayStyle.label, actionLabel: "Change") {
                    coordinator.cycleStatsStyle()
                }
                NativeStreamActionRow(
                    title: "Position",
                    value: coordinator.streamerPreferences.statsPosition.label,
                    actionLabel: "Move"
                ) {
                    coordinator.toggleStatsPosition()
                }
            }

            NativeStreamPanelSection(title: "Connection metrics") {
                metricToggle("Frame rate", \.fps)
                metricToggle("Ping", \.ping)
                metricToggle("Decode", \.latency)
                metricToggle("Bitrate", \.bitrate)
                metricToggle("Packet loss", \.packetLoss)
            }

            NativeStreamPanelSection(title: "Session metrics") {
                metricToggle("Resolution", \.resolution)
                metricToggle("Codec", \.codec)
                metricToggle("Server", \.location)
                metricToggle("Battery", \.battery)
                metricToggle("Network", \.connection)
                NativeStreamToggleRow(
                    title: "Clock",
                    value: coordinator.streamerPreferences.showStatsClock ? "Shown" : "Hidden",
                    isOn: Binding(
                        get: { coordinator.streamerPreferences.showStatsClock },
                        set: { coordinator.setStatsClockVisible($0) }
                    )
                )
            }
        }
    }

    private func metricToggle(
        _ title: String,
        _ keyPath: WritableKeyPath<StreamStatsMetrics, Bool>
    ) -> some View {
        let metrics = coordinator.statsMetrics
        let isLastEnabled = metrics[keyPath: keyPath] && metrics.enabledCount == 1
        return NativeStreamToggleRow(
            title: title,
            value: metrics[keyPath: keyPath] ? "Shown" : "Hidden",
            isOn: Binding(
                get: { coordinator.statsMetrics[keyPath: keyPath] },
                set: { newValue in
                    // The last metric cannot be turned off — an empty HUD looks like a bug and
                    // there is no way back to this page without one.
                    guard newValue || coordinator.statsMetrics.enabledCount > 1 else { return }
                    coordinator.updateLiveSettings { $0.streamStatsMetrics[keyPath: keyPath] = newValue }
                }
            )
        )
        .disabled(isLastEnabled)
    }

    // MARK: Touch controls

    private var touchPage: some View {
        Group {
            NativeStreamPanelSection(title: "Controller") {
                NativeStreamToggleRow(
                    title: "Touch controller",
                    value: touchControllerStateLabel,
                    isOn: Binding(
                        get: { coordinator.streamerPreferences.touchControllerVisible },
                        set: { coordinator.setTouchControllerVisible($0) }
                    )
                )
                NativeStreamActionRow(
                    title: "Style",
                    value: coordinator.liveSettings.touch.style.label,
                    actionLabel: "Change"
                ) {
                    coordinator.updateLiveSettings {
                        $0.touch.style = $0.touch.style == .solid ? .outline : .solid
                    }
                }
                NativeStreamActionRow(
                    title: "Joystick",
                    value: coordinator.liveSettings.touch.joystickMode.label,
                    actionLabel: "Change"
                ) {
                    coordinator.updateLiveSettings {
                        $0.touch.joystickMode = $0.touch.joystickMode == .fixed ? .dynamic : .fixed
                    }
                }
                NativeStreamActionRow(
                    title: "Aim lock",
                    value: coordinator.liveSettings.touch.aimMode.label,
                    actionLabel: "Change"
                ) {
                    coordinator.updateLiveSettings {
                        $0.touch.aimMode = $0.touch.aimMode == .lockJoystick ? .lockZone : .lockJoystick
                    }
                }
                NativeStreamSliderRow(
                    title: "Dead zone",
                    value: Binding(
                        get: { coordinator.liveSettings.touch.joystickDeadZone },
                        set: { value in coordinator.updateLiveSettings { $0.touch.joystickDeadZone = value } }
                    ),
                    range: 0...0.3
                )
                NativeStreamToggleRow(
                    title: "Controller passthrough",
                    value: coordinator.streamerPreferences.physicalControllerPassthrough ? "On" : "Off",
                    isOn: Binding(
                        get: { coordinator.streamerPreferences.physicalControllerPassthrough },
                        set: { coordinator.setPhysicalControllerPassthrough($0) }
                    )
                )
                NativeStreamToggleRow(
                    title: "Rumble",
                    value: coordinator.phoneRumbleFallbackEnabled ? "On" : "Off",
                    isOn: Binding(
                        get: { coordinator.phoneRumbleFallbackEnabled },
                        set: { coordinator.setPhoneRumbleFallback($0) }
                    )
                )
            }

            NativeStreamPanelSection(title: "Layout") {
                NativeStreamActionRow(
                    title: "Edit layout",
                    value: "Drag control groups",
                    actionLabel: coordinator.touchLayoutEditing ? "Resume" : "Edit"
                ) {
                    coordinator.beginTouchLayoutEditing()
                }
                NativeStreamSliderRow(
                    title: "Layout scale",
                    value: Binding(
                        get: { coordinator.touchLayout.scale },
                        set: { coordinator.setTouchLayoutScale($0) }
                    ),
                    range: 0.6...1.4
                )
                NativeStreamSliderRow(
                    title: "Button size",
                    value: Binding(
                        get: { coordinator.touchLayout.buttonScale },
                        set: { coordinator.setTouchButtonScale($0) }
                    ),
                    range: 0.65...1.5
                )
                NativeStreamSliderRow(
                    title: "Stick size",
                    value: Binding(
                        get: { coordinator.touchLayout.stickScale },
                        set: { coordinator.setTouchStickScale($0) }
                    ),
                    range: 0.65...1.5
                )
                NativeStreamSliderRow(
                    title: "Opacity",
                    value: Binding(
                        get: { coordinator.touchLayout.opacity },
                        set: { coordinator.setTouchOpacity($0) }
                    ),
                    range: 0.15...1.0
                )
                NativeStreamActionRow(title: "Reset layout", value: "Back to defaults", actionLabel: "Reset") {
                    coordinator.resetTouchLayout()
                }
            }
        }
    }

    private var touchControllerStateLabel: String {
        guard coordinator.streamerPreferences.touchControllerVisible else { return "Hidden" }
        guard coordinator.physicalControllerConnected else { return "Shown" }
        return coordinator.showTouchControlsWithPhysicalController
            ? "Shown with controller"
            : "Hidden while controller connected"
    }

    // MARK: Mouse & touch input

    private var mousePage: some View {
        Group {
            NativeStreamPanelSection(title: "How touch is sent") {
                NativeStreamActionRow(
                    title: "Touch mode",
                    value: coordinator.liveSettings.touch.nativeTouchMode.label,
                    actionLabel: "Change"
                ) {
                    coordinator.updateLiveSettings {
                        let order = NativeTouchMode.allCases
                        let index = order.firstIndex(of: $0.touch.nativeTouchMode) ?? 0
                        $0.touch.nativeTouchMode = order[(index + 1) % order.count]
                    }
                }
                // The resolved state is what actually matters, and it is not obvious from the
                // three inputs above it.
                NativeStreamInfoRow(title: "Currently", value: coordinator.resolvedTouchModeLabel)
                if coordinator.liveSettings.touch.nativeTouchMode != .never {
                    NativeStreamSliderRow(
                        title: "Touch scroll speed",
                        value: Binding(
                            get: { coordinator.liveSettings.touch.nativeTouchScrollScale },
                            set: { value in coordinator.updateLiveSettings { $0.touch.nativeTouchScrollScale = value } }
                        ),
                        range: 0.25...2
                    )
                    NativeStreamSliderRow(
                        title: "Tap stability",
                        value: Binding(
                            get: { coordinator.liveSettings.touch.nativeTouchJitterThreshold },
                            set: { value in coordinator.updateLiveSettings { $0.touch.nativeTouchJitterThreshold = value } }
                        ),
                        range: 0...24
                    )
                }
            }

            NativeStreamPanelSection(title: "Pointer") {
                NativeStreamToggleRow(
                    title: "Finger mouse",
                    value: coordinator.fingerMouseEnabled ? "On" : "Off",
                    isOn: Binding(
                        get: { coordinator.fingerMouseEnabled },
                        set: { coordinator.setFingerMouseEnabled($0) }
                    )
                )
                if coordinator.fingerMouseEnabled {
                    NativeStreamToggleRow(
                        title: "Tap clicks where you touch",
                        value: coordinator.liveSettings.touch.mouseDirectClick ? "On" : "Off",
                        isOn: Binding(
                            get: { coordinator.liveSettings.touch.mouseDirectClick },
                            set: { value in coordinator.updateLiveSettings { $0.touch.mouseDirectClick = value } }
                        )
                    )
                }
                NativeStreamToggleRow(
                    title: "Controller mouse mode",
                    value: coordinator.liveSettings.controllerMouseEmulation ? "On" : "Off",
                    isOn: Binding(
                        get: { coordinator.liveSettings.controllerMouseEmulation },
                        set: { value in coordinator.updateLiveSettings { $0.controllerMouseEmulation = value } }
                    )
                )
                NativeStreamSliderRow(
                    title: "Mouse sensitivity",
                    value: Binding(
                        get: { coordinator.liveSettings.mouseSensitivity },
                        set: { value in coordinator.updateLiveSettings { $0.mouseSensitivity = value } }
                    ),
                    range: 0.25...3
                )
                NativeStreamSliderRow(
                    title: "Scroll sensitivity",
                    value: Binding(
                        get: { Double(coordinator.liveSettings.mouseScrollSensitivity) },
                        set: { value in
                            coordinator.updateLiveSettings { $0.mouseScrollSensitivity = Int(value.rounded()) }
                        }
                    ),
                    range: 10...100
                )
            }
        }
    }
}

private struct NativeStreamKeyboardSheet: View {
    @ObservedObject var coordinator: NativeStreamCoordinator
    @Binding var text: String

    @Environment(\.dismiss) private var dismiss
    @FocusState private var textFieldFocused: Bool
    @State private var sendError: String?
    @State private var showClearConfirmation = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextEditor(text: $text)
                        .focused($textFieldFocused)
                        .frame(minHeight: 96)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityLabel("Text to type into stream")

                    Button {
                        sendText()
                    } label: {
                        Label("Send", systemImage: "paperplane.fill")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(text.isEmpty)

                    if let sendError {
                        Text(sendError)
                            .font(.footnote)
                            .foregroundStyle(.red)
                    }
                } header: {
                    Text("Type into Stream")
                } footer: {
                    Text("Text is sent directly to the active game. Nothing is copied to the system clipboard.")
                }

                Section("Keys") {
                    LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                        NativeStreamKeyButton(title: "Backspace") {
                            coordinator.sendVirtualKey(.backspace)
                        }
                        NativeStreamKeyButton(title: "Enter") {
                            coordinator.sendVirtualKey(.enter)
                        }
                        NativeStreamKeyButton(title: "Escape") {
                            coordinator.sendVirtualKey(.escape)
                        }
                        NativeStreamKeyButton(title: "Clear") {
                            requestClear()
                        }
                    }
                }
            }
            .navigationTitle("Stream Keyboard")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .task {
            try? await Task.sleep(nanoseconds: 120_000_000)
            textFieldFocused = true
        }
        .confirmationDialog(
            "Clear the active text field?",
            isPresented: $showClearConfirmation,
            titleVisibility: .visible
        ) {
            Button("Clear Text", role: .destructive) {
                clearRemoteText(disableFutureConfirmation: false)
            }
            Button("Clear and Don’t Ask Again", role: .destructive) {
                clearRemoteText(disableFutureConfirmation: true)
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("OpenNOW will select all text in the focused game field and delete it.")
        }
    }

    private func sendText() {
        guard !text.isEmpty else { return }
        let sentCharacterCount = coordinator.sendTextToStream(text)
        if sentCharacterCount > 0 {
            text = String(text.dropFirst(sentCharacterCount))
            sendError = nil
        } else {
            sendError = "Keyboard input is reconnecting. Your text has not been cleared; try again in a moment."
        }
        textFieldFocused = true
    }

    private func requestClear() {
        if coordinator.liveSettings.streamKeyboardClearConfirmationDisabled {
            clearRemoteText(disableFutureConfirmation: false)
        } else {
            showClearConfirmation = true
        }
    }

    private func clearRemoteText(disableFutureConfirmation: Bool) {
        if coordinator.clearStreamText(disableFutureConfirmation: disableFutureConfirmation) {
            text = ""
            sendError = nil
        } else {
            sendError = "Keyboard input is reconnecting. Nothing was cleared; try again in a moment."
        }
        textFieldFocused = true
    }
}

/// Row chrome for the in-stream panel, matching `ControlRowStyle.stream()` on Android.
///
/// Two things it fixes beyond looking the same. The fills are **opaque tones**, not translucent
/// white — translucency composited differently against every frame of the game behind it, so a
/// row's contrast changed as the scene changed. And the whole panel is fixed dark: it used
/// `secondarySystemGroupedBackground`, which is near-white in light appearance and put white rows
/// over live video for anyone not in dark mode.
private enum StreamPanelStyle {
    static let rowRadius: CGFloat = 12
    static let rowFill = OpenNOWPalette.rowOverVideoRest
    static let rowStroke = OpenNOWPalette.hairlineOverVideo
    static let horizontalPadding: CGFloat = 12
    static let verticalPadding: CGFloat = 10
    static let label = Font.subheadline.weight(.semibold)
    static let supporting = Font.caption2
    static let supportingColor = OpenNOWPalette.textMutedOnDark
    static let sectionHeader = Font.caption2.weight(.bold)
}

private extension View {
    /// The shared container. Every row goes through this so a padding change happens once.
    func streamPanelRow() -> some View {
        self
            .padding(.horizontal, StreamPanelStyle.horizontalPadding)
            .padding(.vertical, StreamPanelStyle.verticalPadding)
            .frame(minHeight: 48)
            .background(
                StreamPanelStyle.rowFill,
                in: RoundedRectangle(cornerRadius: StreamPanelStyle.rowRadius, style: .continuous)
            )
            .overlay(
                RoundedRectangle(cornerRadius: StreamPanelStyle.rowRadius, style: .continuous)
                    .strokeBorder(StreamPanelStyle.rowStroke, lineWidth: 1)
            )
    }
}

private struct NativeStreamPanelSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title.uppercased())
                .font(StreamPanelStyle.sectionHeader)
                .kerning(0.7)
                .foregroundStyle(StreamPanelStyle.supportingColor)
                .padding(.leading, 2)
            content()
        }
    }
}

/// The outlined pill the panel header uses. Android's `StreamPanelHeaderButton` exists because an
/// OutlinedButton alone shows no focus state there; the same shape is used here so the two panels
/// read as the same control set.
private struct NativeStreamPanelPillButton<Content: View>: View {
    var prominent: Bool = false
    let action: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        Button(action: action) {
            content()
                .font(.footnote.weight(.semibold))
                .padding(.horizontal, 11)
                .padding(.vertical, 7)
                .frame(minHeight: 34)
        }
        .buttonStyle(.plain)
        .foregroundStyle(prominent ? OpenNOWPalette.textOnDark : OpenNOWPalette.textMutedOnDark)
        .background(
            prominent ? OpenNOWPalette.rowOverVideoFocused : OpenNOWPalette.rowOverVideoRest,
            in: Capsule()
        )
        .overlay(
            Capsule().strokeBorder(
                prominent ? OpenNOWPalette.textOnDark.opacity(0.45) : OpenNOWPalette.hairlineOverVideo,
                lineWidth: prominent ? 1.5 : 1
            )
        )
        .contentShape(Capsule())
    }
}

private struct NativeStreamPanelIconButton: View {
    let systemImage: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 14, weight: .semibold))
                .frame(width: 34, height: 34)
        }
        .buttonStyle(.plain)
        .foregroundStyle(OpenNOWPalette.textOnDark)
        .frame(minWidth: 44, minHeight: 44)
        .background(OpenNOWPalette.rowOverVideoRest, in: Circle())
        .overlay(Circle().strokeBorder(OpenNOWPalette.hairlineOverVideo, lineWidth: 1))
        .accessibilityLabel(label)
    }
}

private struct NativeStreamToggleRow: View {
    let title: String
    let value: String
    @Binding var isOn: Bool

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(StreamPanelStyle.label)
                Text(value)
                    .font(StreamPanelStyle.supporting)
                    .foregroundStyle(StreamPanelStyle.supportingColor)
            }
            Spacer(minLength: 10)
            Toggle("", isOn: $isOn)
                .labelsHidden()
                .fixedSize()
        }
        .streamPanelRow()
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue(value)
    }
}

private struct NativeStreamSliderRow: View {
    let title: String
    @Binding var value: Double
    let range: ClosedRange<Double>
    let step: Double

    init(
        title: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double = 0.05
    ) {
        self.title = title
        self._value = value
        self.range = range
        self.step = step
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                    .font(StreamPanelStyle.label)
                Spacer(minLength: 10)
                Text(percentText)
                    .font(StreamPanelStyle.supporting.monospacedDigit())
                    .foregroundStyle(StreamPanelStyle.supportingColor)
            }
            Slider(value: $value, in: range, step: step)
        }
        .streamPanelRow()
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue(percentText)
    }

    private var percentText: String {
        "\(Int((value * 100).rounded()))%"
    }
}

private struct NativeStreamActionRow: View {
    let title: String
    let value: String
    let actionLabel: String
    let action: () -> Void

    init(
        title: String,
        value: String,
        actionLabel: String = "Change",
        action: @escaping () -> Void
    ) {
        self.title = title
        self.value = value
        self.actionLabel = actionLabel
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(StreamPanelStyle.label)
                    Text(value)
                        .font(StreamPanelStyle.supporting)
                        .foregroundStyle(StreamPanelStyle.supportingColor)
                        .multilineTextAlignment(.leading)
                }
                Spacer(minLength: 10)
                Text(actionLabel)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.tint)
            }
            .streamPanelRow()
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue(value)
        .accessibilityHint(actionLabel)
    }
}

private struct NativeStreamInfoRow: View {
    let title: String
    let value: String

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .font(StreamPanelStyle.label)
            Spacer(minLength: 12)
            Text(value)
                .font(StreamPanelStyle.supporting)
                .foregroundStyle(StreamPanelStyle.supportingColor)
                .multilineTextAlignment(.trailing)
        }
        .streamPanelRow()
    }
}

private struct NativeStreamSessionTimeRow: View {
    let value: String
    let progress: Double
    let isWarning: Bool

    var body: some View {
        VStack(spacing: 7) {
            HStack(alignment: .firstTextBaseline) {
                Text("Session time")
                    .font(.subheadline.weight(.semibold))
                Spacer(minLength: 12)
                Text(value)
                    .font(.caption.weight(isWarning ? .semibold : .regular))
                    .foregroundStyle(isWarning ? Color.orange : Color.primary.opacity(0.72))
                    .multilineTextAlignment(.trailing)
            }
            ProgressView(value: min(max(progress, 0), 1))
                .tint(isWarning ? .orange : .accentColor)
                .accessibilityLabel("Session progress")
                .accessibilityValue("\(Int((min(max(progress, 0), 1) * 100).rounded())) percent")
        }
        .streamPanelRow()
    }
}

private struct NativeStreamKeyButton: View {
    let title: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline.weight(.semibold))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 10)
        }
        .buttonStyle(.bordered)
    }
}

private enum NativeStreamVirtualKey {
    case escape
    case enter
    case backspace

    var mapping: NativeStreamKeyboardMapping {
        switch self {
        case .escape:
            return .init(virtualKey: 0x1b, scanCode: 0x0001)
        case .enter:
            return .init(virtualKey: 0x0d, scanCode: 0x001c)
        case .backspace:
            return .init(virtualKey: 0x08, scanCode: 0x000e)
        }
    }
}

@MainActor
final class NativeStreamCoordinator: NSObject, ObservableObject {
    @Published var statusText = "Preparing stream"
    @Published var detailText = ""
    @Published var statsText = ""
    @Published fileprivate var statsSnapshot = NativeStreamStatsSnapshot.empty
    @Published var showStatusOverlay = true
    @Published var retryAvailable = false
    @Published var showStatsOverlay = false
    @Published var controlsPanelVisible = false
    @Published fileprivate var touchLayoutEditing = false
    @Published fileprivate var statsDisplayStyle: StreamStatsStyle = .compact
    @Published fileprivate var statsMetrics: StreamStatsMetrics = .default
    /// A live copy of app settings the panel can edit mid-session. Persisted through
    /// `onSettingsChange` so a change made in-game survives the session ending.
    @Published fileprivate var liveSettings: AppSettings
    @Published var streamerPreferences: StreamerPreferences
    @Published fileprivate var streamSharpeningEnabled: Bool
    @Published fileprivate var streamSharpeningAmount: Double
    @Published fileprivate var fingerMouseEnabled: Bool
    @Published fileprivate var phoneRumbleFallbackEnabled: Bool
    @Published fileprivate var deviceStatus = NativeStreamDeviceStatus.current()
    @Published var touchLayout: TouchControlLayout
    @Published fileprivate var pictureInPictureAvailable = false
    @Published fileprivate var isPictureInPictureActive = false
    @Published fileprivate var physicalControllerConnected = false
    @Published fileprivate var inputModePrompt: NativeStreamInputModePrompt?
    @Published fileprivate var showTouchControlsWithPhysicalController = false
    @Published fileprivate var presentedGuidanceSheet: NativeStreamGuidanceSheet?
    @Published fileprivate var tutorialDoneCalloutVisible = false
    @Published fileprivate var sessionDurationText: String?
    @Published fileprivate var sessionTimerProgress = 0.0
    @Published fileprivate var sessionTimerWarningActive = false
    @Published fileprivate var sessionWarningText: String?
    @Published fileprivate var modeChangeNotice: StreamModeChangeNotice?
    @Published fileprivate var antiAfkActive = false

    let sessionID: String
    let inputBridge = NativeStreamInputBridge()

    private let session: ActiveSession
    private let settings: AppSettings
    private let sessionLimit: StreamSessionLimit
    private let touchLayoutProfile: String
    private let onTouchLayoutChange: (String, TouchControlLayout) -> Void
    private let onStreamerPreferencesChange: (StreamerPreferences) -> Void
    private let onStreamSharpeningChange: (Bool, Double) -> Void
    private let onFingerMouseEnabledChange: (Bool) -> Void
    private let onPhoneRumbleFallbackChange: (Bool) -> Void
    private let onStreamTutorialCompleted: () -> Void
    private let onControllerTouchPromptDismissed: () -> Void
    private let onStatsOverlayChange: (Bool) -> Void
    private let onTransportStable: () -> Void
    private let onSelectedVideoProfileRetry: (String) -> Void
    /// Fired roughly once a second with the numbers behind the HUD, so the store can accumulate
    /// them into a session report. Deliberately a plain closure rather than a Combine publisher —
    /// nothing in the view hierarchy should observe it.
    private let onRuntimeSample: (StreamRuntimeSample) -> Void
    private let onSettingsChange: (AppSettings) -> Void
    private let onBuildBugReportDeck: () -> BugReportPreflightDeck
    private let onSubmitBugReport: (BugReportDraft, BugReportPreflightDeck) async -> Result<String, Error>
    private let onClose: () -> Void
    private let onRetry: (() -> Void)?
    private let logger = Logger(subsystem: "OpenNOWiOS", category: "NativeStreamer")
    private let workQueue = DispatchQueue(label: "OpenNOW.NativeStreamer")
    private var nativeNVST: (any NativeStreamNVSTTransport)?
    private var nativeNVSTStart: Task<Void, Never>?
    private var nativeNVSTFailed = false

    private let networkMonitor = NWPathMonitor()
    private let networkMonitorQueue = DispatchQueue(label: "OpenNOW.NativeStreamer.Network")
    private let peerName = "peer-\(UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(12))"
    private var peerId = 0
    private var remotePeerId = 1
    private var ackCounter = 0
    private var started = false
    private var stopped = false
    private var selectedCodec: NativeStreamVideoCodec = .h264 {
        didSet { renderer?.setVideoCodec(selectedCodec) }
    }
    private var streamProfile: StreamVideoProfile
    /// What the user actually asked for, before CloudMatch had a say. `streamProfile` already
    /// carries the negotiated geometry, so the two have to be kept apart to spot a difference.
    private let requestedProfile: StreamVideoProfile
    private var modeNoticeDismissTask: Task<Void, Never>?
    private var reportedModeChangeKey: String?
    /// Set when the app itself dropped the profile, so the notice can say so instead of blaming
    /// the server for something OpenNOW did.
    private var safeVideoRecoveryReason: String?
    private var antiAfkTimer: Timer?
    private var lastUserInputAt: TimeInterval = ProcessInfo.processInfo.systemUptime
    private var lastObservedInputPacketCount = 0
    private var antiAfkGeneratedPacketCount = 0
    private var viewportSize: CGSize = .zero
    private var signalingSession: URLSession?
    private var webSocket: URLSessionWebSocketTask?
    private var signalingHeartbeat: DispatchSourceTimer?
    private var offerTimeoutWorkItem: DispatchWorkItem?
    private var iceDisconnectWorkItem: DispatchWorkItem?
    private var statsTimer: DispatchSourceTimer?
    private var factory: RTCPeerConnectionFactory?
    private var peerConnection: RTCPeerConnection?
    private var reliableInputChannel: RTCDataChannel?
    private var partiallyReliableInputChannel: RTCDataChannel?
    private var controlChannel: RTCDataChannel?
    private let pictureInPictureBridge = NativeStreamPictureInPictureBridge()
    private weak var renderer: NativeStreamRenderView?
    private let videoSink = NativeStreamVideoSink()
    private var videoTrack: RTCVideoTrack?
    private var videoSinkAttached = false
    private var receivedColorMode = "--"
    private var renderedFrameCount = 0
    private var renderedVideoSize: CGSize = .zero
    private var streamZoomScale: CGFloat = 1
    private var streamZoomOffset: CGSize = .zero
    private var decodedWithoutRenderStartedAt: TimeInterval?
    private var lastRenderKeyframeRequestAt: TimeInterval?
    private var renderKeyframeAttempts = 0
    private var codecReport = NativeStreamCodecProbe.report()
    private var reliableInputPackets = 0
    private var partiallyReliableInputPackets = 0
    private var negotiationDebugText = ""
    private var localCodecDebugText = ""
    private var answerSent = false
    private var queuedLocalIceCandidates: [[String: Any]] = []
    private var lastStatsSampleAt: TimeInterval?
    private var lastStatsFramesReceived: Int?
    private var lastStatsFramesDecoded: Int?
    private var lastStatsFramesRendered: Int?
    private var lastStatsBytesReceived: Int?
    private var lastStatsTotalDecodeTime: Double?
    private var lastStatsPacketsLost: Int?
    private var lastStatsPacketsReceived: Int?
    private var mediaTransportConnected = false
    private var mediaLivenessWatchdog = NativeStreamLivenessWatchdog()
    private var recoveryProgressTracker = NativeStreamRecoveryProgressTracker()
    private var packetLossRecoveryGate = NativeStreamPacketLossRecoveryGate()
    private var decoderRecoveryGate = NativeStreamDecoderRecoveryGate()
    private var selectedProfileRetryApplied = false
    private var transportWasStable = false
    private var lastRenderedStatsProgressAt: TimeInterval?
    private var lastPostStartKeyframeRequestAt: TimeInterval?
    private var postStartKeyframeAttempts = 0
    private var autoRetryScheduled = false
    private var streamAudioPlaybackConfigured = false
    private var mutedAudioDevice: NativeStreamMutedAudioDevice?
    private var playbackAudioDevice: NativeStreamPlaybackAudioDevice?
    private var audioTrack: RTCAudioTrack?
    private var audioSessionObservers: [NSObjectProtocol] = []
    private var audioSessionRecoveryTask: Task<Void, Never>?
    private var latestScenePhase: ScenePhase = .active
    private var backgroundPictureInPictureStartPending = false
    private var needsForegroundReconnect = false
    @Published fileprivate var videoActive = false
    private var streamTutorialCompleted: Bool
    private var controllerTouchPromptDismissed: Bool
    private var controllerTouchPromptHandledThisSession = false
    private var guidancePresentationTask: Task<Void, Never>?
    private var sessionWarningTracker = StreamSessionWarningTracker()
    private var sessionWarningDismissTask: Task<Void, Never>?
    private var networkMonitorStarted = false
    private var streamInputMode: NativeStreamInputMode
    private var physicalKeyboardMouseConnected: Bool
    private var pendingInputModePrompt: NativeStreamInputModePrompt?

    init(
        session: ActiveSession,
        settings: AppSettings,
        membershipTier: String?,
        onTouchLayoutChange: @escaping (String, TouchControlLayout) -> Void,
        onStreamerPreferencesChange: @escaping (StreamerPreferences) -> Void,
        onStreamSharpeningChange: @escaping (Bool, Double) -> Void,
        onFingerMouseEnabledChange: @escaping (Bool) -> Void,
        onPhoneRumbleFallbackChange: @escaping (Bool) -> Void,
        onStreamTutorialCompleted: @escaping () -> Void,
        onControllerTouchPromptDismissed: @escaping () -> Void,
        onStatsOverlayChange: @escaping (Bool) -> Void,
        onTransportStable: @escaping () -> Void,
        onSelectedVideoProfileRetry: @escaping (String) -> Void,
        onRuntimeSample: @escaping (StreamRuntimeSample) -> Void,
        onSettingsChange: @escaping (AppSettings) -> Void,
        onBuildBugReportDeck: @escaping () -> BugReportPreflightDeck,
        onSubmitBugReport: @escaping (BugReportDraft, BugReportPreflightDeck) async -> Result<String, Error>,
        onClose: @escaping () -> Void,
        onRetry: (() -> Void)?
    ) {
        let resolvedSessionLimit = streamSessionLimit(for: membershipTier)
        let resolvedTouchLayoutProfile = streamTouchLayoutProfile(
            gameTitle: session.game.title,
            settings: settings
        )
        let initialTimerSnapshot = streamSessionTimerSnapshot(
            limit: resolvedSessionLimit,
            startedAt: session.startedAt
        )
        self.session = session
        self.settings = settings
        self.sessionLimit = resolvedSessionLimit
        self.touchLayoutProfile = resolvedTouchLayoutProfile
        self.sessionID = session.id
        self.streamerPreferences = settings.streamerPreferences
        self.streamSharpeningEnabled = settings.streamSharpeningEnabled
        self.streamSharpeningAmount = min(max(settings.streamSharpeningAmount, 0), 1)
        self.fingerMouseEnabled = settings.fingerMouseEnabled
        self.phoneRumbleFallbackEnabled = settings.phoneRumbleFallback
        let keyboardMouseConnected = NativeStreamPhysicalInput.keyboardOrMouseConnected
        let nativeTouchAvailable = NativeTouchSupport.shouldStartWithNativeTouch(
                mode: settings.touch.nativeTouchMode,
                game: session.game,
                keyboardMouseConnected: keyboardMouseConnected,
                provisioned: session.touchProvisioned,
                preferVirtualController: settings.streamerPreferences.touchControllerVisible
            )
        self.physicalKeyboardMouseConnected = keyboardMouseConnected
        self.streamInputMode = nativeTouchAvailable
            ? .nativeTouch
            : .keyboardMouse
        self.pendingInputModePrompt = nil
        self.touchLayout = settings.touchLayout(for: resolvedTouchLayoutProfile)
        self.onTouchLayoutChange = onTouchLayoutChange
        self.onStreamerPreferencesChange = onStreamerPreferencesChange
        self.onStreamSharpeningChange = onStreamSharpeningChange
        self.onFingerMouseEnabledChange = onFingerMouseEnabledChange
        self.onPhoneRumbleFallbackChange = onPhoneRumbleFallbackChange
        self.onStreamTutorialCompleted = onStreamTutorialCompleted
        self.onControllerTouchPromptDismissed = onControllerTouchPromptDismissed
        self.onStatsOverlayChange = onStatsOverlayChange
        self.onTransportStable = onTransportStable
        self.onSelectedVideoProfileRetry = onSelectedVideoProfileRetry
        self.onRuntimeSample = onRuntimeSample
        self.onSettingsChange = onSettingsChange
        self.onBuildBugReportDeck = onBuildBugReportDeck
        self.onSubmitBugReport = onSubmitBugReport
        self.liveSettings = settings
        self.onClose = onClose
        self.onRetry = onRetry
        self.streamProfile = Self.effectiveProfile(for: session, settings: settings)
        self.requestedProfile = StreamSettingsResolver.profile(for: settings, membershipTier: membershipTier)
        self.showStatsOverlay = settings.showStatsOverlay
        self.statsDisplayStyle = settings.streamerPreferences.statsStyle
        self.statsMetrics = settings.streamStatsMetrics
        self.streamTutorialCompleted = settings.streamTutorialCompleted
        self.controllerTouchPromptDismissed = settings.controllerTouchPromptDismissed
        self.sessionDurationText = settings.sessionCounterEnabled
            ? Self.formatSessionDuration(snapshot: initialTimerSnapshot, limit: resolvedSessionLimit)
            : nil
        self.sessionTimerProgress = settings.sessionCounterEnabled ? initialTimerSnapshot.progress : 0
        self.sessionTimerWarningActive = settings.sessionCounterEnabled && initialTimerSnapshot.isWarning
        super.init()
        inputBridge.sink = self
        inputBridge.configureUserPreferences(
            mouseSensitivity: settings.mouseSensitivity,
            mouseAcceleration: settings.mouseAcceleration,
            phoneRumbleFallback: settings.phoneRumbleFallback,
            physicalControllerPassthrough: settings.streamerPreferences.physicalControllerPassthrough,
            controllerMouseEmulation: settings.controllerMouseEmulation,
            mouseScrollSensitivity: settings.mouseScrollSensitivity
        )
        inputBridge.onPhysicalControllerAvailabilityChanged = { [weak self] connected in
            Task { @MainActor in
                guard let self else { return }
                self.handlePhysicalControllerAvailabilityChanged(connected)
            }
        }
        inputBridge.onPhysicalKeyboardMouseAvailabilityChanged = { [weak self] connected in
            Task { @MainActor in
                self?.handlePhysicalKeyboardMouseAvailabilityChanged(connected)
            }
        }
        videoSink.onFrame = { [weak self] count, size, colorMode in
            Task { @MainActor in
                self?.noteRenderedFrame(count: count, size: size, colorMode: colorMode)
            }
        }
        videoSink.onPictureInPictureFrame = { [weak pictureInPictureBridge] frame in
            pictureInPictureBridge?.enqueue(frame: frame)
        }
        pictureInPictureBridge.onAvailabilityChanged = { [weak self] available in
            Task { @MainActor in
                self?.pictureInPictureAvailable = available
            }
        }
        pictureInPictureBridge.onActiveChanged = { [weak self] active in
            Task { @MainActor in
                self?.handlePictureInPictureActiveChanged(active)
            }
        }
        pictureInPictureBridge.onLog = { [weak self] message in
            Task { @MainActor in
                self?.log(message)
            }
        }
    }

    func start(viewportSize: CGSize) {
        guard !started else { return }
        started = true
        stopped = false
        self.viewportSize = viewportSize
        refreshDeviceStatus()
        retryAvailable = onRetry != nil
        updateStatus("Checking codecs", detail: codecReport.summary)

        let requires444 = StreamSettingsResolver.colorQuality(for: settings) == .tenBit444
        if requires444 && !settings.experimentalNativeNVSTEnabled {
            fail("10-bit 4:4:4 requires the Native NVST Receiver. Enable it in Settings → Stream → Connection.")
            return
        }
        if settings.experimentalNativeNVSTEnabled {
            guard #available(iOS 17.0, *) else { fail("Native NVST requires iOS 17 or newer"); return }
            let codec = NativeStreamVideoCodec.normalized(settings.preferredCodec)
                ?? (requires444 ? .h265 : [.h265, .av1, .h264].first { codecReport.capability(for: $0)?.videoToolboxHardwareDecode == true } ?? .h264)
            guard !requires444 || codec == .h265 else {
                fail("Use H.265 for the experimental 10-bit 4:4:4 mode. AV1 remains available with 4:2:0.")
                return
            }
            guard codecReport.capability(for: codec)?.videoToolboxHardwareDecode == true else {
                fail("\(codec.rawValue) hardware decoding is unavailable on this device"); return
            }
            selectedCodec = codec
            startNetworkMonitoring()
            inputBridge.attach()
            setIdleTimerDisabled(true)
            startNativeNVST()
            return
        }

        let requested = NativeStreamVideoCodec.normalized(settings.preferredCodec)
        let resolved = codecReport.launchSafeCodec(preferred: settings.preferredCodec)
        if let requested, requested != resolved {
            if allowsUnsafeCodecDiagnostics,
               codecReport.capability(for: requested)?.webRTCSupported == true {
                selectedCodec = requested
                updateStatus("Checking codecs", detail: "Diagnostic unsafe codec override: \(requested.rawValue)")
            } else {
                fail("\(requested.rawValue) is not available through iOS hardware WebRTC on this runtime")
                return
            }
        } else {
            selectedCodec = resolved
        }

        startNetworkMonitoring()
        startAudioSessionObservation()
        configureStreamAudioPlayback()
        inputBridge.attach()
        setIdleTimerDisabled(true)
        connectSignaling()
    }

    @available(iOS 17.0, *)
    private func startNativeNVST() {
        updateStatus("Connecting native NVST", detail: "Negotiating the dedicated video connection")
        let sink = videoSink
        let transport = NativeStreamNVST(allocation: session, settings: settings, profile: streamProfile,
            codec: selectedCodec, displayFPS: renderer?.window?.screen.maximumFramesPerSecond ?? UIScreen.main.maximumFramesPerSecond,
            onFrame: { frame in sink.renderFrame(frame) },
            onSample: { [weak self] sample in Task { @MainActor in
                guard let self, !self.stopped else { return }
                self.updateNativeNVSTStats(sample)
            } },
            onFailure: { [weak self] reason in Task { @MainActor in
                guard let self, !self.stopped else { return }
                self.nativeNVSTFailed = true
                self.markMediaTransportDisconnected()
                self.updateStatus("Native NVST failed", detail: reason)
                self.retryAvailable = self.onRetry != nil
            } },
            onHaptics: { [weak self] events in Task { @MainActor in
                guard let self, !self.stopped else { return }
                for event in events {
                    self.inputBridge.applyRumble(controllerId: Int(event.gamepadIndex),
                        weakMagnitude: Int(event.rightMotor), strongMagnitude: Int(event.leftMotor))
                }
            } })
        nativeNVST = transport
        nativeNVSTStart = Task { [weak self] in
            do {
                try await transport.start()
                guard let self, !self.stopped, !Task.isCancelled else { return }
                self.markMediaTransportConnected()
            }
            catch {
                guard let self, !self.stopped, !Task.isCancelled else { return }
                self.nativeNVSTFailed = true
                self.updateStatus("Native NVST failed", detail: error.localizedDescription)
                self.retryAvailable = self.onRetry != nil
            }
        }
    }

    private func updateNativeNVSTStats(_ sample: NativeStreamNVSTSample) {
        guard !nativeNVSTFailed else { return }
        updateSessionTimer()
        refreshDeviceStatus()
        let progressed = Int(clamping: sample.decoded) > (lastStatsFramesDecoded ?? 0)
        if !transportWasStable, recoveryProgressTracker.observe(progressed: progressed) {
            transportWasStable = true
            onTransportStable()
        }
        let now = ProcessInfo.processInfo.systemUptime
        let decoded = Int(clamping: sample.decoded), received = Int(clamping: sample.received)
        let bytes = Int(clamping: sample.bytes)
        let decodedFPS = estimatedRate(current: decoded, previous: lastStatsFramesDecoded, now: now)
        let receivedFPS = estimatedRate(current: received, previous: lastStatsFramesReceived, now: now)
        let bitrate = estimatedBitrateKbps(bytesReceived: bytes, now: now)
        let lost = Int(clamping: sample.lost), packets = Int(clamping: sample.packets)
        let lostDelta = monotonicDelta(current: lost, previous: lastStatsPacketsLost)
        let packetsDelta = monotonicDelta(current: packets, previous: lastStatsPacketsReceived)
        let loss = Double(lost) * 100 / Double(max(1, lost + packets))
        let resolution = sample.resolution ?? streamProfile.resolutionString
        statsText = "Native NVST  \(sample.bitstream) \(sample.pixelFormat)  \(sample.detail)"
        statsSnapshot = NativeStreamStatsSnapshot(codec: selectedCodec.rawValue.uppercased(), colorMode: receivedColorMode,
            resolution: resolution, fps: decodedFPS, bitrateKbps: bitrate,
            pingMs: sample.pingMilliseconds.map { Int($0.rounded()) }, decoded: decoded,
            rendered: renderedFrameCount, dropped: max(0, received - decoded), lossPercent: loss,
            jitterMs: Int(sample.jitterMilliseconds.rounded()), decodeMs: sample.decodeMilliseconds >= 0 ? sample.decodeMilliseconds : nil,
            serverLabel: session.zone.isEmpty ? nil : session.zone, gpuLabel: session.gpuType,
            targetFps: streamProfile.fps, inputSummary: "r\(reliableInputPackets)/p\(partiallyReliableInputPackets)", detail: statsText)
        statsSnapshot.requestedColor = "\(StreamSettingsResolver.colorQuality(for: settings).label) \(settings.hdrEnabled ? "HDR" : "SDR")"
        statsSnapshot.effects = renderer?.videoEffectsStatus ?? ""
        statsSnapshot.presentationRates = renderer?.presentationRates
        if decoded > 0 { handleDecodedVideoProgress(framesDecoded: decoded); raiseModeChangeNoticeIfNeeded(deliveredResolution: resolution) }
        onRuntimeSample(StreamRuntimeSample(timestamp: now, pingMs: sample.pingMilliseconds.map { Int($0.rounded()) },
            bitrateKbps: bitrate, jitterMs: sample.jitterMilliseconds, fps: decodedFPS, receivedFps: receivedFPS,
            decodedFps: decodedFPS, decodeMs: sample.decodeMilliseconds >= 0 ? sample.decodeMilliseconds : nil,
            packetsLostDelta: lostDelta, packetsReceivedDelta: packetsDelta, packetLossPercent: loss,
            resolution: resolution, codec: selectedCodec.rawValue.uppercased(), networkKind: deviceStatus.networkTransport.sessionNetworkKind))
        lastStatsSampleAt = now; lastStatsFramesReceived = received; lastStatsFramesDecoded = decoded
        lastStatsBytesReceived = bytes; lastStatsPacketsLost = lost; lastStatsPacketsReceived = packets
    }

    func updateViewportSize(_ size: CGSize) {
        if viewportSize != .zero,
           abs(viewportSize.width - size.width) > 0.5 || abs(viewportSize.height - size.height) > 0.5 {
            resetStreamZoom()
        }
        viewportSize = size
    }

    func handleScenePhase(_ phase: ScenePhase) {
        latestScenePhase = phase
        switch phase {
        case .active:
            backgroundPictureInPictureStartPending = false
            reconnectAfterBackgroundIfNeeded()
            scheduleGuidancePresentation()
        case .inactive:
            startPictureInPictureForBackground(reason: "scene inactive")
        case .background:
            startPictureInPictureForBackground(reason: "scene background")
        @unknown default:
            break
        }
    }

    func handleViewDisappear(scenePhase: ScenePhase) {
        latestScenePhase = scenePhase
        guard scenePhase == .active else {
            log("Preserving native stream during \(String(describing: scenePhase)) scene transition")
            startPictureInPictureForBackground(reason: "view disappeared while app was not active")
            return
        }
        stop()
    }

    var gameTitle: String {
        session.game.title
    }

    var selectedCodecLabel: String {
        selectedCodec.rawValue.uppercased()
    }

    var profileLabel: String {
        "\(streamProfile.resolutionString) @ \(streamProfile.fps) fps"
    }

    var shouldShowVirtualController: Bool {
        streamerPreferences.touchControllerVisible &&
            (!physicalControllerConnected || showTouchControlsWithPhysicalController || touchLayoutEditing)
    }

    var virtualControllerInputEnabled: Bool {
        streamerPreferences.touchControllerVisible &&
            (!physicalControllerConnected || showTouchControlsWithPhysicalController) &&
            !touchLayoutEditing
    }

    /// Whether this session sends real fingers to the host rather than a synthetic cursor.
    ///
    /// The catalog's `TOUCHSCREEN` capability decides it under `.automatic`; asking for the
    /// on-screen controller overrides it, because the two need exclusive ownership of the same
    /// fingers. Mirrors `nativeTouchActive` in `OpenNowStreamSurface.kt`.
    fileprivate var nativeTouchActive: Bool {
        // A session created without `appLaunchMode: touchFriendly` has no digitizer and never will,
        // so sending contacts into it would be silence with no explanation. Sessions from before
        // this was recorded report `nil` and keep trusting the setting.
        guard session.touchProvisioned != false, streamInputMode == .nativeTouch else { return false }
        return NativeTouchSupport.shouldUseNativeTouchForStream(
            mode: liveSettings.touch.nativeTouchMode,
            game: session.game,
            preferVirtualController: streamerPreferences.touchControllerVisible
        )
    }

    fileprivate var nativeTouchCaptureEnabled: Bool {
        nativeTouchActive &&
            !controlsPanelVisible &&
            presentedGuidanceSheet == nil &&
            !isPictureInPictureActive
    }

    fileprivate var fingerMouseCaptureEnabled: Bool {
        fingerMouseEnabled &&
            !nativeTouchActive &&
            !shouldShowVirtualController &&
            !controlsPanelVisible &&
            presentedGuidanceSheet == nil
    }

    /// What the touch surface needs to map a finger onto the picture. Read at touch time rather
    /// than published, because the zoom transform changes once per frame during a pinch and
    /// nothing in the view hierarchy should re-render for it.
    fileprivate func currentTouchGeometry() -> NativeStreamTouchGeometry {
        let decoded = renderedVideoSize
        let streamSize = decoded.width > 0 && decoded.height > 0
            ? decoded
            : CGSize(width: streamProfile.width, height: streamProfile.height)
        return NativeStreamTouchGeometry(
            streamSize: streamSize,
            stretchToFill: streamerPreferences.stretchStreamToFill,
            zoomScale: streamZoomScale,
            zoomOffset: streamZoomOffset,
            scrollScale: CGFloat(liveSettings.touch.nativeTouchScrollScale),
            jitterThreshold: CGFloat(liveSettings.touch.nativeTouchJitterThreshold)
        )
    }

    func toggleControlsPanel() {
        if controlsPanelVisible {
            finishControlsPanel()
        } else {
            setControlsPanelVisible(true)
        }
    }

    func setControlsPanelVisible(_ visible: Bool) {
        withAnimation(.easeInOut(duration: 0.18)) {
            controlsPanelVisible = visible
        }
        if !visible {
            scheduleGuidancePresentation()
        }
    }

    func dismissControlsPanelFromBackdrop() {
        guard !tutorialDoneCalloutVisible else { return }
        setControlsPanelVisible(false)
    }

    func finishControlsPanel() {
        let completesTutorial = tutorialDoneCalloutVisible
        withAnimation(.easeInOut(duration: 0.18)) {
            controlsPanelVisible = false
        }
        if completesTutorial {
            completeStreamTutorial()
        } else {
            scheduleGuidancePresentation()
        }
    }

    func beginStreamTutorialControlsStep() {
        presentedGuidanceSheet = nil
        tutorialDoneCalloutVisible = true
        setControlsPanelVisible(true)
    }

    func skipStreamTutorial() {
        withAnimation(.easeInOut(duration: 0.18)) {
            controlsPanelVisible = false
        }
        completeStreamTutorial()
    }

    func resolveControllerTouchPrompt(showTouchControls: Bool, doNotShowAgain: Bool) {
        controllerTouchPromptHandledThisSession = true
        showTouchControlsWithPhysicalController = showTouchControls
        presentedGuidanceSheet = nil
        updateVirtualControllerAvailability()
        if doNotShowAgain && !controllerTouchPromptDismissed {
            controllerTouchPromptDismissed = true
            onControllerTouchPromptDismissed()
        }
    }

    func setStatsOverlayVisible(_ visible: Bool) {
        showStatsOverlay = visible
        onStatsOverlayChange(visible)
    }

    func cycleStatsStyle() {
        let next: StreamStatsStyle = statsDisplayStyle == .compact ? .detailed : .compact
        statsDisplayStyle = next
        var preferences = streamerPreferences
        preferences.statsStyle = next
        setStreamerPreferences(preferences)
    }

    func toggleStatsPosition() {
        var preferences = streamerPreferences
        let order = StreamStatsPosition.allCases
        let index = order.firstIndex(of: preferences.statsPosition) ?? 0
        preferences.statsPosition = order[(index + 1) % order.count]
        setStreamerPreferences(preferences)
    }

    func bugReportPreflightDeck() -> BugReportPreflightDeck { onBuildBugReportDeck() }

    func submitBugReport(
        _ draft: BugReportDraft,
        deck: BugReportPreflightDeck
    ) async -> Result<String, Error> {
        await onSubmitBugReport(draft, deck)
    }

    /// What the touch-routing settings actually add up to right now. The three inputs that
    /// produce it are not individually readable as an outcome, so the panel shows this instead.
    var resolvedTouchModeLabel: String {
        if nativeTouchActive {
            if StreamSettingsResolver.requiresDesktopColorProvisioning(for: liveSettings),
               liveSettings.experimentalDesktop444TouchEnabled {
                return "Native Touch · experimental 4:4:4"
            }
            return ResolvedTouchMode.nativeTouch.label
        }
        // Turning touch on inside a session the host provisioned without a digitizer cannot take
        // effect, and the picker above gives no hint of that. Say so rather than quietly reporting
        // whatever the fallback happens to be.
        if session.touchProvisioned == false,
           NativeTouchSupport.shouldUseNativeTouchForStream(
            mode: liveSettings.touch.nativeTouchMode,
            game: session.game,
            preferVirtualController: streamerPreferences.touchControllerVisible
           ) {
            return ResolvedTouchMode.nativeTouchUnavailable.label
        }
        // The resolved state, not the stored preference: with the controller wanted but suppressed
        // by a connected gamepad, the fingers are still driving the cursor.
        if shouldShowVirtualController {
            return ResolvedTouchMode.virtualGamepad.label
        }
        if fingerMouseEnabled {
            return ResolvedTouchMode.trackpadCursor(directClick: liveSettings.touch.mouseDirectClick).label
        }
        return ResolvedTouchMode.inert.label
    }

    /// Single channel for the settings the control panel edits that are not part of
    /// `StreamerPreferences`. Keeps the panel from needing one callback per control.
    func updateLiveSettings(_ transform: (inout AppSettings) -> Void) {
        var next = liveSettings
        transform(&next)
        next.normalizeStreamDefaults()
        guard next != liveSettings else { return }
        liveSettings = next
        statsMetrics = next.streamStatsMetrics
        renderer?.setVideoEffects(upscaling: next.metalFXUpscalingEnabled, metal4: next.metal4Enabled)
        inputBridge.configureUserPreferences(
            mouseSensitivity: next.mouseSensitivity,
            mouseAcceleration: next.mouseAcceleration,
            phoneRumbleFallback: phoneRumbleFallbackEnabled,
            physicalControllerPassthrough: streamerPreferences.physicalControllerPassthrough,
            controllerMouseEmulation: next.controllerMouseEmulation,
            mouseScrollSensitivity: next.mouseScrollSensitivity
        )
        onSettingsChange(next)
    }

    func setStretchStreamToFill(_ enabled: Bool) {
        resetStreamZoom()
        var preferences = streamerPreferences
        preferences.stretchStreamToFill = enabled
        setStreamerPreferences(preferences)
        renderer?.setStretchStreamToFill(enabled)
    }

    func setStreamSharpeningEnabled(_ enabled: Bool) {
        guard streamSharpeningEnabled != enabled else { return }
        streamSharpeningEnabled = enabled
        renderer?.setStreamSharpening(enabled: enabled, amount: streamSharpeningAmount)
        onStreamSharpeningChange(enabled, streamSharpeningAmount)
    }

    func setStreamSharpeningAmount(_ amount: Double) {
        let normalized = min(max(amount, 0), 1)
        guard streamSharpeningAmount != normalized else { return }
        streamSharpeningAmount = normalized
        renderer?.setStreamSharpening(enabled: streamSharpeningEnabled, amount: normalized)
        onStreamSharpeningChange(streamSharpeningEnabled, normalized)
    }

    func setFingerMouseEnabled(_ enabled: Bool) {
        guard fingerMouseEnabled != enabled else { return }
        fingerMouseEnabled = enabled
        onFingerMouseEnabledChange(enabled)
    }

    func setPhoneRumbleFallback(_ enabled: Bool) {
        guard phoneRumbleFallbackEnabled != enabled else { return }
        phoneRumbleFallbackEnabled = enabled
        inputBridge.setPhoneRumbleFallback(enabled)
        onPhoneRumbleFallbackChange(enabled)
    }

    fileprivate func applyFingerMouseZoom(_ scaleChange: CGFloat, _ pan: CGSize) {
        guard fingerMouseCaptureEnabled else { return }
        let normalizedScaleChange = min(max(scaleChange, 0.82), 1.22)
        let nextScale = min(max(streamZoomScale * normalizedScaleChange, 1), 3)
        streamZoomScale = nextScale
        if nextScale <= 1.001 {
            streamZoomOffset = .zero
        } else {
            let proposedOffset = CGSize(
                width: streamZoomOffset.width + pan.width,
                height: streamZoomOffset.height + pan.height
            )
            streamZoomOffset = clampedStreamZoomOffset(proposedOffset, scale: nextScale)
        }
        renderer?.setViewportTransform(scale: streamZoomScale, offset: streamZoomOffset)
    }

    private func clampedStreamZoomOffset(_ offset: CGSize, scale: CGFloat) -> CGSize {
        guard scale > 1.001, viewportSize.width > 0, viewportSize.height > 0 else {
            return .zero
        }
        let maximumX = viewportSize.width * (scale - 1) / 2
        let maximumY = viewportSize.height * (scale - 1) / 2
        return CGSize(
            width: min(max(offset.width, -maximumX), maximumX),
            height: min(max(offset.height, -maximumY), maximumY)
        )
    }

    private func resetStreamZoom() {
        guard streamZoomScale != 1 || streamZoomOffset != .zero else { return }
        streamZoomScale = 1
        streamZoomOffset = .zero
        renderer?.setViewportTransform(scale: 1, offset: .zero)
    }

    private func updateRenderedVideoSize(_ size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        if renderedVideoSize.width > 0, renderedVideoSize.height > 0 {
            let previousAspectRatio = renderedVideoSize.width / renderedVideoSize.height
            let nextAspectRatio = size.width / size.height
            let relativeDifference = abs(previousAspectRatio - nextAspectRatio) / max(previousAspectRatio, 0.001)
            if relativeDifference > 0.01 {
                resetStreamZoom()
            }
        }
        renderedVideoSize = size
    }

    func togglePictureInPicture() {
        if pictureInPictureBridge.isPictureInPictureActive {
            pictureInPictureBridge.stop()
        } else if !pictureInPictureBridge.start() {
            updateStatus("Picture in Picture unavailable", detail: "Waiting for a hardware-decoded video frame")
        }
    }

    func setAudioEnabled(_ enabled: Bool) {
        var preferences = streamerPreferences
        preferences.audioMuted = !enabled
        setStreamerPreferences(preferences)
        applyLiveAudioPreference()
    }

    func setStatsClockVisible(_ visible: Bool) {
        var preferences = streamerPreferences
        preferences.showStatsClock = visible
        setStreamerPreferences(preferences)
        refreshDeviceStatus()
    }

    func setStatsBatteryVisible(_ visible: Bool) {
        var preferences = streamerPreferences
        preferences.showStatsBattery = visible
        setStreamerPreferences(preferences)
        refreshDeviceStatus()
    }

    func setStatsNetworkVisible(_ visible: Bool) {
        var preferences = streamerPreferences
        preferences.showStatsCellular = visible
        setStreamerPreferences(preferences)
    }

    func setTouchControllerVisible(_ visible: Bool) {
        if !visible {
            touchLayoutEditing = false
        }
        var preferences = streamerPreferences
        preferences.touchControllerVisible = visible
        setStreamerPreferences(preferences)
        updateVirtualControllerAvailability()
        if visible {
            scheduleGuidancePresentation()
        }
    }

    func setPhysicalControllerPassthrough(_ enabled: Bool) {
        var preferences = streamerPreferences
        preferences.physicalControllerPassthrough = enabled
        setStreamerPreferences(preferences)
        inputBridge.setPhysicalControllerPassthrough(enabled)
    }

    func setTouchLayoutScale(_ value: Double) {
        updateTouchLayout { $0.scale = min(max(value, 0.6), 1.4) }
    }

    func setTouchButtonScale(_ value: Double) {
        updateTouchLayout { $0.buttonScale = min(max(value, 0.65), 1.5) }
    }

    func setTouchStickScale(_ value: Double) {
        updateTouchLayout { $0.stickScale = min(max(value, 0.65), 1.5) }
    }

    func setTouchOpacity(_ value: Double) {
        updateTouchLayout { $0.opacity = min(max(value, 0.15), 1.0) }
    }

    func beginTouchLayoutEditing() {
        if !streamerPreferences.touchControllerVisible {
            var preferences = streamerPreferences
            preferences.touchControllerVisible = true
            setStreamerPreferences(preferences)
        }
        touchLayoutEditing = true
        controlsPanelVisible = false
        updateVirtualControllerAvailability()
    }

    fileprivate func endTouchLayoutEditing() {
        touchLayoutEditing = false
        updateVirtualControllerAvailability()
    }

    fileprivate func resetTouchLayout() {
        touchLayout = TouchControlLayout.preset(for: touchLayoutProfile)
        onTouchLayoutChange(touchLayoutProfile, touchLayout)
    }

    fileprivate func setTouchLayoutPosition(
        _ group: NativeStreamTouchControlGroup,
        _ point: TouchControlPoint
    ) {
        let normalized = TouchControlPoint(
            x: min(max(point.x, 0), 1),
            y: min(max(point.y, 0), 1)
        )
        updateTouchLayout { layout in
            switch group {
            case .topLeft: layout.topLeft = normalized
            case .topCenter: layout.topCenter = normalized
            case .topRight: layout.topRight = normalized
            case .leftStick: layout.leftStick = normalized
            case .rightCluster: layout.rightCluster = normalized
            case .bottomCenter: layout.bottomCenter = normalized
            }
        }
    }

    fileprivate func sendVirtualKey(_ key: NativeStreamVirtualKey) {
        let mapping = key.mapping
        inputBridge.sendKey(mapping: mapping, pressed: true, modifiers: 0)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.045) { [weak self] in
            self?.inputBridge.sendKey(mapping: mapping, pressed: false, modifiers: 0)
        }
    }

    @discardableResult
    fileprivate func sendTextToStream(_ text: String) -> Int {
        guard reliableInputChannel?.readyState == .open else {
            return 0
        }
        let sentCharacters = inputBridge.sendUnicodeText(text)
        guard sentCharacters > 0 else { return 0 }
        log("Sent stream text characters=\(sentCharacters)")
        return sentCharacters
    }

    @discardableResult
    fileprivate func clearStreamText(disableFutureConfirmation: Bool) -> Bool {
        guard reliableInputChannel?.readyState == .open else { return false }
        inputBridge.clearRemoteText()
        if disableFutureConfirmation, !liveSettings.streamKeyboardClearConfirmationDisabled {
            liveSettings.streamKeyboardClearConfirmationDisabled = true
            onSettingsChange(liveSettings)
        }
        log("Cleared focused stream text field")
        return true
    }

    private func setStreamerPreferences(_ preferences: StreamerPreferences) {
        streamerPreferences = preferences
        onStreamerPreferencesChange(preferences)
    }

    private func refreshDeviceStatus() {
        deviceStatus = NativeStreamDeviceStatus.current(
            networkTransport: deviceStatus.networkTransport
        )
    }

    private func startNetworkMonitoring() {
        guard !networkMonitorStarted else { return }
        networkMonitorStarted = true
        networkMonitor.pathUpdateHandler = { [weak self] path in
            let transport = NativeStreamNetworkTransport(path: path)
            Task { @MainActor [weak self] in
                guard let self, !self.stopped else { return }
                var updatedStatus = self.deviceStatus
                guard updatedStatus.networkTransport != transport else { return }
                updatedStatus.networkTransport = transport
                self.deviceStatus = updatedStatus
            }
        }
        networkMonitor.start(queue: networkMonitorQueue)
    }

    private func stopNetworkMonitoring() {
        guard networkMonitorStarted else { return }
        networkMonitor.pathUpdateHandler = nil
        networkMonitor.cancel()
    }

    private func updateVirtualControllerAvailability() {
        inputBridge.setVirtualControllerEnabled(virtualControllerInputEnabled)
    }

    private func handlePhysicalControllerAvailabilityChanged(_ connected: Bool) {
        physicalControllerConnected = connected
        if !connected, presentedGuidanceSheet == .controllerTouchPrompt {
            presentedGuidanceSheet = nil
        }
        updateVirtualControllerAvailability()
        if connected {
            scheduleGuidancePresentation()
        }
    }

    private func handlePhysicalKeyboardMouseAvailabilityChanged(_ connected: Bool) {
        guard connected != physicalKeyboardMouseConnected else { return }
        physicalKeyboardMouseConnected = connected
        inputModePrompt = nil
        switch (streamInputMode, connected, session.touchProvisioned == true) {
        case (.nativeTouch, true, _):
            pendingInputModePrompt = .switchToKeyboardMouse
        case (.keyboardMouse, false, true):
            pendingInputModePrompt = .switchToNativeTouch
        default:
            pendingInputModePrompt = nil
        }
        presentPendingInputModePromptIfPossible()
    }

    fileprivate func presentPendingInputModePromptIfPossible() {
        guard inputModePrompt == nil,
              !controlsPanelVisible,
              presentedGuidanceSheet == nil,
              let pendingInputModePrompt else { return }
        self.pendingInputModePrompt = nil
        inputModePrompt = pendingInputModePrompt
    }

    fileprivate func acceptInputModePrompt(_ prompt: NativeStreamInputModePrompt) {
        streamInputMode = prompt == .switchToKeyboardMouse ? .keyboardMouse : .nativeTouch
        inputModePrompt = nil
        log("Stream input mode switched to \(streamInputMode == .nativeTouch ? "native touch" : "keyboard and mouse")")
    }

    fileprivate func dismissInputModePrompt() {
        inputModePrompt = nil
    }

    private func completeStreamTutorial() {
        presentedGuidanceSheet = nil
        tutorialDoneCalloutVisible = false
        if !streamTutorialCompleted {
            streamTutorialCompleted = true
            onStreamTutorialCompleted()
        }
        scheduleGuidancePresentation()
    }

    private func scheduleGuidancePresentation(delayNanoseconds: UInt64 = 350_000_000) {
        guidancePresentationTask?.cancel()
        guard videoActive else { return }
        guidancePresentationTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: delayNanoseconds)
            guard !Task.isCancelled, let self else { return }
            self.presentNextGuidanceIfNeeded()
        }
    }

    private func presentNextGuidanceIfNeeded() {
        guard videoActive,
              latestScenePhase == .active,
              presentedGuidanceSheet == nil,
              !controlsPanelVisible,
              !touchLayoutEditing,
              !isPictureInPictureActive else {
            return
        }
        if !streamTutorialCompleted {
            presentedGuidanceSheet = .streamTutorial
            return
        }
        guard physicalControllerConnected,
              streamerPreferences.touchControllerVisible,
              !controllerTouchPromptDismissed,
              !controllerTouchPromptHandledThisSession else {
            return
        }
        presentedGuidanceSheet = .controllerTouchPrompt
    }

    private func updateTouchLayout(_ update: (inout TouchControlLayout) -> Void) {
        var next = touchLayout
        update(&next)
        touchLayout = next
        onTouchLayoutChange(touchLayoutProfile, next)
    }

    fileprivate func attachRenderer(_ renderer: NativeStreamRenderView) {
        if self.renderer !== renderer {
            self.renderer?.metalDelegate = nil
            renderer.metalDelegate = self
            renderer.setStretchStreamToFill(streamerPreferences.stretchStreamToFill)
            renderer.setStreamSharpening(enabled: streamSharpeningEnabled, amount: streamSharpeningAmount)
            renderer.setViewportTransform(scale: streamZoomScale, offset: streamZoomOffset)
            videoSink.attach(renderView: renderer)
            pictureInPictureBridge.attach(displayLayer: renderer.pictureInPictureDisplayLayer)
        }
        self.renderer = renderer
        renderer.setVideoCodec(selectedCodec)
        renderer.setStretchStreamToFill(streamerPreferences.stretchStreamToFill)
        renderer.setStreamSharpening(enabled: streamSharpeningEnabled, amount: streamSharpeningAmount)
        renderer.setViewportTransform(scale: streamZoomScale, offset: streamZoomOffset)
        renderer.setVideoEffects(upscaling: liveSettings.metalFXUpscalingEnabled, metal4: liveSettings.metal4Enabled)
        attachCurrentVideoSinkIfNeeded()
    }

    func close() {
        stop()
        onClose()
    }

    func retry() {
        stop()
        onRetry?()
    }

    func stop() {
        guard !stopped else { return }
        stopped = true
        nativeNVSTStart?.cancel(); nativeNVSTStart = nil
        let nativeTransport = nativeNVST; nativeNVST = nil
        Task { await nativeTransport?.close() }
        guidancePresentationTask?.cancel()
        guidancePresentationTask = nil
        sessionWarningDismissTask?.cancel()
        sessionWarningDismissTask = nil
        sessionWarningText = nil
        presentedGuidanceSheet = nil
        setAntiAfkEnabled(false)
        modeNoticeDismissTask?.cancel()
        modeNoticeDismissTask = nil
        modeChangeNotice = nil
        stopNetworkMonitoring()
        inputBridge.detach()
        setIdleTimerDisabled(false)
        stopAudioSessionObservation()
        audioSessionRecoveryTask?.cancel()
        audioSessionRecoveryTask = nil
        audioTrack?.isEnabled = false
        audioTrack = nil
        teardownStreamAudioPlayback()
        offerTimeoutWorkItem?.cancel()
        offerTimeoutWorkItem = nil
        iceDisconnectWorkItem?.cancel()
        iceDisconnectWorkItem = nil
        mediaTransportConnected = false
        mediaLivenessWatchdog.reset()
        packetLossRecoveryGate.reset()
        signalingHeartbeat?.cancel()
        signalingHeartbeat = nil
        statsTimer?.cancel()
        statsTimer = nil
        pictureInPictureBridge.detach()
        pictureInPictureAvailable = false
        isPictureInPictureActive = false
        webSocket?.cancel(with: .goingAway, reason: nil)
        webSocket = nil
        signalingSession?.invalidateAndCancel()
        signalingSession = nil
        reliableInputChannel?.delegate = nil
        partiallyReliableInputChannel?.delegate = nil
        controlChannel?.delegate = nil
        reliableInputChannel?.close()
        partiallyReliableInputChannel?.close()
        controlChannel?.close()
        reliableInputChannel = nil
        partiallyReliableInputChannel = nil
        controlChannel = nil
        if let videoTrack, videoSinkAttached {
            videoTrack.remove(videoSink)
        }
        videoSink.detach()
        renderer?.metalDelegate = nil
        videoTrack = nil
        videoSinkAttached = false
        peerConnection?.delegate = nil
        peerConnection?.close()
        peerConnection = nil
        factory = nil
        mutedAudioDevice = nil
        playbackAudioDevice = nil
        answerSent = false
        queuedLocalIceCandidates.removeAll(keepingCapacity: true)
    }

    /// Rebuilds only the local signaling/decoder transport. The CloudMatch allocation, selected
    /// codec, resolution, frame rate, bitrate, input attachment, and visible stream surface all
    /// remain owned by this coordinator. In particular this never ends or requeues the session.
    @discardableResult
    private func requestSelectedVideoProfileRetry(_ reason: String) -> Bool {
        guard !stopped, !selectedProfileRetryApplied else { return false }
        selectedProfileRetryApplied = true
        let hadStableMedia = transportWasStable
        onSelectedVideoProfileRetry(reason)
        log(
            "Selected profile transport retry codec=\(selectedCodec.rawValue) "
                + "resolution=\(streamProfile.resolutionString) fps=\(streamProfile.fps) "
                + "bitrate=\(streamProfile.maxBitrateKbps) reason=\(reason)"
        )
        updateStatus("Reconnecting", detail: "Retrying your selected stream settings")
        closeLocalTransportForRetry()

        let settleDelay: TimeInterval = hadStableMedia && selectedCodec != .h264 ? 0.18 : 0
        workQueue.asyncAfter(deadline: .now() + settleDelay) { [weak self] in
            Task { @MainActor in
                guard let self, !self.stopped else { return }
                self.connectSignaling()
            }
        }
        return true
    }

    private func closeLocalTransportForRetry() {
        offerTimeoutWorkItem?.cancel()
        offerTimeoutWorkItem = nil
        iceDisconnectWorkItem?.cancel()
        iceDisconnectWorkItem = nil
        signalingHeartbeat?.cancel()
        signalingHeartbeat = nil
        statsTimer?.cancel()
        statsTimer = nil

        let closingWebSocket = webSocket
        webSocket = nil
        closingWebSocket?.cancel(with: .goingAway, reason: nil)
        let closingSignalingSession = signalingSession
        signalingSession = nil
        closingSignalingSession?.invalidateAndCancel()

        reliableInputChannel?.delegate = nil
        partiallyReliableInputChannel?.delegate = nil
        controlChannel?.delegate = nil
        reliableInputChannel?.close()
        partiallyReliableInputChannel?.close()
        controlChannel?.close()
        reliableInputChannel = nil
        partiallyReliableInputChannel = nil
        controlChannel = nil

        audioTrack?.isEnabled = false
        audioTrack = nil
        if let videoTrack, videoSinkAttached {
            videoTrack.remove(videoSink)
        }
        videoTrack = nil
        videoSinkAttached = false
        if let renderer {
            videoSink.attach(renderView: renderer)
        }

        let closingPeerConnection = peerConnection
        peerConnection = nil
        closingPeerConnection?.delegate = nil
        closingPeerConnection?.close()
        teardownStreamAudioPlayback()
        factory = nil
        mutedAudioDevice = nil
        playbackAudioDevice = nil

        peerId = 0
        remotePeerId = 1
        answerSent = false
        queuedLocalIceCandidates.removeAll(keepingCapacity: true)
        mediaTransportConnected = false
        transportWasStable = false
        mediaLivenessWatchdog.reset()
        recoveryProgressTracker = NativeStreamRecoveryProgressTracker()
        packetLossRecoveryGate.reset()
        decoderRecoveryGate.reset()
        videoActive = false
        renderedFrameCount = 0
        receivedColorMode = "--"
        decodedWithoutRenderStartedAt = nil
        lastRenderKeyframeRequestAt = nil
        renderKeyframeAttempts = 0
        lastRenderedStatsProgressAt = nil
        lastPostStartKeyframeRequestAt = nil
        postStartKeyframeAttempts = 0
        lastStatsSampleAt = nil
        lastStatsFramesReceived = nil
        lastStatsFramesDecoded = nil
        lastStatsFramesRendered = nil
        lastStatsBytesReceived = nil
        lastStatsTotalDecodeTime = nil
        lastStatsPacketsLost = nil
        lastStatsPacketsReceived = nil
    }

    private func setVideoTrack(_ track: RTCVideoTrack) {
        if let videoTrack, videoTrack !== track {
            resetStreamZoom()
        }
        if let existingTrack = videoTrack, existingTrack !== track, videoSinkAttached {
            existingTrack.remove(videoSink)
            videoSinkAttached = false
        }
        videoTrack = track
        attachCurrentVideoSinkIfNeeded()
        updateStatus("Video track attached", detail: "Waiting for rendered frames")
    }

    private func setAudioTrack(_ track: RTCAudioTrack) {
        audioTrack = track
        // The remote track can arrive after the one-time startup configuration. Re-arming here
        // closes the first-session race where WebRTC creates its audio unit after iOS has already
        // accepted the app's playback category, but never starts playout for that first track.
        configureStreamAudioPlayback()
        track.isEnabled = shouldPlayAudio
        log("Native audio track attached enabled=\(track.isEnabled)")
    }

    private func attachCurrentVideoSinkIfNeeded() {
        guard !videoSinkAttached, let videoTrack, renderer != nil else { return }
        videoTrack.add(videoSink)
        videoSinkAttached = true
        log("Attached native Metal video sink")
    }

    private func connectSignaling() {
        guard let url = signalingURL() else {
            fail("Missing signaling endpoint")
            return
        }
        updateStatus("Connecting", detail: url.host ?? "signaling")
        let configuration = URLSessionConfiguration.default
        configuration.timeoutIntervalForRequest = 20
        configuration.timeoutIntervalForResource = 60
        let urlSession = URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
        signalingSession = urlSession
        var request = URLRequest(url: url)
        request.timeoutInterval = 20
        request.setValue("x-nv-sessionid.\(session.id)", forHTTPHeaderField: "Sec-WebSocket-Protocol")
        request.setValue("https://play.geforcenow.com", forHTTPHeaderField: "Origin")
        request.setValue("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36", forHTTPHeaderField: "User-Agent")
        if let host = url.host {
            let hostHeader = url.port.map { "\(host):\($0)" } ?? host
            request.setValue(hostHeader, forHTTPHeaderField: "Host")
        }
        let task = urlSession.webSocketTask(with: request)
        webSocket = task
        scheduleOfferTimeout()
        task.resume()
        receiveSignaling(from: task)
    }

    private func signalingURL() -> URL? {
        let rawBase = session.signalingUrl?.trimmingCharacters(in: .whitespacesAndNewlines)
        let fallbackHost = session.signalingServer?.trimmingCharacters(in: .whitespacesAndNewlines)
            ?? session.serverIp?.trimmingCharacters(in: .whitespacesAndNewlines)
            ?? ""
        let base: String
        if let rawBase, !rawBase.isEmpty {
            base = rawBase
        } else if !fallbackHost.isEmpty {
            let host = fallbackHost.contains(":") ? fallbackHost : "\(fallbackHost):443"
            base = "wss://\(host)/nvst/"
        } else {
            return nil
        }
        let withoutScheme = base
            .replacingOccurrences(of: "wss://", with: "", options: [.caseInsensitive])
            .replacingOccurrences(of: "ws://", with: "", options: [.caseInsensitive])
            .replacingOccurrences(of: "https://", with: "", options: [.caseInsensitive])
            .replacingOccurrences(of: "http://", with: "", options: [.caseInsensitive])
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        return URL(string: "wss://\(withoutScheme)/sign_in?peer_id=\(peerName)&version=2&peer_role=1&pairing_id=\(session.id)")
    }

    private func receiveSignaling(from task: URLSessionWebSocketTask) {
        task.receive { [weak self, weak task] result in
            guard let self else { return }
            Task { @MainActor in
                guard let task, !self.stopped, self.webSocket === task else { return }
                switch result {
                case .success(.string(let text)):
                    self.handleSignalingText(text)
                    self.receiveSignaling(from: task)
                case .success(.data(let data)):
                    if let text = String(data: data, encoding: .utf8) {
                        self.handleSignalingText(text)
                    }
                    self.receiveSignaling(from: task)
                case .failure(let error):
                    self.handleSignalingFailure("Signaling failed: \(error.localizedDescription)", task: task)
                @unknown default:
                    self.receiveSignaling(from: task)
                }
            }
        }
    }

    private func handleSignalingFailure(_ message: String, task: URLSessionWebSocketTask) {
        guard !stopped, webSocket === task else { return }
        let disposition = nativeStreamSignalingFailureDisposition(message)
        if nativeStreamShouldPreserveMediaAfterSignalingFailure(
            disposition,
            mediaConnected: mediaTransportConnected || peerConnection?.iceConnectionState == .checking
        ) {
            signalingHeartbeat?.cancel()
            signalingHeartbeat = nil
            webSocket = nil
            task.cancel(with: .goingAway, reason: nil)
            signalingSession?.invalidateAndCancel()
            signalingSession = nil
            log("Signaling ended while media remained connected; preserving the active stream")
            return
        }

        switch disposition {
        case .recoverSession, .sessionEnded:
            // The outer retry refreshes the provider's active-session record and claims its
            // current endpoint. It does not stop or replace the cloud allocation.
            fail(message)
        case .retryTransport, .retrySignaling:
            if !requestSelectedVideoProfileRetry(message) { fail(message) }
        }
    }

    private func handleSignalingText(_ text: String) {
        guard let data = text.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            log("Ignoring non-JSON signaling packet")
            return
        }

        if let info = object["peer_info"] as? [String: Any],
           let name = info["name"] as? String,
           name == peerName {
            peerId = info["id"] as? Int ?? peerId
        }

        if let ack = object["ackid"] as? Int {
            let sourceId = (object["peer_info"] as? [String: Any])?["id"] as? Int
            if sourceId != peerId {
                sendJSON(["ack": ack])
            }
        }

        if object["hb"] != nil {
            sendJSON(["hb": 1])
            return
        }

        guard let peerMessage = object["peer_msg"] as? [String: Any] else { return }
        remotePeerId = peerMessage["from"] as? Int ?? remotePeerId
        guard let message = peerMessage["msg"] as? String,
              let messageData = message.data(using: .utf8),
              let payload = try? JSONSerialization.jsonObject(with: messageData) as? [String: Any] else {
            return
        }

        if payload["type"] as? String == "offer", let sdp = payload["sdp"] as? String {
            handleOffer(sdp)
        } else if let candidate = payload["candidate"] as? String {
            addRemoteCandidate(
                candidate,
                sdpMid: payload["sdpMid"] as? String,
                sdpMLineIndex: payload["sdpMLineIndex"] as? Int
            )
        }
    }

    private func sendPeerInfo() {
        sendJSON([
            "ackid": nextAckId(),
            "peer_info": [
                "browser": "Chrome",
                "browserVersion": "131",
                "connected": true,
                "id": peerId,
                "name": peerName,
                "peerRole": 0,
                "resolution": streamProfile.resolutionString,
                "version": 2
            ]
        ])
    }

    private func handleOffer(_ offerSDP: String) {
        offerTimeoutWorkItem?.cancel()
        updateStatus("Negotiating video", detail: "\(selectedCodec.rawValue) \(streamProfile.resolutionString) @ \(streamProfile.fps) fps")
        inputBridge.configure(
            protocolVersion: NativeStreamSDP.parseInputProtocolVersion(from: offerSDP),
            partiallyReliableGamepadMask: NativeStreamSDP.parsePartiallyReliableGamepadMask(from: offerSDP)
        )

        workQueue.async { [weak self] in
            guard let self else { return }
            Task { @MainActor in
                self.createPeerConnection(with: offerSDP)
            }
        }
    }

    private func createPeerConnection(with offerSDP: String) {
        if factory == nil {
            NativeStreamWebRTCPolicy.initialize()
            _ = RTCInitializeSSL()
            #if targetEnvironment(simulator)
            let audioDevice = NativeStreamMutedAudioDevice()
            mutedAudioDevice = audioDevice
            factory = RTCPeerConnectionFactory(
                encoderFactory: RTCDefaultVideoEncoderFactory(),
                decoderFactory: NativeStreamVideoDecoderFactory(),
                audioDevice: audioDevice
            )
            log("Using muted WebRTC audio device")
            #else
            let audioDevice = NativeStreamPlaybackAudioDevice(
                playbackEnabled: shouldPlayAudio,
                onEvent: { [weak self] message in
                    Task { @MainActor [weak self] in self?.log(message) }
                }
            )
            playbackAudioDevice = audioDevice
            factory = RTCPeerConnectionFactory(
                encoderFactory: RTCDefaultVideoEncoderFactory(),
                decoderFactory: NativeStreamVideoDecoderFactory(),
                audioDevice: audioDevice
            )
            log("Using output-only WebRTC playback device")
            #endif
        }
        guard let factory else {
            fail("Could not create WebRTC factory")
            return
        }
        answerSent = false
        queuedLocalIceCandidates.removeAll(keepingCapacity: true)

        let configuration = RTCConfiguration()
        configuration.iceServers = session.iceServers.map {
            RTCIceServer(urlStrings: $0.urls, username: $0.username, credential: $0.credential)
        }
        configuration.sdpSemantics = .unifiedPlan
        configuration.bundlePolicy = .maxBundle
        configuration.rtcpMuxPolicy = .require
        configuration.tcpCandidatePolicy = NativeStreamTransportPolicy.allowsTCPCandidates ? .enabled : .disabled
        configuration.continualGatheringPolicy = .gatherContinually

        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard let peerConnection = factory.peerConnection(
            with: configuration,
            constraints: constraints,
            delegate: self
        ) else {
            fail("Could not create WebRTC peer connection")
            return
        }
        self.peerConnection = peerConnection

        let reliableConfig = RTCDataChannelConfiguration()
        reliableConfig.isOrdered = true
        reliableInputChannel = peerConnection.dataChannel(forLabel: "input_channel_v1", configuration: reliableConfig)
        reliableInputChannel?.delegate = self

        let partialConfig = RTCDataChannelConfiguration()
        partialConfig.isOrdered = false
        partialConfig.maxPacketLifeTime = Int32(NativeStreamSDP.parsePartialReliableThresholdMs(from: offerSDP))
        partiallyReliableInputChannel = peerConnection.dataChannel(
            forLabel: "input_channel_partially_reliable",
            configuration: partialConfig
        )
        partiallyReliableInputChannel?.delegate = self

        var processedOffer = prepareRemoteOffer(offerSDP)
        let didRewriteCandidate = processedOffer != offerSDP
        negotiationDebugText = [
            "media \(session.mediaIp ?? "nil"):\(session.mediaPort)",
            "sdpFix \(didRewriteCandidate ? "yes" : "no")"
        ].joined(separator: " ")
        let preferTenBit = settings.hdrEnabled || settings.preferredColorQuality.hasPrefix("10bit")
        processedOffer = NativeStreamSDP.preferCodec(in: processedOffer, codec: selectedCodec, preferTenBit: preferTenBit)
        negotiationDebugText = [
            negotiationDebugText,
            NativeStreamSDP.describeOfferedVideoCodecs(from: processedOffer)
        ].filter { !$0.isEmpty }.joined(separator: " ")
        let remoteDescription = RTCSessionDescription(type: .offer, sdp: processedOffer)
        peerConnection.setRemoteDescription(remoteDescription) { [weak self] error in
            Task { @MainActor in
                guard let self, self.peerConnection === peerConnection else { return }
                if let error {
                    self.fail("Remote SDP failed: \(error.localizedDescription)")
                    return
                }
                self.applyCodecPreferences()
                self.createAnswer(for: processedOffer)
            }
        }
    }

    private func prepareRemoteOffer(_ offerSDP: String) -> String {
        var prepared = NativeStreamSDP.fixServerEndpoint(
            in: offerSDP,
            serverIP: session.serverIp,
            mediaIP: session.mediaIp,
            mediaPort: session.mediaPort
        )
        guard selectedCodec == .h265 else { return prepared }

        let maxLevels = h265ReceiverMaxLevelsByProfile()
        if !maxLevels.isEmpty {
            let rewritten = NativeStreamSDP.rewriteH265LevelIdByProfile(in: prepared, maxLevelByProfile: maxLevels)
            if rewritten.replacements > 0 {
                log("H265 level-id clamped replacements=\(rewritten.replacements) maxLevels=\(maxLevels)")
                prepared = rewritten.sdp
            }
        }

        if !supportsH265TierFlagOne() {
            let rewritten = NativeStreamSDP.rewriteH265TierFlag(in: prepared, tierFlag: 0)
            if rewritten.replacements > 0 {
                log("H265 tier-flag rewritten replacements=\(rewritten.replacements)")
                prepared = rewritten.sdp
            }
        }

        return prepared
    }

    private func applyCodecPreferences() {
        guard let peerConnection, let factory else { return }
        guard let videoTransceiver = peerConnection.transceivers.first(where: { $0.mediaType == .video }) else {
            log("Codec preferences skipped: no video transceiver")
            return
        }

        let receiverCapabilities = factory.rtpReceiverCapabilities(forKind: kRTCMediaStreamTrackKindVideo)
        var preferred = receiverCapabilities.codecs.filter { capabilityMatches($0, codec: selectedCodec) }
        let auxiliary = receiverCapabilities.codecs.filter { capability in
            let name = capability.name.uppercased()
            let mime = capability.mimeType.uppercased()
            return name.contains("RTX") || name.contains("FLEXFEC") || mime.contains("RTX") || mime.contains("FLEXFEC")
        }

        guard !preferred.isEmpty else {
            log("Codec preferences skipped: \(selectedCodec.rawValue) not in receiver capabilities")
            return
        }

        if selectedCodec == .h265 {
            preferred.sort { hevcCapabilityPriority($0) < hevcCapabilityPriority($1) }
        }

        let codecList = preferred + auxiliary
        do {
            try videoTransceiver.setCodecPreferences(codecList, error: ())
            log("Codec preferences set: \(selectedCodec.rawValue) preferred=\(preferred.count) auxiliary=\(auxiliary.count)")
        } catch {
            log("Codec preferences failed: \(error.localizedDescription)")
        }
    }

    private func capabilityMatches(_ capability: RTCRtpCodecCapability, codec: NativeStreamVideoCodec) -> Bool {
        let name = capability.name.uppercased()
        let mime = capability.mimeType.uppercased()
        switch codec {
        case .h264:
            return name == "H264" || name == "AVC" || mime == "VIDEO/H264" || mime == "VIDEO/AVC"
        case .h265:
            return name == "H265" || name == "HEVC" || mime == "VIDEO/H265" || mime == "VIDEO/HEVC"
        case .av1:
            return name == "AV1" || name == "AV01" || mime == "VIDEO/AV1" || mime == "VIDEO/AV01"
        }
    }

    private func hevcCapabilityPriority(_ capability: RTCRtpCodecCapability) -> Int {
        let profileID = capability.parameters["profile-id"] ?? capability.parameters["profile_id"]
        if profileID == "1" { return 0 }
        if profileID == nil { return 1 }
        return 2
    }

    private func h265ReceiverMaxLevelsByProfile() -> [Int: Int] {
        receiverH265Capabilities()
            .compactMap { capability -> (Int, Int)? in
                guard let profile = capability.codecParameterInt("profile-id"),
                      let level = capability.codecParameterInt("level-id") else {
                    return nil
                }
                return (profile, level)
            }
            .reduce(into: [Int: Int]()) { partial, pair in
                partial[pair.0] = max(partial[pair.0] ?? 0, pair.1)
            }
            .filter { $0.value > 0 }
    }

    private func supportsH265TierFlagOne() -> Bool {
        receiverH265Capabilities().contains { $0.codecParameterInt("tier-flag") == 1 }
    }

    private func receiverH265Capabilities() -> [RTCRtpCodecCapability] {
        guard let factory else { return [] }
        return factory.rtpReceiverCapabilities(forKind: kRTCMediaStreamTrackKindVideo).codecs.filter {
            capabilityMatches($0, codec: .h265)
        }
    }

    private func createAnswer(for processedOffer: String) {
        guard let peerConnection else { return }
        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        peerConnection.answer(for: constraints) { [weak self] answer, error in
            Task { @MainActor in
                guard let self, self.peerConnection === peerConnection else { return }
                if let error {
                    self.fail("Answer creation failed: \(error.localizedDescription)")
                    return
                }
                guard let answer else {
                    self.fail("Answer creation returned no SDP")
                    return
                }
                let mungedSDP = NativeStreamSDP.mungeAnswerSDP(answer.sdp, maxBitrateKbps: self.streamProfile.maxBitrateKbps)
                self.localCodecDebugText = NativeStreamSDP.describeNegotiatedVideo(from: mungedSDP)
                self.log("Local video SDP \(self.localCodecDebugText)")
                if self.selectedCodec != .h264,
                   !NativeStreamSDP.negotiatesCodec(mungedSDP, codec: self.selectedCodec) {
                    let reason = "\(self.selectedCodec.rawValue) was requested but WebRTC did not negotiate it"
                    self.log("Local answer did not negotiate requested codec \(self.selectedCodec.rawValue); retrying selected profile")
                    if !self.requestSelectedVideoProfileRetry(reason) {
                        self.fail(reason)
                    }
                    return
                }
                let localDescription = RTCSessionDescription(type: .answer, sdp: mungedSDP)
                guard let connection = self.peerConnection else {
                    self.fail("Peer connection closed before local SDP")
                    return
                }
                do {
                    try await connection.setLocalDescription(localDescription)
                } catch {
                    self.fail("Local SDP failed: \(error.localizedDescription)")
                    return
                }
                self.sendAnswer(offerSDP: processedOffer)
                self.startStatsTimer()
            }
        }
    }

    private func sendAnswer(offerSDP: String) {
        guard let localSDP = peerConnection?.localDescription?.sdp else {
            fail("Local answer SDP was unavailable")
            return
        }
        let nvst = NativeStreamSDP.buildNvstSDP(
            offerSDP: offerSDP,
            localAnswerSDP: localSDP,
            profile: streamProfile,
            settings: settings,
            codec: selectedCodec
        )
        sendPeerMessage([
            "type": "answer",
            "sdp": localSDP,
            "nvstSdp": nvst
        ])
        answerSent = true
        flushQueuedLocalIceCandidates()
        updateStatus("Connecting media", detail: "Waiting for first frame")
    }

    private func sendLocalIceCandidate(_ candidate: RTCIceCandidate) {
        let payload: [String: Any] = [
            "candidate": candidate.sdp,
            "sdpMid": candidate.sdpMid ?? "",
            "sdpMLineIndex": Int(candidate.sdpMLineIndex)
        ]
        if answerSent {
            sendPeerMessage(payload)
        } else {
            queuedLocalIceCandidates.append(payload)
        }
    }

    private func flushQueuedLocalIceCandidates() {
        guard answerSent, !queuedLocalIceCandidates.isEmpty else { return }
        let queued = queuedLocalIceCandidates
        queuedLocalIceCandidates.removeAll(keepingCapacity: true)
        for candidate in queued {
            sendPeerMessage(candidate)
        }
    }

    private func addRemoteCandidate(_ candidate: String, sdpMid: String?, sdpMLineIndex: Int?) {
        guard let peerConnection else { return }
        let rewrittenCandidate = NativeStreamSDP.rewriteIceCandidateEndpoint(
            candidate,
            mediaIP: session.mediaIp,
            mediaPort: session.mediaPort
        )
        if rewrittenCandidate != candidate {
            log("Rewrote remote ICE candidate to media endpoint \(session.mediaIp ?? "nil"):\(session.mediaPort)")
        }
        let ice = RTCIceCandidate(
            sdp: rewrittenCandidate,
            sdpMLineIndex: Int32(sdpMLineIndex ?? 0),
            sdpMid: sdpMid
        )
        peerConnection.add(ice) { [weak self] error in
            Task { @MainActor in
                guard let self else { return }
                if let error {
                    self.log("Remote ICE rejected: \(error.localizedDescription)")
                }
            }
        }
    }

    private func sendPeerMessage(_ payload: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let message = String(data: data, encoding: .utf8) else {
            return
        }
        sendJSON([
            "peer_msg": [
                "from": peerId,
                "to": remotePeerId,
                "msg": message
            ],
            "ackid": nextAckId()
        ])
    }

    private func sendJSON(_ object: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: object),
              let text = String(data: data, encoding: .utf8) else {
            return
        }
        webSocket?.send(.string(text)) { [weak self] error in
            if let error {
                Task { @MainActor in
                    self?.log("Signaling send failed: \(error.localizedDescription)")
                }
            }
        }
    }

    private func nextAckId() -> Int {
        ackCounter += 1
        return ackCounter
    }

    private func scheduleOfferTimeout() {
        offerTimeoutWorkItem?.cancel()
        let item = DispatchWorkItem { [weak self] in
            Task { @MainActor in
                guard let self, !self.stopped, self.peerConnection == nil else { return }
                let reason = "Timed out waiting for WebRTC offer"
                if !self.requestSelectedVideoProfileRetry(reason) { self.fail(reason) }
            }
        }
        offerTimeoutWorkItem = item
        workQueue.asyncAfter(deadline: .now() + NativeStreamTransportPolicy.offerTimeout, execute: item)
    }

    private func startSignalingHeartbeat() {
        signalingHeartbeat?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: workQueue)
        timer.schedule(deadline: .now() + 5, repeating: 5)
        timer.setEventHandler { [weak self] in
            Task { @MainActor in
                self?.sendJSON(["hb": 1])
            }
        }
        signalingHeartbeat = timer
        timer.resume()
    }

    private func startStatsTimer() {
        statsTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: workQueue)
        timer.schedule(deadline: .now() + 1, repeating: 1)
        timer.setEventHandler { [weak self] in
            Task { @MainActor in
                self?.collectStats()
            }
        }
        statsTimer = timer
        timer.resume()
    }

    private func collectStats() {
        updateSessionTimer()
        guard let peerConnection else { return }
        peerConnection.statistics { [weak self, weak activePeerConnection = peerConnection] report in
            Task { @MainActor in
                guard let self,
                      let activePeerConnection,
                      self.peerConnection === activePeerConnection else { return }
                self.updateStats(from: report)
            }
        }
    }

    private func updateSessionTimer(now: Date = Date()) {
        guard settings.sessionCounterEnabled else { return }
        let snapshot = streamSessionTimerSnapshot(
            limit: sessionLimit,
            startedAt: session.startedAt,
            now: now
        )
        sessionDurationText = Self.formatSessionDuration(snapshot: snapshot, limit: sessionLimit)
        sessionTimerProgress = snapshot.progress
        sessionTimerWarningActive = snapshot.isWarning

        guard videoActive,
              let threshold = sessionWarningTracker.nextWarning(
                  remainingSeconds: snapshot.remainingSeconds
              ) else {
            return
        }
        presentSessionWarning(thresholdSeconds: threshold)
    }

    private func presentSessionWarning(thresholdSeconds: Int) {
        let minutes = thresholdSeconds / 60
        let interval = minutes == 1 ? "1 minute" : "\(minutes) minutes"
        let message = "\(interval) left in this session"
        sessionWarningDismissTask?.cancel()
        sessionWarningText = message
        UIAccessibility.post(notification: .announcement, argument: message)
        sessionWarningDismissTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(nanoseconds: 3_000_000_000)
            } catch {
                return
            }
            guard let self,
                  !Task.isCancelled,
                  self.sessionWarningText == message else {
                return
            }
            self.sessionWarningText = nil
            self.sessionWarningDismissTask = nil
        }
    }

    private static func formatSessionDuration(
        snapshot: StreamSessionTimerSnapshot,
        limit: StreamSessionLimit
    ) -> String {
        switch limit.mode {
        case .countdown:
            return "\(formatClock(snapshot.remainingSeconds)) left"
        case .stopwatch:
            return "\(formatClock(snapshot.elapsedSeconds)) / \(limit.limitHours)h"
        }
    }

    private static func formatClock(_ secondsValue: Int) -> String {
        let hours = secondsValue / 3_600
        let minutes = (secondsValue % 3_600) / 60
        let seconds = secondsValue % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        }
        return String(format: "%02d:%02d", minutes, seconds)
    }

    private func updateStats(from report: RTCStatisticsReport) {
        refreshDeviceStatus()
        var framesDecoded: Int?
        var framesReceived: Int?
        var framesRendered: Int?
        var framesPerSecond: Int?
        var framesDropped: Int?
        var width: Int?
        var height: Int?
        var bytesReceived: Int?
        var packetsLost: Int?
        var packetsReceived: Int?
        var jitterMs: Double?
        var rttMs: Double?
        var totalDecodeTimeSeconds: Double?
        var decoderImplementation: String?
        var selectedPairValues: [String: NSObject]?
        var candidateStats: [String: [String: NSObject]] = [:]

        for stat in report.statistics.values {
            guard stat.type == "inbound-rtp"
                    || stat.type == "local-candidate"
                    || stat.type == "remote-candidate"
                    || stat.type == "candidate-pair" else { continue }
            if stat.type == "inbound-rtp" {
                if let kind = stat.values["kind"] as? String, kind != "video" { continue }

                decoderImplementation = stat.values["decoderImplementation"] as? String ?? decoderImplementation
                framesDecoded = (stat.values["framesDecoded"] as? NSNumber)?.intValue ?? framesDecoded
                framesReceived = (stat.values["framesReceived"] as? NSNumber)?.intValue ?? framesReceived
                framesRendered = (stat.values["framesRendered"] as? NSNumber)?.intValue ?? framesRendered
                framesPerSecond = (stat.values["framesPerSecond"] as? NSNumber)?.intValue ?? framesPerSecond
                framesDropped = (stat.values["framesDropped"] as? NSNumber)?.intValue ?? framesDropped
                width = (stat.values["frameWidth"] as? NSNumber)?.intValue ?? width
                height = (stat.values["frameHeight"] as? NSNumber)?.intValue ?? height
                bytesReceived = (stat.values["bytesReceived"] as? NSNumber)?.intValue ?? bytesReceived
                packetsLost = (stat.values["packetsLost"] as? NSNumber)?.intValue ?? packetsLost
                packetsReceived = (stat.values["packetsReceived"] as? NSNumber)?.intValue ?? packetsReceived
                if let jitter = stat.values["jitter"] as? NSNumber {
                    jitterMs = jitter.doubleValue * 1000
                }
                // Cumulative seconds spent decoding. Divided by the frames decoded in the same
                // window it gives per-frame decode latency, which is the number worth showing.
                if let total = stat.values["totalDecodeTime"] as? NSNumber {
                    totalDecodeTimeSeconds = total.doubleValue
                }
            } else if stat.type == "local-candidate" || stat.type == "remote-candidate" {
                candidateStats[stat.id] = stat.values
            } else if stat.type == "candidate-pair",
                      (stat.values["state"] as? String) == "succeeded" {
                let nominated = (stat.values["nominated"] as? NSNumber)?.boolValue ?? false
                let selected = (stat.values["selected"] as? NSNumber)?.boolValue ?? false
                if selectedPairValues == nil || nominated || selected {
                    selectedPairValues = stat.values
                }
                if let rtt = stat.values["currentRoundTripTime"] as? NSNumber {
                    rttMs = rtt.doubleValue * 1000
                }
            }
        }

        let packetLossPercent: Double = {
            let lost = packetsLost ?? 0
            let received = packetsReceived ?? 0
            let total = lost + received
            guard total > 0 else { return 0 }
            return (Double(lost) / Double(total)) * 100
        }()
        let packetsLostDelta = monotonicDelta(current: packetsLost, previous: lastStatsPacketsLost)
        let packetsReceivedDelta = monotonicDelta(current: packetsReceived, previous: lastStatsPacketsReceived)
        let icePath = selectedIcePath(pair: selectedPairValues, candidates: candidateStats)
        let resolution = width.flatMap { w in height.map { "\(w)x\($0)" } } ?? "\(streamProfile.width)x\(streamProfile.height)"
        if (framesDecoded ?? 0) > 0 {
            handleDecodedVideoProgress(framesDecoded: framesDecoded ?? 0)
        }
        let now = ProcessInfo.processInfo.systemUptime
        if handleMediaLiveness(
            framesDecoded: framesDecoded,
            bytesReceived: bytesReceived,
            now: now
        ) {
            return
        }
        if packetLossRecoveryGate.observe(
            lostDelta: packetsLostDelta,
            receivedDelta: packetsReceivedDelta,
            recoveryEligible: mediaTransportConnected
                && transportWasStable
                && iceDisconnectWorkItem == nil
        ) {
            requestKeyframe(reason: "packet_loss_recovered", attempt: 1)
            log("Packet-loss burst cleared; requested one recovery keyframe")
        }
        handlePostStartRenderProgress(
            framesDecoded: framesDecoded,
            framesRendered: framesRendered,
            now: now
        )
        let decodedFPS = estimatedRate(current: framesDecoded, previous: lastStatsFramesDecoded, now: now)
        let receivedFPS = estimatedRate(current: framesReceived, previous: lastStatsFramesReceived, now: now)
        let derivedFPS = framesPerSecond ?? decodedFPS
        let derivedBitrate = estimatedBitrateKbps(bytesReceived: bytesReceived, now: now)
        statsText = [
            selectedCodec.rawValue,
            receivedColorMode,
            resolution,
            "decoded \(framesDecoded ?? 0)",
            decoderImplementation.map { "decoder \($0)" },
            receivedFPS.map { "receivedFps \($0)" },
            decodedFPS.map { "decodedFps \($0)" },
            "rendered \(framesRendered ?? renderedFrameCount)",
            localCodecDebugText.isEmpty ? nil : localCodecDebugText,
            "drop \(framesDropped ?? 0)",
            String(format: "loss %.1f%%", packetLossPercent),
            jitterMs.map { String(format: "jitter %.0fms", $0) },
            rttMs.map { String(format: "rtt %.0fms", $0) },
            "input r\(reliableInputPackets)/p\(partiallyReliableInputPackets)",
            negotiationDebugText,
            icePath
        ].compactMap { $0 }.joined(separator: "  ")
        // Per-frame decode latency for this window only. Cumulative totals divided by cumulative
        // frames would keep reporting the first thirty seconds forever.
        let decodeMs: Double? = {
            guard let total = totalDecodeTimeSeconds, let decoded = framesDecoded else { return nil }
            guard let previousTotal = lastStatsTotalDecodeTime,
                  let previousDecoded = lastStatsFramesDecoded else { return nil }
            let frameDelta = decoded - previousDecoded
            let timeDelta = total - previousTotal
            guard frameDelta > 0, timeDelta >= 0 else { return nil }
            return timeDelta / Double(frameDelta) * 1_000
        }()

        if decoderRecoveryGate.observe(
            receivedFPS: receivedFPS,
            decodedFPS: decodedFPS,
            decodeMs: decodeMs,
            requestedFPS: streamProfile.fps,
            advancedCodecActive: selectedCodec != .h264,
            recoveryEligible: mediaTransportConnected
                && transportWasStable
                && videoSinkAttached
                && iceDisconnectWorkItem == nil
        ) {
            let reason = "The decoder could not sustain \(streamProfile.fps) FPS; retrying the selected profile without changing it"
            log("Sustained decoder overload receivedFps=\(receivedFPS ?? 0) decodedFps=\(decodedFPS ?? 0) decodeMs=\(decodeMs ?? 0)")
            if requestSelectedVideoProfileRetry(reason) { return }
        }

        statsSnapshot = NativeStreamStatsSnapshot(
            codec: selectedCodec.rawValue.uppercased(),
            colorMode: receivedColorMode,
            resolution: resolution,
            fps: derivedFPS,
            bitrateKbps: derivedBitrate,
            pingMs: rttMs.map { Int($0.rounded()) },
            decoded: framesDecoded ?? 0,
            rendered: framesRendered ?? renderedFrameCount,
            dropped: framesDropped ?? 0,
            lossPercent: packetLossPercent,
            jitterMs: jitterMs.map { Int($0.rounded()) },
            decodeMs: decodeMs,
            serverLabel: session.zone.isEmpty ? nil : session.zone,
            gpuLabel: session.gpuType?.trimmingCharacters(in: .whitespacesAndNewlines),
            targetFps: streamProfile.fps,
            inputSummary: "r\(reliableInputPackets)/p\(partiallyReliableInputPackets)",
            detail: statsText
        )

        // A decoded frame is authoritative. CloudMatch can publish an intermediate monitor profile
        // before video arrives, and turning that provisional value into a user-facing notice is
        // how the Android build learned to gate this on real frames.
        statsSnapshot.requestedColor = "\(StreamSettingsResolver.colorQuality(for: settings).label) \(settings.hdrEnabled ? "HDR" : "SDR")"
        statsSnapshot.effects = renderer?.videoEffectsStatus ?? ""
        statsSnapshot.presentationRates = renderer?.presentationRates
        if (framesDecoded ?? 0) > 0 {
            raiseModeChangeNoticeIfNeeded(deliveredResolution: resolution)
        }

        onRuntimeSample(
            StreamRuntimeSample(
                timestamp: now,
                pingMs: rttMs.map { Int($0.rounded()) },
                bitrateKbps: derivedBitrate,
                jitterMs: jitterMs,
                fps: derivedFPS,
                receivedFps: receivedFPS,
                decodedFps: decodedFPS,
                decodeMs: decodeMs,
                packetsLostDelta: packetsLostDelta,
                packetsReceivedDelta: packetsReceivedDelta,
                packetLossPercent: packetLossPercent,
                resolution: resolution,
                codec: selectedCodec.rawValue.uppercased(),
                networkKind: deviceStatus.networkTransport.sessionNetworkKind
            )
        )

        lastStatsSampleAt = now
        lastStatsFramesReceived = framesReceived
        lastStatsFramesDecoded = framesDecoded
        lastStatsFramesRendered = framesRendered
        lastStatsBytesReceived = bytesReceived
        lastStatsTotalDecodeTime = totalDecodeTimeSeconds
        lastStatsPacketsLost = packetsLost
        lastStatsPacketsReceived = packetsReceived
    }

    private func raiseModeChangeNoticeIfNeeded(deliveredResolution: String) {
        guard let notice = StreamModeChangeNotice.between(
            requested: requestedProfile,
            deliveredResolution: deliveredResolution,
            reason: safeVideoRecoveryReason.map { .safeRecovery($0) } ?? .serverNegotiated
        ) else { return }
        // Once per distinct change. A stream that oscillates between two modes should not strobe
        // a banner at the player.
        guard reportedModeChangeKey != notice.id else { return }
        reportedModeChangeKey = notice.id
        modeChangeNotice = notice
        modeNoticeDismissTask?.cancel()
        modeNoticeDismissTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 8 * NSEC_PER_SEC)
            guard !Task.isCancelled else { return }
            self?.modeChangeNotice = nil
        }
    }

    // MARK: - Anti-AFK
    //
    // GFN ends an idle session, which is reasonable for them and infuriating during a cutscene or
    // a long load. The keepalive sends the smallest thing the host will count as activity — a
    // one-pixel mouse nudge and its inverse, so the cursor does not actually travel — and only
    // after a real idle stretch. The indicator is not optional while it runs: an input the player
    // did not make has to be attributable.

    private static let antiAfkIdleThreshold: TimeInterval = 120
    private static let antiAfkInterval: TimeInterval = 30

    func setAntiAfkEnabled(_ enabled: Bool) {
        antiAfkTimer?.invalidate()
        antiAfkTimer = nil
        antiAfkActive = false
        guard enabled else { return }
        antiAfkTimer = Timer.scheduledTimer(withTimeInterval: Self.antiAfkInterval, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.stepAntiAfk() }
        }
    }

    /// Marks the player as active. Called on the paste action, and inferred from packet counters
    /// everywhere else — hooking every input path individually would guarantee one gets missed.
    func noteUserInput() {
        lastUserInputAt = ProcessInfo.processInfo.systemUptime
        lastObservedInputPacketCount = totalInputPacketCount
        if antiAfkActive { antiAfkActive = false }
    }

    private var totalInputPacketCount: Int {
        reliableInputPackets + partiallyReliableInputPackets
    }

    private func stepAntiAfk() {
        guard !stopped, mediaTransportConnected else { return }

        // Any outgoing input at all — touch, controller, keyboard, virtual pad — bumps these
        // counters, so one check covers every path rather than one hook per call site. The
        // keepalive's own nudge is excluded because it is subtracted below.
        let observed = totalInputPacketCount - antiAfkGeneratedPacketCount
        if observed != lastObservedInputPacketCount {
            lastObservedInputPacketCount = observed
            lastUserInputAt = ProcessInfo.processInfo.systemUptime
            if antiAfkActive { antiAfkActive = false }
            return
        }

        let idleFor = ProcessInfo.processInfo.systemUptime - lastUserInputAt
        guard idleFor >= Self.antiAfkIdleThreshold else {
            if antiAfkActive { antiAfkActive = false }
            return
        }
        antiAfkActive = true
        // A nudge and its exact inverse: the host sees movement, the cursor ends where it began.
        inputBridge.sendTouchMouseMove(dx: 1, dy: 0)
        antiAfkGeneratedPacketCount += 1
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) { [weak self] in
            guard let self else { return }
            self.inputBridge.sendTouchMouseMove(dx: -1, dy: 0)
            self.antiAfkGeneratedPacketCount += 1
        }
    }

    /// Pastes the clipboard into the remote session as typed text. Nothing is read from the
    /// pasteboard until the player asks for it — iOS shows its own paste banner when it happens.
    @discardableResult
    func pasteClipboardIntoStream() -> Int {
        #if canImport(UIKit)
        guard liveSettings.clipboardPasteEnabled else { return 0 }
        guard let text = UIPasteboard.general.string, !text.isEmpty else { return 0 }
        noteUserInput()
        return inputBridge.sendUnicodeText(String(text.prefix(2_000)))
        #else
        return 0
        #endif
    }

    func dismissModeChangeNotice() {
        modeNoticeDismissTask?.cancel()
        modeNoticeDismissTask = nil
        modeChangeNotice = nil
    }

    private func handleMediaLiveness(
        framesDecoded: Int?,
        bytesReceived: Int?,
        now: TimeInterval
    ) -> Bool {
        let action = mediaLivenessWatchdog.observe(
            now: now,
            bytesReceived: bytesReceived,
            framesDecoded: framesDecoded,
            connected: mediaTransportConnected
        )
        if recoveryProgressTracker.observe(progressed: mediaLivenessWatchdog.latestObservationProgressed) {
            transportWasStable = true
            log("Media transport stable after three consecutive progress samples")
            onTransportStable()
        }
        switch action {
        case .none:
            return false
        case let .requestKeyframe(stalledFor, attempt):
            requestKeyframe(reason: "media_stall", attempt: attempt)
            updateStatus(
                "Recovering video",
                detail: String(format: "Media paused for %.1fs; requesting a fresh keyframe", stalledFor)
            )
            return false
        case let .restartTransport(stalledFor):
            let reason = String(format: "Media stalled for %.1fs", stalledFor)
            if !requestSelectedVideoProfileRetry(reason) { fail(reason) }
            return true
        }
    }

    private func handlePostStartRenderProgress(
        framesDecoded: Int?,
        framesRendered: Int?,
        now: TimeInterval
    ) {
        // Only arm this watchdog when WebRTC exposes a real framesRendered
        // counter. Sink delivery is not proof that Metal presented the frame.
        guard latestScenePhase == .active,
              let framesDecoded,
              let framesRendered,
              framesRendered > 0 else {
            return
        }

        if lastStatsFramesRendered == nil || framesRendered > (lastStatsFramesRendered ?? 0) {
            lastRenderedStatsProgressAt = now
            lastPostStartKeyframeRequestAt = nil
            postStartKeyframeAttempts = 0
            return
        }

        guard let previousDecoded = lastStatsFramesDecoded,
              framesDecoded > previousDecoded,
              let lastRenderedStatsProgressAt else {
            return
        }

        let stalledFor = now - lastRenderedStatsProgressAt
        let keyframeDue = lastPostStartKeyframeRequestAt.map { now - $0 >= 3 } ?? true
        if stalledFor >= 2, keyframeDue {
            postStartKeyframeAttempts += 1
            lastPostStartKeyframeRequestAt = now
            requestKeyframe(reason: "ios_renderer_stall", attempt: postStartKeyframeAttempts)
            updateStatus(
                "Recovering video",
                detail: "Video rendering paused; requesting a fresh keyframe"
            )
        }

        if stalledFor >= 14 {
            let reason = "Video renderer stalled after playback started"
            if !requestSelectedVideoProfileRetry(reason) { fail(reason) }
        }
    }

    private func estimatedRate(current: Int?, previous: Int?, now: TimeInterval) -> Int? {
        guard let current,
              let previous,
              let lastStatsSampleAt,
              now > lastStatsSampleAt else {
            return nil
        }
        let delta = current - previous
        guard delta >= 0 else { return nil }
        return Int((Double(delta) / (now - lastStatsSampleAt)).rounded())
    }

    private func monotonicDelta(current: Int?, previous: Int?) -> Int? {
        guard let current, let previous, current >= previous else { return nil }
        return current - previous
    }

    private func estimatedBitrateKbps(bytesReceived: Int?, now: TimeInterval) -> Int? {
        guard let bytesReceived,
              let lastStatsBytesReceived,
              let lastStatsSampleAt,
              now > lastStatsSampleAt else {
            return nil
        }
        let delta = bytesReceived - lastStatsBytesReceived
        guard delta >= 0 else { return nil }
        return Int((Double(delta) * 8.0 / (now - lastStatsSampleAt) / 1000.0).rounded())
    }

    private func selectedIcePath(pair: [String: NSObject]?, candidates: [String: [String: NSObject]]) -> String? {
        guard let pair else { return nil }
        let localID = pair["localCandidateId"] as? String
        let remoteID = pair["remoteCandidateId"] as? String
        let local = localID.flatMap { candidates[$0] }.flatMap(candidateEndpoint)
        let remote = remoteID.flatMap { candidates[$0] }.flatMap(candidateEndpoint)
        guard local != nil || remote != nil else { return nil }
        return "ice \(local ?? "?")->\(remote ?? "?")"
    }

    private func candidateEndpoint(_ values: [String: NSObject]) -> String {
        let address = (values["address"] as? String)
            ?? (values["ip"] as? String)
            ?? (values["networkAdapterType"] as? String)
            ?? "?"
        let port = (values["port"] as? NSNumber)?.intValue
            ?? Int(values["port"] as? String ?? "")
        let proto = (values["protocol"] as? String)?.lowercased()
        let type = (values["candidateType"] as? String)?.lowercased()
        return [
            port.map { "\(address):\($0)" } ?? address,
            proto,
            type
        ].compactMap { $0 }.joined(separator: "/")
    }

    private func updateStatus(_ status: String, detail: String = "") {
        statusText = status
        detailText = detail
        showStatusOverlay = true
        logger.info("\(status, privacy: .public) \(detail, privacy: .public)")
    }

    private func markVideoActive() {
        let becameActive = !videoActive
        videoActive = true
        statusText = "Streaming"
        detailText = ""
        withAnimation(.easeOut(duration: 0.25)) {
            showStatusOverlay = false
        }
        if becameActive {
            scheduleGuidancePresentation()
        }
    }

    private func noteRenderedFrame(count: Int, size: CGSize, colorMode: String) {
        guard !stopped, !nativeNVSTFailed else { return }
        receivedColorMode = colorMode
        renderedFrameCount = count
        updateRenderedVideoSize(size)

        decodedWithoutRenderStartedAt = nil
        lastRenderKeyframeRequestAt = nil
        renderKeyframeAttempts = 0
        if !showStatusOverlay || count == 0 { return }
        markVideoActive()
    }

    private func handleDecodedVideoProgress(framesDecoded: Int) {
        guard renderedFrameCount == 0 else {
            markVideoActive()
            return
        }

        let now = ProcessInfo.processInfo.systemUptime
        if decodedWithoutRenderStartedAt == nil {
            decodedWithoutRenderStartedAt = now
            updateStatus("Recovering video", detail: "Decoded \(framesDecoded) frames; waiting for iOS renderer")
            return
        }

        let stalledFor = now - (decodedWithoutRenderStartedAt ?? now)
        if stalledFor >= 14 {
            let reason = "Video decoded but iOS renderer did not paint frames"
            if !requestSelectedVideoProfileRetry(reason) { fail(reason) }
            return
        }

        let keyframeDue = lastRenderKeyframeRequestAt.map { now - $0 >= 3 } ?? true
        if stalledFor >= 2, keyframeDue {
            renderKeyframeAttempts += 1
            lastRenderKeyframeRequestAt = now
            requestKeyframe(reason: "ios_renderer_startup", attempt: renderKeyframeAttempts)
        }

        updateStatus(
            "Recovering video",
            detail: "Decoded \(framesDecoded) frames; waiting for rendered frame \(renderKeyframeAttempts > 0 ? "(keyframe \(renderKeyframeAttempts))" : "")"
        )
    }

    private func requestKeyframe(reason: String, attempt: Int) {
        sendPeerMessage([
            "type": "request_keyframe",
            "reason": reason,
            "backlogFrames": 0,
            "attempt": attempt
        ])
        log("Requested keyframe reason=\(reason) attempt=\(attempt)")
    }

    private func fail(_ message: String) {
        if latestScenePhase != .active, isTransientStartupFailure(message) {
            needsForegroundReconnect = true
            statusText = "Reconnecting"
            detailText = "Connection paused while OpenNOW was in the background"
            showStatusOverlay = true
            logger.warning("Deferring transient stream failure until foreground: \(message, privacy: .public)")
            return
        }
        if scheduleAutoRetryIfNeeded(for: message) {
            return
        }
        statusText = "Stream failed"
        detailText = message
        showStatusOverlay = true
        retryAvailable = false
        logger.error("\(message, privacy: .public)")
    }

    private func scheduleAutoRetryIfNeeded(for message: String) -> Bool {
        guard let onRetry,
              !autoRetryScheduled,
              isTransientStartupFailure(message) else {
            return false
        }

        autoRetryScheduled = true
        retryAvailable = false
        updateStatus("Reconnecting", detail: message)
        logger.warning("Auto-retrying stream startup: \(message, privacy: .public)")

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.85) { [weak self] in
            guard let self, !self.stopped else { return }
            self.stop()
            onRetry()
        }
        return true
    }

    private func isTransientStartupFailure(_ message: String) -> Bool {
        let value = message.lowercased()
        return value.contains("bad response")
            || value.contains("timed out waiting for webrtc offer")
            || value.contains("waiting for offer timed out")
            || value.contains("signaling failed")
            || value.contains("signaling closed")
            || value.contains("http=404")
            || value.contains("404 not found")
            || value.contains("http=410")
            || value.contains("410 gone")
            || value.contains("media stalled")
            || value.contains("video renderer stalled")
            || value.contains("ice connection failed")
            || value.contains("peer connection failed")
    }

    private func log(_ message: String) {
        logger.info("\(message, privacy: .public)")
    }

    private func markMediaTransportConnected() {
        mediaTransportConnected = true
        setAntiAfkEnabled(liveSettings.showAntiAfkIndicator)
        iceDisconnectWorkItem?.cancel()
        iceDisconnectWorkItem = nil
        mediaLivenessWatchdog.markConnected(now: ProcessInfo.processInfo.systemUptime)
    }

    private func markMediaTransportDisconnected() {
        mediaTransportConnected = false
        mediaLivenessWatchdog.reset()
    }

    private func scheduleIceDisconnectRecovery() {
        guard iceDisconnectWorkItem == nil else { return }
        let item = DispatchWorkItem { [weak self] in
            Task { @MainActor in
                guard let self, !self.stopped, !self.mediaTransportConnected else { return }
                self.iceDisconnectWorkItem = nil
                let reason = "ICE connection failed after disconnect"
                if !self.requestSelectedVideoProfileRetry(reason) { self.fail(reason) }
            }
        }
        iceDisconnectWorkItem = item
        workQueue.asyncAfter(
            deadline: .now() + NativeStreamTransportPolicy.iceDisconnectedGrace,
            execute: item
        )
    }

    private func handlePictureInPictureActiveChanged(_ active: Bool) {
        isPictureInPictureActive = active
        if active || latestScenePhase == .active {
            backgroundPictureInPictureStartPending = false
        }
    }

    private func startPictureInPictureForBackground(reason: String) {
        guard !stopped else { return }
        guard !pictureInPictureBridge.isPictureInPictureActive else { return }
        guard !backgroundPictureInPictureStartPending else { return }
        guard pictureInPictureBridge.start() else {
            log("Picture in Picture background start skipped for \(reason); waiting for PiP availability")
            return
        }
        backgroundPictureInPictureStartPending = true
        log("Requested Picture in Picture for \(reason)")
    }

    private func reconnectAfterBackgroundIfNeeded() {
        guard needsForegroundReconnect else { return }
        needsForegroundReconnect = false
        guard let onRetry else { return }
        updateStatus("Reconnecting", detail: "App returned from background")
        logger.warning("Restarting native streamer after background interruption")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
            guard let self, !self.stopped else { return }
            self.stop()
            onRetry()
        }
    }

    private var allowsUnsafeCodecDiagnostics: Bool {
        ProcessInfo.processInfo.arguments.contains("--opennow-allow-unsafe-codecs")
            || ProcessInfo.processInfo.environment["OPENNOW_ALLOW_UNSAFE_CODECS"] == "1"
    }

    private static func effectiveProfile(for session: ActiveSession, settings: AppSettings) -> StreamVideoProfile {
        var profile = StreamSettingsResolver.profile(for: settings)
        if let resolution = session.negotiatedStreamProfile?.resolution,
           let parsed = parseResolution(resolution) {
            profile = StreamVideoProfile(
                width: parsed.width,
                height: parsed.height,
                fps: session.negotiatedStreamProfile?.fps ?? profile.fps,
                maxBitrateKbps: profile.maxBitrateKbps
            )
        }
        return profile
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

    private func setIdleTimerDisabled(_ disabled: Bool) {
        UIApplication.shared.isIdleTimerDisabled = disabled
    }

    private func parseHapticsMessage(_ data: Data) {
        guard let command = NativeStreamHapticsParser.parse(data) else { return }
        inputBridge.applyRumble(
            controllerId: command.controllerId,
            weakMagnitude: command.weakMagnitude,
            strongMagnitude: command.strongMagnitude
        )
    }

    private var nativeAudioDeviceAvailable: Bool {
        #if targetEnvironment(simulator)
        false
        #else
        true
        #endif
    }

    private var shouldPlayAudio: Bool {
        nativeAudioDeviceAvailable && !streamerPreferences.audioMuted
    }

    private func configureStreamAudioPlayback() {
        guard shouldPlayAudio else {
            playbackAudioDevice?.setPlaybackEnabled(false)
            streamAudioPlaybackConfigured = false
            log("WebRTC audio held disabled")
            return
        }
        playbackAudioDevice?.setPlaybackEnabled(true)
        streamAudioPlaybackConfigured = true
        log("WebRTC media playback enabled")
    }

    private func applyLiveAudioPreference() {
        if let nativeNVST {
            let muted = streamerPreferences.audioMuted
            Task { await nativeNVST.setAudioMuted(muted) }
            return
        }
        if mutedAudioDevice != nil {
            log("WebRTC audio held disabled until stream restart")
            return
        }

        guard shouldPlayAudio else {
            audioTrack?.isEnabled = false
            teardownStreamAudioPlayback()
            log("WebRTC audio held disabled")
            return
        }

        configureStreamAudioPlayback()
        audioTrack?.isEnabled = true
    }

    private func startAudioSessionObservation() {
        guard audioSessionObservers.isEmpty else { return }
        let center = NotificationCenter.default
        let names: [Notification.Name] = [
            AVAudioSession.routeChangeNotification,
            AVAudioSession.interruptionNotification,
            AVAudioSession.mediaServicesWereResetNotification
        ]
        audioSessionObservers = names.map { name in
            center.addObserver(forName: name, object: nil, queue: .main) { [weak self] notification in
                Task { @MainActor [weak self] in
                    self?.handleAudioSessionNotification(notification)
                }
            }
        }
    }

    private func stopAudioSessionObservation() {
        let center = NotificationCenter.default
        audioSessionObservers.forEach(center.removeObserver)
        audioSessionObservers.removeAll(keepingCapacity: false)
    }

    private func handleAudioSessionNotification(_ notification: Notification) {
        guard !stopped else { return }
        if notification.name == AVAudioSession.interruptionNotification,
           let rawType = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
           AVAudioSession.InterruptionType(rawValue: rawType) == .began {
            log("Audio session interrupted")
            return
        }

        let reason: String
        switch notification.name {
        case AVAudioSession.routeChangeNotification: reason = "route changed"
        case AVAudioSession.mediaServicesWereResetNotification: reason = "media services reset"
        default: reason = "interruption ended"
        }
        // Setting a category or activating a route can itself emit route notifications. Coalesce
        // those callbacks instead of recursively rebuilding the audio unit on the main actor.
        audioSessionRecoveryTask?.cancel()
        audioSessionRecoveryTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(nanoseconds: 120_000_000)
            } catch {
                return
            }
            guard let self, !self.stopped else { return }
            self.playbackAudioDevice?.recoverAudioOutput()
            self.configureStreamAudioPlayback()
            self.audioTrack?.isEnabled = self.shouldPlayAudio
            self.log("Audio session recovered after \(reason)")
        }
    }

    private func teardownStreamAudioPlayback() {
        guard streamAudioPlaybackConfigured else { return }
        playbackAudioDevice?.setPlaybackEnabled(false)
        streamAudioPlaybackConfigured = false
    }
}

extension NativeStreamCoordinator: NativeStreamInputSink {
    nonisolated func sendReliableInput(_ data: Data) {
        Task { @MainActor in
            if let nativeNVST = self.nativeNVST {
                if await nativeNVST.send(data) { self.reliableInputPackets += 1 }
                return
            }
            guard self.reliableInputChannel?.readyState == .open else { return }
            if self.reliableInputChannel?.sendData(RTCDataBuffer(data: data, isBinary: true)) == true {
                self.reliableInputPackets += 1
            }
        }
    }

    nonisolated func sendPartiallyReliableInput(_ data: Data) {
        Task { @MainActor in
            if let nativeNVST = self.nativeNVST {
                if await nativeNVST.send(data) { self.partiallyReliableInputPackets += 1 }
                return
            }
            if self.partiallyReliableInputChannel?.readyState == .open {
                if self.partiallyReliableInputChannel?.sendData(RTCDataBuffer(data: data, isBinary: true)) == true {
                    self.partiallyReliableInputPackets += 1
                }
            } else if self.reliableInputChannel?.readyState == .open {
                if self.reliableInputChannel?.sendData(RTCDataBuffer(data: data, isBinary: true)) == true {
                    self.reliableInputPackets += 1
                }
            }
        }
    }

    nonisolated func logInputEvent(_ message: String) {
        Task { @MainActor in
            self.log(message)
        }
    }
}

extension NativeStreamCoordinator: URLSessionWebSocketDelegate {
    nonisolated func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didOpenWithProtocol protocol: String?
    ) {
        Task { @MainActor in
            guard self.webSocket === webSocketTask else { return }
            self.updateStatus("Signaling connected", detail: "Waiting for offer")
            self.sendPeerInfo()
            self.startSignalingHeartbeat()
        }
    }

    nonisolated func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didCloseWith closeCode: URLSessionWebSocketTask.CloseCode,
        reason: Data?
    ) {
        Task { @MainActor in
            guard !self.stopped, self.webSocket === webSocketTask else { return }
            let reason = "Signaling closed: \(closeCode.rawValue)"
            self.handleSignalingFailure(reason, task: webSocketTask)
        }
    }
}

extension NativeStreamCoordinator: RTCPeerConnectionDelegate {
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {
        Task { @MainActor in
            guard self.peerConnection === peerConnection else { return }
            if let track = stream.videoTracks.first {
                self.setVideoTrack(track)
            }
            if let track = stream.audioTracks.first {
                self.setAudioTrack(track)
            }
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
    nonisolated func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        Task { @MainActor in
            guard self.peerConnection === peerConnection else { return }
            switch newState {
            case .connected, .completed:
                self.markMediaTransportConnected()
                self.updateStatus("Media connected", detail: "Waiting for video")
            case .failed:
                self.markMediaTransportDisconnected()
                self.iceDisconnectWorkItem?.cancel()
                self.iceDisconnectWorkItem = nil
                if !self.requestSelectedVideoProfileRetry("ICE connection failed") {
                    self.fail("ICE connection failed")
                }
            case .disconnected:
                self.markMediaTransportDisconnected()
                self.updateStatus("Reconnecting", detail: "ICE disconnected")
                self.scheduleIceDisconnectRecovery()
            case .new, .checking, .closed:
                self.markMediaTransportDisconnected()
            default:
                break
            }
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {
        Task { @MainActor in
            guard self.peerConnection === peerConnection else { return }
            self.sendLocalIceCandidate(candidate)
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {
        Task { @MainActor in
            guard self.peerConnection === peerConnection else { return }
            dataChannel.delegate = self
            if dataChannel.label == "control_channel" {
                self.controlChannel = dataChannel
            }
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCPeerConnectionState) {
        Task { @MainActor in
            guard self.peerConnection === peerConnection else { return }
            switch stateChanged {
            case .connected:
                self.updateStatus("Peer connected", detail: "Waiting for video")
            case .failed:
                if !self.requestSelectedVideoProfileRetry("Peer connection failed") {
                    self.fail("Peer connection failed")
                }
            case .disconnected:
                self.updateStatus("Reconnecting", detail: "Peer connection disconnected")
            case .closed:
                break
            default:
                break
            }
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didAdd receiver: RTCRtpReceiver, streams mediaStreams: [RTCMediaStream]) {
        Task { @MainActor in
            guard self.peerConnection === peerConnection else { return }
            if let track = receiver.track as? RTCVideoTrack {
                self.setVideoTrack(track)
            } else if let track = receiver.track as? RTCAudioTrack {
                self.setAudioTrack(track)
            }
        }
    }
    nonisolated func peerConnection(
        _ peerConnection: RTCPeerConnection,
        didStartReceivingOn transceiver: RTCRtpTransceiver
    ) {
        Task { @MainActor in
            guard self.peerConnection === peerConnection else { return }
            if let track = transceiver.receiver.track as? RTCVideoTrack {
                self.setVideoTrack(track)
            } else if let track = transceiver.receiver.track as? RTCAudioTrack {
                self.setAudioTrack(track)
            }
        }
    }
}

extension NativeStreamCoordinator: RTCVideoViewDelegate {
    nonisolated func videoView(_ videoView: any RTCVideoRenderer, didChangeVideoSize size: CGSize) {
        Task { @MainActor in
            self.updateRenderedVideoSize(size)
            self.log("Native video size changed \(Int(size.width))x\(Int(size.height))")
        }
    }
}

extension NativeStreamCoordinator: RTCDataChannelDelegate {
    nonisolated func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {
        Task { @MainActor in
            if dataChannel.readyState == .open {
                if dataChannel === self.reliableInputChannel {
                    self.inputBridge.primeReliableChannel()
                    self.log("Reliable input channel open")
                    // One line per session recording the catalog signal and the decision it
                    // produced, so a "touch did not work" report can be answered without guessing.
                    self.log(
                        NativeTouchSupport.diagnostics(
                            game: self.session.game,
                            enabled: self.nativeTouchActive,
                            provisioned: self.session.touchProvisioned ?? true
                        )
                    )
                } else if dataChannel === self.partiallyReliableInputChannel {
                    self.log("Partially reliable input channel open")
                } else if dataChannel.label == "control_channel" {
                    self.log("Control channel open")
                }
            }
        }
    }

    nonisolated func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        Task { @MainActor in
            if buffer.isBinary {
                if dataChannel === self.reliableInputChannel,
                   let version = self.inputBridge.handleServerHandshake(buffer.data) {
                    self.log("Input handshake applied protocol=\(version) bytes=\(buffer.data.count)")
                    return
                }
                self.parseHapticsMessage(buffer.data)
            } else if let text = String(data: buffer.data, encoding: .utf8),
                      text.contains("request_keyframe") {
                self.log("Control channel requested keyframe")
            }
        }
    }
}

private final class NativeStreamPictureInPictureBridge: NSObject {
    var onAvailabilityChanged: ((Bool) -> Void)?
    var onActiveChanged: ((Bool) -> Void)?
    var onLog: ((String) -> Void)?

    private let frameQueue = DispatchQueue(label: "OpenNOW.NativeStreamer.PiPFrames")
    private weak var displayLayer: AVSampleBufferDisplayLayer?
    private var contentSource: AVPictureInPictureController.ContentSource?
    private var pictureInPictureController: AVPictureInPictureController?
    private var possibilityObservation: NSKeyValueObservation?
    private let frameInFlight = DispatchSemaphore(value: 1)
    private let converter = NativeStreamPiPFrameConverter()
    private let generationLock = NSLock()
    private var generation: UInt64 = 0
    private var availabilityPublished = false
    private var playbackPaused = false

    private var hasFrameForPlayback = false
    private var activeForFrameQueue = false
    private var lastAcceptedFrameAt: TimeInterval = 0
    private var cachedFormatDescription: CMVideoFormatDescription?
    private var cachedPixelFormat: OSType = 0
    private var cachedWidth = 0
    private var cachedHeight = 0

    private static let activeFrameInterval: TimeInterval = 1.0 / 30.0
    private static let inactiveFrameInterval: TimeInterval = 0.5

    var isPictureInPictureActive: Bool {
        pictureInPictureController?.isPictureInPictureActive == true
    }

    func attach(displayLayer: AVSampleBufferDisplayLayer) {
        generationLock.lock(); generation &+= 1; generationLock.unlock()
        self.displayLayer = displayLayer
        hasFrameForPlayback = false
        playbackPaused = false
        displayLayer.videoGravity = .resizeAspect
        displayLayer.backgroundColor = UIColor.black.cgColor
        displayLayer.flush()
        var timebase: CMTimebase?
        if CMTimebaseCreateWithSourceClock(allocator: kCFAllocatorDefault,
            sourceClock: CMClockGetHostTimeClock(), timebaseOut: &timebase) == noErr, let timebase {
            CMTimebaseSetTime(timebase, time: CMClockGetTime(CMClockGetHostTimeClock()))
            CMTimebaseSetRate(timebase, rate: 1)
            displayLayer.controlTimebase = timebase
        }
        frameQueue.async { [weak self] in self?.resetFrameState() }

        guard AVPictureInPictureController.isPictureInPictureSupported() else {
            publishAvailability(false)
            onLog?("Picture in Picture is not supported on this device")
            return
        }

        let contentSource = AVPictureInPictureController.ContentSource(
            sampleBufferDisplayLayer: displayLayer,
            playbackDelegate: self
        )
        let controller = AVPictureInPictureController(contentSource: contentSource)
        controller.delegate = self
        controller.requiresLinearPlayback = true
        controller.canStartPictureInPictureAutomaticallyFromInline = true
        self.contentSource = contentSource
        self.pictureInPictureController = controller
        possibilityObservation = controller.observe(\.isPictureInPicturePossible, options: [.new]) { [weak self] _, _ in
            DispatchQueue.main.async { self?.updateAvailabilityFromController() }
        }
        publishAvailability(false)
    }

    @discardableResult
    func start() -> Bool {
        guard let controller = pictureInPictureController,
              AVPictureInPictureController.isPictureInPictureSupported() else {
            return false
        }
        if controller.isPictureInPictureActive {
            return true
        }
        guard controller.isPictureInPicturePossible else {
            return false
        }
        controller.startPictureInPicture()
        return true
    }

    func stop() {
        guard pictureInPictureController?.isPictureInPictureActive == true else { return }
        pictureInPictureController?.stopPictureInPicture()
    }

    func detach() {
        generationLock.lock(); generation &+= 1; generationLock.unlock()
        possibilityObservation = nil
        stop()
        publishActive(false)
        publishAvailability(false)
        hasFrameForPlayback = false
        playbackPaused = false
        pictureInPictureController?.delegate = nil
        pictureInPictureController = nil
        contentSource = nil
        displayLayer?.flush()
        displayLayer = nil
        frameQueue.async { [weak self] in
            self?.resetFrameState()
        }
    }

    func enqueue(frame: RTCVideoFrame) {
        guard let cvBuffer = frame.buffer as? RTCCVPixelBuffer else { return }
        guard frameInFlight.wait(timeout: .now()) == .success else { return }
        let pixelBuffer = cvBuffer.pixelBuffer
        generationLock.lock(); let frameGeneration = generation; generationLock.unlock()
        frameQueue.async { [self] in
            enqueue(pixelBuffer: pixelBuffer, generation: frameGeneration)
        }
    }

    private func enqueue(pixelBuffer: CVPixelBuffer, generation frameGeneration: UInt64) {
        guard shouldAcceptFrame(), let converted = converter.convert(pixelBuffer),
              let sampleBuffer = makeSampleBuffer(pixelBuffer: converted) else {
            frameInFlight.signal()
            return
        }
        DispatchQueue.main.async { [self] in
            defer { frameInFlight.signal() }
            generationLock.lock(); let currentGeneration = generation; generationLock.unlock()
            guard frameGeneration == currentGeneration, let displayLayer = displayLayer else { return }
            if displayLayer.status == .failed || displayLayer.requiresFlushToResumeDecoding {
                displayLayer.flush()
            }
            guard displayLayer.isReadyForMoreMediaData else { return }
            displayLayer.enqueue(sampleBuffer)
            self.hasFrameForPlayback = true
            self.updateAvailabilityFromController()
            self.pictureInPictureController?.invalidatePlaybackState()
        }
    }

    private func shouldAcceptFrame() -> Bool {
        let now = ProcessInfo.processInfo.systemUptime
        let interval = activeForFrameQueue ? Self.activeFrameInterval : Self.inactiveFrameInterval
        guard lastAcceptedFrameAt == 0 || now - lastAcceptedFrameAt >= interval else {
            return false
        }
        lastAcceptedFrameAt = now
        return true
    }

    private func makeSampleBuffer(pixelBuffer: CVPixelBuffer) -> CMSampleBuffer? {
        guard let formatDescription = formatDescription(for: pixelBuffer) else { return nil }
        var timing = NativeStreamPiPSampleTiming.make()
        var sampleBuffer: CMSampleBuffer?
        let status = CMSampleBufferCreateReadyWithImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            formatDescription: formatDescription,
            sampleTiming: &timing,
            sampleBufferOut: &sampleBuffer
        )
        guard status == noErr, let sampleBuffer else { return nil }
        markDisplayImmediately(sampleBuffer)
        return sampleBuffer
    }

    private func formatDescription(for pixelBuffer: CVPixelBuffer) -> CMVideoFormatDescription? {
        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let pixelFormat = CVPixelBufferGetPixelFormatType(pixelBuffer)
        if let cachedFormatDescription,
           cachedWidth == width,
           cachedHeight == height,
           cachedPixelFormat == pixelFormat {
            return cachedFormatDescription
        }

        var formatDescription: CMVideoFormatDescription?
        let status = CMVideoFormatDescriptionCreateForImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            formatDescriptionOut: &formatDescription
        )
        guard status == noErr, let formatDescription else { return nil }
        cachedFormatDescription = formatDescription
        cachedWidth = width
        cachedHeight = height
        cachedPixelFormat = pixelFormat
        return formatDescription
    }

    private func markDisplayImmediately(_ sampleBuffer: CMSampleBuffer) {
        guard let attachments = CMSampleBufferGetSampleAttachmentsArray(
            sampleBuffer,
            createIfNecessary: true
        ), CFArrayGetCount(attachments) > 0 else {
            return
        }
        let rawDictionary = CFArrayGetValueAtIndex(attachments, 0)
        let attachmentsDictionary = unsafeBitCast(rawDictionary, to: CFMutableDictionary.self)
        CFDictionarySetValue(
            attachmentsDictionary,
            Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
            Unmanaged.passUnretained(kCFBooleanTrue).toOpaque()
        )
    }

    private func updateAvailabilityFromController() {
        publishAvailability(pictureInPictureController?.isPictureInPicturePossible == true)
    }

    private func publishAvailability(_ available: Bool) {
        guard availabilityPublished != available else { return }
        availabilityPublished = available
        onAvailabilityChanged?(available)
    }

    private func publishActive(_ active: Bool) {
        playbackPaused = false
        frameQueue.async { [weak self] in
            self?.activeForFrameQueue = active
            if active {
                self?.lastAcceptedFrameAt = 0
            }
        }
        onActiveChanged?(active)
        pictureInPictureController?.invalidatePlaybackState()
    }

    private func resetFrameState() {
        activeForFrameQueue = false
        lastAcceptedFrameAt = 0
        cachedFormatDescription = nil
        cachedPixelFormat = 0
        cachedWidth = 0
        cachedHeight = 0
    }
}

extension NativeStreamPictureInPictureBridge: AVPictureInPictureControllerDelegate {
    func pictureInPictureControllerDidStartPictureInPicture(_ pictureInPictureController: AVPictureInPictureController) {
        publishActive(true)
        onLog?("Picture in Picture started")
    }

    func pictureInPictureControllerDidStopPictureInPicture(_ pictureInPictureController: AVPictureInPictureController) {
        publishActive(false)
        onLog?("Picture in Picture stopped")
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        failedToStartPictureInPictureWithError error: Error
    ) {
        publishActive(false)
        onLog?("Picture in Picture failed: \(error.localizedDescription)")
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        restoreUserInterfaceForPictureInPictureStopWithCompletionHandler completionHandler: @escaping (Bool) -> Void
    ) {
        completionHandler(true)
    }
}

extension NativeStreamPictureInPictureBridge: AVPictureInPictureSampleBufferPlaybackDelegate {
    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        setPlaying playing: Bool
    ) {
        playbackPaused = false
        pictureInPictureController.invalidatePlaybackState()
    }

    func pictureInPictureControllerTimeRangeForPlayback(
        _ pictureInPictureController: AVPictureInPictureController
    ) -> CMTimeRange {
        hasFrameForPlayback
            ? CMTimeRange(start: .zero, duration: .positiveInfinity)
            : .invalid
    }

    func pictureInPictureControllerIsPlaybackPaused(
        _ pictureInPictureController: AVPictureInPictureController
    ) -> Bool {
        playbackPaused
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        didTransitionToRenderSize newRenderSize: CMVideoDimensions
    ) {
        onLog?("Picture in Picture render size \(newRenderSize.width)x\(newRenderSize.height)")
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        skipByInterval skipInterval: CMTime,
        completion completionHandler: @escaping () -> Void
    ) {
        completionHandler()
    }

    func pictureInPictureControllerShouldProhibitBackgroundAudioPlayback(
        _ pictureInPictureController: AVPictureInPictureController
    ) -> Bool {
        false
    }
}

private final class NativeStreamVideoSink: NSObject, RTCVideoRenderer {
    var onFrame: ((Int, CGSize, String) -> Void)?
    var onPictureInPictureFrame: ((RTCVideoFrame) -> Void)?

    private weak var renderView: NativeStreamRenderView?
    private let lock = NSLock()
    private var frameCount = 0
    private var lastSize: CGSize = .zero

    func attach(renderView: NativeStreamRenderView) {
        lock.lock()
        self.renderView = renderView
        frameCount = 0
        lastSize = .zero
        lock.unlock()
    }

    func detach() {
        lock.lock()
        renderView = nil
        frameCount = 0
        lastSize = .zero
        lock.unlock()
    }

    func setSize(_ size: CGSize) {
        lock.lock()
        lastSize = size
        let target = renderView
        lock.unlock()
        target?.setSize(size)
    }

    func renderFrame(_ frame: RTCVideoFrame?) {
        lock.lock()
        frameCount += 1
        let count = frameCount
        let size = CGSize(
            width: frame.map { CGFloat($0.width) } ?? lastSize.width,
            height: frame.map { CGFloat($0.height) } ?? lastSize.height
        )
        let target = renderView
        lock.unlock()

        target?.renderFrame(frame)
        if let frame {
            onPictureInPictureFrame?(frame)
        }
        if count <= 3 || count.isMultiple(of: 60) {
            let colorMode = (frame?.buffer as? RTCCVPixelBuffer).map { NativeStreamHDRTransfer.colorMode(in: $0.pixelBuffer) } ?? (frame == nil ? "--" : "8-bit SDR")
            onFrame?(count, size, colorMode)
        }
    }

}

private struct NativeStreamVideoView: UIViewRepresentable, Equatable {
    let coordinator: NativeStreamCoordinator

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.coordinator === rhs.coordinator }

    func makeUIView(context: Context) -> NativeStreamRenderView {
        let view = NativeStreamRenderView(frame: .zero)
        coordinator.attachRenderer(view)
        return view
    }

    func updateUIView(_ uiView: NativeStreamRenderView, context: Context) {
        coordinator.attachRenderer(uiView)
    }
}

private final class NativeStreamRenderView: UIView {
    let pictureInPictureDisplayLayer = AVSampleBufferDisplayLayer()

    private let videoContainerView = UIView(frame: .zero)
    private let metalVideoView = RTCMTLVideoView(frame: .zero)
    private let rendererStateLock = NSLock()
    private var filteredMetalView: NativeStreamFilteredMetalView?
    private var filteredRendererActive = false
    private var stretchStreamToFill = false
    private var videoCodec: NativeStreamVideoCodec = .h264
    private var streamSharpeningEnabled = false
    private var streamSharpeningAmount = 0.25
    private var metal4Enabled = false
    private var upscalingEnabled = false
    var videoEffectsStatus: String { filteredMetalView?.videoEffectsStatus ?? "" }
    var presentationRates: NativeStreamPresentationRates? {
        filteredRendererActive ? filteredMetalView?.presentationRates : nil
    }

    func setVideoCodec(_ codec: NativeStreamVideoCodec) {
        rendererStateLock.lock()
        videoCodec = codec
        rendererStateLock.unlock()
        if shouldRequestFilteredRenderer { ensureFilteredMetalView() }
        updateRendererVisibility()
    }

    func setVideoEffects(upscaling: Bool, metal4: Bool) {
        metal4Enabled = metal4
        upscalingEnabled = upscaling
        if upscaling { ensureFilteredMetalView() }
        filteredMetalView?.setVideoEffects(upscaling: upscaling, metal4: metal4)
        updateRendererVisibility()
    }
    private var viewportTransformScale: CGFloat = 1
    private var viewportTransformOffset: CGSize = .zero
    private var filteredRendererCreationScheduled = false

    var metalDelegate: RTCVideoViewDelegate? {
        get { metalVideoView.delegate }
        set { metalVideoView.delegate = newValue }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .black
        isOpaque = true
        clipsToBounds = true
        pictureInPictureDisplayLayer.videoGravity = .resizeAspect
        pictureInPictureDisplayLayer.backgroundColor = UIColor.black.cgColor
        layer.addSublayer(pictureInPictureDisplayLayer)
        videoContainerView.backgroundColor = .black
        videoContainerView.isUserInteractionEnabled = false
        addSubview(videoContainerView)
        metalVideoView.videoContentMode = .scaleAspectFit
        metalVideoView.backgroundColor = .black
        metalVideoView.isEnabled = true
        videoContainerView.addSubview(metalVideoView)
        #if targetEnvironment(simulator)
        ensureFilteredMetalView()
        #endif
        updateRendererVisibility()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        pictureInPictureDisplayLayer.frame = bounds
        videoContainerView.bounds = CGRect(origin: .zero, size: bounds.size)
        videoContainerView.center = CGPoint(x: bounds.midX, y: bounds.midY)
        metalVideoView.frame = videoContainerView.bounds
        filteredMetalView?.frame = videoContainerView.bounds
        applyViewportTransform()
    }

    func setSize(_ size: CGSize) {
        metalVideoView.setSize(size)
    }

    func renderFrame(_ frame: RTCVideoFrame?) {
        rendererStateLock.lock()
        let useFilteredRenderer = filteredRendererActive
        let filtered = filteredMetalView
        let shouldRetryFilteredRenderer = shouldRequestFilteredRenderer
            && filtered == nil
            && !filteredRendererCreationScheduled
        if shouldRetryFilteredRenderer {
            filteredRendererCreationScheduled = true
        }
        rendererStateLock.unlock()

        if useFilteredRenderer, let filtered {
            filtered.display(frame: frame)
        } else {
            metalVideoView.renderFrame(frame)
        }
        if shouldRetryFilteredRenderer {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.ensureFilteredMetalView()
                self.rendererStateLock.lock()
                let rendererCreated = self.filteredMetalView != nil
                self.rendererStateLock.unlock()
                self.updateRendererVisibility()
                if rendererCreated {
                    self.rendererStateLock.lock()
                    self.filteredRendererCreationScheduled = false
                    self.rendererStateLock.unlock()
                } else {
                    // Avoid retrying once per decoded frame if Metal is briefly unavailable.
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
                        guard let self else { return }
                        self.rendererStateLock.lock()
                        self.filteredRendererCreationScheduled = false
                        self.rendererStateLock.unlock()
                    }
                }
            }
        }
    }

    func setStretchStreamToFill(_ enabled: Bool) {
        stretchStreamToFill = enabled
        metalVideoView.videoContentMode = enabled ? .scaleToFill : .scaleAspectFit
        rendererStateLock.lock()
        let filtered = filteredMetalView
        rendererStateLock.unlock()
        filtered?.stretchToFill = enabled
    }

    func setStreamSharpening(enabled: Bool, amount: Double) {
        streamSharpeningEnabled = enabled
        streamSharpeningAmount = min(max(amount, 0), 1)
        if shouldRequestFilteredRenderer {
            ensureFilteredMetalView()
        }
        filteredMetalView?.sharpeningAmount = enabled ? streamSharpeningAmount : 0
        updateRendererVisibility()
    }

    func setViewportTransform(scale: CGFloat, offset: CGSize) {
        viewportTransformScale = min(max(scale, 1), 3)
        viewportTransformOffset = viewportTransformScale <= 1.001 ? .zero : offset
        applyViewportTransform()
    }

    private func applyViewportTransform() {
        videoContainerView.transform = CGAffineTransform(
            a: viewportTransformScale,
            b: 0,
            c: 0,
            d: viewportTransformScale,
            tx: viewportTransformOffset.width,
            ty: viewportTransformOffset.height
        )
    }

    private var shouldRequestFilteredRenderer: Bool {
        upscalingEnabled || nativeStreamShouldUseFilteredRenderer(
            osMajorVersion: ProcessInfo.processInfo.operatingSystemVersion.majorVersion,
            videoCodec: videoCodec,
            streamSharpeningEnabled: streamSharpeningEnabled,
            isSimulator: {
                #if targetEnvironment(simulator)
                true
                #else
                false
                #endif
            }()
        )
    }

    private func ensureFilteredMetalView() {
        guard filteredMetalView == nil,
              let filtered = NativeStreamFilteredMetalView.make() else {
            return
        }
        filtered.frame = videoContainerView.bounds
        filtered.stretchToFill = stretchStreamToFill
        filtered.sharpeningAmount = streamSharpeningEnabled ? streamSharpeningAmount : 0
        filtered.setVideoEffects(upscaling: upscalingEnabled, metal4: metal4Enabled)
        filtered.isHidden = true
        videoContainerView.addSubview(filtered)
        rendererStateLock.lock()
        filteredMetalView = filtered
        rendererStateLock.unlock()
    }

    private func updateRendererVisibility() {
        let filteredActive = shouldRequestFilteredRenderer && filteredMetalView != nil
        rendererStateLock.lock()
        filteredRendererActive = filteredActive
        rendererStateLock.unlock()
        metalVideoView.isEnabled = !filteredActive
        metalVideoView.isHidden = filteredActive
        filteredMetalView?.isHidden = !filteredActive
    }
}

/// Core Image + Metal presents HEVC frames and provides the fallback for CoreSimulator,
/// iOS 26+ runtimes, and optional effects on older devices.
/// Bridge software-decoded WebRTC I420 frames to the NV12 IOSurface used by Core Image.
final class NativeStreamFramePixelBufferBridge {
    private let lock = NSLock()
    private var pool: CVPixelBufferPool?
    private var poolSize = CGSize.zero

    func pixelBuffer(for frame: RTCVideoFrame) -> CVPixelBuffer? {
        if let native = frame.buffer as? RTCCVPixelBuffer { return native.pixelBuffer }
        let i420 = frame.buffer.toI420()
        let width = Int(i420.width), height = Int(i420.height)
        guard width > 0, height > 0 else { return nil }
        lock.lock()
        defer { lock.unlock() }
        if pool == nil || poolSize != CGSize(width: width, height: height) {
            let attributes: [CFString: Any] = [
                kCVPixelBufferWidthKey: width, kCVPixelBufferHeightKey: height,
                kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                kCVPixelBufferMetalCompatibilityKey: true,
                kCVPixelBufferIOSurfacePropertiesKey: [:]
            ]
            var nextPool: CVPixelBufferPool?
            guard CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &nextPool) == kCVReturnSuccess else { return nil }
            pool = nextPool
            poolSize = CGSize(width: width, height: height)
        }
        guard let pool else { return nil }
        var buffer: CVPixelBuffer?
        // Bound buffers still owned by pending GPU work; drop a frame instead of growing indefinitely.
        let limits = [kCVPixelBufferPoolAllocationThresholdKey: 6] as CFDictionary
        guard CVPixelBufferPoolCreatePixelBufferWithAuxAttributes(nil, pool, limits, &buffer) == kCVReturnSuccess,
              let buffer, CVPixelBufferLockBaseAddress(buffer, []) == kCVReturnSuccess else { return nil }
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let y = CVPixelBufferGetBaseAddressOfPlane(buffer, 0),
              let uv = CVPixelBufferGetBaseAddressOfPlane(buffer, 1) else { return nil }
        let yStride = CVPixelBufferGetBytesPerRowOfPlane(buffer, 0)
        for row in 0..<height {
            memcpy(y.advanced(by: row * yStride), i420.dataY.advanced(by: row * Int(i420.strideY)), width)
        }
        let chromaWidth = Int(i420.chromaWidth), chromaHeight = Int(i420.chromaHeight)
        var u = vImage_Buffer(data: UnsafeMutableRawPointer(mutating: i420.dataU), height: vImagePixelCount(chromaHeight), width: vImagePixelCount(chromaWidth), rowBytes: Int(i420.strideU))
        var v = vImage_Buffer(data: UnsafeMutableRawPointer(mutating: i420.dataV), height: vImagePixelCount(chromaHeight), width: vImagePixelCount(chromaWidth), rowBytes: Int(i420.strideV))
        let status = withUnsafePointer(to: &u) { uPointer in
            withUnsafePointer(to: &v) { vPointer in
                var sources: [UnsafePointer<vImage_Buffer>?] = [uPointer, vPointer]
                var channels: [UnsafeMutableRawPointer?] = [uv, uv.advanced(by: 1)]
                return sources.withUnsafeMutableBufferPointer { sourcePointers in
                    channels.withUnsafeMutableBufferPointer { destinationPointers in
                        vImageConvert_PlanarToChunky8(sourcePointers.baseAddress!, destinationPointers.baseAddress!, 2, 2,
                            vImagePixelCount(chromaWidth), vImagePixelCount(chromaHeight),
                            CVPixelBufferGetBytesPerRowOfPlane(buffer, 1), vImage_Flags(kvImageNoFlags))
                    }
                }
            }
        }
        guard status == kvImageNoError else { return nil }
        CVBufferSetAttachment(buffer, kCVImageBufferYCbCrMatrixKey, kCVImageBufferYCbCrMatrix_ITU_R_709_2, .shouldPropagate)
        return buffer
    }
}

/// A bounded renderer mailbox: producers overwrite pending work while the GPU is busy.
/// Conversion is deferred until a display tick can submit the newest frame.
/// All mailbox state is lock-protected; producers must not mutate offered frames.
final class NativeStreamLatestFrameMailbox<Frame>: @unchecked Sendable {
    struct Entry {
        let frame: Frame
    }

    private let lock = NSLock()
    private let maximumInFlight: Int
    private var pending: Entry?
    private var inFlight = 0

    init(maximumInFlight: Int = 2) {
        precondition(maximumInFlight > 0)
        self.maximumInFlight = maximumInFlight
    }

    func offer(_ frame: Frame) {
        lock.lock()
        pending = Entry(frame: frame)
        lock.unlock()
    }

    func take() -> Entry? {
        lock.lock()
        defer { lock.unlock() }
        guard inFlight < maximumInFlight, let entry = pending else { return nil }
        pending = nil
        inFlight += 1
        return entry
    }

    func complete() {
        lock.lock()
        precondition(inFlight > 0)
        inFlight -= 1
        lock.unlock()
    }
}

enum NativeStreamHDRTransfer {
    case sdr, pq, hlg

    static func colorMode(in buffer: CVPixelBuffer) -> String {
        let format = CVPixelBufferGetPixelFormatType(buffer)
        let chroma = NativeStreamTenBitSurface.chroma(format)
        let depth = chroma.map { $0 == "4:2:0" ? "10-bit" : "10-bit \($0)" } ?? "8-bit"
        switch detect(in: buffer) {
        case .pq: return "\(depth) HDR PQ"
        case .hlg: return "\(depth) HDR HLG"
        case .sdr: return "\(depth) SDR"
        }
    }

    static func detect(in buffer: CVPixelBuffer) -> Self {
        let transfer = CVBufferCopyAttachment(buffer, kCVImageBufferTransferFunctionKey, nil) as? String
        if transfer == kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ as String { return .pq }
        if transfer == kCVImageBufferTransferFunction_ITU_R_2100_HLG as String { return .hlg }
        return .sdr
    }
}

private final class NativeStreamFilteredMetalView: UIView, MTKViewDelegate {
    var stretchToFill = false
    var sharpeningAmount = 0.0

    private let commandQueue: MTLCommandQueue
    private let ciContext: CIContext
    private let directHDR: NativeStreamHDRMetalRenderer?
    private var metal4HDRStorage: AnyObject?
    private var metal4EffectsStorage: AnyObject?
    private var metal4Disabled = false
    private let submissionTimeline: NativeStreamMetalFrameTimeline?
    private var rendererBackend = "Metal / Core Image"
    private var metal4UpscalingStatus: String?
    private let spatialUpscaler: NativeStreamSpatialUpscaler
    private var metal4Enabled = false
    private var upscalingEnabled = false
    private var suspendUpscalingUntil: CFTimeInterval = 0
    private var effectsGeneration: UInt64 = 0
    var presentationRates: NativeStreamPresentationRates? {
        presentations.rates(now:CACurrentMediaTime())
    }

    var videoEffectsStatus: String {
        var parts: [String] = ["Renderer: " + rendererBackend]
        if upscalingEnabled {
            parts.append("MetalFX: " + (CACurrentMediaTime() < suspendUpscalingUntil
                ? "Paused: processing error" : (metal4UpscalingStatus ?? spatialUpscaler.status)))
        }
        return parts.joined(separator: " · ")
    }

    func setVideoEffects(upscaling: Bool, metal4: Bool) {
        metal4Enabled = metal4
        if metal4 { prepareMetal4IfNeeded() }
        guard upscalingEnabled != upscaling else { return }
        if !upscaling { spatialUpscaler.reset() }
        upscalingEnabled = upscaling
        effectsGeneration &+= 1
    }
    private let sharpeningFilter = CIFilter(name: "CISharpenLuminance")
    private var colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    private var hdrTransfer = NativeStreamHDRTransfer.sdr
    private let mtkView: MTKView
    private let frameBridge = NativeStreamFramePixelBufferBridge()
    private let frames = NativeStreamLatestFrameMailbox<RTCVideoFrame>()
    private let gpuAdmission = DispatchSemaphore(value: 2)
    private let presentations = NativeStreamPresentationTracker()
    private var displayLink: CADisplayLink?
    private var displayTargetTimestamp: CFTimeInterval?
    private lazy var displayClock = DisplayClock(owner: self)

    private final class DisplayClock: NSObject {
        weak var owner: NativeStreamFilteredMetalView?
        init(owner: NativeStreamFilteredMetalView) { self.owner = owner }
        @objc func tick(_ link: CADisplayLink) { owner?.displayTick(link) }
    }

    deinit { displayLink?.invalidate() }

    static func make() -> NativeStreamFilteredMetalView? {
        guard let device = MTLCreateSystemDefaultDevice(),
              let queue = device.makeCommandQueue() else {
            return nil
        }
        return NativeStreamFilteredMetalView(device: device, commandQueue: queue)
    }

    private init(device: MTLDevice, commandQueue: MTLCommandQueue) {
        self.commandQueue = commandQueue
        directHDR = NativeStreamHDRMetalRenderer(device: device)
        if #available(iOS 26.0, *), NativeStreamMetal4HDRRenderer.isSupported(device:device) {
            submissionTimeline = NativeStreamMetalFrameTimeline(device:device)
        } else { submissionTimeline = nil }
        spatialUpscaler = NativeStreamSpatialUpscaler(device: device)
        ciContext = CIContext(mtlDevice: device, options: [.cacheIntermediates: false])
        mtkView = MTKView(frame: .zero, device: device)
        super.init(frame: .zero)
        isOpaque = true
        backgroundColor = .black
        mtkView.framebufferOnly = false
        mtkView.isPaused = true
        mtkView.enableSetNeedsDisplay = false
        mtkView.clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        mtkView.backgroundColor = .black
        mtkView.delegate = self
        addSubview(mtkView)
    }

    private var metal4PreparationStarted = false
    private func prepareMetal4IfNeeded() {
        guard !metal4PreparationStarted, let device = mtkView.device else { return }
        metal4PreparationStarted = true
        if #available(iOS 26.0, *), submissionTimeline != nil {
            DispatchQueue.global(qos:.userInitiated).async { [weak self] in
                let renderer = NativeStreamMetal4HDRRenderer(device:device)
                let effects = NativeStreamMetal4EffectsRenderer(device:device)
                DispatchQueue.main.async { [weak self] in
                    guard let self else { return }
                    if let layer = self.mtkView.layer as? CAMetalLayer {
                        renderer?.setDrawableResidency(layer.residencySet)
                        effects?.setDrawableResidency(layer.residencySet)
                    }
                    self.metal4HDRStorage = renderer; self.metal4EffectsStorage = effects
                }
            }
        }
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        mtkView.frame = bounds
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        displayLink?.invalidate()
        displayLink = nil
        // Use one explicit display clock. MTKView's own timer remains paused.
        // A fixed range asks ProMotion to keep video cadence instead of choosing
        // an intermediate refresh rate during otherwise static game scenes.
        mtkView.isPaused = true
        guard let window else { return }
        let refresh = Float(window.screen.maximumFramesPerSecond)
        let link = CADisplayLink(target: displayClock, selector: #selector(DisplayClock.tick(_:)))
        link.preferredFrameRateRange = CAFrameRateRange(minimum: refresh, maximum: refresh, preferred: refresh)
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    private func displayTick(_ link: CADisplayLink) {
        displayTargetTimestamp = link.targetTimestamp
        mtkView.draw()
    }

    func display(frame: RTCVideoFrame?) {
        guard let frame else { return }
        frames.offer(frame)
    }

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

    func draw(in view: MTKView) {
        guard !isHidden, window != nil else { return }
        let drawStarted = CACurrentMediaTime()
        let presentAt = displayTargetTimestamp
        displayTargetTimestamp = nil
        guard gpuAdmission.wait(timeout: .now()) == .success else { return }
        var submitted = false
        defer { if !submitted { gpuAdmission.signal() } }
        guard let entry = frames.take() else { return }
        defer {
            // Return the slot on conversion/drawable failures too.
            if !submitted { frames.complete() }
        }
        guard let pixelBuffer = frameBridge.pixelBuffer(for: entry.frame) else { return }
        configureColorOutput(for: pixelBuffer)
        guard let drawable = view.currentDrawable,
              let commandBuffer = commandQueue.makeCommandBuffer() else {
            return
        }
        let frameSize = CGSize(width: CGFloat(entry.frame.width), height: CGFloat(entry.frame.height))

        let bounds = CGRect(origin: .zero, size: view.drawableSize)
        let destination = stretchToFill ? bounds : Self.aspectFitRect(source: frameSize, target: bounds.size)
        let shouldUpscale = upscalingEnabled && drawStarted >= suspendUpscalingUntil
        let upscaleSize = shouldUpscale
            ? NativeStreamVideoEffectsPolicy.upscaleSize(source: frameSize, destination: destination.size)
            : nil
        let ticket = metal4Enabled ? submissionTimeline?.next() : nil
        let presentationTracker = presentations
        let mailbox = frames
        let useDirectHDRPath = NativeStreamVideoEffectsPolicy.canUseDirectHDRPath(
            upscalingEnabled: shouldUpscale, upscaleEligible: upscaleSize != nil,
            sharpeningAmount: sharpeningAmount
        )
        if #available(iOS 26.0, *), metal4Enabled && !metal4Disabled, hdrTransfer == .pq,
           useDirectHDRPath,
           let metal4 = metal4HDRStorage as? NativeStreamMetal4HDRRenderer {
            let admission = gpuAdmission
            if metal4.submit(buffer:pixelBuffer,target:drawable.texture,destination:destination,drawable:drawable,ticket:ticket,
                presented: { time in
                    presentationTracker.recordPresentation(at:time)
                }, presentAt: presentAt, completion: { [weak self] duration,error in
                    admission.signal()
                    mailbox.complete()
                    if let error {
                        NSLog("[OpenNOW] Metal 4 HDR failed code=%ld", error.code)
                        DispatchQueue.main.async { [weak self] in self?.metal4Disabled = true }
                    }
                }) {
                if let ticket { submissionTimeline?.accept(ticket) }
                rendererBackend = "Metal 4 · direct 10-bit HDR"
                if upscalingEnabled {
                    metal4UpscalingStatus = shouldUpscale
                        ? "No upscale: \(Int(frameSize.width))×\(Int(frameSize.height)) → \(Int(destination.width))×\(Int(destination.height))"
                        : nil
                }

                submitted = true
                return
            }
        }
        // Cross-queue ordering also covers live switching to effects/legacy.
        if let ticket, ticket.previous > 0 { commandBuffer.encodeWaitForEvent(ticket.event,value:ticket.previous) }
        let effectsToken = effectsGeneration
        let preferMetal4Effects: Bool
        if #available(iOS 26.0, *) {
            preferMetal4Effects = metal4Enabled && !metal4Disabled && metal4EffectsStorage != nil
        } else { preferMetal4Effects = false }
        let direct = !preferMetal4Effects && hdrTransfer != .hlg && !shouldUpscale && sharpeningAmount <= 0.001 && view.currentRenderPassDescriptor.map {
            directHDR?.encode(buffer: pixelBuffer, commandBuffer: commandBuffer,
                descriptor: $0, destination: destination) == true
        } == true
        rendererBackend = direct ? "Metal · direct 10-bit HDR" : "Metal / Core Image"
        if !direct {
            #if !targetEnvironment(simulator)
            let admission = gpuAdmission
            let presentedMetal4: @Sendable (Double) -> Void = { time in
                presentationTracker.recordPresentation(at: time)
            }
            let completeMetal4: @Sendable (Double, NSError?) -> Void = { [weak self, pixelBuffer] duration, error in
                _ = pixelBuffer
                admission.signal()
                mailbox.complete()
                if let error { NSLog("[OpenNOW] Metal 4 effects failed code=%ld", error.code) }
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.effectsGeneration == effectsToken else { return }
                    if error != nil { self.metal4Disabled = true }
                    if self.upscalingEnabled { self.finishEffects(failed: error != nil) }
                }
            }
            if #available(iOS 26.0, *), metal4Enabled && !metal4Disabled,
               let effects = metal4EffectsStorage as? NativeStreamMetal4EffectsRenderer,
               effects.submit(buffer: pixelBuffer, destination: destination, upscale: shouldUpscale,
                    target: drawable.texture, sharpening:Float(sharpeningAmount),
                    drawable: drawable, ticket: ticket,
                    presented: presentedMetal4, presentAt: presentAt, completion: completeMetal4) {
                if let ticket { submissionTimeline?.accept(ticket) }
                rendererBackend = (hdrTransfer == .pq ? "Metal 4 · native PQ conversion"
                    : hdrTransfer == .hlg ? "Metal 4 · native HLG conversion" : "Metal 4 · native SDR conversion")
                metal4UpscalingStatus = shouldUpscale ? effects.status : nil

                submitted = true
                return
            }
            #endif
            if let descriptor = view.currentRenderPassDescriptor,
               let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: descriptor) {
                encoder.endEncoding()
            }

            let sourceImage = CIImage(cvPixelBuffer: pixelBuffer)
            let sourceExtent = sourceImage.extent
            let filteredImage: CIImage = {
                let normalizedAmount = min(max(sharpeningAmount, 0), 1)
                guard normalizedAmount > 0.001,
                      let filter = sharpeningFilter else {
                    return sourceImage
                }
                filter.setValue(sourceImage, forKey: kCIInputImageKey)
                filter.setValue(normalizedAmount, forKey: kCIInputSharpnessKey)
                return (filter.outputImage ?? sourceImage).cropped(to: sourceExtent)
            }()
            let targetBounds = CGRect(origin: .zero, size: view.drawableSize)
            let destination = stretchToFill
                ? targetBounds
                : Self.aspectFitRect(
                    source: frameSize == .zero ? sourceExtent.size : frameSize,
                    target: targetBounds.size
                )
            #if !targetEnvironment(simulator)
            if #available(iOS 26.0, *), metal4Enabled && !metal4Disabled,
               let effects = metal4EffectsStorage as? NativeStreamMetal4EffectsRenderer {
                if effects.submit(image: filteredImage, destination: destination,
                    transfer: hdrTransfer == .pq ? 1 : hdrTransfer == .hlg ? 2 : 0,
                    upscale: shouldUpscale, context: ciContext, producer: commandBuffer,
                    target: drawable.texture, drawable: drawable, ticket: ticket,
                    presented: presentedMetal4, presentAt: presentAt, completion: completeMetal4) {
                    if let ticket { submissionTimeline?.accept(ticket) }
                    rendererBackend = "Metal 4 · effects"
                    if shouldUpscale { metal4UpscalingStatus = effects.status } else { metal4UpscalingStatus = nil }

                    submitted = true
                    return
                }
            }
            #endif
            metal4UpscalingStatus = nil
            let scaledImage: CIImage
            if shouldUpscale, let scaled = spatialUpscaler.encode(image: filteredImage,
                sourceSize: sourceExtent.size, destinationSize: destination.size, hdr: hdrTransfer != .sdr,
                context: ciContext, commandBuffer: commandBuffer) {
                scaledImage = scaled
            } else { scaledImage = filteredImage }
            let scaleX = destination.width / max(scaledImage.extent.width, 1)
            let scaleY = destination.height / max(scaledImage.extent.height, 1)
            let transform = CGAffineTransform(translationX: destination.minX, y: destination.minY)
                .scaledBy(x: scaleX, y: scaleY)
            let outputImage = scaledImage.transformed(by: transform)

            ciContext.render(
                outputImage,
                to: drawable.texture,
                commandBuffer: commandBuffer,
                bounds: targetBounds,
                colorSpace: colorSpace
            )
        }
        #if !targetEnvironment(simulator)
        // Presentation callbacks are available on the physical device, not CoreSimulator.
        drawable.addPresentedHandler { drawable in
            guard drawable.presentedTime > 0 else { return }
            presentationTracker.recordPresentation(at:drawable.presentedTime)
        }
        #endif
        let admission = gpuAdmission
        let checkEffectsFailure = upscalingEnabled
        commandBuffer.addCompletedHandler { [weak self, pixelBuffer] command in
            admission.signal()
            // Keep the pooled IOSurface alive until the GPU has finished reading it.
            _ = pixelBuffer
            if command.status == .error {
                ticket?.recoverAfterGPUFailure()
                if let error = command.error as NSError? { NSLog("[OpenNOW] Metal rendering failed code=%ld", error.code) }
            }
            mailbox.complete()
            guard checkEffectsFailure, command.status == .error else { return }
            DispatchQueue.main.async { [weak self] in
                guard let self, self.effectsGeneration == effectsToken else { return }
                self.finishEffects(failed: true)
            }
        }

        if let ticket { commandBuffer.encodeSignalEvent(ticket.event,value:ticket.value) }
        if let presentAt { commandBuffer.present(drawable, atTime: presentAt) } else { commandBuffer.present(drawable) }
        submitted = true
        commandBuffer.commit()
        if let ticket { submissionTimeline?.accept(ticket) }
    }

    private func finishEffects(failed: Bool) {
        guard failed else { return }
        suspendUpscalingUntil = CACurrentMediaTime() + 5
        spatialUpscaler.reset()
        effectsGeneration &+= 1
    }

    private func configureColorOutput(for buffer: CVPixelBuffer) {
        let transfer = NativeStreamHDRTransfer.detect(in: buffer)
        guard transfer != hdrTransfer else { return }
        hdrTransfer = transfer
        let hdr = transfer != .sdr
        // Normalize PQ/HLG sources into PQ display encoding with BT.2020 primaries
        // in a 10-bit drawable, with an EDR layer matching the rendered color space.
        colorSpace = hdr
            ? CGColorSpace(name: CGColorSpace.itur_2100_PQ)!
            : CGColorSpace(name: CGColorSpace.sRGB)!
        mtkView.colorPixelFormat = hdr ? .bgr10a2Unorm : .bgra8Unorm
        if let layer = mtkView.layer as? CAMetalLayer {
            layer.colorspace = colorSpace
            layer.wantsExtendedDynamicRangeContent = hdr
        }
    }

    private static func aspectFitRect(source: CGSize, target: CGSize) -> CGRect {
        guard source.width > 0, source.height > 0, target.width > 0, target.height > 0 else {
            return CGRect(origin: .zero, size: target)
        }
        let fitted = NativeStreamVideoEffectsPolicy.presentationSize(source: source, display: target, stretch: false)
        let width = fitted.width
        let height = fitted.height
        return CGRect(
            x: (target.width - width) / 2,
            y: (target.height - height) / 2,
            width: width,
            height: height
        )
    }
}

private enum NativeStreamTouchControlGroup: CaseIterable {
    case topLeft
    case topCenter
    case topRight
    case leftStick
    case rightCluster
    case bottomCenter

    var label: String {
        switch self {
        case .topLeft: return "Left shoulder buttons"
        case .topCenter: return "View and Menu buttons"
        case .topRight: return "Right shoulder buttons"
        case .leftStick: return "Left stick and directional pad"
        case .rightCluster: return "Right stick and face buttons"
        case .bottomCenter: return "Hide controls button"
        }
    }
}

private struct NativeStreamVirtualControllerOverlay: View {
    let inputBridge: NativeStreamInputBridge
    let layout: TouchControlLayout
    /// Behaviour that is not geometry: dead zone, follow-finger, and the outline style.
    var touchSettings: TouchSettings = .default
    let editing: Bool
    let inputEnabled: Bool
    let onPositionChange: (NativeStreamTouchControlGroup, TouchControlPoint) -> Void
    let onHide: () -> Void
    let onReset: () -> Void
    let onDoneEditing: () -> Void

    var body: some View {
        GeometryReader { proxy in
            let compact = proxy.size.width < 600
            let buttonSize = (compact ? 34.0 : 42.0) * layout.buttonScale
            let stickSize = (compact ? 68.0 : 84.0) * layout.stickScale

            ZStack {
                controlGroup(
                    .topLeft,
                    point: layout.topLeft,
                    containerSize: proxy.size,
                    safeAreaInsets: proxy.safeAreaInsets
                ) {
                    HStack(spacing: 8) {
                        NativeStreamVirtualHoldButton(
                            label: "L1",
                            size: buttonSize,
                            pressed: { inputBridge.setVirtualButton(.leftShoulder, pressed: $0) }
                        )
                        NativeStreamVirtualHoldButton(
                            label: "L2",
                            size: buttonSize,
                            pressed: { inputBridge.setVirtualTrigger(.left, value: $0 ? 1 : 0) }
                        )
                    }
                }

                controlGroup(
                    .topCenter,
                    point: layout.topCenter,
                    containerSize: proxy.size,
                    safeAreaInsets: proxy.safeAreaInsets
                ) {
                    HStack(spacing: 8) {
                        NativeStreamVirtualHoldButton(
                            label: "View",
                            systemImage: "rectangle.on.rectangle",
                            size: buttonSize,
                            pressed: { inputBridge.setVirtualButton(.options, pressed: $0) }
                        )
                        NativeStreamVirtualHoldButton(
                            label: "Menu",
                            systemImage: "line.3.horizontal",
                            size: buttonSize,
                            pressed: { inputBridge.setVirtualButton(.menu, pressed: $0) }
                        )
                    }
                }

                controlGroup(
                    .topRight,
                    point: layout.topRight,
                    containerSize: proxy.size,
                    safeAreaInsets: proxy.safeAreaInsets
                ) {
                    HStack(spacing: 8) {
                        NativeStreamVirtualHoldButton(
                            label: "R2",
                            size: buttonSize,
                            pressed: { inputBridge.setVirtualTrigger(.right, value: $0 ? 1 : 0) }
                        )
                        NativeStreamVirtualHoldButton(
                            label: "R1",
                            size: buttonSize,
                            pressed: { inputBridge.setVirtualButton(.rightShoulder, pressed: $0) }
                        )
                    }
                }

                controlGroup(
                    .leftStick,
                    point: layout.leftStick,
                    containerSize: proxy.size,
                    safeAreaInsets: proxy.safeAreaInsets
                ) {
                    HStack(alignment: .bottom, spacing: compact ? 5 : 10) {
                        NativeStreamVirtualStickView(
                            label: "L",
                            size: stickSize,
                            deadZone: touchSettings.joystickDeadZone,
                            followsFinger: touchSettings.joystickMode == .dynamic,
                            outlineStyle: touchSettings.style == .outline,
                            changed: { x, y in inputBridge.setVirtualStick(.left, x: x, y: y) },
                            pressed: { inputBridge.setVirtualButton(.leftStick, pressed: $0) }
                        )
                        NativeStreamVirtualDPad(size: buttonSize * 0.72, inputBridge: inputBridge)
                    }
                }

                controlGroup(
                    .rightCluster,
                    point: layout.rightCluster,
                    containerSize: proxy.size,
                    safeAreaInsets: proxy.safeAreaInsets
                ) {
                    HStack(alignment: .bottom, spacing: compact ? 5 : 10) {
                        NativeStreamVirtualStickView(
                            label: "R",
                            size: stickSize,
                            deadZone: touchSettings.joystickDeadZone,
                            followsFinger: touchSettings.joystickMode == .dynamic,
                            outlineStyle: touchSettings.style == .outline,
                            changed: { x, y in inputBridge.setVirtualStick(.right, x: x, y: y) },
                            pressed: { inputBridge.setVirtualButton(.rightStick, pressed: $0) }
                        )
                        NativeStreamVirtualFaceButtons(size: buttonSize, inputBridge: inputBridge)
                    }
                }

                controlGroup(
                    .bottomCenter,
                    point: layout.bottomCenter,
                    containerSize: proxy.size,
                    safeAreaInsets: proxy.safeAreaInsets
                ) {
                    Button(action: onHide) {
                        Label("Hide controls", systemImage: "eye.slash")
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                    }
                    .buttonStyle(.bordered)
                    .tint(.white)
                }

                if editing {
                    NativeStreamTouchLayoutEditorToolbar(
                        onReset: onReset,
                        onDone: onDoneEditing
                    )
                    .frame(maxWidth: min(max(proxy.size.width - 24, 1), 430))
                    .position(x: proxy.size.width / 2, y: proxy.size.height * 0.48)
                    .zIndex(100)
                }
            }
        }
        .onAppear { inputBridge.setVirtualControllerEnabled(inputEnabled) }
        .onChangeCompat(of: inputEnabled) { inputBridge.setVirtualControllerEnabled($0) }
        .onDisappear { inputBridge.setVirtualControllerEnabled(false) }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(editing ? "Edit touch controller layout" : "Touch controller")
    }

    private func controlGroup<Content: View>(
        _ group: NativeStreamTouchControlGroup,
        point: TouchControlPoint,
        containerSize: CGSize,
        safeAreaInsets: EdgeInsets,
        @ViewBuilder content: @escaping () -> Content
    ) -> some View {
        NativeStreamPositionedControlGroup(
            group: group,
            point: point,
            containerSize: containerSize,
            safeAreaInsets: safeAreaInsets,
            scale: layout.scale,
            opacity: layout.opacity,
            edgePadding: touchSettings.edgePadding,
            bottomPadding: touchSettings.bottomPadding,
            sideOffset: sideOffset(for: group, point: point),
            editing: editing,
            onPositionChange: { onPositionChange(group, $0) },
            content: content
        )
    }

    /// Which half of the screen a group lives on decides which nudge applies. Groups pinned near
    /// the middle get neither — nudging them by a "left" offset would be surprising.
    private func sideOffset(for group: NativeStreamTouchControlGroup, point: TouchControlPoint) -> CGSize {
        switch point.x {
        case ..<0.4:
            return CGSize(width: touchSettings.leftOffsetX, height: touchSettings.leftOffsetY)
        case 0.6...:
            return CGSize(width: touchSettings.rightOffsetX, height: touchSettings.rightOffsetY)
        default:
            return .zero
        }
    }
}

private struct NativeStreamTouchLayoutEditorToolbar: View {
    let onReset: () -> Void
    let onDone: () -> Void

    var body: some View {
        VStack(spacing: 10) {
            Label("Drag the highlighted control groups", systemImage: "hand.draw")
                .font(.subheadline.weight(.semibold))
                .multilineTextAlignment(.center)

            HStack(spacing: 10) {
                Button(role: .destructive) {
                    onReset()
                } label: {
                    Label("Reset", systemImage: "arrow.counterclockwise")
                }
                .buttonStyle(.bordered)

                Button {
                    onDone()
                } label: {
                    Label("Done", systemImage: "checkmark")
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(Color.accentColor.opacity(0.55), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.28), radius: 14, y: 7)
    }
}

private struct NativeStreamPositionedControlGroup<Content: View>: View {
    let group: NativeStreamTouchControlGroup
    let point: TouchControlPoint
    let containerSize: CGSize
    let safeAreaInsets: EdgeInsets
    let scale: Double
    let opacity: Double
    /// Extra inset beyond the safe area, for phones whose curved corners or case swallow the
    /// outermost few points.
    var edgePadding: Double = 0
    var bottomPadding: Double = 0
    /// Whole-side nudges, so a left-handed grip can shift both left controls without dragging
    /// each group individually.
    var sideOffset: CGSize = .zero
    let editing: Bool
    let onPositionChange: (TouchControlPoint) -> Void
    @ViewBuilder let content: () -> Content

    @State private var contentSize = CGSize.zero
    @GestureState private var dragTranslation = CGSize.zero

    var body: some View {
        Group {
            if editing {
                positionedContent
                    .contentShape(Rectangle())
                    .gesture(
                        DragGesture(minimumDistance: 0)
                            .updating($dragTranslation) { value, state, _ in
                                state = value.translation
                            }
                            .onEnded { value in
                                onPositionChange(normalizedPoint(after: value.translation))
                            }
                    )
                    .accessibilityHint("Drag to move this control group")
            } else {
                positionedContent
            }
        }
        .onPreferenceChange(NativeStreamControlGroupSizePreferenceKey.self) { size in
            guard size.width > 0, size.height > 0 else { return }
            contentSize = size
        }
    }

    private var positionedContent: some View {
        content()
            .allowsHitTesting(!editing)
            .opacity(editing ? max(opacity, 0.82) : opacity)
            .padding(editing ? 7 : 0)
            .background {
                GeometryReader { proxy in
                    Color.clear.preference(
                        key: NativeStreamControlGroupSizePreferenceKey.self,
                        value: proxy.size
                    )
                }
            }
            .overlay {
                if editing {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .strokeBorder(
                            Color.accentColor,
                            style: StrokeStyle(lineWidth: 2, dash: [6, 4])
                        )
                }
            }
            .overlay(alignment: .topLeading) {
                if editing {
                    Text(group.label)
                        .font(.caption2.weight(.bold))
                        .lineLimit(1)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 3)
                        .foregroundStyle(.white)
                        .background(Color.accentColor, in: Capsule())
                        .offset(y: -12)
                }
            }
            .scaleEffect(scale)
            .position(basePosition)
            .offset(dragTranslation)
            .zIndex(editing ? 20 : 1)
            .accessibilityLabel(group.label)
    }

    private var safeRect: CGRect {
        let margin = CGFloat(8 + max(edgePadding, 0))
        let left = safeAreaInsets.leading + margin
        let top = safeAreaInsets.top + margin
        let right = safeAreaInsets.trailing + margin
        let bottom = safeAreaInsets.bottom + margin + CGFloat(max(bottomPadding, 0))
        return CGRect(
            x: left,
            y: top,
            width: max(1, containerSize.width - left - right),
            height: max(1, containerSize.height - top - bottom)
        )
    }

    private var scaledContentSize: CGSize {
        CGSize(
            width: contentSize.width * max(scale, 0.01),
            height: contentSize.height * max(scale, 0.01)
        )
    }

    private var basePosition: CGPoint {
        clampedPosition(
            CGPoint(
                x: safeRect.minX + safeRect.width * CGFloat(point.x) + sideOffset.width,
                y: safeRect.minY + safeRect.height * CGFloat(point.y) + sideOffset.height
            )
        )
    }

    private func normalizedPoint(after translation: CGSize) -> TouchControlPoint {
        let center = clampedPosition(
            CGPoint(
                x: basePosition.x + translation.width,
                y: basePosition.y + translation.height
            )
        )
        // Subtract the side nudge again, so dragging a group stores where it sits relative to the
        // layout rather than baking the current offset into the saved point.
        return TouchControlPoint(
            x: Double(min(max((center.x - sideOffset.width - safeRect.minX) / safeRect.width, 0), 1)),
            y: Double(min(max((center.y - sideOffset.height - safeRect.minY) / safeRect.height, 0), 1))
        )
    }

    private func clampedPosition(_ proposed: CGPoint) -> CGPoint {
        let halfWidth = scaledContentSize.width / 2
        let halfHeight = scaledContentSize.height / 2
        return CGPoint(
            x: clamped(proposed.x, lower: safeRect.minX + halfWidth, upper: safeRect.maxX - halfWidth),
            y: clamped(proposed.y, lower: safeRect.minY + halfHeight, upper: safeRect.maxY - halfHeight)
        )
    }

    private func clamped(_ value: CGFloat, lower: CGFloat, upper: CGFloat) -> CGFloat {
        guard lower <= upper else { return (lower + upper) / 2 }
        return min(max(value, lower), upper)
    }
}

private struct NativeStreamControlGroupSizePreferenceKey: PreferenceKey {
    static var defaultValue = CGSize.zero

    static func reduce(value: inout CGSize, nextValue: () -> CGSize) {
        let next = nextValue()
        if next.width > 0, next.height > 0 {
            value = next
        }
    }
}

private struct NativeStreamVirtualDPad: View {
    let size: CGFloat
    let inputBridge: NativeStreamInputBridge

    var body: some View {
        Grid(horizontalSpacing: 2, verticalSpacing: 2) {
            GridRow {
                Color.clear.frame(width: size, height: size)
                directionButton("chevron.up", .dpadUp)
                Color.clear.frame(width: size, height: size)
            }
            GridRow {
                directionButton("chevron.left", .dpadLeft)
                Color.white.opacity(0.13).frame(width: size, height: size)
                directionButton("chevron.right", .dpadRight)
            }
            GridRow {
                Color.clear.frame(width: size, height: size)
                directionButton("chevron.down", .dpadDown)
                Color.clear.frame(width: size, height: size)
            }
        }
        .background(.black.opacity(0.18), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Directional pad")
    }

    private func directionButton(
        _ image: String,
        _ button: NativeStreamVirtualGamepadButton
    ) -> some View {
        NativeStreamVirtualHoldButton(
            label: image.replacingOccurrences(of: "chevron.", with: "D-pad "),
            systemImage: image,
            size: size,
            pressed: { inputBridge.setVirtualButton(button, pressed: $0) }
        )
    }
}

private struct NativeStreamVirtualFaceButtons: View {
    let size: CGFloat
    let inputBridge: NativeStreamInputBridge

    var body: some View {
        let size = max(size, 44)
        ZStack {
            faceButton("Y", .y, color: .yellow, size: size).offset(y: -size * 0.82)
            faceButton("B", .b, color: .red, size: size).offset(x: size * 0.82)
            faceButton("A", .a, color: .green, size: size).offset(y: size * 0.82)
            faceButton("X", .x, color: .blue, size: size).offset(x: -size * 0.82)
        }
        .frame(width: size * 2.7, height: size * 2.7)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Face buttons")
    }

    private func faceButton(
        _ label: String,
        _ button: NativeStreamVirtualGamepadButton,
        color: Color,
        size: CGFloat
    ) -> some View {
        NativeStreamVirtualHoldButton(
            label: label,
            size: size,
            tint: color,
            pressed: { inputBridge.setVirtualButton(button, pressed: $0) }
        )
    }
}

private struct NativeStreamVirtualHoldButton: View {
    let label: String
    var systemImage: String? = nil
    let size: CGFloat
    var tint: Color = .white
    let pressed: (Bool) -> Void

    @State private var isPressed = false

    var body: some View {
        Group {
            if let systemImage {
                Image(systemName: systemImage)
            } else {
                Text(label)
            }
        }
        .font(.caption.weight(.bold))
        .foregroundStyle(tint)
        .frame(width: size, height: size)
        .background(.ultraThinMaterial, in: Circle())
        .overlay(Circle().stroke(tint.opacity(isPressed ? 0.8 : 0.28), lineWidth: isPressed ? 2 : 1))
        .scaleEffect(isPressed ? 0.91 : 1)
        .contentShape(Circle())
        .overlay {
            NativeStreamVirtualButtonTouchSurface(pressed: setPressed)
                .accessibilityHidden(true)
        }
        .onDisappear { setPressed(false) }
        .accessibilityLabel(label)
        .accessibilityAddTraits(.isButton)
        .accessibilityAction {
            setPressed(true)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.06) {
                setPressed(false)
            }
        }
        .animation(.easeOut(duration: 0.08), value: isPressed)
    }

    private func setPressed(_ next: Bool) {
        guard isPressed != next else { return }
        isPressed = next
        pressed(next)
    }
}

/// UIKit delivers button-down before gesture recognition and gives cancellation
/// an explicit release path. Short taps must survive at least a few host polls.
final class NativeStreamVirtualButtonTouchControl: UIControl {
    var pressed: (Bool) -> Void = { _ in }
    private(set) var isPressed = false
    private var beganAt: CFTimeInterval = 0
    private var pendingRelease: DispatchWorkItem?
    private let minimumPressDuration: CFTimeInterval = 0.05

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isExclusiveTouch = false
        isMultipleTouchEnabled = false
        isAccessibilityElement = false
        addTarget(self, action: #selector(beginPress), for: .touchDown)
        addTarget(self, action: #selector(endPress), for: [.touchUpInside, .touchUpOutside])
        addTarget(self, action: #selector(cancelPress), for: .touchCancel)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    @objc private func beginPress() {
        guard isEnabled else { return }
        // A second tap cancels the previous delayed release; it must not release
        // this new press later, and the host still needs the new down edge.
        cancelPress()
        beganAt = CACurrentMediaTime()
        isPressed = true
        pressed(true)
    }

    @objc private func endPress() {
        guard isPressed else { return }
        let remaining = minimumPressDuration - (CACurrentMediaTime() - beganAt)
        guard remaining > 0 else { cancelPress(); return }
        pendingRelease?.cancel()
        let release = DispatchWorkItem { [weak self] in self?.cancelPress() }
        pendingRelease = release
        DispatchQueue.main.asyncAfter(deadline: .now() + remaining, execute: release)
    }

    @objc func cancelPress() {
        pendingRelease?.cancel()
        pendingRelease = nil
        guard isPressed else { return }
        isPressed = false
        pressed(false)
    }

    override var isEnabled: Bool {
        didSet { if !isEnabled { cancelPress() } }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil { cancelPress() }
    }
}

private struct NativeStreamVirtualButtonTouchSurface: UIViewRepresentable {
    let pressed: (Bool) -> Void

    func makeUIView(context: Context) -> NativeStreamVirtualButtonTouchControl {
        let control = NativeStreamVirtualButtonTouchControl(frame: .zero)
        control.pressed = pressed
        return control
    }

    func updateUIView(_ control: NativeStreamVirtualButtonTouchControl, context: Context) {
        control.pressed = pressed
    }

    static func dismantleUIView(_ control: NativeStreamVirtualButtonTouchControl, coordinator: ()) {
        control.cancelPress()
    }
}

private struct NativeStreamVirtualStickView: View {
    let label: String
    let size: CGFloat
    /// Fraction of travel ignored around centre. Rests at 0 — a resting thumb on a stick with no
    /// dead zone still sends tiny deflections, which reads as drift in games that poll raw axes.
    var deadZone: Double = 0
    /// `.dynamic` re-centres the stick under wherever the thumb lands rather than pinning it to
    /// the drawn circle, which is what most touch shooters expect.
    var followsFinger: Bool = false
    var outlineStyle: Bool = false
    let changed: (CGFloat, CGFloat) -> Void
    let pressed: (Bool) -> Void

    @State private var knobOffset = CGSize.zero
    @State private var origin: CGPoint?
    @State private var moved = false

    var body: some View {
        ZStack {
            Circle()
                .fill(outlineStyle ? AnyShapeStyle(Color.black.opacity(0.18)) : AnyShapeStyle(.ultraThinMaterial))
                .overlay(Circle().stroke(Color.white.opacity(outlineStyle ? 0.42 : 0.22), lineWidth: outlineStyle ? 1.5 : 1))
            Circle()
                .fill(Color.white.opacity(outlineStyle ? 0.14 : 0.30))
                .overlay(Circle().strokeBorder(Color.white.opacity(outlineStyle ? 0.55 : 0), lineWidth: 1.5))
                .frame(width: size * 0.48, height: size * 0.48)
                .overlay(Text(label).font(.caption2.bold()).foregroundStyle(.white))
                .offset(knobOffset)
        }
        .frame(width: size, height: size)
        .contentShape(Circle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { value in
                    let radius = max(1, size * 0.34)
                    // In follow-finger mode the first touch becomes the centre, so the very first
                    // move is relative to where the thumb landed rather than to the drawn circle.
                    if origin == nil { origin = followsFinger ? value.startLocation : nil }
                    let base = origin ?? value.startLocation
                    let raw = followsFinger
                        ? CGSize(width: value.location.x - base.x, height: value.location.y - base.y)
                        : CGSize(width: value.translation.width, height: value.translation.height)
                    let magnitude = hypot(raw.width, raw.height)
                    let scale = magnitude > radius ? radius / magnitude : 1
                    knobOffset = CGSize(width: raw.width * scale, height: raw.height * scale)
                    moved = moved || magnitude > 5
                    let (x, y) = TouchStickMath.applyDeadZone(
                        x: knobOffset.width / radius,
                        y: -knobOffset.height / radius,
                        deadZone: deadZone
                    )
                    changed(x, y)
                }
                .onEnded { _ in
                    changed(0, 0)
                    knobOffset = .zero
                    origin = nil
                    if !moved {
                        pressed(true)
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.06) { pressed(false) }
                    }
                    moved = false
                }
        )
        .onDisappear { changed(0, 0) }
        .accessibilityLabel("\(label) thumbstick")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: changed(0, 1)
            case .decrement: changed(0, -1)
            @unknown default: break
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) { changed(0, 0) }
        }
    }

}

/// What the touch surface needs to turn a finger into a point on the picture.
///
/// Passed by value at touch time rather than held as view state: the zoom transform moves once per
/// frame during a pinch, and a published property for it would re-render the whole streamer.
struct NativeStreamTouchGeometry {
    var streamSize: CGSize = .zero
    var stretchToFill = false
    var zoomScale: CGFloat = 1
    var zoomOffset: CGSize = .zero
    var scrollScale: CGFloat = 1
    var jitterThreshold: CGFloat = 0
}

private struct NativeStreamTouchCaptureView: UIViewRepresentable {
    let inputBridge: NativeStreamInputBridge
    let inputEnabled: Bool
    let nativeTouchEnabled: Bool
    let geometry: () -> NativeStreamTouchGeometry
    let onZoomGesture: (CGFloat, CGSize) -> Void

    func makeUIView(context: Context) -> NativeStreamTouchView {
        let view = NativeStreamTouchView()
        view.inputBridge = inputBridge
        view.geometryProvider = geometry
        view.onZoomGesture = onZoomGesture
        view.nativeTouchEnabled = nativeTouchEnabled
        view.inputEnabled = inputEnabled
        return view
    }

    func updateUIView(_ uiView: NativeStreamTouchView, context: Context) {
        uiView.inputBridge = inputBridge
        uiView.geometryProvider = geometry
        uiView.onZoomGesture = onZoomGesture
        uiView.nativeTouchEnabled = nativeTouchEnabled
        uiView.inputEnabled = inputEnabled
    }

    static func dismantleUIView(_ uiView: NativeStreamTouchView, coordinator: ()) {
        uiView.releaseAllNativeTouches()
    }
}

private extension CGPoint {
    func distance(to other: CGPoint) -> CGFloat {
        hypot(x - other.x, y - other.y)
    }
}

private final class NativeStreamTouchView: UIView {
    weak var inputBridge: NativeStreamInputBridge?
    var inputEnabled = true {
        didSet {
            guard inputEnabled != oldValue else { return }
            if !inputEnabled {
                cancelTouchInteraction()
            }
            updateAccessibilityPresentation()
        }
    }

    /// Every finger goes straight to the host as a digitizer contact. Mutually exclusive with the
    /// trackpad cursor above — the two would fight over the same fingers.
    var nativeTouchEnabled = false {
        didSet {
            guard nativeTouchEnabled != oldValue else { return }
            if nativeTouchEnabled {
                cancelTouchInteraction()
            } else {
                releaseAllNativeTouches()
            }
            updateAccessibilityPresentation()
        }
    }

    var geometryProvider: (() -> NativeStreamTouchGeometry)?
    var onZoomGesture: ((CGFloat, CGSize) -> Void)?
    private static let tapMovementThreshold: CGFloat = 8
    private static let clickReleaseDelay: TimeInterval = 0.045

    private var touchSlots = NativeTouchSlotAllocator<ObjectIdentifier>()
    /// Where each finger landed, for the jitter dead zone and the scroll-scale pivot.
    private var nativeTouchOrigins: [ObjectIdentifier: CGPoint] = [:]

    private var activeTouch: UITouch?
    private var touchStartPoint: CGPoint?
    private var lastPoint: CGPoint?
    private var movedBeyondTapThreshold = false
    private var pinchTouches: [UITouch] = []
    private var lastPinchDistance: CGFloat = 0
    private var lastPinchCentroid: CGPoint?
    private var pinchActive = false
    private var suppressSingleTouchUntilAllEnded = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
        backgroundColor = .clear
        isAccessibilityElement = true
        accessibilityLabel = "Stream input surface"
        accessibilityTraits = [.button, .allowsDirectInteraction]
        updateAccessibilityPresentation()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var canBecomeFirstResponder: Bool { true }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil {
            becomeFirstResponder()
        } else {
            releaseAllNativeTouches()
        }
    }

    private func updateAccessibilityPresentation() {
        isAccessibilityElement = inputEnabled || nativeTouchEnabled
        if nativeTouchEnabled {
            accessibilityHint = "Touch is sent to the game directly."
        } else if inputEnabled {
            accessibilityHint = "Drag to move the pointer, tap to click, or pinch with two fingers to zoom."
        } else {
            accessibilityHint = nil
        }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        if nativeTouchEnabled {
            becomeFirstResponder()
            dispatchNativeTouch(phase: NativeTouchPhase.down, touches: touches)
            return
        }
        guard inputEnabled else { return }
        becomeFirstResponder()
        let liveTouches = activeTouches(in: event, fallback: touches)
        if liveTouches.count >= 2 {
            if pinchActive {
                updatePinch(using: liveTouches)
            } else {
                beginPinch(using: liveTouches)
            }
            return
        }
        guard !pinchActive,
              !suppressSingleTouchUntilAllEnded,
              activeTouch == nil,
              let touch = liveTouches.first ?? touches.first else { return }
        activeTouch = touch
        let point = touch.location(in: self)
        touchStartPoint = point
        lastPoint = point
        movedBeyondTapThreshold = false
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        if nativeTouchEnabled {
            dispatchNativeTouch(phase: NativeTouchPhase.move, touches: touches)
            return
        }
        guard inputEnabled else { return }
        let liveTouches = activeTouches(in: event, fallback: touches)
        if liveTouches.count >= 2 {
            if pinchActive {
                updatePinch(using: liveTouches)
            } else {
                beginPinch(using: liveTouches)
            }
            return
        }
        guard !pinchActive, !suppressSingleTouchUntilAllEnded else { return }
        guard let activeTouch, touches.contains(activeTouch), let lastPoint else { return }
        let point = activeTouch.location(in: self)
        if let touchStartPoint, point.distance(to: touchStartPoint) > Self.tapMovementThreshold {
            movedBeyondTapThreshold = true
        }
        inputBridge?.sendTouchMouseMove(dx: point.x - lastPoint.x, dy: point.y - lastPoint.y)
        self.lastPoint = point
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        if nativeTouchEnabled {
            dispatchNativeTouch(phase: NativeTouchPhase.up, touches: touches)
            return
        }
        let liveTouches = activeTouches(in: event, fallback: [])
        if pinchActive {
            if liveTouches.count >= 2 {
                updatePinch(using: liveTouches)
            } else {
                endPinch(remainingTouches: liveTouches.count)
            }
            return
        }
        if suppressSingleTouchUntilAllEnded {
            if liveTouches.isEmpty {
                suppressSingleTouchUntilAllEnded = false
            }
            return
        }
        guard inputEnabled else { return }
        guard let activeTouch, touches.contains(activeTouch) else { return }
        let shouldClick = !movedBeyondTapThreshold
        resetActiveTouch()
        if shouldClick {
            sendPrimaryClick()
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        if nativeTouchEnabled {
            dispatchNativeTouch(phase: NativeTouchPhase.cancel, touches: touches)
            return
        }
        let liveTouches = activeTouches(in: event, fallback: [])
        if pinchActive {
            endPinch(remainingTouches: liveTouches.count)
            return
        }
        if suppressSingleTouchUntilAllEnded {
            if liveTouches.isEmpty {
                suppressSingleTouchUntilAllEnded = false
            }
            return
        }
        guard let activeTouch, touches.contains(activeTouch) else { return }
        resetActiveTouch()
    }

    /// Forwards fingers as native touch, so the host presents a digitizer and a touch-aware game
    /// switches to its own mobile UI.
    ///
    /// Unlike the cursor paths this keeps no per-gesture state beyond the finger-to-slot mapping
    /// and each finger's landing point. UIKit already delivers exactly the touches that changed,
    /// which is the split Android has to reconstruct from `MotionEvent.actionMasked`.
    private func dispatchNativeTouch(phase: UInt8, touches: Set<UITouch>) {
        guard let inputBridge, let geometry = geometryProvider?() else { return }
        let viewSize = bounds.size
        guard viewSize.width > 0, viewSize.height > 0,
              geometry.streamSize.width > 0, geometry.streamSize.height > 0 else {
            return
        }

        let scrollScale = min(max(geometry.scrollScale, 0.25), 2)
        let jitterThreshold = max(geometry.jitterThreshold, 0)
        let lifting = phase == NativeTouchPhase.up || phase == NativeTouchPhase.cancel

        var samples: [NativeTouchPointerSample<ObjectIdentifier>] = []
        samples.reserveCapacity(touches.count)

        for touch in touches {
            let identity = ObjectIdentifier(touch)
            let raw = touch.location(in: self)
            var location = raw

            switch phase {
            case NativeTouchPhase.down:
                nativeTouchOrigins[identity] = raw
            case NativeTouchPhase.move:
                let origin = nativeTouchOrigins[identity]
                if let origin, jitterThreshold > 0 {
                    // Hold the contact still until the finger has left a small dead zone around
                    // where it landed. Without this, sensor noise on a resting thumb reads as a
                    // micro-swipe and games treat it as a drag.
                    if hypot(raw.x - origin.x, raw.y - origin.y) < jitterThreshold { continue }
                }
                if let origin, scrollScale != 1 {
                    // Scale the apparent velocity of the gesture by moving the reported point
                    // further from where it started, rather than clamping coordinates.
                    location = CGPoint(
                        x: origin.x + (raw.x - origin.x) * scrollScale,
                        y: origin.y + (raw.y - origin.y) * scrollScale
                    )
                }
            default:
                nativeTouchOrigins.removeValue(forKey: identity)
            }

            let radius = touch.majorRadius.isFinite ? max(touch.majorRadius, 0) : 0
            samples.append(
                NativeTouchPointerSample(
                    pointer: identity,
                    location: location,
                    radiusX: radius,
                    radiusY: radius
                )
            )
        }

        guard !samples.isEmpty else { return }

        let records = NativeTouchGeometry.buildBatch(
            allocator: &touchSlots,
            phase: phase,
            pointers: samples,
            viewSize: viewSize,
            streamSize: geometry.streamSize,
            stretchToFill: geometry.stretchToFill,
            zoomScale: geometry.zoomScale,
            zoomOffset: geometry.zoomOffset
        )
        guard !records.isEmpty else { return }
        inputBridge.sendNativeTouch(records)

        if lifting, touchSlots.activeCount == 0 {
            nativeTouchOrigins.removeAll(keepingCapacity: true)
        }
    }

    /// Lifts every finger the host still believes is down. UIKit will not deliver the missing
    /// `touchesEnded` when the surface is torn down or the mode is switched mid-gesture.
    func releaseAllNativeTouches() {
        nativeTouchOrigins.removeAll(keepingCapacity: true)
        let records = NativeTouchGeometry.cancelAll(allocator: &touchSlots)
        guard !records.isEmpty else { return }
        inputBridge?.sendNativeTouch(records)
    }

    private func activeTouches(in event: UIEvent?, fallback: Set<UITouch>) -> [UITouch] {
        let candidates = event?.touches(for: self) ?? fallback
        return candidates.filter { touch in
            touch.phase == .began || touch.phase == .moved || touch.phase == .stationary
        }
    }

    private func beginPinch(using touches: [UITouch]) {
        guard touches.count >= 2 else { return }
        resetActiveTouch()
        pinchTouches = Array(touches.prefix(2))
        let points = pinchTouches.map { $0.location(in: self) }
        lastPinchDistance = points[0].distance(to: points[1])
        lastPinchCentroid = CGPoint(
            x: (points[0].x + points[1].x) / 2,
            y: (points[0].y + points[1].y) / 2
        )
        pinchActive = true
        suppressSingleTouchUntilAllEnded = true
    }

    private func updatePinch(using touches: [UITouch]) {
        guard touches.count >= 2 else { return }
        let liveTouchSet = Set(touches)
        if pinchTouches.count != 2 || !pinchTouches.allSatisfy(liveTouchSet.contains) {
            pinchTouches = Array(touches.prefix(2))
            let points = pinchTouches.map { $0.location(in: self) }
            lastPinchDistance = points[0].distance(to: points[1])
            lastPinchCentroid = CGPoint(
                x: (points[0].x + points[1].x) / 2,
                y: (points[0].y + points[1].y) / 2
            )
            return
        }

        let firstPoint = pinchTouches[0].location(in: self)
        let secondPoint = pinchTouches[1].location(in: self)
        let distance = firstPoint.distance(to: secondPoint)
        let centroid = CGPoint(
            x: (firstPoint.x + secondPoint.x) / 2,
            y: (firstPoint.y + secondPoint.y) / 2
        )
        if let lastPinchCentroid, lastPinchDistance > 0, distance > 0 {
            let scaleChange = min(max(distance / lastPinchDistance, 0.82), 1.22)
            onZoomGesture?(
                scaleChange,
                CGSize(
                    width: centroid.x - lastPinchCentroid.x,
                    height: centroid.y - lastPinchCentroid.y
                )
            )
        }
        lastPinchDistance = distance
        lastPinchCentroid = centroid
    }

    private func endPinch(remainingTouches: Int) {
        pinchTouches.removeAll(keepingCapacity: true)
        lastPinchDistance = 0
        lastPinchCentroid = nil
        pinchActive = false
        suppressSingleTouchUntilAllEnded = remainingTouches > 0
    }

    private func cancelTouchInteraction() {
        resetActiveTouch()
        pinchTouches.removeAll(keepingCapacity: true)
        lastPinchDistance = 0
        lastPinchCentroid = nil
        pinchActive = false
        suppressSingleTouchUntilAllEnded = false
    }

    private func resetActiveTouch() {
        activeTouch = nil
        touchStartPoint = nil
        lastPoint = nil
        movedBeyondTapThreshold = false
    }

    override func accessibilityActivate() -> Bool {
        guard inputEnabled else { return false }
        sendPrimaryClick()
        return true
    }

    private func sendPrimaryClick() {
        guard inputEnabled else { return }
        inputBridge?.sendMouseButton(1, pressed: true)
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.clickReleaseDelay) { [weak self] in
            self?.inputBridge?.sendMouseButton(1, pressed: false)
        }
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard forwardPresses(presses, pressed: true) else {
            super.pressesBegan(presses, with: event)
            return
        }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard forwardPresses(presses, pressed: false) else {
            super.pressesEnded(presses, with: event)
            return
        }
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        _ = forwardPresses(presses, pressed: false)
        super.pressesCancelled(presses, with: event)
    }

    private func forwardPresses(_ presses: Set<UIPress>, pressed: Bool) -> Bool {
        var handled = false
        for press in presses {
            guard let key = press.key,
                  let mapping = NativeStreamUIKitKeyboardMapper.mapping(for: key.keyCode) else { continue }
            inputBridge?.sendKey(
                mapping: mapping,
                pressed: pressed,
                modifiers: NativeStreamUIKitKeyboardMapper.modifiers(for: key.modifierFlags)
            )
            handled = true
        }
        return handled
    }
}

private enum NativeStreamUIKitKeyboardMapper {
    private static let mappingByHID: [Int: NativeStreamKeyboardMapping] = [
        0x04: .init(virtualKey: 0x41, scanCode: 0x001e),
        0x05: .init(virtualKey: 0x42, scanCode: 0x0030),
        0x06: .init(virtualKey: 0x43, scanCode: 0x002e),
        0x07: .init(virtualKey: 0x44, scanCode: 0x0020),
        0x08: .init(virtualKey: 0x45, scanCode: 0x0012),
        0x09: .init(virtualKey: 0x46, scanCode: 0x0021),
        0x0a: .init(virtualKey: 0x47, scanCode: 0x0022),
        0x0b: .init(virtualKey: 0x48, scanCode: 0x0023),
        0x0c: .init(virtualKey: 0x49, scanCode: 0x0017),
        0x0d: .init(virtualKey: 0x4a, scanCode: 0x0024),
        0x0e: .init(virtualKey: 0x4b, scanCode: 0x0025),
        0x0f: .init(virtualKey: 0x4c, scanCode: 0x0026),
        0x10: .init(virtualKey: 0x4d, scanCode: 0x0032),
        0x11: .init(virtualKey: 0x4e, scanCode: 0x0031),
        0x12: .init(virtualKey: 0x4f, scanCode: 0x0018),
        0x13: .init(virtualKey: 0x50, scanCode: 0x0019),
        0x14: .init(virtualKey: 0x51, scanCode: 0x0010),
        0x15: .init(virtualKey: 0x52, scanCode: 0x0013),
        0x16: .init(virtualKey: 0x53, scanCode: 0x001f),
        0x17: .init(virtualKey: 0x54, scanCode: 0x0014),
        0x18: .init(virtualKey: 0x55, scanCode: 0x0016),
        0x19: .init(virtualKey: 0x56, scanCode: 0x002f),
        0x1a: .init(virtualKey: 0x57, scanCode: 0x0011),
        0x1b: .init(virtualKey: 0x58, scanCode: 0x002d),
        0x1c: .init(virtualKey: 0x59, scanCode: 0x0015),
        0x1d: .init(virtualKey: 0x5a, scanCode: 0x002c),
        0x1e: .init(virtualKey: 0x31, scanCode: 0x0002),
        0x1f: .init(virtualKey: 0x32, scanCode: 0x0003),
        0x20: .init(virtualKey: 0x33, scanCode: 0x0004),
        0x21: .init(virtualKey: 0x34, scanCode: 0x0005),
        0x22: .init(virtualKey: 0x35, scanCode: 0x0006),
        0x23: .init(virtualKey: 0x36, scanCode: 0x0007),
        0x24: .init(virtualKey: 0x37, scanCode: 0x0008),
        0x25: .init(virtualKey: 0x38, scanCode: 0x0009),
        0x26: .init(virtualKey: 0x39, scanCode: 0x000a),
        0x27: .init(virtualKey: 0x30, scanCode: 0x000b),
        0x28: .init(virtualKey: 0x0d, scanCode: 0x001c),
        0x29: .init(virtualKey: 0x1b, scanCode: 0x0001),
        0x2a: .init(virtualKey: 0x08, scanCode: 0x000e),
        0x2b: .init(virtualKey: 0x09, scanCode: 0x000f),
        0x2c: .init(virtualKey: 0x20, scanCode: 0x0039),
        0x2d: .init(virtualKey: 0xbd, scanCode: 0x000c),
        0x2e: .init(virtualKey: 0xbb, scanCode: 0x000d),
        0x2f: .init(virtualKey: 0xdb, scanCode: 0x001a),
        0x30: .init(virtualKey: 0xdd, scanCode: 0x001b),
        0x31: .init(virtualKey: 0xdc, scanCode: 0x002b),
        0x33: .init(virtualKey: 0xba, scanCode: 0x0027),
        0x34: .init(virtualKey: 0xde, scanCode: 0x0028),
        0x35: .init(virtualKey: 0xc0, scanCode: 0x0029),
        0x36: .init(virtualKey: 0xbc, scanCode: 0x0033),
        0x37: .init(virtualKey: 0xbe, scanCode: 0x0034),
        0x38: .init(virtualKey: 0xbf, scanCode: 0x0035),
        0x39: .init(virtualKey: 0x14, scanCode: 0x003a),
        0x3a: .init(virtualKey: 0x70, scanCode: 0x003b),
        0x3b: .init(virtualKey: 0x71, scanCode: 0x003c),
        0x3c: .init(virtualKey: 0x72, scanCode: 0x003d),
        0x3d: .init(virtualKey: 0x73, scanCode: 0x003e),
        0x3e: .init(virtualKey: 0x74, scanCode: 0x003f),
        0x3f: .init(virtualKey: 0x75, scanCode: 0x0040),
        0x40: .init(virtualKey: 0x76, scanCode: 0x0041),
        0x41: .init(virtualKey: 0x77, scanCode: 0x0042),
        0x42: .init(virtualKey: 0x78, scanCode: 0x0043),
        0x43: .init(virtualKey: 0x79, scanCode: 0x0044),
        0x44: .init(virtualKey: 0x7a, scanCode: 0x0057),
        0x45: .init(virtualKey: 0x7b, scanCode: 0x0058),
        0x46: .init(virtualKey: 0x2c, scanCode: 0xe037),
        0x47: .init(virtualKey: 0x91, scanCode: 0x0046),
        0x48: .init(virtualKey: 0x13, scanCode: 0x0045),
        0x49: .init(virtualKey: 0x2d, scanCode: 0xe052),
        0x4a: .init(virtualKey: 0x24, scanCode: 0xe047),
        0x4b: .init(virtualKey: 0x21, scanCode: 0xe049),
        0x4c: .init(virtualKey: 0x2e, scanCode: 0xe053),
        0x4d: .init(virtualKey: 0x23, scanCode: 0xe04f),
        0x4e: .init(virtualKey: 0x22, scanCode: 0xe051),
        0x4f: .init(virtualKey: 0x27, scanCode: 0xe04d),
        0x50: .init(virtualKey: 0x25, scanCode: 0xe04b),
        0x51: .init(virtualKey: 0x28, scanCode: 0xe050),
        0x52: .init(virtualKey: 0x26, scanCode: 0xe048),
        0x58: .init(virtualKey: 0x0d, scanCode: 0xe01c),
        0x65: .init(virtualKey: 0x5d, scanCode: 0xe05d),
        0xe0: .init(virtualKey: 0xa2, scanCode: 0x001d),
        0xe1: .init(virtualKey: 0xa0, scanCode: 0x002a),
        0xe2: .init(virtualKey: 0xa4, scanCode: 0x0038),
        0xe3: .init(virtualKey: 0x5b, scanCode: 0xe05b),
        0xe4: .init(virtualKey: 0xa3, scanCode: 0xe01d),
        0xe5: .init(virtualKey: 0xa1, scanCode: 0x0036),
        0xe6: .init(virtualKey: 0xa5, scanCode: 0xe038),
        0xe7: .init(virtualKey: 0x5c, scanCode: 0xe05c)
    ]

    static func mapping(for keyCode: UIKeyboardHIDUsage) -> NativeStreamKeyboardMapping? {
        mappingByHID[Int(keyCode.rawValue)]
    }

    static func modifiers(for flags: UIKeyModifierFlags) -> UInt16 {
        var value: UInt16 = 0
        if flags.contains(.shift) { value |= 0x01 }
        if flags.contains(.control) { value |= 0x02 }
        if flags.contains(.alternate) { value |= 0x04 }
        if flags.contains(.command) { value |= 0x08 }
        if flags.contains(.alphaShift) { value |= 0x10 }
        return value
    }
}

private struct NativeStreamRumbleCommand {
    let controllerId: Int
    let weakMagnitude: Int
    let strongMagnitude: Int
}

private enum NativeStreamHapticsParser {
    static func parse(_ data: Data) -> NativeStreamRumbleCommand? {
        let bytes = [UInt8](data)
        guard bytes.count >= 2 else { return nil }
        let firstWord = uint16LE(bytes, 0)
        if firstWord == 267 {
            return parseLegacy(bytes, offset: 2)
        }
        switch firstWord & 0xff {
        case 34:
            return parseSubMessage(bytes, offset: 1)
        case 32, 33, 35, 36, 255:
            return nil
        default:
            return parseLegacy(bytes, offset: 0)
        }
    }

    private static func parseSubMessage(_ bytes: [UInt8], offset: Int) -> NativeStreamRumbleCommand? {
        guard offset >= 0, offset + 4 <= bytes.count else { return nil }
        let type = uint32LE(bytes, offset)
        if type == 267 {
            return parseLegacy(bytes, offset: offset + 4)
        }
        if type == 17 {
            return parseOc(bytes, offset: offset + 4)
        }
        return nil
    }

    private static func parseLegacy(_ bytes: [UInt8], offset: Int) -> NativeStreamRumbleCommand? {
        guard offset >= 0, offset + 10 <= bytes.count else { return nil }
        guard uint16LE(bytes, offset) == 1 else { return nil }
        guard uint16LE(bytes, offset + 2) >= 6 else { return nil }
        return NativeStreamRumbleCommand(
            controllerId: Int(uint16LE(bytes, offset + 4)),
            weakMagnitude: Int(uint16LE(bytes, offset + 6)),
            strongMagnitude: Int(uint16LE(bytes, offset + 8))
        )
    }

    private static func parseOc(_ bytes: [UInt8], offset: Int) -> NativeStreamRumbleCommand? {
        guard offset >= 0, offset + 9 <= bytes.count else { return nil }
        let controllerByte = Int(bytes[offset])
        guard controllerByte >= 6, controllerByte < 10 else { return nil }
        let reportKind = bytes[offset + 3]
        let flags = bytes[offset + 4]
        guard reportKind == 5, (flags & 0xfe) == 0 else { return nil }
        return NativeStreamRumbleCommand(
            controllerId: controllerByte - 6,
            weakMagnitude: Int(bytes[offset + 7]) << 8,
            strongMagnitude: Int(bytes[offset + 8]) << 8
        )
    }

    private static func uint16LE(_ bytes: [UInt8], _ offset: Int) -> UInt16 {
        UInt16(bytes[offset]) | (UInt16(bytes[offset + 1]) << 8)
    }

    private static func uint32LE(_ bytes: [UInt8], _ offset: Int) -> UInt32 {
        UInt32(bytes[offset])
            | (UInt32(bytes[offset + 1]) << 8)
            | (UInt32(bytes[offset + 2]) << 16)
            | (UInt32(bytes[offset + 3]) << 24)
    }
}

private extension RTCRtpCodecCapability {
    func codecParameterInt(_ name: String) -> Int? {
        parameters[name].flatMap(Int.init)
    }
}
#endif
