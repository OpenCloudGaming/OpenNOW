package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.opencloudgaming.opennow.ui.controls.ControlRow
import com.opencloudgaming.opennow.ui.controls.ControlRowLabels
import com.opencloudgaming.opennow.ui.controls.controlRowStyle
import androidx.compose.foundation.layout.size

/** Small, predictable inline format for release notes: **bold**, *italic*, __underline__. */
internal fun formattedChangelogNote(source: String): AnnotatedString = buildAnnotatedString {
    var position = 0

    fun parse(until: String? = null) {
        while (position < source.length) {
            if (until != null && source.startsWith(until, position)) {
                position += until.length
                return
            }
            if (source[position] == '\\' && position + 1 < source.length &&
                source[position + 1] in charArrayOf('*', '_', '\\')
            ) {
                append(source[position + 1])
                position += 2
                continue
            }
            val marker = listOf("***", "**", "__", "*").firstOrNull { token ->
                source.startsWith(token, position) &&
                    source.indexOf(token, position + token.length) >= 0
            }
            if (marker == null) {
                append(source[position++])
                continue
            }
            position += marker.length
            val start = length
            parse(marker)
            if (start == length) continue
            addStyle(
                SpanStyle(
                    fontWeight = if (marker == "**" || marker == "***") FontWeight.Bold else null,
                    fontStyle = if (marker == "*" || marker == "***") FontStyle.Italic else null,
                    textDecoration = if (marker == "__") TextDecoration.Underline else null,
                ),
                start,
                length,
            )
        }
    }

    parse()
}

@Composable
internal fun ChangelogSettingsContent(entries: List<ChangelogEntry>) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (entries.isEmpty()) {
            Text(
                stringResource(R.string.changelog_empty),
                modifier = Modifier.padding(16.dp),
                color = SettingsTextMuted,
            )
        }
        entries.forEachIndexed { index, entry ->
            key(entry.versionCode) {
                var expanded by rememberSaveable { mutableStateOf(index == 0) }
                val rowStyle = controlRowStyle()
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    ControlRow(onClick = { expanded = !expanded }, style = rowStyle) {
                        ControlRowLabels(
                            label = stringResource(R.string.changelog_version, entry.version, entry.versionCode),
                            value = null,
                            expandedDescription = null,
                            enabled = true,
                            style = rowStyle,
                        )
                        Icon(
                            imageVector = if (expanded) Icons.Rounded.KeyboardArrowUp else Icons.Rounded.KeyboardArrowDown,
                            contentDescription = stringResource(
                                if (expanded) R.string.changelog_collapse else R.string.changelog_expand,
                            ),
                            tint = rowStyle.supportingColor,
                            modifier = Modifier.size(24.dp),
                        )
                    }
                    if (expanded) {
                        Surface(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(14.dp),
                            color = SettingsPanelAlt,
                        ) {
                            Column(
                                modifier = Modifier.padding(horizontal = 18.dp, vertical = 14.dp),
                                verticalArrangement = Arrangement.spacedBy(10.dp),
                            ) {
                                entry.notes.forEach { note ->
                                    val formatted = remember(note) { formattedChangelogNote(note) }
                                    Text(
                                        buildAnnotatedString {
                                            append("•  ")
                                            append(formatted)
                                        },
                                        color = SettingsText,
                                        style = MaterialTheme.typography.bodyMedium,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** The first-run Play update view uses the same expandable content as the Settings page. */
@Composable
internal fun ChangelogDialog(entries: List<ChangelogEntry>, onDismiss: () -> Unit) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background,
        ) {
            Column(
                modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 20.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        stringResource(R.string.changelog_title),
                        color = SettingsText,
                        style = MaterialTheme.typography.headlineSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    TextButton(onClick = onDismiss) {
                        Text(stringResource(R.string.changelog_done))
                    }
                }
                Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
                    ChangelogSettingsContent(entries)
                }
            }
        }
    }
}
