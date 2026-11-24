package com.opencloudgaming.opennow

import android.content.Context
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource

internal const val APP_MONETIZATION_PREFS_NAME = "opennow_monetization"
private const val CUSTOMIZATION_UNLOCKED_KEY = "customization_unlocked"

/** Installation-level grants and acknowledgements stay outside editable settings and exports. */
internal class AppMonetizationPreferences(context: Context) {
    private val prefs = context.getSharedPreferences(APP_MONETIZATION_PREFS_NAME, Context.MODE_PRIVATE)

    val customizationUnlocked: Boolean
        get() = prefs.getBoolean(CUSTOMIZATION_UNLOCKED_KEY, false)

    val supportNoticeDismissed: Boolean
        get() = prefs.getBoolean("support_notice_dismissed", false)

    fun unlockCustomization() {
        prefs.edit().putBoolean(CUSTOMIZATION_UNLOCKED_KEY, true).apply()
    }

    fun resetCustomizationUnlock() {
        prefs.edit().remove(CUSTOMIZATION_UNLOCKED_KEY).apply()
    }

    fun dismissSupportNotice() {
        prefs.edit().putBoolean("support_notice_dismissed", true).apply()
    }

    fun flushBeforeReset() {
        // The reset explicitly kills the process and cannot rely on lifecycle-driven disk writes.
        prefs.edit().commit()
    }
}

/** Keep the user's existing appearance when an unrelated settings edit is applied while locked. */
internal fun AppSettings.withAppearanceFrom(saved: AppSettings): AppSettings = copy(
    uiAccent = saved.uiAccent,
    dynamicColor = saved.dynamicColor,
    selectionEffectColors = saved.selectionEffectColors,
    nerdCatalogBackground = saved.nerdCatalogBackground,
    ambientBackgroundEnabled = saved.ambientBackgroundEnabled,
    systemWallpaperBackground = saved.systemWallpaperBackground,
    catalogBackgroundPreset = saved.catalogBackgroundPreset,
    nerdCatalogBackgroundUri = saved.nerdCatalogBackgroundUri,
    navigationRailBackgroundOpacity = saved.navigationRailBackgroundOpacity,
)

internal val OpenNowUiState.canCustomizeAppearance: Boolean
    get() = androidTvProfile || appearanceCustomizationUnlocked

@Composable
internal fun SupportNoticeDialog(onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.support_notice_title)) },
        text = {
            Text(
                text = stringResource(R.string.support_notice_body),
                modifier = Modifier.verticalScroll(rememberScrollState()),
            )
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.action_dismiss))
            }
        },
    )
}
