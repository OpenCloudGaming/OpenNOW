package com.opencloudgaming.opennow

/** Runtime state only: a saved toggle preference must never restore a held input. */
internal class TouchButtonPressState(private val toggle: Boolean) {
    private var pressed = false
    fun down(): Boolean { pressed = if (toggle) !pressed else true; return pressed }
    fun up(): Boolean { if (!toggle) pressed = false; return pressed }
    fun cancel(): Boolean { pressed = false; return false }
}
