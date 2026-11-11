package com.opencloudgaming.opennow

import android.view.MotionEvent
import org.junit.Assert.*
import org.junit.Test

class TouchMouseStateInstrumentedTest {
    private class Output : TouchMouseOutput {
        override var settings = StreamSettings(mouseAcceleration = 1)
        val moves = mutableListOf<Pair<Int, Int>>()
        val positions = mutableListOf<AbsoluteMousePosition>()
        val buttons = mutableListOf<Boolean>()
        var taps = 0
        var rightTaps = 0
        var scroll = 0
        var positionAccepted = true
        override fun setTouchMouseButton(pressed: Boolean): Boolean { buttons += pressed; return true }
        override fun setDirectClickMouseButton(pressed: Boolean) = setTouchMouseButton(pressed)
        override fun sendDirectClickMousePosition(position: AbsoluteMousePosition): Boolean { positions += position; return positionAccepted }
        override fun sendDirectClickMouseClick(position: AbsoluteMousePosition) {
            if (sendDirectClickMousePosition(position)) sendTouchMouseClick()
        }
        override fun sendRawMouseMove(dx: Int, dy: Int): Boolean { moves += dx to dy; return true }
        override fun sendTouchMouseClick() { taps++ }
        override fun sendTouchMouseRightClick() { rightTaps++ }
        override fun sendTouchMouseWheel(delta: Int) { scroll += delta }
    }

    private fun TouchMouseState.event(
        output: Output,
        action: Int,
        time: Long,
        x: Float = 100f,
        y: Float = 100f,
        buttonsEnabled: Boolean = false,
        direct: Boolean = false,
        combined: Boolean = false,
        source: Int = android.view.InputDevice.SOURCE_TOUCHSCREEN,
    ) {
        val event = MotionEvent.obtain(time, time, action, x, y, 0)
        event.source = source
        try {
            handle(event, true, output, emptySet(), directClick = direct, experimentalSlideDirectClick = combined, buttonsEnabled = buttonsEnabled,
                width = 1920, height = 1080, decodedStreamWidth = 1920, decodedStreamHeight = 1080)
        } finally { event.recycle() }
    }

    @Test fun liftingAFingerDoesNotMoveTheCursorOrTurnATapIntoMotion() {
        for (buttons in listOf(false, true)) {
            val output = Output()
            val state = TouchMouseState()
            state.event(output, MotionEvent.ACTION_DOWN, 1000, buttonsEnabled = buttons)
            state.event(output, MotionEvent.ACTION_UP, 1040, x = 106f, y = 97f, buttonsEnabled = buttons)
            assertTrue(output.moves.isEmpty())
            assertEquals(if (buttons) 1 else 0, output.taps)
            state.event(output, MotionEvent.ACTION_DOWN, 2000, buttonsEnabled = buttons)
            state.event(output, MotionEvent.ACTION_MOVE, 2020, x = 150f, buttonsEnabled = buttons)
            val beforeRelease = output.moves.toList()
            state.event(output, MotionEvent.ACTION_UP, 2040, x = 156f, y = 97f, buttonsEnabled = buttons)
            assertEquals(beforeRelease, output.moves)
            assertEquals(50, output.moves.sumOf { it.first })
        }
    }

    @Test fun liftingFlushesOnlyMotionAlreadyRecordedDuringMoves() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000)
        state.event(output, MotionEvent.ACTION_MOVE, 1020, x = 120f)
        state.event(output, MotionEvent.ACTION_MOVE, 1021, x = 125f)
        assertEquals(listOf(20 to 0), output.moves)
        state.event(output, MotionEvent.ACTION_UP, 1022, x = 135f, y = 90f)
        assertEquals(listOf(20 to 0, 5 to 0), output.moves)
        state.event(output, MotionEvent.ACTION_DOWN, 2000)
        state.event(output, MotionEvent.ACTION_UP, 2040)
        assertEquals(25, output.moves.sumOf { it.first })
    }

    @Test fun liftingEndsADoubleTapDragWithoutAddingMotion() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_UP, 1040, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_DOWN, 1100, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_MOVE, 1120, x = 150f, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_UP, 1140, x = 160f, buttonsEnabled = true)
        assertEquals(listOf(50 to 0), output.moves)
        assertEquals(listOf(true, false), output.buttons)
        assertEquals(1, output.taps)
    }

    @Test fun directFingerReleaseKeepsTheLastPositionAndReleasesTheButton() {
        for (buttons in listOf(false, true)) {
            val output = Output()
            val state = TouchMouseState()
            state.event(output, MotionEvent.ACTION_DOWN, 1000, direct = true, buttonsEnabled = buttons)
            state.event(output, MotionEvent.ACTION_MOVE, 1020, x = 150f, direct = true, buttonsEnabled = buttons)
            state.event(output, MotionEvent.ACTION_UP, 1040, x = 160f, direct = true, buttonsEnabled = buttons)
            assertEquals(listOf(100, 150), output.positions.map { it.x })
            assertEquals(if (buttons) listOf(true, false) else emptyList<Boolean>(), output.buttons)
        }
    }

    @Test fun externalDirectMouseStillPositionsAtRelease() {
        val output = Output()
        val state = TouchMouseState()
        for ((action, time, x) in listOf(
            Triple(MotionEvent.ACTION_DOWN, 1000L, 100f),
            Triple(MotionEvent.ACTION_MOVE, 1020L, 150f),
            Triple(MotionEvent.ACTION_UP, 1040L, 160f),
        )) {
            state.event(output, action, time, x = x, direct = true, buttonsEnabled = true,
                source = android.view.InputDevice.SOURCE_MOUSE)
        }
        assertEquals(listOf(100, 150, 160), output.positions.map { it.x })
        assertEquals(listOf(true, false), output.buttons)
    }

    @Test fun pointerUpBesideAnOverlayFingerDoesNotAddMotion() {
        for ((direct, combined) in listOf(false to false, true to false, true to true)) {
            val output = Output()
            val state = TouchMouseState()
            state.event(output, MotionEvent.ACTION_DOWN, 1000, direct = direct, combined = combined, buttonsEnabled = true)
            state.event(output, MotionEvent.ACTION_MOVE, 1020, x = 150f, direct = direct, combined = combined, buttonsEnabled = true)
            val properties = Array(2) { id -> MotionEvent.PointerProperties().apply {
                this.id = id; toolType = MotionEvent.TOOL_TYPE_FINGER
            } }
            val coordinates = Array(2) { id -> MotionEvent.PointerCoords().apply {
                x = if (id == 0) 160f else 500f; y = 100f; pressure = 1f; size = 1f
            } }
            val up = MotionEvent.obtain(1000, 1040, MotionEvent.ACTION_POINTER_UP, 2, properties, coordinates,
                0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
            try {
                state.handle(up, true, output, setOf(1), directClick = direct,
                    experimentalSlideDirectClick = combined, width = 1920, height = 1080,
                    decodedStreamWidth = 1920, decodedStreamHeight = 1080)
            } finally { up.recycle() }
            if (direct && !combined) {
                assertEquals(listOf(100, 150), output.positions.map { it.x })
                assertEquals(listOf(true, false), output.buttons)
            } else {
                assertEquals(listOf(50 to 0), output.moves)
                assertTrue(output.buttons.isEmpty())
            }
        }
    }

    @Test fun combinedModeSlidesRelativelyAndOnlyClicksAfterACompletedDoubleTap() {
        val output = Output()
        val state = TouchMouseState()
        fun event(action: Int, time: Long, x: Float = 100f) = state.event(
            output, action, time, x = x, buttonsEnabled = true, direct = true, combined = true,
        )
        event(MotionEvent.ACTION_DOWN, 1000)
        event(MotionEvent.ACTION_UP, 1040)
        event(MotionEvent.ACTION_DOWN, 1100, 105f)
        event(MotionEvent.ACTION_MOVE, 1110, 107f)
        assertTrue(output.moves.isEmpty())
        assertTrue(output.positions.isEmpty())
        assertTrue(output.buttons.isEmpty())
        event(MotionEvent.ACTION_UP, 1140, 108f)
        assertEquals(listOf(108), output.positions.map { it.x })
        assertTrue(output.buttons.isEmpty())
        assertEquals(1, output.taps)
        event(MotionEvent.ACTION_DOWN, 1200)
        event(MotionEvent.ACTION_MOVE, 1220, 160f)
        event(MotionEvent.ACTION_UP, 1240, 180f)
        assertEquals(60, output.moves.sumOf { it.first })
        assertEquals(1, output.positions.size)
        assertTrue(output.buttons.isEmpty())
        assertEquals(1, output.taps)
    }

    @Test fun combinedModeRejectsTapThenSlideEvenWithBatchedReturnToOrigin() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000, direct = true, combined = true, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_UP, 1040, direct = true, combined = true, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_DOWN, 1100, direct = true, combined = true, buttonsEnabled = true)
        val move = MotionEvent.obtain(1100, 1110, MotionEvent.ACTION_MOVE, 160f, 100f, 0)
        move.addBatch(1120, 100f, 100f, 1f, 1f, 0)
        try {
            state.handle(move, true, output, emptySet(), directClick = true, experimentalSlideDirectClick = true)
        } finally { move.recycle() }
        state.event(output, MotionEvent.ACTION_UP, 1140, direct = true, combined = true, buttonsEnabled = true)
        assertTrue(output.moves.isNotEmpty())
        assertTrue(output.positions.isEmpty())
        assertTrue(output.buttons.isEmpty())
    }

    @Test fun combinedModeRespectsCursorOnlyAndResetAndRequiresDirectClick() {
        val output = Output()
        val state = TouchMouseState()
        fun tap(time: Long, buttons: Boolean = true) {
            state.event(output, MotionEvent.ACTION_DOWN, time, direct = true, combined = true, buttonsEnabled = buttons)
            state.event(output, MotionEvent.ACTION_UP, time + 40, direct = true, combined = true, buttonsEnabled = buttons)
        }
        tap(1000)
        state.reset(output)
        tap(1100)
        assertTrue(output.positions.isEmpty())
        tap(1200, buttons = false)
        assertTrue(output.positions.isEmpty())
        tap(1300, buttons = false)
        assertEquals(listOf(AbsoluteMousePosition(100, 100, 1920, 1080)), output.positions)
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
        // With Direct click off, the experiment leaves ordinary touchpad taps intact.
        state.event(output, MotionEvent.ACTION_DOWN, 1400, combined = true, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_UP, 1440, combined = true, buttonsEnabled = true)
        assertEquals(1, output.taps)
        // With the experiment off, Direct click still presses immediately and releases on cancel.
        state.event(output, MotionEvent.ACTION_DOWN, 1500, direct = true, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_CANCEL, 1540, direct = true, buttonsEnabled = true)
        assertEquals(listOf(true, false), output.buttons)
    }

    @Test fun combinedCursorOnlyDoubleTapPositionsAtTheSecondTapAfterSliding() {
        val output = Output()
        val state = TouchMouseState()
        fun event(action: Int, time: Long, x: Float) = state.event(
            output, action, time, x = x, direct = true, combined = true, buttonsEnabled = false,
        )
        event(MotionEvent.ACTION_DOWN, 1000, 100f)
        event(MotionEvent.ACTION_MOVE, 1020, 150f)
        event(MotionEvent.ACTION_UP, 1040, 180f)
        assertEquals(50, output.moves.sumOf { it.first })
        event(MotionEvent.ACTION_DOWN, 1100, 700f)
        event(MotionEvent.ACTION_UP, 1140, 700f)
        assertTrue(output.positions.isEmpty())
        event(MotionEvent.ACTION_DOWN, 1200, 710f)
        event(MotionEvent.ACTION_UP, 1240, 712f)
        assertEquals(listOf(AbsoluteMousePosition(712, 100, 1920, 1080)), output.positions)
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
        // The completed pair is consumed; another isolated tap must not jump the cursor.
        event(MotionEvent.ACTION_DOWN, 1300, 720f)
        event(MotionEvent.ACTION_UP, 1340, 720f)
        assertEquals(1, output.positions.size)
    }

    @Test fun combinedModeScrollsWithoutClickingOrArmingADoubleTap() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000, buttonsEnabled = true, direct = true, combined = true)
        fun twoFingerEvent(action: Int, time: Long, secondY: Float) {
            val properties = Array(2) { id -> MotionEvent.PointerProperties().apply {
                this.id = id; toolType = MotionEvent.TOOL_TYPE_FINGER
            } }
            val coordinates = Array(2) { id -> MotionEvent.PointerCoords().apply {
                x = 100f + id * 50f; y = if (id == 0) 100f else secondY
                pressure = 1f; size = 1f
            } }
            val event = MotionEvent.obtain(1000, time, action, 2, properties, coordinates,
                0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
            try {
                state.handle(event, true, output, emptySet(), directClick = true, experimentalSlideDirectClick = true)
            } finally { event.recycle() }
        }
        twoFingerEvent(MotionEvent.ACTION_POINTER_DOWN or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 1050, 100f)
        twoFingerEvent(MotionEvent.ACTION_MOVE, 1100, 240f)
        twoFingerEvent(MotionEvent.ACTION_POINTER_UP or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 1150, 240f)
        state.event(output, MotionEvent.ACTION_UP, 1200, buttonsEnabled = true, direct = true, combined = true)
        state.event(output, MotionEvent.ACTION_DOWN, 1250, buttonsEnabled = true, direct = true, combined = true)
        state.event(output, MotionEvent.ACTION_UP, 1290, buttonsEnabled = true, direct = true, combined = true)
        assertTrue(output.scroll != 0)
        assertTrue(output.moves.isEmpty())
        assertTrue(output.positions.isEmpty())
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.rightTaps)
        assertEquals(0, output.taps)
    }

    @Test fun combinedModeDoesNotClickWhenAbsolutePositionCannotBeSent() {
        val output = Output().apply { positionAccepted = false }
        val state = TouchMouseState()
        for (time in listOf(1000L, 1100L)) {
            state.event(output, MotionEvent.ACTION_DOWN, time, buttonsEnabled = true, direct = true, combined = true)
            state.event(output, MotionEvent.ACTION_UP, time + 40, buttonsEnabled = true, direct = true, combined = true)
        }
        assertEquals(1, output.positions.size)
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
    }

    @Test fun touchSensitivityChangesCursorTravelAndCanUpdateDuringAGesture() {
        val output = Output()
        val state = TouchMouseState()
        output.settings = StreamSettings(mouseSensitivity = 3f, touchMouseSensitivity = 0.5f)
        state.event(output, MotionEvent.ACTION_DOWN, 1000)
        state.event(output, MotionEvent.ACTION_MOVE, 1100, x = 120f)
        assertEquals(10, output.moves.sumOf { it.first })
        output.settings = output.settings.copy(touchMouseSensitivity = 2f)
        state.event(output, MotionEvent.ACTION_MOVE, 1200, x = 140f)
        state.event(output, MotionEvent.ACTION_UP, 1300, x = 140f)
        assertEquals(50, output.moves.sumOf { it.first })
        assertTrue(output.buttons.isEmpty())
    }

    @Test fun keyboardMouseTapsAndRepeatedDragsOnlyMoveTheCursor() {
        val output = Output()
        val state = TouchMouseState()
        // A tap followed immediately by a drag used to arm double-tap dragging.
        state.event(output, MotionEvent.ACTION_DOWN, 1000)
        state.event(output, MotionEvent.ACTION_UP, 1050)
        state.event(output, MotionEvent.ACTION_DOWN, 1100)
        state.event(output, MotionEvent.ACTION_MOVE, 1150, x = 140f)
        state.event(output, MotionEvent.ACTION_UP, 1200, x = 145f)
        repeat(10) { i ->
            val time = 1300L + i * 100L
            state.event(output, MotionEvent.ACTION_DOWN, time)
            state.event(output, MotionEvent.ACTION_MOVE, time + 20, x = 130f)
            state.event(output, MotionEvent.ACTION_UP, time + 40, x = 135f)
        }
        state.reset(output)
        assertTrue(output.moves.any { it.first != 0 })
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
        assertEquals(0, output.rightTaps)
    }

    @Test fun keyboardMouseDirectPositioningNeverPressesOrReleasesAButton() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000, direct = true)
        state.event(output, MotionEvent.ACTION_MOVE, 1100, x = 700f, direct = true)
        state.event(output, MotionEvent.ACTION_UP, 1200, x = 800f, direct = true)
        state.event(output, MotionEvent.ACTION_DOWN, 1300, direct = true)
        state.event(output, MotionEvent.ACTION_CANCEL, 1400, direct = true)
        state.reset(output)
        assertEquals(listOf(100, 700, 100), output.positions.map { it.x })
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
    }

    @Test fun keyboardMouseTwoFingerScrollKeepsClicksDisabled() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000)
        fun twoFingerEvent(action: Int, time: Long, secondY: Float) {
            val properties = Array(2) { id -> MotionEvent.PointerProperties().apply {
                this.id = id; toolType = MotionEvent.TOOL_TYPE_FINGER
            } }
            val coordinates = Array(2) { id -> MotionEvent.PointerCoords().apply {
                x = 100f + id * 50f; y = if (id == 0) 100f else secondY
                pressure = 1f; size = 1f
            } }
            val event = MotionEvent.obtain(1000, time, action, 2, properties, coordinates,
                0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
            try { state.handle(event, true, output, emptySet(), buttonsEnabled = false) }
            finally { event.recycle() }
        }
        twoFingerEvent(MotionEvent.ACTION_POINTER_DOWN or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 1050, 100f)
        twoFingerEvent(MotionEvent.ACTION_MOVE, 1100, 240f)
        twoFingerEvent(MotionEvent.ACTION_POINTER_UP or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 1150, 240f)
        state.event(output, MotionEvent.ACTION_UP, 1200)
        assertTrue(output.scroll != 0)
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
        assertEquals(0, output.rightTaps)
    }

    @Test fun switchingToCursorOnlyReleasesAnExistingGestureAndRestoresNormalClicksLater() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000, buttonsEnabled = true, direct = true)
        assertEquals(listOf(true), output.buttons)
        state.event(output, MotionEvent.ACTION_MOVE, 1100, buttonsEnabled = false, direct = true)
        state.event(output, MotionEvent.ACTION_UP, 1200, buttonsEnabled = false, direct = true)
        assertEquals(listOf(true, false), output.buttons)
        state.event(output, MotionEvent.ACTION_DOWN, 1300, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_UP, 1350, buttonsEnabled = true)
        assertEquals(1, output.taps)
        state.event(output, MotionEvent.ACTION_DOWN, 1400, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_MOVE, 1450, x = 150f, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_CANCEL, 1500, buttonsEnabled = true)
        assertEquals(listOf(true, false, true, false), output.buttons)
    }
}
