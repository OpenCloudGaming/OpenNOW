package com.opencloudgaming.opennow.ui.controls

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.LocalSpaceUiPresentation
import com.opencloudgaming.opennow.rememberSpaceInteraction
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.InteractionFocusFrame
import com.opencloudgaming.opennow.LocalAbsoluteCinemaEverywhere
import com.opencloudgaming.opennow.formatSliderValue
import com.opencloudgaming.opennow.handleSliderDpadInput
import com.opencloudgaming.opennow.ui.theme.numeric
import kotlin.math.roundToInt

/**
 * A labelled toggle.
 *
 * [value] is an always-visible subtitle reflecting the current state ("Muted", "Fixed") — the
 * stream panel uses it. [description] is long-form help hidden behind an info button — settings
 * uses that. They are independent; a row may have both.
 */
@Composable
internal fun ControlSwitchRow(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    value: String? = null,
    description: String? = null,
    enabled: Boolean = true,
    indentLevel: Int = 0,
    style: ControlRowStyle = controlRowStyle(),
) {
    var descriptionExpanded by remember(label) { mutableStateOf(false) }
    val toggle = { if (enabled) onCheckedChange(!checked) }
    ControlRow(
        modifier = modifier,
        onClick = toggle,
        onLongClick = if (description.isNullOrBlank()) null else ({ descriptionExpanded = !descriptionExpanded }),
        enabled = enabled,
        indentLevel = indentLevel,
        style = style,
    ) {
        ControlRowLabels(
            label = label,
            value = value,
            expandedDescription = description?.takeIf { descriptionExpanded },
            enabled = enabled,
            style = style,
        )
        if (!description.isNullOrBlank()) {
            IconButton(
                onClick = { descriptionExpanded = !descriptionExpanded },
                modifier = Modifier.width(40.dp),
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
        Switch(checked = checked, enabled = enabled, onCheckedChange = onCheckedChange)
    }
}

/** A row that opens a sub-page, showing the current selection and a chevron. */
@Composable
internal fun ControlNavigationRow(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    value: String? = null,
    enabled: Boolean = true,
    indentLevel: Int = 0,
    style: ControlRowStyle = controlRowStyle(),
    leadingIcon: ImageVector? = null,
) {
    ControlRow(
        modifier = modifier,
        onClick = onClick,
        enabled = enabled,
        indentLevel = indentLevel,
        style = style,
    ) {
        if (leadingIcon != null) {
            Icon(leadingIcon, contentDescription = null,
                tint = style.labelColor.copy(alpha = if (enabled) 1f else DISABLED_ALPHA),
                modifier = Modifier.size(24.dp))
            Spacer(Modifier.width(12.dp))
        }
        ControlRowLabels(label, value, expandedDescription = null, enabled = enabled, style = style)
        Icon(
            painter = painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = style.supportingColor,
            modifier = Modifier.width(22.dp),
        )
    }
}

/** A row whose trailing slot is a verb — "Open", "Reset" — rather than a control. */
@Composable
internal fun ControlActionRow(
    label: String,
    actionLabel: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    value: String? = null,
    enabled: Boolean = true,
    indentLevel: Int = 0,
    style: ControlRowStyle = controlRowStyle(),
) {
    ControlRow(
        modifier = modifier,
        onClick = onClick,
        enabled = enabled,
        indentLevel = indentLevel,
        style = style,
    ) {
        ControlRowLabels(label, value, expandedDescription = null, enabled = enabled, style = style)
        Text(
            actionLabel,
            color = MaterialTheme.colorScheme.primary,
            style = MaterialTheme.typography.labelMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * A labelled slider with a formatted readout.
 *
 * [onChange] commits — it fires on release, and on each D-pad step. [onChangePreview] fires on
 * every drag frame and exists for the touch-layout sliders, where watching the overlay move while
 * dragging *is* the feature. Leave it null and the value is only written once, on release.
 */
@Composable
internal fun ControlSliderRow(
    label: String,
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    onChange: (Float) -> Unit,
    modifier: Modifier = Modifier,
    unit: String? = null,
    valueFormatter: ((Float) -> String)? = null,
    description: String? = null,
    descriptionProvider: ((Float) -> String?)? = null,
    onChangePreview: ((Float) -> Unit)? = null,
    style: ControlRowStyle = controlRowStyle(),
) {
    var local by remember(value) { mutableFloatStateOf(value) }
    var descriptionExpanded by remember(label) { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    var focused by remember { mutableStateOf(false) }
    val space = LocalSpaceUiPresentation.current
    val motion = if (space) rememberSpaceInteraction(focused) else null
    val hoverInteraction = motion?.source ?: remember { MutableInteractionSource() }
    val hovered by hoverInteraction.collectIsHoveredAsState()
    val showFocus = style.showFocusRing && focused
    val bonanzaActive = LocalAbsoluteCinemaEverywhere.current
    val quantize = { raw: Float -> ((raw / step).roundToInt() * step).coerceIn(min, max) }
    Box(modifier.fillMaxWidth()) {
        Column(
            Modifier
                .fillMaxWidth()
                .then(motion?.modifier ?: Modifier)
                .hoverable(hoverInteraction)
                .controlRowContainer(style = style, showFocus = showFocus, emphasis = motion?.emphasis),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    label,
                    Modifier.weight(1f),
                    color = MaterialTheme.colorScheme.onSurface,
                    style = style.labelStyle,
                    fontWeight = style.labelWeight,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    formatSliderValue(local, min, max, step, unit, valueFormatter),
                    color = style.supportingColor,
                    // Tabular figures so the readout does not reflow while the thumb is dragged.
                    style = MaterialTheme.typography.labelLarge.numeric(),
                )
                if (!description.isNullOrBlank()) {
                    IconButton(
                        onClick = { descriptionExpanded = !descriptionExpanded },
                        modifier = Modifier.width(40.dp),
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
            }
            if (descriptionExpanded && !description.isNullOrBlank()) {
                Text(
                    description,
                    color = style.supportingColor.copy(alpha = 0.86f),
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 4,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Slider(
                modifier = Modifier
                    .onFocusChanged { focused = style.focusable && it.isFocused }
                    .onPreviewKeyEvent {
                        handleSliderDpadInput(it, local, min, max, step, focusManager) { next ->
                            local = quantize(next)
                            onChange(local)
                        }
                    },
                interactionSource = hoverInteraction,
                value = local,
                onValueChange = {
                    local = quantize(it)
                    onChangePreview?.invoke(local)
                },
                onValueChangeFinished = { onChange(local) },
                valueRange = min..max,
            )
            descriptionProvider?.invoke(local)?.let { description ->
                Spacer(Modifier.height(2.dp))
                Text(
                    description,
                    color = style.supportingColor.copy(alpha = 0.8f),
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        InteractionFocusFrame(
            visible = !space && (focused || (hovered && bonanzaActive)),
            cornerRadius = 14.dp,
            cinemaEffectEnabled = bonanzaActive,
        )
    }
}
