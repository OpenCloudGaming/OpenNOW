package com.opencloudgaming.opennow

import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import kotlinx.coroutines.cancel
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sqrt

internal class NativeUiTouchRoutingState {
    @Volatile
    private var streamChromeBounds: TouchPassthroughBounds? = null
    @Volatile
    private var streamPanelBounds: TouchPassthroughBounds? = null
    @Volatile
    private var overlayBounds: Map<String, TouchPassthroughBounds> = emptyMap()
    @Volatile
    private var touchControllerBounds: Map<String, TouchPassthroughBounds> = emptyMap()
    @Volatile
    private var touchControllerVisible = false

    private val trackedPointerIds = mutableSetOf<Int>()
    private val ownedPointerIds = mutableSetOf<Int>()

    @Volatile
    var passthroughActive: Boolean = false
        private set

    fun setStreamChromeBounds(left: Int, top: Int, right: Int, bottom: Int) {
        streamChromeBounds = TouchPassthroughBounds(left, top, right, bottom)
    }

    fun clearStreamChromeBounds() {
        streamChromeBounds = null
    }

    fun setOverlayBound(id: String, left: Int, top: Int, right: Int, bottom: Int) {
        overlayBounds = overlayBounds.toMutableMap().also {
            it[id] = TouchPassthroughBounds(left, top, right, bottom)
        }
    }

    fun clearOverlayBound(id: String) {
        if (id !in overlayBounds) return
        overlayBounds = overlayBounds.toMutableMap().also { it.remove(id) }
    }

    fun setStreamPanelBounds(left: Int, top: Int, right: Int, bottom: Int) {
        streamPanelBounds = TouchPassthroughBounds(left, top, right, bottom)
    }

    fun clearStreamPanelBounds() {
        streamPanelBounds = null
    }

    fun setTouchControllerBounds(left: Int, top: Int, right: Int, bottom: Int) {
        touchControllerBounds = mapOf("default" to TouchPassthroughBounds(left, top, right, bottom))
    }

    fun setTouchControllerBound(id: String, left: Int, top: Int, right: Int, bottom: Int) {
        touchControllerBounds = touchControllerBounds.toMutableMap().also {
            it[id] = TouchPassthroughBounds(left, top, right, bottom)
        }
    }

    fun clearTouchControllerBound(id: String) {
        if (id !in touchControllerBounds) return
        touchControllerBounds = touchControllerBounds.toMutableMap().also { it.remove(id) }
    }

    fun setTouchControllerVisible(visible: Boolean) {
        touchControllerVisible = visible
        if (!visible) touchControllerBounds = emptyMap()
    }

    fun clearTouchControllerBounds() {
        touchControllerBounds = emptyMap()
        touchControllerVisible = false
    }

    fun routesTouchMouseThroughCompose(): Boolean = touchControllerVisible

    fun touchesRegisteredUi(x: Float, y: Float, width: Int, height: Int): Boolean {
        if (streamChromeBounds?.contains(x, y) == true) return true
        if (streamPanelBounds?.contains(x, y) == true) return true
        if (overlayBounds.values.any { it.contains(x, y) }) return true
        if (touchControllerBounds.values.any { it.contains(x, y) }) return true
        // Before Compose has measured the controller there are no precise bounds to protect, so
        // retain the lower-screen fallback for that short startup window. Once even one control
        // has registered, use the measured bounds exclusively. Keeping the fallback active after
        // layout makes the entire lower half unavailable to Finger Mouse even in empty space.
        return touchControllerVisible &&
            touchControllerBounds.isEmpty() &&
            width > 0 &&
            height > 0 &&
            y >= height * TOUCH_CONTROLLER_FALLBACK_TOP_RATIO
    }

    fun beginPointerGesture(pointerId: Int, touchesUi: Boolean) {
        trackedPointerIds.clear()
        ownedPointerIds.clear()
        trackedPointerIds += pointerId
        if (touchesUi) ownedPointerIds += pointerId
        syncPassthroughActive()
    }

    fun addPointer(pointerId: Int, touchesUi: Boolean) {
        trackedPointerIds += pointerId
        if (touchesUi) ownedPointerIds += pointerId
        syncPassthroughActive()
    }

    fun ownsPointer(pointerId: Int): Boolean = pointerId in ownedPointerIds

    fun classifiesPointerAsUi(pointerId: Int, touchesUiNow: Boolean): Boolean =
        if (pointerId in trackedPointerIds) ownsPointer(pointerId) else touchesUiNow

    fun hasOwnedPointer(): Boolean = ownedPointerIds.isNotEmpty()

    fun ownedPointers(): Set<Int> = ownedPointerIds

    fun releasePointer(pointerId: Int) {
        trackedPointerIds.remove(pointerId)
        ownedPointerIds.remove(pointerId)
        syncPassthroughActive()
    }

    fun endPointerGesture() {
        trackedPointerIds.clear()
        ownedPointerIds.clear()
        syncPassthroughActive()
    }

    fun setLegacyPassthroughActive(active: Boolean) {
        passthroughActive = active
    }

    private fun syncPassthroughActive() {
        passthroughActive = ownedPointerIds.isNotEmpty()
    }

    private data class TouchPassthroughBounds(
        val left: Int,
        val top: Int,
        val right: Int,
        val bottom: Int,
    ) {
        fun contains(x: Float, y: Float): Boolean =
            x >= left - EDGE_SLOP_PX &&
                x <= right + EDGE_SLOP_PX &&
                y >= top - EDGE_SLOP_PX &&
                y <= bottom + EDGE_SLOP_PX

        companion object {
            private const val EDGE_SLOP_PX = 24
        }
    }

    private companion object {
        const val TOUCH_CONTROLLER_FALLBACK_TOP_RATIO = 0.52f
    }
}

internal fun shouldConsumeNativeUiTransitionTouch(
    streamUiActive: Boolean,
    hasOwnedPointer: Boolean,
): Boolean = streamUiActive && hasOwnedPointer

object NativeStreamInputRouter {
    private data class PresentationTransform(
        val zoomScale: Float = 1f,
        val translationX: Float = 0f,
        val translationY: Float = 0f,
    )

    @Volatile
    private var client: NativeStreamClient? = null
    @Volatile
    private var androidTvProfile = false
    fun setAndroidTvProfile(enabled: Boolean) {
        androidTvProfile = enabled
    }
    @Volatile
    private var externalMousePointerCaptureEnabled = false

    fun setExternalMousePointerCaptureEnabled(enabled: Boolean) {
        externalMousePointerCaptureEnabled = enabled
    }

    fun isExternalMousePointerCaptureEnabled(): Boolean =
        externalMousePointerCaptureEnabled

    @Volatile
    private var touchMouseEnabled = false
    @Volatile
    private var mouseDirectClick = false
    @Volatile
    private var stretchToFit = false
    @Volatile
    private var renderingAspectRatio = 0f
    @Volatile
    private var presentationTransform = PresentationTransform()
    @Volatile
    private var decodedStreamResolution = 0 to 0
    @Volatile
    private var captureAllTouch = false
    @Volatile
    private var systemMenuHandler: (() -> Unit)? = null

    @Volatile
    private var systemBackHandler: (() -> Unit)? = null
    @Volatile
    private var streamUiActive = false
    private val nativeUiTouchRouting = NativeUiTouchRoutingState()
    private val touchMouseState = TouchMouseState()

    /**
     * When set, fingers are forwarded to the host as real touch instead of being turned into a
     * cursor. Mutually exclusive with the touch-mouse and direct-click paths by construction:
     * [dispatchTouch] takes this branch first and returns.
     */
    @Volatile
    private var nativeTouchEnabled = false
    private val touchSlots = TouchSlotAllocator()
    private val nativeTouchMotionTracker = NativeTouchMotionTracker()
    /** Native touch settings synced from [AndroidTouchSettings]. */
    @Volatile private var nativeTouchScrollScale: Float = 1.0f
    @Volatile private var nativeTouchJitterThresholdPx: Float = 0f

    fun attach(next: NativeStreamClient) {
        client = next
        // Never carry an in-progress drag position into a different host session.
        touchMouseState.forgetCursorPosition()
        decodedStreamResolution = 0 to 0
        resetPresentationTransform()
    }

    fun detach(next: NativeStreamClient) {
        if (client === next) {
            releaseInputForLifecycle("stream-detached")
            client = null
            touchMouseState.forgetCursorPosition()
            decodedStreamResolution = 0 to 0
            resetPresentationTransform()
        }
    }

    /** Releases held touch, mouse, and keyboard input when focus or lifecycle changes. */
    fun releaseInputForLifecycle(reason: String) {
        client?.releasePhysicalInputForLifecycle(reason)
        touchMouseState.reset(client)
        releaseAllNativeTouches()
        nativeUiTouchRouting.endPointerGesture()
    }

    fun setTouchMouseEnabled(enabled: Boolean) {
        touchMouseEnabled = enabled
        if (!enabled) {
            touchMouseState.reset(client)
        }
    }

    fun setMouseDirectClick(enabled: Boolean) {
        mouseDirectClick = enabled
        touchMouseState.reset(client)
    }

    fun setNativeTouchEnabled(enabled: Boolean) {
        if (nativeTouchEnabled == enabled) return
        nativeTouchEnabled = enabled
        // Leaving the mode mid-gesture would otherwise strand whatever fingers are down.
        releaseAllNativeTouches()
        nativeUiTouchRouting.endPointerGesture()
        touchMouseState.reset(client)
    }

    fun setNativeTouchSettings(scrollScale: Float, jitterThresholdDp: Float) {
        val density = android.content.res.Resources.getSystem().displayMetrics.density
        nativeTouchScrollScale = scrollScale
        nativeTouchJitterThresholdPx = jitterThresholdDp * density
    }

    fun setStretchToFit(enabled: Boolean) {
        if (stretchToFit != enabled) {
            stretchToFit = enabled
            touchMouseState.reset(client)
        }
    }

    fun setRenderingAspectRatio(ratio: Float) {
        if (renderingAspectRatio != ratio) {
            renderingAspectRatio = ratio
            touchMouseState.reset(client)
        }
    }

    /**
     * Mirrors the uniform scale and translation applied by the Compose stream surface. Input is
     * mapped through the inverse transform before letterbox/stretch mapping, so direct click keeps
     * targeting the pixel visibly under the finger after pinch zoom or pan.
     */
    fun setPresentationTransform(zoomScale: Float, translationX: Float, translationY: Float) {
        val safeScale = zoomScale.takeIf { it.isFinite() }?.coerceIn(1f, 3f) ?: 1f
        val safeTranslationX = translationX.takeIf { it.isFinite() } ?: 0f
        val safeTranslationY = translationY.takeIf { it.isFinite() } ?: 0f
        val next = PresentationTransform(safeScale, safeTranslationX, safeTranslationY)
        if (presentationTransform == next) return
        touchMouseState.reset(client)
        presentationTransform = next
    }

    private fun resetPresentationTransform() {
        presentationTransform = PresentationTransform()
    }

    fun setDecodedStreamResolution(width: Int, height: Int) {
        if (width > 0 && height > 0) {
            val next = width to height
            if (decodedStreamResolution != next) {
                // A new decoded geometry changes the visible content bounds even though the
                // selected viewport remains fixed. End in-flight gestures before switching
                // coordinate spaces so a held pointer cannot jump across the host screen.
                touchMouseState.reset(client)
                releaseAllNativeTouches()
            }
            decodedStreamResolution = next
        }
    }

    private fun inputContentAspectRatio(decodedResolution: Pair<Int, Int>): Float =
        if (decodedResolution.first > 0 && decodedResolution.second > 0) {
            decodedResolution.first.toFloat() / decodedResolution.second.toFloat()
        } else {
            renderingAspectRatio
        }

    fun setCaptureAllTouch(enabled: Boolean) {
        captureAllTouch = enabled
    }

    fun setSystemMenuHandler(handler: (() -> Unit)?) {
        systemMenuHandler = handler
    }

    fun setSystemBackHandler(handler: (() -> Unit)?) {
        systemBackHandler = handler
    }

    fun dispatchSystemBack(): Boolean {
        val handler = systemBackHandler ?: return false
        handler()
        return true
    }


    fun setStreamUiActive(active: Boolean) {
        if (active && !streamUiActive) {
            // A system/menu action can open app UI while a native game touch is still held. The
            // host will not receive that finger's eventual UP once UI routing takes over, so cancel
            // it at the transition instead of leaving a stuck press in the game.
            releaseAllNativeTouches()
            touchMouseState.reset(client)
            client?.releasePhysicalInputForLifecycle("stream-ui-opened")
        }
        streamUiActive = active
    }

    fun normalizedStreamUiKeyCode(event: KeyEvent): Int? {
        if (!streamUiActive) return null
        return when (event.keyCode) {
            KeyEvent.KEYCODE_BUTTON_A -> KeyEvent.KEYCODE_DPAD_CENTER
            KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_SELECT -> KeyEvent.KEYCODE_BACK
            else -> null
        }
    }

    fun normalizedAppUiKeyCode(event: KeyEvent): Int? {
        return normalizedAppUiKeyCode(event.keyCode, streamUiActive)
    }

    fun normalizedAppUiKeyCode(keyCode: Int, streamUiActive: Boolean): Int? {
        if (streamUiActive) return null
        return when (keyCode) {
            KeyEvent.KEYCODE_BUTTON_A -> KeyEvent.KEYCODE_DPAD_CENTER
            else -> null
        }
    }

    fun isControllerAppBackKey(event: KeyEvent): Boolean =
        isControllerAppBackKey(
            keyCode = event.keyCode,
            controllerSource = event.isControllerInputDevice(),
            streamUiActive = streamUiActive,
        )

    fun isControllerAppBackKey(keyCode: Int, controllerSource: Boolean, streamUiActive: Boolean): Boolean =
        !streamUiActive &&
            (keyCode == KeyEvent.KEYCODE_BUTTON_B ||
                (keyCode == KeyEvent.KEYCODE_BACK && controllerSource))

    fun setUiTouchPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        nativeUiTouchRouting.setStreamChromeBounds(left, top, right, bottom)
    }

    fun clearUiTouchPassthroughBounds() {
        nativeUiTouchRouting.clearStreamChromeBounds()
    }

    fun setOverlayTouchPassthroughBound(id: String, left: Int, top: Int, right: Int, bottom: Int) {
        nativeUiTouchRouting.setOverlayBound(id, left, top, right, bottom)
    }

    fun clearOverlayTouchPassthroughBound(id: String) {
        nativeUiTouchRouting.clearOverlayBound(id)
    }

    fun setStreamPanelTouchPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        nativeUiTouchRouting.setStreamPanelBounds(left, top, right, bottom)
    }

    fun clearStreamPanelTouchPassthroughBounds() {
        nativeUiTouchRouting.clearStreamPanelBounds()
    }

    fun setTouchControllerPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        nativeUiTouchRouting.setTouchControllerBounds(left, top, right, bottom)
    }

    fun setTouchControllerPassthroughBound(id: String, left: Int, top: Int, right: Int, bottom: Int) {
        nativeUiTouchRouting.setTouchControllerBound(id, left, top, right, bottom)
    }

    fun clearTouchControllerPassthroughBound(id: String) {
        nativeUiTouchRouting.clearTouchControllerBound(id)
    }

    fun setTouchControllerVisible(visible: Boolean) {
        nativeUiTouchRouting.setTouchControllerVisible(visible)
    }

    fun clearTouchControllerPassthroughBounds() {
        nativeUiTouchRouting.clearTouchControllerBounds()
    }

    fun cancelTouchMouse() {
        touchMouseState.reset(client)
    }

    fun isNativeUiTouchGestureActive(): Boolean =
        nativeUiTouchRouting.hasOwnedPointer()

    /**
     * If app UI was opened on DOWN, consume the remainder of that launcher gesture before Android
     * can retarget its UP to a control at the same coordinates in the newly mounted panel.
     */
    fun shouldConsumeUiTransitionTouchBeforeViews(event: MotionEvent): Boolean =
        event.isFingerTouchEvent() &&
            shouldConsumeNativeUiTransitionTouch(
                streamUiActive = streamUiActive,
                hasOwnedPointer = nativeUiTouchRouting.hasOwnedPointer(),
            )

    fun shouldForwardTouchBeforeViews(event: MotionEvent, width: Int, height: Int): Boolean {
        val isDirectClick = mouseDirectClick && event.isExternalMousePointerEvent()
        val isNativeTouch = nativeTouchEnabled && event.isFingerTouchEvent()
        if (
            client == null ||
            streamUiActive ||
            !(touchMouseEnabled || isDirectClick || isNativeTouch) ||
            !captureAllTouch ||
            width <= 0 ||
            height <= 0 ||
            !(event.isFingerTouchEvent() || isDirectClick)
        ) {
            return false
        }
        updateNativeUiTouchPointers(event, width, height)
        if (
            touchMouseEnabled &&
            event.isFingerTouchEvent() &&
            !isNativeTouch &&
            nativeUiTouchRouting.routesTouchMouseThroughCompose()
        ) {
            return false
        }
        if (!eventHasStreamTouchPointer(event, width, height)) return false
        // The single-pointer restriction below belongs to the cursor paths, where only one finger
        // can drive the pointer. Native touch forwards every finger by definition, so a two-finger
        // gesture reaching this point is the normal case rather than something to hand to the views.
        if (isNativeTouch) return true
        return event.pointerCount == 1 || nativeUiTouchRouting.hasOwnedPointer()
    }

    fun shouldCaptureTouchBeforeViews(event: MotionEvent, width: Int, height: Int): Boolean =
        shouldForwardTouchBeforeViews(event, width, height) &&
            !nativeUiTouchRouting.hasOwnedPointer()

    fun dispatchTouch(event: MotionEvent, width: Int, height: Int): Boolean {
        val current = client ?: return false
        if (streamUiActive) return false
        // Direct click supports both external mouse/touchpad events AND finger touch events,
        // as long as the user has enabled mouseDirectClick in settings.
        val isDirectClick = mouseDirectClick && (event.isExternalMousePointerEvent() || event.isFingerTouchEvent())
        if (!event.isFingerTouchEvent() && !isDirectClick) return false
        updateNativeUiTouchPointers(event, width, height)
        if (nativeTouchEnabled && event.isFingerTouchEvent() && width > 0 && height > 0) {
            return dispatchNativeTouch(event, current, width, height)
        }
        val decodedResolution = decodedStreamResolution
        val transform = presentationTransform
        return touchMouseState.handle(
            event = event,
            enabled = (touchMouseEnabled || isDirectClick) && width > 0 && height > 0,
            client = current,
            ignoredPointerIds = nativeUiTouchRouting.ownedPointers(),
            directClick = mouseDirectClick,
            width = width,
            height = height,
            stretchToFit = stretchToFit,
            renderingAspectRatio = inputContentAspectRatio(decodedResolution),
            presentationZoomScale = transform.zoomScale,
            presentationTranslationX = transform.translationX,
            presentationTranslationY = transform.translationY,
            decodedStreamWidth = decodedResolution.first,
            decodedStreamHeight = decodedResolution.second,
        )
    }

    /**
     * Forwards every finger as native touch, so the host presents a digitizer and touch-aware games
     * switch to their own mobile UI. Unlike the cursor paths this keeps no per-gesture state of its
     * own — the only thing carried across events is the pointer-id to slot mapping.
     */
    private fun dispatchNativeTouch(
        event: MotionEvent,
        client: NativeStreamClient,
        width: Int,
        height: Int,
    ): Boolean {
        val phase = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> TouchPhase.DOWN
            MotionEvent.ACTION_MOVE -> TouchPhase.MOVE
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> TouchPhase.UP
            MotionEvent.ACTION_CANCEL -> TouchPhase.CANCEL
            else -> return false
        }

        // Android reports which pointer changed only for the down/up actions; a MOVE carries fresh
        // positions for every finger at once, and all of them belong in the batch.
        val indices = if (phase == TouchPhase.MOVE || event.actionMasked == MotionEvent.ACTION_CANCEL) {
            0 until event.pointerCount
        } else {
            val index = if (event.actionMasked == MotionEvent.ACTION_DOWN) 0 else event.actionIndex
            index..index
        }

        val scrollScale = nativeTouchScrollScale.coerceIn(0.25f, 2.0f)
        val jitterThresholdPx = nativeTouchJitterThresholdPx.coerceAtLeast(0f)

        var hasGameTouchPointer = false
        val pointers = indices.mapNotNull { index ->
            if (index !in 0 until event.pointerCount) return@mapNotNull null
            val pointerId = event.getPointerId(index)
            // Fingers on our own chrome belong to the overlay, not the game.
            if (nativeUiTouchRouting.ownsPointer(pointerId)) return@mapNotNull null
            hasGameTouchPointer = true

            val rawX = event.getX(index)
            val rawY = event.getY(index)

            val stabilized = when (phase) {
                TouchPhase.DOWN -> nativeTouchMotionTracker.begin(pointerId, rawX, rawY)
                TouchPhase.MOVE -> nativeTouchMotionTracker.move(
                    pointerId = pointerId,
                    x = rawX,
                    y = rawY,
                    movementScale = scrollScale,
                    jitterThresholdPx = jitterThresholdPx,
                ) ?: return@mapNotNull null
                TouchPhase.UP, TouchPhase.CANCEL -> nativeTouchMotionTracker.end(
                    pointerId = pointerId,
                    x = rawX,
                    y = rawY,
                    movementScale = scrollScale,
                    jitterThresholdPx = jitterThresholdPx,
                )
                else -> return@mapNotNull null
            }

            stabilized.copy(
                radiusX = event.getTouchMajor(index) / 2f,
                radiusY = event.getTouchMinor(index) / 2f,
            )
        }
        // A MOVE inside the tap guard deliberately emits no packet, but the native-touch path
        // still owns that finger. Report it handled so Activity.dispatchTouchEvent does not offer
        // the middle of an already-captured gesture to Compose and then dispatch it here again.
        if (pointers.isEmpty()) return hasGameTouchPointer

        val settingsResolution = streamResolutionPixels(client.settings)
        val decodedResolution = decodedStreamResolution
        val transform = presentationTransform
        val streamWidth = decodedResolution.first.takeIf { it > 0 } ?: settingsResolution.first
        val streamHeight = decodedResolution.second.takeIf { it > 0 } ?: settingsResolution.second
        val records = buildTouchBatch(
            allocator = touchSlots,
            phase = phase,
            pointers = pointers,
            viewWidth = width,
            viewHeight = height,
            streamWidth = streamWidth,
            streamHeight = streamHeight,
            stretchToFit = stretchToFit,
            renderingAspectRatio = inputContentAspectRatio(decodedResolution),
            presentationZoomScale = transform.zoomScale,
            presentationTranslationX = transform.translationX,
            presentationTranslationY = transform.translationY,
        )
        if (records.isEmpty()) return false
        return client.sendNativeTouch(records)
    }

    /**
     * Lifts every finger the host still believes is down. Called when touch is turned off or the
     * session is interrupted, because in neither case will the platform deliver the missing UP.
     */
    private fun releaseAllNativeTouches() {
        val current = client
        val pointerIds = touchSlots.activePointerIds()
        if (current != null && pointerIds.isNotEmpty()) {
            val records = pointerIds.mapNotNull { pointerId ->
                touchSlots.release(pointerId)?.let { slot ->
                    TouchRecord(slot = slot, phase = TouchPhase.CANCEL, x = 0, y = 0)
                }
            }
            if (records.isNotEmpty()) current.sendNativeTouch(records)
        }
        touchSlots.clear()
        nativeTouchMotionTracker.clear()
    }

    fun dispatchExternalMouseTouch(event: MotionEvent, width: Int, height: Int): Boolean {
        if (streamUiActive) return false
        if (!event.isExternalMousePointerEvent()) return false
        if (mouseDirectClick) return false // Handled in dispatchTouch instead
        if (shouldPassTouchToNativeUi(event, width, height)) return false
        return client?.dispatchMotion(event) == true
    }

    fun dispatchKey(event: KeyEvent): Boolean {
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && event.isStreamSystemMenuKey()) {
            systemMenuHandler?.invoke()
            return systemMenuHandler != null
        }
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && event.isStreamControlsShortcutKey()) {
            systemMenuHandler?.invoke()
            return systemMenuHandler != null
        }
        val streamExitShortcut = event.isStreamExitShortcutKey()
        if (
            androidTvProfile &&
            event.action == KeyEvent.ACTION_DOWN &&
            event.repeatCount == 0 &&
            (event.keyCode == KeyEvent.KEYCODE_BACK || event.keyCode == KeyEvent.KEYCODE_BUTTON_B)
        ) {
            NativeInputDiagnostics.add(
                "tv back key key=${event.keyCode} source=${event.source} device=${event.deviceId}:${event.device?.name.orEmpty()} " +
                    "controller=${event.isControllerInputDevice()} dpad=${event.isDpadSource()} " +
                    "route=${if (streamExitShortcut) "stream_overlay" else "cloud_input"}",
            )
        }
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && streamExitShortcut) {
            return dispatchSystemBack()
        }
        if (event.action == KeyEvent.ACTION_UP && streamExitShortcut) {
            return systemBackHandler != null
        }
        if (streamUiActive) return false
        val current = client ?: return false
        if (event.shouldConsumeAsStreamKeyboard()) {
            return current.dispatchKey(event)
        }
        return current.dispatchKey(event)
    }

    fun dispatchMotion(event: MotionEvent): Boolean {
        if (streamUiActive && event.isExternalMousePointerEvent()) {
            return false
        }
        if (streamUiActive && event.isNativeUiNavigationMotion()) {
            return false
        }
        return client?.dispatchMotion(event) == true
    }

    private fun KeyEvent.isStreamSystemMenuKey(): Boolean =
        shouldOpenStreamSystemMenuKey(
            keyCode = keyCode,
            controllerInputDevice = isControllerInputDevice(),
            androidTvProfile = androidTvProfile,
        )

    private fun KeyEvent.isStreamControlsShortcutKey(): Boolean =
        !streamUiActive &&
            !isControllerInputDevice() &&
            !isHardwareKeyboardSource() &&
            isDpadSource() &&
            (keyCode == KeyEvent.KEYCODE_ENTER ||
                keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER ||
                keyCode == KeyEvent.KEYCODE_DPAD_CENTER)

    private fun KeyEvent.isStreamExitShortcutKey(): Boolean =
        shouldHandleStreamExitKey(
            keyCode = keyCode,
            controllerInputDevice = isControllerInputDevice(),
            hardwareKeyboardSource = isHardwareKeyboardSource(),
            androidTvProfile = androidTvProfile,
            dpadSource = isDpadSource(),
        )

    fun shouldOpenStreamSystemMenuKey(
        keyCode: Int,
        controllerInputDevice: Boolean,
        androidTvProfile: Boolean = false,
    ): Boolean =
        (keyCode == KeyEvent.KEYCODE_MENU && !controllerInputDevice) ||
            // Android TV remotes usually have no MENU key and many are reported as controller
            // devices; the Guide button is the only dedicated "open menu" affordance there.
            (androidTvProfile && keyCode == KeyEvent.KEYCODE_BUTTON_MODE)

    fun shouldHandleStreamExitKey(
        keyCode: Int,
        controllerInputDevice: Boolean,
        hardwareKeyboardSource: Boolean,
        androidTvProfile: Boolean = false,
        dpadSource: Boolean = false,
    ): Boolean =
        (keyCode == KeyEvent.KEYCODE_BACK && !controllerInputDevice) ||
            (androidTvProfile &&
                dpadSource &&
                keyCode == KeyEvent.KEYCODE_BUTTON_B &&
                !controllerInputDevice) ||
            (keyCode == KeyEvent.KEYCODE_ESCAPE && !hardwareKeyboardSource)

    private fun KeyEvent.isControllerInputDevice(): Boolean =
        AndroidControllerInput.isControllerEvent(source, deviceId)

    private fun KeyEvent.isDpadSource(): Boolean =
        (source and InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD

    private fun KeyEvent.isHardwareKeyboardSource(): Boolean =
        !isControllerInputDevice() &&
            ((source and InputDevice.SOURCE_KEYBOARD) == InputDevice.SOURCE_KEYBOARD ||
                InputDevice.getDevice(deviceId)?.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC)

    private fun KeyEvent.shouldConsumeAsStreamKeyboard(): Boolean =
        (action == KeyEvent.ACTION_DOWN || action == KeyEvent.ACTION_UP) &&
            !isControllerInputDevice() &&
            !isAndroidSystemKey() &&
            (isHardwareKeyboardSource() || keyCode.isTextEntryKeyCode())

    private fun KeyEvent.isAndroidSystemKey(): Boolean =
        keyCode == KeyEvent.KEYCODE_VOLUME_UP ||
            keyCode == KeyEvent.KEYCODE_VOLUME_DOWN ||
            keyCode == KeyEvent.KEYCODE_VOLUME_MUTE ||
            keyCode == KeyEvent.KEYCODE_POWER ||
            keyCode == KeyEvent.KEYCODE_HOME

    private fun Int.isKeyboardLikeKeyCode(): Boolean =
        this == KeyEvent.KEYCODE_ENTER ||
            this == KeyEvent.KEYCODE_NUMPAD_ENTER ||
            this == KeyEvent.KEYCODE_ESCAPE ||
            this == KeyEvent.KEYCODE_DEL ||
            this == KeyEvent.KEYCODE_TAB ||
            this == KeyEvent.KEYCODE_SPACE ||
            this == KeyEvent.KEYCODE_DPAD_LEFT ||
            this == KeyEvent.KEYCODE_DPAD_UP ||
            this == KeyEvent.KEYCODE_DPAD_RIGHT ||
            this == KeyEvent.KEYCODE_DPAD_DOWN ||
            this == KeyEvent.KEYCODE_PAGE_UP ||
            this == KeyEvent.KEYCODE_PAGE_DOWN ||
            this == KeyEvent.KEYCODE_FORWARD_DEL ||
            this == KeyEvent.KEYCODE_INSERT ||
            this == KeyEvent.KEYCODE_MOVE_HOME ||
            this == KeyEvent.KEYCODE_MOVE_END ||
            this == KeyEvent.KEYCODE_SHIFT_LEFT ||
            this == KeyEvent.KEYCODE_SHIFT_RIGHT ||
            this == KeyEvent.KEYCODE_CTRL_LEFT ||
            this == KeyEvent.KEYCODE_CTRL_RIGHT ||
            this == KeyEvent.KEYCODE_ALT_LEFT ||
            this == KeyEvent.KEYCODE_ALT_RIGHT ||
            this == KeyEvent.KEYCODE_CAPS_LOCK ||
            this == KeyEvent.KEYCODE_NUM_LOCK ||
            this == KeyEvent.KEYCODE_SCROLL_LOCK ||
            this == KeyEvent.KEYCODE_MINUS ||
            this == KeyEvent.KEYCODE_EQUALS ||
            this == KeyEvent.KEYCODE_LEFT_BRACKET ||
            this == KeyEvent.KEYCODE_RIGHT_BRACKET ||
            this == KeyEvent.KEYCODE_BACKSLASH ||
            this == KeyEvent.KEYCODE_SEMICOLON ||
            this == KeyEvent.KEYCODE_APOSTROPHE ||
            this == KeyEvent.KEYCODE_COMMA ||
            this == KeyEvent.KEYCODE_PERIOD ||
            this == KeyEvent.KEYCODE_SLASH ||
            this == KeyEvent.KEYCODE_GRAVE ||
            this in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z ||
            this in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 ||
            this in KeyEvent.KEYCODE_NUMPAD_0..KeyEvent.KEYCODE_NUMPAD_9 ||
            this in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12

    private fun Int.isTextEntryKeyCode(): Boolean =
        this == KeyEvent.KEYCODE_ENTER ||
            this == KeyEvent.KEYCODE_NUMPAD_ENTER ||
            this == KeyEvent.KEYCODE_ESCAPE ||
            this == KeyEvent.KEYCODE_DEL ||
            this == KeyEvent.KEYCODE_TAB ||
            this == KeyEvent.KEYCODE_SPACE ||
            this == KeyEvent.KEYCODE_FORWARD_DEL ||
            this == KeyEvent.KEYCODE_SHIFT_LEFT ||
            this == KeyEvent.KEYCODE_SHIFT_RIGHT ||
            this == KeyEvent.KEYCODE_CTRL_LEFT ||
            this == KeyEvent.KEYCODE_CTRL_RIGHT ||
            this == KeyEvent.KEYCODE_ALT_LEFT ||
            this == KeyEvent.KEYCODE_ALT_RIGHT ||
            this == KeyEvent.KEYCODE_CAPS_LOCK ||
            this == KeyEvent.KEYCODE_MINUS ||
            this == KeyEvent.KEYCODE_EQUALS ||
            this == KeyEvent.KEYCODE_LEFT_BRACKET ||
            this == KeyEvent.KEYCODE_RIGHT_BRACKET ||
            this == KeyEvent.KEYCODE_BACKSLASH ||
            this == KeyEvent.KEYCODE_SEMICOLON ||
            this == KeyEvent.KEYCODE_APOSTROPHE ||
            this == KeyEvent.KEYCODE_COMMA ||
            this == KeyEvent.KEYCODE_PERIOD ||
            this == KeyEvent.KEYCODE_SLASH ||
            this == KeyEvent.KEYCODE_GRAVE ||
            this in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z ||
            this in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 ||
            this in KeyEvent.KEYCODE_NUMPAD_0..KeyEvent.KEYCODE_NUMPAD_9 ||
            this in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12

    private fun MotionEvent.isNativeUiNavigationMotion(): Boolean =
        isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            isFromSource(InputDevice.SOURCE_GAMEPAD) ||
            AndroidControllerInput.isControllerEvent(source, deviceId)

    private fun MotionEvent.isFromSource(source: Int): Boolean = (this.source and source) == source

    private fun MotionEvent.isFingerTouchEvent(): Boolean =
        isFromSource(InputDevice.SOURCE_TOUCHSCREEN) &&
            !isFromSource(InputDevice.SOURCE_MOUSE) &&
            !isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE)

    private fun MotionEvent.isExternalMousePointerEvent(): Boolean {
        val controllerSource = isFromSource(InputDevice.SOURCE_JOYSTICK) || isFromSource(InputDevice.SOURCE_GAMEPAD)
        return isFromSource(InputDevice.SOURCE_MOUSE) ||
            isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE) ||
            (isFromSource(InputDevice.SOURCE_TOUCHPAD) && !controllerSource)
    }

    private fun shouldPassTouchToNativeUi(event: MotionEvent, width: Int, height: Int): Boolean {
        if (event.isFingerTouchEvent()) {
            updateNativeUiTouchPointers(event, width, height)
            return eventHasNativeUiTouchPointer(event, width, height) &&
                !eventHasStreamTouchPointer(event, width, height)
        }
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                nativeUiTouchRouting.setLegacyPassthroughActive(
                    pointerTouchesNativeUi(event, 0, width, height),
                )
                return nativeUiTouchRouting.passthroughActive
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val wasActive = nativeUiTouchRouting.passthroughActive
                nativeUiTouchRouting.setLegacyPassthroughActive(false)
                return wasActive
            }
            else -> if (nativeUiTouchRouting.passthroughActive) {
                return true
            }
        }
        return false
    }

    private fun updateNativeUiTouchPointers(event: MotionEvent, width: Int, height: Int) {
        if (!event.isFingerTouchEvent()) return
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                nativeUiTouchRouting.beginPointerGesture(
                    pointerId = event.getPointerId(0),
                    touchesUi = pointerTouchesNativeUi(event, 0, width, height),
                )
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                val index = event.actionIndex
                if (index in 0 until event.pointerCount) {
                    nativeUiTouchRouting.addPointer(
                        pointerId = event.getPointerId(index),
                        touchesUi = pointerTouchesNativeUi(event, index, width, height),
                    )
                }
            }
        }
    }

    fun postDispatchTouch(event: MotionEvent) {
        if (!event.isFingerTouchEvent()) return
        when (event.actionMasked) {
            MotionEvent.ACTION_POINTER_UP -> {
                val index = event.actionIndex
                if (index in 0 until event.pointerCount) {
                    nativeUiTouchRouting.releasePointer(event.getPointerId(index))
                }
            }
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL -> {
                nativeUiTouchRouting.endPointerGesture()
            }
        }
    }

    private fun eventHasNativeUiTouchPointer(event: MotionEvent, width: Int, height: Int): Boolean =
        (0 until event.pointerCount).any { index ->
            isNativeUiTouchPointer(event, index, width, height)
        }

    private fun eventHasStreamTouchPointer(event: MotionEvent, width: Int, height: Int): Boolean =
        (0 until event.pointerCount).any { index ->
            !isNativeUiTouchPointer(event, index, width, height)
        }

    private fun isNativeUiTouchPointer(event: MotionEvent, index: Int, width: Int, height: Int): Boolean =
        nativeUiTouchRouting.classifiesPointerAsUi(
            pointerId = event.getPointerId(index),
            touchesUiNow = pointerTouchesNativeUi(event, index, width, height),
        )

    private fun pointerTouchesNativeUi(event: MotionEvent, index: Int, width: Int, height: Int): Boolean {
        if (index !in 0 until event.pointerCount) return false
        val x = event.getX(index)
        val y = event.getY(index)

        // Narrow edge exclusion zone: 12dp is enough to capture system back-swipe gestures while
        // still allowing game UI elements placed near the screen edges to be reached.
        val density = android.content.res.Resources.getSystem().displayMetrics.density
        val edgeWidthPx = 12f * density
        val isNearEdge = !androidTvProfile && width > 0 && (x < edgeWidthPx || x > width - edgeWidthPx)
        if (isNearEdge) return true

        return nativeUiTouchRouting.touchesRegisteredUi(x, y, width, height)
    }
}

enum class InputDataChannelRole {
    Reliable,
    PartiallyReliable,
    Other,
}

object InputDataChannelLabels {
    fun classify(label: String): InputDataChannelRole =
        when (label.lowercase(Locale.US)) {
            "input_channel_v1",
            "input_channel",
            -> InputDataChannelRole.Reliable
            "input_channel_partially_reliable",
            "input_channel_pr",
            -> InputDataChannelRole.PartiallyReliable
            else -> InputDataChannelRole.Other
        }
}

internal enum class AndroidControllerFamily {
    Google,
    Xbox,
    PlayStation,
    Nintendo,
    Generic,
}

internal fun androidGamepadConnectionBitmap(
    controllerId: Int,
    connected: Boolean,
    physicalControllerFamily: AndroidControllerFamily?,
    playStationRumbleCompatibility: Boolean = false,
): Int {
    if (!connected) return 0
    val id = controllerId.coerceIn(0, 3)
    val connectedBit = 1 shl id
    // The host protocol uses bit (slot + 8) to distinguish an Xbox/XInput-style pad from the
    // PlayStation-style identity used by its native controller mapping. Unknown controllers and
    // OpenNOW's virtual pad retain the established XInput fallback for compatibility.
    val xinputStyleBit = if (
        physicalControllerFamily == AndroidControllerFamily.PlayStation &&
        !playStationRumbleCompatibility
    ) {
        0
    } else {
        1 shl (id + 8)
    }
    return connectedBit or xinputStyleBit
}

internal object AndroidControllerInput {
    fun hasControllerSource(source: Int): Boolean =
        source.hasSource(InputDevice.SOURCE_GAMEPAD) ||
            source.hasSource(InputDevice.SOURCE_JOYSTICK)

    fun isControllerDevice(device: InputDevice?): Boolean =
        device != null && isControllerDevice(device.sources, device.name)

    fun isControllerDevice(source: Int, deviceName: String?): Boolean {
        val knownController = isKnownControllerName(deviceName)
        // Some OEM inputs and Bluetooth/USB receivers expose stray GAMEPAD or JOYSTICK source
        // bits. Advertising those idle interfaces as an XInput pad makes games switch away from
        // mouse/keyboard even though no controller exists.
        if (!knownController && isClearlyNotController(deviceName)) return false
        return hasControllerSource(source) ||
            (source.hasSource(InputDevice.SOURCE_DPAD) && knownController)
    }

    fun isControllerEvent(source: Int, deviceId: Int): Boolean {
        val device = InputDevice.getDevice(deviceId)
        return if (device != null) {
            isControllerDevice(device.sources or source, device.name)
        } else {
            hasControllerSource(source)
        }
    }

    fun isKnownControllerName(name: String?): Boolean {
        val normalized = name.orEmpty().lowercase(Locale.US)
        return normalized.contains("stadia controller") ||
            normalized == "stadia" ||
            normalized.contains("google stadia") ||
            normalized.contains("dualsense") ||
            normalized.contains("dualshock") ||
            normalized.contains("wireless controller") ||
            normalized.contains("xbox") ||
            normalized.contains("x-input") ||
            normalized.contains("xinput") ||
            normalized.contains("8bitdo") ||
            normalized.contains("gamesir") ||
            normalized.contains("backbone") ||
            normalized.contains("razer kishi") ||
            normalized.contains("switch pro") ||
            normalized.contains("gamepad")
    }

    private fun isClearlyNotController(name: String?): Boolean {
        val normalized = name.orEmpty().lowercase(Locale.US)
        return normalized.contains("keyboard") ||
            normalized.contains("mouse") ||
            normalized.contains("touchpad") ||
            normalized.contains("trackpad") ||
            normalized.contains("uinput-goodix") ||
            normalized.contains("fingerprint") ||
            normalized.contains("uinput-fpc")
    }

    fun controllerFamily(device: InputDevice?): AndroidControllerFamily? =
        device
            ?.takeIf(::isControllerDevice)
            ?.let { controllerFamily(it.name, it.vendorId) }

    internal fun controllerFamily(name: String?, vendorId: Int = 0): AndroidControllerFamily {
        val normalized = name.orEmpty().lowercase(Locale.US)
        return when {
            vendorId == SONY_VENDOR_ID -> AndroidControllerFamily.PlayStation
            normalized.contains("stadia") ||
                normalized.contains("google") ||
                normalized.contains("chromecast") -> AndroidControllerFamily.Google
            normalized.contains("xbox") ||
                normalized.contains("x-input") ||
                normalized.contains("xinput") -> AndroidControllerFamily.Xbox
            normalized.contains("dualsense") ||
                normalized.contains("dualshock") ||
                normalized.contains("playstation") ||
                normalized.contains("wireless controller") -> AndroidControllerFamily.PlayStation
            normalized.contains("switch") || normalized.contains("nintendo") -> AndroidControllerFamily.Nintendo
            else -> AndroidControllerFamily.Generic
        }
    }

    fun isPrimaryActivationKey(keyCode: Int): Boolean =
        keyCode == KeyEvent.KEYCODE_DPAD_CENTER ||
            keyCode == KeyEvent.KEYCODE_ENTER ||
            keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER

    private fun Int.hasSource(source: Int): Boolean = (this and source) == source

    private const val SONY_VENDOR_ID = 0x054c
}

internal data class AndroidControllerSlotAssignment(
    val slot: Int,
    val removedDevices: Map<Int, Int>,
)

internal object AndroidControllerSlotRegistry {
    fun retainConnected(
        controllerSlots: MutableMap<Int, Int>,
        connectedDeviceIds: Set<Int>,
    ): Map<Int, Int> {
        val removedDevices = controllerSlots.filterKeys { it !in connectedDeviceIds }
        removedDevices.keys.forEach(controllerSlots::remove)
        return removedDevices
    }

    fun assign(
        controllerSlots: MutableMap<Int, Int>,
        deviceId: Int,
        connectedDeviceIds: Set<Int>,
        maxControllers: Int,
    ): AndroidControllerSlotAssignment {
        // Some Android controller stacks deliver synthetic events with deviceId=-1 even though
        // the real InputDevice remains connected. Bind those events to a live controller ID so
        // the periodic connection scan does not delete the synthetic slot every second.
        val stableDeviceId = when {
            deviceId >= 0 -> deviceId
            else -> controllerSlots.entries
                .filter { it.key in connectedDeviceIds }
                .minByOrNull { it.value }
                ?.key
                ?: connectedDeviceIds.minOrNull()
                ?: deviceId
        }
        val removedDevices = retainConnected(
            controllerSlots = controllerSlots,
            connectedDeviceIds = connectedDeviceIds + stableDeviceId,
        )
        val existingSlot = controllerSlots[stableDeviceId]
        if (existingSlot != null) {
            return AndroidControllerSlotAssignment(existingSlot, removedDevices)
        }
        val usedSlots = controllerSlots.values.toSet()
        val slot = (0 until maxControllers).firstOrNull { it !in usedSlots } ?: 0
        controllerSlots[stableDeviceId] = slot
        return AndroidControllerSlotAssignment(slot, removedDevices)
    }
}

internal data class ControllerMouseDelta(
    val dx: Int,
    val dy: Int,
)

internal fun shouldSendGamepadKeepalive(
    hasControllerState: Boolean,
    hasActiveControllerInput: Boolean,
    touchMouseEnabled: Boolean,
): Boolean = hasControllerState && (!touchMouseEnabled || hasActiveControllerInput)

internal object AndroidControllerMouseAssist {
    fun mouseDelta(stickX: Float, stickY: Float): ControllerMouseDelta? {
        if (!stickX.isFinite() || !stickY.isFinite()) return null
        val x = stickX.coerceIn(-1f, 1f)
        val y = stickY.coerceIn(-1f, 1f)
        val magnitude = sqrt(x * x + y * y).coerceIn(0f, 1f)
        if (magnitude < 0.001f) return null
        val speed = CONTROLLER_MOUSE_BASE_DELTA_PX + CONTROLLER_MOUSE_ACCEL_DELTA_PX * magnitude * magnitude
        val dx = (x * speed).roundToInt()
        val dy = (y * speed).roundToInt()
        return if (dx != 0 || dy != 0) ControllerMouseDelta(dx, dy) else null
    }

    fun scrollNotches(stickY: Float, scrollSensitivity: Int, accumulator: Float): Pair<Int, Float> {
        if (!stickY.isFinite() || !accumulator.isFinite()) return Pair(0, 0f)
        val y = stickY.coerceIn(-1f, 1f)
        if (abs(y) < 0.1f) return Pair(0, accumulator)

        val sensitivity = scrollSensitivity.toFloat().coerceIn(10f, 100f)
        val factor = 6.0f / sensitivity
        val nextAccumulator = accumulator + -y * factor
        val notches = nextAccumulator.toInt()
        return Pair(notches, nextAccumulator - notches)
    }

    fun mouseButtonForGamepad(buttonMask: Int): Int? =
        when (buttonMask) {
            GamepadButtonMapping.A -> 1
            GamepadButtonMapping.B -> 3
            else -> null
        }

    fun mouseButtonForTrigger(left: Boolean): Int? = null

    private const val CONTROLLER_MOUSE_BASE_DELTA_PX = 7f
    private const val CONTROLLER_MOUSE_ACCEL_DELTA_PX = 34f
}

internal data class AndroidGamepadRawAxes(
    val x: Float = 0f,
    val y: Float = 0f,
    val z: Float = 0f,
    val rz: Float = 0f,
    val rx: Float = 0f,
    val ry: Float = 0f,
    val hatX: Float = 0f,
    val hatY: Float = 0f,
)

internal data class AndroidGamepadAxisAvailability(
    val x: Boolean = true,
    val y: Boolean = true,
    val z: Boolean = true,
    val rz: Boolean = true,
    val rx: Boolean = true,
    val ry: Boolean = true,
    val hatX: Boolean = true,
    val hatY: Boolean = true,
) {
    fun hasLeftStickPair(): Boolean = x && y
    fun hasHatPair(): Boolean = hatX && hatY
}

internal data class AndroidGamepadResolvedAxes(
    val leftX: Float,
    val leftY: Float,
    val rightX: Float,
    val rightY: Float,
    val leftSource: String,
    val rightSource: String,
    val hatUsedAsLeftStick: Boolean,
)

internal object AndroidGamepadAxisMapping {
    fun resolve(raw: AndroidGamepadRawAxes, available: AndroidGamepadAxisAvailability = AndroidGamepadAxisAvailability()): AndroidGamepadResolvedAxes {
        val rightUsesZRz = axisPairActive(raw.z, raw.rz) || !axisPairActive(raw.rx, raw.ry)
        val rightX = if (rightUsesZRz) raw.z else raw.rx
        val rightY = if (rightUsesZRz) raw.rz else raw.ry
        val rightSource = if (rightUsesZRz) "z/rz" else "rx/ry"

        val useHatForLeft =
            !available.hasLeftStickPair() &&
                available.hasHatPair() &&
                axisPairActive(raw.hatX, raw.hatY)
        val leftX = if (useHatForLeft) raw.hatX else raw.x
        val leftY = if (useHatForLeft) raw.hatY else raw.y
        return AndroidGamepadResolvedAxes(
            leftX = leftX,
            leftY = leftY,
            rightX = rightX,
            rightY = rightY,
            leftSource = if (useHatForLeft) "hat" else "x/y",
            rightSource = rightSource,
            hatUsedAsLeftStick = useHatForLeft,
        )
    }

    private fun axisPairActive(x: Float, y: Float): Boolean =
        abs(x) > AXIS_NOISE || abs(y) > AXIS_NOISE

    private const val AXIS_NOISE = 0.001f
}

internal data class GamepadRumbleCommand(
    val controllerId: Int,
    val weakMagnitude: Int,
    val strongMagnitude: Int,
)

internal object HapticsPacketParser {
    fun parse(bytes: ByteArray, controllerBase: Int = 6): GamepadRumbleCommand? {
        if (bytes.size < 2) return null
        val view = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val firstWord = view.getShort(0).toInt() and 0xffff
        if (firstWord == LEGACY_HAPTIC_SUBMESSAGE_TYPE) {
            return parseLegacy(view, 2)
        }

        return when (firstWord and 0xff) {
            WRAPPER_SINGLE_EVENT -> parseSubMessage(view, 1, controllerBase)
            WRAPPER_BATCHED_EVENT,
            WRAPPER_LEGACY_INPUT,
            WRAPPER_TIMESTAMPED_SINGLE,
            WRAPPER_TIMESTAMPED_BATCHED,
            WRAPPER_RESERVED,
            -> null
            else -> parseLegacy(view, 0)
        }
    }

    private fun parseSubMessage(view: ByteBuffer, offset: Int, controllerBase: Int): GamepadRumbleCommand? {
        if (offset < 0 || offset + 4 > view.limit()) return null
        val type = view.getInt(offset)
        return when (type) {
            LEGACY_HAPTIC_SUBMESSAGE_TYPE -> parseLegacy(view, offset + 4)
            OC_HAPTIC_SUBMESSAGE_TYPE -> parseOc(view, offset + 4, controllerBase)
            else -> null
        }
    }

    private fun parseLegacy(view: ByteBuffer, offset: Int): GamepadRumbleCommand? {
        if (offset < 0 || offset + 10 > view.limit()) return null
        val kind = view.getShort(offset).toInt() and 0xffff
        if (kind != 1) return null
        val length = view.getShort(offset + 2).toInt() and 0xffff
        if (length < 6) return null
        return GamepadRumbleCommand(
            controllerId = view.getShort(offset + 4).toInt() and 0xffff,
            weakMagnitude = view.getShort(offset + 6).toInt() and 0xffff,
            strongMagnitude = view.getShort(offset + 8).toInt() and 0xffff,
        )
    }

    private fun parseOc(view: ByteBuffer, offset: Int, controllerBase: Int): GamepadRumbleCommand? {
        if (offset < 0 || offset + 9 > view.limit()) return null
        val controllerByte = view.get(offset).toInt() and 0xff
        if (controllerByte !in controllerBase until controllerBase + 4) return null
        val reportKind = view.get(offset + 3).toInt() and 0xff
        val flags = view.get(offset + 4).toInt() and 0xff
        if (reportKind != 5 || (flags and 0xfe) != 0) return null
        return GamepadRumbleCommand(
            controllerId = controllerByte - controllerBase,
            weakMagnitude = (view.get(offset + 7).toInt() and 0xff) shl 8,
            strongMagnitude = (view.get(offset + 8).toInt() and 0xff) shl 8,
        )
    }

    private const val LEGACY_HAPTIC_SUBMESSAGE_TYPE = 267
    private const val OC_HAPTIC_SUBMESSAGE_TYPE = 17
    private const val WRAPPER_BATCHED_EVENT = 32
    private const val WRAPPER_LEGACY_INPUT = 33
    private const val WRAPPER_SINGLE_EVENT = 34
    private const val WRAPPER_TIMESTAMPED_SINGLE = 35
    private const val WRAPPER_TIMESTAMPED_BATCHED = 36
    private const val WRAPPER_RESERVED = 255
}

internal object GamepadButtonMapping {
    const val DPAD_UP = 0x0001
    const val DPAD_DOWN = 0x0002
    const val DPAD_LEFT = 0x0004
    const val DPAD_RIGHT = 0x0008
    const val START = 0x0010
    const val BACK = 0x0020
    const val LEFT_THUMB = 0x0040
    const val RIGHT_THUMB = 0x0080
    const val LEFT_SHOULDER = 0x0100
    const val RIGHT_SHOULDER = 0x0200
    const val GUIDE = 0x0400
    const val A = 0x1000
    const val B = 0x2000
    const val X = 0x4000
    const val Y = 0x8000

    fun maskForKeyCode(keyCode: Int, controllerActivation: Boolean = false): Int? = when (keyCode) {
        KeyEvent.KEYCODE_MENU -> if (controllerActivation) START else null
        KeyEvent.KEYCODE_BACK -> if (controllerActivation) BACK else null
        KeyEvent.KEYCODE_DPAD_UP -> DPAD_UP
        KeyEvent.KEYCODE_DPAD_DOWN -> DPAD_DOWN
        KeyEvent.KEYCODE_DPAD_LEFT -> DPAD_LEFT
        KeyEvent.KEYCODE_DPAD_RIGHT -> DPAD_RIGHT
        KeyEvent.KEYCODE_DPAD_CENTER,
        KeyEvent.KEYCODE_ENTER,
        KeyEvent.KEYCODE_NUMPAD_ENTER,
        -> if (controllerActivation) A else null
        KeyEvent.KEYCODE_BUTTON_START -> START
        KeyEvent.KEYCODE_BUTTON_SELECT -> BACK
        KeyEvent.KEYCODE_BUTTON_THUMBL -> LEFT_THUMB
        KeyEvent.KEYCODE_BUTTON_THUMBR -> RIGHT_THUMB
        KeyEvent.KEYCODE_BUTTON_L1 -> LEFT_SHOULDER
        KeyEvent.KEYCODE_BUTTON_R1 -> RIGHT_SHOULDER
        KeyEvent.KEYCODE_BUTTON_MODE -> GUIDE
        KeyEvent.KEYCODE_BUTTON_A -> A
        KeyEvent.KEYCODE_BUTTON_B -> B
        KeyEvent.KEYCODE_BUTTON_X -> X
        KeyEvent.KEYCODE_BUTTON_Y -> Y
        else -> null
    }

    fun isControllerButtonKeyCode(keyCode: Int): Boolean =
        keyCode == KeyEvent.KEYCODE_BUTTON_L2 ||
            keyCode == KeyEvent.KEYCODE_BUTTON_R2 ||
            keyCode in KeyEvent.KEYCODE_BUTTON_A..KeyEvent.KEYCODE_BUTTON_MODE
}

internal object SteamMenuChord {
    // Send only the Guide (Home) button. The GUIDE+A chord previously used caused unintended
    // A-button input during gameplay; a plain Guide press is sufficient to open Steam overlay.
    fun buttons(aPressed: Boolean): Int = GamepadButtonMapping.GUIDE
}

internal class SteamOverlayChordState {
    private var latched = false
    private var chordPressed = false

    fun update(rawButtons: Int): Boolean {
        val topButtons = rawButtons and TOP_BUTTONS
        val activated = !latched && topButtons == TOP_BUTTONS
        if (activated) {
            latched = true
            chordPressed = true
        } else if (latched && topButtons == 0) {
            latched = false
            chordPressed = false
        }
        return activated
    }

    fun effectiveButtons(rawButtons: Int): Int {
        val withoutTopButtons = if (latched) rawButtons and TOP_BUTTONS.inv() else rawButtons
        return if (chordPressed) withoutTopButtons or SteamMenuChord.buttons(aPressed = true) else withoutTopButtons
    }

    fun releaseChord(): Boolean {
        if (!chordPressed) return false
        chordPressed = false
        return true
    }

    fun reset() {
        latched = false
        chordPressed = false
    }

    private companion object {
        const val TOP_BUTTONS = 0x0030
    }
}
