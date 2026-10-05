import Foundation

/// The NVST "Mjolnir" raw-SRTP video handoff, parsed from the WebSocket signaling offer's
/// `nvstSdp` payload.
///
/// Field names and semantics were independently observed on the wire and are also documented
/// by OpenNOW's MIT-licensed `native-opennow-streamer` crate (`nvst.rs`). We do not copy that
/// code; we parse the same signaling object the server sends.
///
/// Key facts (cross-verified against our own captures and upstream documentation):
/// - Legacy handoff defaults to `AeadAes256Gcm8`: NVIDIA's `sec_serv_conf_and_auth` with
///   256-bit keys maps to AES-256-GCM with an **8-byte** auth tag (not RFC 7714's 16).
/// - `pingVersion == 6` is the authenticated "version-6" handshake: the object carries the
///   complete ICE credential set (remote DTLS fingerprint, ufrag/pwd pairs) and `pingPayload`
///   `mjolnirUdpPort`.
/// - Video packets use a 16-byte RTP extension with profile `0x4753` ("GS"), flags
///   SOF (0x04) / EOF (0x02) / contains-pic-data (0x01), reassembled into H.264 Annex-B
///   access units.

public enum NVSTSrtpProfile: String, Equatable, Sendable {
    case aeadAes128Gcm = "AEAD_AES_128_GCM"
    case aeadAes128Gcm8 = "AEAD_AES_128_GCM_8"
    case aeadAes256Gcm = "AEAD_AES_256_GCM"
    case aeadAes256Gcm8 = "AEAD_AES_256_GCM_8"
    case aesCm128HmacSha1_32 = "AES_CM_128_HMAC_SHA1_32"
    case aesCm128HmacSha1_80 = "AES_CM_128_HMAC_SHA1_80"
    case aesCm256HmacSha1_32 = "AES_CM_256_HMAC_SHA1_32"
    case aesCm256HmacSha1_80 = "AES_CM_256_HMAC_SHA1_80"

    /// Auth tag length in bytes for this profile (RFC-derived, with NVIDIA's 8-byte GCM).
    public var authenticationTagLength: Int {
        switch self {
        case .aeadAes128Gcm, .aeadAes256Gcm: 16
        case .aeadAes128Gcm8, .aeadAes256Gcm8: 8
        case .aesCm128HmacSha1_32, .aesCm256HmacSha1_32: 4
        case .aesCm128HmacSha1_80, .aesCm256HmacSha1_80: 10
        }
    }

    /// Master key length required, in bytes.
    public var masterKeyLength: Int {
        switch self {
        case .aeadAes128Gcm, .aeadAes128Gcm8, .aesCm128HmacSha1_32, .aesCm128HmacSha1_80: 16
        case .aeadAes256Gcm, .aeadAes256Gcm8, .aesCm256HmacSha1_32, .aesCm256HmacSha1_80: 32
        }
    }

    /// Master salt length required, in bytes.
    public var masterSaltLength: Int {
        switch self {
        case .aeadAes128Gcm, .aeadAes128Gcm8, .aeadAes256Gcm, .aeadAes256Gcm8: 12
        case .aesCm128HmacSha1_32, .aesCm128HmacSha1_80, .aesCm256HmacSha1_32, .aesCm256HmacSha1_80: 14
        }
    }

    public static func parse(_ raw: String) -> NVSTSrtpProfile? {
        NVSTSrtpProfile(rawValue: raw.uppercased())
    }
}

public enum NVSTVideoCodec: String, Equatable, Sendable {
    case h264 = "H264"
    case hevc = "HEVC"
    case av1 = "AV1"

    public static func parse(_ raw: String) -> NVSTVideoCodec? {
        switch raw.uppercased() {
        case "H264", "H.264", "AVC", "AVC1": .h264
        case "HEVC", "H265", "H.265", "HEV1", "HVC1": .hevc
        case "AV1", "AV01": .av1
        default: nil
        }
    }
}

/// ICE credential pair + fingerprint for the authenticated version-6 handshake.
public struct NVSTHandoffIceCredentials: Equatable, Sendable {
    public let localUsernameFragment: String
    public let localPassword: String
    public let remoteUsernameFragment: String
    public let remotePassword: String
    public let remoteDTLSFingerprint: String?

    public init(localUsernameFragment: String,
                localPassword: String,
                remoteUsernameFragment: String,
                remotePassword: String,
                remoteDTLSFingerprint: String?) {
        self.localUsernameFragment = localUsernameFragment
        self.localPassword = localPassword
        self.remoteUsernameFragment = remoteUsernameFragment
        self.remotePassword = remotePassword
        self.remoteDTLSFingerprint = remoteDTLSFingerprint
    }
}

public struct NVSTVideoHandoff: Equatable, Sendable {
    public let clientUDPPort: UInt16
    public let videoPeerIP: String
    public let videoPeerPort: UInt16
    /// The seat's audio port, from the audio stream's own SETUP. Only present when audio was set up
    /// on a socket of its own rather than left to the bundle.
    public var audioPeerPort: UInt16?
    public let srtpProfile: NVSTSrtpProfile
    public let srtpAESKey: Data
    public let srtpSalt: Data
    public let codec: NVSTVideoCodec
    public let rtpPayloadType: UInt8
    public let rtpSSRC: UInt32
    public let reorderWindowPackets: Int
    public let maxAccessUnitBytes: Int
    public let timeoutMilliseconds: UInt64
    public let pingVersion: UInt8?
    public let pingPayload: String
    public let mjolnirUDPPort: UInt16?
    public let iceCredentials: NVSTHandoffIceCredentials?
    /// Seat ports the video socket punches — the whole `X-GS-ServerPort` range from SETUP.
    /// `videoPeerPort` (the first of them) stays the destination for everything else. Empty means
    /// the range was not advertised and only `videoPeerPort` is punched.
    public var videoPeerPunchPorts: [UInt16] = []

    public var effectiveVideoPeerPunchPorts: [UInt16] {
        videoPeerPunchPorts.isEmpty ? [videoPeerPort] : videoPeerPunchPorts
    }

    public init(clientUDPPort: UInt16,
                videoPeerIP: String,
                videoPeerPort: UInt16,
                srtpProfile: NVSTSrtpProfile,
                srtpAESKey: Data,
                srtpSalt: Data,
                codec: NVSTVideoCodec,
                rtpPayloadType: UInt8,
                rtpSSRC: UInt32,
                reorderWindowPackets: Int,
                maxAccessUnitBytes: Int,
                timeoutMilliseconds: UInt64,
                pingVersion: UInt8?,
                pingPayload: String,
                mjolnirUDPPort: UInt16?,
                iceCredentials: NVSTHandoffIceCredentials?) {
        self.clientUDPPort = clientUDPPort
        self.videoPeerIP = videoPeerIP
        self.videoPeerPort = videoPeerPort
        self.srtpProfile = srtpProfile
        self.srtpAESKey = srtpAESKey
        self.srtpSalt = srtpSalt
        self.codec = codec
        self.rtpPayloadType = rtpPayloadType
        self.rtpSSRC = rtpSSRC
        self.reorderWindowPackets = reorderWindowPackets
        self.maxAccessUnitBytes = maxAccessUnitBytes
        self.timeoutMilliseconds = timeoutMilliseconds
        self.pingVersion = pingVersion
        self.pingPayload = pingPayload
        self.mjolnirUDPPort = mjolnirUDPPort
        self.iceCredentials = iceCredentials
    }
}

public enum NVSTVideoHandoffError: LocalizedError, Equatable, Sendable {
    case notAnObject
    case missingField(String)
    case notUnicastPeer(String)
    case invalidHex(String)
    case invalidValue(String)

    public var errorDescription: String? {
        switch self {
        case .notAnObject: "NVST video handoff JSON is not an object."
        case .missingField(let field): "NVST video handoff is missing required field: \(field)"
        case .notUnicastPeer(let value): "NVST video peer address is not unicast: \(value)"
        case .invalidHex(let field): "NVST video handoff field is not valid hex: \(field)"
        case .invalidValue(let field): "NVST video handoff field is invalid: \(field)"
        }
    }
}

public enum NVSTVideoHandoffParser {
    private static let defaultReorderWindow = 32
    private static let defaultMaxAccessUnitBytes = 2 * 1024 * 1024
    private static let defaultTimeoutMilliseconds: UInt64 = 5_000

    /// Parses the signaling `nvstSdp` object. Tolerant where the server omits optional
    /// fields (pendulum defaults match the observed legacy handoff).
    public static func parse(_ raw: String) throws -> NVSTVideoHandoff {
        guard let data = raw.data(using: .utf8) else {
            throw NVSTVideoHandoffError.notAnObject
        }
        let object: [String: Any]
        do {
            object = try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:]
        } catch {
            throw NVSTVideoHandoffError.notAnObject
        }
        guard !object.isEmpty else { throw NVSTVideoHandoffError.notAnObject }
        return try parse(object)
    }

    public static func parse(_ object: [String: Any]) throws -> NVSTVideoHandoff {
        let clientUDPPort = try requiredUInt16(object, "clientUdpPort")
        guard clientUDPPort != 0 else { throw NVSTVideoHandoffError.invalidValue("clientUdpPort") }

        let peerIP = try requiredString(object, "videoPeerIp")
        guard isUnicastIPv4(peerIP) else { throw NVSTVideoHandoffError.notUnicastPeer(peerIP) }
        let peerPort = try requiredUInt16(object, "videoPeerPort")
        guard peerPort != 0 else { throw NVSTVideoHandoffError.invalidValue("videoPeerPort") }

        let keyHex = try requiredString(object, "srtpAesKeyHex")
        let saltHex = try requiredString(object, "srtpSaltHex")
        let profile = (optionalString(object, "srtpProfile")).flatMap(NVSTSrtpProfile.parse) ?? .aeadAes256Gcm8
        let key = try decodeFixedHex(keyHex, field: "srtpAesKeyHex", expectedLength: profile.masterKeyLength)
        let salt = try decodeFixedHex(saltHex, field: "srtpSaltHex", expectedLength: profile.masterSaltLength)

        let codec = try requiredString(object, "codec")
        guard let parsedCodec = NVSTVideoCodec.parse(codec) else {
            throw NVSTVideoHandoffError.invalidValue("codec: \(codec)")
        }
        let payloadType = try optionalUInt8(object, "rtpPayloadType") ?? 96
        let ssrc = try optionalUInt32(object, "rtpSsrc") ?? 0
        let reorder = try optionalInt(object, "reorderWindowPackets") ?? defaultReorderWindow
        let maxUnit = try optionalInt(object, "maxAccessUnitBytes") ?? defaultMaxAccessUnitBytes
        let timeout = try optionalUInt64(object, "timeoutMs") ?? defaultTimeoutMilliseconds

        let pingVersion = try optionalUInt8(object, "pingVersion")
        let pingPayload = (optionalString(object, "pingPayload")) ?? "PING"
        let mjolnirPort = try optionalUInt16(object, "mjolnirUdpPort")
        let remoteFingerprint = optionalString(object, "remoteDtlsFingerprint")

        let credentials: NVSTHandoffIceCredentials?
        if pingVersion == 6 || remoteFingerprint != nil {
            credentials = NVSTHandoffIceCredentials(
                localUsernameFragment: try requiredString(object, "localIceUsernameFragment"),
                localPassword: try requiredString(object, "localIcePassword"),
                remoteUsernameFragment: try requiredString(object, "remoteIceUsernameFragment"),
                remotePassword: try requiredString(object, "remoteIcePassword"),
                remoteDTLSFingerprint: remoteFingerprint
            )
        } else {
            credentials = nil
        }

        return NVSTVideoHandoff(
            clientUDPPort: clientUDPPort,
            videoPeerIP: peerIP,
            videoPeerPort: peerPort,
            srtpProfile: profile,
            srtpAESKey: key,
            srtpSalt: salt,
            codec: parsedCodec,
            rtpPayloadType: payloadType,
            rtpSSRC: ssrc,
            reorderWindowPackets: reorder,
            maxAccessUnitBytes: maxUnit,
            timeoutMilliseconds: timeout,
            pingVersion: pingVersion,
            pingPayload: pingPayload,
            mjolnirUDPPort: mjolnirPort,
            iceCredentials: credentials
        )
    }

    private static func requiredString(_ object: [String: Any], _ key: String) throws -> String {
        guard let value = object[key] as? String, !value.isEmpty else {
            throw NVSTVideoHandoffError.missingField(key)
        }
        return value
    }

    private static func optionalString(_ object: [String: Any], _ key: String) -> String? {
        (object[key] as? String).flatMap { $0.isEmpty ? nil : $0 }
    }

    private static func requiredUInt16(_ object: [String: Any], _ key: String) throws -> UInt16 {
        guard let number = numericValue(object[key]), number >= 0, number <= Double(UInt16.max) else {
            throw NVSTVideoHandoffError.missingField(key)
        }
        return UInt16(number)
    }

    private static func optionalUInt16(_ object: [String: Any], _ key: String) throws -> UInt16? {
        guard let number = numericValue(object[key]) else { return nil }
        guard number >= 0, number <= Double(UInt16.max) else { throw NVSTVideoHandoffError.invalidValue(key) }
        return UInt16(number)
    }

    private static func optionalUInt8(_ object: [String: Any], _ key: String) throws -> UInt8? {
        guard let number = numericValue(object[key]) else { return nil }
        guard number >= 0, number <= Double(UInt8.max) else { throw NVSTVideoHandoffError.invalidValue(key) }
        return UInt8(number)
    }

    private static func optionalUInt32(_ object: [String: Any], _ key: String) throws -> UInt32? {
        guard let number = numericValue(object[key]) else { return nil }
        guard number >= 0, number <= Double(UInt32.max) else { throw NVSTVideoHandoffError.invalidValue(key) }
        return UInt32(number)
    }

    private static func optionalInt(_ object: [String: Any], _ key: String) throws -> Int? {
        guard let number = numericValue(object[key]) else { return nil }
        guard number <= Double(Int.max), number >= Double(Int.min) else { throw NVSTVideoHandoffError.invalidValue(key) }
        return Int(number)
    }

    private static func optionalUInt64(_ object: [String: Any], _ key: String) throws -> UInt64? {
        guard let number = numericValue(object[key]) else { return nil }
        guard number >= 0, number <= Double(UInt64.max) else { throw NVSTVideoHandoffError.invalidValue(key) }
        return UInt64(number)
    }

    private static func numericValue(_ value: Any?) -> Double? {
        if let number = value as? NSNumber { return number.doubleValue }
        if let string = value as? String { return Double(string) }
        return nil
    }

    private static func isUnicastIPv4(_ text: String) -> Bool {
        guard let addr = parseIPv4(text) else { return false }
        // Reject 0.0.0.0/8, loopback, multicast/broadcast, and direct broadcast all-ones.
        let first = addr[0]
        if first == 0 || first == 127 || first >= 224 { return false }
        return addr != [255, 255, 255, 255]
    }

    private static func parseIPv4(_ text: String) -> [UInt8]? {
        let parts = text.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return nil }
        var octets: [UInt8] = []
        for part in parts {
            guard let value = UInt8(part), part == String(value) else { return nil }
            octets.append(value)
        }
        return octets
    }

    static func decodeFixedHex(_ hex: String, field: String, expectedLength: Int) throws -> Data {
        guard hex.count == expectedLength * 2, hex.allSatisfy({ $0.isHexDigit }) else {
            throw NVSTVideoHandoffError.invalidHex(field)
        }
        var data = Data(capacity: expectedLength)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else {
                throw NVSTVideoHandoffError.invalidHex(field)
            }
            data.append(byte)
            index = next
        }
        return data
    }
}
