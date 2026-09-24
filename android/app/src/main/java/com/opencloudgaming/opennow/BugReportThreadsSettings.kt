package com.opencloudgaming.opennow

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Attachment
import androidx.compose.material.icons.rounded.BugReport
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.Forum
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

/**
 * Reporter-owned support inbox.
 *
 * The parent Settings section already owns the "Your reports" heading, so this composable starts
 * with the summary and actions. Keeping that ownership clear avoids the former three-level title
 * stack (Bug reports -> Your reports -> Your reports) on a phone-sized viewport.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun BugReportThreadsSettings(
    state: AndroidBugReportThreadsState,
    onRefresh: () -> Unit,
    onNewReport: () -> Unit,
    onComment: (reportId: String, comment: String) -> Unit,
    onClose: (reportId: String) -> Unit,
    onDelete: (reportId: String) -> Unit,
) {
    var selectedReportId by rememberSaveable { mutableStateOf<String?>(null) }
    val selectedReport = state.reports.firstOrNull { it.id == selectedReportId }

    BackHandler(enabled = selectedReport != null) { selectedReportId = null }

    LaunchedEffect(Unit) {
        onRefresh()
        while (isActive) {
            delay(60_000)
            onRefresh()
        }
    }

    if (selectedReport != null) {
        BugReportThreadDetail(
            report = selectedReport,
            posting = state.postingReportId == selectedReport.id,
            changing = state.changingReportId == selectedReport.id,
            commentError = state.error.takeIf { state.errorReportId == selectedReport.id },
            actionError = state.actionError.takeIf { state.actionErrorReportId == selectedReport.id },
            onBack = { selectedReportId = null },
            onComment = { comment -> onComment(selectedReport.id, comment) },
            onClose = { onClose(selectedReport.id) },
            onDelete = { onDelete(selectedReport.id) },
        )
        return
    }

    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg)) {
        Text(
            stringResource(R.string.bug_report_inbox_body),
            color = SettingsTextMuted,
            style = MaterialTheme.typography.bodyMedium,
        )

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Button(
                onClick = onNewReport,
                modifier = Modifier.weight(1f).defaultMinSize(minHeight = 48.dp),
            ) {
                Icon(Icons.Rounded.Add, contentDescription = null, modifier = Modifier.size(19.dp))
                Text(
                    stringResource(R.string.bug_report_inbox_new),
                    modifier = Modifier.padding(start = OpenNowSpacing.sm),
                )
            }
            Surface(
                shape = CircleShape,
                color = SettingsPanelAlt,
                border = BorderStroke(1.dp, SettingsTextMuted.copy(alpha = 0.28f)),
            ) {
                IconButton(
                    onClick = onRefresh,
                    enabled = !state.loading && state.postingReportId == null && state.changingReportId == null,
                    modifier = Modifier.size(48.dp),
                ) {
                    if (state.loading) {
                        CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                    } else {
                        Icon(
                            Icons.Rounded.Refresh,
                            contentDescription = stringResource(R.string.bug_report_inbox_refresh),
                        )
                    }
                }
            }
        }

        if (state.error != null && state.errorReportId == null) {
            InboxErrorCard(message = state.error, onRetry = onRefresh)
        }

        when {
            state.loading && state.reports.isEmpty() -> InboxLoadingCard()
            state.reports.isEmpty() -> InboxEmptyCard()
            else -> state.reports.forEach { report ->
                key(report.id) {
                    BugReportThreadListItem(
                        report = report,
                        onClick = { selectedReportId = report.id },
                    )
                }
            }
        }
    }
}

@Composable
private fun InboxErrorCard(message: String, onRetry: () -> Unit) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.md),
        color = MaterialTheme.colorScheme.errorContainer,
        contentColor = MaterialTheme.colorScheme.onErrorContainer,
    ) {
        Row(
            modifier = Modifier.padding(OpenNowSpacing.md),
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(message, modifier = Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
            Button(onClick = onRetry) { Text(stringResource(R.string.action_retry)) }
        }
    }
}

@Composable
private fun InboxLoadingCard() {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = SettingsPanelAlt,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(vertical = OpenNowSpacing.xl),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CircularProgressIndicator(modifier = Modifier.size(22.dp), strokeWidth = 2.dp)
            Text(
                stringResource(R.string.bug_report_inbox_loading),
                modifier = Modifier.padding(start = OpenNowSpacing.md),
                color = SettingsTextMuted,
            )
        }
    }
}

@Composable
private fun InboxEmptyCard() {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = SettingsPanelAlt,
        border = BorderStroke(1.dp, SettingsTextMuted.copy(alpha = 0.18f)),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(OpenNowSpacing.xl),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
        ) {
            Surface(shape = CircleShape, color = MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)) {
                Icon(
                    Icons.Rounded.BugReport,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.padding(OpenNowSpacing.md).size(28.dp),
                )
            }
            Text(
                stringResource(R.string.bug_report_inbox_empty_title),
                color = SettingsText,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.semantics { heading() },
            )
            Text(
                stringResource(R.string.bug_report_inbox_empty),
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BugReportThreadListItem(
    report: AndroidBugReportThread,
    onClick: () -> Unit,
) {
    val status = bugReportStatusPresentation(report.status)
    val updatedLabel = remember(report.updatedAt) { formatBugReportTimestamp(report.updatedAt) }
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = SettingsPanel,
        border = BorderStroke(1.dp, SettingsTextMuted.copy(alpha = 0.16f)),
        onClick = onClick,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(OpenNowSpacing.md),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                verticalAlignment = Alignment.Top,
            ) {
                Text(
                    report.title,
                    modifier = Modifier.weight(1f),
                    color = SettingsText,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Icon(
                    Icons.Rounded.ChevronRight,
                    contentDescription = stringResource(R.string.bug_report_inbox_open_details),
                    tint = SettingsTextMuted,
                )
            }
            Text(
                "${bugReportKindLabel(report.kind)} • ${bugReportAreaLabel(report.area)}",
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.xs),
            ) {
                BugReportStatusPill(status)
                updatedLabel?.let {
                    Text(
                        stringResource(R.string.bug_report_inbox_updated, it),
                        modifier = Modifier.padding(top = OpenNowSpacing.xs),
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BugReportThreadDetail(
    report: AndroidBugReportThread,
    posting: Boolean,
    changing: Boolean,
    commentError: String?,
    actionError: String?,
    onBack: () -> Unit,
    onComment: (String) -> Unit,
    onClose: () -> Unit,
    onDelete: () -> Unit,
) {
    var reply by rememberSaveable(report.id) { mutableStateOf("") }
    var waitingForReplyResult by rememberSaveable(report.id) { mutableStateOf(false) }
    var confirmingAction by rememberSaveable(report.id) { mutableStateOf<String?>(null) }
    val status = bugReportStatusPresentation(report.status)
    val closed = androidBugReportThreadClosed(report.status)

    LaunchedEffect(posting, commentError) {
        if (waitingForReplyResult && !posting) {
            if (commentError == null) {
                reply = ""
            }
            waitingForReplyResult = false
        }
    }

    if (confirmingAction != null) {
        val deleting = confirmingAction == "delete"
        AlertDialog(
            onDismissRequest = { confirmingAction = null },
            title = { Text(stringResource(if (deleting) R.string.bug_report_inbox_delete_title else R.string.bug_report_inbox_close_title)) },
            text = { Text(stringResource(if (deleting) R.string.bug_report_inbox_delete_confirm else R.string.bug_report_inbox_close_confirm)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmingAction = null
                    if (deleting) onDelete() else onClose()
                }) { Text(stringResource(if (deleting) R.string.bug_report_inbox_delete else R.string.bug_report_inbox_close)) }
            },
            dismissButton = { TextButton(onClick = { confirmingAction = null }) { Text(stringResource(R.string.bug_report_inbox_cancel)) } },
        )
    }

    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
    ) {
        TextButton(onClick = onBack) {
            Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null)
            Text(
                stringResource(R.string.bug_report_inbox_back_to_list),
                modifier = Modifier.padding(start = OpenNowSpacing.sm),
            )
        }

        Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            BugReportStatusPill(status)
            Text(
                report.title,
                color = SettingsText,
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.SemiBold,
            )
            formatBugReportTimestamp(report.updatedAt)?.let {
                Text(
                    stringResource(R.string.bug_report_inbox_updated, it),
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }

        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        ) {
            if (!closed) {
                TextButton(onClick = { confirmingAction = "close" }, enabled = !posting && !changing) {
                    Text(stringResource(R.string.bug_report_inbox_close))
                }
            }
            TextButton(onClick = { confirmingAction = "delete" }, enabled = !posting && !changing) {
                Text(stringResource(R.string.bug_report_inbox_delete))
            }
            if (changing) CircularProgressIndicator(modifier = Modifier.size(22.dp), strokeWidth = 2.dp)
        }
        actionError?.let {
            Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }

        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        ) {
            ReportDetailChip(bugReportKindLabel(report.kind))
            ReportDetailChip(bugReportAreaLabel(report.area))
            ReportDetailChip(bugReportFrequencyLabel(report.frequency))
            ReportDetailChip(bugReportImpactLabel(report.impact))
            if (report.versionName.isNotBlank() || report.versionCode.isNotBlank()) {
                ReportDetailChip(
                    stringResource(
                        R.string.bug_report_inbox_version,
                        report.versionName.ifBlank { "?" },
                        report.versionCode.ifBlank { "?" },
                    ),
                )
            }
        }

        ThreadMessage(
            heading = stringResource(R.string.bug_report_inbox_original_report),
            body = report.description,
            own = true,
            createdAt = report.createdAt,
        )

        if (report.files.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                Text(
                    stringResource(R.string.bug_report_inbox_attachments),
                    color = SettingsText,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                    verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                ) {
                    report.files.forEach { AttachmentChip(it) }
                }
            }
        }

        if (report.resolutionNote.isNotBlank()) {
            ThreadMessage(
                heading = stringResource(R.string.bug_report_inbox_status_update),
                body = report.resolutionNote,
                own = false,
            )
        }

        HorizontalDivider(color = SettingsTextMuted.copy(alpha = 0.14f))
        Text(
            stringResource(R.string.bug_report_inbox_conversation),
            color = SettingsText,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.semantics { heading() },
        )
        if (report.comments.isEmpty()) {
            Text(
                stringResource(R.string.bug_report_inbox_no_replies),
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        } else {
            report.comments.forEach { comment ->
                key(comment.id) {
                    ThreadMessage(
                        heading = when {
                            comment.kind == "status" -> stringResource(R.string.bug_report_inbox_status_update)
                            comment.authorRole == "reporter" -> stringResource(R.string.bug_report_inbox_you)
                            else -> stringResource(R.string.bug_report_inbox_release_team)
                        },
                        body = comment.body,
                        own = comment.authorRole == "reporter",
                        createdAt = comment.createdAt,
                    )
                }
            }
        }

        if (closed) {
            ClosedConversationCard()
        } else {
            ReplyComposer(
                reply = reply,
                posting = posting || changing,
                error = commentError,
                onReplyChange = { if (it.length <= 3_000) reply = it },
                onSend = {
                    waitingForReplyResult = true
                    onComment(reply)
                },
            )
        }
    }
}

@Composable
private fun ReportDetailChip(label: String) {
    Surface(shape = RoundedCornerShape(OpenNowRadius.full), color = SettingsPanelAlt) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            color = SettingsTextMuted,
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

@Composable
private fun AttachmentChip(fileName: String) {
    Surface(
        shape = RoundedCornerShape(OpenNowRadius.full),
        color = MaterialTheme.colorScheme.surfaceVariant,
        border = BorderStroke(1.dp, SettingsTextMuted.copy(alpha = 0.18f)),
    ) {
        Row(
            modifier = Modifier.widthIn(max = 240.dp).padding(horizontal = 10.dp, vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(Icons.Rounded.Attachment, contentDescription = null, modifier = Modifier.size(15.dp), tint = SettingsTextMuted)
            Text(
                fileName,
                color = SettingsTextMuted,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun ClosedConversationCard() {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.md),
        color = SettingsPanelAlt,
    ) {
        Row(
            modifier = Modifier.padding(OpenNowSpacing.md),
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            verticalAlignment = Alignment.Top,
        ) {
            Icon(Icons.Rounded.Lock, contentDescription = null, tint = SettingsTextMuted, modifier = Modifier.size(19.dp))
            Text(
                stringResource(R.string.bug_report_inbox_closed),
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun ReplyComposer(
    reply: String,
    posting: Boolean,
    error: String?,
    onReplyChange: (String) -> Unit,
    onSend: () -> Unit,
) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.md),
        color = SettingsPanelAlt,
    ) {
        Column(
            modifier = Modifier.padding(OpenNowSpacing.md),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
        ) {
            Text(
                stringResource(R.string.bug_report_inbox_reply_title),
                color = SettingsText,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            OutlinedTextField(
                value = reply,
                onValueChange = onReplyChange,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text(stringResource(R.string.bug_report_inbox_reply_hint)) },
                supportingText = { Text("${reply.length} / 3000") },
                minLines = 3,
                maxLines = 7,
                enabled = !posting,
                isError = error != null,
            )
            error?.let { message ->
                Text(message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
            Button(
                onClick = onSend,
                modifier = Modifier.fillMaxWidth().defaultMinSize(minHeight = 48.dp),
                enabled = !posting && reply.isNotBlank(),
            ) {
                if (posting) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(17.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onPrimary,
                    )
                } else {
                    Icon(Icons.AutoMirrored.Rounded.Send, contentDescription = null, modifier = Modifier.size(18.dp))
                }
                Text(
                    stringResource(if (posting) R.string.bug_report_inbox_sending_reply else R.string.bug_report_inbox_send),
                    modifier = Modifier.padding(start = OpenNowSpacing.sm),
                )
            }
        }
    }
}

private data class BugReportStatusPresentation(val label: String, val color: Color)

@Composable
private fun bugReportStatusPresentation(status: String): BugReportStatusPresentation = when (status) {
    "in_review" -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_in_review), OpenNowPalette.AccentPixel)
    "needs_info" -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_needs_info), OpenNowPalette.StatusFair)
    "planned_next_update" -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_planned_next_update), OpenNowPalette.AccentDefault)
    "completed" -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_completed), OpenNowPalette.StatusGood)
    "not_reproducible" -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_not_reproducible), SettingsTextMuted)
    "wont_fix" -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_wont_fix), SettingsTextMuted)
    "closed_by_reporter" -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_closed_by_reporter), SettingsTextMuted)
    else -> BugReportStatusPresentation(stringResource(R.string.bug_report_status_open), OpenNowPalette.AccentPixel)
}

@Composable
private fun BugReportStatusPill(status: BugReportStatusPresentation) {
    Surface(
        shape = RoundedCornerShape(OpenNowRadius.full),
        color = status.color.copy(alpha = 0.14f),
        contentColor = status.color,
        border = BorderStroke(1.dp, status.color.copy(alpha = 0.40f)),
    ) {
        Text(
            status.label,
            modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.SemiBold,
            maxLines = 1,
        )
    }
}

@Composable
private fun ThreadMessage(
    heading: String,
    body: String,
    own: Boolean,
    createdAt: Long? = null,
) {
    val timestamp = remember(createdAt) { createdAt?.let(::formatBugReportTimestamp) }
    Box(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = if (own) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Surface(
            modifier = Modifier.fillMaxWidth(0.92f),
            shape = RoundedCornerShape(
                topStart = OpenNowRadius.md,
                topEnd = OpenNowRadius.md,
                bottomStart = if (own) OpenNowRadius.md else OpenNowRadius.xs,
                bottomEnd = if (own) OpenNowRadius.xs else OpenNowRadius.md,
            ),
            color = if (own) MaterialTheme.colorScheme.primary.copy(alpha = 0.10f) else SettingsPanelAlt,
            border = BorderStroke(
                1.dp,
                if (own) MaterialTheme.colorScheme.primary.copy(alpha = 0.22f)
                else SettingsTextMuted.copy(alpha = 0.12f),
            ),
        ) {
            Column(Modifier.padding(OpenNowSpacing.md), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.xs)) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        heading,
                        color = if (own) MaterialTheme.colorScheme.primary else SettingsTextMuted,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    timestamp?.let {
                        Text(it, color = SettingsTextMuted, style = MaterialTheme.typography.labelSmall)
                    }
                }
                Text(body, color = SettingsText, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}

@Composable
private fun bugReportKindLabel(value: String): String = when (value) {
    "suggestion" -> stringResource(R.string.bug_report_kind_suggestion)
    "performance" -> stringResource(R.string.bug_report_kind_performance)
    "compatibility" -> stringResource(R.string.bug_report_kind_compatibility)
    "other" -> stringResource(R.string.bug_report_kind_other)
    else -> stringResource(R.string.bug_report_kind_bug)
}

@Composable
private fun bugReportAreaLabel(value: String): String = when (value) {
    "streaming" -> stringResource(R.string.bug_report_area_streaming)
    "video" -> stringResource(R.string.bug_report_area_video)
    "audio" -> stringResource(R.string.bug_report_area_audio)
    "input" -> stringResource(R.string.bug_report_area_input)
    "catalog" -> stringResource(R.string.bug_report_area_catalog)
    "account" -> stringResource(R.string.bug_report_area_account)
    "updates" -> stringResource(R.string.bug_report_area_updates)
    else -> stringResource(R.string.bug_report_area_other)
}

@Composable
private fun bugReportFrequencyLabel(value: String): String = when (value) {
    "once" -> stringResource(R.string.bug_report_frequency_once)
    "sometimes" -> stringResource(R.string.bug_report_frequency_sometimes)
    "always" -> stringResource(R.string.bug_report_frequency_always)
    else -> stringResource(R.string.bug_report_frequency_not_sure)
}

@Composable
private fun bugReportImpactLabel(value: String): String = when (value) {
    "low" -> stringResource(R.string.bug_report_impact_low)
    "high" -> stringResource(R.string.bug_report_impact_high)
    "blocking" -> stringResource(R.string.bug_report_impact_blocking)
    else -> stringResource(R.string.bug_report_impact_normal)
}

private fun formatBugReportTimestamp(timestamp: Long): String? = timestamp
    .takeIf { it > 0L }
    ?.let { DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(it)) }
