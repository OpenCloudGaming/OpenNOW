package com.opencloudgaming.opennow

import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
internal fun SettingsBackupPanel(viewModel: OpenNowViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var exportSnapshot by remember { mutableStateOf<AppSettings?>(null) }
    var restoreCandidate by remember { mutableStateOf<AppSettings?>(null) }
    var busy by remember { mutableStateOf(false) }

    val saveLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { uri ->
        val snapshot = exportSnapshot
        exportSnapshot = null
        if (uri != null && snapshot != null) {
            busy = true
            scope.launch {
                val success = runCatching {
                    withContext(Dispatchers.IO) {
                        val bytes = encodeSettingsBackup(snapshot)
                        context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(bytes) }
                            ?: error("Could not open backup file")
                    }
                }.onFailure { if (it is CancellationException) throw it }.isSuccess
                busy = false
                Toast.makeText(
                    context,
                    context.getString(if (success) R.string.settings_backup_saved else R.string.settings_backup_save_failed),
                    Toast.LENGTH_LONG,
                ).show()
            }
        }
    }
    val openLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) {
            busy = true
            scope.launch {
                val result = runCatching {
                    withContext(Dispatchers.IO) {
                        context.contentResolver.openInputStream(uri)?.use(::decodeSettingsBackup)
                            ?: error("Could not open backup file")
                    }
                }
                busy = false
                result.onSuccess { restoreCandidate = it }
                    .onFailure {
                        if (it is CancellationException) throw it
                        Toast.makeText(context, context.getString(R.string.settings_backup_invalid), Toast.LENGTH_LONG).show()
                    }
            }
        }
    }

    restoreCandidate?.let { candidate ->
        AlertDialog(
            onDismissRequest = { restoreCandidate = null },
            title = { Text(stringResource(R.string.settings_backup_restore_title)) },
            text = { Text(stringResource(R.string.settings_backup_restore_confirm)) },
            confirmButton = {
                Button(onClick = {
                    restoreCandidate = null
                    busy = true
                    scope.launch {
                        val success = runCatching { withContext(Dispatchers.IO) { viewModel.restoreSettingsBackup(candidate) } }
                            .onFailure { if (it is CancellationException) throw it }
                            .getOrDefault(false)
                        busy = false
                        Toast.makeText(
                            context,
                            context.getString(if (success) R.string.settings_backup_restored else R.string.settings_backup_restore_failed),
                            Toast.LENGTH_LONG,
                        ).show()
                    }
                }) { Text(stringResource(R.string.settings_backup_restore)) }
            },
            dismissButton = {
                TextButton(onClick = { restoreCandidate = null }) { Text(stringResource(R.string.action_cancel)) }
            },
        )
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        Text(
            stringResource(R.string.settings_backup_description),
            color = SettingsTextMuted,
            style = MaterialTheme.typography.bodySmall,
        )
        OutlinedButton(
            onClick = {
                exportSnapshot = viewModel.state.value.settings
                saveLauncher.launch("OpenNOW-settings.json")
            },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth(),
        ) { Text(stringResource(R.string.settings_backup_create)) }
        OutlinedButton(
            onClick = { openLauncher.launch(arrayOf("*/*")) },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth(),
        ) { Text(stringResource(R.string.settings_backup_restore)) }
    }
}
