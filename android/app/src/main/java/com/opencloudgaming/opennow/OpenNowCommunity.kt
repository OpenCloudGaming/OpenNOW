package com.opencloudgaming.opennow

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

internal const val OPENNOW_DISCORD_COMMUNITY_URL = "https://discord.gg/euSABw8CX8"
private const val DISCORD_RULES_ACCEPTED = "rules_accepted"

@Composable
internal fun DiscordCommunityLink(
    summary: String,
    modifier: Modifier = Modifier,
    containerColor: Color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f),
) {
    val context = LocalContext.current
    val preferences = remember(context) {
        context.applicationContext.getSharedPreferences("discord_community", Context.MODE_PRIVATE)
    }
    var showRules by rememberSaveable { mutableStateOf(false) }
    var rememberAgreement by rememberSaveable { mutableStateOf(false) }
    if (showRules) {
        AlertDialog(
            onDismissRequest = { showRules = false },
            title = { Text(stringResource(R.string.discord_community_title)) },
            text = {
                Column(
                    modifier = Modifier.verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    Text(stringResource(R.string.discord_community_rules_notice))
                    Text(stringResource(R.string.discord_community_rules_enforcement))
                    Row(
                        modifier = Modifier.fillMaxWidth().toggleable(
                            value = rememberAgreement,
                            role = Role.Checkbox,
                            onValueChange = { rememberAgreement = it },
                        ).padding(vertical = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(checked = rememberAgreement, onCheckedChange = null)
                        Text(stringResource(R.string.discord_community_rules_remember))
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    if (rememberAgreement) {
                        preferences.edit().putBoolean(DISCORD_RULES_ACCEPTED, true).apply()
                    }
                    showRules = false
                    openDiscordCommunityInvite(context)
                }) {
                    Text(stringResource(R.string.discord_community_rules_yes))
                }
            },
        )
    }
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = containerColor,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    stringResource(R.string.discord_community_title),
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    summary,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            OutlinedButton(
                onClick = {
                    if (preferences.getBoolean(DISCORD_RULES_ACCEPTED, false)) {
                        openDiscordCommunityInvite(context)
                    } else {
                        rememberAgreement = false
                        showRules = true
                    }
                },
            ) {
                Text(
                    stringResource(R.string.discord_community_action),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

private fun openDiscordCommunityInvite(context: Context) {
    if (!openExternalUrl(context, OPENNOW_DISCORD_COMMUNITY_URL)) {
        context.getSystemService(ClipboardManager::class.java)?.setPrimaryClip(
            ClipData.newPlainText(
                context.getString(R.string.discord_community_title),
                OPENNOW_DISCORD_COMMUNITY_URL,
            ),
        )
        Toast.makeText(
            context,
            context.getString(R.string.discord_community_link_copied),
            Toast.LENGTH_SHORT,
        ).show()
    }
}
