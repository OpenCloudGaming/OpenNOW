package com.opencloudgaming.opennow

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.text.KeyboardActions
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
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material.icons.automirrored.rounded.BatteryUnknown
import androidx.compose.material.icons.rounded.Battery0Bar
import androidx.compose.material.icons.rounded.Battery1Bar
import androidx.compose.material.icons.rounded.Battery2Bar
import androidx.compose.material.icons.rounded.Battery3Bar
import androidx.compose.material.icons.rounded.Battery4Bar
import androidx.compose.material.icons.rounded.Battery5Bar
import androidx.compose.material.icons.rounded.Battery6Bar
import androidx.compose.material.icons.rounded.BatteryFull
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.SignalCellular0Bar
import androidx.compose.material.icons.rounded.SignalCellular4Bar
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material.icons.rounded.SignalCellularAlt1Bar
import androidx.compose.material.icons.rounded.SignalCellularAlt2Bar
import androidx.compose.material.icons.rounded.SignalWifi0Bar
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material.icons.rounded.Wifi1Bar
import androidx.compose.material.icons.rounded.Wifi2Bar
import androidx.compose.material.icons.rounded.WifiOff
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.numeric
import com.opencloudgaming.opennow.ui.theme.tint

@Composable
internal fun StreamKeyboardBar(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    clearConfirmationEnabled: Boolean,
    onClear: () -> Unit,
    onDisableClearConfirmation: () -> Unit,
    onEnter: () -> Unit,
    onEsc: () -> Unit,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val inputFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val focusManager = LocalFocusManager.current
    var clearConfirmationOpen by remember { mutableStateOf(false) }
    var neverAskAgain by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { inputFocusRequester.requestFocus() }
        keyboardController?.show()
    }
    Surface(
        modifier = modifier
            // The stream runs edge-to-edge with the system bars hidden, so adjustResize does not
            // push this bar up when the IME opens: without imePadding the Android keyboard would
            // cover the text field and the action buttons below it.
            .imePadding()
            .fillMaxWidth()
            // The keyboard bar registered no passthrough bounds at all, so on a phone every tap on
            // it — including on the text field — was also forwarded into the game as touch input.
            .streamTouchPassthrough(PASSTHROUGH_ID_KEYBOARD),
        // imePadding on the parent keeps this single compact row directly above the system IME.
        shape = RoundedCornerShape(topStart = OpenNowRadius.lg, topEnd = OpenNowRadius.lg),
        color = OpenNowPalette.PanelOverVideo,
        border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
        tonalElevation = 8.dp,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = OpenNowSpacing.sm, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = value,
                onValueChange = onValueChange,
                modifier = Modifier
                    .weight(1f)
                    .height(52.dp)
                    .focusRequester(inputFocusRequester),
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge.copy(
                    color = Color.White,
                    fontWeight = FontWeight.Medium,
                ),
                placeholder = { Text(stringResource(R.string.stream_text_placeholder), color = TextMuted) },
                colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = Color.White,
                    unfocusedTextColor = Color.White,
                    cursorColor = MaterialTheme.colorScheme.primary,
                    focusedBorderColor = MaterialTheme.colorScheme.primary,
                    unfocusedBorderColor = Color.White.copy(alpha = 0.72f),
                    focusedContainerColor = Color.Black.copy(alpha = 0.52f),
                    unfocusedContainerColor = Color.Black.copy(alpha = 0.42f),
                ),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Text,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Send,
                ),
                keyboardActions = KeyboardActions(onSend = { onEnter() }),
            )
            TextButton(
                onClick = {
                    if (clearConfirmationEnabled) {
                        neverAskAgain = false
                        clearConfirmationOpen = true
                    } else {
                        onClear()
                    }
                },
                contentPadding = PaddingValues(horizontal = 10.dp),
            ) {
                Text(stringResource(R.string.stream_panel_clear))
            }
            OutlinedButton(onClick = onEnter, contentPadding = PaddingValues(horizontal = 12.dp)) { Text(stringResource(R.string.stream_panel_key_enter)) }
            TextButton(onClick = onEsc, contentPadding = PaddingValues(horizontal = 10.dp)) { Text(stringResource(R.string.stream_panel_key_esc)) }
            TextButton(
                onClick = {
                    keyboardController?.hide()
                    focusManager.clearFocus(force = true)
                    onDone()
                },
                contentPadding = PaddingValues(horizontal = 10.dp),
            ) { Text(stringResource(R.string.stream_panel_done)) }
        }
    }
    if (clearConfirmationOpen) {
        AlertDialog(
            onDismissRequest = { clearConfirmationOpen = false },
            title = { Text(stringResource(R.string.stream_keyboard_clear_confirm_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(stringResource(R.string.stream_keyboard_clear_confirm_body))
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { neverAskAgain = !neverAskAgain },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(
                            checked = neverAskAgain,
                            onCheckedChange = null,
                        )
                        Text(stringResource(R.string.stream_keyboard_clear_never_ask_again))
                    }
                }
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        clearConfirmationOpen = false
                        if (neverAskAgain) onDisableClearConfirmation()
                        onClear()
                    },
                ) {
                    Text(stringResource(R.string.stream_panel_clear))
                }
            },
            dismissButton = {
                TextButton(onClick = { clearConfirmationOpen = false }) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }
}

internal const val MAX_STREAM_KEYBOARD_TEXT_LENGTH = 4096

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun StreamStatsPill(
    streamStats: StreamRuntimeStats,
    streamSettings: StreamSettings,
    style: StreamStatsStyle,
    metrics: StreamStatsMetrics,
    serverLocation: String?,
    keyboardButtonEnabled: Boolean,
    onKeyboardOpen: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (metrics.enabledCount() == 0 && !keyboardButtonEnabled) return
    val compact = style == StreamStatsStyle.Compact
    val deviceStatus = rememberCompactStreamDeviceStatus()
    Surface(
        modifier = modifier
            .padding(OpenNowSpacing.sm)
            .widthIn(max = if (compact) 720.dp else 300.dp),
        shape = RoundedCornerShape(if (compact) OpenNowRadius.full else OpenNowRadius.lg),
        // This sits over gameplay, so keep the capsule clean and borderless. Top-level Cinema
        // chrome must not leak into the in-stream status overlay.
        color = Panel.copy(alpha = 0.52f),
        tonalElevation = 0.dp,
    ) {
        if (compact) {
            Row(
                Modifier.padding(horizontal = OpenNowSpacing.md, vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                StreamStatsMetricItems(streamStats, streamSettings, metrics, deviceStatus, serverLocation)
                if (keyboardButtonEnabled) {
                    StreamStatusKeyboardButton(onClick = onKeyboardOpen)
                }
            }
        } else {
            FlowRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = OpenNowSpacing.md, vertical = OpenNowSpacing.sm),
                maxItemsInEachRow = 2,
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                StreamStatsMetricItems(
                    streamStats,
                    streamSettings,
                    metrics,
                    deviceStatus,
                    serverLocation,
                    // Two aligned columns instead of a ragged pair of runs.
                    itemModifier = Modifier.weight(1f),
                )
                if (keyboardButtonEnabled) {
                    StreamStatusKeyboardButton(onClick = onKeyboardOpen)
                }
            }
        }
    }
}

@Composable
private fun StreamStatusKeyboardButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .size(28.dp)
            .streamTouchPassthrough(PASSTHROUGH_ID_STATUS_BAR_KEYBOARD, inflate = 8.dp)
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = Icons.Rounded.Keyboard,
            contentDescription = stringResource(R.string.stream_panel_cd_keyboard),
            tint = TextPrimary,
            modifier = Modifier.size(20.dp),
        )
    }
}

@Composable
internal fun StreamNetworkQualityNotice(
    warning: StreamNetworkWarning,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier
            .padding(horizontal = 8.dp)
            .widthIn(max = 520.dp)
            .semantics { contentDescription = warning.message },
        shape = RoundedCornerShape(OpenNowRadius.md),
        color = Color(0xff4a2f0b).copy(alpha = 0.92f),
        border = BorderStroke(1.dp, OpenNowPalette.StatusNotice.copy(alpha = 0.62f)),
        tonalElevation = 0.dp,
    ) {
        Text(
            text = warning.message,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            color = Color(0xffffd38a),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.SemiBold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
internal fun ActiveStreamModePill(
    status: ActiveStreamModeStatus,
    recoveryReason: String?,
    bugReportSubmission: BugReportSubmissionState,
    bugReportVersionCheck: AndroidBugReportVersionCheckState,
    update: AndroidUpdateState,
    onBugReportSubmit: (String, String) -> Unit,
    onBugReportReset: () -> Unit,
    onBugReportVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val appLocale = currentAndroidAppLocale(context)
    // Only the changes that still raise a notice. Resolution and colour negotiation is recorded
    // silently — see activeStreamModeNoticeChanges.
    val changes = remember(status) { activeStreamModeNoticeChanges(status) }
    if (changes.isEmpty()) return
    val causeAssessment = remember(status, recoveryReason) {
        activeStreamModeCauseAssessment(status, recoveryReason)
    }
    val developerReport = remember(status, recoveryReason) {
        activeStreamModeDeveloperReport(status, recoveryReason)
    }
    val headline = changes.first().let { "${it.label} ${it.requestedValue} → ${it.actualValue}" }
    val noticeKey = remember(changes, recoveryReason) {
        changes.joinToString("|") { "${it.label}:${it.requestedValue}:${it.actualValue}" } +
            "|${recoveryReason.orEmpty()}"
    }
    var noticeVisible by remember(noticeKey) { mutableStateOf(true) }
    var detailsOpen by remember(noticeKey) { mutableStateOf(false) }
    var reportConfirmationOpen by remember(noticeKey) { mutableStateOf(false) }

    LaunchedEffect(detailsOpen, update.installSource.isGooglePlay) {
        if (detailsOpen && appLocale.bugReportsAllowed && update.installSource.isGooglePlay) {
            onBugReportVersionCheck()
        }
    }

    LaunchedEffect(noticeKey) {
        noticeVisible = true
        delay(ACTIVE_STREAM_MODE_NOTICE_DURATION_MS)
        noticeVisible = false
    }

    AnimatedVisibility(
        visible = noticeVisible,
        modifier = modifier.padding(horizontal = 8.dp),
        enter = fadeIn(),
        exit = fadeOut(),
    ) {
        Surface(
            modifier = Modifier
                .semantics { contentDescription = "$headline. Tap for details." }
                .clickable {
                    if (!bugReportSubmission.uploading) onBugReportReset()
                    detailsOpen = true
                }
                .focusable(),
            shape = RoundedCornerShape(999.dp),
            color = Color(0xff4a2f0b).copy(alpha = 0.88f),
            tonalElevation = 0.dp,
        ) {
            Text(
                text = headline,
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                color = Color(0xffffd38a),
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }

    if (detailsOpen) {
        AlertDialog(
            onDismissRequest = {
                if (!bugReportSubmission.uploading) detailsOpen = false
            },
            title = { Text(stringResource(R.string.stream_profile_changed_title)) },
            text = {
                Column(
                    modifier = Modifier
                        .heightIn(max = 560.dp)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Surface(
                        modifier = Modifier.fillMaxWidth(),
                        shape = RoundedCornerShape(12.dp),
                        color = OpenNowPalette.StatusNotice.copy(alpha = 0.10f),
                        contentColor = TextPrimary,
                        border = BorderStroke(1.dp, OpenNowPalette.StatusNotice.copy(alpha = 0.32f)),
                    ) {
                        Column(
                            modifier = Modifier.padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            Text(
                                text = stringResource(R.string.stream_profile_changed_why),
                                color = OpenNowPalette.StatusNotice,
                                style = MaterialTheme.typography.labelLarge,
                                fontWeight = FontWeight.Bold,
                            )
                            Text(
                                text = causeAssessment.summary,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        changes.forEach { change ->
                            Column {
                                Text(
                                    text = change.label,
                                    color = TextMuted,
                                    style = MaterialTheme.typography.labelMedium,
                                )
                                Text(
                                    text = "${change.requestedValue} → ${change.actualValue}",
                                    color = TextPrimary,
                                    style = MaterialTheme.typography.bodyMedium,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }
                    }
                    when {
                        bugReportSubmission.uploading -> Text(
                            text = stringResource(R.string.stream_profile_sending),
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        bugReportSubmission.submitted -> Surface(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(10.dp),
                            color = Green.copy(alpha = 0.12f),
                            contentColor = Green,
                        ) {
                            Column(
                                modifier = Modifier.padding(10.dp),
                                verticalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Text(
                                    text = "Sent to developer",
                                    style = MaterialTheme.typography.bodySmall,
                                    fontWeight = FontWeight.Bold,
                                )
                                bugReportSubmission.reference?.let { reportId ->
                                    CopyableBugReportId(reportId)
                                }
                            }
                        }
                        bugReportSubmission.error != null -> Surface(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(10.dp),
                            color = MaterialTheme.colorScheme.error.copy(alpha = 0.12f),
                            contentColor = MaterialTheme.colorScheme.error,
                        ) {
                            Text(
                                text = bugReportSubmission.error,
                                modifier = Modifier.padding(10.dp),
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                    Text(
                        text = stringResource(R.string.stream_profile_settings_unchanged),
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    if (!appLocale.bugReportsAllowed) {
                        BugReportLocaleGateCard()
                    } else if (!androidBugReportsAllowed(update, bugReportVersionCheck)) {
                        BugReportVersionGateCard(
                            update = update,
                            versionCheck = bugReportVersionCheck,
                            onRetry = onBugReportVersionCheck,
                            onOpenUpdate = onOpenUpdate,
                        )
                    }
                }
            },
            confirmButton = {
                when {
                    bugReportSubmission.uploading -> Button(
                        enabled = false,
                        onClick = {},
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(18.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.bug_report_sending))
                    }
                    bugReportSubmission.submitted -> TextButton(onClick = { detailsOpen = false }) {
                        Text(stringResource(R.string.stream_panel_done))
                    }
                    !appLocale.bugReportsAllowed -> Button(
                        onClick = { setAndroidAppLanguage(context, ANDROID_APP_LANGUAGE_ENGLISH) },
                    ) {
                        Text(stringResource(R.string.bug_report_use_english))
                    }
                    !androidBugReportsAllowed(update, bugReportVersionCheck) -> when {
                        update.status == AndroidUpdateStatus.Available ||
                            bugReportVersionCheck.status == AndroidBugReportVersionCheckStatus.UpdateRequired ->
                            Button(onClick = onOpenUpdate) {
                                Text(stringResource(R.string.bug_report_update_play))
                            }
                        bugReportVersionCheck.status == AndroidBugReportVersionCheckStatus.Checking -> Button(
                            enabled = false,
                            onClick = {},
                        ) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onPrimary,
                            )
                            Spacer(Modifier.width(8.dp))
                            Text(stringResource(R.string.bug_report_checking_play))
                        }
                        else -> Button(onClick = onBugReportVersionCheck) {
                            Text(stringResource(R.string.bug_report_retry_version))
                        }
                    }
                    else -> Button(
                        onClick = {
                            onBugReportReset()
                            detailsOpen = false
                            reportConfirmationOpen = true
                        },
                    ) {
                        Text(if (bugReportSubmission.error == null) "Send to developer" else "Try again")
                    }
                }
            },
            dismissButton = {
                if (!bugReportSubmission.uploading && !bugReportSubmission.submitted) {
                    TextButton(onClick = { detailsOpen = false }) {
                        Text(stringResource(R.string.action_close))
                    }
                }
            },
        )
    }

    if (reportConfirmationOpen) {
        AlertDialog(
            onDismissRequest = {
                reportConfirmationOpen = false
                detailsOpen = true
            },
            title = { Text(stringResource(R.string.stream_diag_confirm_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        stringResource(R.string.stream_diag_confirm_body),
                    )
                    BugReportDataDisclosure(
                        includeTypedTextWarning = false,
                    )
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        reportConfirmationOpen = false
                        onBugReportSubmit(developerReport.title, developerReport.description)
                        detailsOpen = true
                    },
                ) {
                    Text(stringResource(R.string.stream_diag_send))
                }
            },
            dismissButton = {
                TextButton(
                    onClick = {
                        reportConfirmationOpen = false
                        detailsOpen = true
                    },
                ) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }
}

internal enum class ActiveStreamModeChangeKind {
    Resolution,
    Codec,
    Fps,
    Bitrate,
    Hdr,
    Color,
    L4S,
    Sharpening,
}

internal data class ActiveStreamModeDisplayChange(
    val label: String,
    val requestedValue: String,
    val actualValue: String,
    val kind: ActiveStreamModeChangeKind,
)

/**
 * Changes the player is worth interrupting for.
 *
 * Resolution and colour-depth differences are routine session negotiation — the cloud allocates a
 * mode, the decoder reports another — and every one of them was raising an in-stream notice whose
 * only action is uploading a diagnostic report. They stay in the session report and the debug log,
 * where they are useful, and no longer prompt.
 */
private val SILENT_ACTIVE_STREAM_MODE_CHANGES = setOf(
    ActiveStreamModeChangeKind.Resolution,
    ActiveStreamModeChangeKind.Color,
)

internal fun activeStreamModeNoticeChanges(status: ActiveStreamModeStatus): List<ActiveStreamModeDisplayChange> =
    activeStreamModeDisplayChanges(status).filterNot { it.kind in SILENT_ACTIVE_STREAM_MODE_CHANGES }

internal fun activeStreamModeDisplayChanges(status: ActiveStreamModeStatus): List<ActiveStreamModeDisplayChange> {
    val requested = status.requestedProfile
    val actual = status.transportProfile
    return buildList {
        if (status.requestedResolution != status.displayedResolution) {
            add(
                ActiveStreamModeDisplayChange(
                    label = "Resolution",
                    requestedValue = formatRuntimeResolution(status.requestedResolution),
                    actualValue = formatRuntimeResolution(status.displayedResolution),
                    kind = ActiveStreamModeChangeKind.Resolution,
                ),
            )
        }
        if (requested.codec != actual.codec) {
            add(
                ActiveStreamModeDisplayChange(
                    "Codec",
                    requested.codec.name,
                    actual.codec.name,
                    ActiveStreamModeChangeKind.Codec,
                ),
            )
        }
        if (requested.fps != actual.fps) {
            add(
                ActiveStreamModeDisplayChange(
                    "FPS",
                    requested.fps.toString(),
                    actual.fps.toString(),
                    ActiveStreamModeChangeKind.Fps,
                ),
            )
        }
        if (requested.maxBitrateMbps != actual.maxBitrateMbps) {
            add(
                ActiveStreamModeDisplayChange(
                    "Bitrate",
                    "${requested.maxBitrateMbps} Mbps",
                    "${actual.maxBitrateMbps} Mbps",
                    ActiveStreamModeChangeKind.Bitrate,
                ),
            )
        }
        if (requested.hdrEnabled != actual.hdrEnabled) {
            add(
                ActiveStreamModeDisplayChange(
                    "HDR",
                    requested.hdrEnabled.onOffLabel(),
                    actual.hdrEnabled.onOffLabel(),
                    ActiveStreamModeChangeKind.Hdr,
                ),
            )
        }
        if (requested.colorQuality != actual.colorQuality) {
            add(
                ActiveStreamModeDisplayChange(
                    "Color",
                    requested.colorQuality.label,
                    actual.colorQuality.label,
                    ActiveStreamModeChangeKind.Color,
                ),
            )
        }
        if (requested.enableL4S != actual.enableL4S) {
            add(
                ActiveStreamModeDisplayChange(
                    "L4S",
                    requested.enableL4S.onOffLabel(),
                    actual.enableL4S.onOffLabel(),
                    ActiveStreamModeChangeKind.L4S,
                ),
            )
        }
        if (requested.streamSharpeningEnabled != actual.streamSharpeningEnabled) {
            add(
                ActiveStreamModeDisplayChange(
                    "Sharpening",
                    requested.streamSharpeningEnabled.onOffLabel(),
                    actual.streamSharpeningEnabled.onOffLabel(),
                    ActiveStreamModeChangeKind.Sharpening,
                ),
            )
        }
    }
}

internal fun Boolean.onOffLabel(): String = if (this) "On" else "Off"

internal data class ActiveStreamModeCauseAssessment(
    val summary: String,
)

internal fun activeStreamModeCauseAssessment(
    status: ActiveStreamModeStatus,
    recoveryReason: String?,
): ActiveStreamModeCauseAssessment {
    val requestedCodec = status.requestedProfile.codec.name
    val actualCodec = status.transportProfile.codec.name
    val primaryChange = activeStreamModeDisplayChanges(status).firstOrNull()
    val saferProfileSummary = primaryChange?.let {
        "a safer profile (${it.label} ${it.requestedValue} to ${it.actualValue})"
    } ?: "a safer live profile"
    val recordedReason = recoveryReason?.trim()?.takeIf(String::isNotEmpty)
    val lowerReason = recordedReason?.lowercase(Locale.US).orEmpty()
    val summary = when {
        "did not negotiate" in lowerReason ->
            "WebRTC could not negotiate the requested $requestedCodec codec for this connection, so OpenNOW retried the local video transport with $actualCodec."
        "video offer" in lowerReason ->
            "The session did not provide a video offer before the startup timeout, so OpenNOW retried the local video transport with $actualCodec."
        "no frame rendered" in lowerReason || "first video frame" in lowerReason ->
            "Video data arrived, but the device did not render a frame before the recovery timeout. OpenNOW applied $saferProfileSummary to restore video."
        "decoder stalled" in lowerReason || "media stall" in lowerReason ->
            "The device decoder stopped producing video frames during startup. OpenNOW applied $saferProfileSummary while keeping the same cloud session."
        "decoded at" in lowerReason ->
            "The decoder produced an unexpected output size for the requested stream mode, so OpenNOW tried the $actualCodec transport profile. Recorded detail: $recordedReason"
        status.resolutionSource == StreamResolutionChangeSource.ServerNegotiatedFallback ->
            "The cloud server selected ${status.displayedResolution} instead of the requested ${status.requestedResolution}. This was a server/session negotiation decision, not a change to your saved setting."
        status.resolutionSource == StreamResolutionChangeSource.ProviderOrGameModeChange ->
            "The decoded stream changed to ${status.displayedResolution} after startup without matching the server's initial mode. This points to a game or cloud-provider output-mode change."
        recordedReason != null ->
            "OpenNOW recorded this recovery reason: $recordedReason"
        status.safeVideoRecoveryActive ->
            "The original video transport stopped progressing, so OpenNOW adjusted the local profile to keep video playing without ending the cloud session."
        else ->
            "The live stream profile no longer matched the requested profile."
    }
    return ActiveStreamModeCauseAssessment(summary)
}

internal data class ActiveStreamModeDeveloperReport(
    val title: String,
    val description: String,
)

internal fun activeStreamModeDeveloperReport(
    status: ActiveStreamModeStatus,
    recoveryReason: String?,
): ActiveStreamModeDeveloperReport {
    val changes = activeStreamModeDisplayChanges(status)
    val primary = activeStreamModeNoticeChanges(status).firstOrNull() ?: changes.first()
    val cause = activeStreamModeCauseAssessment(status, recoveryReason)
    return ActiveStreamModeDeveloperReport(
        title = "Automatic stream change: ${primary.label} ${primary.requestedValue} to ${primary.actualValue}",
        description = buildString {
            appendLine("OpenNOW detected an automatic stream profile change while the session was active.")
            appendLine()
            appendLine("Cause assessment:")
            appendLine(cause.summary)
            appendLine()
            appendLine("Requested to actual changes:")
            changes.forEach { change ->
                appendLine("- ${change.label}: ${change.requestedValue} -> ${change.actualValue}")
            }
            recoveryReason?.trim()?.takeIf(String::isNotEmpty)?.let { reason ->
                appendLine()
                appendLine("Recorded recovery event:")
                appendLine(reason)
            }
            appendLine()
            append("Sent from the in-stream profile-change notice. The user's saved stream settings were not changed.")
        },
    )
}

@Composable
private fun StreamStatsMetricItems(
    streamStats: StreamRuntimeStats,
    streamSettings: StreamSettings,
    metrics: StreamStatsMetrics,
    deviceStatus: CompactStreamDeviceStatus,
    serverLocation: String?,
    /** Applied to every item; the expanded layout passes a weight so its two columns line up. */
    itemModifier: Modifier = Modifier,
) {
    // The target is what the user asked for; streamStats.fps is what is actually arriving.
    val targetFps = streamSettings.fps
    if (metrics.fps) {
        val fps = streamStats.fps
        StreamStatsText(
            value = "FPS ${fps ?: targetFps}",
            modifier = itemModifier,
            quality = fps?.let { StreamQuality.frameRate(it.toDouble(), targetFps) },
            contentDescription = stringResource(R.string.stream_stats_cd_fps, fps ?: targetFps),
        )
    }
    if (metrics.ping) {
        val ping = streamStats.pingMs
        StreamStatsText(
            value = stringResource(R.string.stream_stats_ping, ping?.let { "${it}ms" } ?: NO_STAT_VALUE),
            modifier = itemModifier,
            quality = ping?.let(StreamQuality::latency),
            contentDescription = ping?.let { stringResource(R.string.stream_stats_cd_ping, it) },
        )
    }
    if (metrics.latency) {
        streamStats.decodeMs?.let { decode ->
            StreamStatsText(
                value = stringResource(R.string.stream_stats_decode, "%.1f".format(Locale.US, decode)),
                modifier = itemModifier,
                quality = StreamQuality.decode(decode, targetFps, streamStats.fps?.toDouble()),
                contentDescription = stringResource(R.string.stream_stats_cd_decode, "%.1f".format(Locale.US, decode)),
            )
        }
        streamStats.jitterMs?.let { jitter ->
            StreamStatsText(
                value = stringResource(R.string.stream_stats_jitter, "%.1f".format(Locale.US, jitter)),
                modifier = itemModifier,
                quality = StreamQuality.jitter(jitter),
                contentDescription = stringResource(R.string.stream_stats_cd_jitter, "%.1f".format(Locale.US, jitter)),
            )
        }
    }
    if (metrics.packetLoss) {
        streamStats.packetLossPct?.let { loss ->
            // %.2f, matching the session report — %.1f hid the 0.5% boundary the ladder cares about.
            val formatted = "%.2f".format(Locale.US, loss)
            StreamStatsText(
                value = stringResource(R.string.stream_stats_loss, formatted),
                modifier = itemModifier,
                quality = StreamQuality.packetLoss(loss),
                contentDescription = stringResource(R.string.stream_stats_cd_loss, formatted),
            )
        }
    }
    if (metrics.bitrate) {
        StreamStatsText(
            formatRuntimeBitrateStatus(
                actualBitrateKbps = streamStats.bitrateKbps,
                requestedMaxBitrateMbps = streamSettings.maxBitrateMbps,
            ),
            modifier = itemModifier,
        )
    }
    if (metrics.battery) {
        StreamBatteryIndicator(deviceStatus, itemModifier)
    }
    if (metrics.connection) {
        StreamNetworkIndicator(deviceStatus, itemModifier)
    }
    if (metrics.resolution) {
        StreamStatsText(
            streamStats.resolution?.let(::formatRuntimeResolution)
                ?: formatRuntimeResolution(normalizeStreamResolutionForAspect(streamSettings.resolution, streamSettings.aspectRatio)),
            modifier = itemModifier,
        )
    }
    if (metrics.codec) {
        StreamStatsText(streamStats.codec?.takeIf { it.isNotBlank() } ?: streamSettings.codec.name, modifier = itemModifier)
    }
    if (metrics.location && !serverLocation.isNullOrBlank()) {
        val displayName = serverLocation.removePrefix("NPA-").removePrefix("NP-").uppercase()
        StreamStatsText(displayName, modifier = itemModifier)
    }
}

/** Shown in place of a metric that has not been measured yet. */
private const val NO_STAT_VALUE = "--"

@Composable
private fun StreamStatsText(
    value: String,
    modifier: Modifier = Modifier,
    quality: StreamQualityLevel? = null,
    contentDescription: String? = null,
) {
    // Colour alone used to carry the warning, which says nothing to a colour-blind user or to
    // TalkBack. The quality level is spelled out in the description instead.
    val qualityLabel = quality?.let { stringResource(it.labelRes()) }
    val describedAs = contentDescription?.let { base ->
        if (qualityLabel != null) "$base, $qualityLabel" else base
    }
    Text(
        value,
        modifier = if (describedAs != null) {
            modifier.semantics { this.contentDescription = describedAs }
        } else {
            modifier
        },
        color = quality?.tint() ?: TextPrimary,
        // Tabular figures: without these every value is a different width each tick, so the whole
        // row reflows roughly once a second.
        style = MaterialTheme.typography.labelSmall.numeric(),
        fontWeight = FontWeight.SemiBold,
        maxLines = 1,
    )
}

@StringRes
internal fun StreamQualityLevel.labelRes(): Int = when (this) {
    StreamQualityLevel.Good -> R.string.stream_quality_good
    StreamQualityLevel.Fair -> R.string.stream_quality_fair
    StreamQualityLevel.Poor -> R.string.stream_quality_poor
}

private data class CompactStreamDeviceStatus(
    val batteryPercent: Int? = null,
    val batteryCharging: Boolean = false,
    val networkKind: AndroidNetworkKind = AndroidNetworkKind.Unknown,
    val networkBars: Int? = null,
    val cellularGeneration: String? = null,
)

@Composable
private fun rememberCompactStreamDeviceStatus(): CompactStreamDeviceStatus {
    val context = LocalContext.current
    val appContext = remember(context) { context.applicationContext }
    var status by remember(appContext) { mutableStateOf(CompactStreamDeviceStatus()) }
    LaunchedEffect(appContext) {
        while (true) {
            status = withContext(Dispatchers.IO) { readCompactStreamDeviceStatus(appContext) }
            delay(COMPACT_STREAM_DEVICE_STATUS_REFRESH_MS)
        }
    }
    return status
}

private fun readCompactStreamDeviceStatus(context: Context): CompactStreamDeviceStatus {
    val diagnostics = AndroidRuntimeDiagnostics.snapshot(context)
    return CompactStreamDeviceStatus(
        batteryPercent = diagnostics.batteryPercent,
        batteryCharging = diagnostics.batteryCharging,
        networkKind = diagnostics.networkKind,
        networkBars = diagnostics.networkSignalBars,
        cellularGeneration = diagnostics.cellularGeneration,
    )
}

@Composable
private fun StreamBatteryIndicator(status: CompactStreamDeviceStatus, modifier: Modifier = Modifier) {
    val description = status.batteryPercent?.let { percent ->
        "Battery $percent percent${if (status.batteryCharging) ", charging" else ""}"
    } ?: "Battery unknown"
    val level = streamBatteryLevel(status.batteryPercent)
    val batteryIcon = when (level) {
        StreamBatteryLevel.Unknown -> Icons.AutoMirrored.Rounded.BatteryUnknown
        StreamBatteryLevel.Empty -> Icons.Rounded.Battery0Bar
        StreamBatteryLevel.One -> Icons.Rounded.Battery1Bar
        StreamBatteryLevel.Two -> Icons.Rounded.Battery2Bar
        StreamBatteryLevel.Three -> Icons.Rounded.Battery3Bar
        StreamBatteryLevel.Four -> Icons.Rounded.Battery4Bar
        StreamBatteryLevel.Five -> Icons.Rounded.Battery5Bar
        StreamBatteryLevel.Six -> Icons.Rounded.Battery6Bar
        StreamBatteryLevel.Full -> Icons.Rounded.BatteryFull
    }
    val batteryTint = when {
        status.batteryCharging -> Green
        status.batteryPercent != null && status.batteryPercent <= 20 -> MaterialTheme.colorScheme.error
        else -> TextPrimary
    }
    Row(
        modifier = modifier.semantics { contentDescription = description },
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(18.dp)) {
            Icon(
                imageVector = batteryIcon,
                contentDescription = null,
                tint = batteryTint,
                modifier = Modifier.matchParentSize().graphicsLayer { rotationZ = 90f },
            )
            if (status.batteryCharging) {
                Icon(
                    imageVector = Icons.Rounded.Bolt,
                    contentDescription = null,
                    tint = batteryTint,
                    modifier = Modifier.align(Alignment.Center).size(10.dp),
                )
            }
        }
        Text(
            status.batteryPercent?.let { "$it%" } ?: "--%",
            color = batteryTint,
            style = MaterialTheme.typography.labelSmall,
            maxLines = 1,
        )
    }
}

@Composable
private fun StreamNetworkIndicator(status: CompactStreamDeviceStatus, modifier: Modifier = Modifier) {
    val bars = status.networkBars?.coerceIn(0, 4)
    val label = when (status.networkKind) {
        AndroidNetworkKind.Cellular -> status.cellularGeneration ?: status.networkKind.label
        AndroidNetworkKind.Ethernet,
        AndroidNetworkKind.Other,
        AndroidNetworkKind.None,
        AndroidNetworkKind.Unknown,
        -> status.networkKind.label
        AndroidNetworkKind.Wifi -> null
    }
    val description = "${label ?: status.networkKind.label} signal ${bars?.toString() ?: "unknown"} bars"
    Row(
        modifier = modifier.semantics { contentDescription = description },
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (label != null) {
            Text(
                label,
                color = TextPrimary,
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
        }
        if (status.networkKind == AndroidNetworkKind.Wifi) {
            Icon(
                imageVector = when (bars) {
                    4 -> Icons.Rounded.Wifi
                    3 -> Icons.Rounded.Wifi
                    2 -> Icons.Rounded.Wifi2Bar
                    1 -> Icons.Rounded.Wifi1Bar
                    0 -> Icons.Rounded.SignalWifi0Bar
                    else -> Icons.Rounded.WifiOff
                },
                contentDescription = null,
                tint = TextPrimary,
                modifier = Modifier.size(20.dp),
            )
        } else if (status.networkKind == AndroidNetworkKind.Cellular || status.networkKind == AndroidNetworkKind.Other || status.networkKind == AndroidNetworkKind.Unknown) {
            Icon(
                imageVector = when (bars) {
                    4 -> Icons.Rounded.SignalCellular4Bar
                    3 -> Icons.Rounded.SignalCellularAlt
                    2 -> Icons.Rounded.SignalCellularAlt2Bar
                    1 -> Icons.Rounded.SignalCellularAlt1Bar
                    else -> Icons.Rounded.SignalCellular0Bar
                },
                contentDescription = null,
                tint = TextPrimary,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

internal fun formatRuntimeResolution(resolution: String): String {
    val parts = resolution.lowercase(Locale.US).split("x", limit = 2)
    return if (parts.size == 2 && parts.all { it.trim().isNotBlank() }) {
        "${parts[0].trim()}x${parts[1].trim()}"
    } else {
        resolution
    }
}

internal fun formatRuntimeBitrate(bitrateKbps: Int?): String {
    val kbps = bitrateKbps ?: return "--"
    return if (kbps >= 1000) {
        "${(kbps / 1000.0).let { kotlin.math.round(it * 10.0) / 10.0 }} Mbps"
    } else {
        "$kbps Kbps"
    }
}

internal fun formatRuntimeBitrateStatus(
    actualBitrateKbps: Int?,
    requestedMaxBitrateMbps: Int,
): String = "${formatRuntimeBitrate(actualBitrateKbps)} / ${requestedMaxBitrateMbps.coerceAtLeast(1)} Mbps max"

internal fun shouldHideStreamStatusText(status: String): Boolean =
    status.trim().replace('_', ' ').let {
        it.equals("Streaming", ignoreCase = true) ||
            it.equals("ICE CONNECTED", ignoreCase = true) ||
            it.equals("ICE COMPLETED", ignoreCase = true)
    }

internal data class InitialStreamConnectionStatus(
    val phase: String,
    val title: String,
    val detail: String,
)

internal fun initialStreamConnectionStatus(nativeState: String): InitialStreamConnectionStatus {
    val normalized = nativeState.trim().replace('_', ' ')
    return when {
        normalized.equals("Preparing", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Preparing",
            title = "Preparing your stream",
            detail = "Getting the secure video connection ready.",
        )
        normalized.startsWith("Connecting signaling", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Connecting",
            title = "Connecting to your game",
            detail = "Opening a secure connection to the streaming server.",
        )
        normalized.startsWith("Waiting for offer", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Waiting for video",
            title = "Starting the video stream",
            detail = "The server is preparing the first video frame.",
        )
        normalized.equals("ICE CHECKING", ignoreCase = true) ||
            normalized.equals("ICE NEW", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Securing connection",
            title = "Almost ready",
            detail = "Checking the best route for the live video stream.",
        )
        normalized.equals("ICE DISCONNECTED", ignoreCase = true) ||
            normalized.equals("ICE FAILED", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Retrying",
            title = "Connection interrupted",
            detail = "OpenNOW is retrying the initial stream connection.",
        )
        normalized.startsWith("Recovering video", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Recovering video",
            title = "Waiting for a clear frame",
            detail = "Requesting a fresh video frame before showing the stream.",
        )
        normalized.contains("safe H264 profile", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Optimizing video",
            title = "Trying a compatible video mode",
            detail = "Restarting the initial video connection with safer settings.",
        )
        normalized.startsWith("Reconnecting", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Retrying connection",
            title = "Connecting again",
            detail = "The initial connection did not finish, so OpenNOW is retrying it.",
        )
        normalized.startsWith("Recovering cloud session", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Checking session",
            title = "Restoring your game session",
            detail = "Checking the existing cloud session before continuing.",
        )
        normalized.equals("Streaming", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Starting video",
            title = "Connection established",
            detail = "Waiting for the first video frame to appear.",
        )
        else -> InitialStreamConnectionStatus(
            phase = "Starting stream",
            title = "Preparing your game",
            detail = "OpenNOW is waiting for the live video to begin.",
        )
    }
}

@Composable
internal fun InitialStreamConnectionOverlay(
    gameTitle: String?,
    status: InitialStreamConnectionStatus,
    modifier: Modifier = Modifier,
) {
    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.18f))
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        val cardWidthFraction = if (maxWidth > maxHeight) 0.54f else 0.9f
        Surface(
            modifier = Modifier
                .fillMaxWidth(cardWidthFraction)
                .widthIn(max = 560.dp),
            shape = RoundedCornerShape(22.dp),
            color = Panel.copy(alpha = 0.96f),
            contentColor = TextPrimary,
            tonalElevation = 10.dp,
        ) {
            Row(
                Modifier.padding(horizontal = 22.dp, vertical = 20.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(18.dp),
            ) {
                CircularProgressIndicator(
                    modifier = Modifier
                        .size(38.dp)
                        .semantics { contentDescription = status.phase },
                    strokeWidth = 3.dp,
                    color = MaterialTheme.colorScheme.primary,
                )
                Column(
                    Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Text(
                        gameTitle?.takeIf { it.isNotBlank() } ?: "OpenNOW stream",
                        color = MaterialTheme.colorScheme.primary,
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.Bold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        status.title,
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        status.detail,
                        color = TextMuted,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Text(
                        status.phase,
                        color = TextMuted.copy(alpha = 0.78f),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
            }
        }
    }
}

@Composable
internal fun StreamExitConfirmation(
    gameTitle: String,
    onKeepPlaying: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val keepPlayingFocusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { keepPlayingFocusRequester.requestFocus() }
    }
    val scrimInteraction = remember { MutableInteractionSource() }
    Box(
        Modifier
            .fillMaxSize()
            // The scrim covers everything, so it reports the full screen — otherwise a mis-tap on
            // "Exit Stream" also lands in the game underneath.
            .streamTouchPassthrough(PASSTHROUGH_ID_EXIT, inflate = 0.dp)
            .background(OpenNowPalette.StreamScrim)
            // indication = null: a full-screen ripple is wrong, and without its own interaction
            // source the scrim competes with the two buttons for D-pad focus.
            .clickable(
                interactionSource = scrimInteraction,
                indication = null,
                onClick = onKeepPlaying,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            modifier = modifier
                .padding(OpenNowSpacing.xl)
                .fillMaxWidth()
                // Unbounded fillMaxWidth made this enormous on a tablet or TV.
                .widthIn(max = 440.dp),
            // Same radius as the controls panel, so the two overlays read as one family.
            shape = RoundedCornerShape(OpenNowRadius.lg + 2.dp),
            color = OpenNowPalette.PanelOverVideo,
            contentColor = TextPrimary,
            border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
            tonalElevation = 8.dp,
        ) {
            Column(
                Modifier.padding(OpenNowSpacing.lg + 2.dp),
                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
            ) {
                Text(
                    stringResource(R.string.stream_exit_eyebrow),
                    color = TextMuted,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                )
                Text(stringResource(R.string.stream_exit_title), style = MaterialTheme.typography.titleLarge)
                Text(stringResource(R.string.stream_exit_body, gameTitle), color = TextMuted)
                Text(
                    stringResource(R.string.stream_exit_caveat),
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
                Row(
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    OutlinedButton(
                        onClick = onKeepPlaying,
                        modifier = Modifier
                            .weight(1f)
                            .focusRequester(keepPlayingFocusRequester),
                    ) { Text(stringResource(R.string.stream_exit_keep_playing), maxLines = 1) }
                    Button(onClick = onExit, modifier = Modifier.weight(1f)) {
                        Text(stringResource(R.string.stream_exit_confirm), maxLines = 1)
                    }
                }
            }
        }
    }
}
