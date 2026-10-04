package com.opencloudgaming.opennow

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GameLastPlayedTest {
    @Test
    fun parsesProviderTimestampsAndChoosesTheMostRecentVariant() {
        val expected = Instant.parse("2026-10-03T19:37:00Z").toEpochMilli()
        val game = GameInfo(
            id = "game",
            title = "Game",
            lastPlayed = "2026-10-03T18:00:00Z",
            variants = listOf(
                GameVariant("one", "STEAM", lastPlayedDate = "2026-10-03T12:37:00-07:00"),
                GameVariant("two", "EPIC", lastPlayedDate = "2026-10-02T23:00:00.123456Z"),
            ),
        )

        assertEquals(GameLastPlayedTime(expected, dateOnly = false), latestGameLastPlayed(game))
        assertEquals(expected, parseGameLastPlayed("2026-10-03T19:37:00+0000")?.epochMs)
    }

    @Test
    fun dateOnlyAndMalformedValuesDoNotInventARecentPlayTime() {
        assertEquals(true, parseGameLastPlayed("2026-10-03")?.dateOnly)
        assertNull(parseGameLastPlayed("not a date"))
        assertNull(parseGameLastPlayed("2026-13-03T19:37:00Z"))
        assertNull(latestGameLastPlayed(GameInfo(id = "game", title = "Game", lastPlayed = "bad")))
    }

    @Test
    fun relativeAgeUsesElapsedMinutesAndIgnoresFutureTimes() {
        val now = 1_000_000_000L
        assertEquals(GamePlayAge(23, GamePlayAgeUnit.Minute), gamePlayAge(now - 23 * 60_000L, now))
        assertEquals(GamePlayAge(1, GamePlayAgeUnit.Hour), gamePlayAge(now - 60 * 60_000L, now))
        assertNull(gamePlayAge(now + 60_000L, now))
    }
}
