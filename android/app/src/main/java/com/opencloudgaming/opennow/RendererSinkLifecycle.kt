package com.opencloudgaming.opennow

/**
 * Tracks the sink state requested by Android surface callbacks.
 *
 * Surface callbacks run on the main thread while WebRTC can replace tracks from its own callback
 * threads. Keeping this transition atomic prevents duplicate queued add/remove operations without
 * making either caller wait for the native renderer lock.
 */
internal class RendererSinkLifecycle {
    private var attachRequested = false

    @Synchronized
    fun requestAttach(isAvailable: () -> Boolean): Boolean {
        if (attachRequested || !isAvailable()) return false
        attachRequested = true
        return true
    }

    @Synchronized
    fun requestDetach(): Boolean {
        if (!attachRequested) return false
        attachRequested = false
        return true
    }

    @Synchronized
    fun isAttachRequested(): Boolean = attachRequested
}
