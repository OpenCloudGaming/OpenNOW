package com.opencloudgaming.opennow.ui.controls

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import com.opencloudgaming.opennow.LocalSpaceUiPresentation
import com.opencloudgaming.opennow.rememberSpaceInteraction
import com.opencloudgaming.opennow.spaceAnimatedSurface
import com.opencloudgaming.opennow.focusMoveHaptics
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.LocalSettingsControllerNavigationEnabled
import com.opencloudgaming.opennow.InteractionFocusFrame
import com.opencloudgaming.opennow.LocalAbsoluteCinemaEverywhere
import com.opencloudgaming.opennow.handleVerticalDpadFocusMove
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing

/**
 * How a settings-style row is painted.
 *
 * The settings screen and the in-stream controls panel used to own two entirely separate sets of
 * row widgets — five verbatim copies of the same clip/focus/background/border/clickable chain. The
 * difference between them was always purely presentational plus one focus policy, so it lives here
 * and is pushed down through [LocalControlRowStyle]; no call site passes it.
 *
 * Content differences are *not* modelled here. A row's always-visible `value` subtitle and its
 * collapsible `description` are independent optional parameters, so a row can carry neither,
 * either, or both regardless of which surface it is on.
 */
@Immutable
data class ControlRowStyle(
    val shape: Shape,
    val containerRest: Color,
    val containerFocused: Color,
    val borderRestWidth: Dp,
    val borderFocusWidth: Dp,
    val horizontalPadding: Dp,
    val verticalPadding: Dp,
    val contentGap: Dp,
    val labelColor: Color,
    val labelStyle: TextStyle,
    val labelWeight: FontWeight?,
    val supportingStyle: TextStyle,
    val supportingColor: Color,
    val indentStep: Dp,
    /** Whether rows take D-pad focus at all. */
    val focusable: Boolean,
    /** Whether the focus ring is drawn. Same value as [focusable] today, kept separate so it can diverge. */
    val showFocusRing: Boolean,
) {
    companion object {
        /** Settings screen: opaque card-like rows, focus signalled by the ring alone. */
        @Composable
        fun settings(): ControlRowStyle {
            val controllerNavigation = LocalSettingsControllerNavigationEnabled.current
            val container = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f)
            return ControlRowStyle(
                shape = RoundedCornerShape(SETTINGS_ROW_RADIUS),
                containerRest = container,
                containerFocused = container,
                borderRestWidth = 1.dp,
                borderFocusWidth = 2.dp,
                horizontalPadding = OpenNowSpacing.md,
                verticalPadding = OpenNowSpacing.sm,
                contentGap = 3.dp,
                labelColor = MaterialTheme.colorScheme.onSurface,
                labelStyle = MaterialTheme.typography.bodyLarge,
                labelWeight = null,
                supportingStyle = MaterialTheme.typography.bodySmall,
                supportingColor = MaterialTheme.colorScheme.onSurfaceVariant,
                indentStep = OpenNowSpacing.xl,
                focusable = controllerNavigation,
                showFocusRing = controllerNavigation,
            )
        }

        /**
         * In-stream controls panel: denser, and focusable unconditionally.
         *
         * Shares settings-row focus navigation and Compose activation/long-press handling.
         * Nested controls keep ownership of their own activation events.
         */
        @Composable
        fun stream(): ControlRowStyle = ControlRowStyle(
            shape = RoundedCornerShape(OpenNowRadius.md),
            containerRest = OpenNowPalette.PanelRowRest,
            containerFocused = OpenNowPalette.PanelRowFocused,
            borderRestWidth = 1.dp,
            borderFocusWidth = 2.dp,
            horizontalPadding = OpenNowSpacing.md,
            verticalPadding = 10.dp,
            contentGap = 2.dp,
            labelColor = OpenNowPalette.TextPrimary,
            labelStyle = MaterialTheme.typography.titleSmall,
            labelWeight = FontWeight.SemiBold,
            supportingStyle = MaterialTheme.typography.labelSmall,
            supportingColor = OpenNowPalette.TextMuted,
            indentStep = OpenNowSpacing.xl,
            focusable = true,
            showFocusRing = true,
        )
    }
}

/** Settings rows have always been 14dp — between [OpenNowRadius.md] and [OpenNowRadius.lg]. */
private val SETTINGS_ROW_RADIUS = 14.dp

internal val LocalControlRowStyle = compositionLocalOf<ControlRowStyle?> { null }

@Composable
internal fun controlRowStyle(): ControlRowStyle =
    LocalControlRowStyle.current ?: ControlRowStyle.settings()

/**
 * The container every control row is built on.
 *
 * The modifier order matters and is copied from the original settings row: `border` comes *before*
 * `clip`, which is why the focus ring sits outside the fill rather than being clipped by it.
 */
@Composable
internal fun ControlRow(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    onLongClick: (() -> Unit)? = null,
    enabled: Boolean = true,
    indentLevel: Int = 0,
    style: ControlRowStyle = controlRowStyle(),
    verticalAlignment: Alignment.Vertical = Alignment.CenterVertically,
    content: @Composable RowScope.() -> Unit,
) {
    val focusManager = LocalFocusManager.current
    var focused by remember { mutableStateOf(false) }
    val space = LocalSpaceUiPresentation.current
    val motion = if (space) rememberSpaceInteraction(focused, enabled = enabled) else null
    val hoverInteraction = motion?.source ?: remember { MutableInteractionSource() }
    val hovered by hoverInteraction.collectIsHoveredAsState()
    val showFocus = style.showFocusRing && focused
    val bonanzaActive = LocalAbsoluteCinemaEverywhere.current
    Box(
        modifier
            .fillMaxWidth()
            .padding(start = style.indentStep * indentLevel),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .then(motion?.modifier ?: Modifier)
                .hoverable(hoverInteraction, enabled = enabled)
                .onFocusChanged { focused = style.focusable && (it.isFocused || it.hasFocus) }
                .focusMoveHaptics()
                .controlRowSurface(style, showFocus, motion?.emphasis)
                .then(
                    if (onClick != null) Modifier.combinedClickable(
                        interactionSource = hoverInteraction,
                        indication = if (space) null else androidx.compose.foundation.LocalIndication.current,
                        enabled = enabled,
                        onClick = onClick,
                        onLongClick = onLongClick,
                    )
                    else Modifier,
                )
                .onPreviewKeyEvent { event ->
                    when {
                        style.focusable -> handleVerticalDpadFocusMove(event, focusManager)
                        else -> false
                    }
                }
                .then(if (onClick == null) Modifier.focusable(enabled = style.focusable) else Modifier)
                .padding(horizontal = style.horizontalPadding, vertical = style.verticalPadding),
            verticalAlignment = verticalAlignment,
            content = content,
        )
        InteractionFocusFrame(
            visible = !space && (focused || (hovered && bonanzaActive)),
            cornerRadius = SETTINGS_ROW_RADIUS,
            cinemaEffectEnabled = bonanzaActive,
        )
    }
}

/**
 * The border/clip/background/padding chain on its own, for controls that need the row's look but
 * not its `Row` layout or click behaviour — the slider, which is a `Column` and whose focus lives
 * on the `Slider` itself.
 */
@Composable
internal fun Modifier.controlRowSurface(style: ControlRowStyle, showFocus: Boolean,
    emphasis: State<Float>? = null): Modifier = if (emphasis != null) {
    clip(style.shape).spaceAnimatedSurface(emphasis, style.shape, style.containerRest, style.containerFocused,
        Color.Transparent, MaterialTheme.colorScheme.primary.copy(alpha = .65f))
} else {
    border(if (showFocus) style.borderFocusWidth else style.borderRestWidth, Color.Transparent, style.shape)
        .clip(style.shape).background(if (showFocus) style.containerFocused else style.containerRest)
}

@Composable
internal fun Modifier.controlRowContainer(style: ControlRowStyle, showFocus: Boolean,
    emphasis: State<Float>? = null): Modifier = controlRowSurface(style, showFocus, emphasis)
    .padding(horizontal = style.horizontalPadding, vertical = style.verticalPadding)

/**
 * The leading label block shared by every row: a label, an optional always-visible value line, and
 * an optional expanded description.
 */
@Composable
internal fun RowScope.ControlRowLabels(
    label: String,
    value: String?,
    expandedDescription: String?,
    enabled: Boolean,
    style: ControlRowStyle,
) {
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(style.contentGap)) {
        Text(
            label,
            color = style.labelColor.copy(alpha = if (enabled) 1f else DISABLED_ALPHA),
            style = style.labelStyle,
            fontWeight = style.labelWeight,
            maxLines = 2,
            overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
        )
        if (!value.isNullOrBlank()) {
            Text(
                value,
                color = style.supportingColor.copy(alpha = if (enabled) 1f else DISABLED_ALPHA),
                style = style.supportingStyle,
                maxLines = 2,
                overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
            )
        }
        if (!expandedDescription.isNullOrBlank()) {
            Text(
                expandedDescription,
                color = style.supportingColor.copy(alpha = if (enabled) 0.86f else DISABLED_ALPHA),
                style = MaterialTheme.typography.bodySmall,
                maxLines = 3,
                overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
            )
        }
    }
}

internal const val DISABLED_ALPHA = 0.45f

/**
 * Section wrapper. Settings groups rows inside a card; the stream panel, floating over video, uses
 * a bare caption so it does not stack a second surface on top of the panel itself.
 */
@Immutable
data class ControlSectionStyle(
    val usesCard: Boolean,
    val titleStyle: TextStyle,
    val titleWeight: FontWeight,
    val titleColor: Color,
    val itemSpacing: Dp,
) {
    companion object {
        @Composable
        fun settings(): ControlSectionStyle = ControlSectionStyle(
            usesCard = true,
            titleStyle = MaterialTheme.typography.titleMedium,
            titleWeight = FontWeight.Bold,
            titleColor = MaterialTheme.colorScheme.onSurface,
            itemSpacing = 10.dp,
        )

        @Composable
        fun stream(): ControlSectionStyle = ControlSectionStyle(
            usesCard = false,
            titleStyle = MaterialTheme.typography.labelMedium,
            titleWeight = FontWeight.Bold,
            titleColor = OpenNowPalette.TextMuted,
            itemSpacing = OpenNowSpacing.sm,
        )
    }
}

internal val LocalControlSectionStyle = compositionLocalOf<ControlSectionStyle?> { null }

@Composable
internal fun controlSectionStyle(): ControlSectionStyle =
    LocalControlSectionStyle.current ?: ControlSectionStyle.settings()

@Composable
internal fun ControlSection(
    title: String,
    modifier: Modifier = Modifier,
    style: ControlSectionStyle = controlSectionStyle(),
    content: @Composable ColumnScope.() -> Unit,
) {
    val body: @Composable ColumnScope.() -> Unit = {
        if (title.isNotBlank()) Text(title, color = style.titleColor, style = style.titleStyle, fontWeight = style.titleWeight)
        content()
    }
    if (style.usesCard) {
        Card(
            modifier = modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            shape = RoundedCornerShape(SETTINGS_ROW_RADIUS),
        ) {
            Column(
                Modifier.fillMaxWidth().padding(14.dp),
                verticalArrangement = Arrangement.spacedBy(style.itemSpacing),
                content = body,
            )
        }
    } else {
        Column(
            modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(style.itemSpacing),
            content = body,
        )
    }
}
