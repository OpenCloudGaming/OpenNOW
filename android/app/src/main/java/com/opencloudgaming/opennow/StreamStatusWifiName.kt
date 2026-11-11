package com.opencloudgaming.opennow

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource

/** Only called by status-bar customization after the user enables the SSID item. */
@Composable
internal fun rememberRequestStreamWifiNamePermission(): () -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { }
    return {
        if (context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) != PackageManager.PERMISSION_GRANTED) {
            // Android 12+ requires coarse and fine to be requested together for precise access.
            launcher.launch(arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION, Manifest.permission.ACCESS_FINE_LOCATION))
        }
    }
}

@Composable
internal fun StreamWifiNamePermissionHint() {
    Text(
        text = stringResource(R.string.stream_statusbar_wifi_ssid_permission_hint),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** Kept separate from diagnostics so a network name never enters session reports or logs. */
@Suppress("DEPRECATION")
internal fun readStreamWifiName(context: Context): String? {
    if (context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) != PackageManager.PERMISSION_GRANTED) return null
    return runCatching {
        // getConnectionInfo is also used by the existing runtime diagnostics, supports API 24+,
        // and lets Android redact the SSID if device Location is off. No scan is needed.
        val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
        normalizeStreamWifiName(wifi?.connectionInfo?.ssid)
    }.getOrNull()
}

internal fun normalizeStreamWifiName(ssid: String?): String? {
    val name = ssid?.removeSurrounding("\"") ?: return null
    return name.takeUnless { it.isBlank() || it.equals("<unknown ssid>", ignoreCase = true) }
}
