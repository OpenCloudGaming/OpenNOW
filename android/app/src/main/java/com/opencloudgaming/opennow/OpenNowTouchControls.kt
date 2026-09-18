package com.opencloudgaming.opennow

import android.view.KeyEvent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.layout.findRootCoordinates
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.unit.IntSize
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.key.key
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt

internal val LocalTouchButtonEdit = staticCompositionLocalOf<((String) -> Unit)?> { null }

@Composable
internal fun Modifier.editTouchButtonOnTap(button: String): Modifier {
    val onEdit = LocalTouchButtonEdit.current ?: return this
    return clickable(onClickLabel = stringResource(R.string.touch_button_edit, button)) { onEdit(button) }
}

private val LocalTouchInputEnabled = staticCompositionLocalOf { true }

@Composable
internal fun TouchOverlay(
    client: NativeStreamClient,
    touch: AndroidTouchSettings,
    onButtonTone: () -> Unit,
    layoutEditing: Boolean,
    onSaveAllOffsets: (Map<String, TouchOffset>) -> Unit,
    onButtonAppearanceChange: (String, TouchButtonAppearance) -> Unit,
    inputResetKey: Any = Unit,
    modifier: Modifier = Modifier,
) {
    var editingButton by remember(layoutEditing) { mutableStateOf<String?>(null) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var foreground by remember(lifecycle) { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, _ -> foreground = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    val opacity = touch.opacity
    val layoutScale = touch.scale
    val buttonScale = touch.buttonScale
    val stickScale = touch.stickScale
    val defaultOffsets = remember { AndroidTouchSettings().offsets }

    val localOffsets = remember(touch.offsets) {
        androidx.compose.runtime.mutableStateMapOf<String, TouchOffset>().apply {
            putAll(touch.offsets)
        }
    }

    fun getLocalOffset(key: String): TouchOffset {
        val saved = localOffsets[key]
        if (saved != null) return saved
        defaultOffsets[key]?.let { return it }
        val baseKey = key.substringBeforeLast("_")
        return when (baseKey) {
            "lt", "lb", "lstick", "dpad", "l3" -> TouchOffset(touch.leftOffsetXDp, touch.leftOffsetYDp)
            "rt", "rb", "rstick", "face", "r3" -> TouchOffset(touch.rightOffsetXDp, touch.rightOffsetYDp)
            else -> TouchOffset()
        }
    }

    val onLocalOffsetChange = { key: String, x: Float, y: Float ->
        localOffsets[key] = TouchOffset(x, y)
    }

    val currentLocalOffsets by rememberUpdatedState(localOffsets.toMap())
    val currentOnSaveAllOffsets by rememberUpdatedState(onSaveAllOffsets)
    DisposableEffect(layoutEditing) {
        onDispose {
            if (layoutEditing) {
                currentOnSaveAllOffsets(currentLocalOffsets)
            }
        }
    }

    DisposableEffect(client) {
        onDispose {
            NativeStreamInputRouter.clearTouchControllerPassthroughBounds()
        }
    }

    val skin = remember(touch.touchControllerStyle, touch.opacity, touch.touchSkinTint) {
        touchSkinColors(touch.touchControllerStyle, touch.opacity, touchSkinAccent(touch))
    }
    val skinForm = remember(touch.touchControllerStyle) { touchSkinForm(touch.touchControllerStyle) }
    CompositionLocalProvider(
        LocalTouchControllerStyle provides touch.touchControllerStyle,
        LocalTouchSkin provides skin,
        LocalTouchSkinForm provides skinForm,
        LocalTouchButtonLabels provides touch.touchButtonLabels,
        LocalTouchButtonAppearances provides touch.buttonAppearances,
        LocalTouchButtonEdit provides if (layoutEditing) ({ button: String -> editingButton = button }) else null,
        LocalTouchStickKnobScale provides touch.stickKnobScale,
        LocalTouchInputEnabled provides (!layoutEditing && foreground),
    ) {
        key(client, touch, layoutEditing, inputResetKey) {
            BoxWithConstraints(
                modifier
                    .fillMaxSize()
                    .padding(
                        start = touch.edgePaddingDp.dp,
                        top = 10.dp,
                        end = touch.edgePaddingDp.dp,
                        bottom = touch.bottomPaddingDp.dp,
                    ),
            ) {
                if (touch.enabled) {
                    val landscape = maxWidth > maxHeight
                    val suffix = if (landscape) "_landscape" else "_portrait"
                    val getOrientationLocalOffset = { key: String -> getLocalOffset(key + suffix) }
                    val onOrientationLocalOffsetChange = { key: String, x: Float, y: Float ->
                        onLocalOffsetChange(key + suffix, x, y)
                    }

                    if (landscape) {
                        LandscapeTouchControls(
                            client = client,
                            opacity = opacity,
                            layoutScale = layoutScale,
                            buttonScale = buttonScale,
                            stickScale = stickScale,
                            faceButtonScale = touch.faceButtonScale,
                            dpadScale = touch.dpadScale,
                            shoulderButtonScale = touch.shoulderButtonScale,
                            centerButtonScale = touch.centerButtonScale,
                            leftStickScale = touch.leftStickScale,
                            rightStickScale = touch.rightStickScale,
                            visibleControlGroups = touch.visibleControlGroups,
                            extraButtonCombos = List(TOUCH_EXTRA_BUTTON_COUNT, touch::extraButtonCombo),
                            extraButtonScale = touch.extraButtonScale,
                            joystickMode = touch.joystickMode,
                            aimMode = touch.aimMode,
                            aimZoneScale = touch.aimZoneScale,
                            aimZoneSensitivity = touch.aimZoneSensitivity,
                            joystickDeadZone = touch.joystickDeadZone,
                            viewportHeight = maxHeight,
                            layoutEditing = layoutEditing,
                            getLocalOffset = getOrientationLocalOffset,
                            onLocalOffsetChange = onOrientationLocalOffsetChange,
                            onButtonTone = onButtonTone,
                        )
                    } else {
                        PortraitTouchControls(
                            client = client,
                            opacity = opacity,
                            layoutScale = layoutScale,
                            buttonScale = buttonScale,
                            stickScale = stickScale,
                            faceButtonScale = touch.faceButtonScale,
                            dpadScale = touch.dpadScale,
                            shoulderButtonScale = touch.shoulderButtonScale,
                            centerButtonScale = touch.centerButtonScale,
                            leftStickScale = touch.leftStickScale,
                            rightStickScale = touch.rightStickScale,
                            visibleControlGroups = touch.visibleControlGroups,
                            extraButtonCombos = List(TOUCH_EXTRA_BUTTON_COUNT, touch::extraButtonCombo),
                            extraButtonScale = touch.extraButtonScale,
                            joystickMode = touch.joystickMode,
                            aimMode = touch.aimMode,
                            aimZoneScale = touch.aimZoneScale,
                            aimZoneSensitivity = touch.aimZoneSensitivity,
                            joystickDeadZone = touch.joystickDeadZone,
                            layoutEditing = layoutEditing,
                            getLocalOffset = getOrientationLocalOffset,
                            onLocalOffsetChange = onOrientationLocalOffsetChange,
                            onButtonTone = onButtonTone,
                        )
                    }
                }
            }
        }
    }
    if (layoutEditing) editingButton?.let { button ->
        TouchButtonAppearanceDialog(button, touch,
            onChange = { updated ->
                onButtonAppearanceChange(button, updated.buttonAppearances[button] ?: TouchButtonAppearance())
            },
            onDismiss = { editingButton = null },
        )
    }
}

@Composable
private fun PortraitTouchControls(
    client: NativeStreamClient,
    opacity: Float,
    layoutScale: Float,
    buttonScale: Float,
    stickScale: Float,
    faceButtonScale: Float,
    dpadScale: Float,
    shoulderButtonScale: Float,
    centerButtonScale: Float,
    leftStickScale: Float,
    rightStickScale: Float,
    visibleControlGroups: Set<TouchControlGroup>,
    extraButtonCombos: List<List<TouchExtraButtonAction>>,
    extraButtonScale: Float,
    joystickMode: TouchJoystickMode,
    aimMode: TouchAimMode,
    aimZoneScale: Float,
    aimZoneSensitivity: Float,
    joystickDeadZone: Float,
    layoutEditing: Boolean,
    getLocalOffset: (String) -> TouchOffset,
    onLocalOffsetChange: (String, Float, Float) -> Unit,
    onButtonTone: () -> Unit,
) {
    val leftStickDiameter = 116.dp * stickScale * leftStickScale * layoutScale
    val rightStickDiameter = 104.dp * stickScale * rightStickScale * layoutScale
    val centerScale = buttonScale * centerButtonScale * layoutScale
    val buttonSize48 = 48.dp * centerScale
    val buttonSize44 = 44.dp * centerScale
    val faceWidth = buttonSize48 * 2.44f

    Box(
        Modifier.fillMaxSize().padding(horizontal = 32.dp, vertical = 24.dp)
    ) {
        if (aimMode == TouchAimMode.LockZone && TouchControlGroup.RightStick in visibleControlGroups) {
            val aimOffset = getLocalOffset("aimzone")
            TouchControlGroup(
                id = "portrait-aim-zone-group",
                layoutEditing = layoutEditing,
                offsetX = aimOffset.x.dp,
                offsetY = aimOffset.y.dp,
                onOffsetChange = { x, y -> onLocalOffsetChange("aimzone", x, y) },
                modifier = Modifier.align(Alignment.BottomEnd)
                    .fillMaxWidth(scaledAimZoneFraction(0.54f, aimZoneScale))
                    .fillMaxHeight(scaledAimZoneFraction(0.48f, aimZoneScale)),
            ) {
                LockZoneAimSurface(
                    id = "portrait-aim-zone",
                    client = client,
                    opacity = opacity,
                    deadZone = joystickDeadZone,
                    sensitivity = aimZoneSensitivity,
                    enabled = !layoutEditing,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
        val shoulderScale = buttonScale * shoulderButtonScale * layoutScale
        val triggerWidth = 64.dp * shoulderScale
        val bumperHeight = 32.dp * shoulderScale
        val triggerTouchHeight = if (bumperHeight < 48.dp) 48.dp else bumperHeight

        ExtraTouchButtons(
            orientation = "portrait",
            combos = extraButtonCombos,
            scale = buttonScale * extraButtonScale * layoutScale,
            client = client,
            layoutEditing = layoutEditing,
            getLocalOffset = getLocalOffset,
            onLocalOffsetChange = onLocalOffsetChange,
            onButtonTone = onButtonTone,
            modifier = Modifier.align(Alignment.TopCenter).padding(top = 4.dp),
        )

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-lt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lt").x.dp,
            offsetY = getLocalOffset("lt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lt", x, y) },
            modifier = Modifier.align(Alignment.TopStart),
        ) {
            GamepadTriggerButton(
                label = "LT",
                left = true,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-lb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lb").x.dp,
            offsetY = getLocalOffset("lb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lb", x, y) },
            modifier = Modifier.align(Alignment.TopStart).padding(top = triggerTouchHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "LB",
                mask = 0x0100,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        if (TouchControlGroup.LeftStick in visibleControlGroups) TouchControlGroup(
            id = "portrait-lstick",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lstick").x.dp,
            offsetY = getLocalOffset("lstick").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lstick", x, y) },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            VirtualStick(
                label = "L",
                client = client,
                diameter = leftStickDiameter,
                mode = joystickMode,
                deadZone = joystickDeadZone,
                onChange = client::setVirtualLeftStick,
            )
        }

        if (TouchControlGroup.ThumbButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-l3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("l3").x.dp,
            offsetY = getLocalOffset("l3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("l3", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(
                start = (leftStickDiameter - buttonSize48) / 2,
                bottom = leftStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("LS", GamepadButtonMapping.LEFT_THUMB, client, buttonSize48, onButtonTone)
        }

        if (TouchControlGroup.Dpad in visibleControlGroups) TouchControlGroup(
            id = "portrait-dpad",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("dpad").x.dp,
            offsetY = getLocalOffset("dpad").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("dpad", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(start = leftStickDiameter + 12.dp),
        ) {
            DpadCluster(client, buttonScale * dpadScale * layoutScale, onButtonTone)
        }

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-rt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rt").x.dp,
            offsetY = getLocalOffset("rt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rt", x, y) },
            modifier = Modifier.align(Alignment.TopEnd),
        ) {
            GamepadTriggerButton(
                label = "RT",
                left = false,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-rb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rb").x.dp,
            offsetY = getLocalOffset("rb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rb", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = triggerTouchHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "RB",
                mask = 0x0200,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        if (TouchControlGroup.MenuButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-select",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("select").x.dp,
            offsetY = getLocalOffset("select").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("select", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = buttonSize48 + 8.dp, end = buttonSize44 + 8.dp),
        ) {
            GamepadButton("◀", 0x0020, client, buttonSize44, onButtonTone)
        }

        if (TouchControlGroup.MenuButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-start",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("start").x.dp,
            offsetY = getLocalOffset("start").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("start", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = buttonSize48 + 8.dp),
        ) {
            GamepadButton("▶", 0x0010, client, buttonSize44, onButtonTone)
        }

        if (aimMode == TouchAimMode.LockJoystick && TouchControlGroup.RightStick in visibleControlGroups) {
            TouchControlGroup(
                id = "portrait-rstick",
                layoutEditing = layoutEditing,
                offsetX = getLocalOffset("rstick").x.dp,
                offsetY = getLocalOffset("rstick").y.dp,
                onOffsetChange = { x, y -> onLocalOffsetChange("rstick", x, y) },
                modifier = Modifier.align(Alignment.BottomEnd).padding(end = faceWidth + 12.dp),
            ) {
                VirtualStick(
                    label = "R",
                    client = client,
                    diameter = rightStickDiameter,
                    mode = joystickMode,
                    deadZone = joystickDeadZone,
                    onChange = client::setVirtualRightStick,
                )
            }
        }

        if (TouchControlGroup.ThumbButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-r3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("r3").x.dp,
            offsetY = getLocalOffset("r3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("r3", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd).padding(
                end = faceWidth + 12.dp + (rightStickDiameter - buttonSize48) / 2,
                bottom = rightStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("RS", GamepadButtonMapping.RIGHT_THUMB, client, buttonSize48, onButtonTone)
        }

        if (TouchControlGroup.FaceButtons in visibleControlGroups) TouchControlGroup(
            id = "portrait-face",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("face").x.dp,
            offsetY = getLocalOffset("face").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("face", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd),
        ) {
            FaceButtonCluster(client, buttonScale * faceButtonScale * layoutScale, onButtonTone)
        }
    }
}

@Composable
private fun BoxScope.LandscapeTouchControls(
    client: NativeStreamClient,
    opacity: Float,
    layoutScale: Float,
    buttonScale: Float,
    stickScale: Float,
    faceButtonScale: Float,
    dpadScale: Float,
    shoulderButtonScale: Float,
    centerButtonScale: Float,
    leftStickScale: Float,
    rightStickScale: Float,
    visibleControlGroups: Set<TouchControlGroup>,
    extraButtonCombos: List<List<TouchExtraButtonAction>>,
    extraButtonScale: Float,
    joystickMode: TouchJoystickMode,
    aimMode: TouchAimMode,
    aimZoneScale: Float,
    aimZoneSensitivity: Float,
    joystickDeadZone: Float,
    viewportHeight: Dp,
    layoutEditing: Boolean,
    getLocalOffset: (String) -> TouchOffset,
    onLocalOffsetChange: (String, Float, Float) -> Unit,
    onButtonTone: () -> Unit,
) {
    val controlScale = buttonScale * layoutScale
    val shoulderScale = controlScale * shoulderButtonScale
    val centerScale = controlScale * centerButtonScale
    val topControlClearance = landscapeTouchTopControlClearanceDp(viewportHeight.value, shoulderScale).dp
    Box(Modifier.fillMaxSize().padding(horizontal = 24.dp, vertical = 24.dp)) {
        if (aimMode == TouchAimMode.LockZone && TouchControlGroup.RightStick in visibleControlGroups) {
            val aimOffset = getLocalOffset("aimzone")
            TouchControlGroup(
                id = "landscape-aim-zone-group",
                layoutEditing = layoutEditing,
                offsetX = aimOffset.x.dp,
                offsetY = aimOffset.y.dp,
                onOffsetChange = { x, y -> onLocalOffsetChange("aimzone", x, y) },
                modifier = Modifier.align(Alignment.CenterEnd)
                    .fillMaxWidth(scaledAimZoneFraction(0.48f, aimZoneScale))
                    .fillMaxHeight(scaledAimZoneFraction(0.72f, aimZoneScale)),
            ) {
                LockZoneAimSurface(
                    id = "landscape-aim-zone",
                    client = client,
                    opacity = opacity,
                    deadZone = joystickDeadZone,
                    sensitivity = aimZoneSensitivity,
                    enabled = !layoutEditing,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
        val triggerWidth = 76.dp * shoulderScale
        val bumperHeight = 36.dp * shoulderScale
        val triggerTouchHeight = if (bumperHeight < 48.dp) 48.dp else bumperHeight

        ExtraTouchButtons(
            orientation = "landscape",
            combos = extraButtonCombos,
            scale = controlScale * extraButtonScale,
            client = client,
            layoutEditing = layoutEditing,
            getLocalOffset = getLocalOffset,
            onLocalOffsetChange = onLocalOffsetChange,
            onButtonTone = onButtonTone,
            modifier = Modifier.align(Alignment.TopCenter).padding(top = topControlClearance),
        )

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-lt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lt").x.dp,
            offsetY = getLocalOffset("lt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lt", x, y) },
            modifier = Modifier.align(Alignment.TopStart).padding(top = topControlClearance),
        ) {
            GamepadTriggerButton(
                label = "LT",
                left = true,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-lb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lb").x.dp,
            offsetY = getLocalOffset("lb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lb", x, y) },
            modifier = Modifier.align(Alignment.TopStart).padding(top = topControlClearance + triggerTouchHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "LB",
                mask = 0x0100,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        val selectSize = 42.dp * centerScale
        if (TouchControlGroup.MenuButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-select",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("select").x.dp,
            offsetY = getLocalOffset("select").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("select", x, y) },
            modifier = Modifier.align(Alignment.BottomCenter).padding(end = selectSize / 2 + 27.dp),
        ) {
            GamepadButton("◀", 0x0020, client, selectSize, onButtonTone)
        }

        if (TouchControlGroup.MenuButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-start",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("start").x.dp,
            offsetY = getLocalOffset("start").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("start", x, y) },
            modifier = Modifier.align(Alignment.BottomCenter).padding(start = selectSize / 2 + 27.dp),
        ) {
            GamepadButton("▶", 0x0010, client, selectSize, onButtonTone)
        }

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-rb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rb").x.dp,
            offsetY = getLocalOffset("rb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rb", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = topControlClearance + triggerTouchHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "RB",
                mask = 0x0200,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        if (TouchControlGroup.ShoulderButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-rt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rt").x.dp,
            offsetY = getLocalOffset("rt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rt", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = topControlClearance),
        ) {
            GamepadTriggerButton(
                label = "RT",
                left = false,
                client = client,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        val effectiveDpadScale = controlScale * dpadScale * 0.88f
        val dpadButtonSize = 54.dp * effectiveDpadScale
        // Keep the next control outside the full skin-aware d-pad canvas. The d-pad painter grew
        // beyond the old four-button cluster width when the shaped skins were introduced.
        val dpadWidth = if (TouchControlGroup.Dpad in visibleControlGroups) touchDpadBoxSize(dpadButtonSize) else 0.dp
        if (TouchControlGroup.Dpad in visibleControlGroups) TouchControlGroup(
            id = "landscape-dpad",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("dpad").x.dp,
            offsetY = getLocalOffset("dpad").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("dpad", x, y) },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            DpadCluster(client, effectiveDpadScale, onButtonTone)
        }

        val leftStickDiameter = 112.dp * stickScale * leftStickScale * layoutScale
        if (TouchControlGroup.LeftStick in visibleControlGroups) TouchControlGroup(
            id = "landscape-lstick",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lstick").x.dp,
            offsetY = getLocalOffset("lstick").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lstick", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(start = dpadWidth + 14.dp),
        ) {
            VirtualStick(
                label = "L",
                client = client,
                diameter = leftStickDiameter,
                mode = joystickMode,
                deadZone = joystickDeadZone,
                onChange = client::setVirtualLeftStick,
            )
        }

        val l3Size = 54.dp * centerScale
        if (TouchControlGroup.ThumbButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-l3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("l3").x.dp,
            offsetY = getLocalOffset("l3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("l3", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(
                start = dpadWidth + 14.dp + (leftStickDiameter - l3Size) / 2,
                bottom = leftStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("LS", GamepadButtonMapping.LEFT_THUMB, client, l3Size, onButtonTone)
        }

        val faceScale = controlScale * faceButtonScale * 0.9f
        val faceButtonSize = 54.dp * faceScale
        val faceWidth = faceButtonSize * 2.44f
        val rightStickDiameter = 112.dp * stickScale * rightStickScale * layoutScale
        if (aimMode == TouchAimMode.LockJoystick && TouchControlGroup.RightStick in visibleControlGroups) {
            TouchControlGroup(
                id = "landscape-rstick",
                layoutEditing = layoutEditing,
                offsetX = getLocalOffset("rstick").x.dp,
                offsetY = getLocalOffset("rstick").y.dp,
                onOffsetChange = { x, y -> onLocalOffsetChange("rstick", x, y) },
                modifier = Modifier.align(Alignment.BottomEnd).padding(end = faceWidth + 14.dp),
            ) {
                VirtualStick(
                    label = "R",
                    client = client,
                    diameter = rightStickDiameter,
                    mode = joystickMode,
                    deadZone = joystickDeadZone,
                    onChange = client::setVirtualRightStick,
                )
            }
        }

        val r3Size = 54.dp * centerScale
        if (TouchControlGroup.ThumbButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-r3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("r3").x.dp,
            offsetY = getLocalOffset("r3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("r3", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd).padding(
                end = faceWidth + 14.dp + (rightStickDiameter - r3Size) / 2,
                bottom = rightStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("RS", GamepadButtonMapping.RIGHT_THUMB, client, r3Size, onButtonTone)
        }

        if (TouchControlGroup.FaceButtons in visibleControlGroups) TouchControlGroup(
            id = "landscape-face",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("face").x.dp,
            offsetY = getLocalOffset("face").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("face", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd),
        ) {
            FaceButtonCluster(client, faceScale, onButtonTone)
        }
    }
}

internal fun landscapeTouchTopControlClearanceDp(viewportHeightDp: Float, controlScale: Float): Float {
    val viewportBand = (viewportHeightDp * 0.11f).coerceIn(34f, 58f)
    val scaledBand = viewportBand * controlScale.coerceIn(0.75f, 1.35f)
    return scaledBand.coerceIn(30f, 76f)
}

internal fun touchExtraButtonActionLabel(action: TouchExtraButtonAction): String = when (action) {
    TouchExtraButtonAction.None -> "Off"
    TouchExtraButtonAction.Guide -> "Guide / Home"
    TouchExtraButtonAction.A -> "A"
    TouchExtraButtonAction.B -> "B"
    TouchExtraButtonAction.X -> "X"
    TouchExtraButtonAction.Y -> "Y"
    TouchExtraButtonAction.DpadUp -> "D-pad Up"
    TouchExtraButtonAction.DpadDown -> "D-pad Down"
    TouchExtraButtonAction.DpadLeft -> "D-pad Left"
    TouchExtraButtonAction.DpadRight -> "D-pad Right"
    TouchExtraButtonAction.LeftBumper -> "LB"
    TouchExtraButtonAction.RightBumper -> "RB"
    TouchExtraButtonAction.LeftTrigger -> "LT"
    TouchExtraButtonAction.RightTrigger -> "RT"
    TouchExtraButtonAction.LeftStickClick -> "L3 / LS"
    TouchExtraButtonAction.RightStickClick -> "R3 / RS"
    TouchExtraButtonAction.Start -> "Start"
    TouchExtraButtonAction.Select -> "Select"
    TouchExtraButtonAction.LeftStickLeft -> "Left stick left (A)"
    TouchExtraButtonAction.LeftStickRight -> "Left stick right (D)"
    TouchExtraButtonAction.KeyboardA -> "Keyboard A"
    TouchExtraButtonAction.KeyboardD -> "Keyboard D"
    TouchExtraButtonAction.LeftBumperAndLeftTrigger -> "LB + LT / L1 + L2"
    TouchExtraButtonAction.RightBumperAndRightTrigger -> "RB + RT / R1 + R2"
    TouchExtraButtonAction.LeftAndRightBumpers -> "LB + RB / L1 + R1"
    TouchExtraButtonAction.LeftAndRightTriggers -> "LT + RT / L2 + R2"
}

internal fun touchExtraButtonComboLabel(actions: List<TouchExtraButtonAction>): String {
    val normalized = normalizeTouchExtraButtonCombo(actions)
    return if (normalized.isEmpty()) "Off" else normalized.joinToString(" + ", transform = ::touchExtraButtonActionLabel)
}

internal fun touchControlGroupLabelRes(group: TouchControlGroup): Int = when (group) {
    TouchControlGroup.FaceButtons -> R.string.settings_touch_control_face
    TouchControlGroup.Dpad -> R.string.settings_touch_control_dpad
    TouchControlGroup.LeftStick -> R.string.settings_touch_control_left_stick
    TouchControlGroup.RightStick -> R.string.settings_touch_control_right_stick
    TouchControlGroup.ShoulderButtons -> R.string.settings_touch_control_shoulders
    TouchControlGroup.ThumbButtons -> R.string.settings_touch_control_thumb
    TouchControlGroup.MenuButtons -> R.string.settings_touch_control_menu
}

internal fun nextTouchExtraButtonAction(action: TouchExtraButtonAction): TouchExtraButtonAction {
    val actions = TouchExtraButtonAction.entries
    return actions[(actions.indexOf(action) + 1) % actions.size]
}

private fun touchExtraButtonCapLabel(action: TouchExtraButtonAction): String = when (action) {
    TouchExtraButtonAction.None -> ""
    TouchExtraButtonAction.Guide -> "G"
    TouchExtraButtonAction.DpadUp -> "↑"
    TouchExtraButtonAction.DpadDown -> "↓"
    TouchExtraButtonAction.DpadLeft -> "←"
    TouchExtraButtonAction.DpadRight -> "→"
    TouchExtraButtonAction.LeftBumper -> "LB"
    TouchExtraButtonAction.RightBumper -> "RB"
    TouchExtraButtonAction.LeftTrigger -> "LT"
    TouchExtraButtonAction.RightTrigger -> "RT"
    TouchExtraButtonAction.LeftStickClick -> "LS"
    TouchExtraButtonAction.RightStickClick -> "RS"
    TouchExtraButtonAction.Start -> "▶"
    TouchExtraButtonAction.Select -> "◀"
    TouchExtraButtonAction.LeftStickLeft -> "LS←"
    TouchExtraButtonAction.LeftStickRight -> "LS→"
    TouchExtraButtonAction.KeyboardA -> "A⌨"
    TouchExtraButtonAction.KeyboardD -> "D⌨"
    TouchExtraButtonAction.LeftBumperAndLeftTrigger -> "L1+L2"
    TouchExtraButtonAction.RightBumperAndRightTrigger -> "R1+R2"
    TouchExtraButtonAction.LeftAndRightBumpers -> "L1+R1"
    TouchExtraButtonAction.LeftAndRightTriggers -> "L2+R2"
    else -> action.name
}

private fun touchExtraButtonCapLabel(actions: List<TouchExtraButtonAction>): String {
    val labels = normalizeTouchExtraButtonCombo(actions).map(::touchExtraButtonCapLabel)
    return when {
        labels.size <= 3 -> labels.joinToString("+")
        else -> "${labels.first()}+${labels.size - 1}"
    }
}

internal data class TouchExtraButtonBinding(
    val buttonMasks: List<Int> = emptyList(),
    val leftTrigger: Boolean = false,
    val rightTrigger: Boolean = false,
    val leftStickX: Float? = null,
    val keyboardKeyCodes: List<Int> = emptyList(),
) {
    val keyboardKeyCode: Int? get() = keyboardKeyCodes.singleOrNull()
}

private fun atomicTouchExtraButtonBinding(action: TouchExtraButtonAction): TouchExtraButtonBinding = when (action) {
    TouchExtraButtonAction.Guide -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.GUIDE))
    TouchExtraButtonAction.A -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.A))
    TouchExtraButtonAction.B -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.B))
    TouchExtraButtonAction.X -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.X))
    TouchExtraButtonAction.Y -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.Y))
    TouchExtraButtonAction.DpadUp -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.DPAD_UP))
    TouchExtraButtonAction.DpadDown -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.DPAD_DOWN))
    TouchExtraButtonAction.DpadLeft -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.DPAD_LEFT))
    TouchExtraButtonAction.DpadRight -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.DPAD_RIGHT))
    TouchExtraButtonAction.LeftBumper -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.LEFT_SHOULDER))
    TouchExtraButtonAction.RightBumper -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.RIGHT_SHOULDER))
    TouchExtraButtonAction.LeftTrigger -> TouchExtraButtonBinding(leftTrigger = true)
    TouchExtraButtonAction.RightTrigger -> TouchExtraButtonBinding(rightTrigger = true)
    TouchExtraButtonAction.LeftStickClick -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.LEFT_THUMB))
    TouchExtraButtonAction.RightStickClick -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.RIGHT_THUMB))
    TouchExtraButtonAction.Start -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.START))
    TouchExtraButtonAction.Select -> TouchExtraButtonBinding(listOf(GamepadButtonMapping.BACK))
    TouchExtraButtonAction.LeftStickLeft -> TouchExtraButtonBinding(leftStickX = -1f)
    TouchExtraButtonAction.LeftStickRight -> TouchExtraButtonBinding(leftStickX = 1f)
    TouchExtraButtonAction.KeyboardA -> TouchExtraButtonBinding(keyboardKeyCodes = listOf(KeyEvent.KEYCODE_A))
    TouchExtraButtonAction.KeyboardD -> TouchExtraButtonBinding(keyboardKeyCodes = listOf(KeyEvent.KEYCODE_D))
    TouchExtraButtonAction.LeftBumperAndLeftTrigger,
    TouchExtraButtonAction.RightBumperAndRightTrigger,
    TouchExtraButtonAction.LeftAndRightBumpers,
    TouchExtraButtonAction.LeftAndRightTriggers,
    TouchExtraButtonAction.None,
    -> TouchExtraButtonBinding()
}

internal fun touchExtraButtonBinding(action: TouchExtraButtonAction): TouchExtraButtonBinding =
    touchExtraButtonBinding(listOf(action))

internal fun touchExtraButtonBinding(actions: List<TouchExtraButtonAction>): TouchExtraButtonBinding {
    val bindings = normalizeTouchExtraButtonCombo(actions).map(::atomicTouchExtraButtonBinding)
    val stickValues = bindings.mapNotNull(TouchExtraButtonBinding::leftStickX)
    return TouchExtraButtonBinding(
        buttonMasks = bindings.flatMap(TouchExtraButtonBinding::buttonMasks).distinct(),
        leftTrigger = bindings.any(TouchExtraButtonBinding::leftTrigger),
        rightTrigger = bindings.any(TouchExtraButtonBinding::rightTrigger),
        leftStickX = stickValues.takeIf { it.isNotEmpty() }?.sum()?.coerceIn(-1f, 1f),
        keyboardKeyCodes = bindings.flatMap(TouchExtraButtonBinding::keyboardKeyCodes).distinct(),
    )
}

@Composable
private fun BoxScope.ExtraTouchButtons(
    orientation: String,
    combos: List<List<TouchExtraButtonAction>>,
    scale: Float,
    client: NativeStreamClient,
    layoutEditing: Boolean,
    getLocalOffset: (String) -> TouchOffset,
    onLocalOffsetChange: (String, Float, Float) -> Unit,
    onButtonTone: () -> Unit,
    modifier: Modifier,
) {
    combos.take(TOUCH_EXTRA_BUTTON_COUNT).forEachIndexed { index, actions ->
        if (actions.isEmpty()) return@forEachIndexed
        val controlKey = "extra${index + 1}"
        key("$orientation-$controlKey", actions) {
            TouchControlGroup(
                id = "$orientation-$controlKey",
                layoutEditing = layoutEditing,
                offsetX = getLocalOffset(controlKey).x.dp,
                offsetY = getLocalOffset(controlKey).y.dp,
                onOffsetChange = { x, y -> onLocalOffsetChange(controlKey, x, y) },
                modifier = modifier,
            ) {
                GamepadActionButton(
                    actions = actions,
                    appearanceKey = controlKey,
                    sourceId = "touch-$orientation-$controlKey",
                    client = client,
                    size = 44.dp * scale,
                    onPressTone = onButtonTone,
                )
            }
        }
    }
}

@Composable
internal fun TouchControlGroup(
    id: String,
    layoutEditing: Boolean,
    offsetX: Dp,
    offsetY: Dp,
    onOffsetChange: (Float, Float) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val density = LocalDensity.current
    val currentOffsetX by rememberUpdatedState(offsetX)
    val currentOffsetY by rememberUpdatedState(offsetY)
    val currentOnOffsetChange by rememberUpdatedState(onOffsetChange)
    var controlBounds by remember { mutableStateOf(Rect.Zero) }
    var viewportSize by remember { mutableStateOf(IntSize.Zero) }
    Box(
        modifier
            .absoluteOffset(x = offsetX, y = offsetY)
            .pointerInput(layoutEditing) {
                if (!layoutEditing) return@pointerInput
                var start = TouchOffset()
                var bounds = Rect.Zero
                var viewport = IntSize.Zero
                var travel = Offset.Zero
                detectDragGestures(onDragStart = {
                    start = TouchOffset(currentOffsetX.value, currentOffsetY.value)
                    bounds = controlBounds
                    viewport = viewportSize
                    travel = Offset.Zero
                }) { change, dragAmount ->
                    change.consume()
                    travel += dragAmount
                    with(density) {
                        currentOnOffsetChange(
                            draggedTouchOffset(start.x, travel.x.toDp().value,
                                bounds.left.toDp().value, bounds.right.toDp().value, viewport.width.toDp().value),
                            draggedTouchOffset(start.y, travel.y.toDp().value,
                                bounds.top.toDp().value, bounds.bottom.toDp().value, viewport.height.toDp().value),
                        )
                    }
                }
            }
            .onGloballyPositioned { coordinates ->
                controlBounds = Rect(coordinates.positionInRoot(), androidx.compose.ui.geometry.Size(
                    coordinates.size.width.toFloat(), coordinates.size.height.toFloat(),
                ))
                viewportSize = coordinates.findRootCoordinates().size
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setTouchControllerPassthroughBound(
                    id,
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            },
        contentAlignment = Alignment.Center,
    ) {
        content()
        if (layoutEditing) {
            Box(
                Modifier
                    .matchParentSize()
                    .clip(RoundedCornerShape(18.dp))
                    .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.16f))
                    .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.72f), RoundedCornerShape(18.dp)),
                contentAlignment = Alignment.TopCenter,
            ) {
                Box(
                    modifier = Modifier.padding(top = 4.dp).clip(RoundedCornerShape(999.dp))
                        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.9f)),
                ) {
                    Text(
                        stringResource(R.string.touch_drag_label),
                        color = MaterialTheme.colorScheme.onPrimary,
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                    )
                }
            }
        }
    }
    DisposableEffect(id) {
        onDispose {
            NativeStreamInputRouter.clearTouchControllerPassthroughBound(id)
        }
    }
}

private fun clampStickOffset(offset: Offset, maxRadius: Float): Offset {
    val distance = sqrt(offset.x * offset.x + offset.y * offset.y)
    if (distance <= maxRadius || distance == 0f) return offset
    val scale = maxRadius / distance
    return Offset(offset.x * scale, offset.y * scale)
}

internal fun touchStickValue(
    deltaX: Float,
    deltaY: Float,
    maxTravel: Float,
    deadZone: Float,
    sensitivity: Float = 1f,
): Offset {
    if (!deltaX.isFinite() || !deltaY.isFinite() || !maxTravel.isFinite() || maxTravel <= 0f) {
        return Offset.Zero
    }
    val responsiveTravel = touchAimMaxTravel(maxTravel, sensitivity)
    val clamped = clampStickOffset(Offset(deltaX, deltaY), responsiveTravel)
    val rawX = (clamped.x / responsiveTravel).coerceIn(-1f, 1f)
    val rawY = (clamped.y / responsiveTravel).coerceIn(-1f, 1f)
    val magnitude = sqrt(rawX * rawX + rawY * rawY).coerceIn(0f, 1f)
    val adjustedMagnitude = applyTouchJoystickDeadZone(magnitude, deadZone)
    val adjustment = if (magnitude > 0f) adjustedMagnitude / magnitude else 0f
    return Offset(rawX * adjustment, rawY * adjustment)
}

internal fun scaledAimZoneFraction(baseFraction: Float, scale: Float): Float {
    if (!baseFraction.isFinite() || baseFraction <= 0f) return 0f
    val safeScale = if (scale.isFinite()) scale.coerceIn(0.5f, 1.5f) else 1f
    return (baseFraction * safeScale).coerceIn(0f, 1f)
}

internal fun touchAimMaxTravel(maxTravel: Float, sensitivity: Float): Float {
    val safeSensitivity = if (sensitivity.isFinite()) sensitivity.coerceIn(0.25f, 3f) else 1f
    return maxTravel / safeSensitivity
}

internal fun applyTouchJoystickDeadZone(value: Float, deadZone: Float): Float {
    val clampedValue = value.coerceIn(-1f, 1f)
    val clampedDeadZone = deadZone.coerceIn(0f, 0.95f)
    val magnitude = kotlin.math.abs(clampedValue)
    if (magnitude <= clampedDeadZone) return 0f
    val adjusted = (magnitude - clampedDeadZone) / (1f - clampedDeadZone)
    return if (clampedValue < 0f) -adjusted else adjusted
}

@Composable
private fun LockZoneAimSurface(
    id: String,
    client: NativeStreamClient,
    opacity: Float,
    deadZone: Float,
    sensitivity: Float,
    enabled: Boolean,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val currentOnChange by rememberUpdatedState(client::setVirtualRightStick)
    var aimAnchor by remember { mutableStateOf<Offset?>(null) }
    var aimOffset by remember { mutableStateOf(Offset.Zero) }
    val maxTravelPx = with(density) { LOCK_ZONE_MAX_TRAVEL_DP.dp.toPx() }
    val responsiveTravelPx = touchAimMaxTravel(maxTravelPx, sensitivity)

    DisposableEffect(client, id) {
        onDispose {
            client.setVirtualRightStick(0f, 0f)
            NativeStreamInputRouter.clearTouchControllerPassthroughBound(id)
        }
    }

    Box(
        modifier
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setTouchControllerPassthroughBound(
                    id,
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            }
            .pointerInput(client, deadZone, sensitivity, enabled, maxTravelPx) {
                if (!enabled) return@pointerInput
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    val anchor = down.position
                    aimAnchor = anchor
                    aimOffset = Offset.Zero

                    fun updateAim(position: Offset) {
                        val delta = position - anchor
                        val value = touchStickValue(delta.x, delta.y, maxTravelPx, deadZone, sensitivity)
                        currentOnChange(value.x, value.y)
                        aimOffset = clampStickOffset(delta, responsiveTravelPx)
                    }

                    try {
                        updateAim(down.position)
                        down.consume()
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Initial)
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                change.consume()
                                break
                            }
                            updateAim(change.position)
                            change.consume()
                        }
                    } finally {
                        currentOnChange(0f, 0f)
                        aimAnchor = null
                        aimOffset = Offset.Zero
                    }
                }
            },
        contentAlignment = Alignment.TopCenter,
    ) {
        val zoneColor = Color.White.copy(alpha = opacity * 0.22f)
        Canvas(Modifier.matchParentSize()) {
            drawRoundRect(
                color = zoneColor,
                cornerRadius = CornerRadius(22.dp.toPx()),
                style = Stroke(width = 1.dp.toPx()),
            )
            aimAnchor?.let { anchor ->
                drawCircle(
                    color = Color.White.copy(alpha = opacity * 0.32f),
                    radius = 13.dp.toPx(),
                    center = anchor + aimOffset,
                    style = Stroke(width = 1.5.dp.toPx()),
                )
            }
        }
        Text(
            text = stringResource(R.string.stream_joysticks_aim_zone_label),
            color = Color.White.copy(alpha = opacity * 0.46f),
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.padding(top = 8.dp),
        )
    }
}

@Composable
private fun StickWithThumbButton(
    stickLabel: String,
    thumbLabel: String,
    thumbMask: Int,
    client: NativeStreamClient,
    diameter: Dp,
    buttonScale: Float,
    mode: TouchJoystickMode = TouchJoystickMode.Fixed,
    deadZone: Float = 0f,
    onButtonTone: () -> Unit,
    onChange: (Float, Float) -> Unit,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        GamepadPillButton(
            label = thumbLabel,
            mask = thumbMask,
            client = client,
            width = 56.dp * buttonScale,
            height = 34.dp * buttonScale,
            onPressTone = onButtonTone,
        )
        VirtualStick(
            label = stickLabel,
            client = client,
            diameter = diameter,
            mode = mode,
            deadZone = deadZone,
            onChange = onChange,
        )
    }
}

@Composable
private fun VirtualStick(
    label: String,
    client: NativeStreamClient,
    diameter: androidx.compose.ui.unit.Dp,
    mode: TouchJoystickMode,
    deadZone: Float,
    onChange: (Float, Float) -> Unit,
) {
    val inputEnabled = LocalTouchInputEnabled.current
    val currentOnChange by rememberUpdatedState(onChange)
    var knobOffset by remember { mutableStateOf(Offset.Zero) }
    var baseOffset by remember { mutableStateOf(Offset.Zero) }

    DisposableEffect(client) {
        onDispose {
            currentOnChange(0f, 0f)
        }
    }

    Box(
        Modifier
            .size(diameter)
            .pointerInput(client, mode, deadZone, inputEnabled) {
                if (!inputEnabled) return@pointerInput
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    val fixedCenter = Offset(size.width / 2f, size.height / 2f)
                    val gestureCenter = if (mode == TouchJoystickMode.Dynamic) down.position else fixedCenter
                    val maxRadius = min(size.width, size.height) * 0.34f
                    baseOffset = gestureCenter - fixedCenter

                    fun updateStick(position: Offset) {
                        val clamped = clampStickOffset(position - gestureCenter, maxRadius)
                        val value = touchStickValue(clamped.x, clamped.y, maxRadius, deadZone)
                        currentOnChange(value.x, value.y)
                        knobOffset = clamped
                    }

                    try {
                        updateStick(down.position)
                        down.consume()
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Initial)
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                change.consume()
                                break
                            }
                            updateStick(change.position)
                            change.consume()
                        }
                    } finally {
                        currentOnChange(0f, 0f)
                        knobOffset = Offset.Zero
                        baseOffset = Offset.Zero
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        TouchStickFace(diameter = diameter, base = { baseOffset }, knob = { knobOffset })
    }
}

private const val LOCK_ZONE_MAX_TRAVEL_DP = 72f

@Composable
private fun FaceButtonCluster(client: NativeStreamClient, scale: Float, onButtonTone: () -> Unit) {
    val buttonSize = 54.dp * scale
    val appearances = LocalTouchButtonAppearances.current
    val largestButton = listOf("A", "B", "X", "Y").maxOf { button ->
        (buttonSize * (appearances[button]?.effectiveSizeScale() ?: 1f)).coerceAtLeast(48.dp)
    }
    val distance = maxOf(buttonSize, largestButton) * 1.05f
    val boxSize = distance * 2 + largestButton
    Box(Modifier.size(boxSize)) {
        Box(Modifier.align(Alignment.Center).offset(y = -distance)) {
            GamepadButton("Y", 0x8000, client, buttonSize, onButtonTone)
        }
        Box(Modifier.align(Alignment.Center).offset(y = distance)) {
            GamepadButton("A", 0x1000, client, buttonSize, onButtonTone)
        }
        Box(Modifier.align(Alignment.Center).offset(x = -distance)) {
            GamepadButton("X", 0x4000, client, buttonSize, onButtonTone)
        }
        Box(Modifier.align(Alignment.Center).offset(x = distance)) {
            GamepadButton("B", 0x2000, client, buttonSize, onButtonTone)
        }
    }
}

@Composable
private fun DpadCluster(client: NativeStreamClient, scale: Float, onButtonTone: () -> Unit) {
    val inputEnabled = LocalTouchInputEnabled.current
    val currentOnButtonTone by rememberUpdatedState(onButtonTone)
    val buttonSize = 54.dp * scale
    val boxSize = touchDpadBoxSize(buttonSize)

    var upPressed by remember { mutableStateOf(false) }
    var downPressed by remember { mutableStateOf(false) }
    var leftPressed by remember { mutableStateOf(false) }
    var rightPressed by remember { mutableStateOf(false) }

    DisposableEffect(client) {
        onDispose {
            client.setVirtualButton(0x0001, false)
            client.setVirtualButton(0x0002, false)
            client.setVirtualButton(0x0004, false)
            client.setVirtualButton(0x0008, false)
        }
    }

    Box(
        Modifier
            .size(boxSize)
            .pointerInput(client, inputEnabled) {
                if (!inputEnabled) return@pointerInput
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)

                    fun updateDirection(position: Offset) {
                        val w = size.width
                        val h = size.height
                        val cx = w / 2f
                        val cy = h / 2f
                        val px = position.x
                        val py = position.y
                        val dx = px - cx
                        val dy = py - cy
                        val touchDist = Math.sqrt((dx * dx + dy * dy).toDouble()).toFloat()
                        val deadzone = 12.dp.toPx()
                        var newUp = false
                        var newDown = false
                        var newLeft = false
                        var newRight = false
                        if (touchDist > deadzone) {
                            val absDx = Math.abs(dx)
                            val absDy = Math.abs(dy)
                            if (dy < 0 && absDy > absDx * 0.414f) newUp = true
                            if (dy > 0 && absDy > absDx * 0.414f) newDown = true
                            if (dx < 0 && absDx > absDy * 0.414f) newLeft = true
                            if (dx > 0 && absDx > absDy * 0.414f) newRight = true
                        }

                        val playTone = (!upPressed && newUp) || (!downPressed && newDown) ||
                                       (!leftPressed && newLeft) || (!rightPressed && newRight)
                        if (upPressed != newUp) { client.setVirtualButton(0x0001, newUp); upPressed = newUp }
                        if (downPressed != newDown) { client.setVirtualButton(0x0002, newDown); downPressed = newDown }
                        if (leftPressed != newLeft) { client.setVirtualButton(0x0004, newLeft); leftPressed = newLeft }
                        if (rightPressed != newRight) { client.setVirtualButton(0x0008, newRight); rightPressed = newRight }
                        if (playTone) currentOnButtonTone()
                    }

                    try {
                        updateDirection(down.position)
                        down.consume()
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Initial)
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                change.consume()
                                break
                            }
                            updateDirection(change.position)
                            change.consume()
                        }
                    } finally {
                        if (upPressed) { client.setVirtualButton(0x0001, false); upPressed = false }
                        if (downPressed) { client.setVirtualButton(0x0002, false); downPressed = false }
                        if (leftPressed) { client.setVirtualButton(0x0004, false); leftPressed = false }
                        if (rightPressed) { client.setVirtualButton(0x0008, false); rightPressed = false }
                    }
                }
            }
    ) {
        TouchDpadFace(
            arm = buttonSize,
            up = upPressed,
            down = downPressed,
            left = leftPressed,
            right = rightPressed,
        )
    }
}

internal fun Modifier.virtualPressInput(
    inputOwner: Any,
    controlKey: Any,
    onPressedChange: State<(Boolean) -> Unit>,
    toggle: Boolean,
    enabled: Boolean,
): Modifier = pointerInput(inputOwner, controlKey, toggle, enabled) {
    if (!enabled) return@pointerInput
    val press = TouchButtonPressState(toggle)
    try {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            onPressedChange.value(press.down())
            var completed = false
            try {
                down.consume()
                while (true) {
                    val event = awaitPointerEvent(PointerEventPass.Initial)
                    val change = event.changes.firstOrNull { it.id == down.id } ?: break
                    change.consume()
                    if (!change.pressed) {
                        completed = true
                        break
                    }
                }
            } finally {
                onPressedChange.value(if (completed) press.up() else press.cancel())
            }
        }
    } finally {
        // Disposal, editing, backgrounding and configuration changes release latched inputs too.
        onPressedChange.value(press.cancel())
    }
}

@Composable
private fun GamepadTriggerButton(
    label: String,
    left: Boolean,
    client: NativeStreamClient,
    width: androidx.compose.ui.unit.Dp,
    height: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    var pressed by remember { mutableStateOf(false) }
    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            client.setVirtualTrigger(left, down)
            pressed = down
            if (down) onPressTone()
        }
    }
    Box(
        Modifier
            .editTouchButtonOnTap(label)
            .virtualPressInput(client, left, currentOnPressedChange,
                LocalTouchButtonAppearances.current[label]?.toggle == true, LocalTouchInputEnabled.current),
        contentAlignment = Alignment.TopCenter,
    ) {
        TouchShoulderFace(label = label, pressed = pressed, width = width, height = height)
    }
    DisposableEffect(client, left) {
        onDispose {
            client.setVirtualTrigger(left, false)
        }
    }
}

@Composable
private fun GamepadBumperButton(
    label: String,
    mask: Int,
    client: NativeStreamClient,
    width: androidx.compose.ui.unit.Dp,
    height: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    var pressed by remember { mutableStateOf(false) }
    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            client.setVirtualButton(mask, down)
            pressed = down
            if (down) onPressTone()
        }
    }
    Box(
        Modifier
            .editTouchButtonOnTap(label)
            .virtualPressInput(client, mask, currentOnPressedChange,
                LocalTouchButtonAppearances.current[label]?.toggle == true, LocalTouchInputEnabled.current),
        contentAlignment = Alignment.Center,
    ) {
        TouchShoulderFace(label = label, pressed = pressed, width = width, height = height)
    }
    DisposableEffect(client, mask) {
        onDispose {
            client.setVirtualButton(mask, false)
        }
    }
}

@Composable
private fun GamepadActionButton(
    actions: List<TouchExtraButtonAction>,
    appearanceKey: String,
    sourceId: String,
    client: NativeStreamClient,
    size: Dp,
    onPressTone: () -> Unit,
) {
    val currentOnPressTone by rememberUpdatedState(onPressTone)
    var pressed by remember(actions, sourceId) { mutableStateOf(false) }

    fun dispatch(down: Boolean) {
        val binding = touchExtraButtonBinding(actions)
        binding.buttonMasks.forEach { mask -> client.setVirtualButtonFromSource(mask, sourceId, down) }
        if (binding.leftTrigger) client.setVirtualTriggerFromSource(true, sourceId, down)
        if (binding.rightTrigger) client.setVirtualTriggerFromSource(false, sourceId, down)
        binding.leftStickX?.let { x -> client.setVirtualLeftStickFromSource(sourceId, if (down) x else 0f, 0f) }
        binding.keyboardKeyCodes.forEach { keyCode -> client.setVirtualKeyboardKeyFromSource(keyCode, sourceId, down) }
    }

    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            dispatch(down)
            pressed = down
            if (down) currentOnPressTone()
        }
    }
    Box(
        Modifier
            .editTouchButtonOnTap(appearanceKey)
            .virtualPressInput(client, "$sourceId-${actions.joinToString("-") { it.name }}", currentOnPressedChange,
                LocalTouchButtonAppearances.current[appearanceKey]?.toggle == true, LocalTouchInputEnabled.current),
        contentAlignment = Alignment.Center,
    ) {
        TouchCapFace(label = touchExtraButtonCapLabel(actions), pressed = pressed, diameter = size, appearanceKey = appearanceKey)
    }
    DisposableEffect(client, actions, sourceId) {
        onDispose { dispatch(false) }
    }
}

@Composable
private fun GamepadButton(
    label: String,
    mask: Int,
    client: NativeStreamClient,
    size: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    val currentOnPressTone by rememberUpdatedState(onPressTone)
    var pressed by remember { mutableStateOf(false) }
    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            client.setVirtualButton(mask, down)
            pressed = down
            if (down) currentOnPressTone()
        }
    }
    Box(
        Modifier
            .editTouchButtonOnTap(label)
            .virtualPressInput(client, mask, currentOnPressedChange,
                LocalTouchButtonAppearances.current[label]?.toggle == true, LocalTouchInputEnabled.current),
        contentAlignment = Alignment.Center,
    ) {
        TouchCapFace(label = label, pressed = pressed, diameter = size)
    }
    DisposableEffect(client, mask) {
        onDispose {
            client.setVirtualButton(mask, false)
        }
    }
}

@Composable
private fun GamepadPillButton(
    label: String,
    mask: Int,
    client: NativeStreamClient,
    width: androidx.compose.ui.unit.Dp,
    height: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    val currentOnPressTone by rememberUpdatedState(onPressTone)
    var pressed by remember { mutableStateOf(false) }
    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            client.setVirtualButton(mask, down)
            pressed = down
            if (down) currentOnPressTone()
        }
    }
    Box(
        Modifier
            .editTouchButtonOnTap(label)
            .virtualPressInput(client, mask, currentOnPressedChange,
                LocalTouchButtonAppearances.current[label]?.toggle == true, LocalTouchInputEnabled.current),
        contentAlignment = Alignment.Center,
    ) {
        TouchShoulderFace(label = label, pressed = pressed, width = width, height = height)
    }
    DisposableEffect(client, mask) {
        onDispose {
            client.setVirtualButton(mask, false)
        }
    }
}
