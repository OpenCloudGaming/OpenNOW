package com.opencloudgaming.opennow

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

/** One combo editor shared by Settings and the in-stream control panel. */
@Composable
internal fun TouchExtraButtonComboEditor(
    touch: AndroidTouchSettings,
    onChange: (AndroidTouchSettings) -> Unit,
) {
    var open by rememberSaveable { mutableStateOf(false) }
    var selectedIndex by rememberSaveable { mutableStateOf<Int?>(null) }

    OutlinedButton(
        onClick = { selectedIndex = null; open = true },
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text(stringResource(R.string.touch_extra_combo_customize))
    }
    if (!open) return

    val index = selectedIndex
    if (index == null) {
        AlertDialog(
            onDismissRequest = { open = false },
            title = { Text(stringResource(R.string.touch_extra_combo_customize)) },
            text = {
                Column(
                    Modifier.verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text(
                        stringResource(R.string.touch_extra_combo_description),
                        style = MaterialTheme.typography.bodySmall,
                    )
                    repeat(TOUCH_EXTRA_BUTTON_COUNT) { slot ->
                        TextButton(
                            onClick = { selectedIndex = slot },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Column(Modifier.fillMaxWidth()) {
                                Text(stringResource(R.string.settings_touch_extra_button, slot + 1))
                                Text(
                                    touchExtraButtonComboLabel(touch.extraButtonCombo(slot)),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { open = false }) {
                    Text(stringResource(R.string.touch_editor_close))
                }
            },
        )
    } else {
        TouchExtraButtonComboDialog(
            index = index,
            touch = touch,
            onSave = { actions ->
                onChange(touch.withExtraButtonCombo(index, actions))
                selectedIndex = null
            },
            onDismiss = { selectedIndex = null },
        )
    }
}

@Composable
private fun TouchExtraButtonComboDialog(
    index: Int,
    touch: AndroidTouchSettings,
    onSave: (List<TouchExtraButtonAction>) -> Unit,
    onDismiss: () -> Unit,
) {
    var selected by remember(index, touch.extraButtonCombos[extraButtonComboKey(index)]) {
        mutableStateOf(touch.extraButtonCombo(index))
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_touch_extra_button, index + 1)) },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    stringResource(R.string.touch_extra_combo_action_hint, MAX_TOUCH_EXTRA_BUTTON_COMBO_ACTIONS),
                    style = MaterialTheme.typography.bodySmall,
                )
                customizableTouchExtraButtonActions.forEach { action ->
                    val checked = action in selected
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable {
                                selected = when {
                                    checked -> selected - action
                                    selected.size < MAX_TOUCH_EXTRA_BUTTON_COMBO_ACTIONS -> selected + action
                                    else -> selected
                                }
                            }
                            .padding(vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(
                            checked = checked,
                            onCheckedChange = null,
                        )
                        Text(touchExtraButtonActionLabel(action))
                    }
                }
                TextButton(onClick = { selected = emptyList() }) {
                    Text(stringResource(R.string.touch_extra_combo_clear))
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onSave(selected) }) {
                Text(stringResource(R.string.touch_editor_save))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.touch_editor_cancel))
            }
        },
    )
}
