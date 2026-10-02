package com.opencloudgaming.opennow

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.DisposableEffect
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.controls.ControlRow
import com.opencloudgaming.opennow.ui.controls.ControlRowLabels
import com.opencloudgaming.opennow.ui.controls.ControlSection
import com.opencloudgaming.opennow.ui.controls.ControlSliderRow
import com.opencloudgaming.opennow.ui.controls.ControlSwitchRow
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
    ControlSection(title = title, content = content)
}

@Composable
internal fun SettingSwitch(
    label: String,
    checked: Boolean,
    enabled: Boolean = true,
    description: String? = null,
    indentLevel: Int = 0,
    onCheckedChange: (Boolean) -> Unit,
) {
    InspectableSetting(SettingInspection.Switch(label, description, checked, enabled, onCheckedChange)) {
        ControlSwitchRow(
            label = label,
            checked = checked,
            onCheckedChange = onCheckedChange,
            description = description,
            enabled = enabled,
            indentLevel = indentLevel,
        )
    }
}

/**
 * Publishes a large touch version of a setting to the bottom screen while the setting holds
 * controller focus on the top screen. Single-screen devices simply never read it.
 */
@Composable
private fun InspectableSetting(inspection: SettingInspection, content: @Composable () -> Unit) {
    val owner = remember { Any() }
    var focused by remember { mutableStateOf(false) }
    Box(Modifier.fillMaxWidth().onFocusChanged { focused = it.hasFocus }) {
        content()
    }
    if (focused) {
        SideEffect { DualScreenBridge.inspect(owner, inspection) }
    }
    LaunchedEffect(focused) {
        if (!focused) DualScreenBridge.clearInspection(owner)
    }
    DisposableEffect(owner) {
        onDispose { DualScreenBridge.clearInspection(owner) }
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

/**
 * Turns a raw slider value into something readable.
 *
 * Every sub-integer slider used to render as `"%.2f"`, so opacity showed `0.75` and card size
 * showed `1.00` — numbers with no stated unit and no obvious meaning. Fractional 0..1 ranges now
 * read as percentages, and anything else gets its unit appended.
 */
internal fun formatSliderValue(
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    unit: String? = null,
    valueFormatter: ((Float) -> String)? = null,
): String {
    valueFormatter?.let { return it(value) }
    val isFraction = step < 1f
    val looksLikeRatio = isFraction && min >= 0f && max <= 2f
    return when {
        looksLikeRatio && unit == null -> "${(value * 100f).roundToInt()}%"
        isFraction -> buildString {
            append("%.2f".format(value))
            unit?.let { append(' ').append(it) }
        }
        else -> buildString {
            append(value.roundToInt())
            unit?.let { append(' ').append(it) }
        }
    }
}

@Composable
internal fun NumberSlider(
    label: String,
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    /** Appended to the value, e.g. "FPS", "ms", "dp". Ignored when [valueFormatter] is supplied. */
    unit: String? = null,
    /** Full control over the readout when neither the percent nor the unit default fits. */
    valueFormatter: ((Float) -> String)? = null,
    description: String? = null,
    descriptionProvider: ((Float) -> String?)? = null,
    onChange: (Float) -> Unit,
) {
    val inspection = SettingInspection.Slider(
        label = label,
        description = descriptionProvider?.invoke(value) ?: description,
        value = value,
        min = min,
        max = max,
        step = step,
        valueText = formatSliderValue(value, min, max, step, unit, valueFormatter),
        onChange = onChange,
    )
    InspectableSetting(inspection) {
        ControlSliderRow(
            label = label,
            value = value,
            min = min,
            max = max,
            step = step,
            onChange = onChange,
            unit = unit,
            valueFormatter = valueFormatter,
            description = description,
            descriptionProvider = descriptionProvider,
        )
    }
}

@Composable
internal fun ChoiceRow(
    label: String,
    options: List<String>,
    selected: String,
    description: String? = null,
    activeOutlineColor: Color? = null,
    activeOutlineSecondaryColor: Color? = null,
    onSelect: (String) -> Unit,
) {
    ChoiceMenuRow(
        label = label,
        options = options.map { ChoiceMenuOption(value = it, label = it) },
        selectedLabel = selected,
        description = description,
        activeOutlineColor = activeOutlineColor,
        activeOutlineSecondaryColor = activeOutlineSecondaryColor,
        onSelect = onSelect,
    )
}

@Composable
internal fun ChoiceMenuRow(
    label: String,
    options: List<ChoiceMenuOption>,
    selectedLabel: String,
    description: String? = null,
    activeOutlineColor: Color? = null,
    activeOutlineSecondaryColor: Color? = null,
    onSelect: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    var descriptionExpanded by remember(label) { mutableStateOf(false) }
    BackHandler(enabled = expanded) { expanded = false }
    val autoLabel = stringResource(R.string.option_auto)
    // Outer chrome comes from the shared row; the dropdown body below is specific to this control.
    InspectableSetting(SettingInspection.Choice(label, description, options, selectedLabel.ifBlank { autoLabel }, onSelect)) {
    ControlRow(onClick = { expanded = true }) {
        ControlRowLabels(
            label = label,
            value = null,
            expandedDescription = description?.takeIf { descriptionExpanded },
            enabled = true,
            style = com.opencloudgaming.opennow.ui.controls.controlRowStyle(),
        )
        if (!description.isNullOrBlank()) {
            IconButton(
                onClick = { descriptionExpanded = !descriptionExpanded },
                modifier = Modifier.padding(horizontal = 2.dp),
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_help),
                    contentDescription = stringResource(
                        if (descriptionExpanded) R.string.control_hide_description
                        else R.string.control_show_description,
                    ),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
        Box {
            OutlinedButton(onClick = { expanded = true }) { Text(selectedLabel.ifBlank { autoLabel }, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            ControllerFocusFrame(
                visible = activeOutlineColor != null,
                cornerRadius = 20.dp,
                tint = activeOutlineColor,
                secondaryTint = activeOutlineSecondaryColor,
                // Material's button keeps a 48 dp touch target around its visible 40 dp pill.
                // Follow the pill so the Cinema orbit is vertically centered on the border.
                verticalInset = 4.dp,
            )
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                options.forEach { option ->
                    var optionFocused by remember(option.value) { mutableStateOf(false) }
                    Box {
                        DropdownMenuItem(
                            text = {
                                val disabledAlpha = if (option.enabled) 1f else 0.48f
                                val badgeAlpha = if (option.enabled) 0.7f else 0.48f
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    Text(
                                        option.label,
                                        color = if (option.enabled) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = disabledAlpha),
                                    )
                                    option.badge?.let { badge ->
                                        Text(
                                            badge,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = badgeAlpha),
                                            style = MaterialTheme.typography.labelSmall,
                                            fontWeight = FontWeight.Bold,
                                            modifier = Modifier
                                                .border(1.dp, MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = if (option.enabled) 0.3f else 0.2f), RoundedCornerShape(3.dp))
                                                .padding(horizontal = 4.dp, vertical = 1.dp),
                                        )
                                    }
                                }
                            },
                            enabled = option.enabled,
                            modifier = Modifier.onFocusChanged {
                                optionFocused = option.enabled && (it.isFocused || it.hasFocus)
                            },
                            onClick = {
                                expanded = false
                                onSelect(option.value)
                            },
                        )
                        InteractionFocusFrame(
                            visible = optionFocused,
                            cornerRadius = 4.dp,
                            cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
                        )
                        ControllerFocusFrame(
                            visible = !optionFocused && option.label == selectedLabel && activeOutlineColor != null,
                            cornerRadius = 4.dp,
                            tint = activeOutlineColor,
                            secondaryTint = activeOutlineSecondaryColor,
                        )
                    }
                }
            }
        }
    }
}
}

@Composable
internal fun ChoiceOptionRow(
    label: String,
    options: List<SettingsChoiceOption>,
    selectedValue: String,
    description: String? = null,
    onSelect: (String) -> Unit,
) {
    val selectedLabel = options.firstOrNull { it.value == selectedValue }?.label ?: selectedValue
    ChoiceRow(label, options.map { it.label }, selectedLabel, description = description) { selected ->
        options.firstOrNull { it.label == selected }?.value?.let(onSelect)
    }
}
