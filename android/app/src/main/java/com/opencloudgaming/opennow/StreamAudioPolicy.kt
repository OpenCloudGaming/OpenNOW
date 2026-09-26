package com.opencloudgaming.opennow

import android.media.AudioAttributes

/**
 * Android 8+ supports AudioTrack.PERFORMANCE_MODE_LOW_LATENCY on handhelds and TVs.
 * WebRtcAudioGuard protects the pinned library's buffer tuner from released tracks.
 * Older Android versions retain WebRTC's normal AudioTrack path.
 */
internal fun shouldUseLowLatencyStreamAudio(sdkInt: Int, lowLatencyGameAudio: Boolean = true): Boolean =
    lowLatencyGameAudio && sdkInt >= 26

internal fun streamAudioUsage(lowLatencyGameAudio: Boolean): Int =
    if (lowLatencyGameAudio) AudioAttributes.USAGE_GAME else AudioAttributes.USAGE_MEDIA
