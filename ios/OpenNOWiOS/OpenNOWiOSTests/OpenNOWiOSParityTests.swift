import XCTest
import UIKit
import SwiftUI
@testable import OpenNOWiOS

final class OpenNOWiOSParityTests: XCTestCase {
    @MainActor
    func testIPadSettingsCategoriesCanReturnToHomeRepeatedly() async throws {
        try await verifySettingsCategoriesCanReturnToHome(compact: false)
    }

    @MainActor
    func testPhoneSettingsCategoriesCanReturnToHomeRepeatedly() async throws {
        try await verifySettingsCategoriesCanReturnToHome(compact: true)
    }

    @MainActor
    private func verifySettingsCategoriesCanReturnToHome(compact: Bool) async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let previousKeyWindow = scene.windows.first(where: \.isKeyWindow)
        let store = OpenNOWStore()
        store.settings.analyticsConsentAsked = true
        store.settings.analyticsOptOut = true
        let host = UIHostingController(rootView: MainTabView(initialPage: .store)
            .environmentObject(store).environment(\.horizontalSizeClass, compact ? .compact : .regular))
        if #available(iOS 17.0, *) { host.traitOverrides.horizontalSizeClass = compact ? .compact : .regular }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil; previousKeyWindow?.makeKeyAndVisible() }
        func descendants(_ view: UIView) -> [UIView] { [view] + view.subviews.flatMap(descendants) }
        func settle() async throws {
            try await Task.sleep(nanoseconds: 500_000_000)
            host.view.layoutIfNeeded()
        }
        func selectSidebarRow(_ row: Int) async throws {
            if compact {
                func controllers(_ controller: UIViewController) -> [UIViewController] {
                    [controller] + controller.children.flatMap(controllers)
                }
                let tabs = try XCTUnwrap(controllers(host).compactMap { $0 as? UITabBarController }.first)
                if #available(iOS 18.0, *), tabs.tabs.count > row {
                    let previous = tabs.selectedTab
                    let destination = tabs.tabs[row]
                    tabs.selectedTab = destination
                    tabs.delegate?.tabBarController?(tabs, didSelectTab: destination, previousTab: previous)
                } else {
                    let destination = try XCTUnwrap(tabs.viewControllers?[row])
                    tabs.selectedIndex = row
                    tabs.delegate?.tabBarController?(tabs, didSelect: destination)
                }
                try await settle()
                return
            }
            let sidebar = try XCTUnwrap(descendants(host.view).compactMap { $0 as? UICollectionView }
                .first {
                    $0.numberOfSections > 0 && $0.numberOfItems(inSection: 0) == 4
                        && $0.convert($0.bounds, to: host.view).minX < host.view.bounds.width * 0.25
                        && $0.bounds.width < host.view.bounds.width * 0.5
                })
            let index = IndexPath(item: row, section: 0)
            sidebar.delegate?.collectionView?(sidebar, didSelectItemAt: index)
            sidebar.selectItem(at: index, animated: false, scrollPosition: [])
            try await settle()
        }
        func titles() -> [String] {
            descendants(host.view).compactMap { ($0 as? UINavigationBar)?.topItem?.title }
        }
        try await settle()
        XCTAssertEqual(host.traitCollection.horizontalSizeClass, compact ? .compact : .regular)
        for route in [SettingsRouteTarget.general, .stream, .input, .interface, .account] {
            try await selectSidebarRow(3)
            let settingsTitles = ["Settings", "General", "Stream", "Input", "Interface", "Account"]
            XCTAssertTrue(titles().contains(where: settingsTitles.contains),
                          "Selection must open the actual Settings view: \(titles())")
            store.pendingSettingsRoute = route
            try await settle()
            XCTAssertNil(store.pendingSettingsRoute)
            XCTAssertTrue(titles().contains(String(describing: route).capitalized),
                          "The requested settings category must actually be pushed: \(titles())")
            try await selectSidebarRow(0)
            XCTAssertTrue(titles().contains("OpenNOW") && !titles().contains(where: settingsTitles.contains),
                          "Home navigation must replace the settings stack: \(titles())")
        }
    }


    func testAccountSnapshotRoundTripsSubscriptionStorageAndConnections() throws {
        let storage = StorageAddon(
            type: "STORAGE",
            sizeGb: 200,
            usedGb: 75,
            regionName: "Malaysia",
            regionCode: "MY-KUL",
            status: "OK",
            subType: "PERMANENT_STORAGE",
            autoPayEnabled: true
        )
        let snapshot = CachedAccountSnapshot(
            schemaVersion: 1,
            cachedAt: 123,
            membershipTier: "ULTIMATE",
            subscription: SubscriptionSnapshot(
                membershipTier: "ULTIMATE",
                subscriptionType: "PAID",
                subscriptionSubType: "UNLIMITED",
                isGamePlayAllowed: true,
                isUnlimited: true,
                remainingHours: 80,
                totalHours: 100,
                storageAddon: storage
            ),
            accountConnectors: [
                AccountConnector(
                    store: "STEAM",
                    label: "Steam",
                    supported: true,
                    required: false,
                    userDisplayName: "Player",
                    userIdentifier: "steam-user",
                    expiresInSeconds: nil,
                    syncedGameCount: 40,
                    syncState: "DONE",
                    syncDate: nil
                )
            ],
            availableRegions: [StreamRegion(name: "Malaysia", url: "https://my.example/")],
            vpcId: "MY-YES"
        )

        let decoded = try JSONDecoder().decode(
            CachedAccountSnapshot.self,
            from: JSONEncoder().encode(snapshot)
        )

        XCTAssertEqual(decoded, snapshot)
        XCTAssertEqual(decoded.subscription?.storageAddon?.usedGb, 75)
        XCTAssertEqual(decoded.accountConnectors.first?.store, "STEAM")
    }

    func testAccountErrorsPreferParsedServerMessageOverRawJSON() {
        let error = NSError(
            domain: "OpenNOW.Auth",
            code: 401,
            userInfo: [
                NSLocalizedDescriptionKey: #"{"errors":[{"errorMessage":"This saved account needs a fresh sign-in."}]}"#
            ]
        )

        XCTAssertEqual(
            OpenNOWErrorPresenter.message(for: error, fallback: "Sign-in failed."),
            "This saved account needs a fresh sign-in."
        )
    }

    func testAccountErrorsHumanizeGFNStatusCodes() {
        let error = NSError(
            domain: "OpenNOW.Account",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: #"{"requestStatus":{"statusDescription":"AUTHENTICATION_REQUIRED_STATUS"}}"#]
        )

        XCTAssertEqual(
            OpenNOWErrorPresenter.message(for: error, fallback: "Refresh failed."),
            "Authentication Required"
        )
    }

    func testAccountTimeoutErrorIsActionable() {
        let error = NSError(domain: NSURLErrorDomain, code: NSURLErrorTimedOut)

        XCTAssertEqual(
            OpenNOWErrorPresenter.message(for: error, fallback: "Refresh failed."),
            "The request timed out. Check your connection and try again."
        )
    }

    func testPlaceholderSDPEndpointsUseSignalingEndpointWithoutMediaMetadata() {
        let offer = remoteOffer(address: "0.0.0.0", port: 47_998)

        let fixed = NativeStreamSDP.fixServerIP(
            in: offer,
            serverIP: "66-22-131-132.cloudmatchbeta.nvidiagrid.net"
        )

        XCTAssertTrue(fixed.contains("c=IN IP4 0.0.0.0"))
        XCTAssertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 66.22.131.132 47998 typ host"))
    }

    func testPrivateSDPEndpointsUseCloudMatchMediaEndpoint() {
        let offer = remoteOffer(address: "10.0.175.0", port: 47_998)

        let fixed = NativeStreamSDP.fixServerEndpoint(
            in: offer,
            serverIP: "183-78-14-231.yes.geforcenow.nvidiagrid.net",
            mediaIP: "183-78-14-231.yes.geforcenow.nvidiagrid.net",
            mediaPort: 14_317
        )

        XCTAssertTrue(fixed.contains("c=IN IP4 10.0.175.0"))
        XCTAssertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 183-78-14-231.yes.geforcenow.nvidiagrid.net 14317 typ host"))
    }

    func testPrivateSDPEndpointsStayAdvertisedWithoutMediaMetadata() {
        let offer = remoteOffer(address: "10.0.175.0", port: 47_998)

        let fixed = NativeStreamSDP.fixServerIP(
            in: offer,
            serverIP: "183-78-14-231.yes.geforcenow.nvidiagrid.net"
        )

        XCTAssertEqual(fixed, offer)
    }

    func testCarrierGradeNATSDPEndpointsUseCloudMatchMediaEndpoint() {
        let offer = remoteOffer(address: "100.96.10.4", port: 47_998)

        let fixed = NativeStreamSDP.fixServerEndpoint(
            in: offer,
            serverIP: "183-78-14-231.yes.geforcenow.nvidiagrid.net",
            mediaIP: "183.78.14.231",
            mediaPort: 19_353
        )

        XCTAssertTrue(fixed.contains("c=IN IP4 100.96.10.4"))
        XCTAssertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 183.78.14.231 19353 typ host"))
    }

    func testPublicSDPEndpointsUseExplicitCloudMatchMediaEndpoint() {
        let offer = remoteOffer(address: "203.0.113.10", port: 47_998)

        let fixed = NativeStreamSDP.fixServerEndpoint(
            in: offer,
            serverIP: "183-78-14-231.yes.geforcenow.nvidiagrid.net",
            mediaIP: "183.78.14.231",
            mediaPort: 19_353
        )

        XCTAssertTrue(fixed.contains("c=IN IP4 203.0.113.10"))
        XCTAssertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 183.78.14.231 19353 typ host"))
    }

    func testTrickledRemoteCandidateUsesCloudMatchMediaEndpoint() {
        let candidate = "candidate:2 1 udp 2122260223 100.96.10.4 47998 typ host generation 0"

        let fixed = NativeStreamSDP.rewriteIceCandidateEndpoint(
            candidate,
            mediaIP: "183.78.14.231",
            mediaPort: 19_353
        )

        XCTAssertEqual(
            fixed,
            "candidate:2 1 udp 2122260223 183.78.14.231 19353 typ host generation 0"
        )
    }

    func testExplicitMediaEndpointDoesNotDependOnSignalingHostnameShape() {
        let offer = remoteOffer(address: "100.96.10.4", port: 47_998)

        let fixed = NativeStreamSDP.fixServerEndpoint(
            in: offer,
            serverIP: "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net",
            mediaIP: "183.78.14.231",
            mediaPort: 19_353
        )

        XCTAssertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 183.78.14.231 19353 typ host"))
    }

    func testAndroidInputHandshakeIsAppliedAndPrimesReliableChannel() {
        let bridge = NativeStreamInputBridge()
        let sink = RecordingNativeStreamInputSink()
        bridge.sink = sink

        XCTAssertEqual(bridge.handleServerHandshake(Data([0x0e, 0x02, 0x03, 0x00])), 3)
        XCTAssertGreaterThanOrEqual(sink.reliablePackets.count, 2)
        XCTAssertEqual(sink.reliablePackets.first, Data([0x02, 0x00, 0x00, 0x00]))

        let packetCount = sink.reliablePackets.count
        XCTAssertNil(bridge.handleServerHandshake(Data([0xff, 0x00])))
        XCTAssertEqual(sink.reliablePackets.count, packetCount)
    }

    func testHapticsAvailabilityIsReadvertisedAfterAndroidParityInterval() {
        let bridge = NativeStreamInputBridge()
        let sink = RecordingNativeStreamInputSink()
        bridge.sink = sink

        bridge.advertiseHaptics(force: true, now: 100)
        bridge.advertiseHaptics(now: 104.9)
        XCTAssertEqual(sink.reliablePackets.count, 1)

        bridge.advertiseHaptics(now: 105)
        XCTAssertEqual(sink.reliablePackets.count, 2)
    }

    private func remoteOffer(address: String, port: Int) -> String {
        """
        v=0
        c=IN IP4 \(address)
        m=video \(port) UDP/TLS/RTP/SAVPF 96
        a=candidate:1 1 udp 2122260223 \(address) \(port) typ host generation 0
        a=rtpmap:96 H264/90000
        """
    }

    func testNvstRequestMatchesAndroidStartupAndPacingContract() {
        var settings = AppSettings.default
        settings.preferredCodec = "H264"
        let profile = StreamVideoProfile(width: 1_280, height: 720, fps: 60, maxBitrateKbps: 13_000)
        let nvst = NativeStreamSDP.buildNvstSDP(
            offerSDP: "a=ri.partialReliableThresholdMs:30",
            localAnswerSDP: "a=ice-ufrag:u\na=ice-pwd:p\na=fingerprint:sha-256 AA:BB",
            profile: profile,
            settings: settings,
            codec: .h264
        )

        XCTAssertTrue(nvst.contains("a=vqos.adjustStreamingFpsDuringOutOfFocus:0"))
        XCTAssertTrue(nvst.contains("a=packetPacing.numGroups:5"))
        XCTAssertTrue(nvst.contains("a=video.initialBitrateKbps:9100"))
        XCTAssertTrue(nvst.contains("a=video.initialPeakBitrateKbps:13000"))
        XCTAssertTrue(nvst.contains("a=vqos.bw.minimumBitrateKbps:5000"))
        XCTAssertFalse(nvst.contains("a=vqos.drc.minRequiredBitrateCheckEnabled"))
        XCTAssertFalse(nvst.contains("a=vqos.bllFec.enable"))
    }

    func testStreamPresetsMatchAndroidValuesAndRespectAppleTierFPSCaps() {
        var base = AppSettings.default
        base.preferredAspectRatio = "16:9"

        let low = StreamSettingsResolver.settings(base, applying: .lowDataSaver, membershipTier: "FREE")
        XCTAssertEqual(low.streamPreset, .lowDataSaver)
        XCTAssertEqual(low.preferredResolution, "1366x768")
        XCTAssertEqual(low.preferredFPS, 30)
        XCTAssertEqual(low.maxBitrateMbps, 12)
        XCTAssertEqual(low.preferredQuality, "Data Saver")

        let medium = StreamSettingsResolver.settings(base, applying: .medium, membershipTier: "PERFORMANCE")
        XCTAssertEqual(medium.streamPreset, .medium)
        XCTAssertEqual(medium.preferredResolution, "1920x1080")
        XCTAssertEqual(medium.preferredFPS, 60)
        XCTAssertEqual(medium.maxBitrateMbps, 35)
        XCTAssertEqual(medium.preferredQuality, "Balanced")

        let freeHigh = StreamSettingsResolver.settings(base, applying: .high, membershipTier: "FREE")
        XCTAssertEqual(freeHigh.preferredResolution, "1920x1080")
        XCTAssertEqual(freeHigh.preferredFPS, 60)

        let ultimateHigh = StreamSettingsResolver.settings(base, applying: .high, membershipTier: "ULTIMATE")
        XCTAssertEqual(ultimateHigh.streamPreset, .high)
        XCTAssertEqual(ultimateHigh.preferredResolution, "2560x1440")
        XCTAssertEqual(ultimateHigh.preferredFPS, 120)
        XCTAssertEqual(ultimateHigh.maxBitrateMbps, 75)
        XCTAssertEqual(ultimateHigh.preferredQuality, "Quality")

        var excessiveFPS = base
        excessiveFPS.preferredResolution = "1920x1080"
        excessiveFPS.preferredFPS = 360
        let cappedProfile = deterministicProfile(for: excessiveFPS, membershipTier: "ULTIMATE")
        XCTAssertEqual(cappedProfile.fps, 120)
    }

    func testTwentyByNineResolutionCatalogIncludesEveryAndroidChoice() {
        let choices = StreamSettingsResolver.choices(forAspectRatio: "20:9")
        XCTAssertEqual(
            choices.map(\.value),
            ["1600x720", "2400x1080", "3200x1440", "4800x2160"]
        )
        XCTAssertEqual(
            choices.map(\.requiredPlan),
            [.free, .priority, .priority, .ultimate]
        )
    }

    func testArbitraryCustomResolutionIsPreservedWithinTierAndRejectedAboveTier() {
        XCTAssertTrue(
            StreamSettingsResolver.customResolutionIsAvailable(
                width: 1_800,
                height: 1_000,
                membershipTier: "FREE"
            )
        )

        var withinTier = AppSettings.default
        withinTier.preferredAspectRatio = "16:9"
        withinTier.preferredResolution = "1800x1000"
        let preserved = deterministicProfile(for: withinTier, membershipTier: "FREE")
        XCTAssertEqual(preserved.resolutionString, "1800x1000")

        XCTAssertFalse(
            StreamSettingsResolver.customResolutionIsAvailable(
                width: 2_200,
                height: 1_200,
                membershipTier: "FREE"
            )
        )

        var aboveTier = withinTier
        aboveTier.preferredResolution = "2200x1200"
        let rejected = deterministicProfile(for: aboveTier, membershipTier: "FREE")
        XCTAssertEqual(rejected.resolutionString, "1920x1080")
    }

    func testLegacySettingsDecodeWithSafeDefaultsAndMigration() throws {
        let legacyJSON = Data(
            """
            {
              "preferredRegion": "US East",
              "preferredResolution": "1920x1080",
              "preferredFPS": 60,
              "preferredQuality": "Balanced",
              "preferredCodec": "Auto",
              "maxBitrateMbps": 0,
              "keyboardLayout": "en-US",
              "gameLanguage": "en_US",
              "enableL4S": false,
              "enableCloudGsync": false,
              "keepMicEnabled": false,
              "showStatsOverlay": true,
              "hideServerSelector": false,
              "queueLiveActivitiesEnabled": true,
              "selectedProviderIdpId": "legacy-provider",
              "fortnitePrefersNativeTouch": true,
              "favoriteGameIds": ["game-1"]
            }
            """.utf8
        )

        let settings = try JSONDecoder().decode(AppSettings.self, from: legacyJSON)

        XCTAssertEqual(settings.preferredRegion, "")
        XCTAssertEqual(settings.preferredAspectRatio, "16:9")
        XCTAssertEqual(settings.streamPreset, .custom)
        XCTAssertFalse(settings.sessionProxyEnabled)
        XCTAssertEqual(settings.sessionProxyUrl, "")
        XCTAssertFalse(settings.streamSharpeningEnabled)
        XCTAssertEqual(settings.streamSharpeningAmount, 0.25)
        XCTAssertEqual(settings.mouseSensitivity, 1)
        XCTAssertEqual(settings.mouseAcceleration, 1)
        XCTAssertTrue(settings.fingerMouseEnabled)
        XCTAssertTrue(settings.phoneRumbleFallback)
        XCTAssertEqual(settings.launchPage, .store)
        XCTAssertEqual(settings.posterSizeScale, 1)
        XCTAssertTrue(settings.compactGameCards)
        XCTAssertTrue(settings.showGameStoreLabels)
        XCTAssertTrue(settings.sessionCounterEnabled)
        XCTAssertFalse(settings.nerdMode)
        XCTAssertFalse(settings.catalogWallpaperEnabled)
        XCTAssertNil(settings.catalogWallpaperFilename)
        XCTAssertFalse(settings.streamTutorialCompleted)
        XCTAssertFalse(settings.controllerTouchPromptDismissed)
        XCTAssertFalse(settings.showSessionReportAfterStream)
        XCTAssertEqual(settings.sessionReportDefaultVersion, 1)
        XCTAssertFalse(settings.streamKeyboardClearConfirmationDisabled)
        XCTAssertEqual(settings.streamerPreferences, .default)
        XCTAssertEqual(settings.defaultGameVariantIds, [:])
        XCTAssertEqual(settings.favoriteGameIds, ["game-1"])

        let roundTrip = try JSONDecoder().decode(
            AppSettings.self,
            from: JSONEncoder().encode(settings)
        )
        XCTAssertEqual(roundTrip, settings)
    }

    func testSafeVideoFallbackCapsExpensiveAndUnsupportedSettings() {
        var settings = AppSettings.default
        settings.preferredAspectRatio = "16:9"
        settings.preferredResolution = "5120x2880"
        settings.preferredFPS = 120
        settings.maxBitrateMbps = 100
        settings.preferredCodec = "AV1"
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.hdrEnabled = true
        settings.enableCloudGsync = true

        let fallback = settings.safeVideoFallback()

        XCTAssertEqual(fallback.preferredResolution, "1920x1080")
        XCTAssertEqual(fallback.preferredAspectRatio, "16:9")
        XCTAssertEqual(fallback.preferredFPS, 60)
        XCTAssertEqual(fallback.maxBitrateMbps, 75)
        XCTAssertEqual(fallback.preferredCodec, "H264")
        XCTAssertEqual(fallback.preferredColorQuality, StreamColorQuality.eightBit420.rawValue)
        XCTAssertFalse(fallback.hdrEnabled)
        XCTAssertFalse(fallback.enableCloudGsync)
    }

    func testExplicitUnsupportedCodecDoesNotRewriteSelectedProfile() {
        var settings = AppSettings.default
        settings.preferredResolution = "3840x2160"
        settings.preferredFPS = 120
        settings.maxBitrateMbps = 100
        settings.preferredCodec = "AV1"
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.hdrEnabled = true

        let report = NativeStreamCodecReport(capabilities: [
            NativeStreamCodecCapability(
                codec: .av1,
                videoToolboxHardwareDecode: false,
                webRTCSupported: true,
                webRTCProfileSummary: []
            ),
            NativeStreamCodecCapability(
                codec: .h264,
                videoToolboxHardwareDecode: true,
                webRTCSupported: true,
                webRTCProfileSummary: []
            )
        ])
        let resolved = NativeStreamLaunchSettingsResolver.resolve(settings, codecReport: report)

        XCTAssertEqual(resolved.selectedCodec, .av1)
        XCTAssertEqual(resolved.settings.preferredCodec, "AV1")
        XCTAssertEqual(resolved.settings.preferredResolution, "3840x2160")
        XCTAssertEqual(resolved.settings.preferredFPS, 120)
        XCTAssertEqual(resolved.settings.maxBitrateMbps, 100)
        XCTAssertEqual(resolved.settings.preferredColorQuality, StreamColorQuality.tenBit444.rawValue)
        XCTAssertTrue(resolved.settings.hdrEnabled)
    }

    func testCloudGameDecodesLegacyCachedPayloadWithoutNewCatalogFields() throws {
        let legacyJSON = Data(
            """
            {
              "id": "legacy-game",
              "title": "Legacy Game",
              "genre": "Action",
              "platform": "GeForce NOW",
              "icon": "gamecontroller.fill",
              "launchOptions": []
            }
            """.utf8
        )

        let game = try JSONDecoder().decode(CloudGame.self, from: legacyJSON)

        XCTAssertEqual(game.id, "legacy-game")
        XCTAssertEqual(game.title, "Legacy Game")
        XCTAssertNil(game.catalogSectionId)
        XCTAssertNil(game.catalogSectionTitle)
        XCTAssertNil(game.contentRatings)
    }

    func testCurrentGFNContentRatingObjectParsesForGameDetails() throws {
        let data = Data(
            #"""
            {
              "contentRatings": {
                "type": "ESRB",
                "categoryKey": "T",
                "contentDescriptorKeys": ["VIOLENCE", "STRONG_LANGUAGE"],
                "interactiveElementKeys": ["USERS_INTERACT", "IN_GAME_PURCHASES"]
              }
            }
            """#.utf8
        )
        let app = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])

        XCTAssertEqual(
            GFNContentRatingParser.labels(from: app["contentRatings"]),
            ["ESRB T", "Violence", "Strong Language", "Users Interact", "In Game Purchases"]
        )
    }

    func testLegacyRatingArraysRemainSupportedAndDeduplicated() throws {
        let data = Data(
            #"""
            {
              "contentRatings": [
                "PEGI 12",
                {"displayName": "USK 12"},
                "PEGI 12"
              ]
            }
            """#.utf8
        )
        let app = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])

        XCTAssertEqual(
            GFNContentRatingParser.labels(from: app["contentRatings"]),
            ["PEGI 12", "USK 12"]
        )
    }

    func testCatalogFallbackPreservesDescriptionsWithoutRestoringStaleLaunchers() throws {
        let cached = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"""
                {
                  "id": "game",
                  "title": "Game",
                  "genre": "Action",
                  "platform": "Steam",
                  "icon": "gamecontroller.fill",
                  "launchAppId": "123",
                  "launchOptions": [{"storefront":"Steam","appId":"123"}],
                  "summary": "Cached description",
                  "stores": ["Steam"],
                  "playType": "ACCOUNT_LINKED",
                  "membershipTierLabel": "ULTIMATE",
                  "contentRatings": ["ESRB T"]
                }
                """#.utf8
            )
        )
        let fetched = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"""
                {
                  "id": "game",
                  "title": "Game",
                  "genre": "Action",
                  "platform": "GeForce NOW",
                  "icon": "gamecontroller.fill",
                  "launchOptions": []
                }
                """#.utf8
            )
        )

        let merged = fetched.fillingMissingMetadata(from: cached)

        XCTAssertEqual(merged.summary, "Cached description")
        XCTAssertEqual(merged.contentRatings, ["ESRB T"])
        XCTAssertNil(merged.launchAppId)
        XCTAssertTrue(merged.launchOptions.isEmpty)
        XCTAssertNil(merged.stores)
        XCTAssertNil(merged.playType)
        XCTAssertNil(merged.membershipTierLabel)
    }

    func testPrintedWasteAutoRoutingMatchesAndroidLatencyWeighting() throws {
        let lowLatency = PrintedWasteZone(
            id: "low-latency",
            region: "US",
            queuePosition: 100,
            etaMs: nil,
            zoneUrl: "https://low-latency.example",
            pingMs: 10,
            isMeasuring: false,
            regionSuffix: "west"
        )
        let shortQueue = PrintedWasteZone(
            id: "short-queue",
            region: "US",
            queuePosition: 1,
            etaMs: nil,
            zoneUrl: "https://short-queue.example",
            pingMs: 50,
            isMeasuring: false,
            regionSuffix: "east"
        )
        let unpinged = PrintedWasteZone(
            id: "unpinged",
            region: "US",
            queuePosition: 0,
            etaMs: nil,
            zoneUrl: "https://unpinged.example",
            pingMs: nil,
            isMeasuring: true,
            regionSuffix: "unknown"
        )

        XCTAssertEqual(
            recommendedPrintedWasteZone(in: [lowLatency, shortQueue, unpinged])?.id,
            lowLatency.id
        )
    }

    func testBombayZoneCanBeSelectedByIDURLAndAutomaticRouting() {
        XCTAssertFalse(StreamZonePolicy.isBlocked("np-bom-01"))
        XCTAssertFalse(StreamZonePolicy.isBlocked("https://np-bom-01.cloudmatchbeta.nvidiagrid.net/"))
        XCTAssertFalse(StreamZonePolicy.isBlocked("NP-BOM-01.CLOUDMATCHBETA.NVIDIAGRID.NET"))
        XCTAssertFalse(StreamZonePolicy.isBlocked("np-bom-02"))

        let blocked = PrintedWasteZone(
            id: "NP-BOM-01",
            region: "IN",
            queuePosition: 1,
            etaMs: nil,
            zoneUrl: "https://np-bom-01.cloudmatchbeta.nvidiagrid.net/",
            pingMs: 1,
            isMeasuring: false,
            regionSuffix: "bom-01"
        )
        let allowed = PrintedWasteZone(
            id: "NP-AMS-02",
            region: "EU",
            queuePosition: 30,
            etaMs: nil,
            zoneUrl: "https://np-ams-02.cloudmatchbeta.nvidiagrid.net/",
            pingMs: 40,
            isMeasuring: false,
            regionSuffix: "ams-02"
        )

        XCTAssertEqual(recommendedPrintedWasteZone(in: [blocked, allowed])?.id, blocked.id)
    }

    func testAndroidBitrateAndLanguageChoicesRemainAvailable() {
        XCTAssertEqual(
            StreamSettingsResolver.bitrateOptionsMbps,
            [0, 5, 10, 15, 20, 25, 30, 35, 40, 50, 60, 75, 100]
        )
        XCTAssertTrue(StreamSettingsResolver.keyboardLayoutOptions.contains { $0.value == "zh-TW" })
        XCTAssertTrue(StreamSettingsResolver.keyboardLayoutOptions.contains { $0.value == "ru-RU" })
        for language in ["th_TH", "vi_VN", "id_ID", "uk_UA", "nl_NL", "no_NO"] {
            XCTAssertTrue(
                StreamSettingsResolver.gameLanguageOptions.contains { $0.value == language },
                "Missing game language \(language)"
            )
        }
    }

    func testSessionLimitsUseAndroidDurationsAcrossMembershipAliases() {
        for alias in ["FREE", "free-tier", nil] as [String?] {
            XCTAssertEqual(
                streamSessionLimit(for: alias),
                StreamSessionLimit(tierLabel: "Free", limitHours: 1, mode: .countdown)
            )
        }

        for alias in ["PRIORITY", "PERFORMANCE", "PREMIUM", "FOUNDERS"] {
            XCTAssertEqual(
                streamSessionLimit(for: alias),
                StreamSessionLimit(tierLabel: "Performance", limitHours: 6, mode: .stopwatch),
                "Incorrect session limit for \(alias)"
            )
        }

        for alias in ["ULTIMATE", "RTX 3080"] {
            XCTAssertEqual(
                streamSessionLimit(for: alias),
                StreamSessionLimit(tierLabel: "Ultimate", limitHours: 8, mode: .stopwatch),
                "Incorrect session limit for \(alias)"
            )
        }
    }

    func testSessionWarningTrackerArmsWithoutWarningOnFirstSample() {
        var tracker = StreamSessionWarningTracker()

        XCTAssertNil(tracker.nextWarning(remainingSeconds: 30 * 60))
        XCTAssertEqual(tracker.previousRemainingSeconds, 30 * 60)
        XCTAssertTrue(tracker.warnedThresholds.isEmpty)
    }

    func testSessionWarningTrackerChoosesMostUrgentSkippedThreshold() {
        var tracker = StreamSessionWarningTracker()

        XCTAssertNil(tracker.nextWarning(remainingSeconds: 10 * 60 + 1))
        XCTAssertEqual(tracker.nextWarning(remainingSeconds: 5 * 60 - 1), 5 * 60)
        XCTAssertEqual(tracker.warnedThresholds, [5 * 60])
    }

    func testSessionWarningTrackerWarnsEachThresholdOnlyOnce() {
        var tracker = StreamSessionWarningTracker()

        XCTAssertNil(tracker.nextWarning(remainingSeconds: 10 * 60 + 1))
        XCTAssertEqual(tracker.nextWarning(remainingSeconds: 10 * 60), 10 * 60)
        XCTAssertNil(tracker.nextWarning(remainingSeconds: 10 * 60 + 1))
        XCTAssertNil(tracker.nextWarning(remainingSeconds: 10 * 60))
        XCTAssertEqual(tracker.warnedThresholds, [10 * 60])
    }

    func testUnicodeInputPacketsPreserveScalarBoundariesAndCharacterLimit() throws {
        let encoder = NativeStreamInputEncoder()
        let text = String(repeating: "a", count: 1_015) + "😀B"
        let batch = encoder.encodeUnicodeText(text)

        XCTAssertEqual(batch.characterCount, text.count)
        XCTAssertEqual(batch.packets.count, 2)
        var reconstructed = ""
        for packet in batch.packets {
            let bytes = [UInt8](packet)
            XCTAssertEqual(Array(bytes.prefix(5)), [0x22, 0x17, 0, 0, 0])
            XCTAssertLessThanOrEqual(bytes.count - 5, 1_016)
            reconstructed += try XCTUnwrap(String(data: packet.dropFirst(5), encoding: .utf8))
        }
        XCTAssertEqual(reconstructed, text)

        let limited = encoder.encodeUnicodeText(String(repeating: "x", count: 4_100))
        XCTAssertEqual(limited.characterCount, 4_096)
        XCTAssertEqual(limited.packets.reduce(0) { $0 + max(0, $1.count - 5) }, 4_096)
    }

    // MARK: - Native touch

    func testTouchBatchMatchesTheAndroidWireLayout() throws {
        let encoder = NativeStreamInputEncoder()
        // Protocol 3 is what the handshake settles on, so packets arrive frame-wrapped.
        encoder.setProtocolVersion(3)

        let packet = try XCTUnwrap(
            encoder.encodeTouchBatch([
                NativeTouchRecord(slot: 0, phase: NativeTouchPhase.down, x: 0x1234, y: 0x5678, radiusX: 7, radiusY: 9, timestampUs: 0x0102_0304_0506_0708),
                NativeTouchRecord(slot: 3, phase: NativeTouchPhase.move, x: 65_535, y: 0, timestampUs: 1)
            ])
        )
        let bytes = [UInt8](packet)

        // 10-byte single-message frame, then the payload.
        XCTAssertEqual(bytes[0], 0x23)
        XCTAssertEqual(bytes[9], 0x22)
        let payload = Array(bytes.dropFirst(10))
        XCTAssertEqual(payload.count, 8 + 16 * 2)

        // Opcode 24 little-endian, then size and count big-endian.
        XCTAssertEqual(Array(payload.prefix(4)), [24, 0, 0, 0])
        XCTAssertEqual(Array(payload[4..<6]), [0, UInt8(8 + 32)])
        XCTAssertEqual(Array(payload[6..<8]), [0, 2])

        XCTAssertEqual(payload[8], 0)
        XCTAssertEqual(payload[9], NativeTouchPhase.down)
        XCTAssertEqual(Array(payload[10..<12]), [0x12, 0x34])
        XCTAssertEqual(Array(payload[12..<14]), [0x56, 0x78])
        XCTAssertEqual(payload[14], 7)
        XCTAssertEqual(payload[15], 9)
        XCTAssertEqual(Array(payload[16..<24]), [1, 2, 3, 4, 5, 6, 7, 8])

        XCTAssertEqual(payload[24], 3)
        XCTAssertEqual(payload[25], NativeTouchPhase.move)
        XCTAssertEqual(Array(payload[26..<28]), [0xFF, 0xFF])

        XCTAssertNil(encoder.encodeTouchBatch([]))
    }

    func testTouchSlotsAreReusedRatherThanClimbingWithPointerIdentity() {
        var allocator = NativeTouchSlotAllocator<Int>()

        XCTAssertEqual(allocator.acquire(41), 0)
        XCTAssertEqual(allocator.acquire(42), 1)
        // Re-acquiring an already tracked finger keeps its slot.
        XCTAssertEqual(allocator.acquire(41), 0)
        XCTAssertEqual(allocator.release(41), 0)
        // The freed slot is the lowest one available, not the next integer up.
        XCTAssertEqual(allocator.acquire(43), 0)
        XCTAssertEqual(allocator.activeCount, 2)

        for identity in 100..<106 {
            XCTAssertNotNil(allocator.acquire(identity))
        }
        // Eight concurrent fingers is the host's limit; a ninth is dropped rather than sent.
        XCTAssertNil(allocator.acquire(200))
        XCTAssertNil(allocator.release(200))
    }

    func testTouchPointsUndoLetterboxingAndPresentationZoom() {
        let viewSize = CGSize(width: 1_000, height: 500)
        let streamSize = CGSize(width: 1_920, height: 1_080)

        // 16:9 in a 2:1 view is pillarboxed: 889 points wide, 55.5 points of bar each side.
        let centre = NativeTouchGeometry.streamPoint(
            touch: CGPoint(x: 500, y: 250),
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: false
        )
        XCTAssertEqual(centre.x, 960, accuracy: 0.5)
        XCTAssertEqual(centre.y, 540, accuracy: 0.5)

        // A finger on the pillarbox bar maps outside the picture when clamping is off, which is
        // how the batch builder knows to drop it.
        let onBar = NativeTouchGeometry.streamPoint(
            touch: CGPoint(x: 10, y: 250),
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: false,
            clamp: false
        )
        XCTAssertLessThan(onBar.x, 0)

        // Fill mode really fills, so the same point is inside the picture.
        let stretched = NativeTouchGeometry.streamPoint(
            touch: CGPoint(x: 10, y: 250),
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: true,
            clamp: false
        )
        XCTAssertEqual(stretched.x, 1_920 * 0.01, accuracy: 0.5)

        // Zoomed 2x with no pan, the centre is unmoved and a point halfway to the edge maps to a
        // point a quarter of the way there in the source.
        let zoomedCentre = NativeTouchGeometry.streamPoint(
            touch: CGPoint(x: 500, y: 250),
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: true,
            zoomScale: 2
        )
        XCTAssertEqual(zoomedCentre.x, 960, accuracy: 0.5)
        let zoomedQuarter = NativeTouchGeometry.streamPoint(
            touch: CGPoint(x: 750, y: 250),
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: true,
            zoomScale: 2
        )
        XCTAssertEqual(zoomedQuarter.x, 1_920 * 0.625, accuracy: 0.5)
    }

    func testTouchBatchDropsFingersOffThePictureButNeverSwallowsALift() {
        let viewSize = CGSize(width: 1_000, height: 500)
        let streamSize = CGSize(width: 1_920, height: 1_080)
        var allocator = NativeTouchSlotAllocator<Int>()

        let onPicture = NativeTouchGeometry.buildBatch(
            allocator: &allocator,
            phase: NativeTouchPhase.down,
            pointers: [NativeTouchPointerSample(pointer: 1, location: CGPoint(x: 500, y: 250))],
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: false
        )
        XCTAssertEqual(onPicture.count, 1)
        XCTAssertEqual(onPicture.first?.slot, 0)
        XCTAssertEqual(onPicture.first?.x ?? 0, nativeTouchCoordinateMax / 2, accuracy: 1)

        // A press on the pillarbox bar belongs to nothing and takes no slot.
        let onBar = NativeTouchGeometry.buildBatch(
            allocator: &allocator,
            phase: NativeTouchPhase.down,
            pointers: [NativeTouchPointerSample(pointer: 2, location: CGPoint(x: 4, y: 250))],
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: false
        )
        XCTAssertTrue(onBar.isEmpty)
        XCTAssertEqual(allocator.activeCount, 1)

        // A lift is reported wherever the finger ended up — swallowing one leaves the host holding
        // that contact down for the rest of the session.
        let lift = NativeTouchGeometry.buildBatch(
            allocator: &allocator,
            phase: NativeTouchPhase.up,
            pointers: [NativeTouchPointerSample(pointer: 1, location: CGPoint(x: -400, y: 250))],
            viewSize: viewSize,
            streamSize: streamSize,
            stretchToFill: false
        )
        XCTAssertEqual(lift.count, 1)
        XCTAssertEqual(lift.first?.phase, NativeTouchPhase.up)
        XCTAssertEqual(lift.first?.slot, 0)
        XCTAssertEqual(allocator.activeCount, 0)
    }

    func testEveryHeldFingerIsCancelledWhenTheSurfaceGoesAway() {
        var allocator = NativeTouchSlotAllocator<Int>()
        XCTAssertEqual(allocator.acquire(1), 0)
        XCTAssertEqual(allocator.acquire(2), 1)

        let cancels = NativeTouchGeometry.cancelAll(allocator: &allocator)
        XCTAssertEqual(cancels.count, 2)
        XCTAssertTrue(cancels.allSatisfy { $0.phase == NativeTouchPhase.cancel })
        XCTAssertEqual(Set(cancels.map(\.slot)), [0, 1])
        XCTAssertEqual(allocator.activeCount, 0)
        XCTAssertTrue(NativeTouchGeometry.cancelAll(allocator: &allocator).isEmpty)
    }

    func testNativeTouchReachesTheReliableChannelAsOnePacketPerEvent() {
        let bridge = NativeStreamInputBridge()
        let sink = RecordingNativeStreamInputSink()
        bridge.sink = sink
        bridge.configure(protocolVersion: 3, partiallyReliableGamepadMask: 0)

        XCTAssertTrue(
            bridge.sendNativeTouch([
                NativeTouchRecord(slot: 0, phase: NativeTouchPhase.down, x: 100, y: 200),
                NativeTouchRecord(slot: 1, phase: NativeTouchPhase.down, x: 300, y: 400)
            ])
        )
        // One packet, not one per finger, and never on the lossy channel: a dropped lift is
        // uncorrectable.
        XCTAssertEqual(sink.reliablePackets.count, 1)
        XCTAssertTrue(sink.partiallyReliablePackets.isEmpty)
        XCTAssertFalse(bridge.sendNativeTouch([]))
        XCTAssertEqual(sink.reliablePackets.count, 1)
    }

    func testVirtualControllerMergesIntoPrimaryPhysicalControllerLikeAndroid() {
        let physical = NativeStreamGamepadState(
            controllerId: 2,
            buttons: NativeStreamVirtualGamepadButton.a.rawValue,
            leftTrigger: 40,
            rightTrigger: 100,
            leftStickX: 100,
            leftStickY: 200,
            rightStickX: 300,
            rightStickY: 400,
            connected: true
        )
        let virtual = NativeStreamVirtualGamepadState(
            buttons: NativeStreamVirtualGamepadButton.b.rawValue,
            leftTrigger: 80,
            rightTrigger: 20,
            leftStickX: -1_000,
            leftStickY: 1_000,
            rightStickX: -2_000,
            rightStickY: 2_000,
            leftStickActive: true,
            rightStickActive: false
        )

        let merged = NativeStreamGamepadMixer.merging(physical: physical, virtual: virtual)

        XCTAssertEqual(merged.controllerId, 2)
        XCTAssertEqual(
            merged.buttons,
            NativeStreamVirtualGamepadButton.a.rawValue | NativeStreamVirtualGamepadButton.b.rawValue
        )
        XCTAssertEqual(merged.leftTrigger, 80)
        XCTAssertEqual(merged.rightTrigger, 100)
        XCTAssertEqual(merged.leftStickX, -1_000)
        XCTAssertEqual(merged.leftStickY, 1_000)
        XCTAssertEqual(merged.rightStickX, 300)
        XCTAssertEqual(merged.rightStickY, 400)
        XCTAssertTrue(merged.connected)
    }

    func testFortniteUsesItsPersistedMobileTouchLayoutOnlyWhenEnabled() {
        var settings = AppSettings.default
        XCTAssertEqual(
            streamTouchLayoutProfile(gameTitle: "Fortnite Festival", settings: settings),
            "fortnite-mobile"
        )
        XCTAssertEqual(settings.touchLayout(for: "fortnite-mobile"), .fortniteMobile)

        // The Fortnite-only switch has been replaced by the touch-mode picker ported from
        // Android. Turning touch off entirely must still fall back to the default layout.
        settings.touch.nativeTouchMode = .never
        XCTAssertEqual(
            streamTouchLayoutProfile(gameTitle: "FORTNITE", settings: settings),
            "default"
        )
        XCTAssertEqual(
            streamTouchLayoutProfile(gameTitle: "Aimlabs", settings: AppSettings.default),
            "default"
        )
    }

    func testLegacyFortniteTouchFlagMigratesToTouchMode() throws {
        // Payload from a build that predates the touch-mode picker: it has the Fortnite switch
        // and no `touch` object at all. Someone who turned that switch off must not have touch
        // silently re-enabled by the upgrade.
        let legacyOff = Data(#"{"fortnitePrefersNativeTouch":false}"#.utf8)
        let migratedOff = try JSONDecoder().decode(AppSettings.self, from: legacyOff)
        XCTAssertEqual(migratedOff.touch.nativeTouchMode, .never)
        XCTAssertFalse(migratedOff.fortnitePrefersNativeTouch)

        let legacyOn = Data(#"{"fortnitePrefersNativeTouch":true}"#.utf8)
        let migratedOn = try JSONDecoder().decode(AppSettings.self, from: legacyOn)
        XCTAssertEqual(migratedOn.touch.nativeTouchMode, .automatic)
        XCTAssertTrue(migratedOn.fortnitePrefersNativeTouch)

        // An explicit `touch` object always wins over the legacy flag.
        let explicit = Data(#"{"fortnitePrefersNativeTouch":false,"touch":{"nativeTouchMode":"always"}}"#.utf8)
        let migratedExplicit = try JSONDecoder().decode(AppSettings.self, from: explicit)
        XCTAssertEqual(migratedExplicit.touch.nativeTouchMode, .always)
        XCTAssertTrue(migratedExplicit.fortnitePrefersNativeTouch)
    }

    func testSessionReportsBecomeOptInOnceAndPreserveLaterChoice() throws {
        let migrated = try JSONDecoder().decode(
            AppSettings.self,
            from: Data(#"{"showSessionReportAfterStream":true}"#.utf8)
        )
        XCTAssertFalse(migrated.showSessionReportAfterStream)
        XCTAssertEqual(migrated.sessionReportDefaultVersion, 1)

        let optedIn = try JSONDecoder().decode(
            AppSettings.self,
            from: Data(#"{"showSessionReportAfterStream":true,"sessionReportDefaultVersion":1}"#.utf8)
        )
        XCTAssertTrue(optedIn.showSessionReportAfterStream)
        XCTAssertEqual(optedIn.sessionReportDefaultVersion, 1)
    }

    func testNativeTouchFollowsCatalogCapabilityRatherThanTitle() {
        let touchGame = OpenNOWiOSParityTests.makeGame(
            title: "Genshin Impact",
            controls: ["GAMEPAD", "TOUCHSCREEN"]
        )
        let desktopGame = OpenNOWiOSParityTests.makeGame(
            title: "Cyberpunk 2077",
            controls: ["GAMEPAD", "KEYBOARD_MOUSE"]
        )

        XCTAssertTrue(NativeTouchSupport.catalogClaimsTouchSupport(touchGame))
        XCTAssertFalse(NativeTouchSupport.catalogClaimsTouchSupport(desktopGame))

        XCTAssertTrue(NativeTouchSupport.shouldUseNativeTouch(mode: .automatic, game: touchGame))
        XCTAssertFalse(NativeTouchSupport.shouldUseNativeTouch(mode: .automatic, game: desktopGame))
        XCTAssertTrue(NativeTouchSupport.shouldUseNativeTouch(mode: .always, game: desktopGame))
        XCTAssertFalse(NativeTouchSupport.shouldUseNativeTouch(mode: .never, game: touchGame))

        // A session-level choice to use the on-screen controller beats the catalog hint.
        XCTAssertFalse(
            NativeTouchSupport.shouldUseNativeTouchForStream(
                mode: .automatic,
                game: touchGame,
                preferVirtualController: true
            )
        )
    }

    func testSessionLaunchModeAgreesWithTheStreamTouchDecision() {
        let touchGame = OpenNOWiOSParityTests.makeGame(
            title: "Genshin Impact",
            controls: ["TOUCHSCREEN"]
        )
        let desktopGame = OpenNOWiOSParityTests.makeGame(
            title: "Cyberpunk 2077",
            controls: ["GAMEPAD", "KEYBOARD_MOUSE"]
        )

        // The host provisions its virtual input devices from this once. If it disagrees with what
        // the stream decides, touch is dead for the whole session and nothing says so — which is
        // exactly how native touch shipped broken.
        XCTAssertEqual(
            NativeTouchSupport.appLaunchMode(mode: .automatic, game: touchGame),
            .touchFriendly
        )
        XCTAssertEqual(
            NativeTouchSupport.appLaunchMode(mode: .automatic, game: desktopGame),
            .default
        )
        XCTAssertEqual(
            NativeTouchSupport.appLaunchMode(mode: .always, game: desktopGame),
            .touchFriendly
        )
        XCTAssertEqual(
            NativeTouchSupport.appLaunchMode(mode: .never, game: touchGame),
            .default
        )

        // The value on the wire is the one the official client gates its touch pipeline on.
        XCTAssertEqual(GFNAppLaunchMode.default.rawValue, 1)
        XCTAssertEqual(GFNAppLaunchMode.gamepadFriendly.rawValue, 2)
        XCTAssertEqual(GFNAppLaunchMode.touchFriendly.rawValue, 3)
    }

    func testTouchLaunchModeIgnoresTheRequestedVideoProfile() {
        let touchGame = OpenNOWiOSParityTests.makeGame(
            title: "Genshin Impact",
            controls: ["TOUCHSCREEN"]
        )

        // The touch identity keeps the desktop allocation matrix, so asking for 1440p, 120 fps or
        // HDR no longer costs the digitizer. The old envelope check declined touch above 1080p60
        // while the stream went on sending contacts regardless.
        for mode in [NativeTouchMode.automatic, .always] {
            XCTAssertEqual(
                NativeTouchSupport.appLaunchMode(mode: mode, game: touchGame),
                .touchFriendly,
                "mode \(mode)"
            )
        }
    }

    // MARK: - Failure classification

    func testFailuresAreClassifiedIntoSomethingActionable() {
        let offline = OpenNOWFailure.classify(
            NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet),
            context: .catalog
        )
        XCTAssertEqual(offline.kind, .offline)
        XCTAssertEqual(offline.recovery, .retry)
        XCTAssertFalse(offline.message.contains("Error Domain"), "Raw NSError text must never reach the screen")

        let timeout = OpenNOWFailure.classify(
            NSError(domain: NSURLErrorDomain, code: NSURLErrorTimedOut),
            context: .launch
        )
        XCTAssertEqual(timeout.kind, .timeout)

        // Capacity during a launch offers the server picker; elsewhere that would be meaningless.
        let busyLaunch = OpenNOWFailure.classify(
            NSError(domain: "OpenNOW.Session", code: 503),
            context: .launch
        )
        XCTAssertEqual(busyLaunch.kind, .capacity)
        XCTAssertEqual(busyLaunch.recovery, .changeServer)
        let busyElsewhere = OpenNOWFailure.classify(
            NSError(domain: "OpenNOW.Session", code: 503),
            context: .catalog
        )
        XCTAssertEqual(busyElsewhere.recovery, .retry)

        let expired = OpenNOWFailure.classify(
            NSError(domain: "OpenNOW.Auth", code: 0, userInfo: [NSLocalizedDescriptionKey: "invalid_grant"]),
            context: .account
        )
        XCTAssertEqual(expired.kind, .authExpired)
        XCTAssertEqual(expired.recovery, .signIn)
    }

    func testUnknownFailuresCarryACodeWorthPasting() {
        let failure = OpenNOWFailure.classify(
            NSError(domain: "OpenNOW.Weird", code: 918),
            context: .launch
        )
        XCTAssertEqual(failure.kind, .unknown)
        XCTAssertEqual(failure.recovery, .reportProblem)
        XCTAssertEqual(failure.message, "The game couldn't start.")
        XCTAssertEqual(failure.code, "OpenNOW.Weird 918")
    }

    func testServerJSONNeverReachesTheScreenVerbatim() {
        let htmlish = OpenNOWFailure.classify(
            NSError(
                domain: "OpenNOW.Session",
                code: 500,
                userInfo: [NSLocalizedDescriptionKey: "<html><body>Internal Server Error</body></html>"]
            ),
            context: .launch
        )
        XCTAssertFalse(htmlish.message.contains("<html>"))

        // A real sentence from the server is better than anything invented here, and is used.
        let explained = OpenNOWFailure.classify(
            NSError(
                domain: "OpenNOW.Session",
                code: 400,
                userInfo: [NSLocalizedDescriptionKey: #"{"message":"That game is not available in your region."}"#]
            ),
            context: .launch
        )
        XCTAssertEqual(explained.message, "That game is not available in your region.")
    }

    // MARK: - Queue progress

    func testQueueProgressOnlyMovesForward() {
        var estimator = QueueTrendEstimator()
        let start = Date(timeIntervalSince1970: 2_000_000)

        // Too few samples to say anything.
        estimator.record(position: 100, at: start)
        XCTAssertNil(estimator.progress())

        for step in 0...5 {
            estimator.record(position: 100 - step * 10, at: start.addingTimeInterval(Double(step) * 30))
        }
        // 50 of 100 consumed.
        XCTAssertEqual(estimator.progress() ?? 0, 0.5, accuracy: 0.001)

        // Further movement only increases it.
        estimator.record(position: 20, at: start.addingTimeInterval(200))
        XCTAssertEqual(estimator.progress() ?? 0, 0.8, accuracy: 0.001)
    }

    func testQueueProgressStaysSilentWhenNothingHasDrained() {
        var estimator = QueueTrendEstimator()
        let start = Date(timeIntervalSince1970: 2_000_000)
        for step in 0...5 {
            estimator.record(position: 40, at: start.addingTimeInterval(Double(step) * 30))
        }
        XCTAssertNil(estimator.progress(), "A bar drawn from no movement is a fake bar")
    }

    // MARK: - In-stream behaviour

    func testModeChangeNoticeOnlyFiresOnARealDifference() {
        let requested = StreamVideoProfile(width: 1_920, height: 1_080, fps: 60, maxBitrateKbps: 35_000)

        // Same geometry: silence.
        XCTAssertNil(
            StreamModeChangeNotice.between(requested: requested, deliveredResolution: "1920x1080", reason: .serverNegotiated)
        )
        // No decoded frame yet.
        XCTAssertNil(
            StreamModeChangeNotice.between(requested: requested, deliveredResolution: nil, reason: .serverNegotiated)
        )
        // CloudMatch's provisional monitor profile is below any real stream and must not surface.
        XCTAssertNil(
            StreamModeChangeNotice.between(requested: requested, deliveredResolution: "16x16", reason: .serverNegotiated)
        )
        XCTAssertNil(
            StreamModeChangeNotice.between(requested: requested, deliveredResolution: "garbage", reason: .serverNegotiated)
        )

        let notice = StreamModeChangeNotice.between(
            requested: requested,
            deliveredResolution: "1600x900",
            reason: .serverNegotiated
        )
        XCTAssertNotNil(notice)
        XCTAssertTrue(notice?.message.contains("1600x900") ?? false)
        XCTAssertTrue(notice?.message.contains("1920x1080") ?? false)
    }

    func testModeChangeNoticeBlamesTheRightParty() {
        let requested = StreamVideoProfile(width: 2_560, height: 1_440, fps: 60, maxBitrateKbps: 45_000)
        let recovered = StreamModeChangeNotice.between(
            requested: requested,
            deliveredResolution: "1920x1080",
            reason: .safeRecovery("H265 stalled.")
        )
        // When OpenNOW dropped the profile itself, the copy must not imply the server did it.
        XCTAssertTrue(recovered?.message.contains("keep the stream up") ?? false)
        XCTAssertFalse(recovered?.message.contains("Server chose") ?? true)

        let negotiated = StreamModeChangeNotice.between(
            requested: requested,
            deliveredResolution: "1920x1080",
            reason: .serverNegotiated
        )
        XCTAssertTrue(negotiated?.message.contains("Server chose") ?? false)
    }

    func testStickDeadZoneRescalesRatherThanClips() {
        // Inside the threshold the stick is silent.
        var result = TouchStickMath.applyDeadZone(x: 0.05, y: 0, deadZone: 0.2)
        XCTAssertEqual(result.0, 0, accuracy: 0.0001)
        XCTAssertEqual(result.1, 0, accuracy: 0.0001)

        // Full deflection still reaches full output — clipping would cost the top of the range.
        result = TouchStickMath.applyDeadZone(x: 1, y: 0, deadZone: 0.2)
        XCTAssertEqual(result.0, 1, accuracy: 0.0001)

        // Halfway past the threshold lands halfway through the remaining travel.
        result = TouchStickMath.applyDeadZone(x: 0.6, y: 0, deadZone: 0.2)
        XCTAssertEqual(result.0, 0.5, accuracy: 0.0001)

        // Zero threshold is a straight pass-through.
        result = TouchStickMath.applyDeadZone(x: 0.03, y: -0.04, deadZone: 0)
        XCTAssertEqual(result.0, 0.03, accuracy: 0.0001)
        XCTAssertEqual(result.1, -0.04, accuracy: 0.0001)
    }

    func testControllerCursorCurveKeepsPrecisionNearCentre() {
        // A resting stick must produce nothing, or the cursor walks across the screen.
        XCTAssertEqual(NativeStreamInputBridge.curvedStickAxis(0.1, deadZone: 0.12), 0, accuracy: 0.0001)
        // Full deflection is full speed.
        XCTAssertEqual(NativeStreamInputBridge.curvedStickAxis(1, deadZone: 0.12), 1, accuracy: 0.0001)
        // Squared response: half the travel is a quarter of the speed, which is what makes fine
        // aiming possible with a thumbstick.
        let half = NativeStreamInputBridge.curvedStickAxis(0.56, deadZone: 0.12)
        XCTAssertEqual(half, 0.25, accuracy: 0.01)
        // Sign is preserved.
        XCTAssertLessThan(NativeStreamInputBridge.curvedStickAxis(-1, deadZone: 0.12), 0)
    }

    func testPhysicalMouseYUsesHostScreenCoordinates() {
        let upward = NativeStreamMouseCoordinatePolicy.hostDelta(deltaX: 4, deltaY: 9)
        XCTAssertEqual(upward.x, 4)
        XCTAssertEqual(upward.y, -9)

        let downward = NativeStreamMouseCoordinatePolicy.hostDelta(deltaX: -3, deltaY: -7)
        XCTAssertEqual(downward.x, -3)
        XCTAssertEqual(downward.y, 7)
    }

    func testDecoderRecoveryRequiresSustainedLocalOverloadEvidence() {
        var gate = NativeStreamDecoderRecoveryGate(badSamplesBeforeRecovery: 3)
        for _ in 0..<2 {
            XCTAssertFalse(gate.observe(
                receivedFPS: 60,
                decodedFPS: 34,
                decodeMs: 38,
                requestedFPS: 60,
                advancedCodecActive: true,
                recoveryEligible: true
            ))
        }
        XCTAssertTrue(gate.observe(
            receivedFPS: 60,
            decodedFPS: 34,
            decodeMs: 38,
            requestedFPS: 60,
            advancedCodecActive: true,
            recoveryEligible: true
        ))
        XCTAssertFalse(gate.observe(
            receivedFPS: 60,
            decodedFPS: 34,
            decodeMs: 38,
            requestedFPS: 60,
            advancedCodecActive: true,
            recoveryEligible: true
        ))
    }

    func testDecoderRecoveryDoesNotBlameNetworkOrH264() {
        var gate = NativeStreamDecoderRecoveryGate(badSamplesBeforeRecovery: 2)
        for _ in 0..<4 {
            XCTAssertFalse(gate.observe(
                receivedFPS: 18,
                decodedFPS: 18,
                decodeMs: 40,
                requestedFPS: 60,
                advancedCodecActive: true,
                recoveryEligible: true
            ))
            XCTAssertFalse(gate.observe(
                receivedFPS: 60,
                decodedFPS: 30,
                decodeMs: 40,
                requestedFPS: 60,
                advancedCodecActive: false,
                recoveryEligible: true
            ))
        }
    }

    func testPacketLossRecoveryRequestsOneKeyframeAfterCongestionClears() {
        var gate = NativeStreamPacketLossRecoveryGate(
            badSamplesBeforeArmed: 2,
            minimumPacketSample: 100,
            cooldownSamples: 2
        )
        XCTAssertFalse(gate.observe(lostDelta: 10, receivedDelta: 90, recoveryEligible: true))
        XCTAssertFalse(gate.observe(lostDelta: 8, receivedDelta: 92, recoveryEligible: true))
        XCTAssertTrue(gate.observe(lostDelta: 0, receivedDelta: 120, recoveryEligible: true))
        XCTAssertFalse(gate.observe(lostDelta: 0, receivedDelta: 120, recoveryEligible: true))

        // Counter resets and inactive transports cannot arm or fire recovery.
        XCTAssertFalse(gate.observe(lostDelta: nil, receivedDelta: nil, recoveryEligible: true))
        XCTAssertFalse(gate.observe(lostDelta: 10, receivedDelta: 90, recoveryEligible: false))
    }

    func testSignalingFailureDispositionAndStableMediaPreservation() {
        XCTAssertEqual(
            nativeStreamSignalingFailureDisposition("Expected HTTP 101 but received 404 Not Found http=404"),
            .recoverSession
        )
        XCTAssertEqual(nativeStreamSignalingFailureDisposition("http=410 Gone"), .sessionEnded)
        XCTAssertEqual(nativeStreamSignalingFailureDisposition("http=503 Service Unavailable"), .retrySignaling)
        XCTAssertEqual(nativeStreamSignalingFailureDisposition("code=1000"), .retryTransport)

        XCTAssertTrue(nativeStreamShouldPreserveMediaAfterSignalingFailure(.retryTransport, mediaConnected: true))
        XCTAssertTrue(nativeStreamShouldPreserveMediaAfterSignalingFailure(.retrySignaling, mediaConnected: true))
        XCTAssertFalse(nativeStreamShouldPreserveMediaAfterSignalingFailure(.recoverSession, mediaConnected: true))
        XCTAssertFalse(nativeStreamShouldPreserveMediaAfterSignalingFailure(.sessionEnded, mediaConnected: true))
        XCTAssertFalse(nativeStreamShouldPreserveMediaAfterSignalingFailure(.retryTransport, mediaConnected: false))
    }

    func testQueueReadyChimeAnnouncesOncePerSession() {
        QueueReadyAlert.reset()
        QueueReadyAlert.announceIfNeeded(sessionId: "a", isReady: true, enabled: true)
        XCTAssertTrue(QueueReadyAlert.hasAnnounced(sessionId: "a"))

        // Repeated ready polls must not chime again.
        QueueReadyAlert.announceIfNeeded(sessionId: "a", isReady: true, enabled: true)
        XCTAssertTrue(QueueReadyAlert.hasAnnounced(sessionId: "a"))

        // Not ready, or disabled, records nothing.
        QueueReadyAlert.announceIfNeeded(sessionId: "b", isReady: false, enabled: true)
        XCTAssertFalse(QueueReadyAlert.hasAnnounced(sessionId: "b"))
        QueueReadyAlert.announceIfNeeded(sessionId: "c", isReady: true, enabled: false)
        XCTAssertFalse(QueueReadyAlert.hasAnnounced(sessionId: "c"))

        // A relaunch of the same session id can chime again once it has been forgotten.
        QueueReadyAlert.forget(sessionId: "a")
        QueueReadyAlert.announceIfNeeded(sessionId: "a", isReady: true, enabled: true)
        XCTAssertTrue(QueueReadyAlert.hasAnnounced(sessionId: "a"))
        QueueReadyAlert.reset()
    }

    // MARK: - Bug reports

    func testBugReportDescriptionNeedsSubstanceNotJustLength() {
        // Long enough by character count, but says nothing.
        let padded = String(repeating: "aaaa ", count: 20)
        XCTAssertNotNil(BugReportValidation.descriptionError(padded))

        // The same word repeated clears the word count but not the distinct-word floor.
        let repeated = Array(repeating: "broken", count: 12).joined(separator: " ")
        XCTAssertNotNil(BugReportValidation.descriptionError(repeated))

        // Too short.
        XCTAssertNotNil(BugReportValidation.descriptionError("it doesnt work"))

        // Punctuation does not count toward the meaningful-character floor.
        XCTAssertNotNil(BugReportValidation.descriptionError(String(repeating: ".", count: 200)))

        // A real report passes.
        let real = """
        I launched Cyberpunk on the US Southwest server and the picture froze after about \
        thirty seconds while the audio kept playing. Reconnecting did not help; ending the \
        session and starting again did.
        """
        XCTAssertNil(BugReportValidation.descriptionError(real))
    }

    func testBugReportTitleRejectsMashingAndEmptiness() {
        XCTAssertNotNil(BugReportValidation.titleError(""))
        XCTAssertNotNil(BugReportValidation.titleError("   "))
        XCTAssertNotNil(BugReportValidation.titleError("bug"))
        XCTAssertNotNil(BugReportValidation.titleError("aaaaaaaaaaaa"))
        XCTAssertNil(BugReportValidation.titleError("Video freezes but audio continues"))
    }

    func testBugReportProgressCopyCountsOnlyMeaningfulCharacters() {
        XCTAssertEqual(BugReportValidation.meaningfulCharacterCount("ab! cd?"), 4)
        XCTAssertEqual(BugReportValidation.meaningfulCharacterCount("12 34"), 4)
        let progress = BugReportValidation.descriptionProgress("short")
        XCTAssertTrue(progress?.contains("5 / 50") ?? false, "Progress should show real characters, got \(progress ?? "nil")")
    }

    func testReporterIdIsStableNamespacedAndNotTheRawDeviceId() {
        let deviceId = "8F1C0F7E-0000-4000-8000-ABCDEF012345"
        guard let first = BugReportReporter.reporterId(stableDeviceId: deviceId) else {
            return XCTFail("Expected a reporter id")
        }
        XCTAssertEqual(first, BugReportReporter.reporterId(stableDeviceId: deviceId), "Must be stable")
        XCTAssertTrue(BugReportReporter.isValid(first))
        XCTAssertTrue(first.hasPrefix("br1_"))
        XCTAssertEqual(first.count, 4 + 64)
        XCTAssertFalse(first.contains(deviceId), "The raw device ID must never be uploaded")

        // A different install is a different reporter.
        XCTAssertNotEqual(first, BugReportReporter.reporterId(stableDeviceId: "other-device"))
        XCTAssertNil(BugReportReporter.reporterId(stableDeviceId: "   "))
        XCTAssertFalse(BugReportReporter.isValid("br1_short"))
        XCTAssertFalse(BugReportReporter.isValid(String(repeating: "a", count: 64)))
    }

    func testBugReportServerErrorPrefersTheServersOwnMessage() {
        let banned = """
        {"ok":false,"error":{"code":"REPORTER_BANNED","message":"Bug reporting is disabled for this installation.","retryable":false}}
        """
        guard case .server(let code, let message, let retryable) =
                BugReportClient.parseServerError(body: banned, status: 403) else {
            return XCTFail("Expected a server error")
        }
        XCTAssertEqual(code, "REPORTER_BANNED")
        XCTAssertEqual(message, "Bug reporting is disabled for this installation.")
        XCTAssertFalse(retryable)

        // An HTML proxy page must not reach the screen.
        guard case .server(_, let fallback, _) =
                BugReportClient.parseServerError(body: "<html><body>502 Bad Gateway</body></html>", status: 502) else {
            return XCTFail("Expected a server error")
        }
        XCTAssertFalse(fallback.contains("<html>"))
        XCTAssertTrue(fallback.contains("502"))

        // Rate limits are retryable even when the server does not say so.
        guard case .server(_, _, let rateLimited) =
                BugReportClient.parseServerError(body: "{}", status: 429) else {
            return XCTFail("Expected a server error")
        }
        XCTAssertTrue(rateLimited)
    }

    func testBugReportRequestIsRejectedBeforeItLeavesTheDevice() {
        let valid = BugReportSubmission(
            title: "Video freezes but audio continues",
            description: """
            I launched Cyberpunk on the US Southwest server and the picture froze after about \
            thirty seconds while the audio kept playing. Reconnecting did not help.
            """,
            versionName: "1.1",
            versionCode: "100",
            reporterId: BugReportReporter.reporterId(stableDeviceId: "device")!,
            metadata: "{}",
            attachments: []
        )
        XCTAssertNoThrow(try BugReportClient.buildRequest(valid))

        var badReporter = valid
        badReporter = BugReportSubmission(
            title: valid.title,
            description: valid.description,
            versionName: valid.versionName,
            versionCode: valid.versionCode,
            reporterId: "not-a-reporter-id",
            metadata: valid.metadata,
            attachments: valid.attachments
        )
        XCTAssertThrowsError(try BugReportClient.buildRequest(badReporter))

        let tooManyFiles = BugReportSubmission(
            title: valid.title,
            description: valid.description,
            versionName: valid.versionName,
            versionCode: valid.versionCode,
            reporterId: valid.reporterId,
            metadata: valid.metadata,
            attachments: (0..<6).map {
                BugReportAttachment(fileName: "log\($0).txt", contentType: "text/plain", data: Data())
            }
        )
        XCTAssertThrowsError(try BugReportClient.buildRequest(tooManyFiles))

        let oversizedFile = BugReportSubmission(
            title: valid.title,
            description: valid.description,
            versionName: valid.versionName,
            versionCode: valid.versionCode,
            reporterId: valid.reporterId,
            metadata: valid.metadata,
            attachments: [
                BugReportAttachment(
                    fileName: "large.log",
                    contentType: "text/plain",
                    data: Data(count: BugReportEndpoint.maxFileBytes + 1)
                )
            ]
        )
        XCTAssertThrowsError(try BugReportClient.buildRequest(oversizedFile))
    }

    func testBugReportRequestCarriesTheRedactedDebugFile() throws {
        let submission = BugReportSubmission(
            title: "First session has no game audio",
            description: "I launched a game for the first time and video started normally, but no sound played from the phone speakers.",
            versionName: "1.1",
            versionCode: "103",
            reporterId: BugReportReporter.reporterId(stableDeviceId: "device")!,
            metadata: "{}",
            attachments: [
                BugReportAttachment(
                    fileName: "opennow-ios-logs-20260827-120000.txt",
                    contentType: "text/plain; charset=utf-8",
                    data: Data("strictRedaction=true\nlastError=none\n".utf8)
                ),
                BugReportAttachment(
                    fileName: "stream-state.json",
                    contentType: "application/json",
                    data: Data("{}".utf8)
                )
            ]
        )

        let request = try BugReportClient.buildRequest(submission)
        XCTAssertEqual(request.url, BugReportEndpoint.url)
        XCTAssertEqual(request.url?.absoluteString, "https://api.printedwaste.com/releases/opennow-ios/bug-reports")
        let body = String(decoding: try XCTUnwrap(request.httpBody), as: UTF8.self)
        XCTAssertTrue(body.contains("name=\"platform\"\r\n\r\nios"))
        XCTAssertEqual(body.components(separatedBy: "name=\"files\"").count - 1, 2)
        XCTAssertTrue(body.contains("filename=\"opennow-ios-logs-20260827-120000.txt\""))
        XCTAssertTrue(body.contains("filename=\"stream-state.json\""))
        XCTAssertTrue(body.contains("strictRedaction=true"))
    }

    func testBugReportReceiptIsRequiredAndBounded() {
        XCTAssertNil(BugReportClient.reference(from: #"{"ok":true}"#))
        XCTAssertEqual(
            BugReportClient.reference(from: #"{"ok":true,"reportId":"  BR-123  "}"#),
            "BR-123"
        )
        let longID = String(repeating: "x", count: 200)
        XCTAssertEqual(
            BugReportClient.reference(from: #"{"id":"\#(longID)"}"#)?.count,
            160
        )
    }

    func testBugReportMetadataCarriesTheKnownIssueDecision() throws {
        let deck = BugReportPreflightDeck(items: [
            BugReportPreflightItem(label: "Device", value: "iPhone16,1, iOS 18.2"),
            BugReportPreflightItem(
                label: "Known issue",
                value: "Resolution drops on a weak connection",
                kind: .knownIssue(key: "ios-resolution-fallback-weak-link")
            )
        ])
        let json = BugReportMetadata.build(deck: deck, overridesKnownIssue: true)
        let parsed = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any]
        )
        XCTAssertEqual(parsed["platform"] as? String, "ios")
        XCTAssertEqual(parsed["knownIssueKey"] as? String, "ios-resolution-fallback-weak-link")
        XCTAssertEqual(parsed["knownIssueOverride"] as? Bool, true)
        let preflight = try XCTUnwrap(parsed["preflight"] as? [String: String])
        XCTAssertEqual(preflight["Device"], "iPhone16,1, iOS 18.2")

        // No known issue means no override key at all, rather than a false one.
        let plain = BugReportMetadata.build(
            deck: BugReportPreflightDeck(items: [deck.items[0]]),
            overridesKnownIssue: false
        )
        let plainParsed = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(plain.utf8)) as? [String: Any]
        )
        XCTAssertNil(plainParsed["knownIssueOverride"])
    }

    // MARK: - Launch conflict

    func testLaunchOverALiveSessionAsksBeforeDiscardingIt() {
        let running = Self.makeGame(title: "Halo Infinite", controls: ["GAMEPAD"])
        let wanted = Self.makeGame(title: "Cyberpunk 2077", controls: ["GAMEPAD"])
        let request = PendingLaunchRequest(game: wanted, zoneUrl: nil, launchOption: nil)

        // Queued, setting up and ready all hold a rig, so all three must confirm.
        for status in 1...3 {
            let conflict = LaunchConflict.between(
                active: Self.makeActiveSession(game: running, status: status),
                request: request
            )
            XCTAssertNotNil(conflict, "status \(status) holds a rig and must warn before being discarded")
            XCTAssertTrue(conflict?.message.contains("Halo Infinite") ?? false)
            XCTAssertTrue(conflict?.message.contains("Cyberpunk 2077") ?? false)
        }
    }

    func testLaunchConflictStaysQuietWhenThereIsNothingToLose() {
        let running = Self.makeGame(title: "Halo Infinite", controls: ["GAMEPAD"])
        let wanted = Self.makeGame(title: "Cyberpunk 2077", controls: ["GAMEPAD"])

        // No session at all.
        XCTAssertNil(
            LaunchConflict.between(
                active: nil,
                request: PendingLaunchRequest(game: wanted, zoneUrl: nil, launchOption: nil)
            )
        )

        // Relaunching the same game claims the existing session rather than replacing it.
        XCTAssertNil(
            LaunchConflict.between(
                active: Self.makeActiveSession(game: running, status: 2),
                request: PendingLaunchRequest(game: running, zoneUrl: nil, launchOption: nil)
            )
        )

        // A session that has already released its rig is not worth a dialog.
        for status in [0, 4, 5, 6, 7] {
            XCTAssertNil(
                LaunchConflict.between(
                    active: Self.makeActiveSession(game: running, status: status),
                    request: PendingLaunchRequest(game: wanted, zoneUrl: nil, launchOption: nil)
                ),
                "status \(status) no longer holds a rig"
            )
        }
    }

    private static func makeActiveSession(game: CloudGame, status: Int) -> ActiveSession {
        ActiveSession(
            id: "session-\(game.id)",
            game: game,
            startedAt: Date(timeIntervalSince1970: 1_000_000),
            status: status,
            queuePosition: nil,
            seatSetupStep: nil,
            serverIp: nil,
            mediaIp: nil,
            mediaPort: 0,
            signalingServer: nil,
            signalingUrl: nil,
            iceServers: [],
            zone: "test",
            streamingBaseUrl: "https://example.invalid",
            clientId: "client",
            deviceId: "device",
            adState: nil
        )
    }

    // MARK: - Queue trend

    func testQueueEstimateStaysSilentUntilItHasEvidence() {
        var estimator = QueueTrendEstimator()
        let start = Date(timeIntervalSince1970: 1_000_000)

        // One reading says nothing.
        estimator.record(position: 50, at: start)
        XCTAssertEqual(estimator.trend(now: start), .unknown)
        XCTAssertNil(estimator.estimate(now: start))
        XCTAssertNil(estimator.supportLine(now: start))

        // Two more readings twenty seconds apart is still under the observation floor.
        estimator.record(position: 48, at: start.addingTimeInterval(10))
        estimator.record(position: 46, at: start.addingTimeInterval(20))
        XCTAssertEqual(estimator.trend(now: start.addingTimeInterval(20)), .unknown)
        XCTAssertNil(estimator.estimate(now: start.addingTimeInterval(20)))
    }

    func testQueueEstimateIsARangeDerivedFromObservedMovement() {
        var estimator = QueueTrendEstimator()
        let start = Date(timeIntervalSince1970: 1_000_000)
        // Twelve positions consumed over two minutes = six per minute, eighteen left.
        for step in 0...4 {
            estimator.record(position: 30 - step * 3, at: start.addingTimeInterval(Double(step) * 30))
        }
        let now = start.addingTimeInterval(120)

        guard case .moving(let perMinute) = estimator.trend(now: now) else {
            return XCTFail("Expected a moving trend")
        }
        XCTAssertEqual(perMinute, 6, accuracy: 0.01)

        let estimate = estimator.estimate(now: now)
        XCTAssertNotNil(estimate)
        // 18 remaining at 6/min is 3 minutes; the band is deliberately wide around it.
        XCTAssertLessThanOrEqual(estimate?.lowMinutes ?? 0, 3)
        XCTAssertGreaterThanOrEqual(estimate?.highMinutes ?? 0, 3)
        XCTAssertGreaterThan(estimate?.highMinutes ?? 0, estimate?.lowMinutes ?? 0)
        XCTAssertTrue(estimator.supportLine(now: now)?.contains("Moving") ?? false)
    }

    func testQueueEstimateWithdrawsWhenTheQueueStalls() {
        var estimator = QueueTrendEstimator()
        let start = Date(timeIntervalSince1970: 1_000_000)
        for step in 0...4 {
            estimator.record(position: 30 - step * 3, at: start.addingTimeInterval(Double(step) * 30))
        }
        // Nothing changes for two more minutes.
        let stalled = start.addingTimeInterval(240)
        estimator.record(position: 18, at: stalled)

        XCTAssertEqual(estimator.trend(now: stalled), .holding)
        XCTAssertNil(estimator.estimate(now: stalled), "A stalled queue must not keep publishing an ETA")
        XCTAssertEqual(estimator.supportLine(now: stalled), "Queue is holding")
    }

    func testQueueSlippingBackwardsIsNamedRatherThanHidden() {
        var estimator = QueueTrendEstimator()
        let start = Date(timeIntervalSince1970: 1_000_000)
        estimator.record(position: 20, at: start)
        estimator.record(position: 18, at: start.addingTimeInterval(30))
        estimator.record(position: 26, at: start.addingTimeInterval(60))

        XCTAssertEqual(estimator.trend(now: start.addingTimeInterval(60)), .slipped)
        XCTAssertNil(estimator.estimate(now: start.addingTimeInterval(60)))
        XCTAssertTrue(estimator.supportLine(now: start.addingTimeInterval(60))?.contains("moved back") ?? false)
    }

    func testQueueEstimatorDropsSamplesOlderThanItsWindow() {
        var estimator = QueueTrendEstimator()
        let start = Date(timeIntervalSince1970: 1_000_000)
        estimator.record(position: 500, at: start)
        // Twenty minutes later the first reading describes a queue that no longer exists.
        for step in 0...4 {
            estimator.record(position: 20 - step * 2, at: start.addingTimeInterval(1_200 + Double(step) * 30))
        }
        XCTAssertFalse(
            estimator.samples.contains { $0.position == 500 },
            "Samples outside the window must be pruned or the rate is nonsense"
        )
    }

    func testQueuePhaseCopyNamesEachStage() {
        XCTAssertEqual(
            QueuePhaseCopy.heroSupport(position: nil, seatSetupStep: nil, status: 1, isLaunching: true),
            "Asking for a rig"
        )
        XCTAssertEqual(
            QueuePhaseCopy.heroSupport(position: 1, seatSetupStep: nil, status: 1, isLaunching: false),
            "You're next"
        )
        XCTAssertEqual(
            QueuePhaseCopy.heroSupport(position: 40, seatSetupStep: nil, status: 1, isLaunching: false),
            "In the queue"
        )
        XCTAssertEqual(
            QueuePhaseCopy.heroSupport(position: nil, seatSetupStep: 2, status: 2, isLaunching: false),
            "Preparing your rig — step 2 of 4"
        )
        XCTAssertEqual(
            QueuePhaseCopy.heroSupport(position: nil, seatSetupStep: nil, status: 3, isLaunching: false),
            "Connecting"
        )
    }

    // MARK: - Session report

    func testSessionScoreLaddersMatchAndroidValues() {
        // Values lifted directly from SessionReport.kt so the two platforms cannot drift.
        XCTAssertEqual(StreamQualityLadder.latencyScore(30), 100)
        XCTAssertEqual(StreamQualityLadder.latencyScore(31), 92)
        XCTAssertEqual(StreamQualityLadder.latencyScore(80), 80)
        XCTAssertEqual(StreamQualityLadder.latencyScore(120), 60)
        XCTAssertEqual(StreamQualityLadder.latencyScore(181), 10)

        XCTAssertEqual(StreamQualityLadder.packetLossScore(0.1), 100)
        XCTAssertEqual(StreamQualityLadder.packetLossScore(0.5), 90)
        XCTAssertEqual(StreamQualityLadder.packetLossScore(1.0), 75)
        XCTAssertEqual(StreamQualityLadder.packetLossScore(2.0), 55)
        XCTAssertEqual(StreamQualityLadder.packetLossScore(6.0), 5)

        XCTAssertEqual(StreamQualityLadder.jitterScore(5), 100)
        XCTAssertEqual(StreamQualityLadder.jitterScore(20), 70)
        XCTAssertEqual(StreamQualityLadder.jitterScore(51), 5)

        XCTAssertEqual(StreamQualityLadder.frameRateScore(60, targetFps: 60), 100)
        XCTAssertEqual(StreamQualityLadder.frameRateScore(57, targetFps: 60), 95)
        XCTAssertEqual(StreamQualityLadder.frameRateScore(48, targetFps: 60), 60)
        XCTAssertEqual(StreamQualityLadder.frameRateScore(30, targetFps: 60), 10)
    }

    func testDecodeScoreCreditsHealthyCadenceOverPipelineLatency() {
        // A hardware decoder that pipelines frames can exceed one display interval of latency
        // while still delivering every frame. Android floors that case; iOS must too.
        let laggy = StreamQualityLadder.decodeScore(20, targetFps: 60, actualFps: nil)
        XCTAssertEqual(laggy, 45)
        XCTAssertEqual(StreamQualityLadder.decodeScore(20, targetFps: 60, actualFps: 60), 75)
        XCTAssertEqual(StreamQualityLadder.decodeScore(20, targetFps: 60, actualFps: 55), 55)
        XCTAssertEqual(StreamQualityLadder.decodeScore(20, targetFps: 60, actualFps: 30), 45)
    }

    func testSessionQualityScoreWeightsAndRatingBands() {
        let perfect = StreamSessionReportAccumulator.qualityScore(
            averagePingMs: 20,
            packetLossPercent: 0.0,
            averageJitterMs: 2,
            averageFps: 60,
            targetFps: 60,
            averageDecodeMs: 4
        )
        XCTAssertEqual(perfect, 100)
        XCTAssertEqual(SessionReportRating.forScore(perfect), .excellent)

        let rough = StreamSessionReportAccumulator.qualityScore(
            averagePingMs: 150,
            packetLossPercent: 3.0,
            averageJitterMs: 35,
            averageFps: 40,
            targetFps: 60,
            averageDecodeMs: 14
        )
        XCTAssertLessThan(rough, 60)
        XCTAssertEqual(SessionReportRating.forScore(rough), .poor)

        // Missing metrics must not be scored as zero — only the captured ones carry weight.
        let latencyOnly = StreamSessionReportAccumulator.qualityScore(
            averagePingMs: 20,
            packetLossPercent: nil,
            averageJitterMs: nil,
            averageFps: nil,
            targetFps: 60,
            averageDecodeMs: nil
        )
        XCTAssertEqual(latencyOnly, 100)

        // Nothing captured at all is explicitly neutral rather than a confident-looking zero.
        XCTAssertEqual(
            StreamSessionReportAccumulator.qualityScore(
                averagePingMs: nil,
                packetLossPercent: nil,
                averageJitterMs: nil,
                averageFps: nil,
                targetFps: 60,
                averageDecodeMs: nil
            ),
            50
        )
    }

    func testAccumulatorProducesNoReportWithoutMeasurements() {
        let accumulator = StreamSessionReportAccumulator(launchProfile: Self.launchProfile())
        XCTAssertNil(accumulator.finish())

        // A sample carrying nothing measurable must not count either.
        accumulator.record(StreamRuntimeSample(timestamp: 0, resolution: "1920x1080"))
        XCTAssertNil(accumulator.finish())
    }

    func testAccumulatorPrefersPacketDeltasOverCumulativeRatios() {
        let accumulator = StreamSessionReportAccumulator(launchProfile: Self.launchProfile())
        // Twenty clean windows then one bad one: the average must reflect the whole session,
        // not the single spike, but the peak must still record the spike.
        for _ in 0..<20 {
            accumulator.record(
                StreamRuntimeSample(timestamp: 0, pingMs: 20, packetsLostDelta: 0, packetsReceivedDelta: 1_000)
            )
        }
        accumulator.record(
            StreamRuntimeSample(timestamp: 0, pingMs: 20, packetsLostDelta: 100, packetsReceivedDelta: 900)
        )

        let report = try? XCTUnwrap(accumulator.finish())
        XCTAssertNotNil(report)
        guard let report else { return }
        // 100 lost out of 21,000 total.
        XCTAssertEqual(report.packetLossPercent ?? 0, 100.0 / 21_000.0 * 100, accuracy: 0.001)
        XCTAssertEqual(report.peakPacketLossPercent ?? 0, 10.0, accuracy: 0.001)
        XCTAssertFalse(report.limitedData)
    }

    func testShortSessionsAreMarkedAsLimitedData() {
        let accumulator = StreamSessionReportAccumulator(launchProfile: Self.launchProfile())
        for _ in 0..<3 {
            accumulator.record(StreamRuntimeSample(timestamp: 0, pingMs: 25, fps: 60))
        }
        let report = accumulator.finish()
        XCTAssertEqual(report?.sampleCount, 3)
        XCTAssertEqual(report?.limitedData, true)
        XCTAssertFalse(report?.showsTrendChart ?? true, "A three-point chart is a lie and must be suppressed")
    }

    func testRecommendationsNameTheActualProblem() {
        let cellular = StreamSessionReportAccumulator.buildRecommendations(
            averagePingMs: 120,
            packetLossPercent: 2.5,
            averageJitterMs: 30,
            averageFps: 58,
            averageDecodeMs: nil,
            targetFps: 60,
            targetBitrateKbps: 35_000,
            averageBitrateKbps: 30_000,
            networkKind: .cellular
        )
        XCTAssertTrue(cellular.contains { $0.title.contains("Wi-Fi") })
        XCTAssertTrue(cellular.contains { $0.title.contains("packet loss") })
        XCTAssertTrue(cellular.contains { $0.title.contains("latency") })
        XCTAssertLessThanOrEqual(cellular.count, 4, "The list has to stay short enough to act on")

        let healthy = StreamSessionReportAccumulator.buildRecommendations(
            averagePingMs: 18,
            packetLossPercent: 0.05,
            averageJitterMs: 3,
            averageFps: 60,
            averageDecodeMs: 3,
            targetFps: 60,
            targetBitrateKbps: 35_000,
            averageBitrateKbps: 34_000,
            networkKind: .wifi
        )
        XCTAssertEqual(healthy.count, 1)
        XCTAssertEqual(healthy.first?.kind, .info)
    }

    func testDowngradesNameWhereTheProfileWasReduced() {
        let selected = StreamVideoProfile(width: 2_560, height: 1_440, fps: 120, maxBitrateKbps: 75_000)
        let eligible = StreamVideoProfile(width: 1_920, height: 1_080, fps: 60, maxBitrateKbps: 35_000)
        let findings = StreamSessionReportAccumulator.buildDowngrades(
            launchProfile: StreamReportLaunchProfile(
                gameTitle: "Test",
                selectedProfile: selected,
                eligibleProfile: eligible,
                initialProfile: eligible,
                requestedCodec: "AV1",
                eligibleCodec: "H265",
                hdrRequested: false
            ),
            finalProfile: eligible,
            finalCodec: "H265",
            deliveredResolution: "1920x1080",
            deliveredCodec: "H265",
            recoveryReason: nil
        )
        XCTAssertTrue(findings.contains { $0.title == "Limited by your plan" })
        XCTAssertTrue(findings.contains { $0.title == "Codec changed for this device" })
        // Delivered matches the request, so there must be no spurious "different resolution".
        XCTAssertFalse(findings.contains { $0.title == "Delivered a different resolution" })
    }

    private static func launchProfile(
        target: StreamVideoProfile = StreamVideoProfile(width: 1_920, height: 1_080, fps: 60, maxBitrateKbps: 35_000)
    ) -> StreamReportLaunchProfile {
        StreamReportLaunchProfile(
            gameTitle: "Test Game",
            selectedProfile: target,
            eligibleProfile: target,
            initialProfile: target,
            requestedCodec: "H265",
            eligibleCodec: "H265",
            hdrRequested: false
        )
    }

    private static func makeGame(
        title: String,
        controls: [String],
        sectionId: String? = nil,
        sectionTitle: String? = nil
    ) -> CloudGame {
        CloudGame(
            id: title.lowercased(),
            title: title,
            genre: "Action",
            platform: "STEAM",
            icon: "",
            imageUrl: nil,
            launchAppId: "1",
            launchOptions: [GameLaunchOption(storefront: "STEAM", appId: "1", supportedControls: controls)],
            uuid: nil,
            summary: nil,
            longDescription: nil,
            publisher: nil,
            developer: nil,
            releaseDate: nil,
            featureLabels: nil,
            tags: nil,
            stores: nil,
            playType: nil,
            membershipTierLabel: nil,
            catalogSectionId: sectionId,
            catalogSectionTitle: sectionTitle,
            contentRatings: nil
        )
    }

    func testContentRatingMetadataUsesOrderedDeduplicatedUnion() {
        XCTAssertEqual(
            GFNContentRatingParser.merging(
                ["ESRB T", "Violence"],
                ["ESRB T", "Users Interact"]
            ),
            ["ESRB T", "Violence", "Users Interact"]
        )
    }

    func testContentRatingsUseCatalogStyleAgeBadgesWithoutLegacySeventeenGate() {
        XCTAssertEqual(GFNContentRatingParser.ageBadge(from: ["ESRB T", "Violence"]), "12+")
        XCTAssertEqual(GFNContentRatingParser.ageBadge(from: ["PEGI 18"]), "18+")
        XCTAssertEqual(GFNContentRatingParser.ageBadge(from: ["USK 6"]), "6+")
        XCTAssertNil(GFNContentRatingParser.ageBadge(from: ["Violence", "Users Interact"]))
    }

    func testArtworkRolesMatchAndroidCatalogDetailsAndQueuePriority() throws {
        let game = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"{"id":"game","title":"Game","genre":"Action","platform":"Steam","icon":"gamecontroller.fill","imageUrl":"legacy","boxArtUrl":"box","heroImageUrl":"hero","tvBannerUrl":"tv","launchOptions":[]}"#.utf8
            )
        )

        XCTAssertEqual(game.catalogArtworkUrl, "box")
        XCTAssertEqual(game.detailsArtworkUrl, "hero")
        XCTAssertEqual(game.queueArtworkUrl, "tv")
    }

    func testMobileCatalogArtworkRejectsStaleNvidiaBannerLikeAndroid() throws {
        let validBoxArt = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"{"id":"valid","title":"Valid","genre":"Action","platform":"Steam","icon":"gamecontroller.fill","imageUrl":"https://img.nvidiagrid.net/apps/123/ZZ/GAME_BOX_ART_01_example.jpg","launchOptions":[]}"#.utf8
            )
        )
        let staleBanner = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"{"id":"stale","title":"Stale","genre":"Action","platform":"Steam","icon":"gamecontroller.fill","imageUrl":"https://img.nvidiagrid.net/apps/123/ZZ/TV_BANNER_01_example.jpg","launchOptions":[]}"#.utf8
            )
        )

        XCTAssertEqual(
            validBoxArt.catalogArtworkUrl,
            "https://img.nvidiagrid.net/apps/123/ZZ/GAME_BOX_ART_01_example.jpg"
        )
        XCTAssertNil(staleBanner.catalogArtworkUrl)
    }

    func testGameDetailsPreferFirstScreenshotLikeAndroid() throws {
        let game = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"{"id":"game","title":"Game","genre":"Action","platform":"Steam","icon":"gamecontroller.fill","imageUrl":"poster","heroImageUrl":"hero","launchOptions":[],"screenshotUrls":["shot-one","shot-two"]}"#.utf8
            )
        )

        XCTAssertEqual(game.screenshotUrls, ["shot-one", "shot-two"])
        XCTAssertEqual(game.detailsArtworkUrl, "shot-one")
    }

    func testCompletedQueueAdIsRemovedBeforeReturningToQueueScreen() {
        let ad = SessionAdInfo(
            adId: "ad-1",
            state: nil,
            adState: nil,
            adUrl: nil,
            mediaUrl: "https://example.com/ad.mp4",
            adMediaFiles: [],
            clickThroughUrl: nil,
            adLengthInSeconds: 15,
            durationMs: nil,
            title: nil,
            description: nil
        )
        let state = SessionAdState(
            isAdsRequired: true,
            sessionAdsRequired: true,
            isQueuePaused: true,
            gracePeriodSeconds: nil,
            message: nil,
            sessionAds: [ad],
            ads: [ad],
            opportunity: nil,
            serverSentEmptyAds: false
        )

        let updated = removeSessionAdItem(state, adId: ad.adId)
        XCTAssertTrue(sessionAdItems(updated).isEmpty)
        XCTAssertTrue(isSessionAdsRequired(updated))
    }

    func testNvidiaArtworkRequestsAreClampedToTheStoredMasterWidth() {
        let boxArt = "https://img.nvidiagrid.net/apps/101550411/ZZ/GAME_BOX_ART_01_abc.jpg"
        let screenshot = "https://img.nvidiagrid.net/apps/101550411/ZZ/SCREENSHOT_01_abc.jpg"

        // Under the master, the request is the width we will draw.
        XCTAssertEqual(
            optimizedNvidiaArtworkURL(boxArt, targetPixelWidth: 480),
            "\(boxArt);f=webp;w=480"
        )
        // Above it, the CDN would upscale and charge us more bytes than the untouched original,
        // so the request is clamped to what NVIDIA actually stores.
        XCTAssertEqual(
            optimizedNvidiaArtworkURL(boxArt, targetPixelWidth: 1_600),
            "\(boxArt);f=webp;w=628"
        )
        XCTAssertEqual(
            optimizedNvidiaArtworkURL(screenshot, targetPixelWidth: 4_096),
            "\(screenshot);f=webp;w=1920"
        )

        // Existing sizing parameters are replaced, not appended to.
        XCTAssertEqual(
            optimizedNvidiaArtworkURL("\(boxArt);f=jpeg;w=4096;dpr=2", targetPixelWidth: 400),
            "\(boxArt);f=webp;w=400"
        )

        // Sizing travels as path parameters. Appending them after a query string would leave the
        // CDN serving the full-size master while the URL claimed otherwise.
        XCTAssertEqual(
            optimizedNvidiaArtworkURL("\(boxArt)?sig=abc", targetPixelWidth: 320),
            "\(boxArt);f=webp;w=320?sig=abc"
        )

        // Anything we have not measured is left exactly as it came.
        XCTAssertEqual(
            optimizedNvidiaArtworkURL("https://cdn.example.com/box.jpg", targetPixelWidth: 800),
            "https://cdn.example.com/box.jpg"
        )
        XCTAssertEqual(
            optimizedNvidiaArtworkURL(
                "https://img.nvidiagrid.net/apps/101550411/ZZ/MYSTERY_ART_01_abc.jpg",
                targetPixelWidth: 800
            ),
            "https://img.nvidiagrid.net/apps/101550411/ZZ/MYSTERY_ART_01_abc.jpg"
        )
    }

    func testArtworkRequestWidthFollowsTheCardWidthNotItsLongestEdge() {
        // Box art is portrait. Sizing the request by the longest edge asks the CDN for a third
        // more pixels than a poster can ever show.
        let poster = CGSize(width: 180, height: 600)
        let scale = UIScreen.main.scale
        XCTAssertEqual(
            imageRequestWidth(for: poster),
            normalizedImageTargetPixelSize(Int(ceil(180 * scale)))
        )
        XCTAssertLessThan(imageRequestWidth(for: poster), imageTargetPixelSize(for: poster))
        // Degenerate sizes fall back rather than producing a zero-width request.
        XCTAssertEqual(imageRequestWidth(for: .zero), 480)
    }

    func testCatalogSearchMatchesAllTermsAcrossMetadataLikeAndroid() throws {
        let game = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"{"id":"game","title":"Cyber Adventure","genre":"Action","platform":"Steam","icon":"gamecontroller.fill","launchOptions":[],"publisher":"Cloud Studio","tags":["Open World"]}"#.utf8
            )
        )

        XCTAssertTrue(gameMatchesCatalogSearch(game, query: "cyber world"))
        XCTAssertTrue(gameMatchesCatalogSearch(game, query: "cloud action"))
        XCTAssertFalse(gameMatchesCatalogSearch(game, query: "cyber racing"))
    }

    func testPosterScaleUsesLatestAndroidRange() {
        var settings = AppSettings.default
        settings.posterSizeScale = 2
        settings.normalizeStreamDefaults()
        XCTAssertEqual(settings.posterSizeScale, 1.4)
    }

    func testArtworkTargetSizeUsesStableCacheBuckets() {
        XCTAssertEqual(normalizedImageTargetPixelSize(1), 160)
        XCTAssertEqual(normalizedImageTargetPixelSize(161), 320)
        XCTAssertEqual(normalizedImageTargetPixelSize(319), 320)
        XCTAssertEqual(normalizedImageTargetPixelSize(960), 960)
    }

    func testPersistedBitrateIsNormalizedToNearestMenuPreset() {
        var settings = AppSettings.default
        settings.maxBitrateMbps = 150
        settings.normalizeStreamDefaults()
        XCTAssertEqual(settings.maxBitrateMbps, 100)

        settings.maxBitrateMbps = 73
        settings.normalizeStreamDefaults()
        XCTAssertEqual(settings.maxBitrateMbps, 75)
    }

    func testPrintedWasteEqualScoresPreferLowerPingLikeAndroid() {
        let highPing = PrintedWasteZone(
            id: "high-ping",
            region: "US",
            queuePosition: 1,
            etaMs: nil,
            zoneUrl: "https://high-ping.example",
            pingMs: 2,
            isMeasuring: false,
            regionSuffix: "east"
        )
        let lowPing = PrintedWasteZone(
            id: "low-ping",
            region: "US",
            queuePosition: 4,
            etaMs: nil,
            zoneUrl: "https://low-ping.example",
            pingMs: 1,
            isMeasuring: false,
            regionSuffix: "west"
        )
        let normalizationAnchor = PrintedWasteZone(
            id: "anchor",
            region: "US",
            queuePosition: 4,
            etaMs: nil,
            zoneUrl: "https://anchor.example",
            pingMs: 4,
            isMeasuring: false,
            regionSuffix: "central"
        )

        XCTAssertEqual(
            recommendedPrintedWasteZone(in: [highPing, lowPing, normalizationAnchor])?.id,
            lowPing.id
        )
    }

    func testStableCatalogIdentityDeduplicatesLauncherVariants() throws {
        let first = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"{"id":"shared-app:steam","title":"Game","genre":"Action","platform":"Steam","icon":"gamecontroller.fill","launchOptions":[],"uuid":"shared-app"}"#.utf8
            )
        )
        let second = try JSONDecoder().decode(
            CloudGame.self,
            from: Data(
                #"{"id":"shared-app:epic","title":"Game","genre":"Action","platform":"Epic","icon":"gamecontroller.fill","launchOptions":[],"uuid":"SHARED-APP"}"#.utf8
            )
        )

        XCTAssertEqual(catalogStableGameKey(first), catalogStableGameKey(second))
    }

    func testDiagnosticsStrictlyRedactsCredentialsAndIdentifiers() throws {
        let payload = #"{"requestStatus":{"statusCode":500,"statusDescription":"internal_server_error"},"access_token":"secret-access-token","refresh_token":"secret-refresh-token","email":"person@example.com","sessionId":"c49ec342-4c25-4e0a-9416-9c82e2f53233","serverIp":"203.0.113.42"}"#
        let redacted = DiagnosticsSanitizer.redactedBody(
            Data(payload.utf8),
            headers: ["Content-Type": "application/json"]
        )

        XCTAssertTrue(redacted.contains("internal_server_error"))
        XCTAssertTrue(redacted.contains("500"))
        XCTAssertFalse(redacted.contains("secret-access-token"))
        XCTAssertFalse(redacted.contains("secret-refresh-token"))
        XCTAssertFalse(redacted.contains("person@example.com"))
        XCTAssertFalse(redacted.contains("c49ec342-4c25-4e0a-9416-9c82e2f53233"))
        XCTAssertFalse(redacted.contains("203.0.113.42"))
        XCTAssertFalse(
            DiagnosticsSanitizer.sanitize(#"lastError={"access_token":"short-secret"}"#)
                .contains("short-secret")
        )
    }

    func testDiagnosticsHeadersKeepUsefulMetadataWithoutSecrets() {
        let redacted = DiagnosticsSanitizer.redactedHeaders([
            "Authorization": "GFNJWT super-secret-token",
            "x-device-id": "device-12345",
            "Content-Type": "application/json",
            "x-request-id": "request-67890"
        ])

        XCTAssertTrue(redacted.contains("Content-Type: application/json"))
        XCTAssertFalse(redacted.contains("super-secret-token"))
        XCTAssertFalse(redacted.contains("device-12345"))
        XCTAssertFalse(redacted.contains("request-67890"))
        XCTAssertTrue(redacted.contains("[ID:"))
    }

    func testDiagnosticsPreserveTimestampsAndVersionsWhileRedactingNetworkAddresses() {
        let redacted = DiagnosticsSanitizer.sanitize(
            """
            generatedAt=2026-07-13T10:31:22.768Z
            nv-client-version: 2.0.0.0
            User-Agent: Chrome/131.0.0.0 GFN-PC/2.0.0.0
            serverIp=203.0.113.42 url=https://198.51.100.5/path ipv6=2001:db8::1
            """
        )

        XCTAssertTrue(redacted.contains("2026-07-13T10:31:22.768Z"))
        XCTAssertTrue(redacted.contains("nv-client-version: 2.0.0.0"))
        XCTAssertTrue(redacted.contains("Chrome/131.0.0.0"))
        XCTAssertTrue(redacted.contains("GFN-PC/2.0.0.0"))
        XCTAssertFalse(redacted.contains("203.0.113.42"))
        XCTAssertFalse(redacted.contains("198.51.100.5"))
        XCTAssertFalse(redacted.contains("2001:db8::1"))
    }

    func testPlaybackAudioPolicyNeverUsesCallAudio() {
        XCTAssertEqual(NativeStreamAudioSessionPolicy.category(enableMic: false), .playback)
        XCTAssertEqual(NativeStreamAudioSessionPolicy.mode(enableMic: false), .moviePlayback)
        XCTAssertEqual(
            NativeStreamAudioSessionPolicy.options(enableMic: false),
            []
        )
    }

    func testOAuthCompletionGateAllowsOnlyOneContinuationResume() {
        let gate = OAuthCompletionGate()

        XCTAssertTrue(gate.claim())
        XCTAssertFalse(gate.claim())
        XCTAssertFalse(gate.claim())
    }

    func testNewGamesHeroUsesProviderOrderDeduplicatesAndExcludesPersonalRows() {
        let weeklyID = "section-cbc43218-6ad6-4ff3-8538-bc84f90c796c-week-34"
        let first = Self.makeGame(title: "First", controls: [], sectionTitle: "GFN Thursday")
        let duplicate = Self.makeGame(title: "First", controls: [], sectionId: weeklyID)
        let second = Self.makeGame(title: "Second", controls: [], sectionId: weeklyID)
        let unrelated = Self.makeGame(title: "Recent-ish", controls: [], sectionTitle: "Recently updated")

        let result = newlyAddedStoreHeroGames(
            games: [unrelated, first, duplicate, second],
            excludedGameKeys: [catalogStableGameKey(first)]
        )

        XCTAssertEqual(result.map(\.title), ["Second"])
    }

    func testNewGamesHeroFallsBackToWeeklyListWhenEveryPageIsInPersonalRows() {
        let first = Self.makeGame(title: "First", controls: [], sectionTitle: "GFN Thursday")
        let second = Self.makeGame(title: "Second", controls: [], sectionTitle: "GFN Thursday")

        let result = newlyAddedStoreHeroGames(
            games: [first, second],
            excludedGameKeys: [catalogStableGameKey(first), catalogStableGameKey(second)]
        )

        XCTAssertEqual(result.map(\.title), ["First", "Second"])
    }

    func testIOS26UsesFilteredRendererWithoutRequiringSharpeningToggle() {
        XCTAssertTrue(
            nativeStreamShouldUseFilteredRenderer(
                osMajorVersion: 26,
                streamSharpeningEnabled: false,
                isSimulator: false
            )
        )
        XCTAssertFalse(
            nativeStreamShouldUseFilteredRenderer(
                osMajorVersion: 25,
                streamSharpeningEnabled: false,
                isSimulator: false
            )
        )
        XCTAssertTrue(
            nativeStreamShouldUseFilteredRenderer(
                osMajorVersion: 25,
                streamSharpeningEnabled: true,
                isSimulator: false
            )
        )
    }

    func testNativeStreamTransportRecoveryMatchesAndroidMobileTiming() {
        XCTAssertEqual(NativeStreamTransportPolicy.offerTimeout, 12)
        XCTAssertEqual(NativeStreamTransportPolicy.iceDisconnectedGrace, 3.5)
        XCTAssertTrue(NativeStreamTransportPolicy.allowsTCPCandidates)

        var watchdog = NativeStreamLivenessWatchdog()
        watchdog.markConnected(now: 0)

        XCTAssertEqual(
            watchdog.observe(now: 4.9, bytesReceived: 0, framesDecoded: 0, connected: true),
            .none
        )
        XCTAssertEqual(
            watchdog.observe(now: 5, bytesReceived: 0, framesDecoded: 0, connected: true),
            .requestKeyframe(stalledFor: 5, attempt: 1)
        )
        XCTAssertEqual(
            watchdog.observe(now: 7.4, bytesReceived: 0, framesDecoded: 0, connected: true),
            .none
        )
        XCTAssertEqual(
            watchdog.observe(now: 7.5, bytesReceived: 0, framesDecoded: 0, connected: true),
            .requestKeyframe(stalledFor: 7.5, attempt: 2)
        )
        XCTAssertEqual(
            watchdog.observe(now: 10, bytesReceived: 0, framesDecoded: 0, connected: true),
            .restartTransport(stalledFor: 10)
        )
    }

    func testNativeStreamLivenessProgressAndDisconnectResetRecoveryWindow() {
        var watchdog = NativeStreamLivenessWatchdog()
        watchdog.markConnected(now: 0)

        XCTAssertEqual(
            watchdog.observe(now: 5, bytesReceived: 100, framesDecoded: 1, connected: true),
            .none
        )
        XCTAssertEqual(
            watchdog.observe(now: 9.9, bytesReceived: 100, framesDecoded: 1, connected: true),
            .none
        )
        XCTAssertEqual(
            watchdog.observe(now: 10, bytesReceived: 100, framesDecoded: 1, connected: true),
            .requestKeyframe(stalledFor: 5, attempt: 1)
        )
        XCTAssertEqual(
            watchdog.observe(now: 10.5, bytesReceived: 100, framesDecoded: 1, connected: false),
            .none
        )

        watchdog.markConnected(now: 20)
        XCTAssertEqual(
            watchdog.observe(now: 24.9, bytesReceived: 0, framesDecoded: 0, connected: true),
            .none
        )
        XCTAssertEqual(
            watchdog.observe(now: 25, bytesReceived: 0, framesDecoded: 0, connected: true),
            .requestKeyframe(stalledFor: 5, attempt: 1)
        )
    }

    func testNativeStreamRecoveryBudgetResetsOnlyAfterThreeConsecutiveProgressSamples() {
        var tracker = NativeStreamRecoveryProgressTracker()

        XCTAssertFalse(tracker.observe(progressed: true))
        XCTAssertFalse(tracker.observe(progressed: false))
        XCTAssertFalse(tracker.observe(progressed: true))
        XCTAssertFalse(tracker.observe(progressed: true))
        XCTAssertTrue(tracker.observe(progressed: true))
        XCTAssertFalse(tracker.observe(progressed: true))
    }

    private func deterministicProfile(
        for settings: AppSettings,
        membershipTier: String
    ) -> StreamVideoProfile {
        StreamSettingsResolver.profile(
            for: settings,
            nativeBounds: CGRect(x: 0, y: 0, width: 1_179, height: 2_556),
            nativeScale: 3,
            userInterfaceIdiom: .phone,
            membershipTier: membershipTier
        )
    }
}

private final class RecordingNativeStreamInputSink: NativeStreamInputSink {
    var reliablePackets: [Data] = []
    var partiallyReliablePackets: [Data] = []
    var logMessages: [String] = []

    func sendReliableInput(_ data: Data) {
        reliablePackets.append(data)
    }

    func sendPartiallyReliableInput(_ data: Data) {
        partiallyReliablePackets.append(data)
    }

    func logInputEvent(_ message: String) {
        logMessages.append(message)
    }
}
