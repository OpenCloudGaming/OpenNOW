import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "ConsoleLaunch"
    width: 960
    height: 540
    visible: true
    when: windowShown

    Component {
        id: launchComponent
        ConsoleLaunchAnimation {
            width: testCase.width
            height: testCase.height
            reducedMotion: true
            soundEnabled: false
        }
    }

    SignalSpy { id: finishedSpy; signalName: "finished" }
    SignalSpy { id: coverSpy; signalName: "coverReached" }

    function makeLaunch(properties) {
        const launch = createTemporaryObject(launchComponent, testCase, properties || {})
        verify(launch !== null)
        finishedSpy.target = launch
        coverSpy.target = launch
        return launch
    }

    function cleanup() {
        finishedSpy.target = null
        coverSpy.target = null
        finishedSpy.clear()
        coverSpy.clear()
    }

    function verifyIdle(launch) {
        compare(launch.active, false)
        compare(launch.destinationScale, 1)
        compare(launch.chromeProgress, 1)
        compare(launch.focusHeld, false)
    }

    function test_idleDoesNotTransformDestination() {
        verifyIdle(makeLaunch())
    }

    function test_clockWaitsForFirstPresentation() {
        const launch = makeLaunch({destinationReady: true, presentationReady: false})
        launch.start("cold")
        wait(1100)
        compare(launch.active, true)
        compare(finishedSpy.count, 0)
        launch.presentationReady = true
        tryCompare(launch, "active", false, 2000)
        compare(finishedSpy.count, 1)
    }

    function test_mutedLaunchDoesNotLoadOrPlaySound() {
        const launch = makeLaunch({destinationReady: true})
        launch.start("cold")
        const air = findChild(launch, "launchAirSound")
        const opening = findChild(launch, "launchOpenSound")
        verify(air !== null)
        verify(opening !== null)
        compare(String(air.source), "")
        compare(String(opening.source), "")
        tryCompare(launch, "active", false, 2000)
        compare(air.playing, false)
        compare(opening.playing, false)
    }

    function test_coldWaitsForRenderableDestination() {
        const launch = makeLaunch({destinationReady: false})
        launch.start("cold")
        compare(launch.active, true)
        tryCompare(coverSpy, "count", 1)
        wait(1100)
        compare(launch.active, true)
        compare(finishedSpy.count, 0)
        launch.destinationReady = true
        tryCompare(launch, "active", false, 2000)
        compare(finishedSpy.count, 1)
        verifyIdle(launch)
    }

    function test_quickReducedMotionCompletes() {
        const launch = makeLaunch({destinationReady: true})
        launch.start("quick")
        tryCompare(launch, "active", false, 1500)
        compare(coverSpy.count, 1)
        compare(finishedSpy.count, 1)
        verifyIdle(launch)
    }

    function test_repeatedStartDoesNotRestartRunningIntro() {
        const launch = makeLaunch({destinationReady: false})
        launch.start("cold")
        launch.start("quick")
        compare(launch.variant, "cold")
        compare(launch.active, true)
        launch.cancel()
        verifyIdle(launch)
    }

    function test_cancelRestoresDestinationAndAllowsNextEntry() {
        const launch = makeLaunch({destinationReady: false})
        launch.start("cold")
        wait(100)
        launch.cancel()
        verifyIdle(launch)
        launch.destinationReady = true
        launch.start("quick")
        tryCompare(launch, "active", false, 1500)
        verifyIdle(launch)
    }

    function test_skipCannotRevealAnUnreadyDestination() {
        const launch = makeLaunch({destinationReady: false})
        launch.start("cold")
        wait(100)
        launch.skip()
        wait(600)
        compare(launch.active, true)
        compare(finishedSpy.count, 0)
        launch.destinationReady = true
        tryCompare(launch, "active", false, 1500)
        compare(finishedSpy.count, 1)
        verifyIdle(launch)
    }

    function test_skipFinishesReadyDestinationOnce() {
        const launch = makeLaunch({destinationReady: true})
        launch.start("cold")
        wait(100)
        launch.skip()
        launch.skip()
        tryCompare(launch, "active", false, 1500)
        compare(finishedSpy.count, 1)
        verifyIdle(launch)
    }

    function test_resizingDuringRevealDoesNotLeaveTransform() {
        const launch = makeLaunch({destinationReady: true, reducedMotion: false})
        launch.start("cold")
        wait(900)
        launch.width = 1280
        launch.height = 800
        wait(100)
        launch.width = 2100
        launch.height = 900
        tryCompare(launch, "active", false, 2000)
        compare(finishedSpy.count, 1)
        verifyIdle(launch)
    }
}
