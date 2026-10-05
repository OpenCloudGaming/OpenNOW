import Foundation
import Testing
@testable import OpenNOWiOS

/// The NVST control plane is classic RTSP tunnelled over a WebSocket on `:322`. These tests pin
/// the wire shapes we cannot re-derive at runtime: the Poco-shaped upgrade, the RTSP framing, the
/// DESCRIBE attribute dialect, the ANNOUNCE allowlist, and the ping-payload identity transforms.
@Suite struct NvstRtspWireFormatTests {
    @Test func upgradeRequestMatchesTheOfficialPocoShape() {
        let request = NvstWebSocketUpgrade.request(host: "seat.example.com", port: 322, secWebSocketKey: "dGhlIHNhbXBsZSBub25jZQ==", sessionID: " abc-123 ")
        let lines = request.components(separatedBy: "\r\n")
        #expect(lines[0] == "GET /rtsp HTTP/1.1")
        #expect(lines.contains("Host: seat.example.com:322"))
        #expect(lines.contains("Connection: Upgrade"))
        #expect(lines.contains("Upgrade: websocket"))
        #expect(lines.contains("Sec-WebSocket-Version: 13"))
        #expect(lines.contains("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ=="))
        // Bifrost presets Content-Length: 0 on the upgrade.
        #expect(lines.contains("Content-Length: 0"))
        #expect(lines.contains("x-nv-sessionid: abc-123"))
        #expect(request.hasSuffix("\r\n\r\n"))
    }

    @Test func upgradeAcceptMatchesTheRfc6455KnownAnswer() {
        // RFC 6455 §1.3 worked example.
        #expect(NvstWebSocketUpgrade.expectedAccept(for: "dGhlIHNhbXBsZSBub25jZQ==") == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
    }

    @Test func upgradeResponseParsingSplitsHeadersAndLeftoverFrameBytes() {
        var buffer = Data("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: abc\r\n\r\n".utf8)
        buffer.append(contentsOf: [0x81, 0x02, 0x41, 0x42])
        let parsed = NvstWebSocketUpgrade.parseResponse(buffer)
        #expect(parsed?.statusCode == 101)
        #expect(parsed?.headers["sec-websocket-accept"] == "abc")
        #expect(parsed?.leftover == Data([0x81, 0x02, 0x41, 0x42]))
        // Incomplete header blocks must not parse.
        #expect(NvstWebSocketUpgrade.parseResponse(Data("HTTP/1.1 101 Switching".utf8)) == nil)
    }

    @Test func maskedClientFramesRoundTripThroughTheReader() throws {
        var reader = NvstWebSocketFrameReader()
        let short = Data("OPTIONS rtsps://host RTSP/1.0\r\n\r\n".utf8)
        let medium = Data(repeating: 0x41, count: 400)
        let frames = NvstWebSocketFrame.encodeText(short) + NvstWebSocketFrame.encodeText(medium)
        // Split mid-stream to exercise the incremental path.
        let firstHalf = frames.prefix(frames.count / 3)
        let secondHalf = frames.dropFirst(frames.count / 3)
        var messages = try reader.push(Data(firstHalf))
        messages += try reader.push(Data(secondHalf))
        #expect(messages == [short, medium])
    }

    @Test func encodedFrameSetsFinMaskAndExtendedLength() {
        let short = NvstWebSocketFrame.encodeText(Data(repeating: 0x20, count: 5))
        #expect(short[0] == 0x81)
        #expect(short[1] == 0x85)
        let medium = NvstWebSocketFrame.encodeText(Data(repeating: 0x20, count: 300))
        #expect(medium[1] == 0xfe)
        #expect(Int(medium[2]) << 8 | Int(medium[3]) == 300)
    }

    @Test func closeFrameSurfacesAsAnError() {
        var reader = NvstWebSocketFrameReader()
        #expect(throws: NvstWebSocketFrameError.closed) {
            _ = try reader.push(Data([0x88, 0x00]))
        }
    }

    @Test func requestBuilderEmitsCseqRequestIdAndPreservesEmptyHeaders() {
        let message = NvstRtspMessage.buildRequest(
            method: "SETUP",
            uri: "rtsps://host:322/streamid=video/0/0",
            headers: [("Session", "abc"), ("x-nv-ping", "6"), ("Transport", "")],
            cseq: 3
        )
        #expect(message.hasPrefix("SETUP rtsps://host:322/streamid=video/0/0 RTSP/1.0\r\n"))
        #expect(message.contains("CSeq: 3\r\n"))
        #expect(message.contains("Request-Id: 3\r\n"))
        // The official cloud SETUP sends a literally empty Transport header.
        #expect(message.contains("Transport: \r\n"))
        #expect(!message.contains("Content-Length"))
    }

    @Test func requestBuilderAddsContentLengthOnlyForBodies() {
        let message = NvstRtspMessage.buildRequest(method: "ANNOUNCE", uri: "rtsps://host:322", body: "v=0\r\n", cseq: 4)
        #expect(message.contains("Content-Length: 5\r\n"))
        #expect(message.hasSuffix("\r\n\r\nv=0\r\n"))
    }

    @Test func responseParsingHandlesBothLineEndingsAndLowercasesHeaders() throws {
        let response = try NvstRtspMessage.parseResponse("RTSP/1.0 200 OK\r\nSession: 1234;timeout=60\r\nContent-Length: 4\r\n\r\nv=0\n")
        #expect(response.statusCode == 200)
        #expect(response.statusText == "OK")
        #expect(response.header("SESSION") == "1234;timeout=60")
        #expect(response.body == "v=0\n")
    }

    @Test func responseExtractionWaitsForTheFullBody() throws {
        var buffer = Data("RTSP/1.0 200 OK\r\nContent-Length: 8\r\n\r\nv=0".utf8)
        #expect(try NvstRtspMessage.extractResponse(from: &buffer) == nil)
        buffer.append(Data("\r\ns=x".utf8))
        let response = try NvstRtspMessage.extractResponse(from: &buffer)
        #expect(response?.statusCode == 200)
        #expect(buffer.isEmpty)
    }

    @Test func responseExtractionRejectsGarbageStatusLines() {
        var buffer = Data("NOT-RTSP\r\n\r\n".utf8)
        #expect(throws: NvstRtspMessageError.self) {
            _ = try NvstRtspMessage.extractResponse(from: &buffer)
        }
    }

    /// A negative length used to make `headerByteLength + contentLength` go negative, and both
    /// `Data.prefix` and `Data.removeFirst` trap on that — one hostile response crashed the client.
    @Test func responseExtractionRejectsNegativeContentLength() {
        var buffer = Data("RTSP/1.0 200 OK\r\nContent-Length: -100000\r\n\r\nbody".utf8)
        #expect(throws: NvstRtspMessageError.invalidContentLength(-100_000)) {
            _ = try NvstRtspMessage.extractResponse(from: &buffer)
        }
    }

    @Test func responseExtractionRejectsAbsurdContentLength() {
        var buffer = Data("RTSP/1.0 200 OK\r\nContent-Length: 999999999\r\n\r\n".utf8)
        #expect(throws: NvstRtspMessageError.self) {
            _ = try NvstRtspMessage.extractResponse(from: &buffer)
        }
    }

    /// The ANNOUNCE body is logged line by line into sinks that outlive the session (a file in
    /// `~/Library/Logs`, the unified log, Sentry, the uploadable diagnostics bundle). The SRTP
    /// master key and the ICE passwords must never reach them; the key ID and ufrag are not secret.
    @Test func announceLogRedactionStripsKeyMaterialButKeepsIdentifiers() {
        #expect(NvstRtspSdp.redactedForLog("a=x-nv-runtime.encryptionKey:8F3A2B1C4D5E6F708192A3B4C5D6E7F8")
            == "a=x-nv-runtime.encryptionKey:[redacted-secret]")
        #expect(NvstRtspSdp.redactedForLog("a=x-nv-general.icePasswordV2:Xy9pQz1aBc2d")
            == "a=x-nv-general.icePasswordV2:[redacted-secret]")
        #expect(NvstRtspSdp.redactedForLog("a=x-nv-general.iceUsernamePwd:Xy9pQz1aBc2d")
            == "a=x-nv-general.iceUsernamePwd:[redacted-secret]")
        #expect(NvstRtspSdp.redactedForLog("a=x-nv-runtime.encryptionKeyId:42")
            == "a=x-nv-runtime.encryptionKeyId:42")
        #expect(NvstRtspSdp.redactedForLog("a=x-nv-general.iceUsernameFragment:abcd")
            == "a=x-nv-general.iceUsernameFragment:abcd")
        #expect(NvstRtspSdp.redactedForLog("a=x-nv-video[0].maxFPS:120")
            == "a=x-nv-video[0].maxFPS:120")
    }

    /// Every secret-bearing attribute the announce builder emits has to be covered by the redactor.
    @Test func everySecretAnnounceAttributeIsRedacted() {
        let key = "8F3A2B1C4D5E6F708192A3B4C5D6E7F8091A2B3C4D5E6F708192A3B4C5D6E7F8"
        let password = "IcePasswordValue123"
        // `officialCloudPath: false` so both the legacy `iceUsernamePwd` and `icePasswordV2` are
        // emitted — the legacy line is the one that only exists on this branch.
        let options = NvstRtspSdp.AnnounceOptions(
            resolution: "1920x1080",
            fps: 60,
            encryptionKey: .init(aesKeyHex: key, keyID: 7),
            iceCredentials: .init(usernameFragment: "ufragValue", password: password),
            officialCloudPath: false
        )
        let redacted = NvstRtspSdp.buildAnnounceSdp(options)
            .components(separatedBy: "\r\n")
            .map(NvstRtspSdp.redactedForLog)
            .joined(separator: "\n")
        #expect(!redacted.localizedCaseInsensitiveContains(key))
        #expect(!redacted.contains(password))
        #expect(redacted.contains("ufragValue"))
    }

    @Test func videoPeerComesFromTheSetupTransportHeader() {
        let peer = NvstRtspMessage.extractVideoPeer("unicast;X-GS-ClientPort=49000-49001;X-GS-ServerPort=48322;source=10.20.30.40")
        #expect(peer?.ip == "10.20.30.40")
        #expect(peer?.port == 48_322)
        #expect(NvstRtspMessage.extractVideoPeer("unicast;X-GS-ServerPort=48322") == nil)
        #expect(NvstRtspMessage.extractVideoPeer(nil) == nil)
    }

    @Test func videoPunchPortsCoverTheWholeServerPortRange() {
        // Every captured seat advertises a range; the punch has to open all of it.
        #expect(NvstRtspMessage.extractVideoPeerPorts("unicast;X-GS-ServerPort=5004-5005;source=66.22.144.48") == [5004, 5005])
        #expect(NvstRtspMessage.extractVideoPeerPorts("unicast;X-GS-ServerPort=48322;source=10.20.30.40") == [48_322])
        // An inverted or absurdly wide range falls back to the first port rather than spraying.
        #expect(NvstRtspMessage.extractVideoPeerPorts("X-GS-ServerPort=5005-5004") == [5005])
        #expect(NvstRtspMessage.extractVideoPeerPorts("X-GS-ServerPort=5000-6000") == [5000])
        #expect(NvstRtspMessage.extractVideoPeerPorts("unicast;source=10.20.30.40").isEmpty)
        #expect(NvstRtspMessage.extractVideoPeerPorts(nil).isEmpty)
    }
}

@Suite struct NvstRtspSdpTests {
    /// Shaped like a real DESCRIBE body: NVST `x-nv-` attribute dialect plus SDP media sections.
    static let describeBody = """
    v=0
    o=- 0 0 IN IP4 0.0.0.0
    s=NVIDIA Streaming Server
    k=HMAC:0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF
    a=x-nv-general.dtlsFingerprintV2:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99
    a=x-nv-general.iceUserNameFragmentV2:sEaT
    a=x-nv-general.icePasswordV2:seatPasswordThatIsLong
    a=x-nv-general.pingVersion:6
    a=x-nv-general.disablePlay:0
    a=x-nv-general.nativeRtcOnBundlePort:1
    a=x-nv-general.serverTransport:1
    a=x-nv-srtpProfile:AEAD_AES_256_GCM
    m=video 5004
    a=control:streamid=video/0
    m=audio 5006
    a=control:streamid=audio/0
    m=application 0
    a=control:streamid=control/0
    """.replacingOccurrences(of: "\n", with: "\r\n")

    @Test func describeAttributesAreReadThroughTheXnvPrefix() {
        let body = Self.describeBody
        #expect(NvstRtspSdp.attribute(body, "general.pingVersion") == "6")
        #expect(NvstRtspSdp.attribute(body, "general.nativeRtcOnBundlePort") == "1")
        #expect(NvstRtspSdp.attribute(body, "general.disablePlay") == "0")
        #expect(NvstRtspSdp.attribute(body, "general.dtlsFingerprintV2")?.count == 95)
        #expect(NvstRtspSdp.attribute(body, "general.missingAttribute") == nil)
    }

    /// The live DESCRIBE (2026-09-24) carries these names once in each of main and features, with
    /// different values, and the official client uses the features one. A first-match parser reads
    /// the superseded main value.
    @Test func theFeaturesDocumentOverridesTheMainDocument() {
        let body = [
            "a=x-nv-runtime.micSrtp:1",
            "a=x-nv-audio.enableDynamicAudioConfig:0",
            ";;",
            "a=x-nv-runtime.micSrtp:0",
            "a=x-nv-audio.enableDynamicAudioConfig:1",
            "||",
            "m=video 5004",
            "a=x-nv-runtime.micSrtp:1",
        ].joined(separator: "\r\n")
        let sections = NvstRtspSdp.describeSections(body)
        #expect(sections.main.contains("runtime.micSrtp:1"))
        #expect(sections.features.contains("runtime.micSrtp:0"))
        #expect(sections.offer.contains("m=video"))
        #expect(NvstRtspSdp.attribute(body, "runtime.micSrtp") == "0")
        #expect(NvstRtspSdp.attribute(body, "audio.enableDynamicAudioConfig") == "1")
    }

    @Test func aBodyWithoutSectionsStillResolvesItsAttributes() {
        #expect(NvstRtspSdp.describeSections(Self.describeBody).features.isEmpty)
        #expect(NvstRtspSdp.attribute(Self.describeBody, "general.pingVersion") == "6")
        #expect(NvstRtspSdp.attribute("a=general.disablePlay:0", "general.disablePlay") == "0")
    }

    @Test func offeredAttributesExcludeTheMediaOffer() {
        let body = [
            "a=x-nv-video[0].maxFPS:60",
            ";;",
            "a=x-nv-video[0].maxFPS:120",
            "||",
            "a=x-nv-video[0].maxFPS:240",
        ].joined(separator: "\r\n")
        let offered = NvstRtspSdp.offeredAttributes(body)
        #expect(offered.contains { $0.0 == "x-nv-video[0].maxFPS" && $0.1 == "120" })
        #expect(!offered.contains { $0.1 == "240" })
    }

    @Test func mediaControlsAreScopedToTheirMediaSection() {
        let body = Self.describeBody
        #expect(NvstRtspSdp.mediaControl(body, mediaType: "video") == "streamid=video/0")
        #expect(NvstRtspSdp.mediaControl(body, mediaType: "audio") == "streamid=audio/0")
        let controls = NvstRtspSdp.allMediaControls(body)
        #expect(controls == ["streamid=video/0", "streamid=audio/0", "streamid=control/0"])
        #expect(NvstRtspSdp.primaryControlStream(controls) == "streamid=control/0")
        #expect(NvstRtspSdp.primaryControlStream(["streamid=video/0"]) == nil)
    }

    @Test func hmacSeedAndIceCredentialsComeOutOfDescribe() {
        let body = Self.describeBody
        #expect(NvstRtspSdp.hmacSeed(body)?.count == 64)
        let ice = NvstRtspSdp.iceCredentials(body)
        #expect(ice?.usernameFragment == "sEaT")
        #expect(ice?.password == "seatPasswordThatIsLong")
    }

    @Test func advertisedSrtpProfileIsReadFromSdpAndHeaders() {
        #expect(NvstRtspSdp.advertisedSrtpProfile(sdp: Self.describeBody) == .aeadAes256Gcm)
        #expect(NvstRtspSdp.advertisedSrtpProfile(sdp: "a=crypto:1 AES_CM_128_HMAC_SHA1_80 inline:abc") == .aesCm128HmacSha1_80)
        // "supported"/"capabilities" attributes advertise what the seat *can* do, not the choice.
        #expect(NvstRtspSdp.advertisedSrtpProfile(sdp: "a=x-nv-srtpProfilesSupported:AEAD_AES_128_GCM") == nil)
        #expect(NvstRtspSdp.advertisedSrtpProfile(headers: ["transport": "unicast;srtp=AEAD_AES_128_GCM"]) == .aeadAes128Gcm)
        #expect(NvstRtspSdp.advertisedSrtpProfile(headers: ["session": "1234"]) == nil)
    }

    @Test func runtimeEncryptionKeyNormalizesTheSignedKeyIdToUnsigned() {
        let sdp = """
        a=x-nv-runtime.encryptionKey:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff
        a=x-nv-runtime.encryptionKeyId:-1
        """
        let key = NvstRtspSdp.runtimeEncryptionKey(sdp)
        #expect(key?.aesKeyHex == "00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF")
        #expect(key?.keyID == UInt32.max)
        #expect(NvstRtspSdp.runtimeEncryptionKey("a=x-nv-runtime.encryptionKey:abcd") == nil)
    }

    @Test func srtpSaltIsTheUnsignedKeyIdAsTwelveBigEndianBytes() throws {
        #expect(NvstRtspSdp.srtpSaltHex(keyID: 0) == "000000000000000000000000")
        #expect(NvstRtspSdp.srtpSaltHex(keyID: 0x1234_5678) == "000000000000000012345678")
        #expect(NvstRtspSdp.srtpSaltHex(keyID: UInt32.max) == "0000000000000000FFFFFFFF")
        let packed = try NvstRtspSdp.packMasterKeySalt(aesKeyHex: String(repeating: "ab", count: 32), keyID: 7)
        #expect(packed.count == 88)
        #expect(packed.hasSuffix("000000000000000000000007"))
        #expect(throws: NVSTVideoHandoffError.self) {
            _ = try NvstRtspSdp.packMasterKeySalt(aesKeyHex: "abcd", keyID: 7)
        }
    }

    @Test func generatedCredentialsMatchTheOfficialLengths() {
        // Bifrost length-checks reject the 16-char ufrag a stock WebRTC stack emits.
        let alphabet = Set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/")
        for _ in 0..<32 {
            let credentials = NvstRtspSdp.generateIceCredentials()
            #expect(credentials.usernameFragment.count == 4)
            #expect(credentials.password.count == 22)
            #expect(credentials.usernameFragment.allSatisfy(alphabet.contains))
            #expect(credentials.password.allSatisfy(alphabet.contains))
        }
        let key = NvstRtspSdp.generateClientEncryptionKey()
        #expect(key.aesKeyHex.count == 64)
        #expect(key.aesKeyHex.allSatisfy { $0.isHexDigit && !$0.isLowercase })
    }

    /// The seat states its encoder preferences in DESCRIBE; contradicting them with a hardcoded
    /// default is what produced `FRAME_GRAB_FAILED`.
    @Test func theAnnounceEchoesTheSeatsOfferedAttributes() {
        let offered = NvstRtspSdp.offeredAttributes(Self.describeBody)
        #expect(!offered.isEmpty)
        // The seat's value wins wherever we would otherwise send a default of our own.
        let contested = ("x-nv-video[0].videoSplitEncodeStripsPerFrame", "7")
        let body = NvstRtspSdp.buildAnnounceSdp(.init(offeredAttributes: offered + [contested],
                                                     echoesOfferedAttributes: true))
        #expect(body.contains("a=\(contested.0):\(contested.1)"))
        // Keys we never send are not echoed either: the answer stays the size of the official
        // client's, and only its own values are open to the seat's preference.
        #expect(!body.contains("a=x-nv-general.useNewIceInfo"))
    }

    /// Transport choice and credentials are the client's, so an echoed offer must not restore the
    /// cloud-path flags on a legacy negotiation.
    /// The seat's offer enumerates every stream index it could ever use. Echoing all of it made
    /// the seat drop DTLS at once, so the echo is an intersection, not a union.
    @Test func theAnnounceDoesNotEchoAttributesItNeverSends() {
        let offered = [("x-nv-video[0].chromaFormat", "1"),
                       ("x-nv-vqos[5].turboMode", "1"),
                       ("x-nv-general.someKeyWeNeverSend", "1"),
                       ("x-nv-general.customMessageOnCC", "1")]
        let body = NvstRtspSdp.buildAnnounceSdp(.init(offeredAttributes: offered,
                                                     announcesExtendedSettings: true,
                                                     echoesOfferedAttributes: true))
        #expect(body.contains("a=x-nv-video[0].chromaFormat:1"))
        #expect(body.contains("a=x-nv-general.customMessageOnCC:1"))
        // The rule that stopped the 4112-attribute explosion is the intersection: a key we do not
        // already announce is never introduced by the offer. Stream indices are not the test —
        // the captured official ANNOUNCE does carry video[1…5] and vqos[1…5].
        #expect(!body.contains("a=x-nv-general.someKeyWeNeverSend"))
        #expect(!body.contains("a=x-nv-vqos[5].turboMode"))
        // The official ANNOUNCE is 161 attributes; anything near four thousand is a bug.
        #expect(body.components(separatedBy: "a=x-nv-").count - 1 < 250)
    }

    @Test func theAnnounceNeverEchoesClientOwnedAttributes() {
        let offered = [("x-nv-general.nativeRtcOnBundlePort", "1"),
                       ("x-nv-general.clientPorts.video", "49999"),
                       ("x-nv-video[0].chromaFormat", "1")]
        let body = NvstRtspSdp.buildAnnounceSdp(.init(officialCloudPath: false,
                                                     offeredAttributes: offered,
                                                     echoesOfferedAttributes: true))
        #expect(!body.contains("a=x-nv-general.nativeRtcOnBundlePort:1"))
        #expect(!body.contains("a=x-nv-general.clientPorts.video:49999"))
        #expect(body.contains("a=x-nv-video[0].chromaFormat:1"))
    }

    /// The baseline answer is the one that reached `OPEN_ACK`; the tuning block is opt-in so a
    /// larger ANNOUNCE can never creep back in unnoticed.
    @Test func theBaselineAnnounceStaysSmallAndTheTuningBlockIsOptIn() {
        func attributeCount(_ body: String) -> Int { body.components(separatedBy: "a=x-nv-").count - 1 }
        let baseline = NvstRtspSdp.buildAnnounceSdp(.init())
        let extended = NvstRtspSdp.buildAnnounceSdp(.init(announcesExtendedSettings: true))
        #expect(attributeCount(baseline) < attributeCount(extended))
        #expect(!baseline.contains("a=x-nv-video[0].transferProtocol:"))
        #expect(extended.contains("a=x-nv-video[0].transferProtocol:1"))
        // The baseline is the captured official set, so it carries what that carries — and not the
        // keys we once invented. `maxCodecProfile` and `rateControlMode` were added to fix
        // `VIDEO_ENCODER_INIT_FAILED`; the official client announces neither, and the actual fix
        // was naming the bitstream format.
        #expect(!baseline.contains("a=x-nv-video[0].maxCodecProfile:"))
        #expect(!baseline.contains("a=x-nv-video[0].rateControlMode:"))
        #expect(baseline.contains("a=x-nv-video[0].videoSplitEncodeStripsPerFrame:64"))
    }

    /// The seat encodes whatever `bitStreamFormat` names. Omitting it left it defaulting to a
    /// codec that cannot carry the mode, and it answered `VIDEO_ENCODER_INIT_FAILED`.
    @Test func theAnnounceNamesTheBitstreamFormatForTheNegotiatedCodec() {
        let hevc = NvstRtspSdp.buildAnnounceSdp(.init(codec: .hevc))
        #expect(hevc.contains("a=x-nv-vqos[0].bitStreamFormat:1"))
        #expect(NvstRtspSdp.buildAnnounceSdp(.init(codec: .h264)).contains("a=x-nv-vqos[0].bitStreamFormat:0"))
        #expect(NvstRtspSdp.buildAnnounceSdp(.init(codec: .av1)).contains("a=x-nv-vqos[0].bitStreamFormat:2"))
        // One line per attribute, so the default never survives alongside the negotiated value.
        #expect(hevc.components(separatedBy: "a=x-nv-vqos[0].bitStreamFormat:").count == 2)
    }

    /// The official client's own ANNOUNCE — captured verbatim from its RTSPS WebSocket — carries
    /// exactly two `ri.` attributes. The rest of what appears under `ri.` in its logs is resolved
    /// config it never announces, and announcing `ri.protocol` contradicts the version the seat
    /// negotiates on the control channel.
    /// The captured official ANNOUNCE carries no `rtcpOnSctp`. Announcing it points the seat's
    /// feedback at an SCTP channel the seat never opens for us, so it receives no receiver reports
    /// and its congestion control reduces bitrate and frame rate.
    /// The seat's one-way-delay rate control is fed by `0x0207` QoS reports we do not send, and a
    /// delay controller with no samples parks at a floor: 48 packets/s against the official
    /// client's 401 on the same title, measured.
    @Test func theAnnounceAsksForLossBasedRateControlUntilQosFeedbackExists() {
        #expect(NvstRtspSdp.buildAnnounceSdp(.init()).contains("a=x-nv-bwe.useOwdCongestionControl:0"))
        // And the seat's own preference is restorable for comparison.
        #expect(NvstRtspSdp.buildAnnounceSdp(.init(disablesOwdCongestionControl: false))
            .contains("a=x-nv-bwe.useOwdCongestionControl:1"))
    }

    @Test func theAnnounceDoesNotClaimRtcpOverSctpByDefault() {
        #expect(!NvstRtspSdp.buildAnnounceSdp(.init(rtcpOnSctp: false)).contains("a=x-nv-general.rtcpOnSctp"))
        // Still expressible for a session that really does open the channel.
        #expect(NvstRtspSdp.buildAnnounceSdp(.init(rtcpOnSctp: true)).contains("a=x-nv-general.rtcpOnSctp:1"))
    }

    @Test func theAnnounceCarriesOnlyTheTwoRemoteInputAttributesTheOfficialClientSends() {
        let body = NvstRtspSdp.buildAnnounceSdp(.init())
        #expect(body.contains("a=x-nv-ri.hidDeviceMask:4"))
        #expect(body.contains("a=x-nv-ri.partialReliableThresholdMs:300"))
        let announced = body.components(separatedBy: "\r\n").filter { $0.hasPrefix("a=x-nv-ri.") }
        #expect(announced.count == 2)
        #expect(!body.contains("a=x-nv-ri.protocol:"))
    }

    @Test func theAnnounceCarriesTheEncoderProfileAndBitrate() {
        let body = NvstRtspSdp.buildAnnounceSdp(.init(resolution: "5120x2160",
                                                     fps: 120,
                                                     codec: .hevc,
                                                     bitrateKbps: 75_000))
        // No codec profile/level (the seat takes those from the negotiated profile), but frame rate
        // IS ours: the seat's DESCRIBE baseline caps maxFPS at 60, and the announce overrides only
        // what it states, so a 120 session must announce maxFPS or the encoder stays capped at 60.
        #expect(!body.contains("a=x-nv-video[0].maxCodecProfile:"))
        #expect(body.contains("a=x-nv-video[0].maxFPS:120"))
        #expect(body.contains("a=x-nv-vqos[0].dfc.minTargetFps:100"))
        #expect(body.contains("a=x-nv-vqos[0].gfc.minTargetFps:100"))
        #expect(body.contains("a=x-nv-video[0].clientViewportWd:5120"))
        #expect(body.contains("a=x-nv-video[0].clientViewportHt:2160"))
        #expect(body.contains("a=x-nv-vqos[0].bitStreamFormat:1"))
        #expect(body.contains("a=x-nv-video[0].framePacing.pid.minTargetFrameTimeUs:7936"))
        #expect(!body.contains("a=x-nv-video[0].framePacing.pid.targetFrameTimeUs:"))
        #expect(!body.contains("a=x-nv-video[0].framePacing.pid.maxTargetFrameTimeUs:"))
        #expect(!body.contains("a=x-nv-vqos[0].avoidDuplicateGameFrames:"))
        #expect(!body.contains("a=x-nv-vqos[0].avoidDuplicateNonReflexGameFrames:"))
    }

    @Test func chosenPrefilterOverridesTheCapturedBaselineOnIndexZero() {
        let body = NvstRtspSdp.buildAnnounceSdp(.init(prefilterMode: 3,
                                                     prefilterSharpness: 8,
                                                     prefilterDenoise: 2,
                                                     prefilterModel: 4))
        #expect(body.contains("a=x-nv-video[0].prefilterParams.prefilterMode:3"))
        #expect(body.contains("a=x-nv-video[0].prefilterParams.prefilterModel:4"))
        #expect(body.contains("a=x-nv-video[0].prefilterParams.sharpnessLevel:8"))
        #expect(body.contains("a=x-nv-video[0].prefilterParams.denoiseLevel:2"))
        // Only index 0 is actually decoded/displayed; the captured baseline's other indices are
        // left alone, matching every other client-known override in this file (viewport, maxFPS).
        #expect(body.contains("a=x-nv-video[1].prefilterParams.prefilterMode:2"))
    }

    @Test func prefilterOffZeroesSharpnessAndDenoiseInsteadOfLeavingTheCapturedLevels() {
        let body = NvstRtspSdp.buildAnnounceSdp(.init(prefilterMode: 0))
        #expect(body.contains("a=x-nv-video[0].prefilterParams.prefilterMode:0"))
        #expect(body.contains("a=x-nv-video[0].prefilterParams.sharpnessLevel:0"))
        #expect(body.contains("a=x-nv-video[0].prefilterParams.denoiseLevel:0"))
        // Off does not touch prefilterModel: there is nothing to zero, and the captured default is
        // harmless when the mode itself is off.
        #expect(body.contains("a=x-nv-video[0].prefilterParams.prefilterModel:4"))
    }

    /// Client control frames are masked, as RFC 6455 requires. The keepalive itself is an RTSP
    /// OPTIONS, not a ping: a live seat closes the connection within milliseconds of a client ping
    /// (measured 2026-09-04). The encoder stays for answering the seat's own pings.
    @Test func theWebSocketPingIsAMaskedClientFrame() {
        let ping = NvstWebSocketFrame.encodePing()
        #expect(ping.count == 6)                 // 2-byte header + 4-byte mask, empty payload
        #expect(ping[0] == 0x89)                 // FIN + opcode 9
        #expect(ping[1] == 0x80)                 // masked, zero length
        // Text frames keep their own opcode.
        #expect(NvstWebSocketFrame.encodeText(Data("x".utf8))[0] == 0x81)
        #expect(NvstRtspConnection.keepAliveInterval == 2)
    }

    @Test func announceCarriesTheAllowlistResolutionAndCloudFlags() {
        let options = NvstRtspSdp.AnnounceOptions(
            resolution: "3840x2160",
            fps: 120,
            encryptionKey: NvstRuntimeEncryptionKey(aesKeyHex: String(repeating: "0a", count: 32), keyID: 42),
            iceCredentials: NvstRtspIceCredentials(usernameFragment: "abcd", password: String(repeating: "p", count: 22)),
            videoPort: 48_322,
            clientBundlePort: 49_005,
            localAddress: "192.168.1.20",
            dtlsFingerprint: "AA:BB",
            officialCloudPath: true,
            rtcpOnSctp: true
        )
        let sdp = NvstRtspSdp.buildAnnounceSdp(options)
        let lines = sdp.components(separatedBy: "\r\n")
        #expect(lines[0] == "v=0")
        // The official macOS handshake origin username is "unknown", not "android".
        #expect(lines[1] == "o=unknown 0 14 IN IPv4 127.0.0.1")
        #expect(lines.contains("a=x-nv-video[0].clientViewportWd:3840"))
        #expect(lines.contains("a=x-nv-video[0].clientViewportHt:2160"))
        // Frame rate IS announced (the seat's DESCRIBE baseline caps maxFPS at 60; a 120 session
        // must lift it), while the pacing FLOOR stays the captured 7936 us rather than being derived
        // from the fps.
        #expect(lines.contains("a=x-nv-video[0].maxFPS:120"))
        #expect(lines.contains("a=x-nv-video[0].framePacing.pid.minTargetFrameTimeUs:7936"))
        #expect(!lines.contains("a=x-nv-video[0].framePacing.pid.targetFrameTimeUs:16666"))
        #expect(!lines.contains("a=x-nv-video[0].framePacing.pid.maxTargetFrameTimeUs:16684"))
        // No VSync choice: the captured baseline's Adaptive pair (`mode:1`/`feedbackMode:1`)
        // stands untouched, exactly as every official streaming preset announces it.
        #expect(lines.contains("a=x-nv-video[0].framePacing.mode:1"))
        #expect(lines.contains("a=x-nv-video[0].framePacing.feedbackMode:1"))
        #expect(lines.contains("a=x-nv-runtime.videoSrtp:1"))
        #expect(lines.contains("a=x-nv-runtime.encryptionKeyId:42"))
        #expect(lines.contains("a=x-nv-runtime.encryptionKey:\(String(repeating: "0A", count: 32))"))
        // Official cloud path: every legacy client port is announced as 0, and the bundle port
        // is advertised separately.
        #expect(lines.contains("a=x-nv-general.clientPorts.video:0"))
        #expect(lines.contains("a=x-nv-general.clientPorts.bundle:0"))
        #expect(lines.contains("a=x-nv-general.clientBundlePort:49005"))
        #expect(lines.contains("a=x-nv-general.nativeRtcOnBundlePort:1"))
        #expect(lines.contains("a=x-nv-general.rtcVideoOnNativeBundle:0"))
        // These must describe what the bundle's answer really offers. A live seat reset every SCTP
        // stream and closed DTLS when we announced audio on a data-channel-only bundle.
        // Audio rides the bundle, and the bundle's answer really offers an audio section.
        #expect(lines.contains("a=x-nv-general.rtcAudioOnNativeBundle:1"))
        // The microphone flag mirrors the bundle's answer the same way: no mic setup was
        // supplied, so the answer carries no send section and the flag stays 0.
        #expect(lines.contains("a=x-nv-general.rtcMicOnNativeBundle:0"))
        // No mic sender SSRC without a negotiated mic section.
        #expect(!sdp.contains("x-nv-mic.micSsrcConfig.senderSsrc"))
        #expect(lines.contains("a=x-nv-general.rtcDataChannelOnNativeBundle:1"))
        // RTCP-over-SCTP must be advertised or the seat stops sending video.
        #expect(lines.contains("a=x-nv-general.rtcpOnSctp:1"))
        // V2 only on the official path; the Nvsc V1 ICE/DTLS attributes are skipped.
        #expect(lines.contains("a=x-nv-general.iceUserNameFragmentV2:abcd"))
        #expect(!lines.contains("a=x-nv-general.iceUsernameFragment:abcd"))
        #expect(lines.contains("a=x-nv-general.dtlsFingerprintV2:AA:BB"))
        #expect(!lines.contains("a=x-nv-general.dtlsFingerprint:AA:BB"))
        // The WebRTC-shaped answer is what actually arms inbound UDP.
        #expect(lines.contains("a=ice-ufrag:abcd"))
        #expect(lines.contains("a=fingerprint:sha-256 AA:BB"))
        #expect(lines.contains("a=setup:actpass"))
        #expect(lines.contains("a=candidate:1 1 udp 2122260223 192.168.1.20 49005 typ host"))
        #expect(lines.contains("m=video 48322"))
        #expect(lines.contains("c=IN IP4 0.0.0.0"))
        #expect(lines.contains("i=DeviceString, DeviceName"))
    }

    /// Announcing mic without a negotiated section is the SCTP-reset lesson of the audio flag,
    /// so the SSRC attribute is gated on the flag rather than on the value alone.
    @Test func announceOmitsTheMicSenderSsrcWithoutAMicrophoneChannel() {
        let options = NvstRtspSdp.AnnounceOptions(
            iceCredentials: NvstRtspIceCredentials(usernameFragment: "abcd", password: String(repeating: "p", count: 22)),
            videoPort: 48_322,
            clientBundlePort: 49_005,
            officialCloudPath: true,
            carriesMicrophoneOnBundle: false,
            microphoneSenderSsrc: 0x1234_5678
        )
        let sdp = NvstRtspSdp.buildAnnounceSdp(options)
        #expect(sdp.contains("a=x-nv-general.rtcMicOnNativeBundle:0"))
        #expect(!sdp.contains("x-nv-mic.micSsrcConfig.senderSsrc"))
    }

    @Test func announceFallsBackToTheLegacyPortShapeOffTheCloudPath() {
        let options = NvstRtspSdp.AnnounceOptions(
            iceCredentials: NvstRtspIceCredentials(usernameFragment: "abcd", password: "pw"),
            clientBundlePort: 49_100,
            dtlsFingerprint: "AA:BB",
            officialCloudPath: false
        )
        let lines = NvstRtspSdp.buildAnnounceSdp(options).components(separatedBy: "\r\n")
        #expect(lines.contains("a=x-nv-general.clientPorts.video:49100"))
        #expect(lines.contains("a=x-nv-general.iceUsernameFragment:abcd"))
        #expect(lines.contains("a=x-nv-general.dtlsFingerprint:AA:BB"))
        #expect(!lines.contains("a=x-nv-general.nativeRtcOnBundlePort:1"))
        // Default 1080p when the launch profile has no resolution; frame rate stays the seat's.
        #expect(lines.contains("a=x-nv-video[0].clientViewportWd:1920"))
        #expect(!lines.contains("a=x-nv-video[0].maxFPS:60"))
    }
}

/// The microphone announce A/B knob lives in shared `UserDefaults`, and Swift Testing runs
/// tests in parallel: these four must run serialized, or one test's knob value leaks into
/// another test's announce.
@Suite(.serialized) struct NvstRtspMicrophoneAnnounceKnobTests {
    /// The mic flag and the sender-SSRC attribute describe the same fact — the bundle's answer
    /// really carries the send section — and only go out together. NVST has no SDP transport, so
    /// the SSRC attribute is the seat's only way to learn which SSRC the mic RTP will use.
    @Test func announceCarriesTheMicrophoneFlagAndSenderSsrcWhenTheBundleCarriesMic() {
        let options = NvstRtspSdp.AnnounceOptions(
            iceCredentials: NvstRtspIceCredentials(usernameFragment: "abcd", password: String(repeating: "p", count: 22)),
            videoPort: 48_322,
            clientBundlePort: 49_005,
            localAddress: "192.168.1.20",
            dtlsFingerprint: "AA:BB",
            officialCloudPath: true,
            carriesMicrophoneOnBundle: true,
            microphoneSenderSsrc: 0x1234_5678
        )
        let sdp = NvstRtspSdp.buildAnnounceSdp(options)
        let lines = sdp.components(separatedBy: "\r\n")
        #expect(lines.contains("a=x-nv-general.rtcMicOnNativeBundle:1"))
        #expect(lines.contains("a=x-nv-mic.micSsrcConfig.senderSsrc:305419896"))
        // The captured baseline mic parameters still go out alongside it.
        #expect(lines.contains("a=x-nv-mic.bitrate:16000"))
        #expect(lines.contains("a=x-nv-mic.enablePacketizer:1"))
    }
}

// MARK: - Colour tier on the announce

extension NvstRtspSdpTests {
    @Test func colourTierMapsToDepthAndChromaFormatIdc() {
        #expect(NvstRtspSdp.colorFormat(forColorQuality: "8bit_420") == (8, 1))
        #expect(NvstRtspSdp.colorFormat(forColorQuality: "10bit_420") == (10, 1))
        #expect(NvstRtspSdp.colorFormat(forColorQuality: "8bit_444") == (8, 3))
        #expect(NvstRtspSdp.colorFormat(forColorQuality: "10bit_444") == (10, 3))
        #expect(NvstRtspSdp.colorFormat(forColorQuality: "10BIT_422") == (10, 2))
    }

    @Test func announceCarriesTheSessionsColourTier() {
        // No tier: the captured 10-bit 4:2:0 baseline stands.
        let baseline = NvstRtspSdp.buildAnnounceSdp(.init(resolution: "1920x1080", fps: 60))
        #expect(baseline.contains("a=x-nv-video[0].bitDepth:10"))
        #expect(baseline.contains("a=x-nv-video[0].chromaFormat:1"))

        // An 8-bit session must not keep announcing 10.
        let eight = NvstRtspSdp.buildAnnounceSdp(.init(resolution: "1920x1080", fps: 60, bitDepth: 8, chromaFormat: 1))
        #expect(eight.contains("a=x-nv-video[0].bitDepth:8"))
        #expect(!eight.contains("a=x-nv-video[0].bitDepth:10"))

        let ten444 = NvstRtspSdp.buildAnnounceSdp(.init(resolution: "1920x1080", fps: 60, bitDepth: 10, chromaFormat: 3))
        #expect(ten444.contains("a=x-nv-video[0].bitDepth:10"))
        #expect(ten444.contains("a=x-nv-video[0].chromaFormat:3"))
        #expect(!ten444.contains("a=x-nv-video[0].chromaFormat:1"))
    }

    /// The VSync choice overrides the captured baseline's pacing pair, in the client-owned layer:
    /// Off drops the pacer entirely (`mode:0`), On paces without interval feedback
    /// (`FRAME_PACING_FEEDBACK_NEVER`), Adaptive is the captured baseline (`mode:1`,
    /// `feedbackMode:1` = `FRAME_PACING_FEEDBACK_INTERVAL`). See `NvstVsyncMode`.
    @Test func vsyncModeOverridesTheFramePacingPair() {
        let off = NvstRtspSdp.buildAnnounceSdp(.init(vsyncMode: .off))
        #expect(off.contains("a=x-nv-video[0].framePacing.mode:0"))
        #expect(off.contains("a=x-nv-video[0].framePacing.feedbackMode:0"))

        let on = NvstRtspSdp.buildAnnounceSdp(.init(vsyncMode: .on))
        #expect(on.contains("a=x-nv-video[0].framePacing.mode:1"))
        #expect(on.contains("a=x-nv-video[0].framePacing.feedbackMode:0"))
        #expect(!on.contains("a=x-nv-video[0].framePacing.feedbackMode:1"))

        let adaptive = NvstRtspSdp.buildAnnounceSdp(.init(vsyncMode: .adaptive))
        #expect(adaptive.contains("a=x-nv-video[0].framePacing.mode:1"))
        #expect(adaptive.contains("a=x-nv-video[0].framePacing.feedbackMode:1"))
    }

    @Test func explicitHDRRequestOverridesSDRDescribeAndPreserves444() {
        let offered = [("x-nv-video[0].dynamicRangeMode", "0"),
                       ("x-nv-video[0].encoderCscMode", "2"),
                       ("x-nv-video[0].bitDepth", "8"), ("x-nv-video[0].chromaFormat", "1")]
        let hdr = NvstRtspSdp.buildAnnounceSdp(.init(bitDepth: 10, chromaFormat: 3,
            hdrEnabled: true, offeredAttributes: offered, codec: .hevc))
        #expect(hdr.contains("a=x-nv-video[0].dynamicRangeMode:1"))
        #expect(hdr.contains("a=x-nv-video[0].encoderCscMode:5"))
        #expect(hdr.contains("a=x-nv-video[0].bitDepth:10"))
        #expect(hdr.contains("a=x-nv-video[0].chromaFormat:3"))
        let sdr = NvstRtspSdp.buildAnnounceSdp(.init(bitDepth: 10, hdrEnabled: false, codec: .av1))
        #expect(sdr.contains("a=x-nv-video[0].dynamicRangeMode:0"))
        #expect(sdr.contains("a=x-nv-video[0].bitDepth:10"))
        let h264 = NvstRtspSdp.buildAnnounceSdp(.init(bitDepth: 10, chromaFormat: 3, hdrEnabled: true, codec: .h264))
        #expect(h264.contains("a=x-nv-video[0].dynamicRangeMode:0"))
        #expect(h264.contains("a=x-nv-video[0].bitDepth:8"))
        #expect(h264.contains("a=x-nv-video[0].chromaFormat:1"))
    }
}
