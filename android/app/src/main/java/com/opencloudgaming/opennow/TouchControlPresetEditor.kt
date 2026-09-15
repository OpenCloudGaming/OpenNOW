package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp

/** Shared by app settings and the in-stream editor. Preset names can be game names. */
@Composable
internal fun TouchControlPresetEditor(
    touch: AndroidTouchSettings,
    presets: List<TouchControlPreset>,
    onTouchChange: (AndroidTouchSettings) -> Unit,
    onPresetsChange: (List<TouchControlPreset>) -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    var open by rememberSaveable { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var code by rememberSaveable { mutableStateOf("") }
    var sharing by rememberSaveable { mutableStateOf(false) }
    var invalid by rememberSaveable { mutableStateOf(false) }
    var notice by remember { mutableStateOf<String?>(null) }
    val applied = stringResource(R.string.touch_preset_applied)
    val saved = stringResource(R.string.touch_preset_saved)
    val deleted = stringResource(R.string.touch_preset_deleted)
    OutlinedButton(onClick = { open = true }, modifier = Modifier.fillMaxWidth()) {
        Text(stringResource(R.string.touch_presets_themes))
    }
    if (!open) return
    AlertDialog(
        onDismissRequest = { open = false },
        title = { Text(stringResource(R.string.touch_presets_themes)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.touch_presets_hint))
                notice?.let { Text(it, color = MaterialTheme.colorScheme.primary) }
                Text(stringResource(R.string.touch_presets_builtin), style = MaterialTheme.typography.titleSmall)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    builtinTouchPresets().forEach { preset ->
                        OutlinedButton(onClick = { onTouchChange(touch.applyingTouchPreset(preset)); notice = applied }) { Text(preset.name) }
                    }
                }
                Text(stringResource(R.string.touch_themes), style = MaterialTheme.typography.titleSmall)
                TouchControllerSkinPreview(touch.touchControllerStyle, touch.touchSkinTint, touch.opacity, touch.touchButtonLabels)
                TouchTheme.entries.forEach { theme ->
                    OutlinedButton(onClick = { onTouchChange(theme.apply(touch)); notice = applied }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(theme.titleRes))
                    }
                }
                OutlinedTextField(value = name, onValueChange = { name = it.take(64) }, singleLine = true,
                    label = { Text(stringResource(R.string.touch_preset_name)) }, modifier = Modifier.fillMaxWidth())
                Button(onClick = {
                    onPresetsChange(presets + newTouchPreset(name, touch)); name = ""; notice = saved
                }, enabled = name.isNotBlank() && presets.size < MAX_TOUCH_PRESETS, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(R.string.touch_preset_save_current))
                }
                if (presets.size >= MAX_TOUCH_PRESETS) Text(stringResource(R.string.touch_preset_limit))
                presets.forEach { preset ->
                    HorizontalDivider()
                    Text(preset.name, style = MaterialTheme.typography.titleSmall)
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        TextButton(onClick = { onTouchChange(touch.applyingTouchPreset(preset)); notice = applied }) {
                            Text(stringResource(R.string.touch_preset_load))
                        }
                        TextButton(onClick = {
                            onPresetsChange(presets.map { if (it.id == preset.id) it.copy(controls = touch.normalizedPresetControls()) else it }); notice = saved
                        }) { Text(stringResource(R.string.touch_preset_replace)) }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        TextButton(onClick = { code = exportTouchPreset(preset); invalid = false; sharing = true }) {
                            Text(stringResource(R.string.touch_preset_share))
                        }
                        TextButton(onClick = { onPresetsChange(presets.filterNot { it.id == preset.id }); notice = deleted }) {
                            Text(stringResource(R.string.touch_preset_delete))
                        }
                    }
                }
                OutlinedButton(onClick = { code = ""; invalid = false; sharing = true }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(R.string.touch_preset_import))
                }
            }
        },
        confirmButton = { TextButton(onClick = { open = false }) { Text(stringResource(R.string.touch_editor_close)) } },
    )
    if (sharing) AlertDialog(
        onDismissRequest = { sharing = false },
        title = { Text(stringResource(R.string.touch_preset_code)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.touch_preset_code_hint))
                OutlinedTextField(value = code, onValueChange = { code = it.take(MAX_TOUCH_PRESET_CODE + 1); invalid = false },
                    isError = invalid, modifier = Modifier.fillMaxWidth().heightIn(max = 240.dp))
                TextButton(onClick = { clipboard.setText(AnnotatedString(code)) }, enabled = code.isNotBlank()) {
                    Text(stringResource(R.string.touch_preset_copy))
                }
                if (invalid) Text(stringResource(R.string.touch_preset_invalid), color = MaterialTheme.colorScheme.error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val imported = importTouchPreset(code)
                if (imported == null) invalid = true else {
                    onPresetsChange(presets + imported); notice = saved; sharing = false
                }
            }, enabled = presets.size < MAX_TOUCH_PRESETS && code.isNotBlank()) { Text(stringResource(R.string.touch_preset_import)) }
        },
        dismissButton = { TextButton(onClick = { sharing = false }) { Text(stringResource(R.string.touch_editor_close)) } },
    )
}
