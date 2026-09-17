package com.opencloudgaming.opennow

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import kotlin.math.roundToInt

internal enum class TouchButtonIcon(val vector: ImageVector, @StringRes val labelRes: Int) {
    Aim(TouchActionIcons.Aim, R.string.touch_icon_aim),
    Shoot(Icons.Default.GpsFixed, R.string.touch_icon_shoot),
    Jump(TouchActionIcons.Jump, R.string.touch_icon_jump),
    Run(Icons.Default.DirectionsRun, R.string.touch_icon_run),
    Crouch(TouchActionIcons.Crouch, R.string.touch_icon_crouch),
    Reload(TouchActionIcons.Reload, R.string.touch_icon_reload),
    Interact(Icons.Default.TouchApp, R.string.touch_icon_interact),
    Shield(Icons.Default.Shield, R.string.touch_icon_shield),
    Heal(Icons.Default.Favorite, R.string.touch_icon_heal),
    Inventory(Icons.Default.Backpack, R.string.touch_icon_inventory),
    Map(Icons.Default.Map, R.string.touch_icon_map),
    Menu(Icons.Default.Menu, R.string.touch_icon_menu),
    Flashlight(Icons.Default.FlashlightOn, R.string.touch_icon_flashlight),
    Communicator(Icons.Default.HeadsetMic, R.string.touch_icon_communicator),
    Microphone(Icons.Default.Mic, R.string.touch_icon_microphone),
    Grenade(Icons.Default.LocalFireDepartment, R.string.touch_icon_grenade),
    Melee(Icons.Default.SportsMartialArts, R.string.touch_icon_melee),
    Vehicle(Icons.Default.DirectionsCar, R.string.touch_icon_vehicle),
    Build(Icons.Default.Build, R.string.touch_icon_build),
    Photo(Icons.Default.PhotoCamera, R.string.touch_icon_photo),
    Flag(Icons.Default.Flag, R.string.touch_icon_flag),
    Star(Icons.Default.Star, R.string.touch_icon_star),
    Accelerate(TouchActionIcons.Accelerate, R.string.touch_icon_accelerate),
    Brake(TouchActionIcons.Brake, R.string.touch_icon_brake),
    Handbrake(TouchActionIcons.Handbrake, R.string.touch_icon_handbrake),
    ShiftUp(TouchActionIcons.ShiftUp, R.string.touch_icon_shift_up),
    ShiftDown(TouchActionIcons.ShiftDown, R.string.touch_icon_shift_down),
    Horn(TouchActionIcons.Horn, R.string.touch_icon_horn),
    Dodge(TouchActionIcons.Dodge, R.string.touch_icon_dodge),
    Knife(TouchActionIcons.Knife, R.string.touch_icon_knife),
    Door(TouchActionIcons.Door, R.string.touch_icon_door),
    Key(Icons.Default.Key, R.string.touch_icon_key),
    Puzzle(Icons.Default.Extension, R.string.touch_icon_puzzle),
    Sprint(Icons.Default.Bolt, R.string.touch_icon_sprint),
}

internal val LocalTouchButtonAppearances = staticCompositionLocalOf<Map<String, TouchButtonAppearance>> { emptyMap() }

@Composable
private fun touchButtonName(key: String): String = when {
    key.startsWith("extra") -> stringResource(R.string.settings_touch_extra_button, key.removePrefix("extra").toInt())
    key == "◀" -> "Select"
    key == "▶" -> "Start"
    else -> key
}

/** One editor shared by Settings and the stream panel; drafts apply only when saved. */
@Composable
internal fun TouchButtonAppearanceEditor(touch: AndroidTouchSettings, onChange: (AndroidTouchSettings) -> Unit) {
    var open by rememberSaveable { mutableStateOf(false) }
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    OutlinedButton(onClick = { selected = null; open = true }, modifier = Modifier.fillMaxWidth()) {
        Text(stringResource(R.string.touch_customize_buttons))
    }
    if (!open) return
    val button = selected
    if (button == null) {
        AlertDialog(
            onDismissRequest = { open = false },
            title = { Text(stringResource(R.string.touch_customize_buttons)) },
            text = {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    Text(stringResource(R.string.touch_customize_description))
                    touchButtonKeys.forEach { key ->
                        TextButton(onClick = { selected = key }, modifier = Modifier.fillMaxWidth()) {
                            val label = touch.buttonAppearances[key]?.label.orEmpty()
                            Text(touchButtonName(key) + if (label.isNotBlank()) " · $label" else "")
                        }
                    }
                }
            },
            confirmButton = { TextButton(onClick = { open = false }) { Text(stringResource(R.string.touch_editor_close)) } },
        )
    } else {
        TouchButtonAppearanceDialog(button, touch, onChange, onDismiss = { selected = null })
    }
}

/** Also opened by tapping a button while editing the streaming layout. */
@Composable
internal fun TouchButtonAppearanceDialog(
    button: String,
    touch: AndroidTouchSettings,
    onChange: (AndroidTouchSettings) -> Unit,
    onDismiss: () -> Unit,
) {
    val saved = (touch.buttonAppearances[button] ?: TouchButtonAppearance()).normalized()
    var label by rememberSaveable(button) { mutableStateOf(saved.label) }
    var icon by rememberSaveable(button) { mutableStateOf(saved.icon) }
    var shape by rememberSaveable(button) { mutableStateOf(saved.shape) }
    var sizeScale by rememberSaveable(button) { mutableStateOf(saved.sizeScale) }
    var toggle by rememberSaveable(button) { mutableStateOf(saved.toggle) }
    AlertDialog(
        onDismissRequest = { onDismiss() },
        title = { Text(touchButtonName(button)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                CompositionLocalProvider(
                    LocalTouchSkin provides touchSkinColors(touch.touchControllerStyle, 1f, touchSkinAccent(touch)),
                    LocalTouchSkinForm provides touchSkinForm(touch.touchControllerStyle),
                    LocalTouchButtonLabels provides true,
                    LocalTouchButtonAppearances provides mapOf(button to TouchButtonAppearance(label, icon, shape, toggle, sizeScale)),
                ) {
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                        TouchCapFace(touchButtonName(button), false, 72.dp, appearanceKey = button)
                    }
                }
                OutlinedTextField(
                    value = label,
                    onValueChange = { label = it.take(TOUCH_BUTTON_LABEL_LIMIT) },
                    label = { Text(stringResource(R.string.touch_custom_label)) },
                    placeholder = { Text(touchButtonName(button)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                val sizeLabel = stringResource(R.string.touch_button_size)
                Text(stringResource(R.string.touch_button_size_value, (sizeScale * 100).roundToInt()),
                    style = MaterialTheme.typography.titleSmall)
                Slider(
                    value = sizeScale,
                    onValueChange = { sizeScale = (it * 20).roundToInt() / 20f },
                    valueRange = TOUCH_BUTTON_MIN_SIZE_SCALE..TOUCH_BUTTON_MAX_SIZE_SCALE,
                    steps = 29,
                    modifier = Modifier.semantics { contentDescription = sizeLabel },
                )
                Text(stringResource(R.string.touch_button_size_hint), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.touch_button_shape), style = MaterialTheme.typography.titleSmall)
                TouchButtonShape.entries.chunked(3).forEach { choices ->
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        choices.forEach { choice ->
                            FilterChip(selected = shape == choice, onClick = { shape = choice },
                                label = { Text(touchButtonShapeLabel(choice)) })
                        }
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(R.string.touch_button_toggle), modifier = Modifier.weight(1f))
                    Switch(checked = toggle, onCheckedChange = { toggle = it })
                }
                Text(stringResource(R.string.touch_button_toggle_hint), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.touch_custom_icon), style = MaterialTheme.typography.titleSmall)
                FilterChip(selected = icon == null, onClick = { icon = null },
                    label = { Text(stringResource(R.string.touch_icon_text)) })
                TouchButtonIcon.entries.chunked(3).forEach { row ->
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        row.forEach { choice ->
                            val name = stringResource(choice.labelRes)
                            OutlinedButton(
                                onClick = { icon = choice.name },
                                modifier = Modifier.weight(1f),
                                contentPadding = PaddingValues(4.dp),
                                colors = ButtonDefaults.outlinedButtonColors(
                                    containerColor = if (icon == choice.name) MaterialTheme.colorScheme.secondaryContainer
                                        else androidx.compose.ui.graphics.Color.Transparent,
                                ),
                            ) {
                                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                    Icon(choice.vector, contentDescription = name, modifier = Modifier.size(24.dp))
                                    Text(name, style = MaterialTheme.typography.labelSmall, maxLines = 1)
                                }
                            }
                        }
                    }
                }
                if (!touch.touchButtonLabels) Text(stringResource(R.string.touch_labels_enable_hint))
                TextButton(onClick = { label = ""; icon = null; shape = TouchButtonShape.Theme; toggle = false; sizeScale = 1f }) { Text(stringResource(R.string.touch_editor_reset)) }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                onChange(touch.withButtonAppearance(button, TouchButtonAppearance(label, icon, shape, toggle, sizeScale)))
                onDismiss()
            }) { Text(stringResource(R.string.touch_editor_save)) }
        },
        dismissButton = { TextButton(onClick = { onDismiss() }) { Text(stringResource(R.string.touch_editor_cancel)) } },
    )
}

@Composable
private fun touchButtonShapeLabel(shape: TouchButtonShape): String = stringResource(when (shape) {
    TouchButtonShape.Theme -> R.string.touch_shape_theme
    TouchButtonShape.Circle -> R.string.touch_shape_circle
    TouchButtonShape.Square -> R.string.touch_shape_square
    TouchButtonShape.Rounded -> R.string.touch_shape_rounded
    TouchButtonShape.Hexagon -> R.string.touch_shape_hexagon
    TouchButtonShape.Diamond -> R.string.touch_shape_diamond
    TouchButtonShape.Octagon -> R.string.touch_shape_octagon
    TouchButtonShape.Trigger -> R.string.touch_shape_trigger
})
