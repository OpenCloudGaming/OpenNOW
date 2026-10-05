import Foundation

/// NVST SDP: the DESCRIBE body the seat sends and the ANNOUNCE body the client must send back.
///
/// The NVST dialect prefixes every attribute with `x-nv-` (`a=x-nv-general.…`) and groups the
/// video/vqos attributes as `x-nv-video[0].…`. Values below are the minimal allowlist the seat
/// accepts; independently observed, and documented by OpenNOW's MIT
/// `opennow-stable/src/main/platforms/gfn/nvstRtsp/sdp.ts`.
public struct NvstRtspIceCredentials: Equatable, Sendable {
    public let usernameFragment: String
    public let password: String

    public init(usernameFragment: String, password: String) {
        self.usernameFragment = usernameFragment
        self.password = password
    }
}

public struct NvstRuntimeEncryptionKey: Equatable, Sendable {
    public let aesKeyHex: String
    public let keyID: UInt32

    public init(aesKeyHex: String, keyID: UInt32) {
        self.aesKeyHex = aesKeyHex
        self.keyID = keyID
    }
}

public enum NvstRtspSdp {
    /// Drops the `general.*` port/transport block the official client never sends.
    static var suppressesClientPortBlock: Bool {
        ProcessInfo.processInfo.environment["OPN_NVST_ANNOUNCE_OFFICIAL_GENERAL"] == "1"
    }

    // MARK: - DESCRIBE parsing

    /// The three documents a DESCRIBE 200 body joins end to end, as the official client reads them.
    ///
    /// `main ;; features || offer`: the main SDP holds the bulk of the client configuration, a small
    /// features SDP **overrides** it, and the upstream media offer follows. The live DESCRIBE carries
    /// `runtime.micSrtp` and `audio.enableDynamicAudioConfig` once in each of main and features with
    /// different values, so a parser that takes the first match anywhere reads the stale one.
    public struct DescribeSections: Equatable, Sendable {
        public let main: String
        public let features: String
        public let offer: String

        /// Main then features, in the order their values must overwrite. The media offer is not
        /// configuration and is excluded.
        public var configuration: String {
            features.isEmpty ? main : main + "\n" + features
        }
    }

    public static func describeSections(_ sdp: String) -> DescribeSections {
        let mainParts = sdp.components(separatedBy: ";;")
        let main = mainParts.first ?? ""
        let afterMain = mainParts.dropFirst().joined(separator: ";;")
        let offerParts = afterMain.components(separatedBy: "||")
        let features = offerParts.first ?? ""
        let offer = offerParts.dropFirst().joined(separator: "||")
        return DescribeSections(main: main, features: features, offer: offer)
    }

    /// Reads `a=x-nv-<name>:<value>` (also accepting the unprefixed `a=<name>:<value>` form).
    ///
    /// Features win over main, which is what the official client does; taking the first match
    /// anywhere in the body would return main's superseded value.
    public static func attribute(_ sdp: String, _ name: String) -> String? {
        let sections = describeSections(sdp)
        return attribute(in: sections.features, name) ?? attribute(in: sections.main, name)
    }

    private static func attribute(in sdp: String, _ name: String) -> String? {
        let escaped = NSRegularExpression.escapedPattern(for: name)
        let value = NvstRtspMessage.firstCapture(in: sdp, pattern: "(?m)^a=(?:x-nv-)?\(escaped):([^\\r\\n]*)$")
        let trimmed = value?.trimmingCharacters(in: .whitespaces)
        return (trimmed?.isEmpty ?? true) ? nil : trimmed
    }

    /// Every `a=x-nv-<name>:<value>` in the seat's offer, in offer order.
    ///
    /// The seat states its own preferences in DESCRIBE (`videoSplitEncodeStripsPerFrame:64`,
    /// `pingIntervalAfterConnectionMs:100`, …). The official client merges those into its config
    /// before building the ANNOUNCE, so contradicting them with a hardcoded value is a bug: the
    /// answer has to agree with the offer on everything only the seat knows.
    public static func offeredAttributes(_ sdp: String) -> [(String, String)] {
        var result: [(String, String)] = []
        // Configuration only: the media offer is not the seat's config, and echoing it back is how
        // the seat came to drop DTLS on an oversized ANNOUNCE.
        for rawLine in describeSections(sdp).configuration.components(separatedBy: .newlines) {
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            guard line.hasPrefix("a=x-nv-") else { continue }
            let body = line.dropFirst("a=".count)
            guard let separator = body.firstIndex(of: ":") else { continue }
            let name = String(body[body.startIndex..<separator])
            let value = String(body[body.index(after: separator)...]).trimmingCharacters(in: .whitespaces)
            guard !name.isEmpty, !value.isEmpty else { continue }
            result.append((name, value))
        }
        return result
    }

    /// `a=control:` under the requested `m=<mediaType>` section.
    public static func mediaControl(_ sdp: String, mediaType: String) -> String? {
        var current: String?
        for rawLine in sdp.components(separatedBy: .newlines) {
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("m=") {
                current = line.dropFirst(2).split(separator: " ").first.map { $0.lowercased() }
                continue
            }
            guard current == mediaType.lowercased(), line.hasPrefix("a=control:") else { continue }
            let control = line.dropFirst("a=control:".count).trimmingCharacters(in: .whitespaces)
            if !control.isEmpty, control != "*" { return control }
        }
        return nil
    }

    public static func allMediaControls(_ sdp: String) -> [String] {
        sdp.components(separatedBy: .newlines).compactMap { rawLine in
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            guard line.hasPrefix("a=control:") else { return nil }
            let control = line.dropFirst("a=control:".count).trimmingCharacters(in: .whitespaces)
            return control.isEmpty ? nil : control
        }
    }

    /// The primary `control/0` stream must be advertised or the seat is not NVST-capable.
    public static func primaryControlStream(_ controls: [String]) -> String? {
        controls.first { control in
            NvstRtspMessage.firstCapture(in: control, pattern: "(?:^|[=/])(control/0)(?:/|$)") != nil
        }
    }

    public static func hmacSeed(_ sdp: String) -> String? {
        NvstRtspMessage.firstCapture(in: sdp, pattern: "(?m)^k=HMAC:([0-9A-Fa-f]{64})\\s*$")
    }

    /// DESCRIBE ICE credentials: V1 (`iceUsernameFragment`/`iceUsernamePwd`) or V2.
    public static func iceCredentials(_ sdp: String) -> NvstRtspIceCredentials? {
        let ufrag = attribute(sdp, "general.iceUsernameFragment")
            ?? NvstRtspMessage.firstCapture(in: sdp, pattern: "(?m)^a=ice-ufrag:([^\\r\\n]+)$")?.trimmingCharacters(in: .whitespaces)
            ?? attribute(sdp, "general.iceUserNameFragmentV2")
        let password = attribute(sdp, "general.iceUsernamePwd")
            ?? NvstRtspMessage.firstCapture(in: sdp, pattern: "(?m)^a=ice-pwd:([^\\r\\n]+)$")?.trimmingCharacters(in: .whitespaces)
            ?? attribute(sdp, "general.icePasswordV2")
        guard let ufrag, !ufrag.isEmpty, let password, !password.isEmpty else { return nil }
        return NvstRtspIceCredentials(usernameFragment: ufrag, password: password)
    }

    /// `runtime.encryptionKey` + `runtime.encryptionKeyId`. The seat writes the id as a signed
    /// i32; the salt derivation uses its unsigned form.
    public static func runtimeEncryptionKey(_ sdp: String) -> NvstRuntimeEncryptionKey? {
        guard let key = NvstRtspMessage.firstCapture(in: sdp, pattern: "(?m)^a=x-nv-runtime\\.encryptionKey:([0-9A-Fa-f]{64})\\s*$"),
              let idText = NvstRtspMessage.firstCapture(in: sdp, pattern: "(?m)^a=x-nv-runtime\\.encryptionKeyId:(-?[0-9]+)\\s*$"),
              let signed = Int64(idText) else { return nil }
        let unsigned = signed < 0 ? UInt32(truncatingIfNeeded: signed) : UInt32(truncatingIfNeeded: signed)
        return NvstRuntimeEncryptionKey(aesKeyHex: key.uppercased(), keyID: unsigned)
    }

    // MARK: - Key material

    /// libsrtp master salt = the unsigned key id rendered as a 12-byte big-endian hex string.
    public static func srtpSaltHex(keyID: UInt32) -> String {
        String(format: "%024X", keyID)
    }

    /// libsrtp master key||salt (AES-256 key + 12-byte salt = 88 hex chars).
    public static func packMasterKeySalt(aesKeyHex: String, keyID: UInt32) throws -> String {
        let key = aesKeyHex.trimmingCharacters(in: .whitespaces).uppercased()
        guard key.count == 64, key.allSatisfy({ $0.isHexDigit }) else {
            throw NVSTVideoHandoffError.invalidHex("runtime.encryptionKey")
        }
        return key + srtpSaltHex(keyID: keyID)
    }

    /// Official Bifrost always client-generates `runtime.encryptionKey` for ANNOUNCE; the
    /// video SRTP socket is keyed by this runtime key, never by DTLS-SRTP.
    public static func generateClientEncryptionKey() -> NvstRuntimeEncryptionKey {
        var key = Data(capacity: 32)
        for _ in 0..<32 { key.append(UInt8.random(in: 0...255)) }
        let hex = key.map { String(format: "%02X", $0) }.joined()
        return NvstRuntimeEncryptionKey(aesKeyHex: hex, keyID: UInt32.random(in: 0...UInt32.max))
    }

    /// Official `GenerateIceCredentials()`: 4-char ufrag + 22-char password over the base64
    /// alphabet. Bifrost length-checks reject the 16-char ufrag stock WebRTC stacks emit.
    public static func generateIceCredentials() -> NvstRtspIceCredentials {
        let alphabet = Array("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/")
        func encode(_ length: Int) -> String {
            String((0..<length).map { _ in alphabet[Int(UInt8.random(in: 0...255)) & 0x3f] })
        }
        return NvstRtspIceCredentials(usernameFragment: encode(4), password: encode(22))
    }

    // MARK: - SRTP profile advertisement

    private static let knownProfiles: [String] = [
        "AEAD_AES_128_GCM",
        "AEAD_AES_256_GCM",
        "AES_CM_128_HMAC_SHA1_32",
        "AES_CM_128_HMAC_SHA1_80",
        "AES_CM_256_HMAC_SHA1_32",
        "AES_CM_256_HMAC_SHA1_80",
    ]

    private static func findProfile(_ value: String) -> NVSTSrtpProfile? {
        let upper = value.uppercased()
        for token in upper.components(separatedBy: CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "_")).inverted) {
            guard knownProfiles.contains(token) else { continue }
            return NVSTSrtpProfile.parse(token)
        }
        return nil
    }

    public static func advertisedSrtpProfile(sdp: String) -> NVSTSrtpProfile? {
        for rawLine in sdp.components(separatedBy: .newlines) {
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            if NvstRtspMessage.firstCapture(in: line, pattern: "^(a=crypto:[0-9]+)\\s+") != nil {
                if let profile = findProfile(line) { return profile }
                continue
            }
            guard let colon = line.firstIndex(of: ":"), line.hasPrefix("a=") else { continue }
            let name = String(line[line.index(line.startIndex, offsetBy: 2)..<colon])
            guard matchesProfileAttribute(name) else { continue }
            if let profile = findProfile(String(line[line.index(after: colon)...])) { return profile }
        }
        return nil
    }

    public static func advertisedSrtpProfile(headers: [String: String]) -> NVSTSrtpProfile? {
        for (name, value) in headers {
            guard name.lowercased() == "transport" || matchesProfileAttribute(name) else { continue }
            if let profile = findProfile(value) { return profile }
        }
        return nil
    }

    private static func matchesProfileAttribute(_ name: String) -> Bool {
        let lower = name.lowercased()
        guard lower.contains("srtp") || lower.contains("crypto") else { return false }
        guard lower.contains("profile") || lower.contains("suite") else { return false }
        return !(lower.contains("supported") || lower.contains("capabilit"))
    }

    // MARK: - ANNOUNCE

    public struct AnnounceOptions: Sendable {
        public var resolution: String?
        public var fps: Int?
        /// Server-side AI sharpen/denoise (GFN's "Clarity"/"Noise Reduction"). The captured base
        /// announce hardcodes `prefilterMode:2`/`prefilterModel:4` on every video index; these
        /// override index 0 the same way `bitrateKbps`/viewport do below. `nil` leaves the captured
        /// value in place.
        public var prefilterMode: Int?
        public var prefilterSharpness: Int?
        public var prefilterDenoise: Int?
        public var prefilterModel: Int?
        /// Sample depth (8 or 10) and chroma layout announced as `video[0].bitDepth` /
        /// `video[0].chromaFormat`. `chromaFormat` follows `chroma_format_idc` (1 = 4:2:0,
        /// 2 = 4:2:2, 3 = 4:4:4): the vendor capture of a `10bit_420` session carries
        /// `bitDepth:10 chromaFormat:1`, which is the only value observed on the wire; 4:4:4
        /// as 3 is inferred from that convention, not captured. nil leaves the captured values.
        public var bitDepth: Int?
        public var chromaFormat: Int?
        public var hdrEnabled: Bool?
        /// Playback channels the bundle's audio section decodes (2, 6 or 8). Above 2 the announce
        /// carries `x-nv-audio.surround.enable:1` with the channel count and speaker mask so the
        /// seat encodes multi-channel Opus; the vendor stack names exactly these attributes.
        public var audioChannelCount: Int
        public var encryptionKey: NvstRuntimeEncryptionKey?
        public var iceCredentials: NvstRtspIceCredentials?
        public var videoPort: UInt16
        public var clientBundlePort: UInt16?
        /// The port the seat should send Opus audio to when audio is not riding the bundle. Set
        /// together with `carriesAudioOnBundle == false`.
        public var clientAudioPort: UInt16?
        /// Legacy path only: the client port video actually arrives on (the Mjolnir socket).
        public var clientVideoPort: UInt16?
        public var localAddress: String?
        public var dtlsFingerprint: String?
        /// Official cloud path (`general.nativeRtcOnBundlePort:1`): every legacy `clientPorts.*`
        /// is announced as 0 and the ICE/DTLS bundle port carries control/audio.
        public var officialCloudPath: Bool
        public var rtcpOnSctp: Bool
        /// What the bundle actually carries. These must match the media sections the bundle's
        /// answer really offers: announcing audio on the bundle and then presenting a
        /// data-channel-only answer makes the seat reset every SCTP stream and close DTLS.
        public var carriesAudioOnBundle: Bool
        public var carriesMicrophoneOnBundle: Bool
        public var carriesDataChannelOnBundle: Bool
        /// The mic sender SSRC the bundle's answer assigned, announced as
        /// `x-nv-mic.micSsrcConfig.senderSsrc` when `carriesMicrophoneOnBundle`. NVST has no
        /// WebRTC signaling, so this attribute is the seat's only way to learn which SSRC the
        /// mic RTP will use — the vendor client parses the mic sender SSRC for exactly this.
        public var microphoneSenderSsrc: UInt32?
        /// The seat's own `a=x-nv-*` offer, echoed back so the answer never contradicts it.
        public var offeredAttributes: [(String, String)]
        /// Drives `maxCodecProfile`/`maxCodecLevel`; the seat cannot init an encoder without them.
        public var codec: NVSTVideoCodec?
        public var bitrateKbps: Int?
        /// The user's configured ceiling. The captured base announces 100000, which is NVIDIA's own
        /// default rather than anything this client chose, so a configured cap has to replace it or
        /// the setting does nothing on this path.
        public var maximumBitrateKbps: Int?
        /// Adds the official client's encoder tuning and timer block. Opt-in: a larger ANNOUNCE has
        /// been observed to make the seat close DTLS immediately, so it is isolated from the
        /// encoder identity that a stream actually needs.
        /// The seat's one-way-delay rate controller stays on by default (the vendor shape);
        /// `OPN_NVST_OWD_CC=0` disables it and parks the encoder on the loss-based fallback.
        public var disablesOwdCongestionControl: Bool
        public var announcesExtendedSettings: Bool
        /// Lets the seat's DESCRIBE offer override our value for keys we already send.
        public var echoesOfferedAttributes: Bool
        /// The client's VSync mode, overriding the captured baseline's
        /// `framePacing.mode:1` / `framePacing.feedbackMode:1` pair (which is the Adaptive
        /// announce — the official client's profiles all pin `vSync` to Adaptive and the capture
        /// never left it). Nil keeps the captured baseline, matching today's behaviour.
        public var vsyncMode: NvstVsyncMode?
        /// Applied last, verbatim: the A/B harness for encoder knobs. Empty in normal operation.
        public var announceOverrides: [(String, String)]

        public init(resolution: String? = nil,
                    fps: Int? = nil,
                    prefilterMode: Int? = nil,
                    prefilterSharpness: Int? = nil,
                    prefilterDenoise: Int? = nil,
                    prefilterModel: Int? = nil,
                    bitDepth: Int? = nil,
                    chromaFormat: Int? = nil,
                    hdrEnabled: Bool? = nil,
                    audioChannelCount: Int = 2,
                    encryptionKey: NvstRuntimeEncryptionKey? = nil,
                    iceCredentials: NvstRtspIceCredentials? = nil,
                    videoPort: UInt16 = 0,
                    clientBundlePort: UInt16? = nil,
                    clientVideoPort: UInt16? = nil,
                    clientAudioPort: UInt16? = nil,
                    localAddress: String? = nil,
                    dtlsFingerprint: String? = nil,
                    officialCloudPath: Bool = true,
                    rtcpOnSctp: Bool = true,
                    carriesAudioOnBundle: Bool = true,
                    carriesMicrophoneOnBundle: Bool = false,
                    carriesDataChannelOnBundle: Bool = true,
                    microphoneSenderSsrc: UInt32? = nil,
                    offeredAttributes: [(String, String)] = [],
                    codec: NVSTVideoCodec? = nil,
                    bitrateKbps: Int? = nil,
                    maximumBitrateKbps: Int? = nil,
                    disablesOwdCongestionControl: Bool = true,
                    announcesExtendedSettings: Bool = false,
                    echoesOfferedAttributes: Bool = false,
                    vsyncMode: NvstVsyncMode? = nil,
                    announceOverrides: [(String, String)] = []) {
            self.disablesOwdCongestionControl = disablesOwdCongestionControl
            self.announcesExtendedSettings = announcesExtendedSettings
            self.echoesOfferedAttributes = echoesOfferedAttributes
            self.vsyncMode = vsyncMode
            self.announceOverrides = announceOverrides
            self.offeredAttributes = offeredAttributes
            self.codec = codec
            self.bitrateKbps = bitrateKbps
            self.maximumBitrateKbps = maximumBitrateKbps
            self.resolution = resolution
            self.fps = fps
            self.prefilterMode = prefilterMode
            self.prefilterSharpness = prefilterSharpness
            self.prefilterDenoise = prefilterDenoise
            self.prefilterModel = prefilterModel
            self.bitDepth = bitDepth
            self.chromaFormat = chromaFormat
            self.hdrEnabled = hdrEnabled
            self.audioChannelCount = audioChannelCount
            self.encryptionKey = encryptionKey
            self.iceCredentials = iceCredentials
            self.videoPort = videoPort
            self.clientBundlePort = clientBundlePort
            self.clientVideoPort = clientVideoPort
            self.localAddress = localAddress
            self.dtlsFingerprint = dtlsFingerprint
            self.officialCloudPath = officialCloudPath
            self.rtcpOnSctp = rtcpOnSctp
            self.clientAudioPort = clientAudioPort
            self.carriesAudioOnBundle = carriesAudioOnBundle
            self.carriesMicrophoneOnBundle = carriesMicrophoneOnBundle
            self.carriesDataChannelOnBundle = carriesDataChannelOnBundle
            self.microphoneSenderSsrc = microphoneSenderSsrc
        }
    }




    /// `vqos[0].bitStreamFormat`. Only HEVC is measured (the official client resolves 1 for an
    /// HEVC session); H.264 and AV1 follow the codec order either side of it.
    static func bitStreamFormat(_ codec: NVSTVideoCodec) -> String {
        switch codec {
        case .h264: "0"
        case .hevc: "1"
        case .av1: "2"
        }
    }

    /// `maxCodecProfile` / `maxCodecLevel` as the official client resolves them.
    static func codecProfileAndLevel(_ codec: NVSTVideoCodec) -> (String, String) {
        // The captured client reports profile 3 / level 61 across codecs; `maxH264*` carries the
        // same pair, so the seat reads one shape whichever codec the session negotiated.
        switch codec {
        case .h264, .hevc, .av1: ("3", "61")
        }
    }

    static func parseResolution(_ resolution: String?) -> (Int, Int) {
        guard let text = resolution?.trimmingCharacters(in: .whitespaces),
              let width = NvstRtspMessage.firstCapture(in: text, pattern: "^([0-9]+)\\s*[xX]\\s*[0-9]+$").flatMap(Int.init),
              let height = NvstRtspMessage.firstCapture(in: text, pattern: "^[0-9]+\\s*[xX]\\s*([0-9]+)$").flatMap(Int.init) else {
            return (1920, 1080)
        }
        return (width, height)
    }
}
