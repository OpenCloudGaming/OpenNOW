package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.hypot
import kotlin.math.sign
import kotlin.random.Random

class MascotMotionTest {
    @Test
    fun eachEdgeAndCornerReflectsInward() {
        val random = Random(42)
        val minX = mascotMinX(200f)
        val maxX = mascotMaxX(200f)
        val cases = listOf(
            listOf(minX, 50f, -300f, 200f, 1f, 1f),
            listOf(maxX, 50f, 300f, 200f, -1f, 1f),
            listOf(50f, 0f, 300f, -200f, 1f, 1f),
            listOf(50f, 155f, 300f, 200f, 1f, -1f),
            listOf(maxX, 155f, 300f, 200f, -1f, -1f),
        )
        cases.forEach { values ->
            val motion = MascotMotion(values[0], values[1], values[2], values[3])
            assertTrue(motion.advance(200f, 200f, 0.016f, random))
            assertEquals(values[4], sign(motion.vx))
            assertEquals(values[5], sign(motion.vy))
            assertTrue(motion.x in minX..maxX && motion.y in 0f..155f)
        }
    }

    @Test
    fun motionSurvivesResizesTinyWindowsAndLongFrames() {
        val random = Random(42)
        val motion = MascotMotion(800f, 600f, 350f, -250f)
        repeat(2_000) { index ->
            val width = if (index % 300 < 10) 30f else 500f
            val height = if (index % 300 < 10) 20f else 300f
            motion.advance(width, height, if (index % 100 == 0) 20f else 1f / 60f, random)
            assertTrue(motion.x in mascotMinX(width)..mascotMaxX(width))
            assertTrue(motion.y in 0f..(height - MASCOT_SIZE_DP).coerceAtLeast(0f))
            assertTrue(motion.vx.isFinite() && motion.vy.isFinite())
        }
    }

    @Test
    fun horizontalBounceHoldsMascotPartlyVisibleBeforeReturning() {
        val width = 200f
        val maxX = mascotMaxX(width)
        val motion = MascotMotion(maxX - 1f, 80f, 300f, 100f)

        assertTrue(motion.advance(width, 200f, 0.016f, Random(42)))
        assertEquals(maxX, motion.x, 0f)
        assertEquals(MASCOT_SIDE_PEEK_VISIBLE_DP, width - motion.x, 0f)
        assertTrue(motion.isSidePeeking)

        val heldY = motion.y
        repeat(13) {
            assertFalse(motion.advance(width, 200f, 0.05f, Random(42)))
            assertEquals(maxX, motion.x, 0f)
            assertEquals(heldY, motion.y, 0f)
        }
        assertFalse(motion.isSidePeeking)
        motion.advance(width, 200f, 0.016f, Random(42))
        assertTrue(motion.x < maxX)
    }

    @Test
    fun randomizedMotionUsesTheSlowerSpeedRange() {
        val motion = MascotMotion(0f, 0f, 0f, 0f)
        val random = Random(42)
        repeat(1_000) {
            motion.randomizeVelocity(random)
            val speed = hypot(motion.vx, motion.vy)
            assertTrue(speed >= MASCOT_MIN_SPEED_DP_PER_SECOND)
            assertTrue(speed <= MASCOT_MAX_SPEED_DP_PER_SECOND + 0.001f)
        }
    }

    @Test
    fun movementAwayFromWallsIsContinuousAndDoesNotChangeDirection() {
        val motion = MascotMotion(100f, 100f, 300f, -200f)
        assertFalse(motion.advance(500f, 500f, 0.02f))
        assertEquals(106f, motion.x, 0.001f)
        assertEquals(96f, motion.y, 0.001f)
        assertEquals(300f, motion.vx, 0f)
        assertEquals(-200f, motion.vy, 0f)
    }

    @Test
    fun everyBounceGetsADifferentMessage() {
        val random = Random(123)
        var current = 0
        repeat(1_000) {
            val next = nextMascotMessage(current, 5, random)
            assertNotEquals(current, next)
            assertTrue(next in 0..4)
            current = next
        }
    }

    @Test
    fun mascotDefaultsAndSettingsSurviveSerialization() {
        val legacy = OpenNowJson.decodeFromString<AppSettings>("{}")
        assertFalse(legacy.uselessMascotEnabled)
        assertEquals(5, legacy.uselessMascotDelaySeconds)
        val enabled = legacy.copy(uselessMascotEnabled = true, uselessMascotDelaySeconds = 30)
        assertEquals(enabled, OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(enabled)))
        for (value in listOf(Int.MIN_VALUE, -1, 0, 4, 5, 12, 299, 300, Int.MAX_VALUE)) {
            val normalized = enabled.copy(uselessMascotDelaySeconds = value).normalizedForAndroid()
            assertTrue(normalized.uselessMascotDelaySeconds in 5..300)
            assertEquals(0, normalized.uselessMascotDelaySeconds % 5)
            assertEquals(normalized, normalized.normalizedForAndroid())
        }
    }
}
