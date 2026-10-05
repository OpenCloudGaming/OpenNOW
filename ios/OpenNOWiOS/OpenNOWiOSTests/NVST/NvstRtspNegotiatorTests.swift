//  The negotiator itself: endpoint selection, the feedback channel, and the handshake it drives end
//  to end against a stubbed control channel.
//

import Foundation
import Testing
@testable import OpenNOWiOS

@Suite struct NvstRtspEndpointTests {
    @Test func theTaggedControlPortOutranksTaggedMediaPorts() {
        // A real seat tags BOTH its :322 control endpoint and its :48322 media endpoint with
        // appLevelProtocol 6, so the control port has to win on port number.
        let connections: [[String: Any]] = [
            ["usage": 14, "appLevelProtocol": 6, "port": 48_322, "ip": "seat.example.com"],
            ["usage": 14, "appLevelProtocol": 6, "port": 322, "ip": "seat.example.com"],
        ]
        #expect(NvstRtspEndpoints.collect(connections: connections, fallbackHost: "seat.example.com") == [
            "rtsps://seat.example.com:322",
            "rtsps://seat.example.com:48322",
        ])
    }

    @Test func anUntaggedControlPortAndTheAssumedDefaultAreBothCandidates() {
        // Our own captures showed the :322 control service under a usage code other than 16, and
        // some seats do not advertise it at all — so both cases must still yield a candidate.
        let connections: [[String: Any]] = [
            ["usage": 14, "port": 322, "ip": "media.example.com"],
            ["usage": 14, "port": 443, "resourcePath": "/nvst/"],
        ]
        let endpoints = NvstRtspEndpoints.collect(connections: connections, fallbackHost: "seat.example.com")
        #expect(endpoints == ["rtsps://media.example.com:322", "rtsps://seat.example.com:322"])
        // A tagged endpoint always outranks the untagged and assumed ones.
        let tagged: [[String: Any]] = [
            ["usage": 14, "port": 322, "ip": "media.example.com"],
            ["usage": 16, "port": 322, "ip": "control.example.com"],
        ]
        #expect(NvstRtspEndpoints.collect(connections: tagged, fallbackHost: "seat.example.com").first == "rtsps://control.example.com:322")
        // Placeholder hosts fall back to the session host rather than becoming 0.0.0.0.
        #expect(NvstRtspEndpoints.collect(connections: [["usage": 16, "port": 322, "ip": "0.0.0.0"]], fallbackHost: "seat.example.com")
            == ["rtsps://seat.example.com:322"])
        // With no host at all there is nothing to try.
        #expect(NvstRtspEndpoints.collect(connections: [["usage": 14, "port": 443]], fallbackHost: nil).isEmpty)
    }

    @Test func candidatesArePreferenceOrderedAndDeduplicated() {
        let candidates = NvstRtspEndpoints.candidates([" rtsps://a:322 ", "https://ignored", "rtsps://a:322", "rtsp://b:8554"])
        #expect(candidates == ["rtsps://a:322", "rtsp://b:8554"])
    }

    @Test func rtspsEndpointsComeFromUsageSixteenOrProtocolSix() {
        let connections: [[String: Any]] = [
            ["usage": 14, "port": 443, "resourcePath": "/nvst/"],
            ["usage": 16, "port": 322, "resourcePath": ""],
            ["usage": 10, "port": 443, "appLevelProtocol": 6],
            ["usage": 2, "port": 48322, "resourcePath": "rtsps://explicit.example.com:322"],
        ]
        let endpoints = NvstRtspEndpoints.collect(connections: connections, fallbackHost: "seat.example.com")
        // Tagged-and-on-:322 first, then the explicit rtsps:// resourcePath, and a tagged
        // endpoint on another port last.
        #expect(endpoints == [
            "rtsps://seat.example.com:322",
            "rtsps://explicit.example.com:322",
            "rtsps://seat.example.com:443",
        ])
        #expect(NvstRtspEndpoints.collect(connections: connections, fallbackHost: nil) == ["rtsps://explicit.example.com:322"])
    }

    @Test func rtspsEndpointsAreReadOutOfTheRawSessionJson() {
        let json = """
        {"connectionInfo":[{"usage":16,"port":322},{"usage":14,"port":443}]}
        """
        #expect(NvstRtspEndpoints.collect(rawSessionJSON: json, fallbackHost: "seat") == ["rtsps://seat:322"])
        #expect(NvstRtspEndpoints.collect(rawSessionJSON: "not json", fallbackHost: "seat").isEmpty)
    }

    @Test func endpointParsingDefaultsToTheNvstControlPort() {
        #expect(NvstRtspEndpoints.parse(endpoint: "rtsps://seat.example.com:322") == NvstRtspEndpoints.Target(host: "seat.example.com", port: 322))
        #expect(NvstRtspEndpoints.parse(endpoint: "rtsps://seat.example.com")?.port == 322)
        #expect(NvstRtspEndpoints.parse(endpoint: "rtsp://seat.example.com:8554")?.port == 8554)
        #expect(NvstRtspEndpoints.parse(endpoint: "garbage") == nil)
        #expect(NvstRtspEndpoints.selectPrimary(["", "https://x", "rtsps://a:322", "rtsps://b:322"]) == "rtsps://a:322")
        #expect(NvstRtspEndpoints.selectPrimary(["https://x"]) == nil)
    }

    @Test func setupUriCandidatesPreferTheBareControlToken() {
        // A live seat accepted OPTIONS and DESCRIBE, then answered 400 to an absolute SETUP URI:
        // the official form is the bare control token DESCRIBE advertised.
        let candidates = NvstRtspEndpoints.videoSetupURICandidates(
            control: "streamid=video/0",
            base: "rtsps://seat.example.com:322",
            officialCloudPath: true
        )
        #expect(candidates == [
            "streamid=video/0/0",
            "streamid=video/0",
            "rtsps://seat.example.com:322/streamid=video/0/0",
            "rtsps://seat.example.com:322/streamid=video/0",
        ])
        // Off the cloud path there is no substream transform.
        #expect(NvstRtspEndpoints.videoSetupURICandidates(control: "streamid=video/0", base: "rtsps://seat:322", officialCloudPath: false)
            == ["streamid=video/0", "rtsps://seat:322/streamid=video/0"])
        // An already-absolute control resolves to itself and is not duplicated.
        #expect(NvstRtspEndpoints.videoSetupURICandidates(control: "rtsps://other:322/x", base: "rtsps://seat:322", officialCloudPath: true)
            == ["rtsps://other:322/x"])
    }

    @Test func officialSetupAddressesTheSubstreamOfTheVideoControl() {
        #expect(NvstRtspEndpoints.officialVideoSetupControl("streamid=video/0") == "streamid=video/0/0")
        #expect(NvstRtspEndpoints.officialVideoSetupControl("streamid=video/0/0") == "streamid=video/0/0")
        #expect(NvstRtspEndpoints.officialVideoSetupControl("streamid=audio/0") == "streamid=audio/0")
    }

    @Test func controlUrisResolveAgainstTheRtspBase() {
        #expect(NvstRtspEndpoints.resolveControlURI(base: "rtsps://seat:322", control: "streamid=video/0") == "rtsps://seat:322/streamid=video/0")
        #expect(NvstRtspEndpoints.resolveControlURI(base: "rtsps://seat:322/", control: "/streamid=video/0") == "rtsps://seat:322/streamid=video/0")
        #expect(NvstRtspEndpoints.resolveControlURI(base: "rtsps://seat:322", control: "rtsps://other:322/x") == "rtsps://other:322/x")
    }

    @Test func pingUfragIncrementsAsAFixedWidthLowercaseHexString() {
        // Official bundle ICE remote ufrag is the SETUP ping payload + 1.
        #expect(NvstRtspEndpoints.incrementPingUfrag("998") == "999")
        #expect(NvstRtspEndpoints.incrementPingUfrag("99f") == "9a0")
        #expect(NvstRtspEndpoints.incrementPingUfrag("99F") == "9a0")
        #expect(NvstRtspEndpoints.incrementPingUfrag("fff") == "1000")
        // Wider than 64 bits must still increment.
        #expect(NvstRtspEndpoints.incrementPingUfrag(String(repeating: "0", count: 40)) == String(repeating: "0", count: 39) + "1")
        #expect(NvstRtspEndpoints.incrementPingUfrag("PING") == nil)
        #expect(NvstRtspEndpoints.incrementPingUfrag("") == nil)
    }

    @Test func remoteUfragPrefersTheSetupPingIdentityOverDescribe() {
        #expect(NvstRtspEndpoints.resolveIceRemoteUfrag(pingPayload: "998", describeUfrag: "sEaT", pingVersion: 6) == "999")
        // "Old server only supports PING": the ping string itself is the identity.
        #expect(NvstRtspEndpoints.resolveIceRemoteUfrag(pingPayload: "PING", describeUfrag: "sEaT", pingVersion: 6) == "PING")
        #expect(NvstRtspEndpoints.resolveIceRemoteUfrag(pingPayload: nil, describeUfrag: "sEaT", pingVersion: nil) == "sEaT")
        // A non-hex, non-PING payload on a legacy seat falls back to DESCRIBE.
        #expect(NvstRtspEndpoints.resolveIceRemoteUfrag(pingPayload: "zz", describeUfrag: "sEaT", pingVersion: nil) == "sEaT")
    }
}

@Suite struct NvstFeedbackChannelTests {
    @Test func feedbackChannelUsesTheOfficialPrivateLabel() {
        // `rtcp1` is refused: the seat resets a DCEP open whose label it does not recognise.
        #expect(NvstFeedbackSender.channelLabel == "rtcp_on_sctp_private")
        #expect(NvstFeedbackSender.streamIdentifierCandidates == 8)
    }

    @Test func cadenceEmitsPlainReceiverReportsAndPliOnRequest() {
        let sender = NvstFeedbackSender(interval: 3600)
        sender.configure(channelWriter: { _ in }, senderSSRC: 0x4f4e_4f57, mediaSSRC: 0x1234)
        sender.updateMediaState(highestExtendedSequence: 500, cumulativeLost: 3)
        let plain = sender.nextPayloads()
        #expect(plain.count == 1)
        // Plain RTCP RR: DTLS already encrypts the SCTP association, so there is no SRTCP trailer.
        #expect(plain[0].count == 32)
        #expect(plain[0][1] == NvstRtcp.receiverReportPayloadType)

        sender.requestKeyframe()
        let withPli = sender.nextPayloads()
        #expect(withPli.count == 2)
        #expect(withPli[1].count == 12)
        #expect(withPli[1][1] == NvstRtcp.payloadSpecificFeedbackPayloadType)
        // The request is one-shot.
        #expect(sender.nextPayloads().count == 1)
    }

    @Test func noReportIsEmittedUntilAMediaSsrcIsKnown() {
        // A Receiver Report about SSRC 0 describes a stream that does not exist. A live seat reset
        // the SCTP stream and closed the whole DTLS association shortly after our first report, so
        // reports wait for a bound SSRC.
        let sender = NvstFeedbackSender(interval: 3600)
        final class Sink: @unchecked Sendable {
            private let lock = NSLock()
            private var count = 0
            func record() { lock.lock(); count += 1; lock.unlock() }
            var value: Int { lock.lock(); defer { lock.unlock() }; return count }
        }
        let sink = Sink()
        sender.configure(channelWriter: { _ in sink.record() }, senderSSRC: 0x4f4e_4f57, mediaSSRC: 0)
        sender.emitForTesting()
        #expect(sink.value == 0)
        sender.updateMediaSSRC(0x1234_5678)
        sender.emitForTesting()
        #expect(sink.value == 1)
    }

    @Test func mediaSsrcIsLearnedFromTheFirstAuthenticatedPacket() {
        let sender = NvstFeedbackSender(interval: 3600)
        sender.configure(channelWriter: { _ in }, senderSSRC: 1, mediaSSRC: 0)
        sender.updateMediaSSRC(0xdead_beef)
        let report = sender.nextPayloads()[0]
        let ssrc = (UInt32(report[8]) << 24) | (UInt32(report[9]) << 16) | (UInt32(report[10]) << 8) | UInt32(report[11])
        #expect(ssrc == 0xdead_beef)
    }
}

/// A recorded seat: answers the handshake in order and records exactly what the client sent.
private actor RecordedSeat: NvstRtspControlChannel {
    struct SentRequest: Sendable {
        let method: String
        let uri: String
        let headers: [(String, String)]
        let body: String

        func header(_ name: String) -> String? {
            headers.first { $0.0.lowercased() == name.lowercased() }?.1
        }
    }

    private let responses: [String: NvstRtspResponse]
    /// When set, only this SETUP request-URI is accepted; every other form gets the canned
    /// rejection, which is how the live seat behaved.
    private let acceptedSetupURI: String?
    private let acceptedSetupResponse: NvstRtspResponse?
    private(set) var sent: [SentRequest] = []
    private(set) var connected = false
    private(set) var closed = false

    init(responses: [String: NvstRtspResponse],
         acceptedSetupURI: String? = nil,
         acceptedSetupResponse: NvstRtspResponse? = nil) {
        self.responses = responses
        self.acceptedSetupURI = acceptedSetupURI
        self.acceptedSetupResponse = acceptedSetupResponse
    }

    func connect(sessionID: String?) async throws { connected = true }

    func request(method: String, uri: String, headers: [(String, String)], body: String) async throws -> NvstRtspResponse {
        sent.append(SentRequest(method: method, uri: uri, headers: headers, body: body))
        if method == "SETUP", let acceptedSetupURI, let acceptedSetupResponse, uri == acceptedSetupURI {
            return acceptedSetupResponse
        }
        guard let response = responses[method] else {
            return NvstRtspResponse(statusCode: 500, statusText: "No recorded response", headers: [:], body: "")
        }
        return response
    }

    func close() async { closed = true }

    func requests(_ method: String) -> [SentRequest] { sent.filter { $0.method == method } }
}

private struct StubReserver: NvstBundleReserving {
    let reservation: NvstBundleReservation
    /// Stands in for the real ICE/DTLS bundle, which can only come up after SETUP.
    let lateIdentity: NvstBundleReservation?

    init(reservation: NvstBundleReservation, lateIdentity: NvstBundleReservation? = nil) {
        self.reservation = reservation
        self.lateIdentity = lateIdentity
    }

    func reserveBundle() async throws -> NvstBundleReservation { reservation }
    func bundleIdentity(for handoff: NVSTVideoHandoff, microphoneOfferedOnBundle: Bool) async -> NvstBundleReservation? { lateIdentity }
}

@Suite struct NvstRtspNegotiatorTests {
    private static let reservation = NvstBundleReservation(
        bundlePort: 49_005,
        mjolnirPort: 49_006,
        localAddress: "192.168.1.20",
        iceCredentials: NvstRtspIceCredentials(usernameFragment: "abcd", password: String(repeating: "p", count: 22)),
        dtlsFingerprint: "AA:BB:CC:DD"
    )

    private static func seat(describeBody: String = NvstRtspSdpTests.describeBody,
                             setupHeaders: [String: String] = [
                                "session": "S123",
                                "transport": "unicast;X-GS-ServerPort=48322;source=10.20.30.40",
                                "x-nv-ping": "6",
                                "x-nv-ping-payload": "0998",
                             ]) -> RecordedSeat {
        RecordedSeat(responses: [
            "OPTIONS": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: ["x-gs-version": "14.2"], body: ""),
            "DESCRIBE": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: ["session": "S123;timeout=60"], body: describeBody),
            "SETUP": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: setupHeaders, body: ""),
            "ANNOUNCE": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
            "PLAY": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
            "TEARDOWN": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
        ])
    }

    private static func negotiator(_ seat: RecordedSeat,
                                   reservation: NvstBundleReservation = Self.reservation) -> NvstRtspNegotiator {
        NvstRtspNegotiator(
            reserver: StubReserver(reservation: reservation),
            connectionFactory: { _, _, _ in seat }
        )
    }

    private static let input = NvstRtspNegotiationInput(
        sessionID: "session-1",
        rtspsEndpoints: ["rtsps://seat.example.com:322"],
        resolution: "2560x1440",
        fps: 60,
        codec: "HEVC"
    )

    @Test func handshakeRunsOptionsDescribeSetupAnnouncePlayInOrder() async throws {
        let seat = Self.seat()
        let session = try await Self.negotiator(seat).negotiate(Self.input)
        #expect(await seat.sent.map(\.method) == ["OPTIONS", "DESCRIBE", "SETUP", "ANNOUNCE", "PLAY"])
        #expect(session.steps == ["wss-open", "options", "describe", "setup-video", "announce", "play"])
        #expect(session.sessionIdentifier == "S123")
        #expect(session.hmacSeedPresent)
    }

    @Test func describeCarriesTheAbTestingHeaderTheSeatKeysPingOff() async throws {
        let seat = Self.seat()
        _ = try await Self.negotiator(seat).negotiate(Self.input)
        let describe = await seat.requests("DESCRIBE").first
        #expect(describe?.header("x-nv-abtesting") == "2")
        #expect(describe?.header("Accept") == "application/sdp")
        #expect(describe?.header("X-GS-Version") == "14.2")
        #expect(describe?.header("x-nv-sessionid") == "session-1")
    }

    @Test func setupRetriesTheOtherRequestUriFormsUntilOneIsAccepted() async throws {
        // Mirrors the observed seat: OPTIONS/DESCRIBE fine, then 400 for every form but one.
        let seat = RecordedSeat(
            responses: [
                "OPTIONS": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
                "DESCRIBE": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: ["session": "S1"], body: NvstRtspSdpTests.describeBody),
                "SETUP": NvstRtspResponse(statusCode: 400, statusText: "OK", headers: [:], body: ""),
                "ANNOUNCE": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
                "PLAY": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
            ],
            acceptedSetupURI: "streamid=video/0",
            acceptedSetupResponse: NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [
                "transport": "unicast;X-GS-ServerPort=48322;source=10.20.30.40",
                "x-nv-ping": "6",
                "x-nv-ping-payload": "0998",
            ], body: "")
        )
        let session = try await Self.negotiator(seat).negotiate(Self.input)
        #expect(session.handoff.videoPeerPort == 48_322)
        // The first form is tried first and rejected, the second accepted.
        #expect(await seat.requests("SETUP").map(\.uri) == ["streamid=video/0/0", "streamid=video/0"])
    }

    @Test func setupFailingEveryFormReportsTheFormsItTried() async throws {
        let seat = Self.seat(setupHeaders: [:])
        let allRejected = RecordedSeat(responses: [
            "OPTIONS": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
            "DESCRIBE": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: ["session": "S1"], body: NvstRtspSdpTests.describeBody),
            "SETUP": NvstRtspResponse(statusCode: 400, statusText: "OK", headers: [:], body: ""),
        ])
        _ = seat
        await #expect(throws: NvstRtspNegotiationError.self) {
            _ = try await Self.negotiator(allRejected).negotiate(Self.input)
        }
        // Four request-URI forms across both Transport forms are attempted before giving up.
        #expect(await allRejected.requests("SETUP").count == 8)
        let transports = await allRejected.requests("SETUP").map { $0.header("Transport") ?? "" }
        // The official empty Transport is tried first, the legacy client-port form second.
        #expect(transports.prefix(4).allSatisfy { $0.isEmpty })
        #expect(transports.suffix(4).allSatisfy { $0.contains("X-GS-ClientPort=") })
    }

    @Test func officialCloudSetupSendsAnEmptyTransportAndTheAdvertisedPingVersion() async throws {
        let seat = Self.seat()
        _ = try await Self.negotiator(seat).negotiate(Self.input)
        let setup = await seat.requests("SETUP").first
        // Official cloud SETUPs video only, with an empty Transport, at the `/0` substream —
        // addressed by the bare control token, not a resolved absolute URI.
        #expect(setup?.uri == "streamid=video/0/0")
        #expect(setup?.header("Transport") == "")
        #expect(setup?.header("x-nv-ping") == "6")
        #expect(setup?.header("Session") == "S123")
    }

    @Test func handoffIsDerivedFromSetupAndTheClientGeneratedRuntimeKey() async throws {
        let seat = Self.seat()
        let session = try await Self.negotiator(seat).negotiate(Self.input)
        let handoff = session.handoff
        #expect(handoff.videoPeerIP == "10.20.30.40")
        #expect(handoff.videoPeerPort == 48_322)
        // Video arrives on the dedicated Mjolnir socket, not the ICE/DTLS bundle socket.
        #expect(handoff.clientUDPPort == 49_006)
        #expect(handoff.mjolnirUDPPort == 49_006)
        // DESCRIBE advertised AEAD_AES_256_GCM explicitly, so the 16-byte tag wins over the
        // legacy GCM-8 default.
        #expect(handoff.srtpProfile == .aeadAes256Gcm)
        #expect(handoff.srtpAESKey.count == 32)
        #expect(handoff.srtpSalt.count == 12)
        #expect(handoff.codec == .hevc)
        #expect(handoff.pingVersion == 6)
        #expect(handoff.pingPayload == "0998")
        // The handshake needs far longer than the 5 s media idle default.
        #expect(handoff.timeoutMilliseconds == 60_000)
        // Remote ICE identity is the ping payload + 1; the password comes from DESCRIBE.
        #expect(handoff.iceCredentials?.remoteUsernameFragment == "0999")
        #expect(handoff.iceCredentials?.remotePassword == "seatPasswordThatIsLong")
        #expect(handoff.iceCredentials?.localUsernameFragment == "abcd")
        #expect(handoff.iceCredentials?.remoteDTLSFingerprint?.count == 95)
        #expect(session.remoteIceUsernameFragment == "0999")
    }

    @Test func aLateBundleIdentityOverridesTheAnnouncedPortAndFingerprint() async throws {
        // The real bundle needs SETUP's ping payload and DESCRIBE's fingerprint before it can bind,
        // so its port and fingerprint only exist after SETUP — and ANNOUNCE must carry those.
        let seat = Self.seat()
        let late = NvstBundleReservation(
            bundlePort: 61_000,
            mjolnirPort: 49_006,
            localAddress: "10.0.0.7",
            iceCredentials: NvstRtspIceCredentials(usernameFragment: "OyEL", password: String(repeating: "p", count: 22)),
            dtlsFingerprint: "99:88:77"
        )
        let negotiator = NvstRtspNegotiator(
            reserver: StubReserver(reservation: Self.reservation, lateIdentity: late),
            connectionFactory: { _, _, _ in seat }
        )
        _ = try await negotiator.negotiate(Self.input)
        let body = await seat.requests("ANNOUNCE").first?.body ?? ""
        #expect(body.contains("a=x-nv-general.clientBundlePort:61000"))
        #expect(body.contains("a=x-nv-general.dtlsFingerprintV2:99:88:77"))
        #expect(body.contains("a=fingerprint:sha-256 99:88:77"))
        #expect(body.contains("a=candidate:1 1 udp 2122260223 10.0.0.7 61000 typ host"))
        // A live bundle means the feedback plane moves onto its SCTP channel.
        #expect(body.contains("a=x-nv-general.rtcpOnSctp:1"))
        // The reserved placeholder port must not leak into ANNOUNCE.
        #expect(!body.contains("a=x-nv-general.clientBundlePort:49005"))
    }

    @Test func announceReportsTheBundleIdentityAndRtcpOverSctp() async throws {
        let seat = Self.seat()
        _ = try await Self.negotiator(seat).negotiate(Self.input)
        let announce = await seat.requests("ANNOUNCE").first
        #expect(announce?.header("Content-Type") == "application/sdp")
        let body = announce?.body ?? ""
        #expect(body.contains("a=x-nv-general.clientBundlePort:49005"))
        #expect(body.contains("a=x-nv-general.rtcpOnSctp:1"))
        #expect(body.contains("a=x-nv-general.dtlsFingerprintV2:AA:BB:CC:DD"))
        #expect(body.contains("a=x-nv-general.iceUserNameFragmentV2:abcd"))
        #expect(body.contains("a=x-nv-runtime.encryptionKey:"))
        #expect(body.contains("a=x-nv-runtime.videoSrtp:1"))
        #expect(body.contains("a=x-nv-video[0].clientViewportWd:2560"))
        #expect(body.contains("m=video 48322"))
        #expect(body.contains("a=candidate:1 1 udp 2122260223 192.168.1.20 49005 typ host"))
    }

    @Test func theReceiverIsArmedBeforeAnnounceAndTheBundleAfterIt() async throws {
        let seat = Self.seat()
        let order = OrderRecorder()
        _ = try await Self.negotiator(seat).negotiate(
            Self.input,
            onVideoReady: { _ in await order.record("video-ready") },
            onAnnounceReady: { _ in await order.record("announce-ready") }
        )
        // Official Bifrost arms MjolnirVideoReceiver before ANNOUNCE, then WebRtcTransport after.
        #expect(await order.events == ["video-ready", "announce-ready"])
        let methods = await seat.sent.map(\.method)
        #expect(methods == ["OPTIONS", "DESCRIBE", "SETUP", "ANNOUNCE", "PLAY"])
    }

    @Test func releaseTearsDownAndClosesTheControlChannel() async throws {
        let seat = Self.seat()
        let session = try await Self.negotiator(seat).negotiate(Self.input)
        await session.release("test complete")
        #expect(await seat.requests("TEARDOWN").first?.header("Session") == "S123")
        #expect(await seat.closed)
    }

    @Test func forcingTheLegacyPathIgnoresTheAdvertisedCloudPath() async throws {
        let seat = Self.seat()
        let input = NvstRtspNegotiationInput(
            sessionID: "session-1",
            rtspsEndpoints: ["rtsps://seat.example.com:322"],
            resolution: "1920x1080",
            fps: 60,
            codec: "H264",
            rtcpOnSctp: false,
            forcesLegacyPath: true
        )
        _ = try await Self.negotiator(seat).negotiate(input)
        // Legacy SETUP advertises a real client port instead of the empty Transport, and does not
        // address the `/0` substream.
        let setup = await seat.requests("SETUP").first
        #expect(setup?.uri == "streamid=video/0")
        #expect(setup?.header("Transport")?.contains("X-GS-ClientPort=49006-49007") == true)
        // ANNOUNCE announces the socket video actually arrives on and drops the cloud flags.
        let body = await seat.requests("ANNOUNCE").first?.body ?? ""
        #expect(body.contains("a=x-nv-general.clientPorts.video:49006"))
        #expect(!body.contains("a=x-nv-general.nativeRtcOnBundlePort:1"))
        #expect(!body.contains("a=x-nv-general.clientBundlePort:"))
        // Absent rather than 0: the captured official ANNOUNCE carries no `rtcpOnSctp` at all, and
        // claiming it without a feedback channel starves the seat of receiver reports.
        #expect(!body.contains("a=x-nv-general.rtcpOnSctp"))
        // The Nvsc V1 ICE/DTLS attributes come back on the legacy path.
        #expect(body.contains("a=x-nv-general.iceUsernameFragment:abcd"))
    }

    @Test func playIsSkippedWhenDescribeDisabledIt() async throws {
        let body = NvstRtspSdpTests.describeBody.replacingOccurrences(of: "a=x-nv-general.disablePlay:0", with: "a=x-nv-general.disablePlay:1")
        let seat = Self.seat(describeBody: body)
        let session = try await Self.negotiator(seat).negotiate(Self.input)
        #expect(session.steps.contains("play-skipped"))
        #expect(await !seat.sent.map(\.method).contains("PLAY"))
    }

    @Test func playIsSentWhenDescribeOmitsDisablePlay() async throws {
        // The official client sends PLAY unless the seat explicitly disables it, so an absent
        // attribute means "not disabled" rather than "disabled". Only a literal 1 suppresses it.
        let body = NvstRtspSdpTests.describeBody.replacingOccurrences(of: "a=x-nv-general.disablePlay:0", with: "")
        #expect(!body.contains("disablePlay"))
        let seat = Self.seat(describeBody: body)
        let session = try await Self.negotiator(seat).negotiate(Self.input)
        #expect(session.steps.contains("play"))
        #expect(await seat.sent.map(\.method).contains("PLAY"))
    }

    @Test func announceOnlySeatsAnsweringFourFiveFiveStillYieldASession() async throws {
        let seat = RecordedSeat(responses: [
            "OPTIONS": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
            "DESCRIBE": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: ["session": "S9"], body: NvstRtspSdpTests.describeBody),
            "SETUP": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [
                "transport": "unicast;X-GS-ServerPort=48322;source=10.20.30.40",
                "x-nv-ping": "6",
                "x-nv-ping-payload": "0998",
            ], body: ""),
            "ANNOUNCE": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
            "PLAY": NvstRtspResponse(statusCode: 455, statusText: "Method Not Valid In This State", headers: [:], body: ""),
        ])
        let session = try await Self.negotiator(seat).negotiate(Self.input)
        #expect(session.steps.contains("play-455"))
    }

    @Test func pingVersionSixWithoutCredentialsFailsClosed() async throws {
        let body = NvstRtspSdpTests.describeBody
            .replacingOccurrences(of: "a=x-nv-general.iceUserNameFragmentV2:sEaT\r\n", with: "")
            .replacingOccurrences(of: "a=x-nv-general.icePasswordV2:seatPasswordThatIsLong\r\n", with: "")
        let seat = Self.seat(describeBody: body)
        await #expect(throws: NvstRtspNegotiationError.missingIceCredentials) {
            _ = try await Self.negotiator(seat).negotiate(Self.input)
        }
        #expect(await seat.closed)
    }

    @Test func aSetupWithoutAVideoPeerFailsClosed() async throws {
        let seat = Self.seat(setupHeaders: ["x-nv-ping": "1"])
        await #expect(throws: NvstRtspNegotiationError.missingVideoPeer) {
            _ = try await Self.negotiator(seat).negotiate(Self.input)
        }
    }

    @Test func conflictingSrtpProfilesBetweenDescribeAndSetupFailClosed() async throws {
        let seat = Self.seat(setupHeaders: [
            "transport": "unicast;X-GS-ServerPort=48322;source=10.20.30.40;srtpProfile=AES_CM_128_HMAC_SHA1_80",
            "x-nv-ping": "6",
            "x-nv-ping-payload": "0998",
        ])
        await #expect(throws: NvstRtspNegotiationError.conflictingSrtpProfile("AEAD_AES_256_GCM", "AES_CM_128_HMAC_SHA1_80")) {
            _ = try await Self.negotiator(seat).negotiate(Self.input)
        }
    }

    @Test func aDescribeWithoutTheControlStreamIsNotAnNvstSeat() async throws {
        let body = NvstRtspSdpTests.describeBody.replacingOccurrences(of: "a=control:streamid=control/0", with: "a=control:streamid=other/0")
        let seat = Self.seat(describeBody: body)
        await #expect(throws: NvstRtspNegotiationError.missingControlStream) {
            _ = try await Self.negotiator(seat).negotiate(Self.input)
        }
    }

    @Test func aSessionWithoutRtspsEndpointsCannotNegotiate() async throws {
        let seat = Self.seat()
        await #expect(throws: NvstRtspNegotiationError.missingEndpoint) {
            _ = try await Self.negotiator(seat).negotiate(
                NvstRtspNegotiationInput(sessionID: "s", rtspsEndpoints: [])
            )
        }
    }

    @Test func aNonTwoHundredDescribeSurfacesTheStatus() async throws {
        let seat = RecordedSeat(responses: [
            "OPTIONS": NvstRtspResponse(statusCode: 200, statusText: "OK", headers: [:], body: ""),
            "DESCRIBE": NvstRtspResponse(statusCode: 401, statusText: "Unauthorized", headers: [:], body: ""),
        ])
        await #expect(throws: NvstRtspNegotiationError.requestFailed("DESCRIBE", 401, "Unauthorized")) {
            _ = try await Self.negotiator(seat).negotiate(Self.input)
        }
    }
}

private actor OrderRecorder {
    private(set) var events: [String] = []
    func record(_ event: String) { events.append(event) }

    @Test func theAudioRelevantDescribeLinesCoverMicAqosAndCodecLines() {
        let body = [
            "v=0",
            "a=x-nv-general.pingVersion:6",
            "a=x-nv-general.rtcMicOnNativeBundle:1",
            "a=x-nv-aqos.enableRedundancyForMic:1",
            "a=x-nv-mic.frameSize:10",
            "a=x-nv-audio.jbConfig.initialThreshold:80",
            "a=x-nv-video.maxFPS:120",
            "m=audio 0 RTP/AVP 96 97",
            "a=rtpmap:96 opus/48000/2",
            "a=rtpmap:97 opus/16000/2",
            "a=fmtp:97 maxplaybackrate=16000",
            "a=control:streamid=audio/0",
        ].joined(separator: "\r\n")
        let lines = NvstRtspNegotiator.audioRelevantDescribeLines(body)
        #expect(lines == [
            "a=x-nv-general.rtcMicOnNativeBundle:1",
            "a=x-nv-aqos.enableRedundancyForMic:1",
            "a=x-nv-mic.frameSize:10",
            "a=x-nv-audio.jbConfig.initialThreshold:80",
            "m=audio 0 RTP/AVP 96 97",
            "a=rtpmap:96 opus/48000/2",
            "a=rtpmap:97 opus/16000/2",
            "a=fmtp:97 maxplaybackrate=16000",
        ])
    }
}
