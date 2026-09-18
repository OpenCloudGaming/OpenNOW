package com.opencloudgaming.opennow

internal const val MAX_TOUCH_EXTRA_BUTTON_COMBO_ACTIONS = 12

internal fun extraButtonComboKey(index: Int): String = (index + 1).toString()

private val legacyComboActions = mapOf(
    TouchExtraButtonAction.LeftBumperAndLeftTrigger to listOf(
        TouchExtraButtonAction.LeftBumper,
        TouchExtraButtonAction.LeftTrigger,
    ),
    TouchExtraButtonAction.RightBumperAndRightTrigger to listOf(
        TouchExtraButtonAction.RightBumper,
        TouchExtraButtonAction.RightTrigger,
    ),
    TouchExtraButtonAction.LeftAndRightBumpers to listOf(
        TouchExtraButtonAction.LeftBumper,
        TouchExtraButtonAction.RightBumper,
    ),
    TouchExtraButtonAction.LeftAndRightTriggers to listOf(
        TouchExtraButtonAction.LeftTrigger,
        TouchExtraButtonAction.RightTrigger,
    ),
)

internal val customizableTouchExtraButtonActions: List<TouchExtraButtonAction> =
    TouchExtraButtonAction.entries.filter { action ->
        action != TouchExtraButtonAction.None && action !in legacyComboActions
    }

internal fun expandedTouchExtraButtonActions(
    action: TouchExtraButtonAction,
): List<TouchExtraButtonAction> = when (action) {
    TouchExtraButtonAction.None -> emptyList()
    else -> legacyComboActions[action] ?: listOf(action)
}

internal fun normalizeTouchExtraButtonCombo(
    actions: List<TouchExtraButtonAction>,
): List<TouchExtraButtonAction> = actions
    .flatMap(::expandedTouchExtraButtonActions)
    .distinct()
    .take(MAX_TOUCH_EXTRA_BUTTON_COMBO_ACTIONS)
