import Foundation
import XCTest

#if canImport(OpenNOWiOS)
@testable import OpenNOWiOS
#endif

final class SessionLifecycleTests: XCTestCase {
    func testGameExitAndIntendedTerminationAreCleanEnds() {
        for result: UInt32 in [0x8003_0000, 0x8003_0014] {
            XCTAssertEqual(SessionLifecyclePolicy.nativeTerminationDisposition(termination(result)), .ended)
        }
    }

    func testPauseAndRecoverableDisconnectsAreNotAllocationEnds() {
        XCTAssertEqual(SessionLifecyclePolicy.nativeTerminationDisposition(termination(0x8003_000e)), .paused)
        for result: UInt32 in [0x8003_0001, 0x8003_0002, 0x8003_0004, 0x8003_0005,
                               0x8003_001e, 0x8003_0023, 0x8004_0002, 0x8012_0001, 0xffff_ffff] {
            XCTAssertEqual(SessionLifecyclePolicy.nativeTerminationDisposition(termination(result)), .disconnected)
        }
    }

    func testExplicitHostErrorsAreNotCleanQuits() {
        for result: UInt32 in [0x8003_0003, 0x8003_000b, 0x8003_000c, 0x8003_000d,
                               0x8003_000f, 0x8003_0013, 0x8003_001a, 0x8003_001b, 0x8003_0020] {
            XCTAssertEqual(SessionLifecyclePolicy.nativeTerminationDisposition(termination(result)), .failed)
        }
    }

    func testTerminationReasonUsesNetworkOrderAndRejectsTruncation() {
        let command = NvstControlCommand(code: .termination, payload: Data([0x80, 0x03, 0x00, 0x14]))
        let parsed = NvstSeatTermination.parse(command)
        XCTAssertEqual(parsed?.result, 0x8003_0014)
        XCTAssertEqual(parsed?.payload, command.payload)
        XCTAssertNil(NvstSeatTermination.parse(NvstControlCommand(code: .termination, payload: Data([0x80, 0x03, 0x00]))))
        XCTAssertNil(NvstSeatTermination.parse(NvstControlCommand(code: .pingBackAck, payload: command.payload)))
    }

    func testProviderChecksOnlyTreatAuthoritativeResponsesAsEnded() {
        for status in [4, 5, 7, 8] {
            XCTAssertThrowsError(try SessionLifecyclePolicy.sessionObject(in: [
                "requestStatus": ["statusCode": 1], "session": ["status": status]
            ], httpStatus: 200)) { error in
                XCTAssertTrue(SessionLifecyclePolicy.isEnded(error))
            }
        }
        XCTAssertNoThrow(try SessionLifecyclePolicy.sessionObject(in: [
            "requestStatus": ["statusCode": 1], "session": ["status": 6]
        ], httpStatus: 200))
        for status in [401, 403, 429, 500, 503] {
            XCTAssertThrowsError(try SessionLifecyclePolicy.sessionObject(in: [:], httpStatus: status)) { error in
                XCTAssertFalse(SessionLifecyclePolicy.isEnded(error))
            }
        }
        for status in [404, 410] {
            XCTAssertThrowsError(try SessionLifecyclePolicy.sessionObject(in: [:], httpStatus: status)) { error in
                XCTAssertTrue(SessionLifecyclePolicy.isEnded(error))
            }
        }
        XCTAssertFalse(SessionLifecyclePolicy.isEnded(URLError(.networkConnectionLost)))
        XCTAssertFalse(SessionLifecyclePolicy.isEnded(CancellationError()))
    }

    private func termination(_ result: UInt32) -> NvstSeatTermination {
        NvstSeatTermination(result: result, payload: Data())
    }

    #if canImport(OpenNOWiOS)
    @MainActor
    func testCleanNativeEndClearsAllLocalSurfacesAndIgnoresDuplicateOrStaleEvents() async {
        await withStore { store in
            store.installDebugQueuePreview(position: 8)
            store.streamSession = store.activeSession
            let id = store.activeSession!.id
            UserDefaults.standard.set(try? JSONEncoder().encode(store.activeSession), forKey: "OpenNOW.iOS.activeSession")
            UserDefaults.standard.set(try? JSONEncoder().encode(store.activeStreamSettings), forKey: "OpenNOW.iOS.activeStreamSettings")
            store.handleNativeStreamTermination(id: "old-stream", event: self.termination(0x8003_0014))
            XCTAssertEqual(store.activeSession?.id, id)
            XCTAssertNil(store.lastError)
            store.handleNativeStreamTermination(id: id, event: self.termination(0x8003_0014))
            XCTAssertNil(store.activeSession)
            XCTAssertNil(store.streamSession)
            XCTAssertNil(store.activeStreamSettings)
            XCTAssertFalse(store.showStreamLoading)
            XCTAssertFalse(store.queueOverlayVisible)
            XCTAssertFalse(store.canReopenStreamer)
            XCTAssertTrue(store.resumableSessions.isEmpty)
            XCTAssertNil(store.lastError)
            XCTAssertNil(store.sessionError)
            XCTAssertNil(UserDefaults.standard.data(forKey: "OpenNOW.iOS.activeSession"))
            XCTAssertNil(UserDefaults.standard.data(forKey: "OpenNOW.iOS.activeStreamSettings"))
            store.handleNativeStreamTermination(id: id, event: self.termination(0x8003_000f))
            XCTAssertNil(store.lastError)
            await store.resumeSession(candidate: self.candidate(id))
            XCTAssertNil(store.lastError)
        }
    }

    @MainActor
    func testPauseAndUnknownDisconnectRetainAllocationAndSettings() async {
        for result: UInt32 in [0x8003_000e, 0x8003_001e, 0x8004_0002, 0xffff_ffff] {
            await withStore { store in
                store.installDebugQueuePreview(position: 8)
                store.streamSession = store.activeSession
                let allocation = store.activeSession
                let settings = store.activeStreamSettings
                store.handleNativeStreamTermination(id: allocation!.id, event: self.termination(result))
                XCTAssertEqual(store.activeSession, allocation)
                XCTAssertEqual(store.activeStreamSettings, settings)
                XCTAssertNil(store.streamSession)
                XCTAssertEqual(store.sessionAvailability, .checking)
                XCTAssertTrue(store.resumableSessions.isEmpty)
                XCTAssertTrue(store.canJumpBackToSession)
                XCTAssertNotNil(UserDefaults.standard.data(forKey: "OpenNOW.iOS.activeSession"))
                if result == 0x8003_000e { XCTAssertNil(store.lastError) }
                else { XCTAssertNotNil(store.lastError) }
            }
        }
    }

    @MainActor
    func testHostFailureClearsEndedAllocationButPreservesError() async {
        await withStore { store in
            store.installDebugQueuePreview(position: 8)
            store.streamSession = store.activeSession
            store.handleNativeStreamTermination(id: store.activeSession!.id, event: self.termination(0x8003_000f))
            XCTAssertNil(store.activeSession)
            XCTAssertNil(store.streamSession)
            XCTAssertTrue(store.sessionError?.contains("FRAME_GRAB_FAILED") == true)
        }
    }

    @MainActor
    func testCallbacksFromDismissedStreamCannotEndRetainedOrReplacementAllocation() async {
        await withStore { store in
            store.installDebugQueuePreview(position: 8)
            let retained = store.activeSession
            store.streamSession = nil
            store.handleNativeStreamTermination(id: retained!.id, event: self.termination(0x8003_0014))
            XCTAssertEqual(store.activeSession, retained)
            store.streamSession = retained
            store.handleNativeStreamTermination(id: "old-allocation", event: self.termination(0x8003_0014))
            XCTAssertEqual(store.activeSession, retained)
            XCTAssertEqual(store.streamSession, retained)
            let ended = await store.reconcileStreamFailure(id: "old-allocation")
            XCTAssertFalse(ended)
            XCTAssertEqual(store.activeSession, retained)
        }
    }

    private func candidate(_ id: String) -> RemoteSessionCandidate {
        RemoteSessionCandidate(id: id, appId: "1", status: 3, serverIp: nil,
                               streamSettingsSignature: nil, resolution: nil, fps: nil)
    }

    @MainActor
    private func withStore(_ body: @MainActor (OpenNOWStore) async -> Void) async {
        let defaults = UserDefaults.standard
        let keys = ["OpenNOW.iOS.settings", "OpenNOW.iOS.activeSession", "OpenNOW.iOS.activeStreamSettings"]
        let saved = keys.map { defaults.data(forKey: $0) }
        defer {
            for (key, value) in zip(keys, saved) {
                if let value { defaults.set(value, forKey: key) }
                else { defaults.removeObject(forKey: key) }
            }
        }
        let store = OpenNOWStore()
        await body(store)
        if let id = store.activeSession?.id { store.handleEndedSession(id: id, message: nil) }
    }
    #endif
}
