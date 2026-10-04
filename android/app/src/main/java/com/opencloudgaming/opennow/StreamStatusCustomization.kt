package com.opencloudgaming.opennow

import androidx.annotation.StringRes

internal const val DEFAULT_STREAM_STATS_BACKGROUND_OPACITY = 0.52f

internal fun AppSettings.streamStatsBackgroundAlpha(): Float = if (!streamStatsBackgroundEnabled) {
    0f
} else {
    streamStatsBackgroundOpacity.takeIf { it.isFinite() }?.coerceIn(0f, 1f)
        ?: DEFAULT_STREAM_STATS_BACKGROUND_OPACITY
}

/**
 * The items that can appear in the in-stream status line.
 *
 * This is shared by the live Stream Controls panel and first-run setup so both surfaces always
 * expose the same choices and write the same persisted fields.
 */
internal enum class StreamStatusItem(
    @StringRes val labelRes: Int,
    @StringRes val previewValueRes: Int?,
) {
    Keyboard(R.string.stream_statusbar_metric_keyboard, null),
    Fps(R.string.stream_statusbar_metric_fps, R.string.setup_play_metric_fps_preview),
    Ping(R.string.stream_statusbar_metric_ping, R.string.setup_play_metric_ping_preview),
    Bitrate(R.string.stream_statusbar_metric_bitrate, R.string.setup_play_metric_bitrate_preview),
    Battery(R.string.stream_statusbar_metric_battery, R.string.setup_play_metric_battery_preview),
    SessionBattery(R.string.stream_statusbar_metric_session_battery, R.string.setup_play_metric_session_battery_preview),
    Connection(R.string.stream_statusbar_metric_connection, R.string.setup_play_metric_connection_preview),
    Resolution(R.string.stream_statusbar_metric_resolution, R.string.setup_play_metric_resolution_preview),
    Codec(R.string.stream_statusbar_metric_codec, R.string.setup_play_metric_codec_preview),
    Server(R.string.stream_statusbar_metric_server, R.string.setup_play_metric_server_preview),
    Latency(R.string.stream_statusbar_metric_latency, R.string.setup_play_metric_latency_preview),
    PacketLoss(R.string.stream_statusbar_metric_loss, R.string.setup_play_metric_loss_preview),
    ;

    fun enabledIn(settings: AppSettings): Boolean = when (this) {
        Keyboard -> !settings.hideStreamButtons
        Fps -> settings.streamStatsMetrics.fps
        Ping -> settings.streamStatsMetrics.ping
        Bitrate -> settings.streamStatsMetrics.bitrate
        Battery -> settings.streamStatsMetrics.battery
        SessionBattery -> settings.streamStatsMetrics.sessionBattery
        Connection -> settings.streamStatsMetrics.connection
        Resolution -> settings.streamStatsMetrics.resolution
        Codec -> settings.streamStatsMetrics.codec
        Server -> settings.streamStatsMetrics.location
        Latency -> settings.streamStatsMetrics.latency
        PacketLoss -> settings.streamStatsMetrics.packetLoss
    }

    fun setEnabled(settings: AppSettings, enabled: Boolean): AppSettings {
        if (enabledIn(settings) == enabled) return settings
        if (this == Keyboard) return settings.copy(hideStreamButtons = !enabled)

        val metrics = settings.streamStatsMetrics
        val updatedMetrics = when (this) {
            Keyboard -> metrics
            Fps -> metrics.copy(fps = enabled)
            Ping -> metrics.copy(ping = enabled)
            Bitrate -> metrics.copy(bitrate = enabled)
            Battery -> metrics.copy(battery = enabled)
            SessionBattery -> metrics.copy(sessionBattery = enabled)
            Connection -> metrics.copy(connection = enabled)
            Resolution -> metrics.copy(resolution = enabled)
            Codec -> metrics.copy(codec = enabled)
            Server -> metrics.copy(location = enabled)
            Latency -> metrics.copy(latency = enabled)
            PacketLoss -> metrics.copy(packetLoss = enabled)
        }
        return settings.copy(streamStatsMetrics = updatedMetrics)
    }
}
