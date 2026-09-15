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
    return listOf(TouchControlPreset("xbox", "Xbox", xbox), TouchControlPreset("playstation", "PlayStation", playstation))
}

/** Curated combinations; shared presets can carry any existing style, tint and button shapes. */
internal enum class TouchTheme(val titleRes: Int, val style: TouchControllerStyle, val tint: ControllerThemeRgb) {
    Aurora(R.string.touch_theme_aurora, TouchControllerStyle.Neon, ControllerThemeRgb(72, 235, 197)),
    Midnight(R.string.touch_theme_midnight, TouchControllerStyle.Frost, ControllerThemeRgb(128, 157, 255)),
    Rose(R.string.touch_theme_rose, TouchControllerStyle.Arcade, ControllerThemeRgb(255, 142, 196));

    fun apply(touch: AndroidTouchSettings): AndroidTouchSettings = touch.copy(touchControllerStyle = style, touchSkinTint = tint)
}
