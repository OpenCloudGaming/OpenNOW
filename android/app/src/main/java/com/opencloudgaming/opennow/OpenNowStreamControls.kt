package com.opencloudgaming.opennow

import android.content.res.Configuration
import android.provider.Settings
import androidx.annotation.StringRes
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.delay
import kotlin.math.min
import com.opencloudgaming.opennow.ui.controls.ControlActionRow
import com.opencloudgaming.opennow.ui.controls.ControlNavigationRow
import com.opencloudgaming.opennow.ui.controls.ControlRowStyle
import com.opencloudgaming.opennow.ui.controls.ControlSection
import com.opencloudgaming.opennow.ui.controls.ControlSectionStyle
import com.opencloudgaming.opennow.ui.controls.ControlSliderRow
import com.opencloudgaming.opennow.ui.controls.ControlSwitchRow
import com.opencloudgaming.opennow.ui.controls.LocalControlRowStyle
import com.opencloudgaming.opennow.ui.controls.LocalControlSectionStyle
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.tint
import kotlin.math.roundToInt

@Composable
internal fun ActiveSessionDecisionScreen(
    state: OpenNowUiState,
    onResumeSession: () -> Unit,
    onReplaceSession: () -> Unit,
    onCancel: () -> Unit,
) {
    val decision = state.activeSessionDecision ?: return
    val active = decision.activeSession
    val activeGame = activeSessionGame(state, active)
    Column(
        Modifier
            .fillMaxSize()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Surface(
            modifier = Modifier.fillMaxWidth().widthIn(max = 560.dp),
            shape = RoundedCornerShape(18.dp),
            color = PanelAlt.copy(alpha = 0.96f),
            contentColor = TextPrimary,
            tonalElevation = 4.dp,
        ) {
            Column(
                Modifier.padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    UrlImage(
                        activeGame?.imageUrl ?: state.streamGame?.imageUrl,
                        Modifier
                            .width(56.dp)
                            .height(74.dp)
                            .clip(RoundedCornerShape(10.dp)),
                    )
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(stringResource(R.string.stream_session_active_title), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                        Text(
                            activeGame?.title ?: "App ${active.appId}",
                            color = TextPrimary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            activeSessionSummary(active),
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                Text(
                    "Resume the existing session, or terminate it and start ${decision.requestedGameTitle}.",
                    color = TextMuted,
                    style = MaterialTheme.typography.bodyMedium,
                )
                FlowRow(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    TextButton(onClick = onCancel) { Text(stringResource(R.string.action_cancel)) }
                    OutlinedButton(onClick = onReplaceSession) { Text(stringResource(R.string.stream_session_terminate_start)) }
                    Button(onClick = onResumeSession) { Text(stringResource(R.string.action_resume)) }
                }
            }
        }
    }
}

@Composable
internal fun NoActiveStreamScreen(
    canResumeSession: Boolean,
    canEndSession: Boolean,
    onBack: () -> Unit,
    onResumeSession: () -> Unit,
    onEndSession: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(stringResource(R.string.stream_no_active), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(8.dp))
        Text(
            stringResource(R.string.stream_no_local_stream),
            color = TextMuted,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(18.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            OutlinedButton(onClick = onBack) { Text(stringResource(R.string.stream_back_to_library)) }
            if (canResumeSession) {
                Button(onClick = onResumeSession) { Text(stringResource(R.string.action_resume)) }
            }
            if (canEndSession) {
                Button(onClick = onEndSession) { Text(stringResource(R.string.stream_end_cloud_session)) }
            }
        }
    }
}

@Composable
private fun StreamControlLauncher(
    controlsOpen: Boolean,
    status: String?,
    onToggle: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier
            .padding(top = 10.dp, end = 10.dp)
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setUiTouchPassthroughBounds(
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            },
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (status != null) {
            Surface(
                shape = RoundedCornerShape(999.dp),
                color = Panel.copy(alpha = 0.8f),
                tonalElevation = 3.dp,
            ) {
                Text(
                    status,
                    color = TextMuted,
                    style = MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                )
            }
        }
        Button(onClick = onToggle, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
            Text(if (controlsOpen) stringResource(R.string.action_close) else stringResource(R.string.stream_panel_title))
        }
        OutlinedButton(onClick = onExit, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
            Text(stringResource(R.string.stream_panel_exit))
        }
    }

    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.clearUiTouchPassthroughBounds()
        }
    }
}

@Composable
internal fun StreamFirstLaunchGuide(
    step: StreamGuideStep,
    controlsOpen: Boolean,
    touchControlsEnabled: Boolean,
    onOpenControls: () -> Unit,
    onSkip: () -> Unit,
) {
    val primaryFocusRequester = remember { FocusRequester() }
    val overlayInteraction = remember { MutableInteractionSource() }
    LaunchedEffect(step, controlsOpen) {
        delay(80)
        if (step == StreamGuideStep.OpenControls || !controlsOpen) {
            runCatching { primaryFocusRequester.requestFocus() }
        }
    }
    BoxWithConstraints(
        if (step == StreamGuideStep.OpenControls) {
            Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.62f))
                .clickable(
                    interactionSource = overlayInteraction,
                    indication = null,
                    onClick = {},
                )
        } else {
            Modifier.fillMaxSize()
        },
    ) {
        val landscape = maxWidth > maxHeight
        if (step == StreamGuideStep.OpenControls) {
            StreamGuideEdgeCue(Modifier.align(Alignment.CenterStart))
            StreamGuideCard(
                stepLabel = "Step 1 of 2",
                title = "Open the stream menu",
                body = "Press Android Back, Menu, or swipe from the left edge. That opens the menu without exiting the stream.",
                details = listOf(
                    "Back or the left-edge gesture opens controls.",
                    if (touchControlsEnabled) {
                        "Touch controls pause while this guide is up."
                    } else {
                        "You can turn touch controls on from the menu."
                    },
                    "Use Skip tutorial if you already know this flow.",
                ),
                modifier = Modifier
                    .align(Alignment.Center)
                    .padding(18.dp)
                    .fillMaxWidth(if (landscape) 0.54f else 0.92f)
                    .then(if (landscape) Modifier.fillMaxHeight(0.82f) else Modifier),
                primaryLabel = "Open controls",
                primaryFocusRequester = primaryFocusRequester,
                onPrimary = onOpenControls,
                secondaryLabel = "Skip tutorial",
                onSecondary = onSkip,
            )
        } else {
            StreamGuideDoneCallout(
                controlsOpen = controlsOpen,
                onOpenControls = onOpenControls,
                onSkip = onSkip,
                primaryFocusRequester = primaryFocusRequester,
                modifier = Modifier
                    .align(if (landscape) Alignment.TopStart else Alignment.TopCenter)
                    .padding(18.dp)
                    .then(if (landscape) Modifier.fillMaxWidth(0.34f) else Modifier.fillMaxWidth(0.86f))
                    .widthIn(max = 340.dp),
            )
        }
    }
}

@Composable
private fun StreamGuideCard(
    stepLabel: String,
    title: String,
    body: String,
    details: List<String>,
    primaryLabel: String,
    primaryFocusRequester: FocusRequester,
    onPrimary: () -> Unit,
    secondaryLabel: String,
    onSecondary: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(20.dp),
        color = Panel.copy(alpha = 0.96f),
        contentColor = TextPrimary,
        tonalElevation = 8.dp,
    ) {
        Column(
            Modifier
                .padding(18.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(stepLabel, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                Text(title, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text(body, color = TextMuted, style = MaterialTheme.typography.bodyMedium)
            }
            details.forEachIndexed { index, detail ->
                StreamGuidePoint(number = index + 1, body = detail)
            }
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                OutlinedButton(
                    onClick = onSecondary,
                    modifier = Modifier.weight(1f),
                ) {
                    Text(secondaryLabel, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Button(
                    onClick = onPrimary,
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(primaryFocusRequester),
                ) {
                    Text(primaryLabel, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

@Composable
private fun StreamGuideDoneCallout(
    controlsOpen: Boolean,
    onOpenControls: () -> Unit,
    onSkip: () -> Unit,
    primaryFocusRequester: FocusRequester,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(18.dp),
        color = Panel.copy(alpha = 0.9f),
        tonalElevation = 6.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text(stringResource(R.string.stream_guide_step, 2, 2), color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)
                Text(stringResource(R.string.stream_guide_press_done), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            TextButton(
                onClick = onSkip,
                contentPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp),
            ) {
                Text(stringResource(R.string.action_skip), maxLines = 1)
            }
            if (!controlsOpen) {
                Button(
                    onClick = onOpenControls,
                    modifier = Modifier.focusRequester(primaryFocusRequester),
                    contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                ) {
                    Text(stringResource(R.string.action_open), maxLines = 1)
                }
            }
        }
    }
}

@Composable
internal fun PhysicalControllerTouchControlsDialog(
    doNotShowAgain: Boolean,
    onDoNotShowAgainChange: (Boolean) -> Unit,
    onOk: () -> Unit,
    onUndo: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onOk,
        title = { Text(stringResource(R.string.stream_controller_detected)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    stringResource(R.string.stream_controller_hidden),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(12.dp))
                        .clickable { onDoNotShowAgainChange(!doNotShowAgain) }
                        .padding(horizontal = 8.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Checkbox(
                        checked = doNotShowAgain,
                        onCheckedChange = onDoNotShowAgainChange,
                    )
                    Text(stringResource(R.string.common_dont_show_again), color = MaterialTheme.colorScheme.onSurface)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onOk) {
                Text(stringResource(R.string.action_ok))
            }
        },
        dismissButton = {
            TextButton(onClick = onUndo) {
                Text(stringResource(R.string.action_undo))
            }
        },
    )
}

@Composable
internal fun StreamInputModeSwitchDialog(
    prompt: StreamInputModePrompt,
    onStay: () -> Unit,
    onSwitch: () -> Unit,
) {
    val title = when (prompt) {
        StreamInputModePrompt.SwitchToKeyboardMouse -> R.string.stream_keyboard_mouse_detected
        StreamInputModePrompt.SwitchToNativeTouch -> R.string.stream_keyboard_mouse_disconnected
    }
    val body = when (prompt) {
        StreamInputModePrompt.SwitchToKeyboardMouse -> R.string.stream_keyboard_mouse_detected_body
        StreamInputModePrompt.SwitchToNativeTouch -> R.string.stream_keyboard_mouse_disconnected_body
    }
    val switchLabel = when (prompt) {
        StreamInputModePrompt.SwitchToKeyboardMouse -> R.string.stream_input_switch_keyboard_mouse
        StreamInputModePrompt.SwitchToNativeTouch -> R.string.stream_input_switch_native_touch
    }
    val stayLabel = when (prompt) {
        StreamInputModePrompt.SwitchToKeyboardMouse -> R.string.stream_input_stay_native_touch
        StreamInputModePrompt.SwitchToNativeTouch -> R.string.stream_input_keep_keyboard_mouse
    }
    AlertDialog(
        onDismissRequest = onStay,
        title = { Text(stringResource(title)) },
        text = {
            Text(
                stringResource(body),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        confirmButton = {
            TextButton(onClick = onSwitch) {
                Text(stringResource(switchLabel))
            }
        },
        dismissButton = {
            TextButton(onClick = onStay) {
                Text(stringResource(stayLabel))
            }
        },
    )
}

@Composable
private fun StreamGuideEdgeCue(modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxHeight()
            .width(112.dp)
            .background(
                Brush.horizontalGradient(
                    listOf(
                        MaterialTheme.colorScheme.primary.copy(alpha = 0.28f),
                        Color.Transparent,
                    ),
                ),
            ),
    ) {
        Surface(
            modifier = Modifier
                .align(Alignment.CenterStart)
                .padding(start = 14.dp),
            shape = RoundedCornerShape(999.dp),
            color = MaterialTheme.colorScheme.primary.copy(alpha = 0.92f),
            tonalElevation = 6.dp,
        ) {
            Row(
                Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_arrow_back),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onPrimary,
                    modifier = Modifier.size(18.dp),
                )
                Text(stringResource(R.string.action_back), color = MaterialTheme.colorScheme.onPrimary, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
            }
        }
    }
}

@Composable
private fun StreamGuidePoint(number: Int, body: String) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Surface(
            modifier = Modifier.size(22.dp),
            shape = CircleShape,
            color = MaterialTheme.colorScheme.primary,
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(number.toString(), color = MaterialTheme.colorScheme.onPrimary, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(body, color = TextMuted, style = MaterialTheme.typography.bodySmall)
        }
    }
}

private enum class StreamControlsPage {
    Main,
    StatusBar,
    TouchControls,
    MouseMode,
    ReportProblem,
}

@Composable
private fun ControlBitrateLiveHint(
    liveBitrateMbps: Int,
    liveOverridden: Boolean,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = 14.dp, end = 14.dp, top = 2.dp, bottom = 6.dp),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Box(
                Modifier
                    .clip(RoundedCornerShape(999.dp))
                    .background(Green.copy(alpha = if (liveOverridden) 0.18f else 0.10f))
                    .padding(horizontal = 7.dp, vertical = 2.dp),
            ) {
                Text(
                    stringResource(R.string.stream_panel_bitrate_live_badge),
                    color = if (liveOverridden) Green else TextMuted,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                )
            }
            Text(
                stringResource(R.string.stream_panel_bitrate_live_summary, liveBitrateMbps),
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
            )
        }
        Text(
            stringResource(R.string.stream_panel_bitrate_next_session_hint),
            color = TextMuted.copy(alpha = 0.72f),
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

@Composable
internal fun StreamControlsPanel(
    gameTitle: String,
    status: String?,
    settings: AppSettings,
    tvProfile: Boolean,
    touchControlsVisible: Boolean,
    builtInGameTouchSupported: Boolean,
    nativeTouchActive: Boolean,
    gyroscopeAvailable: Boolean,
    controllerMouseAssistEnabled: Boolean,
    controllerMouseEmulationEnabled: Boolean,
    showSessionTimer: Boolean,
    sessionTimerLimit: SmartSessionLimit,
    sessionStartedAtMs: Long,
    sessionNowMs: Long,
    audioMuted: Boolean,
    microphoneRequested: Boolean,
    microphonePermissionGranted: Boolean,
    microphoneEnabled: Boolean,
    statsVisible: Boolean,
    liveBitrateLimitKbps: Int?,
    touchLayoutEditing: Boolean,
    bugReportSubmission: BugReportSubmissionState,
    bugReportVersionCheck: AndroidBugReportVersionCheckState,
    update: AndroidUpdateState,
    bugReportPreflightProvider: () -> BugReportPreflightDeck,
    onAudioToggle: () -> Unit,
    onMicrophoneToggle: () -> Unit,
    onStatsToggle: () -> Unit,
    onStatsStyleCycle: () -> Unit,
    onStatsPositionCycle: () -> Unit,
    onStatsMetricsChange: (StreamStatsMetrics) -> Unit,
    onKeyboardButtonToggle: () -> Unit,
    onVibrationToggle: () -> Unit,
    onTouchLayoutEditingToggle: () -> Unit,
    onKeyboardOpen: () -> Unit,
    onEsc: () -> Unit,
    onEnter: () -> Unit,
    onBackspace: () -> Unit,
    onSteamMenuOpen: () -> Unit,
    onControllerMouseAssistToggle: () -> Unit,
    onControllerMouseEmulationToggle: () -> Unit,
    onExit: () -> Unit,
    onTouchControlsToggle: () -> Unit,
    onMousePadToggle: () -> Unit,
    onMouseDirectClickToggle: () -> Unit,
    onToggleTouchControllerStyle: () -> Unit,
    onTouchButtonLabelsToggle: () -> Unit,
    onJoystickModeToggle: () -> Unit,
    onTouchAimModeToggle: () -> Unit,
    onJoystickDeadZoneChange: (Float) -> Unit,
    onSharpeningToggle: () -> Unit,
    onSharpeningAmountChange: (Float) -> Unit,
    onMaxBitrateChange: (Int) -> Unit,
    onStretchToFitToggle: () -> Unit,
    onTouchScaleChange: (Float) -> Unit,
    onButtonScaleChange: (Float) -> Unit,
    onStickScaleChange: (Float) -> Unit,
    onOpacityChange: (Float) -> Unit,
    onMouseSensitivityChange: (Float) -> Unit,
    onMouseScrollSensitivityChange: (Int) -> Unit,
    onTouchEdgePaddingChange: (Float) -> Unit,
    onTouchBottomPaddingChange: (Float) -> Unit,
    onTouchLeftOffsetChange: (Float) -> Unit,
    onTouchRightOffsetChange: (Float) -> Unit,
    onTouchLayoutReset: () -> Unit,
    onTouchSettingsChange: (AndroidTouchSettings) -> Unit,
    onBugReportSubmit: (String, String, String?) -> Unit,
    onBugReportReset: () -> Unit,
    onBugReportVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    onButtonTone: () -> Unit,
    highlightDone: Boolean = false,
    onClose: () -> Unit,
) {
    val doneFocusRequester = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    var page by remember { mutableStateOf(StreamControlsPage.Main) }
    val reduceMotion = LocalReduceMotion.current
    BackHandler(enabled = page != StreamControlsPage.Main) {
        page = StreamControlsPage.Main
    }
    LaunchedEffect(page) {
        delay(120)
        runCatching { doneFocusRequester.requestFocus() }
    }
    Surface(
        modifier = Modifier
            .padding(14.dp)
            .fillMaxWidth(0.94f)
            .fillMaxHeight(0.72f)
            .streamTouchPassthrough(PASSTHROUGH_ID_PANEL),
        shape = RoundedCornerShape(OpenNowRadius.lg + 2.dp),
        // Firmer than the old 0.93: at that alpha TextMuted did not reliably clear 4.5:1 over
        // bright gameplay. The hairline keeps the panel's edge visible against a light frame.
        color = OpenNowPalette.PanelOverVideo,
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
        tonalElevation = 6.dp,
    ) {
        // Every control row inside the panel picks up the denser, over-video styling — and, more
        // importantly, becomes properly focusable. The panel's own row widgets never were.
        CompositionLocalProvider(
            LocalControlRowStyle provides ControlRowStyle.stream(),
            LocalControlSectionStyle provides ControlSectionStyle.stream(),
        ) {
        Column(Modifier.fillMaxSize()) {
        // The header stays outside the scrolling area so every focused sub-page keeps navigation
        // and session actions visible while its settings scroll independently.
        StreamPanelHeader(
            page = page,
            gameTitle = gameTitle,
            status = status,
            highlightDone = highlightDone,
            focusRequester = doneFocusRequester,
            onBack = { page = StreamControlsPage.Main },
            onKeyboardOpen = onKeyboardOpen,
            onExit = onExit,
            onClose = onClose,
            onButtonTone = onButtonTone,
        )
        AnimatedContent(
            targetState = page,
            transitionSpec = { streamPanelPageTransition(initialState, targetState, reduceMotion) },
            label = "stream-controls-page",
        ) { currentPage ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
            contentPadding = PaddingValues(OpenNowSpacing.md + 2.dp),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
        ) {
            when (currentPage) {
                StreamControlsPage.StatusBar -> statusBarPageItems(
                    settings = settings,
                    statsVisible = statsVisible,
                    onStatsToggle = onStatsToggle,
                    onStatsStyleCycle = onStatsStyleCycle,
                    onStatsPositionCycle = onStatsPositionCycle,
                    onStatsMetricsChange = onStatsMetricsChange,
                    onKeyboardButtonToggle = onKeyboardButtonToggle,
                    onButtonTone = onButtonTone,
                )
                StreamControlsPage.TouchControls -> {
                    if (builtInGameTouchSupported) {
                        item {
                            BuiltInGameTouchNotice(usingBuiltInTouch = nativeTouchActive)
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.stream_panel_section_touch_controller)) {
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_touch_controller),
                                checked = touchControlsVisible,
                                onCheckedChange = {
                                    onButtonTone()
                                    onTouchControlsToggle()
                                },
                                value = when {
                                    touchControlsVisible -> stringResource(R.string.common_visible)
                                    nativeTouchActive -> stringResource(R.string.stream_touch_builtin_active)
                                    else -> stringResource(R.string.common_hidden)
                                },
                            )
                            if (touchControlsVisible) {
                                // Cycles rather than opens a menu: this row is used mid-session,
                                // often one-handed, and each press shows its result immediately
                                // behind the panel.
                                ControlActionRow(
                                    label = stringResource(R.string.stream_panel_touch_skin),
                                    actionLabel = stringResource(R.string.common_next),
                                    value = touchControllerStyleLabel(settings.androidTouch.touchControllerStyle),
                                    onClick = {
                                        onButtonTone()
                                        onToggleTouchControllerStyle()
                                    },
                                )
                                ControlActionRow(
                                    label = stringResource(R.string.settings_touch_skin_tint),
                                    actionLabel = stringResource(R.string.common_next),
                                    value = touchSkinTintLabel(settings.androidTouch.touchSkinTint),
                                    onClick = {
                                        onButtonTone()
                                        onTouchSettingsChange(
                                            settings.androidTouch.copy(
                                                touchSkinTint = nextTouchSkinTint(settings.androidTouch.touchSkinTint),
                                            ),
                                        )
                                    },
                                )
                                ControlSwitchRow(
                                    label = stringResource(R.string.settings_touch_button_labels),
                                    checked = settings.androidTouch.touchButtonLabels,
                                    onCheckedChange = {
                                        onButtonTone()
                                        onTouchButtonLabelsToggle()
                                    },
                                    value = onOffLabel(settings.androidTouch.touchButtonLabels),
                                )
                            }
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_vibration),
                                checked = settings.vibrationEnabled,
                                onCheckedChange = {
                                    onButtonTone()
                                    onVibrationToggle()
                                },
                                value = onOffLabel(settings.vibrationEnabled),
                                description = stringResource(R.string.stream_panel_vibration_summary),
                            )
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.stream_joysticks_title)) {
                            val dynamic = settings.androidTouch.joystickMode == TouchJoystickMode.Dynamic
                            val lockZone = settings.androidTouch.aimMode == TouchAimMode.LockZone
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_joysticks_aim_mode),
                                checked = lockZone,
                                onCheckedChange = {
                                    onButtonTone()
                                    onTouchAimModeToggle()
                                },
                                value = stringResource(
                                    if (lockZone) R.string.stream_joysticks_lock_zone else R.string.stream_joysticks_lock_joystick,
                                ),
                            )
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_joysticks_dynamic),
                                checked = dynamic,
                                onCheckedChange = {
                                    onButtonTone()
                                    onJoystickModeToggle()
                                },
                                value = stringResource(
                                    if (dynamic) R.string.stream_joysticks_dynamic_on else R.string.stream_joysticks_dynamic_off,
                                ),
                            )
                            if (lockZone) {
                                TouchLayoutSlider(
                                    R.string.stream_joysticks_aim_zone_scale,
                                    settings.androidTouch.aimZoneScale,
                                    0.5f,
                                    1.5f,
                                    TOUCH_SCALE_SLIDER_STEP,
                                    onChange = { value ->
                                        onTouchSettingsChange(settings.androidTouch.copy(aimZoneScale = value))
                                    },
                                )
                                TouchLayoutSlider(
                                    R.string.stream_joysticks_aim_zone_sensitivity,
                                    settings.androidTouch.aimZoneSensitivity,
                                    0.25f,
                                    3f,
                                    TOUCH_SCALE_SLIDER_STEP,
                                    onChange = { value ->
                                        onTouchSettingsChange(settings.androidTouch.copy(aimZoneSensitivity = value))
                                    },
                                )
                            }
                            TouchLayoutSlider(
                                R.string.stream_joysticks_stick_size,
                                settings.androidTouch.stickScale,
                                0.65f,
                                1.5f,
                                TOUCH_SCALE_SLIDER_STEP,
                                onStickScaleChange,
                            )
                            TouchLayoutSlider(
                                R.string.stream_joysticks_dead_zone,
                                settings.androidTouch.joystickDeadZone,
                                0f,
                                0.3f,
                                JOYSTICK_DEAD_ZONE_STEP,
                                onJoystickDeadZoneChange,
                            )
                            Text(
                                stringResource(
                                    if (lockZone) {
                                        R.string.stream_joysticks_lock_zone_summary
                                    } else {
                                        R.string.stream_joysticks_explainer
                                    },
                                ),
                                color = TextMuted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.settings_touch_visible_controls)) {
                            Text(
                                text = stringResource(R.string.settings_touch_visible_controls_desc),
                                color = TextMuted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                            TouchControlGroup.entries.forEach { group ->
                                val visible = settings.androidTouch.isControlVisible(group)
                                ControlSwitchRow(
                                    label = stringResource(touchControlGroupLabelRes(group)),
                                    checked = visible,
                                    onCheckedChange = { enabled ->
                                        onButtonTone()
                                        onTouchSettingsChange(
                                            settings.androidTouch.withControlVisible(group, enabled),
                                        )
                                    },
                                    value = onOffLabel(visible),
                                )
                            }
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.settings_touch_extra_buttons)) {
                            Text(
                                text = stringResource(R.string.settings_touch_extra_buttons_desc),
                                color = TextMuted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                            repeat(TOUCH_EXTRA_BUTTON_COUNT) { index ->
                                val action = settings.androidTouch.extraButtonAction(index)
                                ControlActionRow(
                                    label = stringResource(R.string.settings_touch_extra_button, index + 1),
                                    actionLabel = stringResource(R.string.common_next),
                                    value = touchExtraButtonActionLabel(action),
                                    onClick = {
                                        onButtonTone()
                                        onTouchSettingsChange(
                                            settings.androidTouch.withExtraButtonAction(
                                                index,
                                                nextTouchExtraButtonAction(action),
                                            ),
                                        )
                                    },
                                )
                            }
                            TouchLayoutSlider(
                                R.string.settings_touch_extra_button_size,
                                settings.androidTouch.extraButtonScale,
                                0.6f,
                                1.6f,
                                TOUCH_SCALE_SLIDER_STEP,
                                onChange = { value ->
                                    onTouchSettingsChange(settings.androidTouch.copy(extraButtonScale = value))
                                },
                            )
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.stream_panel_section_touch_layout)) {
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_drag_edit),
                                checked = touchLayoutEditing,
                                onCheckedChange = {
                                    onButtonTone()
                                    onTouchLayoutEditingToggle()
                                },
                                value = onOffLabel(touchLayoutEditing),
                            )
                            ControlActionRow(
                                label = stringResource(R.string.stream_panel_reset_layout),
                                actionLabel = stringResource(R.string.action_reset),
                                onClick = {
                                    onButtonTone()
                                    onTouchLayoutReset()
                                },
                                value = stringResource(R.string.stream_panel_reset_layout_summary),
                            )
                            // These controls preview live so the player can position the overlay
                            // against the game without leaving the stream.
                            TouchLayoutSlider(R.string.stream_panel_layout_scale, settings.androidTouch.scale, 0.6f, 1.4f, TOUCH_SCALE_SLIDER_STEP, onTouchScaleChange)
                            TouchLayoutSlider(R.string.stream_panel_button_size, settings.androidTouch.buttonScale, 0.65f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onButtonScaleChange)
                            TouchLayoutSlider(R.string.settings_touch_face_size, settings.androidTouch.faceButtonScale, 0.6f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onChange = { value ->
                                onTouchSettingsChange(settings.androidTouch.copy(faceButtonScale = value))
                            })
                            TouchLayoutSlider(R.string.settings_touch_dpad_size, settings.androidTouch.dpadScale, 0.6f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onChange = { value ->
                                onTouchSettingsChange(settings.androidTouch.copy(dpadScale = value))
                            })
                            TouchLayoutSlider(R.string.settings_touch_shoulders_size, settings.androidTouch.shoulderButtonScale, 0.6f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onChange = { value ->
                                onTouchSettingsChange(settings.androidTouch.copy(shoulderButtonScale = value))
                            })
                            TouchLayoutSlider(R.string.settings_touch_center_size, settings.androidTouch.centerButtonScale, 0.6f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onChange = { value ->
                                onTouchSettingsChange(settings.androidTouch.copy(centerButtonScale = value))
                            })
                            TouchLayoutSlider(R.string.settings_touch_left_stick_size, settings.androidTouch.leftStickScale, 0.6f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onChange = { value ->
                                onTouchSettingsChange(settings.androidTouch.copy(leftStickScale = value))
                            })
                            TouchLayoutSlider(R.string.settings_touch_right_stick_size, settings.androidTouch.rightStickScale, 0.6f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onChange = { value ->
                                onTouchSettingsChange(settings.androidTouch.copy(rightStickScale = value))
                            })
                            TouchLayoutSlider(R.string.settings_touch_stick_knob_size, settings.androidTouch.stickKnobScale, 0.28f, 0.72f, 0.02f, onChange = { value ->
                                onTouchSettingsChange(settings.androidTouch.copy(stickKnobScale = value))
                            })
                            TouchLayoutSlider(R.string.stream_panel_opacity, settings.androidTouch.opacity, 0f, 1f, TOUCH_SCALE_SLIDER_STEP, onOpacityChange)
                            TouchLayoutSlider(R.string.stream_panel_edge_padding, settings.androidTouch.edgePaddingDp, 0f, 72f, TOUCH_DP_SLIDER_STEP, onTouchEdgePaddingChange, unit = DP_UNIT)
                            TouchLayoutSlider(R.string.stream_panel_bottom_padding, settings.androidTouch.bottomPaddingDp, 0f, 120f, TOUCH_DP_SLIDER_STEP, onTouchBottomPaddingChange, unit = DP_UNIT)
                            TouchLayoutSlider(R.string.stream_panel_left_position, settings.androidTouch.leftOffsetYDp, -160f, 160f, TOUCH_DP_SLIDER_STEP, onTouchLeftOffsetChange, unit = DP_UNIT)
                            TouchLayoutSlider(R.string.stream_panel_right_position, settings.androidTouch.rightOffsetYDp, -160f, 160f, TOUCH_DP_SLIDER_STEP, onTouchRightOffsetChange, unit = DP_UNIT)
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.stream_panel_section_motion_aiming)) {
                            ControlSwitchRow(
                                label = stringResource(R.string.settings_touch_gyro),
                                checked = settings.androidTouch.gyroscopeEnabled && gyroscopeAvailable,
                                enabled = gyroscopeAvailable,
                                onCheckedChange = { enabled ->
                                    onButtonTone()
                                    onTouchSettingsChange(settings.androidTouch.copy(gyroscopeEnabled = enabled))
                                },
                                value = if (gyroscopeAvailable) {
                                    onOffLabel(settings.androidTouch.gyroscopeEnabled)
                                } else {
                                    stringResource(R.string.common_unavailable)
                                },
                                description = stringResource(
                                    if (gyroscopeAvailable) R.string.settings_touch_gyro_desc
                                    else R.string.settings_touch_gyro_unavailable,
                                ),
                            )
                            if (settings.androidTouch.gyroscopeEnabled && gyroscopeAvailable) {
                                ControlSliderRow(
                                    label = stringResource(R.string.settings_touch_gyro_sensitivity),
                                    value = settings.androidTouch.gyroscopeSensitivity,
                                    min = 0.25f,
                                    max = 3f,
                                    step = 0.05f,
                                    onChange = { value ->
                                        onTouchSettingsChange(settings.androidTouch.copy(gyroscopeSensitivity = value))
                                    },
                                    onChangePreview = { value ->
                                        onTouchSettingsChange(settings.androidTouch.copy(gyroscopeSensitivity = value))
                                    },
                                )
                                ControlSliderRow(
                                    label = stringResource(R.string.settings_touch_gyro_smoothing),
                                    value = settings.androidTouch.gyroscopeSmoothing,
                                    min = 0f,
                                    max = 0.9f,
                                    step = 0.05f,
                                    onChange = { value ->
                                        onTouchSettingsChange(settings.androidTouch.copy(gyroscopeSmoothing = value))
                                    },
                                )
                                ControlSliderRow(
                                    label = stringResource(R.string.settings_touch_gyro_dead_zone),
                                    value = settings.androidTouch.gyroscopeDeadZone,
                                    min = 0f,
                                    max = 0.2f,
                                    step = 0.005f,
                                    onChange = { value ->
                                        onTouchSettingsChange(settings.androidTouch.copy(gyroscopeDeadZone = value))
                                    },
                                )
                                ControlSwitchRow(
                                    label = stringResource(R.string.settings_touch_gyro_invert_horizontal),
                                    checked = settings.androidTouch.gyroscopeInvertHorizontal,
                                    onCheckedChange = { enabled ->
                                        onTouchSettingsChange(settings.androidTouch.copy(gyroscopeInvertHorizontal = enabled))
                                    },
                                    value = onOffLabel(settings.androidTouch.gyroscopeInvertHorizontal),
                                )
                                ControlSwitchRow(
                                    label = stringResource(R.string.settings_touch_gyro_invert_vertical),
                                    checked = settings.androidTouch.gyroscopeInvertVertical,
                                    onCheckedChange = { enabled ->
                                        onTouchSettingsChange(settings.androidTouch.copy(gyroscopeInvertVertical = enabled))
                                    },
                                    value = onOffLabel(settings.androidTouch.gyroscopeInvertVertical),
                                )
                            }
                        }
                    }
                }
                StreamControlsPage.MouseMode -> mouseModePageItems(
                    settings = settings,
                    controllerMouseEmulationEnabled = controllerMouseEmulationEnabled,
                    onControllerMouseEmulationToggle = onControllerMouseEmulationToggle,
                    onMouseSensitivityChange = onMouseSensitivityChange,
                    onMouseScrollSensitivityChange = onMouseScrollSensitivityChange,
                    onButtonTone = onButtonTone,
                )
                StreamControlsPage.ReportProblem -> {
                    item {
                        StreamBugReporter(
                            submission = bugReportSubmission,
                            versionCheck = bugReportVersionCheck,
                            update = update,
                            onSubmit = onBugReportSubmit,
                            onReset = onBugReportReset,
                            onVersionCheck = onBugReportVersionCheck,
                            onOpenUpdate = onOpenUpdate,
                            onButtonTone = onButtonTone,
                            preflightProvider = bugReportPreflightProvider,
                            initiallyExpanded = true,
                            onExpandedClose = { page = StreamControlsPage.Main },
                        )
                    }
                }
                StreamControlsPage.Main -> {
            if (showSessionTimer) {
                item {
                    StreamSessionTimerMenuRow(
                        limit = sessionTimerLimit,
                        startedAtMs = sessionStartedAtMs,
                        nowMs = sessionNowMs,
                    )
                }
            }
            item {
                ControlSection(stringResource(R.string.stream_panel_section_display)) {
                    ControlSwitchRow(
                        label = stringResource(R.string.stream_panel_stretch_to_fit),
                        checked = settings.stretchStreamToFit,
                        onCheckedChange = {
                            onButtonTone()
                            onStretchToFitToggle()
                        },
                        value = onOffLabel(settings.stretchStreamToFit),
                    )
                    ControlSwitchRow(
                        label = stringResource(R.string.stream_panel_audio),
                        checked = !audioMuted,
                        onCheckedChange = {
                            onButtonTone()
                            onAudioToggle()
                        },
                        value = if (audioMuted) stringResource(R.string.stream_panel_audio_muted) else onOffLabel(true),
                    )
                    ControlNavigationRow(
                        label = stringResource(R.string.stream_panel_status_bar),
                        onClick = {
                            onButtonTone()
                            page = StreamControlsPage.StatusBar
                        },
                        value = if (!statsVisible) {
                            onOffLabel(false)
                        } else {
                            stringResource(
                                R.string.stream_panel_status_bar_summary,
                                settings.streamStatsStyle.label,
                                settings.streamStatsMetrics.enabledCount(),
                            )
                        },
                    )
                    ControlSwitchRow(
                        label = stringResource(R.string.stream_panel_sharpening),
                        checked = settings.stream.streamSharpeningEnabled,
                        onCheckedChange = {
                            onButtonTone()
                            onSharpeningToggle()
                        },
                        value = onOffLabel(settings.stream.streamSharpeningEnabled),
                    )
                    if (settings.stream.streamSharpeningEnabled) {
                        ControlSliderRow(
                            label = stringResource(R.string.stream_panel_sharpening_amount),
                            value = settings.stream.streamSharpeningAmount,
                            min = 0f,
                            max = 1f,
                            step = SHARPENING_SLIDER_STEP,
                            onChange = onSharpeningAmountChange,
                        )
                    }
                    ControlSliderRow(
                        label = stringResource(R.string.settings_bitrate),
                        value = settings.stream.maxBitrateMbps.toFloat(),
                        min = 1f,
                        max = 150f,
                        step = 1f,
                        unit = "Mbps",
                        descriptionProvider = { mbps -> streamBitrateUsageEstimate(mbps) },
                        onChange = { value -> onMaxBitrateChange(value.roundToInt()) },
                    )
                    ControlBitrateLiveHint(
                        liveBitrateMbps = liveBitrateLimitKbps?.div(1000) ?: settings.stream.maxBitrateMbps,
                        liveOverridden = liveBitrateLimitKbps != null,
                    )
                }
            }
            item {
                ControlSection(stringResource(R.string.stream_panel_section_input)) {
                    if (microphoneRequested) {
                        ControlSwitchRow(
                            label = stringResource(R.string.stream_panel_microphone),
                            checked = microphoneEnabled && microphonePermissionGranted,
                            onCheckedChange = {
                                onButtonTone()
                                onMicrophoneToggle()
                            },
                            value = when {
                                !microphonePermissionGranted -> stringResource(R.string.stream_panel_microphone_permission)
                                microphoneEnabled -> onOffLabel(true)
                                else -> stringResource(R.string.stream_panel_audio_muted)
                            },
                        )
                    }
                    ControlActionRow(
                        label = stringResource(R.string.stream_panel_steam_menu),
                        actionLabel = stringResource(R.string.action_open),
                        onClick = {
                            onButtonTone()
                            onSteamMenuOpen()
                        },
                        value = stringResource(R.string.stream_panel_steam_menu_summary),
                    )
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        StreamPanelKeyButton(stringResource(R.string.stream_panel_key_esc), Modifier.weight(1f)) {
                            onButtonTone()
                            onEsc()
                        }
                        StreamPanelKeyButton(stringResource(R.string.stream_panel_key_enter), Modifier.weight(1f)) {
                            onButtonTone()
                            onEnter()
                        }
                        StreamPanelKeyButton(stringResource(R.string.stream_panel_key_backspace), Modifier.weight(1f)) {
                            onButtonTone()
                            onBackspace()
                        }
                    }
                    if (tvProfile) {
                        ControlSwitchRow(
                            label = stringResource(R.string.stream_panel_controller_mouse),
                            checked = controllerMouseAssistEnabled,
                            onCheckedChange = {
                                onButtonTone()
                                onControllerMouseAssistToggle()
                            },
                            value = if (controllerMouseAssistEnabled) {
                                stringResource(R.string.stream_panel_controller_mouse_summary)
                            } else {
                                onOffLabel(false)
                            },
                        )
                    } else {
                        ControlSwitchRow(
                            label = stringResource(R.string.stream_panel_finger_mouse),
                            checked = settings.androidTouch.mousePad,
                            onCheckedChange = {
                                onButtonTone()
                                onMousePadToggle()
                            },
                            value = onOffLabel(settings.androidTouch.mousePad),
                        )
                        if (settings.androidTouch.mousePad) {
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_direct_click),
                                checked = settings.androidTouch.mouseDirectClick,
                                onCheckedChange = {
                                    onButtonTone()
                                    onMouseDirectClickToggle()
                                },
                                value = onOffLabel(settings.androidTouch.mouseDirectClick),
                                // Reads as a child of Finger mouse; replaces a hand-written Box.
                                indentLevel = 1,
                            )

                            val scrollHint = when {
                                settings.stream.mouseScrollSensitivity <= 20 -> "Fast"
                                settings.stream.mouseScrollSensitivity <= 40 -> "Normal"
                                settings.stream.mouseScrollSensitivity <= 60 -> "Precise"
                                else -> "Slow"
                            }

                            ControlActionRow(
                                label = "Scroll sensitivity",
                                actionLabel = scrollHint,
                                onClick = {
                                    onButtonTone()
                                    val next = when {
                                        settings.stream.mouseScrollSensitivity <= 20 -> 40
                                        settings.stream.mouseScrollSensitivity <= 40 -> 60
                                        settings.stream.mouseScrollSensitivity <= 60 -> 80
                                        else -> 20
                                    }
                                    onMouseScrollSensitivityChange(next)
                                },
                                indentLevel = 1
                            )
                        }
                        ControlNavigationRow(
                            label = stringResource(R.string.stream_panel_touch_controller),
                            onClick = {
                                onButtonTone()
                                page = StreamControlsPage.TouchControls
                            },
                            value = when {
                                touchControlsVisible -> stringResource(R.string.common_visible)
                                nativeTouchActive -> stringResource(R.string.stream_touch_builtin_active)
                                else -> stringResource(R.string.common_hidden)
                            },
                        )
                    }
                    // Mouse mode (Left stick): shown for all profiles — works with both physical
                    // gamepad and touch controller.
                    ControlNavigationRow(
                        label = stringResource(R.string.stream_panel_mouse_mode),
                        onClick = {
                            onButtonTone()
                            page = StreamControlsPage.MouseMode
                        },
                        value = if (controllerMouseEmulationEnabled) {
                            stringResource(R.string.stream_panel_mouse_mode_summary)
                        } else {
                            onOffLabel(false)
                        },
                    )
                }
            }
            item {
                ControlSection(stringResource(R.string.stream_panel_section_support)) {
                    ControlNavigationRow(
                        label = stringResource(R.string.bug_report_open_label),
                        onClick = {
                            onButtonTone()
                            page = StreamControlsPage.ReportProblem
                        },
                        value = stringResource(R.string.bug_report_open_summary),
                    )
                }
            }
                } // StreamControlsPage.Main
            } // when (currentPage)
        } // LazyColumn
        } // AnimatedContent
        } // Column
        } // CompositionLocalProvider
    } // Surface
    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.clearStreamPanelTouchPassthroughBounds()
        }
    }
}

@Composable
private fun BuiltInGameTouchNotice(usingBuiltInTouch: Boolean) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = OpenNowPalette.StatusNotice.copy(alpha = 0.10f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, OpenNowPalette.StatusNotice.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 11.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                stringResource(R.string.stream_touch_builtin_title),
                color = OpenNowPalette.StatusNotice,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
            )
            Text(
                stringResource(
                    if (usingBuiltInTouch) {
                        R.string.stream_touch_builtin_available
                    } else {
                        R.string.stream_touch_builtin_overridden
                    },
                ),
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
internal fun BugReportDataDisclosure(
    includeTypedTextWarning: Boolean,
    modifier: Modifier = Modifier,
) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = OpenNowPalette.StatusNotice.copy(alpha = 0.10f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, OpenNowPalette.StatusNotice.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .minimumInteractiveComponentSize()
                    .clip(RoundedCornerShape(14.dp))
                    .clickable { expanded = !expanded }
                    .padding(horizontal = 12.dp, vertical = 11.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    "PrintedWaste API",
                    modifier = Modifier.weight(1f),
                    color = OpenNowPalette.StatusNotice,
                    fontWeight = FontWeight.Bold,
                    style = MaterialTheme.typography.labelLarge,
                )
                Text(
                    stringResource(R.string.bug_report_collected_title),
                    color = TextPrimary,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Icon(
                    imageVector = if (expanded) Icons.Rounded.KeyboardArrowUp else Icons.Rounded.KeyboardArrowDown,
                    contentDescription = if (expanded) "Collapse collection details" else "Expand collection details",
                    tint = TextMuted,
                )
            }
            AnimatedVisibility(visible = expanded) {
                Column(
                    modifier = Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp),
                    verticalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    Text(
                        stringResource(R.string.bug_report_collected_viewers),
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Text(
                        stringResource(R.string.bug_report_collected_redaction),
                        color = TextPrimary,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    if (includeTypedTextWarning) {
                        Text(
                            stringResource(R.string.bug_report_collected_verbatim),
                            color = TextPrimary,
                            style = MaterialTheme.typography.bodySmall,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                    Text(
                        stringResource(R.string.bug_report_collected_not_sold),
                        color = TextPrimary,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        stringResource(R.string.bug_report_collected_log),
                        color = TextMuted,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }
        }
    }
}

/**
 * Shared header for the main panel and every focused settings/support page. It stays put while the
 * selected page scrolls.
 */
/**
 * Publishes this composable's screen bounds to the native input router so touches landing on it are
 * treated as UI rather than forwarded into the game.
 *
 * Two guards the hand-written version did not have:
 *  - a zero-size measurement is ignored, instead of publishing a degenerate rect;
 *  - the rect is inflated slightly, because boundsInRoot() includes graphicsLayer transforms and
 *    the panel enters under scaleIn(0.96f) — mid-animation it would otherwise under-report and
 *    leak touches around its edge.
 *
 * The caller must keep this on a node whose size does not depend on its content. A content-driven
 * height would shrink the rect during a transition and leak touches into the game.
 */
@Composable
internal fun Modifier.streamTouchPassthrough(id: String, inflate: Dp = 8.dp): Modifier {
    val inflatePx = with(LocalDensity.current) { inflate.roundToPx() }
    DisposableEffect(id) {
        onDispose { NativeStreamInputRouter.clearOverlayTouchPassthroughBound(id) }
    }
    return onGloballyPositioned { coordinates ->
        val bounds = coordinates.boundsInRoot()
        if (bounds.width <= 0f || bounds.height <= 0f) return@onGloballyPositioned
        NativeStreamInputRouter.setOverlayTouchPassthroughBound(
            id,
            bounds.left.roundToInt() - inflatePx,
            bounds.top.roundToInt() - inflatePx,
            bounds.right.roundToInt() + inflatePx,
            bounds.bottom.roundToInt() + inflatePx,
        )
    }
}

private const val PASSTHROUGH_ID_PANEL = "controls-panel"
internal const val PASSTHROUGH_ID_KEYBOARD = "keyboard-bar"
internal const val PASSTHROUGH_ID_STATUS_BAR_KEYBOARD = "status-bar-keyboard"
internal const val PASSTHROUGH_ID_EXIT = "exit-confirmation"

@Composable
private fun StreamPanelHeader(
    page: StreamControlsPage,
    gameTitle: String,
    status: String?,
    highlightDone: Boolean,
    focusRequester: FocusRequester,
    onBack: () -> Unit,
    onKeyboardOpen: () -> Unit,
    onExit: () -> Unit,
    onClose: () -> Unit,
    onButtonTone: () -> Unit,
) {
    val onMain = page == StreamControlsPage.Main
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = OpenNowSpacing.md + 2.dp,
                end = OpenNowSpacing.md + 2.dp,
                top = OpenNowSpacing.md + 2.dp,
                bottom = OpenNowSpacing.sm,
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
    ) {
        if (!onMain) {
            StreamPanelHeaderButton(
                onClick = {
                    onButtonTone()
                    onBack()
                },
                modifier = Modifier.focusRequester(focusRequester),
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_arrow_back),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
                Spacer(Modifier.width(6.dp))
                Text(stringResource(R.string.action_back), maxLines = 1)
            }
        }
        Column(Modifier.weight(1f)) {
            Text(
                stringResource(
                    when (page) {
                        StreamControlsPage.Main -> R.string.stream_panel_title
                        StreamControlsPage.StatusBar -> R.string.stream_statusbar_title
                        StreamControlsPage.TouchControls -> R.string.stream_touch_controls_title
                        StreamControlsPage.MouseMode -> R.string.stream_mouse_mode_title
                        StreamControlsPage.ReportProblem -> R.string.stream_report_problem_title
                    },
                ),
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                when (page) {
                    StreamControlsPage.Main -> gameTitle
                    StreamControlsPage.StatusBar -> stringResource(R.string.stream_statusbar_subtitle)
                    StreamControlsPage.TouchControls -> stringResource(R.string.stream_touch_controls_subtitle)
                    StreamControlsPage.MouseMode -> stringResource(R.string.stream_mouse_mode_subtitle)
                    StreamControlsPage.ReportProblem -> stringResource(R.string.stream_report_problem_subtitle)
                },
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (onMain) {
            if (status != null) {
                Text(status, color = TextMuted, style = MaterialTheme.typography.labelMedium, maxLines = 1)
            }
            StreamPanelHeaderButton(
                onClick = {
                    onButtonTone()
                    onKeyboardOpen()
                },
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_keyboard),
                    contentDescription = stringResource(R.string.stream_panel_cd_keyboard),
                    tint = TextPrimary,
                    modifier = Modifier.size(20.dp),
                )
            }
            StreamPanelHeaderButton(
                onClick = {
                    onButtonTone()
                    onExit()
                },
            ) {
                Text(stringResource(R.string.stream_panel_exit), maxLines = 1)
            }
            val doneAction = {
                onButtonTone()
                onClose()
            }
            if (highlightDone) {
                var doneFocused by remember { mutableStateOf(false) }
                Button(
                    onClick = doneAction,
                    modifier = Modifier
                        .focusRequester(focusRequester)
                        .onFocusChanged { doneFocused = it.isFocused },
                    border = BorderStroke(2.dp, if (doneFocused) MaterialTheme.colorScheme.primary else TextPrimary),
                    contentPadding = PaddingValues(horizontal = OpenNowSpacing.md, vertical = 6.dp),
                ) {
                    Text(stringResource(R.string.stream_panel_done), maxLines = 1)
                }
            } else {
                StreamPanelHeaderButton(onClick = doneAction, modifier = Modifier.focusRequester(focusRequester)) {
                    Text(stringResource(R.string.stream_panel_done), maxLines = 1)
                }
            }
        }
    }
}

/**
 * An outlined button that actually shows a focus ring. OutlinedButton alone gives no visible focus
 * state here, so the panel used to repeat this onFocusChanged + border pattern per button.
 */
@Composable
private fun StreamPanelHeaderButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    OutlinedButton(
        onClick = onClick,
        modifier = modifier.onFocusChanged { focused = it.isFocused },
        border = BorderStroke(
            width = if (focused) 2.dp else 1.dp,
            color = if (focused) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
        ),
        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
        content = content,
    )
}

/** Slides forward going into a sub-page and back coming out of one. */
private fun streamPanelPageTransition(
    from: StreamControlsPage,
    to: StreamControlsPage,
    reduceMotion: Boolean,
): ContentTransform {
    if (reduceMotion) {
        return fadeIn(tween(0)) togetherWith fadeOut(tween(0))
    }
    val forward = from == StreamControlsPage.Main && to != StreamControlsPage.Main
    val duration = OpenNowMotion.DurationStandard
    val easing = OpenNowMotion.EasingStandard
    return (
        slideInHorizontally(tween(duration, easing = easing)) { width -> if (forward) width / 6 else -width / 6 } +
            fadeIn(tween(duration, easing = easing))
        ) togetherWith (
        slideOutHorizontally(tween(duration, easing = easing)) { width -> if (forward) -width / 6 else width / 6 } +
            fadeOut(tween(OpenNowMotion.DurationFast, easing = easing))
        )
}

@Composable
private fun BugReportSubmissionRequirements(modifier: Modifier = Modifier) {
    Text(
        "Bug reports require English as the OpenNOW or device language. Descriptions must contain at least $ANDROID_BUG_REPORT_MIN_MEANINGFUL_CHARS meaningful letters or numbers and explain what happened. Non-English, repeated, or random text cannot be sent.",
        modifier = modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.error,
        fontWeight = FontWeight.Bold,
        style = MaterialTheme.typography.bodyMedium,
    )
}

@Composable
internal fun BugReportLocaleGateCard(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.error.copy(alpha = 0.10f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.error.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                stringResource(R.string.bug_report_english_required),
                color = MaterialTheme.colorScheme.error,
                fontWeight = FontWeight.Bold,
                style = MaterialTheme.typography.labelLarge,
            )
            Text(
                stringResource(R.string.bug_report_english_required_body),
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
            Button(
                onClick = { setAndroidAppLanguage(context, ANDROID_APP_LANGUAGE_ENGLISH) },
            ) {
                Text(stringResource(R.string.bug_report_use_english))
            }
        }
    }
}

@Composable
internal fun BugReportKnownIssueOverride(
    block: BugReportKnownIssueBlock,
    checked: Boolean,
    enabled: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val warning = Color(0xffffc266)
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(10.dp),
        color = warning.copy(alpha = 0.09f),
        border = BorderStroke(1.dp, warning.copy(alpha = 0.34f)),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 9.dp, vertical = 7.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            Text(block.title, color = warning, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
            Text(block.action, color = TextMuted, style = MaterialTheme.typography.labelSmall)
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(8.dp))
                    .clickable(enabled = enabled) { onCheckedChange(!checked) },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Checkbox(
                    checked = checked,
                    onCheckedChange = onCheckedChange,
                    enabled = enabled,
                    modifier = Modifier.size(36.dp),
                )
                Text(
                    stringResource(R.string.bug_report_known_issue_override),
                    modifier = Modifier.weight(1f),
                    color = TextMuted,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
        }
    }
}

@Composable
internal fun BugReportVersionGateCard(
    update: AndroidUpdateState,
    versionCheck: AndroidBugReportVersionCheckState,
    onRetry: () -> Unit,
    onOpenUpdate: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val updateRequired = update.status == AndroidUpdateStatus.Available ||
        versionCheck.status == AndroidBugReportVersionCheckStatus.UpdateRequired
    val checking = versionCheck.status == AndroidBugReportVersionCheckStatus.Checking
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.error.copy(alpha = 0.10f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.error.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                when {
                    updateRequired -> "Update required before reporting"
                    checking -> "Checking Google Play"
                    else -> "Google Play version check required"
                },
                color = MaterialTheme.colorScheme.error,
                fontWeight = FontWeight.Bold,
                style = MaterialTheme.typography.labelLarge,
            )
            Text(
                androidBugReportBlockMessage(update, versionCheck)
                    ?: "OpenNOW must verify the installed Play Store build before sending a report.",
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
            when {
                updateRequired -> Button(onClick = onOpenUpdate) {
                    Text(stringResource(R.string.bug_report_update_play))
                }
                checking -> Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text(stringResource(R.string.bug_report_checking_build), style = MaterialTheme.typography.bodySmall)
                }
                else -> OutlinedButton(onClick = onRetry) {
                    Text(stringResource(R.string.bug_report_retry_version))
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BugReportPreflightDeckView(
    deck: BugReportPreflightDeck,
    page: Int,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onRefresh: () -> Unit,
    onCancel: () -> Unit,
) {
    val card = deck.cards[page]
    val accent = when (card.tone) {
        BugReportPreflightTone.Healthy -> Green
        BugReportPreflightTone.Notice -> MaterialTheme.colorScheme.primary
        BugReportPreflightTone.Warning -> Color(0xffffc266)
    }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(stringResource(R.string.bug_report_preflight_title), fontWeight = FontWeight.Bold, style = MaterialTheme.typography.titleMedium)
                Text(
                    stringResource(R.string.bug_report_preflight_subtitle),
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Text(
                "${page + 1} / ${deck.cards.size}",
                color = accent,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
            )
        }

        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            deck.cards.indices.forEach { index ->
                Box(
                    Modifier
                        .height(4.dp)
                        .weight(1f)
                        .clip(RoundedCornerShape(999.dp))
                        .background(if (index <= page) accent else Color.White.copy(alpha = 0.10f)),
                )
            }
        }

        AnimatedContent(
            targetState = page,
            transitionSpec = { fadeIn(tween(140)) togetherWith fadeOut(tween(100)) },
            label = "bug-report-preflight-card",
        ) { targetPage ->
            val targetCard = deck.cards[targetPage]
            val targetAccent = when (targetCard.tone) {
                BugReportPreflightTone.Healthy -> Green
                BugReportPreflightTone.Notice -> MaterialTheme.colorScheme.primary
                BugReportPreflightTone.Warning -> Color(0xffffc266)
            }
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(18.dp),
                color = targetAccent.copy(alpha = 0.08f),
                border = BorderStroke(1.dp, targetAccent.copy(alpha = 0.34f)),
            ) {
                Column(
                    modifier = Modifier.padding(15.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text(
                        targetCard.label,
                        color = targetAccent,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        letterSpacing = 0.8.sp,
                    )
                    Text(
                        targetCard.title,
                        color = TextPrimary,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        targetCard.summary,
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    if (targetCard.facts.isNotEmpty()) {
                        FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            targetCard.facts.forEach { fact ->
                                Surface(
                                    shape = RoundedCornerShape(999.dp),
                                    color = PanelAlt,
                                    border = if (LocalAbsoluteCinemaEffects.current) {
                                        BorderStroke(1.dp, LocalActiveSelectionColor.current)
                                    } else {
                                        null
                                    },
                                ) {
                                    Text(
                                        fact,
                                        modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
                                        color = TextPrimary,
                                        style = MaterialTheme.typography.labelSmall,
                                    )
                                }
                            }
                        }
                    }
                    if (targetCard.recommendations.isNotEmpty()) {
                        Text(
                            stringResource(R.string.bug_report_matched_suggestions),
                            color = targetAccent,
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Bold,
                        )
                        targetCard.recommendations.forEach { finding ->
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(9.dp),
                                verticalAlignment = Alignment.Top,
                            ) {
                                Surface(
                                    modifier = Modifier.size(7.dp).offset(y = 6.dp),
                                    shape = CircleShape,
                                    color = targetAccent,
                                ) {}
                                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                    Text(
                                        finding.title,
                                        color = TextPrimary,
                                        style = MaterialTheme.typography.bodySmall,
                                        fontWeight = FontWeight.Bold,
                                    )
                                    Text(
                                        finding.detail,
                                        color = TextMuted,
                                        style = MaterialTheme.typography.labelSmall,
                                    )
                                }
                            }
                        }
                    } else {
                        Text(
                            stringResource(R.string.bug_report_no_suggestions),
                            color = targetAccent,
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
        }

        Text(
            stringResource(R.string.bug_report_still_happening),
            color = TextMuted,
            style = MaterialTheme.typography.labelSmall,
        )

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onRefresh) {
                Text(stringResource(R.string.action_refresh))
            }
            Spacer(Modifier.weight(1f))
            OutlinedButton(onClick = if (page == 0) onCancel else onPrevious) {
                Text(if (page == 0) stringResource(R.string.action_cancel) else stringResource(R.string.action_back))
            }
            Button(onClick = onNext) {
                Text(if (page == deck.cards.lastIndex) stringResource(R.string.action_continue) else stringResource(R.string.action_next))
            }
        }
    }
}

@Composable
internal fun BugReportFormInputs(
    title: String,
    description: String,
    consentChecked: Boolean,
    knownIssueBlock: BugReportKnownIssueBlock?,
    acknowledgedKnownIssueKey: String?,
    submission: BugReportSubmissionState,
    onTitleChange: (String) -> Unit,
    onDescriptionChange: (String) -> Unit,
    onConsentChange: (Boolean) -> Unit,
    onKnownIssueAcknowledgementChange: (String?) -> Unit,
    onConfirm: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val descriptionError = androidBugReportDescriptionError(description)
    val titleError = androidBugReportTitleError(title)
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        OutlinedTextField(
            value = title,
            onValueChange = onTitleChange,
            modifier = Modifier.fillMaxWidth(),
            enabled = !submission.uploading,
            singleLine = true,
            label = { Text(stringResource(R.string.bug_report_title_label)) },
            placeholder = { Text(stringResource(R.string.bug_report_title_placeholder)) },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
        )
        OutlinedTextField(
            value = description,
            onValueChange = onDescriptionChange,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 128.dp),
            enabled = !submission.uploading,
            minLines = 4,
            maxLines = 7,
            label = { Text(stringResource(R.string.bug_report_description_label)) },
            placeholder = { Text(stringResource(R.string.bug_report_description_placeholder)) },
            supportingText = {
                Text(
                    descriptionError
                        ?: "${androidBugReportMeaningfulCharacterCount(description)} / $ANDROID_BUG_REPORT_MIN_MEANINGFUL_CHARS meaningful characters",
                )
            },
            isError = description.isNotEmpty() && descriptionError != null,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Default),
        )
        BugReportDescriptionFeedback(
            description = description,
            error = descriptionError,
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .clickable(enabled = !submission.uploading) {
                    onConsentChange(!consentChecked)
                }
                .padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(
                checked = consentChecked,
                onCheckedChange = onConsentChange,
                enabled = !submission.uploading,
            )
            Text(
                stringResource(R.string.bug_report_consent_upload),
                modifier = Modifier.weight(1f),
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        knownIssueBlock?.let { block ->
            BugReportKnownIssueOverride(
                block = block,
                checked = acknowledgedKnownIssueKey == block.key,
                enabled = !submission.uploading,
                onCheckedChange = { checked ->
                    onKnownIssueAcknowledgementChange(block.key.takeIf { checked })
                },
            )
        }
        submission.error?.let { error ->
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(10.dp),
                color = MaterialTheme.colorScheme.error.copy(alpha = 0.12f),
                contentColor = MaterialTheme.colorScheme.error,
            ) {
                Text(
                    error,
                    modifier = Modifier.padding(10.dp),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
        Button(
            onClick = onConfirm,
            enabled = titleError == null &&
                descriptionError == null &&
                consentChecked &&
                bugReportKnownIssueAllowsSubmission(knownIssueBlock, acknowledgedKnownIssueKey) &&
                !submission.uploading,
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (submission.uploading) {
                CircularProgressIndicator(
                    modifier = Modifier.size(18.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onPrimary,
                )
                Spacer(Modifier.width(8.dp))
                Text(stringResource(R.string.bug_report_uploading))
            } else {
                Text(if (knownIssueBlock == null) "Send bug report" else "Send anyway")
            }
        }
    }
}

@Composable
internal fun BugReportDescriptionFeedback(
    description: String,
    error: String?,
    modifier: Modifier = Modifier,
) {
    if (description.isEmpty() || error == null) return

    val meaningfulCharacters = androidBugReportMeaningfulCharacterCount(description)
    val missingCharacters = (ANDROID_BUG_REPORT_MIN_MEANINGFUL_CHARS - meaningfulCharacters).coerceAtLeast(0)
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(10.dp),
        color = MaterialTheme.colorScheme.error.copy(alpha = 0.14f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.error.copy(alpha = 0.48f)),
    ) {
        Column(
            modifier = Modifier.padding(10.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            Text(
                if (missingCharacters > 0) "Add more detail" else "Description needs attention",
                color = MaterialTheme.colorScheme.error,
                fontWeight = FontWeight.Bold,
                style = MaterialTheme.typography.labelLarge,
            )
            Text(
                if (missingCharacters > 0) {
                    "$meaningfulCharacters / $ANDROID_BUG_REPORT_MIN_MEANINGFUL_CHARS meaningful characters. Add $missingCharacters more letters or numbers before sending."
                } else {
                    error
                },
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}

@Composable
private fun StreamBugReporter(
    submission: BugReportSubmissionState,
    versionCheck: AndroidBugReportVersionCheckState,
    update: AndroidUpdateState,
    onSubmit: (String, String, String?) -> Unit,
    onReset: () -> Unit,
    onVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    onButtonTone: () -> Unit,
    preflightProvider: () -> BugReportPreflightDeck,
    initiallyExpanded: Boolean = false,
    onExpandedClose: () -> Unit = {},
) {
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val appLocale = currentAndroidAppLocale(LocalContext.current)
    var expanded by rememberSaveable(initiallyExpanded) { mutableStateOf(initiallyExpanded) }
    var title by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    var consentChecked by rememberSaveable { mutableStateOf(false) }
    var confirmationOpen by rememberSaveable { mutableStateOf(false) }
    var preflightReviewed by rememberSaveable { mutableStateOf(false) }
    var preflightPage by rememberSaveable { mutableStateOf(0) }
    var preflightDeck by remember { mutableStateOf<BugReportPreflightDeck?>(null) }
    var acknowledgedKnownIssueKey by rememberSaveable { mutableStateOf<String?>(null) }
    val knownIssueBlock = preflightDeck?.let { deck ->
        bugReportKnownIssueBlock(title, description, deck)
    }

    LaunchedEffect(expanded, update.installSource.isGooglePlay) {
        if (expanded && update.installSource.isGooglePlay) {
            onVersionCheck()
        }
        if (expanded && !preflightReviewed && preflightDeck == null) {
            preflightDeck = preflightProvider()
        }
    }

    ControlSection(stringResource(R.string.bug_report_section)) {
        if (!expanded) {
            ControlActionRow(
                label = stringResource(R.string.bug_report_open_label),
                actionLabel = stringResource(R.string.action_open),
                onClick = {
                    onButtonTone()
                    preflightReviewed = false
                    preflightPage = 0
                    preflightDeck = preflightProvider()
                    expanded = true
                },
                value = stringResource(R.string.bug_report_open_summary),
            )
            return@ControlSection
        }

        DiscordCommunityLink(
            summary = stringResource(R.string.discord_community_bug_report_summary),
        )

        if (submission.submitted) {
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(14.dp),
                color = Green.copy(alpha = 0.12f),
                contentColor = TextPrimary,
                border = BorderStroke(1.dp, Green.copy(alpha = 0.45f)),
            ) {
                Column(
                    modifier = Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Icon(Icons.Rounded.Check, contentDescription = null, tint = Green)
                        Text(stringResource(R.string.bug_report_sent), fontWeight = FontWeight.Bold)
                    }
                    submission.reference?.let { reportId ->
                        CopyableBugReportId(reportId)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(
                            onClick = {
                                onButtonTone()
                                title = ""
                                description = ""
                                consentChecked = false
                                acknowledgedKnownIssueKey = null
                                confirmationOpen = false
                                preflightReviewed = false
                                preflightPage = 0
                                preflightDeck = preflightProvider()
                                onReset()
                            },
                        ) {
                            Text(stringResource(R.string.bug_report_send_another))
                        }
                        TextButton(
                            onClick = {
                                onButtonTone()
                                preflightReviewed = false
                                preflightPage = 0
                                preflightDeck = null
                                expanded = false
                                onExpandedClose()
                            },
                        ) {
                            Text(stringResource(R.string.action_close))
                        }
                    }
                }
            }
            return@ControlSection
        }

        if (!appLocale.bugReportsAllowed) {
            BugReportLocaleGateCard()
            return@ControlSection
        }

        if (!androidBugReportsAllowed(update, versionCheck)) {
            BugReportVersionGateCard(
                update = update,
                versionCheck = versionCheck,
                onRetry = onVersionCheck,
                onOpenUpdate = onOpenUpdate,
            )
            return@ControlSection
        }

        if (!preflightReviewed) {
            val deck = preflightDeck
            if (deck == null) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(vertical = 20.dp),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(22.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text(stringResource(R.string.bug_report_checking_session), color = TextMuted)
                }
            } else {
                BugReportPreflightDeckView(
                    deck = deck,
                    page = preflightPage.coerceIn(deck.cards.indices),
                    onPrevious = {
                        onButtonTone()
                        preflightPage = (preflightPage - 1).coerceAtLeast(0)
                    },
                    onNext = {
                        onButtonTone()
                        if (preflightPage < deck.cards.lastIndex) {
                            preflightPage += 1
                        } else {
                            preflightReviewed = true
                        }
                    },
                    onRefresh = {
                        onButtonTone()
                        preflightPage = 0
                        preflightDeck = preflightProvider()
                    },
                    onCancel = {
                        onButtonTone()
                        preflightReviewed = false
                        preflightPage = 0
                        preflightDeck = null
                        expanded = false
                        onExpandedClose()
                    },
                )
            }
            return@ControlSection
        }

        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(stringResource(R.string.bug_report_open_label), fontWeight = FontWeight.Bold)
                    Text(
                        stringResource(R.string.bug_report_inline_subtitle),
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                TextButton(
                    enabled = !submission.uploading,
                    onClick = {
                        onButtonTone()
                        preflightReviewed = false
                        preflightPage = 0
                        preflightDeck = preflightProvider()
                    },
                ) {
                    Text(stringResource(R.string.bug_report_checks_tab))
                }
                TextButton(
                    enabled = !submission.uploading,
                    onClick = {
                        onButtonTone()
                        preflightReviewed = false
                        preflightPage = 0
                        preflightDeck = null
                        expanded = false
                        onExpandedClose()
                    },
                ) {
                    Text(stringResource(R.string.action_cancel))
                }
            }

            if (landscapeLayout) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
                    verticalAlignment = Alignment.Top,
                ) {
                    Column(
                        modifier = Modifier.weight(0.9f),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        BugReportSubmissionRequirements()
                        BugReportDataDisclosure(includeTypedTextWarning = true)
                    }
                    BugReportFormInputs(
                        title = title,
                        description = description,
                        consentChecked = consentChecked,
                        knownIssueBlock = knownIssueBlock,
                        acknowledgedKnownIssueKey = acknowledgedKnownIssueKey,
                        submission = submission,
                        onTitleChange = { value ->
                            title = value
                            if (submission.error != null) onReset()
                        },
                        onDescriptionChange = { value ->
                            description = value
                            if (submission.error != null) onReset()
                        },
                        onConsentChange = { consentChecked = it },
                        onKnownIssueAcknowledgementChange = { acknowledgedKnownIssueKey = it },
                        onConfirm = {
                            onButtonTone()
                            confirmationOpen = true
                        },
                        modifier = Modifier.weight(1.1f),
                    )
                }
            } else {
                BugReportSubmissionRequirements()
                BugReportDataDisclosure(includeTypedTextWarning = true)
                BugReportFormInputs(
                    title = title,
                    description = description,
                    consentChecked = consentChecked,
                    knownIssueBlock = knownIssueBlock,
                    acknowledgedKnownIssueKey = acknowledgedKnownIssueKey,
                    submission = submission,
                    onTitleChange = { value ->
                        title = value
                        if (submission.error != null) onReset()
                    },
                    onDescriptionChange = { value ->
                        description = value
                        if (submission.error != null) onReset()
                    },
                    onConsentChange = { consentChecked = it },
                    onKnownIssueAcknowledgementChange = { acknowledgedKnownIssueKey = it },
                    onConfirm = {
                        onButtonTone()
                        confirmationOpen = true
                    },
                )
            }
        }
    }

    if (confirmationOpen) {
        AlertDialog(
            onDismissRequest = { confirmationOpen = false },
            modifier = if (landscapeLayout) {
                Modifier.widthIn(max = 760.dp).fillMaxWidth(0.82f)
            } else {
                Modifier
            },
            properties = DialogProperties(usePlatformDefaultWidth = !landscapeLayout),
            title = { Text(stringResource(R.string.bug_report_upload_confirm_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    knownIssueBlock?.let { block ->
                        Text(block.title, color = Color(0xffffc266), fontWeight = FontWeight.Bold)
                        Text(block.action, color = TextMuted, style = MaterialTheme.typography.bodySmall)
                    }
                    BugReportSubmissionRequirements()
                    BugReportDataDisclosure(includeTypedTextWarning = true)
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        onButtonTone()
                        confirmationOpen = false
                        onSubmit(
                            title,
                            description,
                            knownIssueBlock?.key?.takeIf { it == acknowledgedKnownIssueKey },
                        )
                    },
                ) {
                    Text(stringResource(R.string.bug_report_upload_action))
                }
            },
            dismissButton = {
                TextButton(
                    onClick = {
                        onButtonTone()
                        confirmationOpen = false
                    },
                ) {
                    Text(stringResource(R.string.action_go_back))
                }
            },
        )
    }
}

private fun LazyListScope.mouseModePageItems(
    settings: AppSettings,
    controllerMouseEmulationEnabled: Boolean,
    onControllerMouseEmulationToggle: () -> Unit,
    onMouseSensitivityChange: (Float) -> Unit,
    onMouseScrollSensitivityChange: (Int) -> Unit,
    onButtonTone: () -> Unit,
) {
    item {
        ControlSwitchRow(
            label = "Enable Mouse Mode",
            checked = controllerMouseEmulationEnabled,
            onCheckedChange = {
                onButtonTone()
                onControllerMouseEmulationToggle()
            },
            value = onOffLabel(controllerMouseEmulationEnabled),
        )
    }
    if (controllerMouseEmulationEnabled) {
        item {
            ControlSliderRow(
                label = "Mouse sensitivity",
                value = settings.stream.mouseSensitivity,
                min = 0.25f,
                max = 3f,
                step = 0.05f,
                onChange = onMouseSensitivityChange,
                valueFormatter = { "%.2fx".format(it) }
            )
        }
        item {
            val scrollHint = when {
                settings.stream.mouseScrollSensitivity <= 20 -> "Fast"
                settings.stream.mouseScrollSensitivity <= 40 -> "Normal"
                settings.stream.mouseScrollSensitivity <= 60 -> "Precise"
                else -> "Slow"
            }
            ControlSliderRow(
                label = "Scroll sensitivity",
                value = settings.stream.mouseScrollSensitivity.toFloat(),
                min = 10f,
                max = 100f,
                step = 5f,
                onChange = { onMouseScrollSensitivityChange(it.toInt()) },
                descriptionProvider = { "Speed: $scrollHint" }
            )
        }
    }
}


@OptIn(ExperimentalLayoutApi::class)
private fun LazyListScope.statusBarPageItems(
    settings: AppSettings,
    statsVisible: Boolean,
    onStatsToggle: () -> Unit,
    onStatsStyleCycle: () -> Unit,
    onStatsPositionCycle: () -> Unit,
    onStatsMetricsChange: (StreamStatsMetrics) -> Unit,
    onKeyboardButtonToggle: () -> Unit,
    onButtonTone: () -> Unit,
) {
    item {
        ControlSwitchRow(
            label = stringResource(R.string.common_visible),
            checked = statsVisible,
            onCheckedChange = {
                onButtonTone()
                onStatsToggle()
            },
            value = onOffLabel(statsVisible),
        )
    }
    item {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            ControlActionRow(
                label = stringResource(R.string.stream_statusbar_appearance),
                actionLabel = settings.streamStatsStyle.label,
                onClick = {
                    onButtonTone()
                    onStatsStyleCycle()
                },
                modifier = Modifier.weight(1f),
            )
            ControlActionRow(
                label = stringResource(R.string.stream_statusbar_position),
                actionLabel = settings.streamStatsPosition.label,
                onClick = {
                    onButtonTone()
                    onStatsPositionCycle()
                },
                modifier = Modifier.weight(1f),
            )
        }
    }
    item {
        Text(
            stringResource(R.string.stream_statusbar_items),
            color = TextMuted,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
        )
    }
    item {
        // Compact toggles side by side; the standard row height would waste the panel.
        val statusBarMetricStyle = ControlRowStyle.stream().copy(
            verticalPadding = 6.dp,
            labelStyle = MaterialTheme.typography.labelMedium,
        )
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val columns = when {
                maxWidth >= 800.dp -> 5
                maxWidth >= 620.dp -> 4
                maxWidth >= 460.dp -> 3
                else -> 2
            }
            val gap = 8.dp
            val itemWidth = (maxWidth - gap * (columns - 1)) / columns.toFloat()
            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                maxItemsInEachRow = columns,
                horizontalArrangement = Arrangement.spacedBy(gap),
                verticalArrangement = Arrangement.spacedBy(gap),
            ) {
                StreamStatusItem.entries.forEach { item ->
                    ControlSwitchRow(
                        label = stringResource(item.labelRes),
                        checked = item.enabledIn(settings),
                        onCheckedChange = { enabled ->
                            onButtonTone()
                            if (item == StreamStatusItem.Keyboard) {
                                onKeyboardButtonToggle()
                            } else {
                                onStatsMetricsChange(item.setEnabled(settings, enabled).streamStatsMetrics)
                            }
                        },
                        modifier = Modifier.width(itemWidth),
                        style = statusBarMetricStyle,
                    )
                }
            }
        }
    }
}

/**
 * The three bare key buttons in the Input section. Extracted so the manual focus-ring pattern the
 * panel needs lives in one place instead of being repeated per button.
 */
@Composable
private fun StreamPanelKeyButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    var focused by remember { mutableStateOf(false) }
    OutlinedButton(
        onClick = onClick,
        modifier = modifier.onFocusChanged { focused = it.isFocused },
        border = BorderStroke(
            width = if (focused) 2.dp else 1.dp,
            color = if (focused) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
        ),
    ) {
        Text(label, maxLines = 1)
    }
}

/**
 * A touch-layout slider. Unlike the settings sliders these preview on every drag frame, because
 * the overlay they are adjusting is on screen underneath the panel and watching it move is the
 * point of the control.
 */
@Composable
private fun TouchLayoutSlider(
    @StringRes labelRes: Int,
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    onChange: (Float) -> Unit,
    unit: String? = null,
) {
    ControlSliderRow(
        label = stringResource(labelRes),
        value = value,
        min = min,
        max = max,
        step = step,
        onChange = onChange,
        onChangePreview = onChange,
        unit = unit,
    )
}

/** "On" / "Off", so the same boolean reads the same way everywhere. */
@Composable
internal fun onOffLabel(enabled: Boolean): String =
    stringResource(if (enabled) R.string.common_on else R.string.common_off)

private const val SHARPENING_SLIDER_STEP = 0.05f
private const val TOUCH_SCALE_SLIDER_STEP = 0.05f
private const val TOUCH_DP_SLIDER_STEP = 2f
private const val JOYSTICK_DEAD_ZONE_STEP = 0.01f
private const val DP_UNIT = "dp"
