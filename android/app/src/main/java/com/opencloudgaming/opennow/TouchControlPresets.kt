package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.longOrNull
import org.snakeyaml.engine.v2.api.Dump
import org.snakeyaml.engine.v2.api.DumpSettings
import org.snakeyaml.engine.v2.api.Load
import org.snakeyaml.engine.v2.api.LoadSettings
import org.snakeyaml.engine.v2.common.FlowStyle
import org.snakeyaml.engine.v2.schema.JsonSchema
import kotlinx.serialization.json.Json
import java.util.UUID

internal const val MAX_TOUCH_PRESETS = 40
internal const val MAX_TOUCH_PRESET_CODE = 65536

@Serializable
data class TouchControlPreset(
    val id: String,
    val name: String,
    val controls: AndroidTouchSettings,
    val genre: TouchPresetGenre = TouchPresetGenre.Custom,
    val description: String = "",
)

@Serializable
private data class SharedTouchPreset(
    val format: String = "opennow-touch",
    val version: Int = 1,
    val name: String,
    val genre: TouchPresetGenre = TouchPresetGenre.Custom,
    val description: String = "",
    val controls: AndroidTouchSettings,
)

private val touchPresetJson = Json { ignoreUnknownKeys = true; encodeDefaults = true }
private val touchPresetYamlLoader = Load(
    LoadSettings.builder()
        .setLabel("OpenNOW touch preset")
        .setSchema(JsonSchema())
        .setAllowDuplicateKeys(false)
        .setAllowRecursiveKeys(false)
        .setAllowNonScalarKeys(false)
        .setMaxAliasesForCollections(16)
        .setCodePointLimit(MAX_TOUCH_PRESET_CODE)
        .build(),
)
private val touchPresetYamlDumper = Dump(
    DumpSettings.builder()
        .setSchema(JsonSchema())
        .setDefaultFlowStyle(FlowStyle.BLOCK)
        .setIndent(2)
        .setIndicatorIndent(2)
        .setIndentWithIndicator(true)
        .setSplitLines(false)
        .setWidth(120)
        .build(),
)

internal fun AndroidTouchSettings.normalizedPresetControls(): AndroidTouchSettings =
    normalizedTouchControls()

internal fun newTouchPreset(
    name: String,
    controls: AndroidTouchSettings,
    genre: TouchPresetGenre = TouchPresetGenre.Custom,
    description: String = "",
): TouchControlPreset = TouchControlPreset(
    id = UUID.randomUUID().toString(),
    name = name.filterNot(Char::isISOControl).trim().take(64),
    controls = controls.normalizedPresetControls(),
    genre = genre,
    description = description.filterNot(Char::isISOControl).trim().take(240),
)

/** Applying a layout must not change input mode or hide an active overlay. */
internal fun AndroidTouchSettings.applyingTouchPreset(preset: TouchControlPreset): AndroidTouchSettings =
    preset.controls.normalizedPresetControls().copy(
        enabled = enabled, mousePad = mousePad, mouseDirectClick = mouseDirectClick,
        nativeTouchMode = nativeTouchMode, nativeTouchOptedIn = nativeTouchOptedIn,
    )

internal fun exportTouchPreset(preset: TouchControlPreset): String {
    val shared = SharedTouchPreset(
        name = preset.name,
        genre = preset.genre,
        description = preset.description,
        controls = preset.controls.normalizedPresetControls(),
    )
    val yamlValue = touchPresetJson.encodeToJsonElement(shared).toYamlValue()
    return touchPresetYamlDumper.dumpToString(yamlValue)
}

internal fun exportTouchPresetJson(preset: TouchControlPreset): String = touchPresetJson.encodeToString(
    SharedTouchPreset(
        name = preset.name,
        genre = preset.genre,
        description = preset.description,
        controls = preset.controls.normalizedPresetControls(),
    ),
)

internal fun importTouchPreset(code: String): TouchControlPreset? {
    if (code.length > MAX_TOUCH_PRESET_CODE) return null
    return runCatching {
        val trimmed = code.trim()
        val shared = if (trimmed.startsWith("{")) {
            touchPresetJson.decodeFromString<SharedTouchPreset>(trimmed)
        } else {
            val value = touchPresetYamlLoader.loadFromString(trimmed)
            touchPresetJson.decodeFromJsonElement<SharedTouchPreset>(value.toJsonElement())
        }
        require(shared.format == "opennow-touch" && shared.version == 1)
        require(shared.controls.offsets.size <= 100 && shared.controls.buttonAppearances.size <= touchButtonKeys.size)
        newTouchPreset(shared.name, shared.controls, shared.genre, shared.description)
            .also { require(it.name.isNotBlank()) }
    }.getOrNull()
}

private fun JsonElement.toYamlValue(): Any? = when (this) {
    JsonNull -> null
    is JsonObject -> entries.associateTo(linkedMapOf()) { (key, value) -> key to value.toYamlValue() }
    is JsonArray -> map(JsonElement::toYamlValue)
    is JsonPrimitive -> when {
        isString -> content
        booleanOrNull != null -> booleanOrNull
        longOrNull != null -> longOrNull
        doubleOrNull != null -> doubleOrNull
        else -> contentOrNull
    }
}

private fun Any?.toJsonElement(): JsonElement = when (this) {
    null -> JsonNull
    is String -> JsonPrimitive(this)
    is Boolean -> JsonPrimitive(this)
    is Number -> JsonPrimitive(this)
    is List<*> -> JsonArray(map(Any?::toJsonElement))
    is Map<*, *> -> JsonObject(entries.associate { (key, value) ->
        require(key is String) { "Preset keys must be text" }
        key to value.toJsonElement()
    })
    else -> error("Unsupported YAML value: ${this::class.simpleName}")
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
    val racing = xbox.copy(
        touchControllerStyle = TouchControllerStyle.Neon,
        touchSkinTint = ControllerThemeRgb(255, 178, 48),
        extraButtonActions = listOf(
            TouchExtraButtonAction.LeftStickLeft,
            TouchExtraButtonAction.LeftStickRight,
            TouchExtraButtonAction.RightTrigger,
            TouchExtraButtonAction.LeftTrigger,
            TouchExtraButtonAction.A,
            TouchExtraButtonAction.B,
            TouchExtraButtonAction.X,
            TouchExtraButtonAction.Y,
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
            "extra1" to TouchButtonAppearance("Left · A", "SteerLeft", TouchButtonShape.Circle),
            "extra2" to TouchButtonAppearance("Right · D", "SteerRight", TouchButtonShape.Circle),
            "extra3" to TouchButtonAppearance("Gas", "Accelerate", TouchButtonShape.Trigger),
            "extra4" to TouchButtonAppearance("Brake", "Brake", TouchButtonShape.Trigger),
            "extra5" to TouchButtonAppearance("E-brake", "Handbrake", TouchButtonShape.Circle),
            "extra6" to TouchButtonAppearance("Shift +", "ShiftUp", TouchButtonShape.Circle),
            "extra7" to TouchButtonAppearance("Shift -", "ShiftDown", TouchButtonShape.Circle),
            "extra8" to TouchButtonAppearance("Rewind", "Reload", TouchButtonShape.Circle),
        ),
    )
    val horror = playstation.copy(
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
    val survival = xbox.copy(
        touchControllerStyle = TouchControllerStyle.Frost,
        touchSkinTint = ControllerThemeRgb(105, 153, 92),
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
            "extra2" to TouchButtonAppearance("Crouch", "Crouch"),
            "extra3" to TouchButtonAppearance("Interact", "Interact"),
            "extra4" to TouchButtonAppearance("Inventory", "Inventory", TouchButtonShape.Square),
            "extra5" to TouchButtonAppearance("Build", "Build"),
            "extra6" to TouchButtonAppearance("Tool", "Melee"),
            "extra7" to TouchButtonAppearance("Aim", "Aim", TouchButtonShape.Trigger),
            "extra8" to TouchButtonAppearance("Use", "Shoot", TouchButtonShape.Trigger),
        ),
    )
    val action = xbox.copy(
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
        TouchControlPreset("xbox", "Xbox", xbox, description = "Neutral Xbox-style layout"),
        TouchControlPreset("playstation", "PlayStation", playstation, description = "Neutral PlayStation-style labels"),
        TouchControlPreset(
            "racing", "Racing", racing, TouchPresetGenre.Racing,
            "Steering buttons, throttle, brake, handbrake, shifting and rewind",
        ),
        TouchControlPreset(
            "survival", "Survival", survival, TouchPresetGenre.Survival,
            "Movement, interaction, inventory, building, tools and aiming",
        ),
        TouchControlPreset(
            "horror", "Horror", horror, TouchPresetGenre.Horror,
            "Aim, fire, interact, reload, evade, inventory and healing",
        ),
        TouchControlPreset(
            "action", "Action", action, TouchPresetGenre.Action,
            "Jump, dodge, attack, interact, guard and abilities",
        ),
    )
}

/** Curated combinations; shared presets can carry any existing style, tint and button shapes. */
internal enum class TouchTheme(val titleRes: Int, val style: TouchControllerStyle, val tint: ControllerThemeRgb) {
    Aurora(R.string.touch_theme_aurora, TouchControllerStyle.Neon, ControllerThemeRgb(72, 235, 197)),
    Midnight(R.string.touch_theme_midnight, TouchControllerStyle.Frost, ControllerThemeRgb(128, 157, 255)),
    Rose(R.string.touch_theme_rose, TouchControllerStyle.Arcade, ControllerThemeRgb(255, 142, 196));

    fun apply(touch: AndroidTouchSettings): AndroidTouchSettings = touch.copy(touchControllerStyle = style, touchSkinTint = tint)
}
