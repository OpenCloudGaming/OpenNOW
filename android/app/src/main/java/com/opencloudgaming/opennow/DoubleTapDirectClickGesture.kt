package com.opencloudgaming.opennow

import kotlin.math.abs

/** Recognizes completed double taps without letting a tap-then-slide turn into a click or drag. */
internal class DoubleTapDirectClickGesture {
    private var downX = 0f
    private var downY = 0f
    private var downTimeMs = 0L
    private var lastTapTimeMs: Long? = null
    private var lastTapX = 0f
    private var lastTapY = 0f
    private var secondTap = false
    private var tapEligible = false
    var isSliding = false
        private set

    fun begin(x: Float, y: Float, timeMs: Long) {
        secondTap = lastTapTimeMs?.let {
            timeMs - it in 0..320 && abs(x - lastTapX) <= 36f && abs(y - lastTapY) <= 36f
        } == true
        lastTapTimeMs = null
        downX = x
        downY = y
        downTimeMs = timeMs
        tapEligible = true
        isSliding = false
    }

    fun move(x: Float, y: Float) {
        // Once a slide starts, returning to the original position must never restore a tap.
        if (abs(x - downX) > 10f || abs(y - downY) > 10f) isSliding = true
    }

    fun finish(x: Float, y: Float, timeMs: Long): Boolean {
        move(x, y)
        val click = tapEligible && !isSliding && timeMs - downTimeMs in 0..450
        tapEligible = false
        if (!click) return false
        if (secondTap) return true
        lastTapTimeMs = timeMs
        lastTapX = x
        lastTapY = y
        return false
    }

    /** Multi-finger gestures can scroll, but cannot arm a later direct click. */
    fun invalidateTap() {
        tapEligible = false
        lastTapTimeMs = null
    }

    fun reset() {
        invalidateTap()
        secondTap = false
        isSliding = false
    }
}
