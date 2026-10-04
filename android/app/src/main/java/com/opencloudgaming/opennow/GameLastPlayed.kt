package com.opencloudgaming.opennow

import java.text.ParsePosition
import java.text.SimpleDateFormat
import java.util.Locale
import java.util.TimeZone

internal data class GameLastPlayedTime(val epochMs: Long, val dateOnly: Boolean)

private val timestampPattern = Regex(
    """^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})(?:\.(\d+))?(Z|[+-]\d{2}:?\d{2})$""",
)
private val dateOnlyPattern = Regex("""^\d{4}-\d{2}-\d{2}$""")

internal fun parseGameLastPlayed(raw: String): GameLastPlayedTime? {
    val value = raw.trim()
    val timestamp = timestampPattern.matchEntire(value)
    val dateOnly = timestamp == null && dateOnlyPattern.matches(value)
    if (timestamp == null && !dateOnly) return null
    val normalized = if (timestamp == null) {
        value
    } else {
        val (date, time, fraction, zone) = timestamp.destructured
        val offset = if (zone.length == 5 && zone != "Z") "${zone.take(3)}:${zone.takeLast(2)}" else zone
        "${date}T$time.${fraction.padEnd(3, '0').take(3)}$offset"
    }
    val formatter = SimpleDateFormat(
        if (dateOnly) "yyyy-MM-dd" else "yyyy-MM-dd'T'HH:mm:ss.SSSXXX",
        Locale.US,
    ).apply {
        isLenient = false
        timeZone = TimeZone.getTimeZone("UTC")
    }
    val position = ParsePosition(0)
    val parsed = formatter.parse(normalized, position) ?: return null
    if (position.index != normalized.length) return null
    return GameLastPlayedTime(parsed.time, dateOnly)
}

internal fun latestGameLastPlayed(game: GameInfo): GameLastPlayedTime? =
    (listOfNotNull(game.lastPlayed) + game.variants.mapNotNull { it.lastPlayedDate })
        .mapNotNull(::parseGameLastPlayed)
        .maxByOrNull { it.epochMs }

internal enum class GamePlayAgeUnit { JustNow, Minute, Hour, Day, Week, Month, Year }

internal data class GamePlayAge(val count: Int, val unit: GamePlayAgeUnit)

internal fun gamePlayAge(epochMs: Long, nowMs: Long): GamePlayAge? {
    if (epochMs > nowMs) return null
    val minutes = (nowMs - epochMs) / 60_000L
    val (count, unit) = when {
        minutes < 1 -> 0L to GamePlayAgeUnit.JustNow
        minutes < 60 -> minutes to GamePlayAgeUnit.Minute
        minutes < 1_440 -> minutes / 60 to GamePlayAgeUnit.Hour
        minutes < 10_080 -> minutes / 1_440 to GamePlayAgeUnit.Day
        minutes < 43_200 -> minutes / 10_080 to GamePlayAgeUnit.Week
        minutes < 525_600 -> minutes / 43_200 to GamePlayAgeUnit.Month
        else -> minutes / 525_600 to GamePlayAgeUnit.Year
    }
    return GamePlayAge(count.coerceAtMost(Int.MAX_VALUE.toLong()).toInt(), unit)
}
