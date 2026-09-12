package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Native touch is the one input path whose wire format we transcribed from the official client
 * rather than derived, so the byte layout is asserted literally here — a mistake in it would show
 * up on a device only as "the game ignores my fingers", with nothing to point at.
 *
 * The behavioural rules matter just as much and are just as easy to get wrong: fingers are
 * identified by a dense slot index rather than the platform's pointer id, and a lift must be
 * reported even when the finger has left the picture, or the host holds it down forever.
 */
class NativeTouchTest {

    // -- MOVE traffic shaping ---------------------------------------------------------------

    @Test
    fun nativeTouchMoveLimiterSendsLeadingAndLatestSnapshotsOnly() {
        val limiter = NativeTouchMoveLimiter()
        val first = listOf(TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 10, y = 20))
        val middle = listOf(TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 20, y = 30))
        val latest = listOf(TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 30, y = 40))

        assertEquals(first, limiter.offer(first, nowMs = 100L, minimumIntervalMs = 16L))
        assertNull(limiter.offer(middle, nowMs = 104L, minimumIntervalMs = 16L))
        assertNull(limiter.offer(latest, nowMs = 108L, minimumIntervalMs = 16L))
        assertEquals(8L, limiter.delayUntilFlushMs(nowMs = 108L, minimumIntervalMs = 16L))
        assertEquals(latest, limiter.flush(nowMs = 116L))
        assertFalse(limiter.hasPendingMove)
    }

    @Test
    fun nativeTouchMoveLimiterResetsBetweenTransports() {
        val limiter = NativeTouchMoveLimiter()
        val move = listOf(TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 10, y = 20))
        limiter.offer(move, nowMs = 100L, minimumIntervalMs = 16L)
        limiter.offer(move, nowMs = 104L, minimumIntervalMs = 16L)

        limiter.reset()

        assertFalse(limiter.hasPendingMove)
        assertEquals(move, limiter.offer(move, nowMs = 105L, minimumIntervalMs = 16L))
    }

    @Test
    fun nativeTouchMoveLimiterDropsRepeatedCoordinatesEvenWhenContactSizeChanges() {
        val limiter = NativeTouchMoveLimiter()
        val first = listOf(
            TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 10, y = 20, radiusX = 4, radiusY = 5),
        )
        val samePosition = listOf(
            TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 10, y = 20, radiusX = 8, radiusY = 9),
        )

        assertEquals(first, limiter.offer(first, nowMs = 100L, minimumIntervalMs = 8L))
        assertNull(limiter.offer(samePosition, nowMs = 108L, minimumIntervalMs = 8L))
        assertFalse(limiter.hasPendingMove)
    }

    // -- Local UI ownership -----------------------------------------------------------------

    @Test
    fun removingLauncherBoundsDoesNotReclassifyItsActiveFingerAsGameTouch() {
        val routing = NativeUiTouchRoutingState()
        routing.setStreamChromeBounds(left = 20, top = 20, right = 120, bottom = 80)
        routing.beginPointerGesture(
            pointerId = 7,
            touchesUi = routing.touchesRegisteredUi(x = 60f, y = 50f, width = 1280, height = 720),
        )

        routing.clearStreamChromeBounds()

        assertFalse(routing.touchesRegisteredUi(x = 60f, y = 50f, width = 1280, height = 720))
        assertTrue(routing.ownsPointer(7))
        assertTrue(routing.passthroughActive)

        routing.endPointerGesture()
        assertFalse(routing.ownsPointer(7))
        assertFalse(routing.passthroughActive)
    }

    @Test
    fun replacingTransientOverlayKeepsPointerOwnershipUntilLift() {
        val routing = NativeUiTouchRoutingState()
        routing.setOverlayBound("menu-launcher", left = 900, top = 20, right = 1020, bottom = 100)
        routing.beginPointerGesture(
            pointerId = 12,
            touchesUi = routing.touchesRegisteredUi(x = 960f, y = 60f, width = 1280, height = 720),
        )

        routing.clearOverlayBound("menu-launcher")
        routing.setOverlayBound("controls-panel", left = 700, top = 10, right = 1280, bottom = 720)

        assertTrue(routing.ownsPointer(12))
        routing.releasePointer(12)
        assertFalse(routing.hasOwnedPointer())
    }

    @Test
    fun measuredTouchControllerLeavesUnoccupiedLowerScreenForFingerMouse() {
        val routing = NativeUiTouchRoutingState()
        routing.setTouchControllerVisible(true)

        // Protect the controller area until Compose reports its first measured control.
        assertTrue(routing.touchesRegisteredUi(x = 640f, y = 600f, width = 1280, height = 720))

        routing.setTouchControllerBound("face", left = 1040, top = 500, right = 1240, bottom = 700)

        assertTrue(routing.touchesRegisteredUi(x = 1140f, y = 600f, width = 1280, height = 720))
        assertFalse(routing.touchesRegisteredUi(x = 640f, y = 600f, width = 1280, height = 720))
    }

    @Test
    fun visibleControllerKeepsFingerMouseGestureInComposeForEitherPointerOrder() {
        val routing = NativeUiTouchRoutingState()
        assertFalse(routing.routesTouchMouseThroughCompose())

        routing.setTouchControllerVisible(true)
        routing.setTouchControllerBound("left-stick", left = 20, top = 450, right = 260, bottom = 700)

        assertTrue(routing.routesTouchMouseThroughCompose())

        routing.beginPointerGesture(pointerId = 1, touchesUi = false)
        routing.addPointer(pointerId = 2, touchesUi = true)
        assertFalse(routing.ownsPointer(1))
        assertTrue(routing.ownsPointer(2))

        routing.endPointerGesture()
        routing.beginPointerGesture(pointerId = 2, touchesUi = true)
        routing.addPointer(pointerId = 1, touchesUi = false)
        assertFalse(routing.ownsPointer(1))
        assertTrue(routing.ownsPointer(2))

        routing.setTouchControllerVisible(false)
        assertFalse(routing.routesTouchMouseThroughCompose())
    }

    @Test
    fun pointerOwnershipDoesNotChangeWhenFingerCrossesControllerBounds() {
        val routing = NativeUiTouchRoutingState()

        routing.beginPointerGesture(pointerId = 4, touchesUi = false)
        assertFalse(routing.classifiesPointerAsUi(pointerId = 4, touchesUiNow = true))

        routing.endPointerGesture()
        routing.beginPointerGesture(pointerId = 7, touchesUi = true)
        assertTrue(routing.classifiesPointerAsUi(pointerId = 7, touchesUiNow = false))
    }

    @Test
    fun untrackedPointerFallsBackToCurrentUiBounds() {
        val routing = NativeUiTouchRoutingState()

        assertTrue(routing.classifiesPointerAsUi(pointerId = 9, touchesUiNow = true))
        assertFalse(routing.classifiesPointerAsUi(pointerId = 9, touchesUiNow = false))
    }

    @Test
    fun onlyAnOwnedLauncherGestureIsConsumedAfterUiOpens() {
        assertTrue(shouldConsumeNativeUiTransitionTouch(streamUiActive = true, hasOwnedPointer = true))
        assertFalse(shouldConsumeNativeUiTransitionTouch(streamUiActive = false, hasOwnedPointer = true))
        assertFalse(shouldConsumeNativeUiTransitionTouch(streamUiActive = true, hasOwnedPointer = false))
    }

    // -- Slot allocation ---------------------------------------------------------------------

    @Test
    fun slotsAreDenseAndStartAtZero() {
        val allocator = TouchSlotAllocator()
        assertEquals(0, allocator.acquire(pointerId = 42))
        assertEquals(1, allocator.acquire(pointerId = 7))
        assertEquals(2, allocator.acquire(pointerId = 99))
    }

    @Test
    fun theSamePointerKeepsItsSlot() {
        val allocator = TouchSlotAllocator()
        val first = allocator.acquire(pointerId = 42)
        assertEquals(first, allocator.acquire(pointerId = 42))
        assertEquals(1, allocator.activeCount)
    }

    /** The reason this class exists: platform pointer ids climb, host slots must not. */
    @Test
    fun aFreedSlotIsReusedRatherThanSkipped() {
        val allocator = TouchSlotAllocator()
        allocator.acquire(pointerId = 10) // slot 0
        allocator.acquire(pointerId = 11) // slot 1

        assertEquals(0, allocator.release(pointerId = 10))
        // A brand new finger, with a pointer id nothing like the old one.
        assertEquals(0, allocator.acquire(pointerId = 5000))
    }

    @Test
    fun releasingAnUnknownPointerIsHarmless() {
        val allocator = TouchSlotAllocator()
        assertNull(allocator.release(pointerId = 3))
    }

    @Test
    fun allocationStopsAtTheHostLimit() {
        val allocator = TouchSlotAllocator()
        repeat(MAX_CONCURRENT_TOUCHES) { index -> assertNotNull(allocator.acquire(pointerId = index)) }
        assertNull(allocator.acquire(pointerId = 999))
    }

    // -- Batch construction ------------------------------------------------------------------

    private fun batch(
        allocator: TouchSlotAllocator = TouchSlotAllocator(),
        phase: Int,
        pointers: List<TouchPointerSample>,
        viewWidth: Int = 1280,
        viewHeight: Int = 720,
    ) = buildTouchBatch(
        allocator = allocator,
        phase = phase,
        pointers = pointers,
        viewWidth = viewWidth,
        viewHeight = viewHeight,
        streamWidth = 1920,
        streamHeight = 1080,
        stretchToFit = false,
        renderingAspectRatio = 0f,
    )

    @Test
    fun coordinatesAreAFractionOfTheVideoArea() {
        val records = batch(
            phase = TouchPhase.DOWN,
            pointers = listOf(TouchPointerSample(pointerId = 1, x = 640f, y = 360f)),
        )
        assertEquals(1, records.size)
        // Dead centre of a matching-aspect view.
        assertEquals((TOUCH_COORDINATE_MAX / 2).toDouble(), records[0].x.toDouble(), 1.0)
        assertEquals((TOUCH_COORDINATE_MAX / 2).toDouble(), records[0].y.toDouble(), 1.0)
    }

    @Test
    fun twoFingersGetDistinctSlotsInOneBatch() {
        val records = batch(
            phase = TouchPhase.MOVE,
            pointers = listOf(
                TouchPointerSample(pointerId = 1, x = 100f, y = 100f),
                TouchPointerSample(pointerId = 2, x = 900f, y = 500f),
            ),
        )
        assertEquals(2, records.size)
        assertEquals(setOf(0, 1), records.map { it.slot }.toSet())
    }

    /**
     * A finger on the letterbox bar is not a finger on the picture. Reporting it clamped to the
     * edge would fire an unintended tap right where the game's UI usually lives.
     */
    @Test
    fun aTouchOutsideThePictureIsDroppedWhilePressed() {
        // 4:3 view onto a 16:9 stream: 90px bars top and bottom, video occupies y 90..630.
        val records = batch(
            phase = TouchPhase.MOVE,
            pointers = listOf(TouchPointerSample(pointerId = 1, x = 480f, y = 10f)),
            viewWidth = 960,
            viewHeight = 720,
        )
        assertTrue("expected the touch on the bar to be dropped, got $records", records.isEmpty())
    }

    /** ...but a lift out there must still be sent, or that finger never comes up on the host. */
    @Test
    fun aLiftOutsideThePictureIsStillReported() {
        val allocator = TouchSlotAllocator()
        batch(
            allocator = allocator,
            phase = TouchPhase.DOWN,
            pointers = listOf(TouchPointerSample(pointerId = 1, x = 480f, y = 360f)),
            viewWidth = 960,
            viewHeight = 720,
        )

        val records = batch(
            allocator = allocator,
            phase = TouchPhase.UP,
            pointers = listOf(TouchPointerSample(pointerId = 1, x = 480f, y = 10f)),
            viewWidth = 960,
            viewHeight = 720,
        )
        assertEquals(1, records.size)
        assertEquals(TouchPhase.UP, records[0].phase)
        assertEquals(0, records[0].slot)
        assertEquals(0, allocator.activeCount)
    }

    @Test
    fun coordinatesAreClampedIntoRange() {
        // Slightly outside, but within the finger's radius, so it survives and clamps.
        val records = batch(
            phase = TouchPhase.MOVE,
            pointers = listOf(TouchPointerSample(pointerId = 1, x = -2f, y = 360f, radiusX = 40f)),
        )
        assertEquals(1, records.size)
        assertEquals(0, records[0].x)
    }

    @Test
    fun degenerateSizesProduceNothing() {
        val pointers = listOf(TouchPointerSample(pointerId = 1, x = 10f, y = 10f))
        assertTrue(batch(phase = TouchPhase.DOWN, pointers = pointers, viewWidth = 0).isEmpty())
        assertTrue(batch(phase = TouchPhase.DOWN, pointers = pointers, viewHeight = 0).isEmpty())
    }

    @Test
    fun nonFinitePointerIsDroppedButItsLiftStillReleasesTheHostSlot() {
        val allocator = TouchSlotAllocator()
        batch(
            allocator = allocator,
            phase = TouchPhase.DOWN,
            pointers = listOf(TouchPointerSample(pointerId = 1, x = 640f, y = 360f)),
        )

        assertTrue(
            batch(
                allocator = allocator,
                phase = TouchPhase.MOVE,
                pointers = listOf(TouchPointerSample(pointerId = 1, x = Float.NaN, y = 360f)),
            ).isEmpty(),
        )
        val release = batch(
            allocator = allocator,
            phase = TouchPhase.UP,
            pointers = listOf(TouchPointerSample(pointerId = 1, x = Float.NaN, y = Float.NaN)),
        )

        assertEquals(1, release.size)
        assertEquals(TouchPhase.UP, release.single().phase)
        assertEquals(0, allocator.activeCount)
    }

    // -- Wire format -------------------------------------------------------------------------

    /** Strips the transport wrapper so the assertions below address the payload itself. */
    private fun payloadOf(packet: ByteArray, recordCount: Int): ByteArray {
        val payloadSize = 8 + 16 * recordCount
        return packet.copyOfRange(packet.size - payloadSize, packet.size)
    }

    private fun be16(bytes: ByteArray, offset: Int): Int =
        ((bytes[offset].toInt() and 0xff) shl 8) or (bytes[offset + 1].toInt() and 0xff)

    private fun le32(bytes: ByteArray, offset: Int): Int =
        (bytes[offset].toInt() and 0xff) or
            ((bytes[offset + 1].toInt() and 0xff) shl 8) or
            ((bytes[offset + 2].toInt() and 0xff) shl 16) or
            ((bytes[offset + 3].toInt() and 0xff) shl 24)

    @Test
    fun packetMatchesTheDocumentedLayout() {
        val encoder = InputEncoder()
        val packet = encoder.encodeTouchBatch(
            listOf(
                TouchRecord(slot = 3, phase = TouchPhase.DOWN, x = 0x1234, y = 0x5678, radiusX = 9, radiusY = 11),
                TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 1, y = 2),
            ),
            nowUs = FIXED_NOW_US,
        )
        assertNotNull(packet)
        val payload = payloadOf(packet!!, recordCount = 2)

        // Opcode is little-endian; everything after it is big-endian.
        assertEquals(InputEncoder.INPUT_TOUCH, le32(payload, 0))
        assertEquals(8 + 16 * 2, be16(payload, 4))
        assertEquals(2, be16(payload, 6))

        assertEquals(3, payload[8].toInt())
        assertEquals(TouchPhase.DOWN, payload[9].toInt())
        assertEquals(0x1234, be16(payload, 10))
        assertEquals(0x5678, be16(payload, 12))
        assertEquals(9, payload[14].toInt())
        assertEquals(11, payload[15].toInt())

        assertEquals(0, payload[24].toInt())
        assertEquals(TouchPhase.MOVE, payload[25].toInt())
        assertEquals(1, be16(payload, 26))
        assertEquals(2, be16(payload, 28))
    }

    @Test
    fun theTopCoordinateSurvivesAsUnsigned() {
        val encoder = InputEncoder()
        val packet = encoder.encodeTouchBatch(
            listOf(TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = TOUCH_COORDINATE_MAX, y = TOUCH_COORDINATE_MAX)),
            nowUs = FIXED_NOW_US,
        )
        val payload = payloadOf(packet!!, recordCount = 1)
        assertEquals(TOUCH_COORDINATE_MAX, be16(payload, 10))
        assertEquals(TOUCH_COORDINATE_MAX, be16(payload, 12))
    }

    /** A record left at 0 is stamped by the encoder, so the host never sees a zero timestamp. */
    @Test
    fun anUnstampedRecordGetsTheEncodersClock() {
        val payload = payloadOf(
            InputEncoder().encodeTouchBatch(
                listOf(TouchRecord(slot = 0, phase = TouchPhase.DOWN, x = 0, y = 0)),
                nowUs = FIXED_NOW_US,
            )!!,
            recordCount = 1,
        )
        val stamp = (16..23).fold(0L) { acc, i -> (acc shl 8) or (payload[i].toLong() and 0xff) }
        assertEquals(FIXED_NOW_US, stamp)
    }

    @Test
    fun anExplicitRecordTimestampIsKept() {
        val payload = payloadOf(
            InputEncoder().encodeTouchBatch(
                listOf(TouchRecord(slot = 0, phase = TouchPhase.DOWN, x = 0, y = 0, timestampUs = 4242L)),
                nowUs = FIXED_NOW_US,
            )!!,
            recordCount = 1,
        )
        val stamp = (16..23).fold(0L) { acc, i -> (acc shl 8) or (payload[i].toLong() and 0xff) }
        assertEquals(4242L, stamp)
    }

    @Test
    fun anEmptyBatchProducesNoPacket() {
        assertNull(InputEncoder().encodeTouchBatch(emptyList(), nowUs = FIXED_NOW_US))
    }

    @Test
    fun aBatchIsCappedRatherThanOverflowing() {
        val encoder = InputEncoder()
        val touches = (0 until MAX_TOUCH_RECORDS_PER_BATCH + 10).map {
            TouchRecord(slot = 0, phase = TouchPhase.MOVE, x = 0, y = 0)
        }
        val payload = payloadOf(
            encoder.encodeTouchBatch(touches, nowUs = FIXED_NOW_US)!!,
            recordCount = MAX_TOUCH_RECORDS_PER_BATCH,
        )
        assertEquals(MAX_TOUCH_RECORDS_PER_BATCH, be16(payload, 6))
    }

    private companion object {
        /** Any fixed value; SystemClock is unavailable on the JVM, and exactness beats "> 0". */
        const val FIXED_NOW_US = 1_234_567L
    }
}
