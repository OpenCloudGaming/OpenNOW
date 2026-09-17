package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.util.UUID

internal const val MAX_TOUCH_PRESETS = 40
internal const val MAX_TOUCH_PRESET_CODE = 65536

@Serializable
data class TouchControlPreset(
    val id: String,
    val name: String,
    val controls: AndroidTouchSettings,
)

@Serializable
private data class SharedTouchPreset(
    val format: String = "opennow-touch",
    val version: Int = 1,
    val name: String,
    val controls: AndroidTouchSettings,
)

private val touchPresetJson = Json { ignoreUnknownKeys = true; encodeDefaults = true }

internal fun AndroidTouchSettings.normalizedPresetControls(): AndroidTouchSettings =
    normalizedTouchControls()

internal fun newTouchPreset(name: String, controls: AndroidTouchSettings): TouchControlPreset = TouchControlPreset(
    UUID.randomUUID().toString(), name.filterNot(Char::isISOControl).trim().take(64), controls.normalizedPresetControls(),
)

/** Applying a layout must not change input mode or hide an active overlay. */
internal fun AndroidTouchSettings.applyingTouchPreset(preset: TouchControlPreset): AndroidTouchSettings =
    preset.controls.normalizedPresetControls().copy(
        enabled = enabled, mousePad = mousePad, mouseDirectClick = mouseDirectClick,
        nativeTouchMode = nativeTouchMode, nativeTouchOptedIn = nativeTouchOptedIn,
    )

internal fun exportTouchPreset(preset: TouchControlPreset): String = touchPresetJson.encodeToString(
    SharedTouchPreset(name = preset.name, controls = preset.controls.normalizedPresetControls()),
)

internal fun importTouchPreset(code: String): TouchControlPreset? {
    if (code.length > MAX_TOUCH_PRESET_CODE) return null
    return runCatching {
        val shared = touchPresetJson.decodeFromString<SharedTouchPreset>(code)
        require(shared.format == "opennow-touch" && shared.version == 1)
        require(shared.controls.offsets.size <= 100 && shared.controls.buttonAppearances.size <= touchButtonKeys.size)
        newTouchPreset(shared.name, shared.controls).also { require(it.name.isNotBlank()) }
    }.getOrNull()
}

internal fun builtinTouchPresets(): List<TouchControlPreset> {
    val xbox = AndroidTouchSettings()
    val playstation = xbox.copy(buttonAppearances = mapOf(
        "A" to TouchButtonAppearance("×"), "B" to TouchButtonAppearance("○"),
        "X" to TouchButtonAppearance("□"), "Y" to TouchButtonAppearance("△"),
        "LB" to TouchButtonAppearance("L1"), "RB" to TouchButtonAppearance("R1"),
        "LT" to TouchButtonAppearance("L2"), "RT" to TouchButtonAppearance("R2"),
        "LS" to TouchButtonAppearance("L3"), "RS" to TouchButtonAppearance("R3"),
    ))
    val forzaHorizon = xbox.copy(
        touchControllerStyle = TouchControllerStyle.Neon,
        touchSkinTint = ControllerThemeRgb(255, 178, 48),
        extraButtonActions = listOf(
            TouchExtraButtonAction.A,
            TouchExtraButtonAction.B,
            TouchExtraButtonAction.X,
            TouchExtraButtonAction.Y,
            TouchExtraButtonAction.RightBumper,
            TouchExtraButtonAction.LeftBumper,
            TouchExtraButtonAction.DpadUp,
            TouchExtraButtonAction.DpadDown,
        ),
        buttonAppearances = mapOf(
            "RT" to TouchButtonAppearance("Gas", "Accelerate", TouchButtonShape.Trigger),
            "LT" to TouchButtonAppearance("Brake", "Brake", TouchButtonShape.Trigger),
            "A" to TouchButtonAppearance("E-brake", "Handbrake"),
            "B" to TouchButtonAppearance("Shift +", "ShiftUp"),
            "X" to TouchButtonAppearance("Shift -", "ShiftDown"),
            "Y" to TouchButtonAppearance("Rewind", "Reload"),
            "RB" to TouchButtonAppearance("Camera", "Photo"),
            "LB" to TouchButtonAppearance("Clutch", "Vehicle"),
            "extra1" to TouchButtonAppearance("E-brake", "Handbrake", TouchButtonShape.Circle),
            "extra2" to TouchButtonAppearance("Shift +", "ShiftUp", TouchButtonShape.Circle),
            "extra3" to TouchButtonAppearance("Shift -", "ShiftDown", TouchButtonShape.Circle),
            "extra4" to TouchButtonAppearance("Rewind", "Reload", TouchButtonShape.Circle),
            "extra5" to TouchButtonAppearance("Camera", "Photo", TouchButtonShape.Circle),
            "extra6" to TouchButtonAppearance("Clutch", "Vehicle", TouchButtonShape.Circle),
            "extra7" to TouchButtonAppearance("Radio", "Communicator", TouchButtonShape.Circle),
            "extra8" to TouchButtonAppearance("Telemetry", "Map", TouchButtonShape.Circle),
        ),
    )
    val residentEvil = playstation.copy(
        touchControllerStyle = TouchControllerStyle.Frost,
        touchSkinTint = ControllerThemeRgb(124, 178, 142),
        extraButtonActions = listOf(
            TouchExtraButtonAction.LeftTrigger,
            TouchExtraButtonAction.RightTrigger,
            TouchExtraButtonAction.A,
            TouchExtraButtonAction.X,
            TouchExtraButtonAction.B,
            TouchExtraButtonAction.LeftBumper,
            TouchExtraButtonAction.Y,
            TouchExtraButtonAction.RightBumper,
        ),
        buttonAppearances = playstation.buttonAppearances + mapOf(
            "LT" to TouchButtonAppearance("Aim", "Aim", TouchButtonShape.Trigger),
            "RT" to TouchButtonAppearance("Fire", "Shoot", TouchButtonShape.Trigger),
            "A" to TouchButtonAppearance("Use", "Interact"),
            "X" to TouchButtonAppearance("Reload", "Reload"),
            "B" to TouchButtonAppearance("Dodge", "Dodge"),
            "Y" to TouchButtonAppearance("Inventory", "Inventory"),
            "LB" to TouchButtonAppearance("Knife", "Knife"),
            "RB" to TouchButtonAppearance("Heal", "Heal"),
            "extra1" to TouchButtonAppearance("Aim", "Aim", TouchButtonShape.Circle),
            "extra2" to TouchButtonAppearance("Fire", "Shoot", TouchButtonShape.Circle),
            "extra3" to TouchButtonAppearance("Use", "Interact", TouchButtonShape.Circle),
            "extra4" to TouchButtonAppearance("Reload", "Reload", TouchButtonShape.Circle),
            "extra5" to TouchButtonAppearance("Dodge", "Dodge", TouchButtonShape.Diamond),
            "extra6" to TouchButtonAppearance("Knife", "Knife", TouchButtonShape.Circle),
            "extra7" to TouchButtonAppearance("Inventory", "Inventory", TouchButtonShape.Square),
            "extra8" to TouchButtonAppearance("Heal", "Heal", TouchButtonShape.Circle),
        ),
    )
    val firstPersonShooter = xbox.copy(
        extraButtonActions = listOf(
            TouchExtraButtonAction.LeftTrigger,
            TouchExtraButtonAction.RightTrigger,
            TouchExtraButtonAction.A,
            TouchExtraButtonAction.X,
            TouchExtraButtonAction.B,
            TouchExtraButtonAction.RightBumper,
            TouchExtraButtonAction.LeftBumper,
            TouchExtraButtonAction.RightStickClick,
        ),
        buttonAppearances = mapOf(
            "extra1" to TouchButtonAppearance("Aim", "Aim"),
            "extra2" to TouchButtonAppearance("Fire", "Shoot"),
            "extra3" to TouchButtonAppearance("Jump", "Jump"),
            "extra4" to TouchButtonAppearance("Reload", "Reload"),
            "extra5" to TouchButtonAppearance("Crouch", "Crouch"),
            "extra6" to TouchButtonAppearance("Grenade", "Grenade"),
            "extra7" to TouchButtonAppearance("Ability", "Sprint"),
            "extra8" to TouchButtonAppearance("Melee", "Melee"),
        ),
    )
    val actionAdventure = xbox.copy(
        touchControllerStyle = TouchControllerStyle.Arcade,
        extraButtonActions = listOf(
            TouchExtraButtonAction.A,
            TouchExtraButtonAction.B,
            TouchExtraButtonAction.X,
            TouchExtraButtonAction.Y,
            TouchExtraButtonAction.LeftBumper,
            TouchExtraButtonAction.RightBumper,
            TouchExtraButtonAction.LeftTrigger,
            TouchExtraButtonAction.RightTrigger,
        ),
        buttonAppearances = mapOf(
            "extra1" to TouchButtonAppearance("Jump", "Jump"),
            "extra2" to TouchButtonAppearance("Dodge", "Dodge"),
            "extra3" to TouchButtonAppearance("Attack", "Melee"),
            "extra4" to TouchButtonAppearance("Interact", "Interact"),
            "extra5" to TouchButtonAppearance("Guard", "Shield"),
            "extra6" to TouchButtonAppearance("Ability", "Sprint"),
            "extra7" to TouchButtonAppearance("Aim", "Aim"),
            "extra8" to TouchButtonAppearance("Use", "Interact"),
        ),
    )
    return listOf(
        TouchControlPreset("xbox", "Xbox", xbox),
        TouchControlPreset("playstation", "PlayStation", playstation),
        TouchControlPreset("forza-horizon", "Forza Horizon", forzaHorizon),
        TouchControlPreset("resident-evil", "Resident Evil", residentEvil),
        TouchControlPreset("first-person-shooter", "First-person shooter", firstPersonShooter),
        TouchControlPreset("action-adventure", "Action adventure", actionAdventure),
    )
}

/** Curated combinations; shared presets can carry any existing style, tint and button shapes. */
internal enum class TouchTheme(val titleRes: Int, val style: TouchControllerStyle, val tint: ControllerThemeRgb) {
    Aurora(R.string.touch_theme_aurora, TouchControllerStyle.Neon, ControllerThemeRgb(72, 235, 197)),
    Midnight(R.string.touch_theme_midnight, TouchControllerStyle.Frost, ControllerThemeRgb(128, 157, 255)),
    Rose(R.string.touch_theme_rose, TouchControllerStyle.Arcade, ControllerThemeRgb(255, 142, 196));

    fun apply(touch: AndroidTouchSettings): AndroidTouchSettings = touch.copy(touchControllerStyle = style, touchSkinTint = tint)
}
