package com.opencloudgaming.opennow

/**
 * Last-active virtual stick source wins. Releasing it restores any still-held source underneath,
 * so a steering button cannot zero a real on-screen joystick (or its opposite steering button).
 */
internal class VirtualStickSourceState {
    private val sources = linkedMapOf<String, StickInput>()

    fun update(sourceId: String, x: Float, y: Float): StickInput {
        sources.remove(sourceId)
        if (x != 0f || y != 0f) sources[sourceId] = StickInput(x, y)
        return sources.values.lastOrNull() ?: StickInput()
    }

    fun clear() = sources.clear()
}

internal data class StickInput(val x: Float = 0f, val y: Float = 0f)
