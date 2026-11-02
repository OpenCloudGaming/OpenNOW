package com.opencloudgaming.opennow

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.BugReport
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.ThumbUp
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun BugReportsSettings(
    publishedState: AndroidPublishedBugsState,
    onRefreshPublished: () -> Unit,
    onLoadMore: () -> Unit,
    onVote: (String) -> Unit,
    onUploadLogs: (String) -> Unit,
    yourReports: @Composable () -> Unit,
) {
    var tab by rememberSaveable { mutableStateOf(0) }
    Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg)) {
        PrimaryTabRow(selectedTabIndex = tab, containerColor = SettingsPanel) {
            Tab(selected = tab == 0, onClick = { tab = 0 }, text = { Text(stringResource(R.string.published_bugs_title)) })
            Tab(selected = tab == 1, onClick = { tab = 1 }, text = { Text(stringResource(R.string.bug_report_inbox_title)) })
        }
        if (tab == 0) PublishedBugsSettings(publishedState, onRefreshPublished, onLoadMore, onVote, onUploadLogs)
        else yourReports()
    }
}

@Composable
internal fun PublishedBugsSettings(
    state: AndroidPublishedBugsState,
    onRefresh: () -> Unit,
    onLoadMore: () -> Unit,
    onVote: (String) -> Unit,
    onUploadLogs: (String) -> Unit,
) {
    var selectedId by rememberSaveable { mutableStateOf<String?>(null) }
    var uploadId by rememberSaveable { mutableStateOf<String?>(null) }
    val selected = state.bugs.firstOrNull { it.id == selectedId }
    val working = state.actingBugId != null || state.loading
    BackHandler(enabled = selectedId != null) { selectedId = null }
    val refresh by rememberUpdatedState(onRefresh)
    val canAutoRefresh by rememberUpdatedState(selectedId == null && state.bugs.size <= 20)
    LaunchedEffect(Unit) {
        refresh()
        while (isActive) {
            delay(60_000)
            if (canAutoRefresh) refresh()
        }
    }
    uploadId?.let { id ->
        AlertDialog(
            onDismissRequest = { uploadId = null },
            title = { Text(stringResource(R.string.published_bugs_share_title)) },
            text = { Text(stringResource(R.string.published_bugs_share_body)) },
            confirmButton = { TextButton(onClick = { uploadId = null; onUploadLogs(id) }, enabled = !working) { Text(stringResource(R.string.published_bugs_share_action)) } },
            dismissButton = { TextButton(onClick = { uploadId = null }) { Text(stringResource(R.string.bug_report_inbox_cancel)) } },
        )
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            Text(stringResource(R.string.published_bugs_intro), modifier = Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = SettingsTextMuted)
            IconButton(onClick = onRefresh, enabled = !working, modifier = Modifier.size(48.dp)) {
                if (state.loading) CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                else Icon(Icons.Rounded.Refresh, contentDescription = stringResource(R.string.published_bugs_refresh), tint = SettingsTextMuted)
            }
        }
        state.error?.let {
            Surface(shape = RoundedCornerShape(OpenNowRadius.md), color = MaterialTheme.colorScheme.errorContainer, contentColor = MaterialTheme.colorScheme.onErrorContainer) {
                Column(Modifier.fillMaxWidth().padding(OpenNowSpacing.md), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                    Text(it, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }, style = MaterialTheme.typography.bodyMedium)
                    TextButton(onClick = onRefresh, enabled = !working) { Text(stringResource(R.string.action_retry)) }
                }
            }
        }
        if (selected != null) {
            TextButton(onClick = { selectedId = null }) {
                Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null)
                Text(stringResource(R.string.published_bugs_back), Modifier.padding(start = OpenNowSpacing.sm))
            }
            PublishedBugCard(selected, detail = true, onOpen = {}, busy = working, uploading = state.uploadingLogs && state.actingBugId == selected.id, uploaded = state.uploadedBugId == selected.id, onVote = { onVote(selected.id) }, onUpload = { uploadId = selected.id })
        } else if (state.bugs.isEmpty()) {
            Surface(Modifier.fillMaxWidth(), shape = RoundedCornerShape(OpenNowRadius.lg), color = SettingsPanelAlt) {
                Column(Modifier.padding(OpenNowSpacing.xl), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md), horizontalAlignment = Alignment.CenterHorizontally) {
                    if (state.loading) CircularProgressIndicator(Modifier.size(28.dp)) else Icon(Icons.Rounded.BugReport, contentDescription = null, modifier = Modifier.size(32.dp), tint = MaterialTheme.colorScheme.primary)
                    Text(stringResource(if (state.loading) R.string.published_bugs_loading else R.string.published_bugs_empty_title), style = MaterialTheme.typography.titleMedium, color = SettingsText)
                    if (!state.loading) Text(stringResource(R.string.published_bugs_empty_body), style = MaterialTheme.typography.bodyMedium, color = SettingsTextMuted)
                }
            }
        } else {
            state.bugs.forEach { bug -> key(bug.id) {
                PublishedBugCard(bug, detail = false, busy = working, uploading = state.uploadingLogs && state.actingBugId == bug.id, uploaded = state.uploadedBugId == bug.id, onOpen = { selectedId = bug.id }, onVote = { onVote(bug.id) }, onUpload = { uploadId = bug.id })
            } }
            if (state.nextCursor != null) OutlinedButton(onClick = onLoadMore, enabled = !working, modifier = Modifier.fillMaxWidth().defaultMinSize(minHeight = 48.dp)) { Text(stringResource(R.string.published_bugs_load_more)) }
        }
    }
}

@Composable
private fun publishedBugStatus(bug: AndroidPublishedBug): String = when (bug.status) {
    "fixing" -> stringResource(R.string.published_bugs_status_fixing)
    "awaiting_review" -> stringResource(R.string.published_bugs_status_review)
    "released" -> stringResource(R.string.published_bugs_status_released, bug.fixedVersion)
    else -> stringResource(R.string.published_bugs_status_investigating)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PublishedBugCard(
    bug: AndroidPublishedBug,
    detail: Boolean,
    onOpen: () -> Unit,
    busy: Boolean,
    uploading: Boolean,
    uploaded: Boolean,
    onVote: () -> Unit,
    onUpload: () -> Unit,
) {
    Surface(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(OpenNowRadius.lg), color = SettingsPanel, border = BorderStroke(1.dp, SettingsTextMuted.copy(alpha = 0.18f))) {
        Column(Modifier.fillMaxWidth().padding(OpenNowSpacing.md), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md)) {
            Surface(shape = RoundedCornerShape(OpenNowRadius.md), color = MaterialTheme.colorScheme.primaryContainer, contentColor = MaterialTheme.colorScheme.onPrimaryContainer) {
                Text(publishedBugStatus(bug), Modifier.padding(horizontal = OpenNowSpacing.md, vertical = OpenNowSpacing.sm), style = MaterialTheme.typography.labelMedium)
            }
            Text(bug.title, modifier = Modifier.semantics { heading() }, style = if (detail) MaterialTheme.typography.headlineSmall else MaterialTheme.typography.titleMedium, color = SettingsText, fontWeight = FontWeight.SemiBold)
            Text(
                formattedAppMessageBody(bug.body, MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.error),
                style = MaterialTheme.typography.bodyMedium, color = SettingsText,
                maxLines = if (detail) Int.MAX_VALUE else 3, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
            )
            Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(stringResource(R.string.published_bugs_progress), style = MaterialTheme.typography.labelMedium, color = SettingsTextMuted)
                    Text(stringResource(R.string.published_bugs_percent, bug.progress), style = MaterialTheme.typography.labelMedium, color = SettingsText)
                }
                LinearProgressIndicator(progress = { bug.progress / 100f }, modifier = Modifier.fillMaxWidth())
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                Button(onClick = onVote, enabled = !busy, modifier = Modifier.defaultMinSize(minHeight = 48.dp)) {
                    Icon(Icons.Rounded.ThumbUp, contentDescription = null, modifier = Modifier.size(18.dp))
                    Text(stringResource(if (bug.voted) R.string.published_bugs_voted else R.string.published_bugs_vote, bug.votes), Modifier.padding(start = OpenNowSpacing.sm))
                }
                OutlinedButton(onClick = onUpload, enabled = !busy && bug.acceptingLogs, modifier = Modifier.defaultMinSize(minHeight = 48.dp)) {
                    if (uploading) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    else Icon(Icons.Rounded.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                    Text(stringResource(R.string.published_bugs_add_logs), Modifier.padding(start = OpenNowSpacing.sm))
                }
                if (!detail) TextButton(onClick = onOpen, modifier = Modifier.defaultMinSize(minHeight = 48.dp)) { Text(stringResource(R.string.published_bugs_details)) }
            }
            if (!bug.acceptingLogs) Text(stringResource(R.string.published_bugs_logs_paused), style = MaterialTheme.typography.bodySmall, color = SettingsTextMuted)
            if (detail && bug.status == "awaiting_review") {
                Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                    Text(stringResource(R.string.published_bugs_review_title), style = MaterialTheme.typography.titleSmall, color = SettingsText, fontWeight = FontWeight.SemiBold)
                    Text(formattedAppMessageBody(stringResource(R.string.published_bugs_review_body), MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.error), style = MaterialTheme.typography.bodyMedium, color = SettingsTextMuted)
                }
            }
            if (uploaded) Text(stringResource(R.string.published_bugs_uploaded), modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary)
        }
    }

}
