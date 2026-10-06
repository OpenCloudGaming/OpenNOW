package com.opencloudgaming.opennow

/** Main-thread policy: one automatic prompt per attachment per foreground visit. */
internal class KishiUsbPermissionPrompt {
    private var foreground = false
    private val attempted = mutableSetOf<String>()
    private var pending: String? = null

    fun onForeground() {
        if (!foreground) attempted.clear()
        foreground = true
    }

    fun onBackground() {
        // The system permission activity can stop our activity. Its return is still the
        // same visit, so a denial must not immediately open the same dialog again.
        if (pending == null) foreground = false
    }

    fun retainDevices(connected: Set<String>) {
        attempted.retainAll(connected)
        if (pending !in connected) pending = null
    }

    fun request(device: String, manual: Boolean): Boolean {
        if (pending != null || (!manual && (!foreground || device in attempted))) return false
        attempted += device
        pending = device
        return true
    }

    fun complete(device: String): Boolean {
        if (pending != device) return false
        pending = null
        return true
    }

    fun reset() {
        pending = null
        attempted.clear()
    }
}
