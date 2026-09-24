package com.opencloudgaming.opennow

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Attachment
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.Serializable

private const val ANDROID_BUG_REPORT_MAX_MEDIA_TOTAL_BYTES = 20L * 1024L * 1024L

internal enum class AndroidBugReportKind(val wireValue: String, @StringRes val labelRes: Int) {
    Bug("bug", R.string.bug_report_kind_bug),
    Suggestion("suggestion", R.string.bug_report_kind_suggestion),
    Performance("performance", R.string.bug_report_kind_performance),
    Compatibility("compatibility", R.string.bug_report_kind_compatibility),
    Other("other", R.string.bug_report_kind_other),
}

internal enum class AndroidBugReportArea(val wireValue: String, @StringRes val labelRes: Int) {
    Streaming("streaming", R.string.bug_report_area_streaming),
    Video("video", R.string.bug_report_area_video),
    Audio("audio", R.string.bug_report_area_audio),
    Input("input", R.string.bug_report_area_input),
    Catalog("catalog", R.string.bug_report_area_catalog),
    Account("account", R.string.bug_report_area_account),
    Updates("updates", R.string.bug_report_area_updates),
    Other("other", R.string.bug_report_area_other),
}

internal enum class AndroidBugReportFrequency(val wireValue: String, @StringRes val labelRes: Int) {
    Once("once", R.string.bug_report_frequency_once),
    Sometimes("sometimes", R.string.bug_report_frequency_sometimes),
    Always("always", R.string.bug_report_frequency_always),
    NotSure("not_sure", R.string.bug_report_frequency_not_sure),
}

internal enum class AndroidBugReportImpact(val wireValue: String, @StringRes val labelRes: Int) {
    Low("low", R.string.bug_report_impact_low),
    Normal("normal", R.string.bug_report_impact_normal),
    High("high", R.string.bug_report_impact_high),
    Blocking("blocking", R.string.bug_report_impact_blocking),
}

internal data class AndroidBugReportDetails(
    val kind: AndroidBugReportKind = AndroidBugReportKind.Bug,
    val area: AndroidBugReportArea = AndroidBugReportArea.Other,
    val frequency: AndroidBugReportFrequency = AndroidBugReportFrequency.NotSure,
    val impact: AndroidBugReportImpact = AndroidBugReportImpact.Normal,
) : Serializable

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun BugReportOptions(
    details: AndroidBugReportDetails,
    attachments: List<AndroidBugReportAttachment>,
    enabled: Boolean,
    onDetailsChange: (AndroidBugReportDetails) -> Unit,
    onAttachmentsChange: (List<AndroidBugReportAttachment>) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var expanded by rememberSaveable { mutableStateOf(false) }
    var mediaError by remember { mutableStateOf<String?>(null) }
    val mediaPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    var totalBytes = attachments.sumOf { it.bytes.size.toLong() }
                    uris.take(4 - attachments.size).map { uri ->
                        readBugReportMedia(context.contentResolver, uri).also { media ->
                            totalBytes += media.bytes.size
                            require(totalBytes <= ANDROID_BUG_REPORT_MAX_MEDIA_TOTAL_BYTES) {
                                "Attached media must be 20 MiB or less in total"
                            }
                        }
                    }
                }
            }
            result.onSuccess { selected ->
                onAttachmentsChange((attachments + selected).take(4))
                mediaError = null
            }.onFailure { error ->
                mediaError = error.message ?: "Could not attach that media"
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md)) {
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(OpenNowRadius.lg),
            color = OpenNowPalette.PanelAlt,
            border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
        ) {
            Column(
                modifier = Modifier.padding(OpenNowSpacing.md),
                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            ) {
                Text(
                    stringResource(R.string.bug_report_kind_label),
                    style = MaterialTheme.typography.titleSmall,
                )
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    AndroidBugReportKind.entries.forEach { option ->
                        FilterChip(
                            selected = details.kind == option,
                            onClick = { onDetailsChange(details.copy(kind = option)) },
                            enabled = enabled,
                            label = { Text(stringResource(option.labelRes)) },
                        )
                    }
                }
                TextButton(onClick = { expanded = !expanded }, enabled = enabled) {
                    Text(stringResource(if (expanded) R.string.bug_report_fewer_details else R.string.bug_report_more_details))
                    Icon(
                        if (expanded) Icons.Rounded.KeyboardArrowUp else Icons.Rounded.KeyboardArrowDown,
                        contentDescription = null,
                        modifier = Modifier.size(20.dp),
                    )
                }
                if (expanded) {
                    Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md)) {
                        BugReportOptionChips(
                            label = stringResource(R.string.bug_report_area_label),
                            entries = AndroidBugReportArea.entries,
                            selected = details.area,
                            enabled = enabled,
                            labelRes = { it.labelRes },
                            onSelect = { onDetailsChange(details.copy(area = it)) },
                        )
                        BugReportOptionChips(
                            label = stringResource(R.string.bug_report_frequency_label),
                            entries = AndroidBugReportFrequency.entries,
                            selected = details.frequency,
                            enabled = enabled,
                            labelRes = { it.labelRes },
                            onSelect = { onDetailsChange(details.copy(frequency = it)) },
                        )
                        BugReportOptionChips(
                            label = stringResource(R.string.bug_report_impact_label),
                            entries = AndroidBugReportImpact.entries,
                            selected = details.impact,
                            enabled = enabled,
                            labelRes = { it.labelRes },
                            onSelect = { onDetailsChange(details.copy(impact = it)) },
                        )
                    }
                }
            }
        }
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(OpenNowRadius.lg),
            color = OpenNowPalette.PanelAlt,
            border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
        ) {
            Column(
                modifier = Modifier.padding(OpenNowSpacing.md),
                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                    Icon(Icons.Rounded.Attachment, contentDescription = null, modifier = Modifier.size(20.dp))
                    Text(
                        stringResource(R.string.bug_report_inbox_attachments),
                        style = MaterialTheme.typography.titleSmall,
                        modifier = Modifier.weight(1f),
                    )
                    if (attachments.isNotEmpty()) {
                        Text(
                            stringResource(R.string.bug_report_media_count, attachments.size),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                Text(
                    stringResource(R.string.bug_report_media_note),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedButton(
                    onClick = { mediaPicker.launch(arrayOf("image/*", "video/*")) },
                    enabled = enabled && attachments.size < 4,
                ) {
                    Text(stringResource(R.string.bug_report_add_media))
                }
                attachments.forEachIndexed { index, attachment ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            attachment.fileName,
                            modifier = Modifier.weight(1f).padding(end = OpenNowSpacing.sm),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        TextButton(
                            onClick = { onAttachmentsChange(attachments.filterIndexed { itemIndex, _ -> itemIndex != index }) },
                            enabled = enabled,
                        ) {
                            Text(stringResource(R.string.action_remove))
                        }
                    }
                }
                mediaError?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun <T> BugReportOptionChips(
    label: String,
    entries: List<T>,
    selected: T,
    enabled: Boolean,
    labelRes: (T) -> Int,
    onSelect: (T) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            entries.forEach { option ->
                FilterChip(
                    selected = selected == option,
                    onClick = { onSelect(option) },
                    enabled = enabled,
                    label = { Text(stringResource(labelRes(option))) },
                )
            }
        }
    }
}

private fun readBugReportMedia(contentResolver: ContentResolver, uri: Uri): AndroidBugReportAttachment {
    val fileName = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0) else null
    }?.take(180)?.takeIf { it.isNotBlank() } ?: "media"
    val declaredSize = contentResolver.openAssetFileDescriptor(uri, "r")?.use { it.length } ?: -1L
    require(declaredSize <= ANDROID_BUG_REPORT_MAX_FILE_BYTES || declaredSize < 0) { "$fileName is larger than 10 MiB" }
    val bytes = contentResolver.openInputStream(uri)?.use { input ->
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(16 * 1024)
        var total = 0L
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            total += count
            require(total <= ANDROID_BUG_REPORT_MAX_FILE_BYTES) { "$fileName is larger than 10 MiB" }
            output.write(buffer, 0, count)
        }
        output.toByteArray()
    } ?: error("Could not read $fileName")
    return AndroidBugReportAttachment(
        fileName = fileName,
        contentType = contentResolver.getType(uri)?.take(120) ?: "application/octet-stream",
        bytes = bytes,
    )
}
