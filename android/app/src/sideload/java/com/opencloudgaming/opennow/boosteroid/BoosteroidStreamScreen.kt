package com.opencloudgaming.opennow.boosteroid

import android.view.KeyEvent
import android.view.MotionEvent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.opencloudgaming.opennow.AndroidTouchSettings
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.StreamKeyboardBar
import com.opencloudgaming.opennow.StreamRuntimeStats
import com.opencloudgaming.opennow.TouchMouseState
import com.opencloudgaming.opennow.TouchOverlay
import com.opencloudgaming.opennow.rememberPhysicalControllerConnected
import com.opencloudgaming.opennow.streamResolutionPixels

@Composable
internal fun BoosteroidStreamScreen(
    launch: BoosteroidLaunch,
    session: BoosteroidSession,
    tv: Boolean,
    model: BoosteroidViewModel,
    onInputChanged: (BoosteroidStreamClient?, Boolean) -> Unit,
) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val controllerConnected = rememberPhysicalControllerConnected(enabled = true)
    var menuOpen by remember(session) { mutableStateOf(false) }
    var keyboardOpen by remember(session) { mutableStateOf(false) }
    var keyboardText by remember(session) { mutableStateOf(TextFieldValue()) }
    var touchControls by remember(session, controllerConnected) { mutableStateOf(!tv && !controllerConnected) }
    var muted by remember(session) { mutableStateOf(false) }
    var firstFrame by remember(session) { mutableStateOf(false) }
    var cursor by remember(session) { mutableStateOf<StreamRemoteCursorState?>(null) }
    var stats by remember(session) { mutableStateOf(StreamRuntimeStats()) }
    val client = remember(session) {
        boosteroidResult {
            BoosteroidStreamClient(
                context = context,
                onState = {},
                onError = { model.streamError(session) },
                onSafeVideoFallbackRequired = { model.recover(session, fallback = true) },
                onSessionRecoveryRequired = { model.recover(session, fallback = false) },
                onStats = { stats = it },
                onRemoteCursor = { cursor = it },
                onFirstFrame = { firstFrame = true },
            )
        }.getOrNull()
    }
    val touchMouse = remember(client) { TouchMouseState() }
    val menuVisible = menuOpen || keyboardOpen
    BackHandler {
        when {
            keyboardOpen -> keyboardOpen = false
            menuOpen -> menuOpen = false
            else -> menuOpen = true
        }
    }
    LaunchedEffect(client, session) {
        if (client == null) model.streamError(session)
        else client.start(session, launch.settings)
    }
    SideEffect {
        onInputChanged(client, menuVisible)
        client?.setVirtualControllerVisible(touchControls && firstFrame && !menuVisible)
        if (menuVisible) touchMouse.reset(client)
    }
    DisposableEffect(client, lifecycle) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_PAUSE) touchMouse.reset(client)
        }
        lifecycle.addObserver(observer)
        onDispose {
            lifecycle.removeObserver(observer)
            touchMouse.reset(client)
            onInputChanged(null, false)
            client?.release()
        }
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (client != null) {
            val remoteResolution = stats.resolution?.let { resolution ->
                val pieces = resolution.split('x', limit = 2)
                val width = pieces.getOrNull(0)?.toIntOrNull()?.takeIf { it > 0 }
                val height = pieces.getOrNull(1)?.toIntOrNull()?.takeIf { it > 0 }
                if (width != null && height != null) width to height else null
            } ?: streamResolutionPixels(launch.settings)
            val aspect = remoteResolution.first.toFloat() / remoteResolution.second
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { client.createRenderer(it) },
                update = { renderer ->
                    renderer.setOnTouchListener { view, event ->
                        if (menuVisible) {
                            false
                        } else if (event.isFromSource(android.view.InputDevice.SOURCE_MOUSE)) {
                            client.dispatchMotion(event)
                        } else {
                            touchMouse.handle(
                                event = event,
                                enabled = firstFrame,
                                client = client,
                                ignoredPointerIds = emptySet(),
                                width = view.width,
                                height = view.height,
                                renderingAspectRatio = aspect,
                            )
                        }
                    }
                },
                onRelease = client::releaseRenderer,
            )
            cursor?.takeIf { firstFrame && it.absolute && it.visible && !menuVisible }?.let {
                BoosteroidCursor(it, aspect)
            }
            if (firstFrame && touchControls && !menuVisible) {
                TouchOverlay(
                    client = client,
                    touch = AndroidTouchSettings(),
                    onButtonTone = {},
                    layoutEditing = false,
                    onSaveAllOffsets = {},
                    onButtonAppearanceChange = { _, _ -> },
                )
            }
            if (keyboardOpen) {
                StreamKeyboardBar(
                    value = keyboardText,
                    onValueChange = { keyboardText = it },
                    clearConfirmationEnabled = false,
                    onClear = { keyboardText = TextFieldValue() },
                    onDisableClearConfirmation = {},
                    onEnter = {
                        client.submitText(keyboardText.text)
                        keyboardText = TextFieldValue()
                    },
                    onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                    onDone = { keyboardOpen = false },
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
        }
        if (!firstFrame && !menuVisible) {
            Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                CircularProgressIndicator(Modifier.size(32.dp))
                Text(
                    stringResource(R.string.boosteroid_connecting),
                    color = Color.White,
                    modifier = Modifier.padding(10.dp),
                )
                TextButton(onClick = model::stop) { Text(stringResource(R.string.action_cancel)) }
            }
        }
        if (firstFrame && !menuVisible && !tv) {
            TextButton(
                onClick = { menuOpen = true },
                modifier = Modifier.align(Alignment.TopEnd).windowInsetsPadding(WindowInsets.safeDrawing),
            ) {
                Text(stringResource(R.string.stream_panel_title), color = Color.White)
            }
        }
        if (menuOpen && !keyboardOpen) {
            Surface(
                modifier = Modifier.align(Alignment.TopCenter).windowInsetsPadding(WindowInsets.safeDrawing).padding(12.dp),
                color = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f),
                shape = MaterialTheme.shapes.medium,
            ) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    launch.title?.let { Text(it, style = MaterialTheme.typography.titleMedium) }
                    val summary = listOfNotNull(stats.resolution, stats.fps?.let { stringResource(R.string.boosteroid_fps, it) }, stats.codec)
                    if (summary.isNotEmpty()) Text(summary.joinToString(" · "))
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Button(onClick = { menuOpen = false }) { Text(stringResource(R.string.action_resume)) }
                        TextButton(onClick = { menuOpen = false; keyboardOpen = true }) {
                            Text(stringResource(R.string.stream_panel_keyboard))
                        }
                        TextButton(onClick = { touchControls = !touchControls }) {
                            Text(stringResource(if (touchControls) R.string.boosteroid_hide_controls else R.string.boosteroid_controls))
                        }
                        TextButton(onClick = {
                            muted = !muted
                            client?.setAudioMuted(muted)
                        }) { Text(stringResource(if (muted) R.string.boosteroid_unmute else R.string.boosteroid_mute)) }
                        if (controllerConnected) {
                            TextButton(onClick = { client?.setControllerMouseAssist(true); menuOpen = false }) {
                                Text(stringResource(R.string.stream_panel_controller_mouse))
                            }
                        }
                        TextButton(onClick = { client?.openSteamMenu(); menuOpen = false }) {
                            Text(stringResource(R.string.stream_panel_steam_menu))
                        }
                        TextButton(onClick = model::stop) { Text(stringResource(R.string.boosteroid_end_session)) }
                    }
                }
            }
        }
    }
}

@Composable
private fun BoosteroidCursor(cursor: StreamRemoteCursorState, aspect: Float) {
    val bitmap = cursor.bitmap
    val image = remember(bitmap) { bitmap?.asImageBitmap() }
    Canvas(Modifier.fillMaxSize()) {
        val width = size.width.coerceAtMost(size.height * aspect)
        val height = width / aspect
        val position = Offset(
            (size.width - width) / 2f + width * cursor.normalizedX,
            (size.height - height) / 2f + height * cursor.normalizedY,
        )
        if (image != null) {
            drawImage(image, topLeft = position - Offset(cursor.hotSpotX, cursor.hotSpotY))
        } else {
            drawCircle(Color.Black, radius = 5.dp.toPx(), center = position)
            drawCircle(Color.White, radius = 3.dp.toPx(), center = position)
        }
    }
}
