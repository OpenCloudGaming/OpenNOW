package com.opencloudgaming.opennow

/** Sends a mouse button edge only when the first source presses or the last source releases it. */
internal class MouseButtonPressSources {
    private val sourcesByButton = mutableMapOf<Int, MutableSet<String>>()

    fun update(button: Int, sourceId: String, pressed: Boolean, sendEdge: (Boolean) -> Boolean): Boolean {
        val sources = sourcesByButton[button]
        val wasPressed = sources?.isNotEmpty() == true
        if (pressed == (sourceId in sources.orEmpty())) return true

        val isPressed = if (pressed) true else sources.orEmpty().size > 1
        val sent = wasPressed == isPressed || sendEdge(isPressed)
        // A failed DOWN can be retried. A failed UP still ends this local hold so a later press
        // cannot be swallowed after an input-channel interruption.
        if (!sent && pressed) return false

        if (pressed) sourcesByButton.getOrPut(button) { mutableSetOf() }.add(sourceId)
        else {
            sources?.remove(sourceId)
            if (sources?.isEmpty() == true) sourcesByButton.remove(button)
        }
        return sent
    }

    fun clear() = sourcesByButton.clear()
}
