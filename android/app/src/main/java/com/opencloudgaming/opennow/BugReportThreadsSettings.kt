package com.opencloudgaming.opennow

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

@Composable
internal fun BugReportThreadsSettings(
    state: AndroidBugReportThreadsState,
    onRefresh: () -> Unit,
    onComment: (reportId: String, comment: String, termsAccepted: Boolean) -> Unit,
) {
    var expandedReportId by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        while (isActive) {
            onRefresh()
            delay(30_000)
        }
    }

    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    stringResource(R.string.bug_report_inbox_title),
                    color = SettingsText,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    stringResource(R.string.bug_report_inbox_body),
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            OutlinedButton(onClick = onRefresh, enabled = !state.loading) {
                Text(stringResource(R.string.bug_report_inbox_refresh))
            }
        }

        state.error?.let { message ->
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = MaterialTheme.colorScheme.errorContainer,
                contentColor = MaterialTheme.colorScheme.onErrorContainer,
            ) {
                Text(message, modifier = Modifier.padding(12.dp), style = MaterialTheme.typography.bodySmall)
            }
        }

        when {
            state.loading && state.reports.isEmpty() -> Row(
                modifier = Modifier.fillMaxWidth().padding(vertical = 20.dp),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(modifier = Modifier.padding(end = 10.dp).size(22.dp), strokeWidth = 2.dp)
                Text(stringResource(R.string.bug_report_inbox_loading), color = SettingsTextMuted)
            }
            state.reports.isEmpty() -> Text(
                stringResource(R.string.bug_report_inbox_empty),
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.padding(vertical = 12.dp),
            )
            else -> state.reports.forEach { report ->
                key(report.id) {
                    BugReportThreadCard(
                        report = report,
                        expanded = expandedReportId == report.id,
                        posting = state.postingReportId == report.id,
                        onToggle = {
                            expandedReportId = if (expandedReportId == report.id) null else report.id
                        },
                        onComment = { comment, accepted -> onComment(report.id, comment, accepted) },
                    )
                }
            }
        }
    }
}

@Composable
private fun BugReportThreadCard(
    report: AndroidBugReportThread,
    expanded: Boolean,
    posting: Boolean,
    onToggle: () -> Unit,
    onComment: (String, Boolean) -> Unit,
) {
    var reply by rememberSaveable(report.id) { mutableStateOf("") }
    var termsAccepted by rememberSaveable(report.id) { mutableStateOf(false) }
    val updatedLabel = remember(report.updatedAt) {
        DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT)
            .format(Date(report.updatedAt))
    }
    val closed = androidBugReportThreadClosed(report.status)
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = SettingsPanel,
        border = BorderStroke(1.dp, SettingsTextMuted.copy(alpha = 0.24f)),
    ) {
        Column(Modifier.fillMaxWidth()) {
            Column(
                modifier = Modifier.fillMaxWidth().clickable(onClick = onToggle).padding(14.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.Top,
                ) {
                    Text(
                        report.title,
                        modifier = Modifier.weight(1f).padding(end = 10.dp),
                        color = SettingsText,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    BugReportStatusPill(report.status)
                }
                Text(updatedLabel, color = SettingsTextMuted, style = MaterialTheme.typography.labelSmall)
            }

            if (expanded) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(start = 14.dp, end = 14.dp, bottom = 14.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    if (report.versionName.isNotBlank() || report.versionCode.isNotBlank()) {
                        Text(
                            stringResource(
                                R.string.bug_report_inbox_version,
                                report.versionName.ifBlank { "?" },
                                report.versionCode.ifBlank { "?" },
                            ),
                            color = SettingsTextMuted,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                    Text(
                        listOf(report.kind, report.area, report.frequency, report.impact)
                            .joinToString(" • ") { it.replace('_', ' ').replaceFirstChar(Char::uppercase) },
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.labelSmall,
                    )
                    if (report.files.isNotEmpty()) {
                        Text(
                            "Attachments: ${report.files.joinToString()}",
                            color = SettingsTextMuted,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                    ThreadMessage(
                        heading = stringResource(R.string.bug_report_inbox_original_report),
                        body = report.description,
                        own = true,
                    )
                    if (report.resolutionNote.isNotBlank()) {
                        ThreadMessage(
                            heading = stringResource(R.string.bug_report_inbox_status_update),
                            body = report.resolutionNote,
                            own = false,
                        )
                    }
                    Text(
                        stringResource(R.string.bug_report_inbox_conversation),
                        color = SettingsText,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    if (report.comments.isEmpty()) {
                        Text(
                            stringResource(R.string.bug_report_inbox_no_replies),
                            color = SettingsTextMuted,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    } else {
                        report.comments.forEach { comment ->
                            ThreadMessage(
                                heading = when {
                                    comment.kind == "status" -> stringResource(R.string.bug_report_inbox_status_update)
                                    comment.authorRole == "reporter" -> stringResource(R.string.bug_report_inbox_you)
                                    else -> stringResource(R.string.bug_report_inbox_release_team)
                                },
                                body = comment.body,
                                own = comment.authorRole == "reporter",
                            )
                        }
                    }
                    if (closed) {
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = SettingsPanelAlt,
                        ) {
                            Text(
                                stringResource(R.string.bug_report_inbox_closed),
                                modifier = Modifier.padding(12.dp),
                                color = SettingsTextMuted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    } else {
                        OutlinedTextField(
                            value = reply,
                            onValueChange = { if (it.length <= 3_000) reply = it },
                            modifier = Modifier.fillMaxWidth(),
                            label = { Text(stringResource(R.string.bug_report_inbox_reply_hint)) },
                            minLines = 3,
                            enabled = !posting,
                        )
                        Row(
                            modifier = Modifier.fillMaxWidth().clickable { termsAccepted = !termsAccepted },
                            verticalAlignment = Alignment.Top,
                        ) {
                            Checkbox(
                                checked = termsAccepted,
                                onCheckedChange = { termsAccepted = it },
                            )
                            Text(
                                stringResource(R.string.bug_report_inbox_terms),
                                modifier = Modifier.padding(top = 11.dp),
                                color = SettingsTextMuted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                        Button(
                            onClick = {
                                onComment(reply, termsAccepted)
                                reply = ""
                                termsAccepted = false
                            },
                            enabled = !posting && reply.isNotBlank() && termsAccepted,
                        ) {
                            if (posting) {
                                CircularProgressIndicator(
                                    modifier = Modifier.padding(end = 8.dp).size(16.dp),
                                    strokeWidth = 2.dp,
                                )
                            }
                            Text(stringResource(R.string.bug_report_inbox_send))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun BugReportStatusPill(status: String) {
    val (label, color) = when (status) {
        "in_review" -> stringResource(R.string.bug_report_status_in_review) to Color(0xff64b5f6)
        "needs_info" -> stringResource(R.string.bug_report_status_needs_info) to Color(0xffffb74d)
        "planned_next_update" -> stringResource(R.string.bug_report_status_planned_next_update) to Color(0xff81c784)
        "completed" -> stringResource(R.string.bug_report_status_completed) to Color(0xff66bb6a)
        "not_reproducible" -> stringResource(R.string.bug_report_status_not_reproducible) to SettingsTextMuted
        "wont_fix" -> stringResource(R.string.bug_report_status_wont_fix) to SettingsTextMuted
        else -> stringResource(R.string.bug_report_status_open) to SettingsTextMuted
    }
    Surface(
        shape = RoundedCornerShape(999.dp),
        color = color.copy(alpha = 0.14f),
        contentColor = color,
        border = BorderStroke(1.dp, color.copy(alpha = 0.42f)),
    ) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 9.dp, vertical = 4.dp),
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

@Composable
private fun ThreadMessage(heading: String, body: String, own: Boolean) {
    Surface(
        modifier = Modifier.fillMaxWidth(if (own) 0.94f else 1f),
        shape = RoundedCornerShape(10.dp),
        color = if (own) SettingsPanelAlt else MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Column(Modifier.padding(11.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(heading, color = SettingsTextMuted, style = MaterialTheme.typography.labelSmall)
            Text(body, color = SettingsText, style = MaterialTheme.typography.bodyMedium)
        }
    }
}
