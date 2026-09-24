package com.opencloudgaming.opennow

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

@Composable
internal fun SelectionEffectColorSetting(settings: AppSettings, onSave: (SelectionEffectColors?) -> Unit) {
    var open by rememberSaveable { mutableStateOf(false) }
    OutlinedButton(onClick = { open = true }, modifier = Modifier.fillMaxWidth()) {
        Text(if (settings.selectionEffectColors == null) "Effect colors · Follow theme" else "Effect colors · Custom")
    }
    if (open) {
        val style = settings.activeSelectionEffectStyle()
        val colors = controllerFocusEnergyColors(settings.uiAccent == UiAccent.AbsoluteCinema,
            style.color, style.secondaryColor, settings.selectionEffectColors)
        EffectColorDialog(
            initial = SelectionEffectColors(colors.first.toArgb(), colors.second.toArgb()).normalized(),
            onDismiss = { open = false },
            onSave = { onSave(it); open = false },
        )
    }
}

@Composable
private fun EffectColorDialog(initial: SelectionEffectColors, onDismiss: () -> Unit, onSave: (SelectionEffectColors?) -> Unit) {
    var first by rememberSaveable { mutableStateOf(effectColorHex(initial.firstRgb)) }
    var second by rememberSaveable { mutableStateOf(effectColorHex(initial.secondRgb)) }
    var editingSecond by rememberSaveable { mutableStateOf(false) }
    val firstRgb = parseEffectColor(first)
    val secondRgb = parseEffectColor(second)
    val draft = SelectionEffectColors(firstRgb ?: initial.firstRgb, secondRgb ?: initial.secondRgb)
    val current = if (editingSecond) draft.secondRgb else draft.firstRgb
    fun update(rgb: Int) {
        if (editingSecond) second = effectColorHex(rgb) else first = effectColorHex(rgb)
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Effect colors") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Choose two colors for selection effects. Your interface theme stays the same.", style = MaterialTheme.typography.bodySmall)
                CompositionLocalProvider(LocalSelectionEffectColors provides draft,
                    LocalAbsoluteCinemaEffects provides true, LocalAbsoluteCinemaPalette provides false) {
                    Box(Modifier.fillMaxWidth().padding(8.dp).height(64.dp)
                        .background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(16.dp)),
                        contentAlignment = Alignment.Center) {
                        Text("Live preview")
                        ControllerFocusFrame(visible = true, cornerRadius = 16.dp)
                    }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(value = first, onValueChange = { first = it },
                        label = { Text("First · #RRGGBB") }, singleLine = true,
                        isError = firstRgb == null, modifier = Modifier.weight(1f))
                    OutlinedTextField(value = second, onValueChange = { second = it },
                        label = { Text("Second · #RRGGBB") }, singleLine = true,
                        isError = secondRgb == null, modifier = Modifier.weight(1f))
                }
                if (firstRgb == null || secondRgb == null) {
                    Text("Enter six hexadecimal digits, for example #33CCFF.", color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall)
                }
                ChoiceRow("Adjust color", listOf("First", "Second"), if (editingSecond) "Second" else "First") {
                    editingSecond = it == "Second"
                }
                listOf("Red" to 16, "Green" to 8, "Blue" to 0).forEach { (label, shift) ->
                    NumberSlider(label, ((current shr shift) and 255).toFloat(), 0f, 255f, 1f) { value ->
                        update((current and (255 shl shift).inv()) or (value.roundToInt() shl shift))
                    }
                }
                ChoiceRow("Preset", listOf("Violet & ice", "Cinema", "Mint & sky", "White"), "Choose colors") { preset ->
                    val pair = when (preset) {
                        "Cinema" -> 0xFF6A2B to 0x42C9FF
                        "Mint & sky" -> 0x72F1B8 to 0x67CFFF
                        "White" -> 0xFFFFFF to 0xFFFFFF
                        else -> 0xB49BFF to 0xCBF0FF
                    }
                    first = effectColorHex(pair.first); second = effectColorHex(pair.second)
                }
                TextButton(onClick = { onSave(null) }) { Text("Follow theme colors") }
            }
        },
        confirmButton = {
            TextButton(enabled = firstRgb != null && secondRgb != null, onClick = { onSave(draft.normalized()) }) { Text("Save colors") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
