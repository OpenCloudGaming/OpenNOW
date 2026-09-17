package com.opencloudgaming.opennow

/**
 * Android 8+ supports AudioTrack.PERFORMANCE_MODE_LOW_LATENCY on handhelds and TVs.
 * WebRtcAudioGuard protects the pinned library's buffer tuner from released tracks.
 * Older Android versions retain WebRTC's normal AudioTrack path.
 */
internal fun shouldUseLowLatencyStreamAudio(sdkInt: Int): Boolean = sdkInt >= 26
