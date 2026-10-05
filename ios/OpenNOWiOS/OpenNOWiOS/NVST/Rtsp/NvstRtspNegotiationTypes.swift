//  The value types the RTSP negotiation exchanges: what the client reserves, what can go wrong,
//  what it is asked for, and what it hands back.
//

import Foundation

/// What the client contributes to ANNOUNCE: the ICE/DTLS bundle identity plus the ports of the
/// two sockets in the official cloud model (ICE/DTLS bundle + dedicated raw-SRTP Mjolnir video).
public struct NvstBundleReservation: Equatable, Sendable {
    public let bundlePort: UInt16
    public let mjolnirPort: UInt16
    /// Reserved only when audio is to arrive on its own socket rather than the bundle.
    public var audioPort: UInt16?
    public let localAddress: String?
    public let iceCredentials: NvstRtspIceCredentials?
    /// SHA-256 colon hex of the local DTLS certificate that owns the bundle socket.
    public let dtlsFingerprint: String?
    /// True only when the bundle's answer really carries the microphone send section. ANNOUNCE's
    /// `rtcMicOnNativeBundle` must mirror this — announcing mic on a bundle that does not send it
    /// repeats the SCTP-reset lesson of the audio flag.
    public var microphoneNegotiated: Bool
    /// The SSRC libwebrtc assigned to the mic sender, announced as
    /// `x-nv-mic.micSsrcConfig.senderSsrc` so the seat can map the arriving RTP to the mic stream
    /// without ever seeing the SDP that names it.
    public var microphoneSenderSsrc: UInt32?

    public init(bundlePort: UInt16,
                mjolnirPort: UInt16,
                audioPort: UInt16? = nil,
                localAddress: String? = nil,
                iceCredentials: NvstRtspIceCredentials? = nil,
                dtlsFingerprint: String? = nil,
                microphoneNegotiated: Bool = false,
                microphoneSenderSsrc: UInt32? = nil) {
        self.bundlePort = bundlePort
        self.mjolnirPort = mjolnirPort
        self.audioPort = audioPort
        self.localAddress = localAddress
        self.iceCredentials = iceCredentials
        self.dtlsFingerprint = dtlsFingerprint
        self.microphoneNegotiated = microphoneNegotiated
        self.microphoneSenderSsrc = microphoneSenderSsrc
    }
}

/// Supplies the bundle/Mjolnir sockets before video SETUP so ANNOUNCE never races a rebind.
public protocol NvstBundleReserving: Sendable {
    func reserveBundle() async throws -> NvstBundleReservation

    /// Called after SETUP and before ANNOUNCE, once the negotiated remote identity is known. A
    /// bundle that needs the seat's fingerprint and ping-derived ufrag to come up (the real
    /// ICE/DTLS bundle does) reports its actual port and fingerprint here; returning nil keeps
    /// the values from `reserveBundle()`.
    ///
    /// `microphoneOfferedOnBundle` is the seat's DESCRIBE verdict on bundle mic carriage: the
    /// bundle builds its mic send section only under that offer, because announcing bundle mic
    /// to a legacy seat makes it withhold the game audio.
    func bundleIdentity(for handoff: NVSTVideoHandoff, microphoneOfferedOnBundle: Bool) async -> NvstBundleReservation?
}

public extension NvstBundleReserving {
    func bundleIdentity(for handoff: NVSTVideoHandoff, microphoneOfferedOnBundle: Bool) async -> NvstBundleReservation? { nil }
}

public enum NvstRtspNegotiationError: LocalizedError, Equatable, Sendable {
    case missingEndpoint
    case invalidEndpoint(String)
    case requestFailed(String, Int, String)
    case missingSessionHeader
    case missingVideoControl
    case missingAudioControl
    case missingControlStream
    case missingVideoPeer
    case missingIceCredentials
    case conflictingSrtpProfile(String, String)

    public var errorDescription: String? {
        switch self {
        case .missingEndpoint: "The session did not provide an rtsps:// NVST control endpoint."
        case .invalidEndpoint(let value): "Unparseable NVST control endpoint: \(value)"
        case .requestFailed(let method, let code, let text): "RTSP \(method) failed: \(code) \(text)"
        case .missingSessionHeader: "The DESCRIBE response carried no Session header."
        case .missingVideoControl: "DESCRIBE did not advertise a video media control URI."
        case .missingAudioControl: "DESCRIBE did not advertise an audio media control URI."
        case .missingControlStream: "DESCRIBE did not advertise the primary control/0 stream."
        case .missingVideoPeer: "SETUP returned no video peer (X-GS-ServerPort/source)."
        case .missingIceCredentials: "SETUP selected ping version 6 without the ICE credential set."
        case .conflictingSrtpProfile(let described, let setup): "DESCRIBE advertised \(described) but SETUP advertised \(setup)."
        }
    }
}

/// The negotiated NVST control session. Holding it keeps the seat's RTSP session alive; releasing
/// it TEARDOWNs and closes the control channel.
public struct NvstRtspSession: Sendable {
    public let endpoint: String
    public let sessionIdentifier: String
    public let handoff: NVSTVideoHandoff
    /// Remote ICE identity for the ICE/DTLS bundle (`pingPayload + 1`, or the ping string).
    public let remoteIceUsernameFragment: String?
    public let remoteDTLSFingerprint: String?
    public let hmacSeedPresent: Bool
    public let steps: [String]
    public let release: @Sendable (String) async -> Void
    /// Latest keepalive ping/pong round trip on the control connection, in milliseconds, or -1
    /// before the first pong. The WebSocket stays open for the whole session, so this is a live
    /// network-path measurement at the keepalive cadence.
    public let controlRoundTripMilliseconds: @Sendable () async -> Double
    /// Keepalive ping/pong counters for the diagnostic log.
    public let controlKeepAliveSummary: @Sendable () async -> String
}

public struct NvstRtspNegotiationInput: Sendable {
    public let sessionID: String
    public let rtspsEndpoints: [String]
    public let resolution: String?
    public let fps: Int?
    public let codec: String?
    /// Announced as `video[0].initialBitrateKbps` / `initialPeakBitrateKbps`.
    public let bitrateKbps: Int?
    /// The configured ceiling, announced as `vqos[0].bw.maximumBitrateKbps`.
    public let maximumBitrateKbps: Int?
    /// Server-side AI sharpen/denoise, announced as `x-nv-video[0].prefilterParams.*`. `mode` and
    /// `model` are captured, verified attribute names; `sharpness`/`denoise` map to the client's
    /// sliders but their wire key names (`sharpnessLevel`/`denoiseLevel`) are inferred from the
    /// same param family, not confirmed against a live capture the way the rest of this file is.
    public let prefilterMode: Int?
    public let prefilterSharpness: Int?
    public let prefilterDenoise: Int?
    public let prefilterModel: Int?
    /// The session's colour tier string (`10bit_420`, `10bit_444`, `8bit_420`...). Announced as
    /// `video[0].bitDepth` and `video[0].chromaFormat`; nil leaves the captured values in place.
    public let colorQuality: String?
    public let hdrEnabled: Bool?
    /// Playback channels the bundle's audio section was built for (2, 6 or 8); announced as the
    /// `x-nv-audio.surround` block when above 2.
    public let audioChannelCount: Int
    public let timeout: Duration
    /// `general.rtcpOnSctp`. True routes RTCP feedback onto the bundle's
    /// `rtcp_on_sctp_private` data channel; false keeps it as SRTCP on the Mjolnir socket,
    /// which is the only option until the ICE/DTLS bundle is up.
    public let rtcpOnSctp: Bool
    /// Ignores the seat's `general.nativeRtcOnBundlePort` and negotiates the pre-bundle shape:
    /// SETUP with a real `X-GS-ClientPort`, ANNOUNCE with real `clientPorts.*`, and no
    /// `nativeRtcOnBundlePort`. The legacy path needs no DTLS at all, so it isolates "video needs
    /// the bundle" from "video needs something else".
    public let forcesLegacyPath: Bool
    /// The seat's one-way-delay rate controller stays on by default; its OWD evidence rides the
    /// `0x207` QoS reports this client sends. `OPN_NVST_OWD_CC=0` disables it.
    public let disablesOwdCongestionControl: Bool
    /// `OPN_NVST_ANNOUNCE_EXTENDED=1`: adds the official client's encoder tuning and timers.
    public let announcesExtendedSettings: Bool
    /// `OPN_NVST_ANNOUNCE_ECHO_OFFER=1`: lets the seat's offer override our announced values.
    public let echoesOfferedAttributes: Bool
    /// The client's VSync mode, announced as `video[0].framePacing.mode` / `feedbackMode`.
    /// Nil keeps the captured baseline, which is the Adaptive announce.
    public let vsyncMode: NvstVsyncMode?
    /// Attributes applied after every other layer, verbatim. The encoder-knob A/B harness
    /// (`OPN_NVST_ANNOUNCE_OVERRIDES`, see `NvstBifrostFreeTransport.announceOverridesFromEnvironment`);
    /// empty in normal operation.
    public let announceOverrides: [(String, String)]

    public init(sessionID: String,
                rtspsEndpoints: [String],
                resolution: String? = nil,
                fps: Int? = nil,
                codec: String? = nil,
                bitrateKbps: Int? = nil,
                maximumBitrateKbps: Int? = nil,
                prefilterMode: Int? = nil,
                prefilterSharpness: Int? = nil,
                prefilterDenoise: Int? = nil,
                prefilterModel: Int? = nil,
                colorQuality: String? = nil,
                hdrEnabled: Bool? = nil,
                audioChannelCount: Int = 2,
                timeout: Duration = .seconds(20),
                rtcpOnSctp: Bool = true,
                forcesLegacyPath: Bool = false,
                disablesOwdCongestionControl: Bool = true,
                announcesExtendedSettings: Bool = false,
                echoesOfferedAttributes: Bool = false,
                vsyncMode: NvstVsyncMode? = nil,
                announceOverrides: [(String, String)] = []) {
        self.audioChannelCount = audioChannelCount
        self.disablesOwdCongestionControl = disablesOwdCongestionControl
        self.announcesExtendedSettings = announcesExtendedSettings
        self.echoesOfferedAttributes = echoesOfferedAttributes
        self.vsyncMode = vsyncMode
        self.announceOverrides = announceOverrides
        self.sessionID = sessionID
        self.rtspsEndpoints = rtspsEndpoints
        self.resolution = resolution
        self.bitrateKbps = bitrateKbps
        self.maximumBitrateKbps = maximumBitrateKbps
        self.prefilterMode = prefilterMode
        self.prefilterSharpness = prefilterSharpness
        self.prefilterDenoise = prefilterDenoise
        self.prefilterModel = prefilterModel
        self.colorQuality = colorQuality
        self.hdrEnabled = hdrEnabled
        self.fps = fps
        self.codec = codec
        self.timeout = timeout
        self.rtcpOnSctp = rtcpOnSctp
        self.forcesLegacyPath = forcesLegacyPath
    }
}
