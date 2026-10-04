package com.opencloudgaming.opennow

import kotlinx.coroutines.Job

/** Input from one Android controller. Host gamepad packets are full snapshots per slot. */
internal class PhysicalGamepadState {
    var buttons = 0
    var hatButtons = 0
    var leftTriggerButtonPressed = false
    var rightTriggerButtonPressed = false
    var leftTrigger = 0
    var rightTrigger = 0
    var leftStickX = 0
    var leftStickY = 0
    var rightStickX = 0
    var rightStickY = 0
    val steamOverlayChord = SteamOverlayChordState()
    var guideAutoReleaseJob: Job? = null
    var steamOverlayChordReleaseJob: Job? = null

    fun reset() {
        guideAutoReleaseJob?.cancel()
        guideAutoReleaseJob = null
        steamOverlayChordReleaseJob?.cancel()
        steamOverlayChordReleaseJob = null
        steamOverlayChord.reset()
        buttons = 0
        hatButtons = 0
        leftTriggerButtonPressed = false
        rightTriggerButtonPressed = false
        leftTrigger = 0
        rightTrigger = 0
        leftStickX = 0
        leftStickY = 0
        rightStickX = 0
        rightStickY = 0
    }
}
