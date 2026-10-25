import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property int key: 0
    property int modifiers: 0
    property int step: 0
    property Item parent: null
    property Item video: null
    property string sessionId: "stream-controllers-fixture"

    readonly property var xboxOne: ({instanceId: 11, slot: 1, name: "Xbox Wireless Controller", family: "xbox",
        powerState: "onBattery", batteryPercent: 72, charging: false,
        profileKey: "guid:030000005e040000130b000001050000", hasSerial: false})
    readonly property var dualSense: ({instanceId: 12, slot: 2, name: "DualSense Wireless Controller", family: "playstation",
        powerState: "charging", batteryPercent: 40, charging: true,
        profileKey: "serial:054c:0ce6:a1b2c3d4e5f6", hasSerial: true})
    readonly property var xboxTwin: ({instanceId: 13, slot: 3, name: "Xbox Wireless Controller", family: "xbox",
        powerState: "onBattery", batteryPercent: 41, charging: false,
        profileKey: "guid:030000005e040000130b000001050000", hasSerial: false})
    readonly property var genericPad: ({instanceId: 14, slot: 4, name: "8BitDo Pro 2", family: "generic",
        powerState: "noBattery", batteryPercent: -1, charging: false,
        profileKey: "guid:03000000c82d00000660000000010000", hasSerial: false})

    property QtObject settingsClient: QtObject {
        property int serial: 0
        function request(method, params, timeout) {
            return method === "settings.set" ? "stream-controllers-setting-" + (++serial) : ""
        }
        function cancel(id) {}
    }

    property QtObject input: QtObject {
        property var flightDevices: []
        property var supportedFlightDevices: []
        property bool forwardFlightControls: true
        property var basePads: []
        property var controllerProfiles: ({})
        property int leftStickDeadzone: 5
        property int rightStickDeadzone: 5
        property int vibrationIntensity: 100
        function tuned(pad) {
            const profile = controllerProfiles[pad.profileKey] || {}
            return Object.assign({}, pad, {
                leftStickDeadzone: profile.leftStickDeadzone ?? leftStickDeadzone,
                rightStickDeadzone: profile.rightStickDeadzone ?? rightStickDeadzone,
                vibrationIntensity: profile.vibrationIntensity ?? vibrationIntensity
            })
        }
        readonly property var availableControllers: basePads.map(pad => tuned(pad))
        property int inputControllerId: 0
        readonly property var controllers: inputControllerId === 0 ? availableControllers
            : availableControllers.filter(controller => controller.instanceId === inputControllerId)
        readonly property int controllerCount: controllers.length
        property bool shellCaptureEnabled: true
        property bool inputSuspended: false
        property bool backgroundControllerEnabled: false
        readonly property bool backgroundControllerActive: backgroundControllerEnabled && inputControllerId !== 0
        property bool shellInputBlocked: false
        readonly property bool shellInputDraining: false
        property bool leftStickNavigationEnabled: true
        property var samples: ({})
        property int rumbles: 0
        function stickSample(instanceId) { return samples[instanceId] || ({}) }
        function testRumble(instanceId) { rumbles += 1; return true }
        signal controllerActivity()
        signal shellInputSkipRequested()
        signal controllerActivityDetailed(string device, string control, int value)
    }

    function check(ok, message) { if (!ok) throw new Error("Stream controllers: " + message) }
    function find(item, name) {
        if (!item) return null
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function need(name) {
        const item = find(parent, name)
        check(item !== null, name + " exists")
        return item
    }
    function argument(name, fallback) {
        const args = Qt.application.arguments
        const index = args.indexOf(name)
        return index >= 0 && index + 1 < args.length ? args[index + 1] : fallback
    }
    function profiles() { return ShellStore.settings.controllerProfiles || ({}) }
    function pad(id) { return input.availableControllers.find(entry => entry.instanceId === id) }
    function streamAlive() {
        return find(parent, "streamSurfaceHost") === video && ShellStore.streamState === "streaming"
            && ShellStore.activeSession && ShellStore.activeSession.sessionId === sessionId
    }
    function press(value, keyModifiers) {
        key = value
        modifiers = keyModifiers || 0
    }

    function run(root) {
        parent = root
        ShellStore.streamerStartRequestId = sessionId
        ShellStore.streamInputPauseRequestId = sessionId
        ShellStore.settings = Object.assign({}, ShellStore.settings, {controllerProfiles: {},
            controllerLeftStickDeadzone: 5, controllerRightStickDeadzone: 5, controllerVibrationIntensity: 100})
        const owner = ShellStore.settingsOwnerState
        owner.confirmedSettings = Object.assign({}, owner.settings)
        owner.coreClient = settingsClient
        owner.ready = true
        ShellStore.selectedGame = {title: "Cyberpunk 2077", imageUrl: ""}
        ShellStore.activeSession = {sessionId: sessionId, phase: "ready", status: 2,
            streamProfile: {height: 1440, codec: "av1"}}
        ShellStore.streamer = {status: "streaming", sessionId: sessionId, fps: 120, bitrateMbps: 48,
            transport: "nvst", codec: "av1", mediaBackend: "vaapi"}
        ShellStore.streamState = "streaming"
        ShellStore.streamStartedAtMs = Date.now() - 5047000
        input.basePads = [xboxOne, dualSense]
        video = find(parent, "streamSurfaceHost")
        check(video !== null, "the desktop stream item is mounted")
        AppController.showOverlay("desktop-stream-menu")
        if (argument("--controller-scene", "") !== "") {
            AppController.showOverlay("")
            step = steps.length - 1
        }
        return true
    }

    readonly property var steps: [
        function() {
            const drawer = need("streamMenuDrawer")
            check(drawer.visible && drawer.x >= 0 && drawer.x < 40, "the drawer docks on the left edge")
            check(drawer.x + drawer.width <= parent.width * 0.45, "the game stays visible on the right")
            check(need("streamMenuScrim").width < parent.width, "the scrim only darkens the drawer side")
            check(need("streamMenuEdgeSeam").x > drawer.width - 4, "the edge seam sits on the inner edge")
            check(need("streamMenuEdgeShadow").x >= drawer.x + drawer.width - 8, "the shadow falls onto the game")
            for (const name of ["streamMenuConnection", "streamMicrophoneControl", "streamAntiAfkControl",
                                "streamStatsControl", "streamFullscreenControl", "streamControllersRow",
                                "streamInviteRow", "streamConsoleModeRow", "streamEndSessionRow"])
                check(need(name).visible, name + " is shown")
            check(need("streamMenuBackToGame").activeFocus, "Back to game starts focused")
            check(input.shellCaptureEnabled, "the open drawer keeps controller input away from the game")
            check(streamAlive(), "opening the drawer keeps the stream and the video item")
            const saved = ShellStore.streamer
            ShellStore.streamer = {status: "streaming", sessionId: sessionId, transport: "nvst",
                codec: "automatic", mediaBackend: "pending"}
            const card = need("streamMenuConnection")
            const found = []
            function walk(item) {
                if (!item) return
                if (item.text) found.push(item)
                for (const child of item.children || []) walk(child)
            }
            walk(card)
            const metrics = found.filter(item => item.text === qsTr("Measuring"))
            const meta = found.find(item => String(item.text).indexOf("NVST") >= 0)
            check(metrics.length >= 1 && meta, "the measuring card is missing its values")
            function box(item) {
                const point = item.mapToItem(card, 0, 0)
                return {x: point.x, y: point.y, r: point.x + item.width, b: point.y + item.height}
            }
            const metaBox = box(meta)
            for (const metric of metrics) {
                const metricBox = box(metric)
                check(metricBox.r <= metaBox.x + 1 || metricBox.b <= metaBox.y + 1 || metaBox.b <= metricBox.y + 1,
                    "a measuring value overlaps the transport line")
            }
            ShellStore.streamer = saved
        },
        function() {
            const window = parent.Window.window
            if (window.height !== 540) {
                window.width = 960
                window.height = 540
                need("streamMenuBackToGame").forceActiveFocus()
                return false
            }
            const end = need("streamEndSessionRow")
            const scroll = need("streamMenuMainScroll")
            if (endBeforeY < 0) {
                check(scroll.contentY === 0, "the short drawer was already scrolled")
                const before = end.mapToItem(scroll, 0, 0)
                endBeforeY = before.y
                check(before.y >= scroll.height,
                    "End session already fits in the short drawer y=" + before.y + " view=" + scroll.height)
            }
            if (!end.activeFocus) {
                check(shortTabs < 12, "Tab never reached End session on the short drawer")
                shortTabs += 1
                press(Qt.Key_Tab)
                return false
            }
            check(scroll.contentY > 0, "Tab did not scroll the short drawer, contentY=" + scroll.contentY)
            const local = end.mapToItem(scroll, 0, 0)
            check(local.y >= -1 && local.y + end.height <= scroll.height + 1,
                "End session is outside the scroll viewport y=" + local.y + " h=" + end.height + " view=" + scroll.height)
            check(streamAlive(), "scrolling to End session keeps the stream")
            window.width = 1600
            window.height = 900
            need("streamMenuBackToGame").forceActiveFocus()
            press(Qt.Key_Down)
        },
        function() {
            check(!need("streamMicrophoneControl").enabled, "the fixture has no microphone")
            check(need("streamAntiAfkControl").activeFocus, "Down skips the disabled microphone to Anti-AFK")
            press(Qt.Key_Return)
        },
        function() {
            check(ShellStore.antiAfkEnabled && need("streamAntiAfkControl").checked, "Enter flips Anti-AFK on")
            check(AppController.overlay === "desktop-stream-menu", "quick toggles keep the drawer open")
            press(Qt.Key_Return)
        },
        function() {
            check(!ShellStore.antiAfkEnabled, "Enter flips Anti-AFK off again")
            press(Qt.Key_Tab)
        },
        function() {
            check(need("streamStatsControl").activeFocus, "Tab walks the quick toggles in order")
            press(Qt.Key_Right)
        },
        function() {
            check(need("streamFullscreenControl").activeFocus, "Right moves across the toggle grid")
            press(Qt.Key_Down)
        },
        function() {
            const row = need("streamControllersRow")
            check(row.activeFocus, "Down leaves the grid for Controllers")
            check(row.subtitle === qsTr("%1 connected · dead zones and vibration").arg(2), "the row counts connected pads")
            press(Qt.Key_Return)
        },
        function() {
            const page = need("streamControllersPage")
            if (!page.active || !need("controllerPadTabs").activeFocus) return false
            check(!input.leftStickNavigationEnabled, "the left stick stops moving focus on the controller page")
            check(need("controllerTab-1").visible && need("controllerTab-2").visible, "both pads get a tab")
            check(page.selectedInstanceId === xboxOne.instanceId, "the page opens on P1")
            check(need("controllerProfileBadge").visible && !page.custom, "an untouched pad uses the defaults")
            press(Qt.Key_Right)
        },
        function() {
            check(need("streamControllersPage").selectedInstanceId === dualSense.instanceId, "Right on the tabs picks the next pad")
            press(Qt.Key_Tab)
        },
        function() {
            check(need("controllerLeftDeadzone").control.activeFocus, "Tab reaches the left dead zone slider")
            press(Qt.Key_Right)
        },
        function() {
            const saved = profiles()[dualSense.profileKey]
            if (!saved || saved.leftStickDeadzone !== 6) return false
            check(saved.rightStickDeadzone === undefined && saved.vibrationIntensity === undefined,
                "only the edited field becomes custom")
            check(saved.name === dualSense.name && saved.lastSeen > 0, "the profile records the pad name and time")
            check(pad(dualSense.instanceId).leftStickDeadzone === 6, "the new dead zone reaches ControllerInput for that pad")
            check(pad(xboxOne.instanceId).leftStickDeadzone === 5, "the other pad keeps the global dead zone")
            check(need("streamControllersPage").custom, "the badge switches to custom")
            press(Qt.Key_Escape)
        },
        function() {
            check(!need("streamControllersPage").active && need("streamControllersRow").activeFocus,
                "Esc steps back to the main page on the Controllers row")
            check(input.leftStickNavigationEnabled, "leaving the page gives the left stick back to navigation")
            check(streamAlive(), "pushing and popping the page keeps the stream")
            press(Qt.Key_Q, Qt.ControlModifier | Qt.ShiftModifier)
        },
        function() {
            if (AppController.overlay !== "desktop-stream-exit-confirm") return false
            check(streamAlive(), "Ctrl+Shift+Q only asks; the stream keeps running")
            AppController.showOverlay("desktop-stream-menu")
            return true
        },
        function() {
            if (!need("streamMenuBackToGame").activeFocus) return false
            press(Qt.Key_Escape)
        },
        function() {
            if (AppController.overlay !== "") return false
            check(streamAlive(), "closing the drawer keeps the stream and the video item")
            return true
        },
        function() {
            AppController.showOverlay("desktop-stream-menu")
            need("streamControllersRow").clicked()
            const page = need("streamControllersPage")
            check(page.active, "clicking Controllers opens the page")
            need("controllerTab-2")
            page.selectedInstanceId = dualSense.instanceId
            input.basePads = [xboxOne]
            check(page.selectedInstanceId === xboxOne.instanceId, "a dropped pad hands the page to the next pad")
            check(need("controllerDisconnectedNotice").visible
                && need("controllerDisconnectedNotice").title === qsTr("%1 disconnected").arg(dualSense.name),
                "a pad disconnecting mid-edit is named calmly")
            check(profiles()[dualSense.profileKey].leftStickDeadzone === 6, "its saved values stay")
            input.basePads = [xboxOne, dualSense]
            check(!need("controllerDisconnectedNotice").visible, "the notice clears when the pad returns")
            input.basePads = []
            check(need("noControllerState").visible, "no pads shows the calm empty state")
            check(need("controllerDefaultLeftDeadzone").visible, "the defaults stay editable without a pad")
            check(!need("controllerPadTabs").visible, "no tabs without pads")
            need("controllerDefaultVibration").committed(70)
            check(ShellStore.settings.controllerVibrationIntensity === 70, "the empty state edits the global default")
            need("controllerDefaultVibration").committed(100)
            input.basePads = [xboxOne, dualSense, xboxTwin, genericPad]
            page.selectedInstanceId = xboxTwin.instanceId
            check(need("controllerSharedProfileNotice").visible, "identical pads without a serial say they share one profile")
            check(page.sharedWith.length === 1 && page.sharedWith[0].instanceId === xboxOne.instanceId,
                "P3 shares with P1")
            page.selectedInstanceId = dualSense.instanceId
            check(!need("controllerSharedProfileNotice").visible, "a pad with a serial has its own profile")
            need("controllerResetPad").clicked()
            check(profiles()[dualSense.profileKey] === undefined, "Reset removes the pad's own values")
            check(pad(dualSense.instanceId).leftStickDeadzone === 5, "a reset pad follows the global value again")
            page.selectedInstanceId = xboxOne.instanceId
            need("controllerVibration").committed(30)
            check(pad(xboxTwin.instanceId).vibrationIntensity === 30, "a change on P1 also reaches its identical twin")
            need("controllerApplyAll").clicked()
            for (const entry of input.availableControllers)
                check(entry.vibrationIntensity === 30, "Apply to all copies the values to " + entry.name)
            need("controllerTestVibration").clicked()
            check(input.rumbles === 1, "Test rumbles the selected pad")
            need("controllerPlayerOne").clicked()
            check(input.inputControllerId === xboxOne.instanceId, "the Player 1 switch selects this pad only")
            need("controllerPlayerOne").clicked()
            check(input.inputControllerId === 0, "the Player 1 switch releases the other pads")
            page.selectedInstanceId = dualSense.instanceId
            need("controllerPlayerOne").clicked()
            check(input.inputControllerId === dualSense.instanceId, "Player 1 locks to the DualSense")
            input.basePads = [xboxOne, xboxTwin, genericPad]
            check(need("controllerPlayerOne").visible && need("controllerPlayerOne").playerOne
                && need("controllerPlayerOne").text === qsTr("Player 1 pad disconnected"),
                "a lock on a disconnected pad stays visible instead of reading off")
            const before = JSON.stringify(profiles())
            page.writePad(null, {vibrationIntensity: 40})
            check(JSON.stringify(profiles()) === before, "a commit after the selected pad dropped writes nothing")
            need("controllerPlayerOne").clicked()
            check(input.inputControllerId === 0, "turning the stale lock off releases the remaining pads")
            check(need("controllerPlayerOne").text === qsTr("Only this pad is Player 1"), "the row returns to normal")
            check(streamAlive(), "edge states never touch the stream")
            ShellStore.settings = Object.assign({}, ShellStore.settings, {controllerProfiles: {}})
            AppController.showOverlay("")
            return true
        },
        function() {
            if (!fixture.sceneReady && AppController.overlay !== "") return false
            return fixture.scene()
        }
    ]

    property int shortTabs: 0
    property real endBeforeY: -1
    property bool sceneReady: false
    property int sceneTicks: 0
    function scene() {
        const name = argument("--controller-scene", "")
        if (name === "") return true
        if (sceneReady) {
            applySceneInput(name)
            return ++sceneTicks >= 12
        }
        const padCount = Number(argument("--controller-pads", "2"))
        input.basePads = [xboxOne, dualSense, xboxTwin, genericPad].slice(0, padCount)
        input.samples = {
            [xboxOne.instanceId]: {leftX: 0.05, leftY: -0.03, rightX: 0.62, rightY: -0.33,
                leftOutX: 0, leftOutY: 0, rightOutX: 0.61, rightOutY: -0.32},
            [dualSense.instanceId]: {leftX: -0.42, leftY: 0.28, rightX: 0.02, rightY: 0.01,
                leftOutX: -0.39, leftOutY: 0.25, rightOutX: 0, rightOutY: 0},
            [xboxTwin.instanceId]: {leftX: -0.2, leftY: 0.5, rightX: 0.03, rightY: -0.02,
                leftOutX: -0.14, leftOutY: 0.44, rightOutX: 0, rightOutY: 0}
        }
        ShellStore.settings = Object.assign({}, ShellStore.settings, {controllerProfiles: {
            [xboxOne.profileKey]: {name: xboxOne.name, leftStickDeadzone: 8, vibrationIntensity: 70, lastSeen: 1}
        }})
        parent.Window.window.requestActivate()
        AppController.showOverlay("desktop-stream-menu")
        const page = need("streamControllersPage")
        if (name.indexOf("controllers") === 0 || name === "disconnect" || name === "four") {
            need("streamControllersRow").clicked()
            if (name === "four") page.selectedInstanceId = xboxTwin.instanceId
            if (name === "disconnect") {
                page.selectedInstanceId = dualSense.instanceId
                input.basePads = [xboxOne]
            }
        }
        applySceneInput(name)
        sceneReady = true
        return false
    }
    function applySceneInput(name) {
        AppController.inputMode = name.endsWith("gamepad") ? "controller"
            : name.endsWith("keyboard") ? "keyboard" : "pointer"
        if (name === "main-keyboard") need("streamAntiAfkControl").forceActiveFocus()
        if (name === "controllers-keyboard") need("controllerLeftDeadzone").control.forceActiveFocus()
    }

    function advance() {
        if (step >= steps.length) return 1
        const result = steps[step]()
        if (result !== false) step += 1
        return step >= steps.length ? 1 : 0
    }
}
