package com.opencloudgaming.opennow

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.util.Locale
import kotlin.math.roundToInt

@Composable
internal fun SearchableSettingsSection(
    searchQuery: String,
    title: String,
    vararg keywords: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    if (settingsSearchMatches(searchQuery, title, *keywords)) {
        SettingsSection(title, content)
    }
}

private fun settingsSearchMatches(searchQuery: String, vararg terms: String): Boolean {
    val tokens = searchQuery.trim().lowercase(Locale.US).split(Regex("\\s+")).filter { it.isNotBlank() }
    if (tokens.isEmpty()) return true
    val haystack = terms.joinToString(" ").lowercase(Locale.US)
    return tokens.all { token -> token in haystack }
}

@Composable
private fun SettingsSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    val sectionShape = RoundedCornerShape(14.dp)
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        shape = sectionShape,
    ) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, color = SettingsText, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            content()
        }
    }
}

@Composable
internal fun SettingSwitch(label: String, checked: Boolean, enabled: Boolean = true, onCheckedChange: (Boolean) -> Unit) {
    val focusManager = LocalFocusManager.current
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(14.dp)
    val toggle = {
        if (enabled) {
            onCheckedChange(!checked)
        }
    }
    Row(
        Modifier
            .fillMaxWidth()
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .border(
                width = if (focused) 2.dp else 1.dp,
                color = if (focused) MaterialTheme.colorScheme.primary else Color.Transparent,
                shape = shape,
            )
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f))
            .clickable(enabled = enabled, onClick = toggle)
            .onPreviewKeyEvent { event ->
                when {
                    enabled && isTvActivateKey(event) -> {
                        toggle()
                        true
                    }
                    else -> handleVerticalDpadFocusMove(event, focusManager)
                }
            }
            .focusable()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val contentColor = MaterialTheme.colorScheme.onSurface.copy(alpha = if (enabled) 1f else 0.45f)
        Text(label, Modifier.weight(1f), color = contentColor, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Switch(checked = checked, enabled = enabled, onCheckedChange = onCheckedChange)
    }
}

@Composable
internal fun SessionProxyWarningDialog(onCancel: () -> Unit, onEnable: () -> Unit) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.settings_session_proxy_warning_title), fontWeight = FontWeight.Bold) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(stringResource(R.string.settings_session_proxy_warning_traffic), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.settings_session_proxy_warning_breakage), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.settings_session_proxy_warning_trust), style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = {
            Button(onClick = onEnable) {
                Text(stringResource(R.string.settings_session_proxy_warning_enable))
            }
        },
        dismissButton = {
            TextButton(onClick = onCancel) {
                Text(stringResource(R.string.action_cancel))
            }
        },
        containerColor = SettingsPanel,
        titleContentColor = SettingsText,
        textContentColor = SettingsTextMuted,
    )
}

@Composable
internal fun NumberSlider(label: String, value: Float, min: Float, max: Float, step: Float, onChange: (Float) -> Unit) {
    var local by remember(value) { mutableFloatStateOf(value) }
    val focusManager = LocalFocusManager.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurface, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(if (step < 1f) "%.2f".format(local) else local.roundToInt().toString(), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Slider(
            modifier = Modifier.onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
            value = local,
            onValueChange = { local = ((it / step).roundToInt() * step).coerceIn(min, max) },
            onValueChangeFinished = { onChange(local) },
            valueRange = min..max,
        )
    }
}

@Composable
internal fun ChoiceRow(label: String, options: List<String>, selected: String, onSelect: (String) -> Unit) {
    ChoiceMenuRow(
        label = label,
        options = options.map { ChoiceMenuOption(value = it, label = it) },
        selectedLabel = selected,
        onSelect = onSelect,
    )
}

@Composable
internal fun ChoiceMenuRow(
    label: String,
    options: List<ChoiceMenuOption>,
    selectedLabel: String,
    onSelect: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val autoLabel = stringResource(R.string.option_auto)
    val shape = RoundedCornerShape(14.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .border(
                width = if (focused) 2.dp else 1.dp,
                color = if (focused) MaterialTheme.colorScheme.primary else Color.Transparent,
                shape = shape,
            )
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f))
            .clickable { expanded = true }
            .onPreviewKeyEvent { event ->
                when {
                    isTvActivateKey(event) -> {
                        expanded = true
                        true
                    }
                    else -> handleVerticalDpadFocusMove(event, focusManager)
                }
            }
            .focusable()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurface, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Box {
            OutlinedButton(onClick = { expanded = true }) { Text(selectedLabel.ifBlank { autoLabel }, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                options.forEach { option ->
                    DropdownMenuItem(
                        text = {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Text(
                                    option.label,
                                    color = if (option.enabled) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.48f),
                                )
                                option.badge?.let { badge ->
                                    Text(
                                        badge,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                                        style = MaterialTheme.typography.labelSmall,
                                        fontWeight = FontWeight.Bold,
                                        modifier = Modifier
                                            .border(1.dp, MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f), RoundedCornerShape(3.dp))
                                            .padding(horizontal = 4.dp, vertical = 1.dp),
                                    )
                                }
                            }
                        },
                        enabled = option.enabled,
                        onClick = {
                            expanded = false
                            onSelect(option.value)
                        },
                    )
                }
            }
        }
    }
}

@Composable
internal fun ChoiceOptionRow(label: String, options: List<SettingsChoiceOption>, selectedValue: String, onSelect: (String) -> Unit) {
    val selectedLabel = options.firstOrNull { it.value == selectedValue }?.label ?: selectedValue
    ChoiceRow(label, options.map { it.label }, selectedLabel) { selected ->
        options.firstOrNull { it.label == selected }?.value?.let(onSelect)
    }
}

