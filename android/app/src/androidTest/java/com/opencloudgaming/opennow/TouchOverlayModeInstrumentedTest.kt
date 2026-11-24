package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import android.view.KeyEvent
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.io.File

class TouchOverlayModeInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun freshInstallRendersGamepadAndKeyboardRequiresExplicitSelection() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
        var touch by mutableStateOf(loadAndroidSettings(null, playStoreRelease = false).androidTouch)
        compose.setContent {
            val client = remember {
                WebRtcRuntime.ensureInitialized(compose.activity)
                NativeStreamClient(compose.activity, lowLatencyGameAudio = false, onState = {}, onError = {})
            }
            DisposableEffect(client) { onDispose { client.release() } }
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                Box(Modifier.fillMaxSize().background(Color.Black)) {
                    TouchOverlay(
                        client = client,
                        mouseOutput = remember(client) { NativeTouchMouseOutput(client) },
                        touch = touch,
                        onButtonTone = {},
                        layoutEditing = false,
                        onSaveAllOffsets = {},
                        onButtonAppearanceChange = { _, _ -> },
                    )
                }
            }
        }

        fun assertGamepad() {
            listOf("A", "B", "X", "Y", "LS", "RS").forEach {
                compose.onNodeWithText(it, substring = false).assertIsDisplayed()
            }
            compose.onNodeWithText("WASD").assertDoesNotExist()
            compose.onNodeWithText("LMB").assertDoesNotExist()
        }
        assertGamepad()
        val screenshot = compose.onRoot().captureToImage().asAndroidBitmap()
        val output = File(compose.activity.getExternalFilesDir(null), "default-gamepad-overlay.png")
        output.outputStream().use { screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }

        compose.runOnIdle { touch = touch.copy(keyboardModeEnabled = true) }
        compose.onNodeWithText("WASD").assertIsDisplayed()
        compose.onNodeWithText("LMB").assertIsDisplayed()
        compose.onNodeWithText("A", substring = false).assertDoesNotExist()

        compose.runOnIdle { touch = touch.copy(keyboardModeEnabled = false) }
        assertGamepad()
    }

    @Test fun mouseButtonDragsWhileWasdIsHeldByAnotherFinger() {
        val input = RecordingInput()
        showKeyboardOverlay(input)
        val button = buttonCenter("LMB")
        val wasdLabel = compose.onNodeWithText("WASD").fetchSemanticsNode().boundsInRoot

        compose.onRoot().performTouchInput {
            val stickCenter = Offset(wasdLabel.center.x, wasdLabel.bottom + 64.dp.toPx())
            down(0, stickCenter)
            moveTo(0, stickCenter - Offset(0f, 35.dp.toPx()))
            down(1, button)
            val drag = button - Offset(80f, 10f)
            moveTo(1, drag)
            moveTo(1, drag - Offset(3f, 2f), delayMillis = 1)
            up(1)
            up(0)
        }

        compose.runOnIdle {
            assertEquals(
                listOf("W:true", "mouse1:true", "move:-160,-20", "move:-6,-4", "mouse1:false", "W:false"),
                input.calls,
            )
        }
    }

    @Test fun cancellingAMouseButtonDragReleasesItAndAllowsAnotherPress() {
        val input = RecordingInput()
        showKeyboardOverlay(input)
        val button = buttonCenter("RMB")

        compose.onRoot().performTouchInput {
            down(button)
            moveTo(button - Offset(10f, 5f))
            moveTo(button - Offset(12f, 7f), delayMillis = 1)
            cancel()
            click(button)
        }

        compose.runOnIdle {
            assertEquals(
                listOf("mouse3:true", "move:-20,-10", "mouse3:false", "mouse3:true", "mouse3:false"),
                input.calls,
            )
        }
    }

    private fun buttonCenter(label: String): Offset =
        compose.onNodeWithText(label).fetchSemanticsNode().boundsInRoot.center

    private fun showKeyboardOverlay(input: RecordingInput) {
        compose.setContent {
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                TouchOverlay(
                    client = input,
                    mouseOutput = input,
                    touch = AndroidTouchSettings(keyboardModeEnabled = true),
                    onButtonTone = {},
                    layoutEditing = false,
                    onSaveAllOffsets = {},
                    onButtonAppearanceChange = { _, _ -> },
                )
            }
        }
    }

    private class RecordingInput : TouchOverlayInput, TouchMouseOutput {
        override val settings = StreamSettings(touchMouseSensitivity = 2f)
        val calls = mutableListOf<String>()
        private val mouseButtons = MouseButtonPressSources()

        override fun setVirtualKeyboardKeyFromSource(keyCode: Int, sourceId: String, pressed: Boolean) {
            calls += "${KeyEvent.keyCodeToString(keyCode).removePrefix("KEYCODE_")}:$pressed"
        }

        override fun setVirtualMouseButtonFromSource(button: Int, sourceId: String, pressed: Boolean): Boolean =
            mouseButtons.update(button, sourceId, pressed) { down ->
                calls += "mouse$button:$down"
                true
            }

        override fun sendRawMouseMove(dx: Int, dy: Int): Boolean {
            calls += "move:$dx,$dy"
            return true
        }

        override fun setVirtualButton(buttonMask: Int, pressed: Boolean) = Unit
        override fun setVirtualTrigger(left: Boolean, pressed: Boolean) = Unit
        override fun setVirtualLeftStick(x: Float, y: Float) = Unit
        override fun setVirtualRightStick(x: Float, y: Float) = Unit
        override fun setVirtualButtonFromSource(buttonMask: Int, sourceId: String, pressed: Boolean) = Unit
        override fun setVirtualTriggerFromSource(left: Boolean, sourceId: String, pressed: Boolean) = Unit
        override fun setVirtualLeftStickFromSource(sourceId: String, x: Float, y: Float) = Unit
        override fun setTouchMouseButton(pressed: Boolean): Boolean = error("Unexpected touchpad click")
        override fun setDirectClickMouseButton(pressed: Boolean): Boolean = error("Unexpected direct click")
        override fun sendDirectClickMousePosition(position: AbsoluteMousePosition): Boolean = error("Unexpected absolute motion")
        override fun sendDirectClickMouseClick(position: AbsoluteMousePosition) = error("Unexpected direct click")
        override fun sendTouchMouseClick() = error("Unexpected touchpad click")
        override fun sendTouchMouseRightClick() = error("Unexpected touchpad right click")
        override fun sendTouchMouseWheel(delta: Int) = error("Unexpected scroll")
    }
}
