package com.opencloudgaming.opennow

import android.content.Context
import android.os.Build
import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.controls.ControlActionRow
import com.opencloudgaming.opennow.ui.controls.ControlSection
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import kotlinx.coroutines.launch

/**
 * Settings > Developer options.
 *
 * Revealed by the build-number gesture in About, and built from the same `ControlSection` /
 * `ControlActionRow` kit as every other settings page so it does not read as a bolted-on debug
 * screen — a developer using it is still using OpenNOW.
 *
 * The unlock gesture, and every transform an action applies to [AppSettings], live in
 * `AndroidDeveloperOptions.kt` where they are unit tested.
 *
 * Scope is deliberately narrow, and stays inside what a store build may ship: each action either
 * resets local state the user can already reach elsewhere, or shows information already present in
 * the diagnostics export. Nothing here grants entitlements, alters what is reported about the user,
 * or reaches outside OpenNOW's own data.
 */
@Composable
internal fun DeveloperOptionsPanel(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val settings = state.settings
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val scope = rememberCoroutineScope()
    var pendingDestructive by remember { mutableStateOf<DeveloperDestructiveAction?>(null) }

    fun update(transform: (AppSettings) -> AppSettings, message: String) {
        viewModel.updateSettings(transform(settings))
        Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
    }

    pendingDestructive?.let { action ->
        AlertDialog(
            onDismissRequest = { pendingDestructive = null },
            title = { Text(action.title) },
            text = { Text(action.body) },
            confirmButton = {
                Button(
                    onClick = {
                            pendingDestructive = null
                            action.run()
                    },
                ) {
                    Text(action.confirmLabel)
                }
            },
            dismissButton = {
                TextButton(onClick = { pendingDestructive = null }) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }

    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md)) {
        DeveloperOptionsNotice()

        ControlSection(stringResource(R.string.dev_section_flows)) {
            ControlActionRow(
                label = stringResource(R.string.dev_replay_first_launch),
                value = listOf(
                    stringResource(R.string.dev_section_flows),
                    stringResource(R.string.dev_section_catalog),
                ).joinToString(" · "),
                actionLabel = stringResource(R.string.dev_action_replay),
                onClick = {
                    pendingDestructive = DeveloperDestructiveAction(
                        title = context.getString(R.string.dev_replay_first_launch),
                        body = listOf(
                            R.string.dev_reset_setup,
                            R.string.dev_reset_stream_guide,
                            R.string.dev_reset_controller_prompt,
                            R.string.dev_reset_migrations,
                            R.string.dev_reset_browsing,
                        ).joinToString("\n") { context.getString(it) },
                        confirmLabel = context.getString(R.string.dev_action_replay),
                    ) {
                        update({ it.replayingFirstLaunch() }, context.getString(R.string.dev_toast_first_launch))
                    }
                },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_reset_setup),
                value = stringResource(R.string.dev_reset_setup_desc),
                actionLabel = stringResource(R.string.dev_action_reset),
                onClick = { update({ it.restartingSetupFlow() }, context.getString(R.string.dev_toast_setup)) },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_reset_stream_guide),
                value = stringResource(R.string.dev_reset_stream_guide_desc),
                actionLabel = stringResource(R.string.dev_action_reset),
                onClick = { update({ it.resettingStreamGuide() }, context.getString(R.string.dev_toast_stream_guide)) },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_reset_controller_prompt),
                value = stringResource(R.string.dev_reset_controller_prompt_desc),
                actionLabel = stringResource(R.string.dev_action_reset),
                onClick = {
                    update({ it.resettingControllerPrompt() }, context.getString(R.string.dev_toast_controller_prompt))
                },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_reset_migrations),
                value = stringResource(R.string.dev_reset_migrations_desc),
                actionLabel = stringResource(R.string.dev_action_reset),
                onClick = { update({ it.resettingProfileMigrations() }, context.getString(R.string.dev_toast_migrations)) },
            )
        }

        ControlSection(stringResource(R.string.dev_section_catalog)) {
            ControlActionRow(
                label = stringResource(R.string.dev_clear_catalog_cache),
                value = stringResource(R.string.dev_clear_catalog_cache_desc),
                actionLabel = stringResource(R.string.dev_action_clear),
                onClick = viewModel::clearCatalogCache,
            )
            ControlActionRow(
                label = stringResource(R.string.dev_refresh_catalog),
                value = stringResource(R.string.dev_refresh_catalog_desc),
                actionLabel = stringResource(R.string.dev_action_run),
                onClick = viewModel::refreshGames,
            )
            ControlActionRow(
                label = stringResource(R.string.dev_reset_browsing),
                value = stringResource(R.string.dev_reset_browsing_desc),
                actionLabel = stringResource(R.string.dev_action_reset),
                onClick = { update({ it.resettingCatalogBrowsing() }, context.getString(R.string.dev_toast_browsing)) },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_clear_store_preferences),
                value = stringResource(R.string.dev_clear_store_preferences_desc),
                actionLabel = stringResource(R.string.dev_action_clear),
                onClick = { update({ it.clearingStorePreferences() }, context.getString(R.string.dev_toast_store_preferences)) },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_clear_favorites),
                value = stringResource(R.string.dev_clear_favorites_count, settings.favoriteGameIds.size),
                actionLabel = stringResource(R.string.dev_action_clear),
                onClick = {
                    pendingDestructive = DeveloperDestructiveAction(
                        title = context.getString(R.string.dev_clear_favorites),
                        body = context.getString(R.string.dev_clear_favorites_confirm),
                        confirmLabel = context.getString(R.string.dev_action_clear),
                    ) {
                        update({ it.clearingFavorites() }, context.getString(R.string.dev_toast_favorites))
                    }
                },
            )
            if (BuildConfig.LOCAL_APP_LAUNCHER_SUPPORTED) {
                ControlActionRow(
                    label = stringResource(R.string.dev_clear_local_apps),
                    value = stringResource(R.string.dev_clear_local_apps_count, settings.localAppPackageNames.size),
                    actionLabel = stringResource(R.string.dev_action_clear),
                    onClick = { update({ it.clearingLocalAppShelf() }, context.getString(R.string.dev_toast_local_apps)) },
                )
            }
        }

        ControlSection(stringResource(R.string.dev_section_stream)) {
            ControlActionRow(
                label = stringResource(R.string.dev_apply_recommended),
                value = state.recommendedStreamSettings?.recommendationSummary()
                    ?: stringResource(R.string.setup_streaming_measuring),
                actionLabel = stringResource(R.string.dev_action_apply),
                onClick = {
                    viewModel.applyStreamPreset(StreamPreset.Recommended)
                    Toast.makeText(context, context.getString(R.string.dev_toast_recommended), Toast.LENGTH_SHORT).show()
                },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_reset_touch_layout),
                value = stringResource(R.string.dev_reset_touch_layout_desc),
                actionLabel = stringResource(R.string.dev_action_reset),
                onClick = { update({ it.resettingTouchLayout() }, context.getString(R.string.dev_toast_touch_layout)) },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_refresh_servers),
                value = stringResource(R.string.dev_refresh_servers_desc),
                actionLabel = stringResource(R.string.dev_action_run),
                onClick = viewModel::refreshPrintedWasteQueues,
            )
        }

        ControlSection(stringResource(R.string.dev_section_interface)) {
            ControlActionRow(
                label = stringResource(R.string.dev_reset_interface),
                value = stringResource(R.string.dev_reset_interface_desc),
                actionLabel = stringResource(R.string.dev_action_reset),
                onClick = { update({ it.resettingInterface() }, context.getString(R.string.dev_toast_interface)) },
            )
        }

        ControlSection(stringResource(R.string.dev_section_diagnostics)) {
            DeveloperEnvironmentCard(state)
            ControlActionRow(
                label = stringResource(R.string.dev_copy_diagnostics),
                value = stringResource(R.string.dev_copy_diagnostics_desc),
                actionLabel = stringResource(R.string.dev_action_copy),
                onClick = {
                    scope.launch {
                        clipboard.setText(AnnotatedString(viewModel.sanitizedDebugLogText()))
                        Toast.makeText(context, context.getString(R.string.dev_toast_diagnostics_copied), Toast.LENGTH_SHORT).show()
                    }
                },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_copy_environment),
                value = stringResource(R.string.dev_copy_environment_desc),
                actionLabel = stringResource(R.string.dev_action_copy),
                onClick = {
                    clipboard.setText(AnnotatedString(developerEnvironmentSummary(context, state)))
                    Toast.makeText(context, context.getString(R.string.dev_toast_environment_copied), Toast.LENGTH_SHORT).show()
                },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_check_update),
                value = stringResource(R.string.dev_check_update_desc),
                actionLabel = stringResource(R.string.dev_action_run),
                onClick = viewModel::checkAndroidUpdate,
            )
        }

        ControlSection(stringResource(R.string.dev_section_danger)) {
            ControlActionRow(
                label = stringResource(R.string.dev_sign_out_all),
                value = stringResource(R.string.dev_sign_out_all_desc, state.savedAccounts.size),
                actionLabel = stringResource(R.string.dev_action_run),
                onClick = {
                    pendingDestructive = DeveloperDestructiveAction(
                        title = context.getString(R.string.dev_sign_out_all),
                        body = context.getString(R.string.dev_sign_out_all_confirm),
                        confirmLabel = context.getString(R.string.dev_action_run),
                        run = viewModel::logoutAll,
                    )
                },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_wipe_app_data),
                value = stringResource(R.string.dev_wipe_app_data_desc),
                actionLabel = stringResource(R.string.dev_action_run),
                onClick = {
                    pendingDestructive = DeveloperDestructiveAction(
                        title = context.getString(R.string.dev_wipe_app_data),
                        body = context.getString(R.string.dev_wipe_app_data_confirm),
                        confirmLabel = context.getString(R.string.dev_action_run),
                        run = viewModel::resetSettings,
                    )
                },
            )
            ControlActionRow(
                label = stringResource(R.string.dev_lock),
                value = stringResource(R.string.dev_lock_desc),
                actionLabel = stringResource(R.string.dev_action_lock),
                onClick = { update({ it.lockingDeveloperOptions() }, context.getString(R.string.dev_toast_locked)) },
            )
        }
    }
}

private class DeveloperDestructiveAction(
    val title: String,
    val body: String,
    val confirmLabel: String,
    val run: () -> Unit,
)

@Composable
private fun DeveloperOptionsNotice() {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = OpenNowPalette.StatusNotice.copy(alpha = 0.10f),
        contentColor = SettingsText,
    ) {
        Column(
            Modifier.fillMaxWidth().padding(OpenNowSpacing.md),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                stringResource(R.string.dev_notice_title),
                color = OpenNowPalette.StatusNotice,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
            )
            Text(
                stringResource(R.string.dev_notice_body),
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun DeveloperEnvironmentCard(state: OpenNowUiState) {
    val context = LocalContext.current
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.md),
        color = SettingsPanelAlt,
    ) {
        Column(
            Modifier.fillMaxWidth().padding(OpenNowSpacing.md),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            developerEnvironmentRows(context, state).forEach { (label, value) ->
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                ) {
                    Text(
                        label,
                        Modifier.weight(1f),
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Text(
                        value,
                        Modifier.weight(1.4f),
                        color = SettingsText,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Medium,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

/**
 * The runtime facts a bug report almost always starts by asking for.
 *
 * Everything here is already in the diagnostics export; this is the same data at a glance so a
 * developer does not have to export and read a log to answer "which build, which tier, which
 * decoder".
 */
private fun developerEnvironmentRows(
    context: Context,
    state: OpenNowUiState,
): List<Pair<String, String>> {
    val tier = state.subscriptionInfo?.membershipTier?.takeIf { it.isNotBlank() }
        ?: state.authSession?.user?.membershipTier?.takeIf { it.isNotBlank() }
        ?: context.getString(R.string.dev_value_signed_out)
    return listOf(
        context.getString(R.string.dev_env_build) to "${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})",
        context.getString(R.string.dev_env_flavor) to buildString {
            append(if (BuildConfig.DEBUG) "debug" else "release")
            if (BuildConfig.PLAY_STORE_RELEASE) append(" · play")
        },
        context.getString(R.string.dev_env_device) to "${Build.MANUFACTURER} ${Build.MODEL}",
        context.getString(R.string.dev_env_android) to "${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})",
        context.getString(R.string.dev_env_profile) to context.getString(
            if (state.androidTvProfile) R.string.dev_value_tv else R.string.dev_value_handheld,
        ),
        context.getString(R.string.dev_env_tier) to tier,
        context.getString(R.string.dev_env_provider) to
            (state.authSession?.provider?.code ?: context.getString(R.string.dev_value_none)),
        context.getString(R.string.dev_env_stream) to state.settings.stream.recommendationSummary(),
        context.getString(R.string.dev_env_decoder) to (
            state.codecReport?.capabilities
                ?.filter { it.hardwareDecoder }
                ?.joinToString(", ") { it.codec.name }
                ?.takeIf { it.isNotBlank() }
                ?: context.getString(R.string.dev_value_none)
            ),
        context.getString(R.string.dev_env_catalog) to
            "${state.games.size} / ${state.libraryGames.size}",
    )
}

private fun developerEnvironmentSummary(context: Context, state: OpenNowUiState): String =
    developerEnvironmentRows(context, state).joinToString("\n") { (label, value) -> "$label: $value" }
