import Foundation
import SwiftUI
import UIKit
import XCTest
@testable import OpenNOWiOS

final class SessionLimitTests: XCTestCase {
    func testSessionLimitTakesPrecedenceOverHTTPMembershipFallback() throws {
        let body = try JSONSerialization.data(withJSONObject: Self.refusal)
        let error = NSError(domain: "OpenNOW.Session", code: 403,
                            userInfo: [NSLocalizedDescriptionKey: String(decoding: body, as: UTF8.self)])
        let failure = OpenNOWFailure.classify(error, context: .launch)
        XCTAssertEqual(failure.kind, .sessionLimit)
        XCTAssertEqual(failure.recovery, .manageSessions)
        XCTAssertEqual(failure.code, "4AF1201A")
        XCTAssertFalse(failure.message.contains("membership"))
        XCTAssertEqual(failure.recovery.label, "Manage Sessions")
    }

    func testSessionLimitRecognizesProtocolStatusWithoutHTTP403() {
        for status: [String: Any] in [
            ["statusCode": 11],
            ["statusCode": "11"],
            ["unifiedErrorCode": 1_257_316_378],
            ["statusDescription": "SESSION_LIMIT_EXCEEDED_STATUS 4AF1201A"]
        ] {
            XCTAssertTrue(CloudMatchSessionLimitError.isSessionLimit(["requestStatus": status]))
        }
        XCTAssertFalse(CloudMatchSessionLimitError.isSessionLimit(["requestStatus": ["statusCode": 1]]))
        let failure = OpenNOWFailure.classify(CloudMatchSessionLimitError(sessions: [], code: nil), context: .launch)
        XCTAssertEqual(failure.kind, .sessionLimit)
        XCTAssertNil(failure.code)
    }

    func testForbiddenDoesNotInventAMembershipRequirement() {
        let generic = OpenNOWFailure.classify(NSError(domain: "OpenNOW.Session", code: 403), context: .launch)
        XCTAssertEqual(generic.kind, .rejected)
        XCTAssertEqual(generic.recovery, .reportProblem)
        let entitled = OpenNOWFailure.classify(NSError(domain: "OpenNOW.Session", code: 403,
            userInfo: [NSLocalizedDescriptionKey: #"{"requestStatus":{"statusDescription":"NOT_ENTITLED"}}"#]), context: .launch)
        XCTAssertEqual(entitled.kind, .notEntitled)
        XCTAssertEqual(entitled.recovery, .seePlans)
        XCTAssertEqual(OpenNOWFailure.classify(NSError(domain: "OpenNOW.Session", code: 401), context: .launch).kind, .authExpired)
    }

    func testCapturedQueuedSessionPreservesItsControlHostAndRequestSignature() throws {
        let candidate = try XCTUnwrap(GFNAPIClient.remoteSessionCandidates(in: [Self.queuedSession]).first)
        XCTAssertEqual(candidate.appId, "100013311")
        XCTAssertEqual(candidate.serverIp, "np-sof-02.cloudmatchbeta.nvidiagrid.net")
        XCTAssertEqual(candidate.sessionControlBaseUrl, "https://np-sof-02.cloudmatchbeta.nvidiagrid.net")
        XCTAssertEqual(candidate.resolution, "1920x1080")
        XCTAssertEqual(candidate.fps, 60)
        XCTAssertTrue(candidate.matchesStreamSettings(Self.settings, profile: Self.profile))
        let restored = try JSONDecoder().decode(RemoteSessionCandidate.self, from: JSONEncoder().encode(candidate))
        XCTAssertEqual(restored, candidate)
    }

    func testQueuedCompatibilityAllowsUnfinalizedMetadataButNotDifferentRequests() {
        let signature = StreamSettingsResolver.sessionSignature(for: Self.settings, profile: Self.profile)
        let queued = RemoteSessionCandidate(id: "queued", appId: "100013311", status: 1,
            serverIp: nil, streamSettingsSignature: signature, resolution: nil, fps: nil)
        XCTAssertTrue(queued.matchesStreamSettings(Self.settings, profile: Self.profile))
        let ready = RemoteSessionCandidate(id: "ready", appId: "100013311", status: 2,
            serverIp: nil, streamSettingsSignature: signature, resolution: nil, fps: nil)
        XCTAssertFalse(ready.matchesStreamSettings(Self.settings, profile: Self.profile))
        var changed = Self.settings
        changed.preferredCodec = "H264"
        XCTAssertFalse(queued.matchesStreamSettings(changed, profile: Self.profile))
        let downgraded = RemoteSessionCandidate(id: "ready", appId: "100013311", status: 2,
            serverIp: nil, streamSettingsSignature: signature, resolution: "1280x720", fps: 60)
        XCTAssertFalse(downgraded.matchesStreamSettings(Self.settings, profile: Self.profile))
    }

    @MainActor
    func testCreateRefusalRetainsOnlyTheExistingBlockingAllocation() async throws {
        let client = Self.client(protocolClass: SessionLimitRefusalProtocol.self)
        do {
            _ = try await client.startSession(session: Self.auth, game: OpenNOWStore.debugStorePreviewGames[0],
                vpcId: "NP-AMS-08", settings: Self.settings, streamProfile: Self.profile,
                streamingBaseUrl: "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net",
                launchAppIdOverride: "100013311", deviceId: "fixture-device")
            XCTFail("The provider refused the second allocation")
        } catch let limit as CloudMatchSessionLimitError {
            XCTAssertEqual(limit.sessions.map(\.id), ["existing-queue"])
            XCTAssertEqual(limit.code, "4AF1201A")
            XCTAssertEqual(OpenNOWFailure.classify(limit, context: .launch).recovery, .manageSessions)
        }
    }

    @MainActor
    func testQueuedResumePollsTheExistingControlHostWithoutCreatingOrClaiming() async throws {
        let client = Self.client(protocolClass: SessionLimitQueueProtocol.self)
        let candidate = try XCTUnwrap(GFNAPIClient.remoteSessionCandidates(in: [Self.queuedSession]).first)
        let active = try await client.resumeQueuedSession(session: Self.auth, candidate: candidate,
            game: OpenNOWStore.debugStorePreviewGames[0],
            streamingBaseUrl: "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net",
            vpcId: "NP-AMS-08", settings: Self.settings, deviceId: "fixture-device")
        XCTAssertEqual(active.id, "existing-queue")
        XCTAssertEqual(active.status, 1)
        XCTAssertEqual(active.queuePosition, 48)
        XCTAssertEqual(active.allocatedAppId, "100013311")
        XCTAssertEqual(SessionControlRouting.pollBase(for: active), "https://np-sof-02.cloudmatchbeta.nvidiagrid.net")
    }

    @MainActor
    func testManageSessionsRecoveryPresentsExistingQueueControls() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let previousKeyWindow = scene.windows.first(where: \.isKeyWindow)
        let store = OpenNOWStore()
        store.installDebugQueuePreview(position: 48)
        store.queueOverlayVisible = false
        let host = UIHostingController(rootView: MainTabView(initialPage: .store).environmentObject(store))
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer {
            store.sessionManagementPresented = false
            if let id = store.activeSession?.id { store.handleEndedSession(id: id) }
            window.isHidden = true
            window.rootViewController = nil
            previousKeyWindow?.makeKeyAndVisible()
        }
        try await Task.sleep(nanoseconds: 300_000_000)
        store.performRecovery(.manageSessions)
        XCTAssertTrue(store.sessionManagementPresented)
        XCTAssertNil(store.pendingSettingsRoute)
        try await Task.sleep(nanoseconds: 700_000_000)
        XCTAssertNotNil(host.presentedViewController)
        window.layoutIfNeeded()
        let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        let attachment = XCTAttachment(image: image)
        attachment.name = "Session-Limit-Recovery"
        attachment.lifetime = .keepAlways
        add(attachment)
        XCTAssertEqual(store.activeSession?.queuePosition, 48)
    }

    private static func client(protocolClass: AnyClass) -> GFNAPIClient {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [protocolClass]
        return GFNAPIClient(session: URLSession(configuration: configuration))
    }

    static var settings: AppSettings {
        var settings = AppSettings.default
        settings.streamPreset = .custom
        settings.preferredResolution = "1920x1080"
        settings.preferredFPS = 60
        settings.maxBitrateMbps = 18
        settings.preferredCodec = "H265"
        settings.preferredColorQuality = "8bit_420"
        settings.hdrEnabled = false
        settings.experimentalNativeNVSTEnabled = false
        return settings
    }

    static var profile: StreamVideoProfile {
        StreamVideoProfile(width: 1920, height: 1080, fps: 60, maxBitrateKbps: 18000)
    }

    static var auth: AuthSession {
        AuthSession(provider: LoginProvider(idpId: "fixture", code: "NVIDIA", displayName: "NVIDIA",
            streamingServiceUrl: "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net", priority: 0),
            tokens: AuthTokens(accessToken: "fixture", refreshToken: nil, idToken: nil,
                expiresAt: Date().timeIntervalSince1970 + 3600, clientToken: nil, clientTokenExpiresAt: nil),
            user: UserProfile(userId: "fixture-user", displayName: "Fixture", email: nil, membershipTier: "FREE"))
    }

    static var queuedSession: [String: Any] {
        ["sessionId": "existing-queue", "status": 1,
         "seatSetupInfo": ["queuePosition": 48, "seatSetupStep": 1],
         "sessionControlInfo": ["ip": "np-sof-02.cloudmatchbeta.nvidiagrid.net", "port": 443],
         "connectionInfo": [["usage": 14, "appLevelProtocol": 5,
                             "ip": ".cloudmatchbeta.nvidiagrid.net", "port": 443, "resourcePath": "/nvst/"]],
         "sessionRequestData": ["appId": 100013311,
            "clientRequestMonitorSettings": [["widthInPixels": 1920, "heightInPixels": 1080, "framesPerSecond": 60]],
            "metaData": [["key": "OpenNOWStreamSettingsSignature",
                          "value": StreamSettingsResolver.sessionSignature(for: settings, profile: profile)]]]]
    }

    static var refusal: [String: Any] {
        ["requestStatus": ["statusCode": 11, "statusDescription": "SESSION_LIMIT_EXCEEDED_STATUS 4AF1201A",
                           "unifiedErrorCode": 1_257_316_378],
         "otherUserSessions": [queuedSession], "session": ["sessionId": "rejected-new-allocation", "status": 1]]
    }
}

private class SessionLimitRefusalProtocol: URLProtocol, @unchecked Sendable {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        XCTAssertEqual(request.httpMethod, "POST")
        respond(status: 403, payload: SessionLimitTests.refusal)
    }

    func respond(status: Int, payload: [String: Any]) {
        do {
            let data = try JSONSerialization.data(withJSONObject: payload)
            let response = try XCTUnwrap(HTTPURLResponse(url: XCTUnwrap(request.url), statusCode: status,
                httpVersion: nil, headerFields: ["Content-Type": "application/json"]))
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}

private final class SessionLimitQueueProtocol: SessionLimitRefusalProtocol, @unchecked Sendable {
    override func startLoading() {
        XCTAssertEqual(request.httpMethod, "GET")
        XCTAssertEqual(request.url?.host, "np-sof-02.cloudmatchbeta.nvidiagrid.net")
        XCTAssertEqual(request.url?.path, "/v2/session/existing-queue")
        respond(status: 200, payload: ["requestStatus": ["statusCode": 1], "session": SessionLimitTests.queuedSession])
    }
}
