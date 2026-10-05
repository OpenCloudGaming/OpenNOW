import XCTest
import UIKit
import WebRTC
import CoreVideo
import CoreMedia
import SwiftUI
import Metal

#if canImport(MetalFX)
import MetalFX
#endif
import CoreImage
@testable import OpenNOWiOS

@MainActor
private final class GameDetailsPresentationTestDriver: ObservableObject {
    @Published var selectedGame: CloudGame?
    var activate: (() -> Void)?
    var registry: GameDetailsTransitionRegistry?
}

private struct GameDetailsPresentationTestSource: View {
    @Environment(\.gameDetailsTransition) private var transition
    @ObservedObject var driver: GameDetailsPresentationTestDriver
    let game: CloudGame
    let sourceID: UUID

    var body: some View {
        Color.blue.frame(width: 160, height: 240)
            .gameDetailsArtworkSource(id: sourceID)
            .onAppear {
                driver.registry = transition?.registry
                driver.activate = {
                    transition?.selectSource(GameDetailsTransitionOrigin(sourceID: sourceID, gameKey: catalogStableGameKey(game)))
                    driver.selectedGame = game
                }
            }
    }
}

private struct GameDetailsPresentationTestRoot: View {
    @ObservedObject var driver: GameDetailsPresentationTestDriver
    let store: OpenNOWStore
    let game: CloudGame
    let sourceID: UUID

    var body: some View {
        GameDetailsPresentationTestSource(driver: driver, game: game, sourceID: sourceID)
            .presentGameDetailsSheet(selectedGame: $driver.selectedGame, store: store) { _, _ in }
    }
}

final class OpenNOWiOSParityTests: XCTestCase {
    @MainActor
    func testGameDetailsUsesNativeZoomOnFirstPresentationAndClearsSourceAfterDismissal() async throws {
        guard #available(iOS 18, *) else { throw XCTSkip("Native zoom requires iOS 18") }
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let previousKeyWindow = scene.windows.first(where: \.isKeyWindow)
        let store = OpenNOWStore()
        let driver = GameDetailsPresentationTestDriver()
        let game = OpenNOWStore.debugStorePreviewGames[1]
        let sourceID = UUID()
        let host = UIHostingController(rootView: GameDetailsPresentationTestRoot(
            driver: driver, store: store, game: game, sourceID: sourceID
        ))
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil; previousKeyWindow?.makeKeyAndVisible() }
        try await Task.sleep(for: .milliseconds(500))
        let activate = try XCTUnwrap(driver.activate)
        activate()
        try await Task.sleep(for: .milliseconds(900))
        let presentation = try XCTUnwrap(host.presentedViewController)
        XCTAssertNotNil(presentation.preferredTransition, "The first presentation must receive its native zoom transition")
        XCTAssertEqual(driver.registry?.sourceID(for: game), sourceID)
        driver.selectedGame = nil
        // Native zoom dismissal completes asynchronously; wait for its lifecycle callback,
        // rather than assuming the iPad's animation will finish at the iPhone's timing.
        for _ in 0..<50 {
            if host.presentedViewController == nil, driver.registry?.origin == nil { break }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTAssertNil(host.presentedViewController)
        XCTAssertNil(driver.registry?.origin)
    }

    func testHeroProgressKeepsCircularEndsThroughoutFill() {
        let bounds = CGRect(x: 0, y: 0, width: 28, height: 6)
        for (progress, width): (CGFloat, CGFloat) in [(0, 6), (0.5, 17), (1, 28)] {
            let path = HeroPageProgressFill(progress: progress).path(in: bounds)
            XCTAssertEqual(path.boundingRect.width, width, accuracy: 0.001)
            XCTAssertEqual(path.boundingRect.height, 6, accuracy: 0.001)
            XCTAssertTrue(path.contains(CGPoint(x: width - 3, y: 0.25)))
            XCTAssertFalse(path.contains(CGPoint(x: width - 0.25, y: 0.25)), "The right end must remain rounded")
            XCTAssertFalse(path.contains(CGPoint(x: 0.25, y: 0.25)), "The left end must remain rounded")
        }
    }

    @MainActor
    func testStorePlayUsesSavedLauncherAndOtherwiseAsksForChoice() {
        let steam = GameLaunchOption(storefront: "STEAM", appId: "101", supportedControls: nil,
            libraryStatus: "PLATFORM_SYNC", lastPlayedDate: "2026-10-03T14:00:00Z")
        let epic = GameLaunchOption(storefront: "EPIC", appId: "202", supportedControls: nil,
            libraryStatus: "NOT_OWNED")
        let game = Self.makeGame(title: "Store Launch Choice Test", controls: [], options: [steam, epic])
        let store = OpenNOWStore()
        store.settings.defaultGameVariantIds.removeValue(forKey: game.id)
        XCTAssertEqual(store.launchChoice(for: game), .chooseLauncher)
        store.settings.defaultGameVariantIds[game.id] = epic.id
        XCTAssertEqual(store.launchChoice(for: game), .launch(epic))
        XCTAssertEqual(game.lastPlayedDate, steam.lastPlayedDate)
        XCTAssertEqual(game.ownedStorefronts, ["STEAM"])
    }

    func testStoreLinksRequireHTTPSAndOldLauncherCacheStillDecodes() throws {
        let old = Data(#"{"storefront":"STEAM","appId":"101","supportedControls":null}"#.utf8)
        let decoded = try JSONDecoder().decode(GameLaunchOption.self, from: old)
        XCTAssertNil(decoded.libraryStatus)
        XCTAssertNil(decoded.lastPlayedDate)
        XCTAssertNil(decoded.storeURL)

        var option = decoded
        option.storeURL = "https://store.steampowered.com/app/101"
        XCTAssertNotNil(option.externalStoreURL)
        option.storeURL = "http://store.steampowered.com/app/101"
        XCTAssertNil(option.externalStoreURL)
    }

    func testQueueSelectorUsesAdvertisedRegionalRoute() {
        let regions = [
            StreamRegion(name: "Southern California", url: "https://us-west.cloudmatchbeta.nvidiagrid.net/"),
            StreamRegion(name: "Southern California", url: "https://np-lax-02.cloudmatchbeta.nvidiagrid.net/")
        ]
        XCTAssertEqual(
            printedWasteRegionalURL(zoneId: "NP-LAX-02", title: "Southern California (USA)", regions: regions),
            "https://us-west.cloudmatchbeta.nvidiagrid.net/"
        )
        XCTAssertNil(printedWasteRegionalURL(zoneId: "NPA-LAX-02", title: "Southern California", regions: regions))
        XCTAssertNil(printedWasteRegionalURL(zoneId: "NP-LAX-02", title: "Unknown", regions: regions))
    }

    func testQueueRecommendationFallsBackToLowestPingWhenAllRoutesAreSlow() {
        func zone(_ id: String, queue: Int, ping: Int) -> PrintedWasteZone {
            PrintedWasteZone(
                id: id, title: id, region: "US", regionLabel: "North America",
                queuePosition: queue, etaMs: nil, zoneUrl: "https://example.com/\(id)",
                pingMs: ping, isMeasuring: false, regionSuffix: "US", gpuTier: nil
            )
        }
        let slower = zone("NP-A", queue: 1, ping: 160)
        let faster = zone("NP-B", queue: 30, ping: 110)
        XCTAssertEqual(recommendedPrintedWasteZone(in: [slower, faster])?.id, faster.id)
    }

    @MainActor
    func testQueueDisplayHoldsLowestPositionAndSignOutClearsSession() {
        let store = OpenNOWStore()
        store.installDebugQueuePreview(position: 18)
        XCTAssertEqual(store.displayQueuePosition, 18)
        store.installDebugQueuePreview(position: 21)
        XCTAssertEqual(store.displayQueuePosition, 18)
        store.installDebugQueuePreview(position: 12)
        XCTAssertEqual(store.displayQueuePosition, 12)
        store.signOutAll()
        XCTAssertNil(store.activeSession)
        XCTAssertNil(store.displayQueuePosition)
        XCTAssertTrue(store.savedAccounts.isEmpty)
    }

    @MainActor
    func testQuickVirtualButtonTapSurvivesHostPollingAndReleases() async throws {
        var events: [Bool] = []
        let button = NativeStreamVirtualButtonTouchControl(frame: .zero)
        button.pressed = { events.append($0) }
        button.sendActions(for: .touchDown)
        button.sendActions(for: .touchUpInside)
        XCTAssertEqual(events, [true], "A quick tap must not send down/up in the same host poll")
        try await Task.sleep(nanoseconds: 90_000_000)
        XCTAssertEqual(events, [true, false])
        XCTAssertFalse(button.isPressed)
    }

    @MainActor
    func testVirtualButtonHoldOutsideReleaseAndCancellation() async throws {
        var events: [Bool] = []
        let button = NativeStreamVirtualButtonTouchControl(frame: .zero)
        button.pressed = { events.append($0) }
        button.sendActions(for: .touchDown)
        try await Task.sleep(nanoseconds: 70_000_000)
        XCTAssertEqual(events, [true], "Holding must not auto-release or repeat")
        button.sendActions(for: .touchUpOutside)
        XCTAssertEqual(events, [true, false])
        button.sendActions(for: .touchDown)
        button.sendActions(for: .touchCancel)
        XCTAssertEqual(events, [true, false, true, false], "Cancellation releases immediately")
        button.sendActions(for: .touchDown)
        button.sendActions(for: .touchUpInside)
        button.isEnabled = false
        let afterDisable = events
        try await Task.sleep(nanoseconds: 90_000_000)
        XCTAssertEqual(events, afterDisable, "Disabled controls must not retain a delayed release")
        XCTAssertFalse(button.isPressed)
    }

    @MainActor
    func testSecondVirtualButtonTapDoesNotInheritFirstRelease() async throws {
        var events: [Bool] = []
        let button = NativeStreamVirtualButtonTouchControl(frame: .zero)
        button.pressed = { events.append($0) }
        button.sendActions(for: .touchDown)
        button.sendActions(for: .touchUpInside)
        button.sendActions(for: .touchDown)
        XCTAssertEqual(events, [true, false, true])
        try await Task.sleep(nanoseconds: 90_000_000)
        XCTAssertTrue(button.isPressed, "The old tap must not release the second held press")
        XCTAssertEqual(events, [true, false, true])
        button.sendActions(for: .touchUpInside)
        XCTAssertEqual(events, [true, false, true, false])
    }

    @MainActor
    func testSimultaneousVirtualButtonsReleaseIndependentlyAlongsideStick() async throws {
        let bridge = NativeStreamInputBridge()
        bridge.configure(protocolVersion: 2, partiallyReliableGamepadMask: 0)
        let sink = RecordingNativeStreamInputSink()
        bridge.sink = sink
        bridge.setVirtualControllerEnabled(true)
        bridge.setVirtualStick(.left, x: 0.5, y: 0)
        let a = NativeStreamVirtualButtonTouchControl(frame: .zero)
        let b = NativeStreamVirtualButtonTouchControl(frame: .zero)
        a.pressed = { bridge.setVirtualButton(.a, pressed: $0) }
        b.pressed = { bridge.setVirtualButton(.b, pressed: $0) }
        a.sendActions(for: .touchDown)
        b.sendActions(for: .touchDown)
        let combined = try XCTUnwrap(sink.reliablePackets.last)
        func value(_ data: Data, at offset: Int) -> UInt16 {
            UInt16(data[offset]) | (UInt16(data[offset + 1]) << 8)
        }
        XCTAssertEqual(combined.count, 38)
        XCTAssertEqual(value(combined, at: 12), 0x3000)
        XCTAssertEqual(value(combined, at: 16), 16_384)
        a.sendActions(for: .touchCancel)
        let bOnly = try XCTUnwrap(sink.reliablePackets.last)
        XCTAssertEqual(value(bOnly, at: 12), 0x2000)
        XCTAssertEqual(value(bOnly, at: 16), 16_384)
        XCTAssertTrue(b.isPressed)
        XCTAssertFalse(a.isPressed)
        b.sendActions(for: .touchUpInside)
        try await Task.sleep(nanoseconds: 90_000_000)
        let released = try XCTUnwrap(sink.reliablePackets.last)
        XCTAssertEqual(value(released, at: 12), 0)
        XCTAssertEqual(value(released, at: 16), 16_384)
        XCTAssertFalse(b.isPressed)
        bridge.setVirtualControllerEnabled(false)
    }

    @MainActor
    func testLiveMetalFXTogglePersistsWithoutOverwritingNewStreamSettings() throws {
        let defaults = UserDefaults.standard
        let key = "OpenNOW.iOS.settings"
        let previous = defaults.object(forKey: key)
        defer {
            if let previous { defaults.set(previous, forKey: key) }
            else { defaults.removeObject(forKey: key) }
        }
        let store = OpenNOWStore()
        var live = AppSettings.default
        live.metal4Enabled = false
        store.settings.metal4Enabled = true
        live.metalFXUpscalingEnabled = false
        store.settings.metalFXUpscalingEnabled = true
        store.settings.preferredFPS = 120
        store.settings.preferredAspectRatio = "21:9"
        store.settings.preferredResolution = "2560x1080"
        store.settings.hdrEnabled = true
        store.settings.enableCloudGsync = false
        let before = store.settings
        store.applyStreamerSettings(live)
        XCTAssertFalse(store.settings.metalFXUpscalingEnabled)
        XCTAssertEqual(store.settings.preferredFPS, before.preferredFPS)
        XCTAssertEqual(store.settings.preferredResolution, before.preferredResolution)
        XCTAssertEqual(store.settings.hdrEnabled, before.hdrEnabled)
        XCTAssertEqual(store.settings.enableCloudGsync, before.enableCloudGsync)
        let saved = try JSONDecoder().decode(AppSettings.self, from: XCTUnwrap(defaults.data(forKey: key)))
        XCTAssertFalse(saved.metal4Enabled)
        XCTAssertFalse(saved.metalFXUpscalingEnabled)
        live.metal4Enabled = true
        live.metalFXUpscalingEnabled = true
        store.applyStreamerSettings(live)
        let savedOn = try JSONDecoder().decode(AppSettings.self, from: XCTUnwrap(defaults.data(forKey: key)))
        XCTAssertTrue(savedOn.metal4Enabled)
        XCTAssertTrue(savedOn.metalFXUpscalingEnabled)
    }

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
            NSLog("[SettingsNavigationTest] opening settings for %@", String(describing: route))
            try await selectSidebarRow(3)
            let settingsTitles = ["Settings", "General", "Stream", "Input", "Interface", "Account"]
            XCTAssertTrue(titles().contains(where: settingsTitles.contains),
                          "Selection must open the actual Settings view: \(titles())")
            store.pendingSettingsRoute = route
            NSLog("[SettingsNavigationTest] pushing %@", String(describing: route))
            try await settle()
            XCTAssertNil(store.pendingSettingsRoute)
            XCTAssertTrue(titles().contains(String(describing: route).capitalized),
                          "The requested settings category must actually be pushed: \(titles())")
            try await selectSidebarRow(0)
            NSLog("[SettingsNavigationTest] home selected")
            XCTAssertTrue(titles().contains("Store") && !titles().contains(where: settingsTitles.contains),
                          "Home navigation must replace the settings stack: \(titles())")
        }
    }

    func testLiveActivityProgressRemainsFiniteAndBoundedForRestoredState() {
        for progress: Double? in [nil, .nan, .infinity, -.infinity, -5, 0, 0.5, 1, 5, .greatestFiniteMagnitude] {
            for phase in [QueueActivityAttributes.ContentState.Phase.queued, .waiting, .ready] {
                let fraction = QueueActivityProgress.barFraction(progress, phase: phase)
                XCTAssertTrue(fraction.isFinite)
                XCTAssertTrue((0...1).contains(fraction))
            }
        }
        XCTAssertNil(QueueActivityProgress.normalized(.nan))
        XCTAssertNil(QueueActivityProgress.normalized(.infinity))
        XCTAssertEqual(QueueActivityProgress.normalized(-1), 0)
        XCTAssertEqual(QueueActivityProgress.normalized(2), 1)
        XCTAssertEqual(QueueActivityProgress.barFraction(0.5, phase: .queued), 0.5)
        XCTAssertEqual(QueueActivityProgress.barFraction(nil, phase: .waiting), 0.75)
        XCTAssertEqual(QueueActivityProgress.barFraction(nil, phase: .ready), 1)
    }

    func testMetal4RenderingIsOptInAndPersistsIndependentlyOfHDRAndMetalFX() throws {
        let legacy = try JSONDecoder().decode(AppSettings.self, from: Data("{}".utf8))
        XCTAssertFalse(legacy.metal4Enabled)
        var settings = AppSettings.default
        XCTAssertFalse(settings.metal4Enabled)
        settings.hdrEnabled = true
        settings.metalFXUpscalingEnabled = true
        for enabled in [true, false] {
            settings.metal4Enabled = enabled
            let restored = try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(settings))
            XCTAssertEqual(restored.metal4Enabled, enabled)
            XCTAssertTrue(restored.hdrEnabled)
            XCTAssertTrue(restored.metalFXUpscalingEnabled)
        }
    }

    func testRetiredFrameGenerationPreferencesAreIgnoredAndOmittedOnSave() throws {
        var settings = AppSettings.default
        settings.preferredResolution = "2560x1440"
        settings.preferredFPS = 120
        settings.preferredCodec = "AV1"
        settings.hdrEnabled = true
        settings.preferredColorQuality = StreamColorQuality.tenBit420.rawValue
        settings.metalFXUpscalingEnabled = true
        settings.metalFXQualityPreset = .quality
        let encoded = try JSONEncoder().encode(settings)
        // Compare with the ordinary load path, which also normalizes unrelated
        // legacy defaults such as the automatic region and report version.
        let baseline = try JSONDecoder().decode(AppSettings.self, from: encoded)
        var legacy = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        legacy["frameGenerationEnabled"] = true
        legacy["frameGenerationQuality"] = "native"
        legacy["showMetalPerformanceHUD"] = true
        let restored = try JSONDecoder().decode(AppSettings.self, from: JSONSerialization.data(withJSONObject: legacy))
        XCTAssertEqual(restored, baseline)
        XCTAssertEqual(restored.preferredResolution, "2560x1440")
        XCTAssertEqual(restored.preferredFPS, 120)
        XCTAssertEqual(restored.preferredCodec, "AV1")
        XCTAssertTrue(restored.hdrEnabled)
        XCTAssertEqual(restored.preferredColorQuality, StreamColorQuality.tenBit420.rawValue)
        XCTAssertTrue(restored.metalFXUpscalingEnabled)
        XCTAssertEqual(restored.metalFXQualityPreset, .quality)
        let saved = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(restored)) as? [String: Any])
        XCTAssertNil(saved["frameGenerationEnabled"])
        XCTAssertNil(saved["frameGenerationQuality"])
        XCTAssertNil(saved["showMetalPerformanceHUD"])
        let display = CGSize(width: 2868, height: 1320)
        let resolution = try XCTUnwrap(StreamSettingsResolver.metalFXResolution(preset: .quality,
            aspectRatio: "21:9", displaySize: display, stretch: true, membershipTier: "ULTIMATE"))
        XCTAssertEqual(resolution.value, "2560x1080")
    }

    func testNativeEffectsColorMetadataPreservesHDRAndRejectsAmbiguity() throws {
        func buffer(_ format:OSType) throws -> CVPixelBuffer {
            var result:CVPixelBuffer?
            XCTAssertEqual(CVPixelBufferCreate(nil,64,32,format,
                [kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,&result),kCVReturnSuccess)
            return try XCTUnwrap(result)
        }
        let yuv = try buffer(kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange)
        XCTAssertEqual(NativeStreamMetalVideoInput.color(yuv)?.presentationTransfer,0)
        CVBufferSetAttachment(yuv,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,.shouldPropagate)
        XCTAssertNil(NativeStreamMetalVideoInput.color(yuv))
        CVBufferSetAttachment(yuv,kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_2020,.shouldPropagate)
        XCTAssertEqual(NativeStreamMetalVideoInput.color(yuv)?.presentationTransfer,1)
        CVBufferSetAttachment(yuv,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_ITU_R_2100_HLG,.shouldPropagate)
        XCTAssertEqual(NativeStreamMetalVideoInput.color(yuv)?.transfer,2)
        CVBufferSetAttachment(yuv,kCVImageBufferTransferFunctionKey,"UnknownTransfer" as CFString,.shouldPropagate)
        XCTAssertNil(NativeStreamMetalVideoInput.color(yuv))
        let half = try buffer(kCVPixelFormatType_64RGBAHalf)
        XCTAssertNil(NativeStreamMetalVideoInput.color(half))
        CVBufferSetAttachment(half,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_Linear,.shouldPropagate)
        CVBufferSetAttachment(half,kCVImageBufferCGColorSpaceKey,NativeStreamVideoEffectsPolicy.workingColorSpace(hdr:true),.shouldPropagate)
        XCTAssertEqual(NativeStreamMetalVideoInput.color(half)?.presentationTransfer,1)
        CVBufferSetAttachment(half,kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_709_2,.shouldPropagate)
        XCTAssertNil(NativeStreamMetalVideoInput.color(half))
        CVBufferRemoveAttachment(half,kCVImageBufferColorPrimariesKey)
        CVBufferSetAttachment(half,kCVImageBufferCGColorSpaceKey,CGColorSpace(name:CGColorSpace.displayP3)!, .shouldPropagate)
        XCTAssertNil(NativeStreamMetalVideoInput.color(half))
    }

    func testSelectingEightBitDisablesHDRAndSurvivesSettingsReloadAndLaunch() throws {
        var initial = AppSettings.default
        initial.experimentalNativeNVSTEnabled = true
        initial.preferredCodec = "H265"
        initial.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        initial.hdrEnabled = true
        initial.metalFXUpscalingEnabled = true
        let selected = StreamSettingsResolver.selectingColor(.eightBit420,in:initial)
        let restored = try JSONDecoder().decode(AppSettings.self,from:JSONEncoder().encode(selected))
        let launch = NativeStreamLaunchSettingsResolver.resolve(restored).settings
        XCTAssertFalse(launch.hdrEnabled)
        XCTAssertEqual(launch.preferredColorQuality,StreamColorQuality.eightBit420.rawValue)
        XCTAssertEqual(StreamSettingsResolver.colorQuality(for:launch),.eightBit420)
        XCTAssertFalse(StreamSettingsResolver.requiresDesktopColorProvisioning(for:launch))
        XCTAssertTrue(launch.metalFXUpscalingEnabled)
        XCTAssertEqual(launch.preferredCodec,"H265")
        let quality = StreamSettingsResolver.colorQuality(for:launch)
        let request = CloudMatchStreamingFeatureRequest.build(settings:launch,
            profile:StreamSettingsResolver.profile(for:launch),bitDepth:quality.bitDepth,chromaFormat:quality.chromaFormat)
        XCTAssertEqual(request["trueHdr"] as? Bool,false)
        XCTAssertEqual(request["bitDepth"] as? Int,0)
        XCTAssertEqual(request["chromaFormat"] as? Int,0)
        XCTAssertFalse(StreamSettingsResolver.remoteColorMatches(color:.tenBit444,hdr:true,settings:launch))
        XCTAssertTrue(StreamSettingsResolver.remoteColorMatches(color:.eightBit420,hdr:false,settings:launch))
        XCTAssertNotEqual(StreamSettingsResolver.sessionSignature(for:initial),StreamSettingsResolver.sessionSignature(for:launch))
        let sdp = NativeStreamSDP.buildNvstSDP(offerSDP:"",localAnswerSDP:"",
            profile:StreamSettingsResolver.profile(for:launch),settings:launch,codec:.h265)
        XCTAssertTrue(sdp.contains("a=video.dynamicRangeMode:0\n"))
        XCTAssertTrue(sdp.contains("a=video.bitDepth:8\n"))
    }

    func testHDRTogglePreservesExplicitSDRChromaAndReenablesTenBit() throws {
        var initial = AppSettings.default
        initial.experimentalNativeNVSTEnabled = true
        initial.preferredCodec = "H265"
        initial.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        initial.hdrEnabled = true
        let off = StreamSettingsResolver.selectingHDR(false,in:initial,h265Available:true)
        let reloaded = try JSONDecoder().decode(AppSettings.self,from:JSONEncoder().encode(off))
        XCTAssertFalse(reloaded.hdrEnabled)
        XCTAssertEqual(StreamSettingsResolver.colorQuality(for:reloaded),.tenBit444)
        let chromaOff = StreamSettingsResolver.selectingColor(.tenBit420,in:reloaded)
        XCTAssertFalse(chromaOff.hdrEnabled)
        XCTAssertEqual(StreamSettingsResolver.colorQuality(for:chromaOff),.tenBit420)
        let eightBit = StreamSettingsResolver.selectingColor(.eightBit420,in:chromaOff)
        let on = StreamSettingsResolver.selectingHDR(true,in:eightBit,h265Available:true)
        XCTAssertTrue(on.hdrEnabled)
        XCTAssertEqual(on.preferredColorQuality,StreamColorQuality.tenBit420.rawValue)
        XCTAssertEqual(StreamSettingsResolver.colorQuality(for:on),.tenBit420)
        XCTAssertFalse(StreamSettingsResolver.requiresDesktopColorProvisioning(for:on))
        XCTAssertEqual(StreamSettingsResolver.selectingColor(.tenBit420,in:initial).hdrEnabled,true)
    }

    func testVideoSelectionKeepsCodecRulesAndLegacyHDRMigration() {
        var initial = AppSettings.default
        initial.preferredCodec = "AV1"
        initial.experimentalNativeNVSTEnabled = true
        let fullChroma = StreamSettingsResolver.selectingColor(.tenBit444,in:initial)
        XCTAssertEqual(fullChroma.preferredCodec,"H265")
        XCTAssertFalse(fullChroma.hdrEnabled)
        let hdr = StreamSettingsResolver.selectingHDR(true,in:initial,h265Available:false)
        XCTAssertEqual(hdr.preferredCodec,"AV1")
        XCTAssertEqual(hdr.preferredColorQuality,StreamColorQuality.tenBit420.rawValue)
        // Old saved HDR + 8-bit settings retain their migration behavior. Only
        // a new explicit 8-bit picker choice turns HDR off.
        initial.hdrEnabled = true
        XCTAssertEqual(StreamSettingsResolver.colorQuality(for:initial),.tenBit420)
        XCTAssertFalse(StreamSettingsResolver.selectingColor(.eightBit420,in:initial).hdrEnabled)
    }

    func testPresentationRatesCountActualDisplayedFramesAndExpire() throws {
        var meter = NativeStreamPresentationRateMeter()
        XCTAssertNil(meter.snapshot(now: 100))
        for i in 0...240 { meter.observe(time: 100 + Double(i)/120) }
        let rates = try XCTUnwrap(meter.snapshot(now: 102))
        XCTAssertEqual(rates.displayedFPS, 120, accuracy: 0.01)
        meter.observe(time: 101) // delayed/out-of-order callback
        meter.observe(time: .nan)
        XCTAssertEqual(meter.snapshot(now: 102), rates)
        XCTAssertNil(meter.snapshot(now: 99))
        XCTAssertNil(meter.snapshot(now: 105))
        meter.observe(time: 110) // resume starts a fresh window
        XCTAssertNil(meter.snapshot(now: 110))
        for i in 1...60 { meter.observe(time: 110 + Double(i)/60) }
        let realOnly = try XCTUnwrap(meter.snapshot(now: 111))
        XCTAssertEqual(realOnly.displayedFPS, 60, accuracy: 0.01)
    }

    func testMetalFXPresetsSelectEligibleSizesAndRespectPlanAndAspect() throws {
        let phone = CGSize(width: 2796, height: 1290)
        let selected = try [MetalFXQualityPreset.quality, .balanced, .performance].map { preset in
            try XCTUnwrap(StreamSettingsResolver.metalFXResolution(preset: preset, aspectRatio: "16:10",
                displaySize: phone, stretch: false, membershipTier: "ULTIMATE"))
        }
        XCTAssertEqual(selected.map(\.value), ["1680x1050", "1440x900", "1280x800"])
        for choice in selected {
            let source = StreamSettingsResolver.pixelSize(choice.value)
            let output = NativeStreamVideoEffectsPolicy.presentationSize(source: source, display: phone, stretch: false)
            XCTAssertNotNil(NativeStreamVideoEffectsPolicy.upscaleSize(source: source, destination: output))
            XCTAssertEqual(output.width / output.height, 1.6, accuracy: 0.001)
        }
        XCTAssertNil(StreamSettingsResolver.metalFXResolution(preset: .manual, aspectRatio: "16:10",
            displaySize: phone, stretch: false, membershipTier: "ULTIMATE"))
        XCTAssertNil(StreamSettingsResolver.metalFXResolution(preset: .quality, aspectRatio: "32:9",
            displaySize: phone, stretch: false, membershipTier: "ULTIMATE"))
        let hugeDisplay = CGSize(width: 7680, height: 4320)
        let free = try XCTUnwrap(StreamSettingsResolver.metalFXResolution(preset: .quality, aspectRatio: "16:9",
            displaySize: hugeDisplay, stretch: false, membershipTier: nil))
        XCTAssertEqual(free.value, "1920x1080")
        XCTAssertEqual(free.requiredPlan, .free)
        let filled = NativeStreamVideoEffectsPolicy.presentationSize(source: CGSize(width: 1280, height: 800),
            display: phone, stretch: true)
        XCTAssertEqual(filled, phone)
    }

    func testMetalFXEligibilityUsesFittedOutputGeometry() {
        // A 16:10 source fitted within the narrower-height landscape phone viewport
        // must not be advertised as upscaling when both axes actually shrink.
        XCTAssertNil(NativeStreamVideoEffectsPolicy.upscaleSize(source: CGSize(width: 2560, height: 1600),
            destination: CGSize(width: 2064, height: 1290)))
        XCTAssertNil(NativeStreamVideoEffectsPolicy.upscaleSize(source: CGSize(width: 1920, height: 1200),
            destination: CGSize(width: 2064, height: 1290)))
        XCTAssertNil(NativeStreamVideoEffectsPolicy.upscaleSize(source: CGSize(width: 2560, height: 1080),
            destination: CGSize(width: 2868, height: 1320)))
        XCTAssertEqual(NativeStreamVideoEffectsPolicy.upscaleSize(source: CGSize(width: 1920, height: 1080),
            destination: CGSize(width: 2560, height: 1440)), CGSize(width: 2560, height: 1440))
    }

    func testMetalFXEnabledButIneligibleUsesSinglePassHDRRenderer() {
        let source = CGSize(width: 2560, height: 1080)
        let destination = CGSize(width: 2868, height: 1320)
        let eligible = NativeStreamVideoEffectsPolicy.upscaleSize(source: source, destination: destination) != nil
        XCTAssertFalse(eligible)
        XCTAssertTrue(NativeStreamVideoEffectsPolicy.canUseDirectHDRPath(
            upscalingEnabled: true, upscaleEligible: eligible, sharpeningAmount: 0))
        XCTAssertFalse(NativeStreamVideoEffectsPolicy.canUseDirectHDRPath(
            upscalingEnabled: true, upscaleEligible: true, sharpeningAmount: 0))
        XCTAssertFalse(NativeStreamVideoEffectsPolicy.canUseDirectHDRPath(
            upscalingEnabled: true, upscaleEligible: eligible, sharpeningAmount: 0.1))
    }

    func testVideoEffectsSettingsMigrateOffAndRoundTripWithoutChangingStream() throws {
        let old = try JSONDecoder().decode(AppSettings.self, from: Data("{}".utf8))
        XCTAssertFalse(old.metalFXUpscalingEnabled)
        XCTAssertEqual(old.metalFXQualityPreset, .manual)
        var settings = AppSettings.default
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.hdrEnabled = true
        settings.experimentalNativeNVSTEnabled = true
        settings.preferredCodec = "H265"
        settings.normalizeStreamDefaults()
        settings.metalFXUpscalingEnabled = true
        settings.metalFXQualityPreset = .balanced
        let decoded = try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(settings))
        XCTAssertEqual(decoded, settings)
        XCTAssertEqual(decoded.preferredColorQuality, StreamColorQuality.tenBit444.rawValue)
        XCTAssertTrue(decoded.hdrEnabled)
    }

    func testVideoEffectsAvoidDownscaling() {
        let source = CGSize(width: 1920, height: 1080)
        XCTAssertEqual(NativeStreamVideoEffectsPolicy.upscaleSize(source: source,
            destination: CGSize(width: 2560, height: 1440)), CGSize(width: 2560, height: 1440))
        XCTAssertNil(NativeStreamVideoEffectsPolicy.upscaleSize(source: source, destination: source))
        XCTAssertNil(NativeStreamVideoEffectsPolicy.upscaleSize(source: source,
            destination: CGSize(width: 1280, height: 720)))

    }

    @MainActor
    func testMetalFXSpatialHDRRetainsHighlightsAndImageOrientation() async throws {
        #if canImport(MetalFX)
        let device = try XCTUnwrap(MTLCreateSystemDefaultDevice())
        guard MTLFXSpatialScalerDescriptor.supportsDevice(device) else { throw XCTSkip("MetalFX unavailable on this simulator GPU") }
        let queue = try XCTUnwrap(device.makeCommandQueue())
        let context = CIContext(mtlDevice: device, options: [.cacheIntermediates: false])
        let scaler = NativeStreamSpatialUpscaler(device: device)
        let space = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr: true)
        var data = [Float](repeating: 0, count: 64 * 32 * 4)
        for y in 0..<32 { for x in 0..<64 {
            let i = (y * 64 + x) * 4
            data[i] = x < 32 ? 4 : 0.1
            data[i + 1] = y < 16 ? 0.1 : 2
            data[i + 2] = 0.25; data[i + 3] = 1
        } }
        let image = data.withUnsafeBytes { CIImage(bitmapData: Data($0), bytesPerRow: 64 * 16,
            size: CGSize(width: 64, height: 32), format: .RGBAf, colorSpace: space) }
        var result: CIImage?
        for _ in 0..<200 {
            let command = try XCTUnwrap(queue.makeCommandBuffer())
            result = scaler.encode(image: image, sourceSize: image.extent.size,
                destinationSize: CGSize(width: 128, height: 64), hdr: true, context: context, commandBuffer: command)
            command.commit(); command.waitUntilCompleted()
            XCTAssertEqual(command.status, .completed)
            if result != nil { break }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        let output = try XCTUnwrap(result, scaler.status)
        XCTAssertEqual(output.extent.size, CGSize(width: 128, height: 64))
        let td = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba32Float, width: 128, height: 64, mipmapped: false)
        td.storageMode = .shared; td.usage = [.renderTarget, .shaderRead, .shaderWrite]
        let texture = try XCTUnwrap(device.makeTexture(descriptor: td))
        let command = try XCTUnwrap(queue.makeCommandBuffer())
        context.render(output, to: texture, commandBuffer: command, bounds: output.extent, colorSpace: space)
        command.commit(); command.waitUntilCompleted()
        XCTAssertEqual(command.status, .completed)
        var values = [Float](repeating: 0, count: 128 * 64 * 4)
        values.withUnsafeMutableBytes { texture.getBytes($0.baseAddress!, bytesPerRow: 128 * 16,
            from: MTLRegionMake2D(0, 0, 128, 64), mipmapLevel: 0) }
        XCTAssertGreaterThan(values[(8 * 128 + 8) * 4], 3, "HDR highlights must not clamp to SDR")
        XCTAssertLessThan(values[(8 * 128 + 120) * 4], 0.5, "Horizontal orientation must be preserved")
        let reference = try XCTUnwrap(device.makeTexture(descriptor: td))
        let referenceCommand = try XCTUnwrap(queue.makeCommandBuffer())
        context.render(image.transformed(by: CGAffineTransform(scaleX: 2, y: 2)), to: reference,
            commandBuffer: referenceCommand, bounds: output.extent, colorSpace: space)
        referenceCommand.commit(); referenceCommand.waitUntilCompleted()
        XCTAssertEqual(referenceCommand.status, .completed)
        var baseline = [Float](repeating: 0, count: values.count)
        baseline.withUnsafeMutableBytes { reference.getBytes($0.baseAddress!, bytesPerRow: 128 * 16,
            from: MTLRegionMake2D(0, 0, 128, 64), mipmapLevel: 0) }
        for offset in [(8 * 128 + 8) * 4 + 1, (56 * 128 + 8) * 4 + 1] {
            XCTAssertEqual(values[offset], baseline[offset], accuracy: 0.1,
                "MetalFX must preserve ordinary playback orientation")
        }
        #else
        throw XCTSkip("Apple does not ship MetalFX in the iOS simulator SDK; device path is tested separately on macOS Metal")
        #endif
    }

    func testNativeKeyframeRecoverySendsExplicitControlCommandWithoutFeedbackChannel() throws {
        var sent: NvstControlCommand?
        var udpRequests = 0
        XCTAssertTrue(NativeStreamKeyframeRecovery.request(sendControl: { sent = $0; return true },
            sendUDP: { udpRequests += 1 }))
        XCTAssertEqual(try XCTUnwrap(sent).encoded, Data([0x02, 0x03, 0x02, 0x00, 0x00, 0x00]))
        XCTAssertEqual(udpRequests, 0)
    }

    func testNativeKeyframeRecoveryFallsBackWhenControlCannotSend() {
        var attempts = 0
        var udpRequests = 0
        XCTAssertFalse(NativeStreamKeyframeRecovery.request(sendControl: { _ in attempts += 1; return false },
            sendUDP: { udpRequests += 1 }))
        XCTAssertEqual(attempts, 1)
        XCTAssertEqual(udpRequests, 1)
    }

    func testDecodeInboxPreservesHealthyBurstsAndFIFO() {
        var inbox = NvstDecodeFrameInbox<Int>()
        for frame in 0..<12 {
            let events = inbox.offer(frame, isKeyframe: frame == 0, now: 1)
            XCTAssertEqual(events.discarded, 0)
            XCTAssertFalse(events.requestKeyframe)
        }
        for frame in 0..<12 {
            let next = inbox.take(now: 80_000_000)
            XCTAssertEqual(next.entry?.value, frame)
            XCTAssertFalse(next.events.resynchronised)
        }
        XCTAssertNil(inbox.take(now: 80_000_000).entry)
    }

    func testDecodeInboxOverflowDropsBrokenChainUntilFreshKeyframe() {
        var inbox = NvstDecodeFrameInbox<Int>(capacity: 4)
        for frame in 0..<4 { _ = inbox.offer(frame, isKeyframe: frame == 0, now: 1) }
        let overflow = inbox.offer(4, isKeyframe: false, now: 2)
        XCTAssertEqual(overflow.discarded, 5)
        XCTAssertTrue(overflow.resynchronised)
        XCTAssertTrue(overflow.requestKeyframe)
        XCTAssertTrue(inbox.awaitingKeyframe)
        XCTAssertEqual(inbox.count, 0)
        for frame in 5..<10_000 {
            let rejected = inbox.offer(frame, isKeyframe: false, now: 3)
            XCTAssertEqual(rejected.discarded, 1)
            XCTAssertFalse(rejected.requestKeyframe)
            XCTAssertEqual(inbox.count, 0)
        }
        XCTAssertTrue(inbox.offer(10_000, isKeyframe: false, now: 400_000_002).requestKeyframe)
        XCTAssertFalse(inbox.offer(10_001, isKeyframe: true, now: 400_000_003).requestKeyframe)
        _ = inbox.offer(10_002, isKeyframe: false, now: 400_000_004)
        XCTAssertEqual(inbox.take(now: 400_000_005).entry?.value, 10_001)
        XCTAssertEqual(inbox.take(now: 400_000_005).entry?.value, 10_002)
        XCTAssertFalse(inbox.awaitingKeyframe)
    }

    func testDecodeInboxExpiresQueuedWorkAndKeepsFreshRecoveryKeyframe() {
        var inbox = NvstDecodeFrameInbox<Int>(capacity: 2)
        _ = inbox.offer(0, isKeyframe: true, now: 0)
        _ = inbox.offer(1, isKeyframe: false, now: 1)
        let expired = inbox.take(now: 250_000_002)
        XCTAssertNil(expired.entry)
        XCTAssertEqual(expired.events.discarded, 2)
        XCTAssertTrue(expired.events.requestKeyframe)
        _ = inbox.offer(2, isKeyframe: true, now: 250_000_003)
        _ = inbox.offer(3, isKeyframe: false, now: 250_000_004)
        let recovery = inbox.offer(4, isKeyframe: true, now: 250_000_005)
        XCTAssertEqual(recovery.discarded, 2)
        XCTAssertFalse(recovery.requestKeyframe)
        XCTAssertEqual(inbox.take(now: 250_000_006).entry?.value, 4)
        inbox.removeAll()
        XCTAssertEqual(inbox.count, 0)
        XCTAssertFalse(inbox.awaitingKeyframe)
    }

    @available(iOS 17.0, *)
    func test444DecoderOffersCompatibleRangesWithoutAllowingDepthOrChromaDowngrade() {
        var format = NvstVideoToolboxDecoder.BitstreamFormat()
        format.bitDepth = 10; format.chroma = .yuv444
        for fullRange in [false, true] {
            let requests = NvstVideoToolboxDecoder.outputPixelFormatRequests(for: format,
                requiresTenBit444: true, sourceIsFullRange: fullRange)
            XCTAssertEqual(requests.count, 3)
            XCTAssertEqual(Set(requests[0]), Set([kCVPixelFormatType_444YpCbCr10BiPlanarFullRange,
                kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange]))
            XCTAssertEqual(requests[1], [fullRange ? kCVPixelFormatType_444YpCbCr10BiPlanarFullRange
                : kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange])
            for request in requests {
                XCTAssertTrue(request.allSatisfy { NativeStreamTenBitSurface.chroma($0) == "4:4:4" })
            }
        }
        format.chroma = .yuv420
        XCTAssertEqual(NvstVideoToolboxDecoder.outputPixelFormatRequests(for: format,
            requiresTenBit444: false, sourceIsFullRange: false),
            [[kCVPixelFormatType_420YpCbCr10BiPlanarFullRange]])
    }

    func testPresentationTrackerKeepsRatesDuringConcurrentSnapshots() throws {
        let tracker = NativeStreamPresentationTracker()
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global().async {
            for i in 0...240 { tracker.recordPresentation(at: 100 + Double(i) / 120) }
            group.leave()
        }
        group.enter()
        DispatchQueue.global().async {
            for _ in 0..<1000 { _ = tracker.rates(now: 102) }
            group.leave()
        }
        XCTAssertEqual(group.wait(timeout: .now() + 5), .success)
        XCTAssertEqual(try XCTUnwrap(tracker.rates(now: 102)).displayedFPS, 120, accuracy: 0.01)
        tracker.resetRates()
        XCTAssertNil(tracker.rates(now: 102))
        for i in 0...60 { tracker.recordPresentation(at: 110 + Double(i) / 60) }
        XCTAssertEqual(try XCTUnwrap(tracker.rates(now: 111)).displayedFPS, 60, accuracy: 0.01)
    }

    func testSpatialInputClampsInScalerGamutAndRetainsHDRHighlights() throws {
        let context = CIContext(options: [.workingColorSpace: NativeStreamVideoEffectsPolicy.workingColorSpace(hdr: false),
                                          .workingFormat: CIFormat.RGBAf])
        for (hdr, source, expected) in [(true, [Float(4), 0, 0, 1], [Float(4), 0, 0, 1]),
                                       (false, [Float(-0.5), 2, 0.5, 1], [Float(0), 1, 0.5, 1])] {
            let space = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr: hdr)
            let image = source.withUnsafeBytes { CIImage(bitmapData: Data($0), bytesPerRow: 16,
                size: CGSize(width: 1, height: 1), format: .RGBAf, colorSpace: space) }
            let clamped = NativeStreamVideoEffectsPolicy.spatialInput(image: image, hdr: hdr)
            var values = [Float](repeating: 0, count: 4)
            values.withUnsafeMutableBytes { context.render(clamped, toBitmap: $0.baseAddress!, rowBytes: 16,
                bounds: image.extent, format: .RGBAf, colorSpace: space) }
            for component in 0..<4 {
                XCTAssertEqual(values[component], expected[component], accuracy: 0.005,
                    "Clamping must retain saturated BT.2020 HDR and bound SDR overshoot")
            }
        }
    }

    func testHDRMetalProgramRejectsUnsupportedFormatMatrixAndTransfer() throws {
        let device = try XCTUnwrap(MTLCreateSystemDefaultDevice())
        var cache: CVMetalTextureCache?
        XCTAssertEqual(CVMetalTextureCacheCreate(nil,nil,device,nil,&cache),kCVReturnSuccess)
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgr10a2Unorm,width:16,height:8,mipmapped:false)
        descriptor.usage = .renderTarget
        let target = try XCTUnwrap(device.makeTexture(descriptor:descriptor))
        let destination = CGRect(x:0,y:0,width:16,height:8)
        for format in [kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,kCVPixelFormatType_420YpCbCr10BiPlanarFullRange] {
            var allocation: CVPixelBuffer?
            XCTAssertEqual(CVPixelBufferCreate(nil,16,8,format,
                [kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,&allocation),kCVReturnSuccess)
            let buffer = try XCTUnwrap(allocation)
            XCTAssertEqual(NativeStreamTenBitSurface.hasValidPlanes(buffer),
                format == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange)
            func input() -> NativeStreamHDRMetalProgram.Input? {
                NativeStreamHDRMetalProgram.Input(buffer:buffer,cache:cache!,target:target,destination:destination)
            }
            CVBufferSetAttachment(buffer,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,.shouldPropagate)
            CVBufferSetAttachment(buffer,kCVImageBufferYCbCrMatrixKey,kCVImageBufferYCbCrMatrix_ITU_R_709_2,.shouldPropagate)
            XCTAssertNil(input())
            CVBufferSetAttachment(buffer,kCVImageBufferYCbCrMatrixKey,kCVImageBufferYCbCrMatrix_ITU_R_2020,.shouldPropagate)
            if format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange { XCTAssertNil(input()) }
            else {
                XCTAssertNotNil(input())
                CVBufferSetAttachment(buffer,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_ITU_R_2100_HLG,.shouldPropagate)
                XCTAssertNotNil(input())
                CVBufferSetAttachment(buffer,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_ITU_R_709_2,.shouldPropagate)
                XCTAssertNil(input())
            }
        }
        #if targetEnvironment(simulator)
        if #available(iOS 26.0, *) {
            XCTAssertFalse(NativeStreamMetal4HDRRenderer.isSupported(device:device))
            XCTAssertNil(NativeStreamMetal4HDRRenderer(device:device))
            XCTAssertNotNil(NativeStreamHDRMetalRenderer(device:device))
        }
        #endif
    }

    func testDirect444HDRMetalPreservesAlternatingFullResolutionChroma() throws {
        let device = try XCTUnwrap(MTLCreateSystemDefaultDevice())
        let queue = try XCTUnwrap(device.makeCommandQueue())
        let renderer = try XCTUnwrap(NativeStreamHDRMetalRenderer(device: device))
        for format in [kCVPixelFormatType_444YpCbCr10BiPlanarFullRange, kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange] {
            var buffer: CVPixelBuffer?
            XCTAssertEqual(CVPixelBufferCreate(nil, 16, 8, format,
                [kCVPixelBufferIOSurfacePropertiesKey: [:], kCVPixelBufferMetalCompatibilityKey: true] as CFDictionary,
                &buffer), kCVReturnSuccess)
            let source = try XCTUnwrap(buffer)
            CVPixelBufferLockBaseAddress(source, [])
            for plane in 0..<2 {
                let base = try XCTUnwrap(CVPixelBufferGetBaseAddressOfPlane(source, plane)).assumingMemoryBound(to: UInt16.self)
                let stride = CVPixelBufferGetBytesPerRowOfPlane(source, plane) / 2
                for y in 0..<8 {
                    for x in 0..<16 {
                        if plane == 0 { base[y * stride + x] = 512 << 6 }
                        else {
                            base[y * stride + x * 2] = 512 << 6
                            base[y * stride + x * 2 + 1] = UInt16(x.isMultiple(of: 2) ? 512 : 640) << 6
                        }
                    }
                }
            }
            CVPixelBufferUnlockBaseAddress(source, [])
            CVBufferSetAttachment(source, kCVImageBufferTransferFunctionKey, kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ, .shouldPropagate)
            CVBufferSetAttachment(source, kCVImageBufferYCbCrMatrixKey, kCVImageBufferYCbCrMatrix_ITU_R_2020, .shouldPropagate)
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgr10a2Unorm, width: 16, height: 8, mipmapped: false)
            descriptor.storageMode = .shared; descriptor.usage = [.renderTarget, .shaderRead]
            let output = try XCTUnwrap(device.makeTexture(descriptor: descriptor))
            let pass = MTLRenderPassDescriptor()
            pass.colorAttachments[0].texture = output
            pass.colorAttachments[0].loadAction = .clear; pass.colorAttachments[0].storeAction = .store
            let command = try XCTUnwrap(queue.makeCommandBuffer())
            XCTAssertTrue(renderer.encode(buffer: source, commandBuffer: command, descriptor: pass,
                destination: CGRect(x: 0, y: 0, width: 16, height: 8)))
            command.commit(); command.waitUntilCompleted()
            XCTAssertEqual(command.status, .completed)
            var pixels = [UInt32](repeating: 0, count: 16 * 8)
            pixels.withUnsafeMutableBytes { bytes in
                output.getBytes(bytes.baseAddress!, bytesPerRow: 16 * 4, from: MTLRegionMake2D(0, 0, 16, 8), mipmapLevel: 0)
            }
            let evenRed = Int((pixels[0] >> 20) & 1023), oddRed = Int((pixels[1] >> 20) & 1023)
            XCTAssertGreaterThan(oddRed - evenRed, 100, "The renderer must retain adjacent 4:4:4 chroma differences")
        }
    }

    @available(iOS 17.0, *)
    func testStrict444RejectsHostDowngradeAndOffersOnlyTenBit444Surfaces() throws {
        var format = NvstVideoToolboxDecoder.BitstreamFormat()
        format.bitDepth = 10; format.chroma = .yuv444
        XCTAssertNoThrow(try NvstVideoToolboxDecoder.validate444Bitstream(format))
        XCTAssertEqual(NvstVideoToolboxDecoder.preferredOutputPixelFormats(for: format, requiresTenBit444: true),
            [kCVPixelFormatType_444YpCbCr10BiPlanarFullRange, kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange])
        format.chroma = .yuv420
        XCTAssertThrowsError(try NvstVideoToolboxDecoder.validate444Bitstream(format))
        format.chroma = .yuv444; format.bitDepth = 8
        XCTAssertThrowsError(try NvstVideoToolboxDecoder.validate444Bitstream(format))
        format.bitDepth = 12
        XCTAssertThrowsError(try NvstVideoToolboxDecoder.validate444Bitstream(format))
    }

    func test444HDRHUDAndSurfaceValidationUseDecodedPixels() throws {
        for format in [kCVPixelFormatType_444YpCbCr10BiPlanarFullRange, kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange] {
            var buffer: CVPixelBuffer?
            XCTAssertEqual(CVPixelBufferCreate(nil, 16, 8, format,
                [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary, &buffer), kCVReturnSuccess)
            let pixels = try XCTUnwrap(buffer)
            XCTAssertTrue(NativeStreamTenBitSurface.preserves444(pixels))
            XCTAssertEqual(CVPixelBufferGetWidthOfPlane(pixels, 1), 16)
            XCTAssertEqual(CVPixelBufferGetHeightOfPlane(pixels, 1), 8)
            CVBufferSetAttachment(pixels, kCVImageBufferTransferFunctionKey,
                kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ, .shouldPropagate)
            XCTAssertEqual(NativeStreamHDRTransfer.colorMode(in: pixels), "10-bit 4:4:4 HDR PQ")
        }
        var buffer: CVPixelBuffer?
        XCTAssertEqual(CVPixelBufferCreate(nil, 16, 8, kCVPixelFormatType_420YpCbCr10BiPlanarFullRange, nil, &buffer), kCVReturnSuccess)
        XCTAssertFalse(NativeStreamTenBitSurface.preserves444(try XCTUnwrap(buffer)))
        XCTAssertEqual(NativeStreamTenBitSurface.chroma(kCVPixelFormatType_420YpCbCr10BiPlanarFullRange), "4:2:0")
    }

    func testResumingHDRDoesNotReuseKnownSDRHostFormat() {
        var settings = AppSettings.default
        settings.hdrEnabled = true
        settings.preferredColorQuality = "8bit_420"
        XCTAssertEqual(StreamSettingsResolver.colorQuality(for: settings), .tenBit420)
        XCTAssertFalse(StreamSettingsResolver.remoteColorMatches(color: .eightBit420, hdr: false, settings: settings))
        XCTAssertFalse(StreamSettingsResolver.remoteColorMatches(color: .tenBit420, hdr: false, settings: settings))
        XCTAssertTrue(StreamSettingsResolver.remoteColorMatches(color: .tenBit420, hdr: true, settings: settings))
        settings.preferredColorQuality = "10bit_444"
        XCTAssertFalse(StreamSettingsResolver.remoteColorMatches(color: .tenBit420, hdr: true, settings: settings))
        XCTAssertTrue(StreamSettingsResolver.remoteColorMatches(color: .tenBit444, hdr: true, settings: settings))
    }

    func test444ColorRequestUsesSeparateCloudMatchAndRTSPChromaEnums() throws {
        var settings = AppSettings.default
        settings.experimentalNativeNVSTEnabled = true
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.preferredCodec = "H265"; settings.hdrEnabled = true
        let cloud = CloudMatchStreamingFeatureRequest.build(settings: settings,
            profile: StreamSettingsResolver.profile(for: settings), bitDepth: 10, chromaFormat: 2)
        XCTAssertEqual(cloud["bitDepth"] as? Int, 1)
        XCTAssertEqual(cloud["chromaFormat"] as? Int, 1)
        XCTAssertEqual(cloud["trueHdr"] as? Bool, true)
        let native = NvstRtspSdp.colorFormat(forColorQuality: StreamSettingsResolver.colorQuality(for: settings).rawValue)
        XCTAssertEqual(native.bitDepth, 10)
        XCTAssertEqual(native.chromaFormat, 3)
        let roundTrip = try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(settings))
        XCTAssertEqual(roundTrip.preferredColorQuality, "10bit_444")
        XCTAssertTrue(roundTrip.hdrEnabled)
    }

    func testPointerCaptureReleasesForControlsPiPAndInactiveScene() {
        func capture(video: Bool = true, active: Bool = true, controls: Bool = false,
                     editing: Bool = false, guidance: Bool = false, pip: Bool = false) -> Bool {
            NativeStreamPointerCapturePolicy.shouldCapture(videoActive: video, sceneActive: active,
                controlsVisible: controls, editing: editing, guidanceVisible: guidance, pipActive: pip)
        }
        XCTAssertTrue(capture())
        XCTAssertFalse(capture(video: false))
        XCTAssertFalse(capture(active: false))
        XCTAssertFalse(capture(controls: true))
        XCTAssertFalse(capture(editing: true))
        XCTAssertFalse(capture(guidance: true))
        XCTAssertFalse(capture(pip: true))
    }

    @MainActor
    func testStreamPresentationOwnsPointerLockAndReleasesOnTeardown() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let priorWindow = scene.windows.first(where: \.isKeyWindow)
        let window = UIWindow(windowScene: scene)
        let presenter = NativeStreamPresentationController(content: AnyView(
            Color.black.background(NativeStreamPointerLockPreference(requested: true))))
        window.rootViewController = presenter
        window.makeKeyAndVisible()
        defer { presenter.tearDown(); window.isHidden = true; priorWindow?.makeKeyAndVisible() }
        try await Task.sleep(nanoseconds: 300_000_000)
        XCTAssertTrue(presenter.presentedViewController === presenter.host)
        XCTAssertTrue(presenter.host.prefersPointerLocked)
        presenter.tearDown()
        XCTAssertFalse(presenter.host.prefersPointerLocked)
    }

    func testPiPSamplesUseHostClockAndBoundedAspectCorrectSurfaces() {
        XCTAssertEqual(NativeStreamPiPFrameConverter.outputSize(width: 3840, height: 2160), CGSize(width: 1280, height: 720))
        XCTAssertEqual(NativeStreamPiPFrameConverter.outputSize(width: 2160, height: 3840), CGSize(width: 405, height: 720))
        XCTAssertEqual(NativeStreamPiPFrameConverter.outputSize(width: 0, height: 100), .zero)
        let now = CMClockGetTime(CMClockGetHostTimeClock())
        let sample = NativeStreamPiPSampleTiming.make(at: now)
        XCTAssertEqual(CMTimeCompare(sample.presentationTimeStamp, now), 0)
        XCTAssertEqual(CMTimeGetSeconds(sample.duration), 1.0 / 30.0, accuracy: 0.00001)
        XCTAssertFalse(sample.decodeTimeStamp.isValid)
    }

    func testPiPConvertsTenBitHDRIntoIndependentVisibleSurface() throws {
        var created: CVPixelBuffer?
        XCTAssertEqual(CVPixelBufferCreate(nil, 16, 8, kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
            [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary, &created), kCVReturnSuccess)
        let source = try XCTUnwrap(created)
        CVPixelBufferLockBaseAddress(source, [])
        for plane in 0..<2 {
            let base = try XCTUnwrap(CVPixelBufferGetBaseAddressOfPlane(source, plane)).assumingMemoryBound(to: UInt16.self)
            let words = CVPixelBufferGetBytesPerRowOfPlane(source, plane) / 2 * CVPixelBufferGetHeightOfPlane(source, plane)
            for i in 0..<words { base[i] = UInt16(plane == 0 ? 601 : 512) << 6 }
        }
        CVPixelBufferUnlockBaseAddress(source, [])
        CVBufferSetAttachment(source, kCVImageBufferColorPrimariesKey, kCVImageBufferColorPrimaries_ITU_R_2020, .shouldPropagate)
        CVBufferSetAttachment(source, kCVImageBufferTransferFunctionKey, kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ, .shouldPropagate)
        CVBufferSetAttachment(source, kCVImageBufferYCbCrMatrixKey, kCVImageBufferYCbCrMatrix_ITU_R_2020, .shouldPropagate)
        let converter = NativeStreamPiPFrameConverter()
        let output = try XCTUnwrap(converter.convert(source))
        XCTAssertFalse(output === source)
        XCTAssertEqual(CVPixelBufferGetPixelFormatType(output), kCVPixelFormatType_32BGRA)
        XCTAssertEqual(CVPixelBufferGetPixelFormatType(source), kCVPixelFormatType_420YpCbCr10BiPlanarFullRange)
        XCTAssertEqual(NativeStreamHDRTransfer.detect(in: source), .pq)
        CVPixelBufferLockBaseAddress(output, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(output, .readOnly) }
        let pixels = try XCTUnwrap(CVPixelBufferGetBaseAddress(output)).assumingMemoryBound(to: UInt8.self)
        XCTAssertGreaterThan(pixels[0], 20)
        XCTAssertGreaterThan(pixels[1], 20)
        XCTAssertGreaterThan(pixels[2], 20)
        XCTAssertEqual(pixels[3], 255)
    }

    func testDecodeCompletionHandlesEarlyReorderedAndFailedOutputsWithoutShiftingTiming() {
        var ledger = NvstDecodeCompletionLedger<Double>()
        ledger.register(frameIndex: 40, value: 100)
        // A callback runs before the submission function returns.
        XCTAssertEqual(ledger.take(frameIndex: 40), 100)
        XCTAssertEqual(ledger.count, 0)
        ledger.register(frameIndex: 41, value: 108)
        ledger.register(frameIndex: 42, value: 116)
        XCTAssertEqual(ledger.take(frameIndex: 42), 116)
        // A rejection removes only its own entry; the next callback keeps its start time.
        XCTAssertEqual(ledger.take(frameIndex: 41), 108)
        XCTAssertNil(ledger.take(frameIndex: 40))
        ledger.register(frameIndex: 43, value: 124)
        XCTAssertNil(ledger.take(frameIndex: 42))
        XCTAssertEqual(ledger.take(frameIndex: 43), 124)
        XCTAssertEqual(ledger.count, 0)
    }







    func testGameImportParsesNamesIDsAndFragmentLinks() throws {
        XCTAssertEqual(try GFNGameImportReference.parse(" Cubiscape 2 ").search, "Cubiscape 2")
        XCTAssertEqual(try GFNGameImportReference.parse("105389455").launchID, "105389455")
        let id = "73c174bd-ab5e-4be8-89df-e6fd59477d53"
        let reference = try GFNGameImportReference.parse("https://play.geforcenow.com/mall/#/game/\(id)?appId=105389455&store=STEAM")
        XCTAssertEqual(reference.catalogID, id)
        XCTAssertEqual(reference.launchID, "105389455")
        XCTAssertEqual(reference.store, "STEAM")
        XCTAssertThrowsError(try GFNGameImportReference.parse("https://store.steampowered.com/app/105389455"))
        XCTAssertThrowsError(try GFNGameImportReference.parse("https://play.geforcenow.com.attacker.example/game/\(id)"))
        XCTAssertThrowsError(try GFNGameImportReference.parse("https://play.geforcenow.com/"))
    }

    func testSharedGameRoundTripsTitleStoreAndGFNIdentifiers() throws {
        let game = CloudGame(id: "test", title: "A Game & Friends: + Edition", genre: "Cloud Game",
            platform: "EPIC", icon: "gamecontroller.fill", imageUrl: nil, launchAppId: "12345",
            launchOptions: [], uuid: "73c174bd-ab5e-4be8-89df-e6fd59477d53",
            summary: nil, longDescription: nil, publisher: nil, developer: nil, releaseDate: nil,
            featureLabels: nil, tags: nil, stores: nil, playType: nil, membershipTierLabel: nil,
            catalogSectionId: nil, catalogSectionTitle: nil, contentRatings: nil)
        let option = GameLaunchOption(storefront: "STEAM", appId: "105389455", supportedControls: nil)
        let url = try XCTUnwrap(GFNGameImportReference.shareURL(game: game, option: option))
        let reference = try GFNGameImportReference.parse(url.absoluteString)
        XCTAssertEqual(reference.launchID, option.appId)
        XCTAssertEqual(reference.store, option.storefront)
        XCTAssertEqual(reference.title, game.title)
        XCTAssertEqual(reference.catalogID, game.uuid)
        XCTAssertNil(reference.search)
    }

    func testHomeScreenLinkPreservesSelectedGameAndStoreWithoutCredentials() throws {
        let game = Self.makeGame(title: "A Game & Friends: + Edition", controls: [])
        let option = GameLaunchOption(storefront: "EPIC", appId: "101606111", supportedControls: nil)
        let url = try XCTUnwrap(GFNGameImportReference.homeScreenURL(game: game, option: option))
        XCTAssertEqual(url.scheme, "https")
        XCTAssertEqual(url.host, "joemossjr16.github.io")
        XCTAssertEqual(URLComponents(url: url, resolvingAgainstBaseURL: false)?.percentEncodedPath, "/ios-apps/launch/")
        let query = try XCTUnwrap(URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems)
        XCTAssertEqual(query.first { $0.name == "appid" }?.value, option.appId)
        XCTAssertEqual(query.first { $0.name == "store" }?.value, "EPIC")
        XCTAssertEqual(query.first { $0.name == "title" }?.value, game.title)
        XCTAssertEqual(Set(query.map(\.name)), Set(["appid", "title", "store"]))
        let restored = try GFNGameImportReference.parse("opennowios://launch/101606111?title=A%20Game%20%26%20Friends&store=EPIC")
        XCTAssertEqual(restored.launchID, "101606111")
        XCTAssertEqual(restored.store, "EPIC")
        XCTAssertEqual(restored.title, "A Game & Friends")
    }

    func testHomeScreenSetupRetainsGameIdentityAndPreventsImmediateLaunch() throws {
        let game = Self.makeGame(title: "Cyberpunk & Friends", controls: [])
        let option = GameLaunchOption(storefront: "STEAM", appId: "101606111", supportedControls: nil)
        let url = try XCTUnwrap(GFNGameImportReference.homeScreenSetupURL(game: game, option: option))
        let components = try XCTUnwrap(URLComponents(url: url, resolvingAgainstBaseURL: false))
        let items = try XCTUnwrap(components.queryItems)
        XCTAssertEqual(items.first { $0.name == "setup" }?.value, "1")
        XCTAssertEqual(items.first { $0.name == "appid" }?.value, option.appId)
        XCTAssertEqual(items.first { $0.name == "store" }?.value, "STEAM")
        XCTAssertEqual(items.first { $0.name == "title" }?.value, game.title)
        var saved = components
        saved.queryItems = items.filter { $0.name != "setup" }
        XCTAssertEqual(saved.url, GFNGameImportReference.homeScreenURL(game: game, option: option))
    }

    func test5080BadgeRequiresNvidiaReadyCollectionInsteadOfInstallToPlay() {
        let panels: [[String: Any]] = [["sections": [
            ["title": "Install-to-Play", "items": [["app": ["id": "unconfirmed"]]]],
            ["title": "GeForce RTX 5080 Ready", "items": [["app": ["id": "confirmed"]]]],
            ["title": "RTX ON", "items": [["app": ["id": "rtx-only"]]]]
        ]]]
        XCTAssertEqual(GFNCatalogFeatureParser.readyAppIDs(panels: panels), Set(["confirmed"]))
        XCTAssertTrue(GFNCatalogFeatureParser.readyAppIDs(panels: []).isEmpty)
    }

    func testCatalogFeatureFlagsRespectDisabledAndPerStoreCapabilities() {
        let steam: [String: Any] = ["gfn": ["features": [
            ["key": "RTX_ENABLED", "value": "true"],
            ["key": "HDR_ENABLED", "value": "false"],
            ["key": "SUPPORTED_HDR_VERSION", "values": ["HDR"]],
            ["key": "REFLEX_ENABLED", "value": true]
        ]]]
        let gog: [String: Any] = ["gfn": ["features": [
            ["key": "RTX_ENABLED", "value": "false"],
            ["key": "SUPPORTED_HDR_VERSION", "values": [String]()]
        ]]]
        XCTAssertEqual(GFNCatalogFeatureParser.labels(variant: steam), ["RTX", "HDR", "Reflex"])
        XCTAssertTrue(GFNCatalogFeatureParser.labels(variant: gog).isEmpty)
        XCTAssertTrue(GFNCatalogFeatureParser.labels(variant: [:]).isEmpty)
        XCTAssertEqual(Set(GFNCatalogFeatureParser.labels(app: ["variants": [steam, gog]])),
                       Set(["RTX", "HDR", "Reflex"]))
        XCTAssertFalse(GFNCatalogFeatureParser.labels(variant: steam).contains("RTX 5080 Ready"))
    }

    func testLaunchOptionDecodesCachedMetadataWithoutFeatureFlags() throws {
        let old = Data(#"{"storefront":"STEAM","appId":"101606111","supportedControls":["MOUSE"]}"#.utf8)
        let option = try JSONDecoder().decode(GameLaunchOption.self, from: old)
        XCTAssertNil(option.featureLabels)
        XCTAssertEqual(option.appId, "101606111")
    }

    func testHEVCOutputPreservesSourceBitDepthAndRejectsTruncatedSPS() {
        // SPS through bit_depth_chroma_minus8: one sublayer, 4:2:0, 16x16.
        func sps(depth: Int) -> Data {
            func ue(_ value: Int) -> String {
                let code = String(value + 1, radix: 2)
                return String(repeating: "0", count: code.count - 1) + code
            }
            var bits = "00000001" + String(repeating: "0", count: 96)
            bits += ue(0) + ue(1) + ue(16) + ue(16) + "0" + ue(depth - 8) + ue(depth - 8)
            while bits.count % 8 != 0 { bits += "0" }
            let chars = Array(bits)
            var result = Data([0x42, 0x01])
            var zeros = 0
            for offset in stride(from: 0, to: chars.count, by: 8) {
                let byte = UInt8(String(chars[offset..<offset + 8]), radix: 2)!
                if zeros >= 2 && byte <= 3 { result.append(3); zeros = 0 }
                result.append(byte)
                zeros = byte == 0 ? zeros + 1 : 0
            }
            return result
        }
        XCTAssertEqual(NativeStreamHEVCOutput.bitDepth(sps: sps(depth: 8)), 8)
        XCTAssertEqual(NativeStreamHEVCOutput.bitDepth(sps: sps(depth: 10)), 10)
        XCTAssertEqual(NativeStreamHEVCOutput.pixelFormat(sps: sps(depth: 8)), kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
        XCTAssertEqual(NativeStreamHEVCOutput.pixelFormat(sps: sps(depth: 10)), kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange)
        XCTAssertNil(NativeStreamHEVCOutput.bitDepth(sps: Data(sps(depth: 10).prefix(8))))
    }

    func testHUDColorModeUsesReceivedPixelsAndTransfer() throws {
        for format in [kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                       kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange] {
            var buffer: CVPixelBuffer?
            XCTAssertEqual(CVPixelBufferCreate(nil, 16, 16, format, nil, &buffer), kCVReturnSuccess)
            let pixels = try XCTUnwrap(buffer)
            let depth = format == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange ? "10-bit" : "8-bit"
            XCTAssertEqual(NativeStreamHDRTransfer.colorMode(in: pixels), "\(depth) SDR")
            for (transfer, suffix) in [(kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ, "PQ"),
                                       (kCVImageBufferTransferFunction_ITU_R_2100_HLG, "HLG")] {
                CVBufferSetAttachment(pixels, kCVImageBufferTransferFunctionKey, transfer, .shouldPropagate)
                XCTAssertEqual(NativeStreamHDRTransfer.colorMode(in: pixels), "\(depth) HDR \(suffix)")
            }
        }
    }

    func testHDRHandshakeRequestsHDRAndBT2020WithoutChangingTenBitSDR() {
        for codec in [NativeStreamVideoCodec.av1, .h265, .h264] {
            for hdr in [false, true] {
                var settings = AppSettings.default
                settings.hdrEnabled = hdr
                settings.preferredColorQuality = StreamColorQuality.tenBit420.rawValue
                let sdp = NativeStreamSDP.buildNvstSDP(
                    offerSDP: "", localAnswerSDP: "",
                    profile: StreamSettingsResolver.profile(for: settings), settings: settings, codec: codec)
                let supportsHDR = hdr && codec != .h264
                XCTAssertTrue(sdp.contains("a=video.dynamicRangeMode:\(supportsHDR ? 1 : 0)\n"))
                XCTAssertTrue(sdp.contains("a=video.encoderCscMode:\(supportsHDR ? 5 : 3)\n"))
                XCTAssertTrue(sdp.contains("a=video.bitDepth:\(codec == .h264 ? 8 : 10)\n"))
            }
        }
    }

    func testCloudMatchFeatureRequestExplicitlyRequestsHDRAndPreservesSDR() throws {
        var settings = AppSettings.default
        settings.preferredResolution = "2560x1600"
        settings.preferredFPS = 120
        settings.preferredCodec = "AV1"
        settings.preferredColorQuality = StreamColorQuality.tenBit420.rawValue
        settings.enableCloudGsync = false
        for hdr in [false, true] {
            settings.hdrEnabled = hdr
            let features = CloudMatchStreamingFeatureRequest.build(
                settings: settings, profile: StreamSettingsResolver.profile(for: settings),
                bitDepth: 10, chromaFormat: 0
            )
            XCTAssertEqual(features["bitDepth"] as? Int, 1)
            XCTAssertEqual(features["chromaFormat"] as? Int, 0)
            XCTAssertEqual(features["reflex"] as? Bool, true)
            XCTAssertEqual(features["cloudGsync"] as? Bool, false)
            XCTAssertEqual(features["trueHdr"] as? Bool, hdr)
            for field in ["mouseMovementFlags", "hidDevices", "sdrColorSpace", "hdrColorSpace"] {
                XCTAssertNil(features[field], "\(field) is absent from the desktop request")
            }
            XCTAssertNoThrow(try JSONSerialization.data(withJSONObject: features))
        }
    }

    func testCloudMatchReflexUsesDesktopThresholdWithoutCloudGsync() {
        var settings = AppSettings.default
        settings.enableCloudGsync = false
        for fps in [30, 60, 90, 120] {
            settings.preferredFPS = fps
            let features = CloudMatchStreamingFeatureRequest.build(
                settings: settings, profile: StreamSettingsResolver.profile(for: settings),
                bitDepth: 0, chromaFormat: 0
            )
            XCTAssertEqual(features["reflex"] as? Bool, fps >= 120)
        }
        settings.enableCloudGsync = true
        settings.preferredFPS = 60
        let features = CloudMatchStreamingFeatureRequest.build(
            settings: settings, profile: StreamSettingsResolver.profile(for: settings),
            bitDepth: 0, chromaFormat: 0
        )
        XCTAssertEqual(features["reflex"] as? Bool, true)
    }

    func testCloudMatchInternalRejectionExplainsThatDecoderHasNotStarted() {
        let error = NSError(domain: "OpenNOW.Session", code: 400, userInfo: [
            NSLocalizedDescriptionKey: #"{"requestStatus":{"statusDescription":"INTERNAL_ERROR_STATUS 8A8C0000","statusCode":4}}"#
        ])
        let message = OpenNOWErrorPresenter.message(for: error, fallback: "Launch failed")
        XCTAssertTrue(message.contains("8A8C0000"))
        XCTAssertTrue(message.contains("Video decoding has not started"))
    }

    func testNativeReceiverSettingMigratesOffAndRoundTrips() throws {
        let old = try JSONEncoder().encode(AppSettings.default)
        var object = try XCTUnwrap(JSONSerialization.jsonObject(with: old) as? [String: Any])
        object.removeValue(forKey: "experimentalNativeNVSTEnabled")
        let migrated = try JSONDecoder().decode(AppSettings.self, from: JSONSerialization.data(withJSONObject: object))
        XCTAssertFalse(migrated.experimentalNativeNVSTEnabled)
        var native = migrated; native.experimentalNativeNVSTEnabled = true
        XCTAssertTrue(try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(native)).experimentalNativeNVSTEnabled)
        XCTAssertNotEqual(StreamSettingsResolver.sessionSignature(for: native), StreamSettingsResolver.sessionSignature(for: migrated))
        XCTAssertEqual(native.preferredResolution, migrated.preferredResolution)
        XCTAssertEqual(native.hdrEnabled, migrated.hdrEnabled)
    }

    func testNativeEndpointsRequireAdvertisedControlAndRetainExplicitHost() {
        XCTAssertEqual(NativeStreamNVSTConfiguration.endpoints(sessionObj: ["connectionInfo": [["port": 443, "usage": 1, "resourcePath": "/nvst/"]]], fallbackHost: "old.example"), [])
        XCTAssertEqual(NativeStreamNVSTConfiguration.endpoints(sessionObj: ["connectionInfo": [["port": 322, "usage": 16, "resourcePath": "rtsps://new.example:322"]]], fallbackHost: "old.example"), ["rtsps://new.example:322"])
    }

    func testNativeProvisioningPreservesQualityAndIdentityOnLaunchAndResume() throws {
        for action: Int? in [nil, 2] {
            var body: [String: Any] = ["sessionRequestData": [
                "deviceHashId": "stable-device", "secureRTSPSupported": false,
                "clientRequestMonitorSettings": [["widthInPixels": 3840, "heightInPixels": 2160, "framesPerSecond": 120, "sdrHdrMode": 1]],
                "requestedStreamingFeatures": ["bitDepth": 1, "trueHdr": true],
                "metaData": [["key": "GSStreamerType", "value": "WebRTC"], ["key": "wssignaling", "value": "1"]]
            ]]
            if let action { body["action"] = action; body["data"] = "RESUME" }
            let original = try JSONSerialization.data(withJSONObject: NativeStreamNVSTConfiguration.requestBody(body, enabled: false), options: .sortedKeys)
            XCTAssertEqual(original, try JSONSerialization.data(withJSONObject: body, options: .sortedKeys))
            let converted = NativeStreamNVSTConfiguration.requestBody(body, enabled: true)
            let native = try XCTUnwrap(converted["sessionRequestData"] as? [String: Any])
            let before = try XCTUnwrap(body["sessionRequestData"] as? [String: Any])
            XCTAssertEqual(native["secureRTSPSupported"] as? Bool, true)
            XCTAssertEqual(native["deviceHashId"] as? String, "stable-device")
            for key in ["clientRequestMonitorSettings", "requestedStreamingFeatures"] {
                XCTAssertEqual(try JSONSerialization.data(withJSONObject: native[key]!, options: .sortedKeys), try JSONSerialization.data(withJSONObject: before[key]!, options: .sortedKeys))
            }
            XCTAssertEqual(native["metaData"] as? [[String: String]], [["key": "wssignaling", "value": "1"]])
            XCTAssertEqual(converted["action"] as? Int, action)
        }
    }

    func testNativeInputRebasesKeyboardAndGamepadTimestamps() throws {
        let encoder = NativeStreamInputEncoder()
        encoder.setProtocolVersion(4)
        let key = encoder.encodeKeyDown(mapping: .init(virtualKey: 0x41, scanCode: 0x1e), modifiers: 1)
        let outputs = try NativeStreamNVSTInput.translate(key, timestamp: 12345, sequence: 2)
        guard case .control(let command) = outputs.first else { return XCTFail("Expected keyboard command") }
        XCTAssertEqual(command.code, .remoteInput)
        // The native keyboard envelope has a LE session timestamp at its tail.
        XCTAssertEqual(Array(command.payload.suffix(8)), [0x39, 0x30, 0, 0, 0, 0, 0, 0])
        XCTAssertEqual(Array(command.payload[16..<20]), [0, 0x41, 0, 1])
        XCTAssertThrowsError(try NativeStreamNVSTInput.translate(Data([0x23, 1, 2]), timestamp: 0, sequence: 0))
        // Web client gamepad event, explicit player slot 1 and bitmap 3.
        var pad = [UInt8](repeating: 0, count: 38)
        pad[0] = 12; pad[6] = 1; pad[8] = 3; pad[12] = 0; pad[13] = 0x10
        let translated = try NativeStreamNVSTInput.translate(Data(pad), timestamp: 54321, sequence: 7)
        guard case .gamepad(let packet) = translated.first else { return XCTFail("Expected gamepad packet") }
        XCTAssertEqual(packet.gamepadIndex, 1); XCTAssertEqual(packet.connectedBitmap, 0x0303)
        XCTAssertEqual(packet.timestampMicroseconds, 54321); XCTAssertEqual(packet.buttons, 0x1000)
    }

    @available(iOS 17.0, *)
    func testNativeAV1PreservesHDRColorAndNeverRequestsEightBitOutputForTenBit() throws {
        let data = try XCTUnwrap(Data(base64Encoded: "EgAKDQAAAAM3+ObXyoSIBIIyDxAAgAAAAEsPxmwcv/+vVg=="))
        let sequence = try XCTUnwrap(NvstAv1Obu.units(in: data)?.first { $0.type == NvstAv1Obu.sequenceHeaderType })
        let header = try XCTUnwrap(NvstAv1Obu.parseSequenceHeader(data.subdata(in: sequence.payloadOffset..<(sequence.payloadOffset+sequence.payloadLength))))
        XCTAssertEqual(header.bitDepth, 10); XCTAssertEqual(header.colorPrimaries, 9)
        XCTAssertEqual(header.transferCharacteristics, 16); XCTAssertEqual(header.matrixCoefficients, 9)
        var format = NvstVideoToolboxDecoder.BitstreamFormat(); format.bitDepth = 10
        let output = NvstVideoToolboxDecoder.preferredOutputPixelFormats(for: format)
        XCTAssertFalse(output.contains(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange))
        XCTAssertFalse(output.contains(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange))
    }

    func testAV1ConfigurationParsesRealSDRAndHDRSamples() throws {
        // libaom encodes of a 128x72 flat frame; the HDR frame uses Y=600, UV=512.
        for (encoded, depth, primaries, transfer) in [
            ("EgAKDQAAAAM3+ObXyICAgIIyDxAAgAAAAEsX2PkL//rC4A==", 8, 1, 1),
            ("EgAKDQAAAAM3+ObXyoSIBIIyDxAAgAAAAEsPxmwcv/+vVg==", 10, 9, 16)
        ] {
            let data = try XCTUnwrap(Data(base64Encoded: encoded))
            let config = try XCTUnwrap(NativeStreamAV1Configuration.parse(data))
            XCTAssertEqual(config.width, 128)
            XCTAssertEqual(config.height, 72)
            XCTAssertEqual(config.bitDepth, depth)
            XCTAssertEqual(config.primaries, primaries)
            XCTAssertEqual(config.transfer, transfer)
            XCTAssertEqual(config.matrix, primaries)
            XCTAssertEqual(config.codecConfiguration.prefix(4), Data([0x81, 0, depth == 10 ? 0x4c : 0x0c, 0]))
            XCTAssertEqual(config.subsamplingX, 1)
            XCTAssertEqual(config.subsamplingY, 1)
            // Canonical configOBUs must round-trip, including a missing size field.
            var withoutSize = config.sequenceOBU
            withoutSize[0] &= ~2
            withoutSize.remove(at: 1)
            XCTAssertEqual(try NativeStreamAV1Configuration.parse(withoutSize), config)
            for length in 3..<17 {
                XCTAssertThrowsError(try NativeStreamAV1Configuration.parse(Data(data.prefix(length))))
            }
        }
        XCTAssertThrowsError(try NativeStreamAV1Configuration.parse(Data([0x8a, 1, 0])))
        XCTAssertThrowsError(try NativeStreamAV1Configuration.parse(Data([0x0a, 0x80])))
    }

    func testHDRPixelBufferBridgePreservesTenBitValuesAndColorAttachments() throws {
        var buffer: CVPixelBuffer?
        XCTAssertEqual(CVPixelBufferCreate(kCFAllocatorDefault, 16, 8,
            kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
            [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary, &buffer), kCVReturnSuccess)
        let source = try XCTUnwrap(buffer)
        CVPixelBufferLockBaseAddress(source, [])
        let pixels = try XCTUnwrap(CVPixelBufferGetBaseAddressOfPlane(source, 0)).assumingMemoryBound(to: UInt16.self)
        pixels[0] = 601 << 6
        CVPixelBufferUnlockBaseAddress(source, [])
        CVBufferSetAttachment(source, kCVImageBufferColorPrimariesKey,
            kCVImageBufferColorPrimaries_ITU_R_2020, .shouldPropagate)
        CVBufferSetAttachment(source, kCVImageBufferTransferFunctionKey,
            kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ, .shouldPropagate)
        let frame = RTCVideoFrame(buffer: RTCCVPixelBuffer(pixelBuffer: source), rotation: ._0, timeStampNs: 0)
        let output = try XCTUnwrap(NativeStreamFramePixelBufferBridge().pixelBuffer(for: frame))
        XCTAssertTrue(output === source)
        XCTAssertEqual(CVPixelBufferGetPixelFormatType(output), kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange)
        XCTAssertEqual(NativeStreamHDRTransfer.detect(in: output), .pq)
        CVPixelBufferLockBaseAddress(output, .readOnly)
        XCTAssertEqual(try XCTUnwrap(CVPixelBufferGetBaseAddressOfPlane(output, 0)).assumingMemoryBound(to: UInt16.self)[0] >> 6, 601)
        CVPixelBufferUnlockBaseAddress(output, .readOnly)
    }

    func testCloudMatchColorReplyRecognizesTenBitEnumsAndSDRDowngrade() {
        XCTAssertEqual(CloudMatchStreamingFeatureRequest.normalizedColorQuality(bitDepth: 1, chromaFormat: 0), .tenBit420)
        XCTAssertEqual(CloudMatchStreamingFeatureRequest.normalizedColorQuality(bitDepth: 1, chromaFormat: 1), .tenBit444)
        XCTAssertEqual(CloudMatchStreamingFeatureRequest.normalizedColorQuality(bitDepth: 0, chromaFormat: 0), .eightBit420)
        XCTAssertEqual(CloudMatchStreamingFeatureRequest.normalizedColorQuality(bitDepth: 0, chromaFormat: 1), .eightBit444)
        XCTAssertNil(CloudMatchStreamingFeatureRequest.normalizedColorQuality(bitDepth: nil, chromaFormat: nil))
    }

    func testCloudMatchUsesEnumsForTenBitAnd444WhileSDPUsesLiteralDepth() {
        var settings = AppSettings.default
        for color in StreamColorQuality.allCases {
            settings.preferredColorQuality = color.rawValue
            let features = CloudMatchStreamingFeatureRequest.build(settings: settings,
                profile: StreamSettingsResolver.profile(for: settings),
                bitDepth: color.bitDepth, chromaFormat: color.chromaFormat)
            XCTAssertEqual(features["bitDepth"] as? Int, color == .tenBit420 || color == .tenBit444 ? 1 : 0)
            XCTAssertEqual(features["chromaFormat"] as? Int, color == .eightBit444 || color == .tenBit444 ? 1 : 0)
        }
        settings.preferredColorQuality = StreamColorQuality.eightBit420.rawValue
        settings.hdrEnabled = true
        let sdp = NativeStreamSDP.buildNvstSDP(offerSDP: "", localAnswerSDP: "",
            profile: StreamSettingsResolver.profile(for: settings), settings: settings, codec: .av1)
        XCTAssertTrue(sdp.contains("a=video.bitDepth:10"))
    }

    func testAV1HeaderScannerSkipsLargeFramePayloadAndHandlesSlicedData() throws {
        let sequence = try XCTUnwrap(Data(base64Encoded: "EgAKDQAAAAM3+ObXyoSIBIIyDxAAgAAAAEsPxmwcv/+vVg=="))
        let expected = try XCTUnwrap(NativeStreamAV1Configuration.parse(sequence))
        // A sized frame OBU containing 512 KiB of compressed bytes must be skipped,
        // not interpreted as configuration. Follow it with a genuine sequence header.
        var bytes = Data([0x32, 0x80, 0x80, 0x20])
        bytes.append(Data(repeating: 0xff, count: 512 * 1024))
        XCTAssertNil(try NativeStreamAV1Configuration.parse(bytes))
        bytes.append(sequence)
        XCTAssertEqual(try NativeStreamAV1Configuration.parse(bytes), expected)
        var padded = Data([0xff, 0xff])
        padded.append(bytes)
        XCTAssertEqual(try NativeStreamAV1Configuration.parse(padded.dropFirst(2)), expected)
    }

    func testAsyncDecodeAdmissionBoundsWorkAndReturnsEachSlotOnce() throws {
        let admission = NativeStreamDecodeAdmission(maximumInFlight: 2)
        let first = try XCTUnwrap(admission.acquire(timeout: .now()))
        let second = try XCTUnwrap(admission.acquire(timeout: .now()))
        XCTAssertNil(admission.acquire(timeout: .now()), "Cannot accumulate a hardware decode backlog")
        first.complete()
        first.complete() // An error return and callback must not release two slots.
        let third = try XCTUnwrap(admission.acquire(timeout: .now()))
        XCTAssertNil(admission.acquire(timeout: .now()))
        second.complete()
        third.complete()
        var cancelled = try XCTUnwrap(admission.acquire(timeout: .now())) as NativeStreamDecodeAdmission.Permit?
        cancelled = nil // A cancelled callback returns admission when released.
        XCTAssertNil(cancelled)
        let recovered = try XCTUnwrap(admission.acquire(timeout: .now()))
        let other = try XCTUnwrap(admission.acquire(timeout: .now()))
        XCTAssertNil(admission.acquire(timeout: .now()))
        recovered.complete()
        other.complete()
    }

    func testRendererDropsBacklogAndResumesWithNewestFrame() throws {
        let mailbox = NativeStreamLatestFrameMailbox<Int>()
        mailbox.offer(1)
        XCTAssertEqual(try XCTUnwrap(mailbox.take()).frame, 1)
        mailbox.offer(2)
        XCTAssertEqual(try XCTUnwrap(mailbox.take()).frame, 2)
        // Simulate a stalled GPU while decoded video continues arriving.
        for frame in 3...240 { mailbox.offer(frame) }
        XCTAssertNil(mailbox.take(), "GPU submissions stay bounded")
        mailbox.complete()
        XCTAssertEqual(try XCTUnwrap(mailbox.take()).frame, 240, "Resume at the latest frame, without replaying stale video")
        mailbox.complete()
        mailbox.complete()
        XCTAssertNil(mailbox.take(), "A display tick must not resubmit the same frame")
        mailbox.offer(241)
        XCTAssertEqual(try XCTUnwrap(mailbox.take()).frame, 241)
        mailbox.complete()
    }

    func testSoftwareI420BridgeCopiesPaddedPlanesAndInterleavesChroma() throws {
        let bridge = NativeStreamFramePixelBufferBridge()
        for (width, height) in [(6, 4), (8, 6)] {
            let source = RTCMutableI420Buffer(width: Int32(width), height: Int32(height), strideY: Int32(width + 4), strideU: Int32(width / 2 + 3), strideV: Int32(width / 2 + 5))
            for row in 0..<height {
                for col in 0..<width { source.mutableDataY[row * Int(source.strideY) + col] = UInt8(32 + row * width + col) }
            }
            for row in 0..<(height / 2) {
                for col in 0..<(width / 2) {
                    source.mutableDataU[row * Int(source.strideU) + col] = UInt8(70 + row * width / 2 + col)
                    source.mutableDataV[row * Int(source.strideV) + col] = UInt8(150 + row * width / 2 + col)
                }
            }
            let frame = RTCVideoFrame(buffer: source, rotation: ._0, timeStampNs: 123)
            let output = try XCTUnwrap(bridge.pixelBuffer(for: frame))
            XCTAssertEqual(CVPixelBufferGetWidth(output), width)
            XCTAssertEqual(CVPixelBufferGetHeight(output), height)
            XCTAssertEqual(CVPixelBufferGetPixelFormatType(output), kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
            CVPixelBufferLockBaseAddress(output, .readOnly)
            let y = try XCTUnwrap(CVPixelBufferGetBaseAddressOfPlane(output, 0)).assumingMemoryBound(to: UInt8.self)
            let uv = try XCTUnwrap(CVPixelBufferGetBaseAddressOfPlane(output, 1)).assumingMemoryBound(to: UInt8.self)
            for row in 0..<height {
                for col in 0..<width { XCTAssertEqual(y[row * CVPixelBufferGetBytesPerRowOfPlane(output, 0) + col], UInt8(32 + row * width + col)) }
            }
            for row in 0..<(height / 2) {
                for col in 0..<(width / 2) {
                    let offset = row * CVPixelBufferGetBytesPerRowOfPlane(output, 1) + col * 2
                    XCTAssertEqual(uv[offset], UInt8(70 + row * width / 2 + col))
                    XCTAssertEqual(uv[offset + 1], UInt8(150 + row * width / 2 + col))
                }
            }
            CVPixelBufferUnlockBaseAddress(output, .readOnly)
            let nativeFrame = RTCVideoFrame(buffer: RTCCVPixelBuffer(pixelBuffer: output), rotation: ._0, timeStampNs: 123)
            XCTAssertTrue(try XCTUnwrap(bridge.pixelBuffer(for: nativeFrame)) === output)
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

    func testFixedFPSRequestDisablesHostDynamicFrameControlWithoutChangingHDR() {
        for fps in [60, 120, 240] {
            for codec in [NativeStreamVideoCodec.av1, .h265] {
                var settings = AppSettings.default
                settings.preferredFPS = fps
                settings.preferredResolution = "3840x2160"
                settings.hdrEnabled = true
                settings.preferredColorQuality = StreamColorQuality.tenBit420.rawValue
                let profile = StreamVideoProfile(width: 3840, height: 2160, fps: fps, maxBitrateKbps: 100_000)
                let offer = "a=vqos.resControl.enable:1\na=vqos.resControl.dfc.adjustResAndFps:1\na=vqos.resControl.dfc.maxResLevels:5\n"
                let sdp = NativeStreamSDP.buildNvstSDP(offerSDP: offer, localAnswerSDP: "",
                    profile: profile, settings: settings, codec: codec)
                XCTAssertTrue(sdp.contains("a=vqos.dfc.enable:0\n"))
                XCTAssertFalse(sdp.contains("a=vqos.dfc.enable:1\n"))
                for key in ["vqos.resControl.enable", "vqos.resControl.dfc.adjustResAndFps", "vqos.resControl.dfc.maxResLevels"] {
                    XCTAssertTrue(sdp.contains("a=\(key):0\n"))
                    XCTAssertEqual(sdp.components(separatedBy: "a=\(key):").count - 1, 1)
                }
                XCTAssertFalse(sdp.contains("a=vqos.dfc.decodeFpsAdjPercent:"))
                XCTAssertTrue(sdp.contains("a=video.maxFPS:\(fps)\n"))
                XCTAssertTrue(sdp.contains("a=video.bitDepth:10\n"))
                XCTAssertTrue(sdp.contains("a=video.dynamicRangeMode:1\n"))
                XCTAssertTrue(sdp.contains("a=video.clientViewportWd:3840\n"))
            }
        }
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

        // This input preference helper does not decide the allocation identity. The production
        // StreamDeviceProfile resolver separately protects native 4:4:4's desktop color profile.
        for mode in [NativeTouchMode.automatic, .always] {
            XCTAssertEqual(
                NativeTouchSupport.appLaunchMode(mode: mode, game: touchGame),
                .touchFriendly,
                "mode \(mode)"
            )
        }
    }

    func testNative444DefaultsToDesktopWithoutChangingColorOrTouchPreferences() throws {
        let game = Self.makeGame(title: "Touch Game", controls: ["TOUCHSCREEN"])
        var settings = AppSettings.default
        settings.experimentalNativeNVSTEnabled = true
        settings.preferredCodec = "H265"
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.hdrEnabled = true
        for mode in [NativeTouchMode.automatic, .always] {
            settings.touch.nativeTouchMode = mode
            let device = StreamDeviceProfile.resolve(game: game, settings: settings, keyboardMouseConnected: false)
            XCTAssertEqual(device, .desktop)
            XCTAssertEqual(device.nvDeviceOS, "WINDOWS")
            XCTAssertEqual(device.nvDeviceType, "DESKTOP")
            XCTAssertEqual(device.appLaunchMode, .gamepadFriendly)
            XCTAssertEqual(settings.touch.nativeTouchMode, mode)
            let color = StreamSettingsResolver.colorQuality(for: settings)
            let features = CloudMatchStreamingFeatureRequest.build(settings: settings,
                profile: StreamSettingsResolver.profile(for: settings), bitDepth: color.bitDepth, chromaFormat: color.chromaFormat)
            XCTAssertEqual(features["bitDepth"] as? Int, 1)
            XCTAssertEqual(features["chromaFormat"] as? Int, 1)
            XCTAssertEqual(features["trueHdr"] as? Bool, true)
        }
        // An old digitizer marker cannot silently reintroduce the Android color downgrade.
        XCTAssertEqual(StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: false, touchProvisionedOverride: true), .desktop)
    }

    func testExperimental444TouchKeepsWindowsColorIdentityAndRespectsInputChoice() {
        let touchGame = Self.makeGame(title: "Touch Game", controls: ["TOUCHSCREEN"])
        let desktopGame = Self.makeGame(title: "Desktop Game", controls: ["KEYBOARD_MOUSE"])
        var settings = AppSettings.default
        settings.experimentalNativeNVSTEnabled = true
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.hdrEnabled = true
        settings.experimentalDesktop444TouchEnabled = true
        settings.touch.nativeTouchMode = .always
        let device = StreamDeviceProfile.resolve(game: desktopGame, settings: settings, keyboardMouseConnected: false)
        XCTAssertEqual(device, .desktopTouch)
        XCTAssertEqual(device.nvDeviceOS, "WINDOWS")
        XCTAssertEqual(device.nvDeviceType, "DESKTOP")
        XCTAssertEqual(device.userAgent, StreamDeviceProfile.desktop.userAgent)
        XCTAssertEqual(device.clientPlatformName, "windows")
        XCTAssertEqual(device.clientIdentification, "GFN-PC")
        XCTAssertEqual(device.appLaunchMode.rawValue, 3)
        XCTAssertEqual(device.remoteControllersBitmap, 0)
        XCTAssertEqual(device.availableSupportedControllers, [])
        XCTAssertEqual(StreamSettingsResolver.colorQuality(for: settings), .tenBit444)
        XCTAssertTrue(settings.hdrEnabled)
        XCTAssertEqual(StreamDeviceProfile.resolve(game: desktopGame, settings: settings, keyboardMouseConnected: true), .desktopTouch)
        settings.touch.nativeTouchMode = .never
        XCTAssertEqual(StreamDeviceProfile.resolve(game: touchGame, settings: settings, keyboardMouseConnected: false), .desktop)
        settings.touch.nativeTouchMode = .automatic
        XCTAssertEqual(StreamDeviceProfile.resolve(game: desktopGame, settings: settings, keyboardMouseConnected: false), .desktop)
        XCTAssertEqual(StreamDeviceProfile.resolve(game: touchGame, settings: settings, keyboardMouseConnected: false), .desktopTouch)
        XCTAssertEqual(StreamDeviceProfile.resolve(game: touchGame, settings: settings, keyboardMouseConnected: true), .desktop)
    }

    func testNativeTouchClaimRetainsInputProfileAnd420Behavior() {
        let game = Self.makeGame(title: "Desktop Game", controls: ["KEYBOARD_MOUSE"])
        var settings = AppSettings.default
        settings.experimentalNativeNVSTEnabled = true
        settings.experimentalDesktop444TouchEnabled = true
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.touch.nativeTouchMode = .always
        // Claiming keeps the allocation's input envelope even after keyboard/mouse hot-plug.
        XCTAssertEqual(StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: true, touchProvisionedOverride: true), .desktopTouch)
        XCTAssertEqual(StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: false, touchProvisionedOverride: false), .desktop)
        settings.preferredColorQuality = StreamColorQuality.tenBit420.rawValue
        XCTAssertEqual(StreamDeviceProfile.resolve(game: game, settings: settings, keyboardMouseConnected: false), .touch)
        settings.experimentalDesktop444TouchEnabled = false
        let touch = StreamDeviceProfile.resolve(game: game, settings: settings, keyboardMouseConnected: false)
        XCTAssertEqual(touch, .touch)
        XCTAssertEqual(touch.nvDeviceOS, "ANDROID")
        XCTAssertEqual(touch.appLaunchMode, .touchFriendly)
        XCTAssertEqual(StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: true, touchProvisionedOverride: true), .touch)
        XCTAssertEqual(StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: false, touchProvisionedOverride: false), .desktop)
    }

    func testDesktop444TouchMigrationAndSignatureRequireFreshAllocation() throws {
        let legacy = try JSONDecoder().decode(AppSettings.self, from: Data("{}".utf8))
        XCTAssertFalse(legacy.experimentalDesktop444TouchEnabled)
        var settings = AppSettings.default
        settings.experimentalNativeNVSTEnabled = true
        settings.preferredColorQuality = StreamColorQuality.tenBit444.rawValue
        settings.touch.nativeTouchMode = .always
        let desktop = StreamSettingsResolver.sessionSignature(for: settings)
        XCTAssertTrue(desktop.contains("provisioning=desktop-444-v3"))
        settings.experimentalDesktop444TouchEnabled = true
        let touch = StreamSettingsResolver.sessionSignature(for: settings)
        XCTAssertNotEqual(desktop, touch)
        let restored = try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(settings))
        XCTAssertTrue(restored.experimentalDesktop444TouchEnabled)
        XCTAssertEqual(restored.touch.nativeTouchMode, settings.touch.nativeTouchMode)
        XCTAssertEqual(restored.preferredColorQuality, settings.preferredColorQuality)
        XCTAssertEqual(StreamSettingsResolver.sessionSignature(for: restored), touch)
        settings.touch.nativeTouchMode = .never
        XCTAssertNotEqual(StreamSettingsResolver.sessionSignature(for: settings), touch)
        settings.preferredColorQuality = StreamColorQuality.tenBit420.rawValue
        let old420 = StreamSettingsResolver.sessionSignature(for: settings)
        settings.experimentalDesktop444TouchEnabled = false
        XCTAssertEqual(StreamSettingsResolver.sessionSignature(for: settings), old420)
    }

    func testAlwaysTouchProvisioningAndInitialRouteOverridePhysicalInput() {
        let game = Self.makeGame(title: "Touch Game", controls: ["TOUCHSCREEN"])
        XCTAssertTrue(NativeTouchSupport.shouldProvisionNativeTouch(mode: .always, game: game,
            keyboardMouseConnected: true))
        XCTAssertTrue(NativeTouchSupport.shouldStartWithNativeTouch(mode: .always, game: game,
            keyboardMouseConnected: true, provisioned: true, preferVirtualController: false))
        XCTAssertFalse(NativeTouchSupport.shouldStartWithNativeTouch(mode: .always, game: game,
            keyboardMouseConnected: true, provisioned: false, preferVirtualController: false))
        XCTAssertFalse(NativeTouchSupport.shouldStartWithNativeTouch(mode: .always, game: game,
            keyboardMouseConnected: false, provisioned: true, preferVirtualController: true))
        XCTAssertFalse(NativeTouchSupport.shouldStartWithNativeTouch(mode: .automatic, game: game,
            keyboardMouseConnected: true, provisioned: true, preferVirtualController: false))
        XCTAssertTrue(NativeTouchSupport.shouldStartWithNativeTouch(mode: .automatic, game: game,
            keyboardMouseConnected: false, provisioned: true, preferVirtualController: false))
        XCTAssertFalse(NativeTouchSupport.shouldStartWithNativeTouch(mode: .never, game: game,
            keyboardMouseConnected: false, provisioned: true, preferVirtualController: false))
        var settings = AppSettings.default
        settings.touch.nativeTouchMode = .always
        XCTAssertEqual(StreamDeviceProfile.resolve(game: game, settings: settings,
            keyboardMouseConnected: true), .touch)
    }

    func testNVSTTouchTranslationPreservesContactsAndUsesSessionClock() throws {
        for version in [2, 3, 4] {
            let encoder = NativeStreamInputEncoder()
            encoder.setProtocolVersion(version)
            let source = try XCTUnwrap(encoder.encodeTouchBatch([
                NativeTouchRecord(slot: 0, phase: NativeTouchPhase.down, x: 0x1234, y: 0x5678,
                    radiusX: 7, radiusY: 9, timestampUs: 99),
                NativeTouchRecord(slot: 1, phase: NativeTouchPhase.up, x: 0xFFFF, y: 0, timestampUs: 100)
            ]))
            let stamp: UInt64 = 0x0102_0304_0506_0708
            let translated = try NativeStreamNVSTInput.translate(source, timestamp: stamp, sequence: 9)
            XCTAssertEqual(translated.count, 1)
            guard case .touch(let command, let count) = translated.first else {
                return XCTFail("Expected one reliable native touch command")
            }
            XCTAssertEqual(count, 2)
            XCTAssertEqual(command.code, .remoteInput)
            let body = Data([0, 40, 0, 2,
                0, 1, 0x12, 0x34, 0x56, 0x78, 7, 9, 1, 2, 3, 4, 5, 6, 7, 8,
                1, 2, 0xFF, 0xFF, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8])
            let expected = NvstRemoteInput.framed(NvstRemoteInput.packet(type: .lowLevelTouch, body: body),
                framing: .enveloped, sequence: 9, timestampMicroseconds: stamp)
            XCTAssertEqual(command.payload, expected, "Input protocol \(version)")
            XCTAssertThrowsError(try NativeStreamNVSTInput.translate(Data(source.dropLast()), timestamp: stamp, sequence: 10))
        }
    }

    func testExperimental444TouchPreservesEveryDesktopIdentityField() {
        let touch = StreamDeviceProfile.desktopTouch
        let desktop = StreamDeviceProfile.desktop
        XCTAssertEqual(touch.nvDeviceOS, desktop.nvDeviceOS)
        XCTAssertEqual(touch.nvDeviceType, desktop.nvDeviceType)
        XCTAssertEqual(touch.nvDeviceMake, desktop.nvDeviceMake)
        XCTAssertEqual(touch.nvDeviceModel, desktop.nvDeviceModel)
        XCTAssertEqual(touch.userAgent, desktop.userAgent)
        XCTAssertEqual(touch.clientPlatformName, desktop.clientPlatformName)
        XCTAssertEqual(touch.clientIdentification, desktop.clientIdentification)
        XCTAssertEqual(touch.persistsInGameSettings, desktop.persistsInGameSettings)
        XCTAssertEqual(touch.appLaunchMode, .touchFriendly)
        XCTAssertEqual(desktop.appLaunchMode, .gamepadFriendly)
        XCTAssertEqual(StreamDeviceProfile.touch.nvDeviceType, "TABLET")
        XCTAssertEqual(StreamDeviceProfile.touch.nvDeviceOS, "ANDROID")
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
        sectionTitle: String? = nil,
        options: [GameLaunchOption]? = nil
    ) -> CloudGame {
        CloudGame(
            id: title.lowercased(),
            title: title,
            genre: "Action",
            platform: "STEAM",
            icon: "",
            imageUrl: nil,
            launchAppId: "1",
            launchOptions: options ?? [GameLaunchOption(storefront: "STEAM", appId: "1", supportedControls: controls)],
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

    func testQueuePollingUsesValidatedControlHostRatherThanMediaHost() {
        let game = Self.makeGame(title: "Queue Test", controls: [])
        var session = ActiveSession(
            id: "session", game: game, startedAt: .now, status: 1, queuePosition: 9,
            seatSetupStep: nil, serverIp: "media.example.invalid", mediaIp: nil, mediaPort: 0,
            signalingServer: nil, signalingUrl: nil, iceServers: [], zone: "NP-PDX-01",
            streamingBaseUrl: "https://np-pdx-01.cloudmatchbeta.nvidiagrid.net",
            clientId: "client", deviceId: "device", adState: nil
        )
        session.sessionControlBaseUrl = SessionControlRouting.baseURL(
            host: "np-ams-01.cloudmatchbeta.nvidiagrid.net", port: 443)
        XCTAssertEqual(SessionControlRouting.pollBase(for: session),
            "https://np-ams-01.cloudmatchbeta.nvidiagrid.net")
        session.sessionControlBaseUrl = nil
        XCTAssertEqual(SessionControlRouting.pollBase(for: session), session.streamingBaseUrl)
        XCTAssertNil(SessionControlRouting.baseURL(host: "media.example.invalid", port: 443))
        XCTAssertNil(SessionControlRouting.baseURL(host: "np-ams-01.cloudmatchbeta.nvidiagrid.net", port: 8443))
        XCTAssertNil(SessionControlRouting.baseURL(host: "np-ams-01.cloudmatchbeta.nvidiagrid.net.attacker.test", port: 443))
    }

    private func queueFixture() -> ActiveSession {
        ActiveSession(id: "queue", game: Self.makeGame(title: "Queue Test", controls: []),
            startedAt: .now, status: 1, queuePosition: 1, seatSetupStep: nil,
            serverIp: nil, mediaIp: nil, mediaPort: 0, signalingServer: nil,
            signalingUrl: nil, iceServers: [], zone: "NP-PDX-01",
            streamingBaseUrl: "https://us-west.cloudmatchbeta.nvidiagrid.net",
            clientId: "client", deviceId: "device", adState: nil)
    }

    func testPartialPollRetainsAssignedControlRouteThroughReadyTransition() {
        var previous = queueFixture()
        previous.sessionControlBaseUrl = "https://np-pdx-01.cloudmatchbeta.nvidiagrid.net"
        var next = previous
        next.sessionControlBaseUrl = nil
        for status in [1, 2, 3] {
            next.status = status
            let merged = mergeQueueSessionState(previous: previous, next: next)
            XCTAssertEqual(SessionControlRouting.pollBase(for: merged), previous.sessionControlBaseUrl)
        }
        next.sessionControlBaseUrl = "https://np-ams-01.cloudmatchbeta.nvidiagrid.net"
        XCTAssertEqual(mergeQueueSessionState(previous: previous, next: next).sessionControlBaseUrl,
                       next.sessionControlBaseUrl)
    }

    func testReadyAllocationHydratesFromRigWithoutRedirectingQueuedPolls() {
        var session = queueFixture()
        session.serverIp = "203.0.113.24"
        XCTAssertNil(SessionControlRouting.readyDetailsBase(for: session))
        session.status = 2
        XCTAssertEqual(SessionControlRouting.readyDetailsBase(for: session), "https://203.0.113.24")
        session.status = 3
        XCTAssertEqual(SessionControlRouting.readyDetailsBase(for: session), "https://203.0.113.24")
        session.serverIp = "np-pdx-01.cloudmatchbeta.nvidiagrid.net"
        XCTAssertNil(SessionControlRouting.readyDetailsBase(for: session))
    }

    func testNextInQueueAndMissingSetupMetadataDoNotStartSetupTimeout() {
        var session = queueFixture()
        XCTAssertTrue(QueueSessionPhase.isQueued(session))
        XCTAssertFalse(QueueSessionPhase.isSettingUp(session))
        session.queuePosition = nil
        XCTAssertFalse(QueueSessionPhase.isSettingUp(session))
        session.seatSetupStep = 1
        XCTAssertTrue(QueueSessionPhase.isQueued(session))
        XCTAssertFalse(QueueSessionPhase.isSettingUp(session))
        session.seatSetupStep = 3
        XCTAssertTrue(QueueSessionPhase.isSettingUp(session))
        session.status = 2
        XCTAssertFalse(QueueSessionPhase.isSettingUp(session))
    }

    func testLibraryQueryUsesOwnedVariantFilterAndRegistryMissDetectionIsSpecific() throws {
        let encoded = try JSONSerialization.data(withJSONObject: CatalogQueryPolicy.libraryFilter, options: .sortedKeys)
        XCTAssertEqual(String(decoding: encoded, as: UTF8.self),
                       #"{"variants":{"gfn":{"library":{"status":{"notEquals":"NOT_OWNED"}}}}}"#)
        XCTAssertTrue(CatalogQueryPolicy.isRegistryMiss(#"{"errors":[{"message":"PersistedQueryNotFound"}]}"#))
        XCTAssertTrue(CatalogQueryPolicy.isRegistryMiss("PERSISTED_QUERY_NOT_FOUND"))
        XCTAssertFalse(CatalogQueryPolicy.isRegistryMiss("Unauthorized"))
    }

    func testAbandonedQueueIsTerminalEvenWhenProviderReturnsHTTP503() {
        XCTAssertTrue(CloudMatchQueueStatus.isAbandoned(["statusCode": 69]))
        XCTAssertTrue(CloudMatchQueueStatus.isAbandoned(["statusDescription": "SESSION_REQUEST_IN_QUEUE_ABANDONED"]))
        XCTAssertTrue(CloudMatchQueueStatus.isAbandoned(["unifiedErrorCode": "4A8C300F"]))
        XCTAssertFalse(CloudMatchQueueStatus.isAbandoned(["statusCode": 1]))
    }

    func testSavedBrowserTokensDecodeWithoutDeviceClientId() throws {
        let json = Data(#"{"accessToken":"access","expiresAt":123,"clientToken":null,"clientTokenExpiresAt":null,"idToken":null,"refreshToken":null}"#.utf8)
        let tokens = try JSONDecoder().decode(AuthTokens.self, from: json)
        XCTAssertNil(tokens.authClientId)
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
