package com.opencloudgaming.opennow

import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

@Composable
internal fun AndroidStreamMenuShortcutDialog(
    currentShortcut: String,
    onShortcutChange: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { focusRequester.requestFocus() }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        modifier = Modifier
            .focusRequester(focusRequester)
            .focusable()
            .onPreviewKeyEvent { keyEvent ->
                val shortcut = androidKeyboardShortcutFromEvent(keyEvent.nativeKeyEvent)
                    ?: return@onPreviewKeyEvent false
                onShortcutChange(shortcut)
                true
            },
        title = {
            Text(
                text = stringResource(R.string.stream_menu_shortcut_dialog_title),
                fontWeight = FontWeight.Bold,
            )
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    text = stringResource(R.string.stream_menu_shortcut_press_keys),
                    style = MaterialTheme.typography.bodyMedium,
                )
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    shape = MaterialTheme.shapes.small,
                    tonalElevation = 2.dp,
                ) {
                    Column(Modifier.padding(14.dp)) {
                        Text(
                            text = stringResource(R.string.stream_menu_shortcut_current),
                            style = MaterialTheme.typography.labelSmall,
                            color = SettingsTextMuted,
                        )
                        Text(
                            text = if (currentShortcut.equals(
                                    DISABLED_ANDROID_STREAM_MENU_SHORTCUT,
                                    ignoreCase = true,
                                )
                            ) {
                                stringResource(R.string.stream_menu_shortcut_disabled)
                            } else {
                                androidKeyboardShortcutDisplay(currentShortcut)
                            },
                            style = MaterialTheme.typography.titleMedium,
                        )
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onShortcutChange(DISABLED_ANDROID_STREAM_MENU_SHORTCUT) },
            ) {
                Text(stringResource(R.string.stream_menu_shortcut_disable))
            }
        },
        dismissButton = {
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                TextButton(
                    onClick = { onShortcutChange(DEFAULT_ANDROID_STREAM_MENU_SHORTCUT) },
                ) {
                    Text(stringResource(R.string.stream_menu_shortcut_reset))
                }
                TextButton(onClick = onDismiss) {
                    Text(stringResource(R.string.action_cancel))
                }
            }
        },
        containerColor = SettingsPanel,
        titleContentColor = SettingsText,
        textContentColor = SettingsTextMuted,
    )
}
