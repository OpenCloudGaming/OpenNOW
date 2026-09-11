package com.opencloudgaming.opennow

import android.content.Context
import android.os.Build
import android.telephony.PhoneStateListener
import android.telephony.TelephonyCallback
import android.telephony.TelephonyDisplayInfo
import android.telephony.TelephonyManager

/**
 * Owns the carrier-facing cellular generation shown by compact stream stats.
 * TelephonyDisplayInfo is intentionally used instead of guessing from bandwidth:
 * it includes carrier display overrides such as 5G NSA and 5G+.
 */
internal object CellularNetworkStatus {
    @Volatile
    private var displayLabel: String? = null

    @Volatile
    private var monitoringStarted = false

    private var retainedCallback: Any? = null

    fun displayLabel(context: Context): String? {
        startMonitoring(context.applicationContext)
        return displayLabel
    }

    fun signalBars(context: Context): Int? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return null
        val telephony = context.getSystemService(Context.TELEPHONY_SERVICE) as? TelephonyManager ?: return null
        return runCatching { telephony.signalStrength?.level?.coerceIn(0, 4) }.getOrNull()
    }

    @Synchronized
    private fun startMonitoring(context: Context) {
        if (monitoringStarted || Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        val telephony = context.getSystemService(Context.TELEPHONY_SERVICE) as? TelephonyManager ?: return
        monitoringStarted = true
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                registerModernCallback(context, telephony)
            } else {
                registerAndroid11Listener(context, telephony)
            }
        }.onFailure {
            monitoringStarted = false
            retainedCallback = null
        }
    }

    @android.annotation.TargetApi(Build.VERSION_CODES.S)
    private fun registerModernCallback(context: Context, telephony: TelephonyManager) {
        val callback = object : TelephonyCallback(), TelephonyCallback.DisplayInfoListener {
            override fun onDisplayInfoChanged(displayInfo: TelephonyDisplayInfo) {
                displayLabel = cellularGenerationLabel(displayInfo.networkType, displayInfo.overrideNetworkType)
            }
        }
        retainedCallback = callback
        telephony.registerTelephonyCallback(context.mainExecutor, callback)
    }

    @Suppress("DEPRECATION")
    @android.annotation.TargetApi(Build.VERSION_CODES.R)
    private fun registerAndroid11Listener(context: Context, telephony: TelephonyManager) {
        // Diagnostics may start on an IO worker, which has no Looper. Deliver callbacks through
        // the main executor just like the Android 12+ registration path.
        val listener = object : PhoneStateListener(context.mainExecutor) {
            override fun onDisplayInfoChanged(displayInfo: TelephonyDisplayInfo) {
                displayLabel = cellularGenerationLabel(displayInfo.networkType, displayInfo.overrideNetworkType)
            }
        }
        retainedCallback = listener
        telephony.listen(listener, PhoneStateListener.LISTEN_DISPLAY_INFO_CHANGED)
    }
}

internal fun cellularGenerationLabel(networkType: Int, overrideNetworkType: Int): String? = when (overrideNetworkType) {
    TelephonyDisplayInfo.OVERRIDE_NETWORK_TYPE_NR_ADVANCED,
    TelephonyDisplayInfo.OVERRIDE_NETWORK_TYPE_NR_NSA_MMWAVE,
    -> "5G+"
    TelephonyDisplayInfo.OVERRIDE_NETWORK_TYPE_NR_NSA -> "5G"
    TelephonyDisplayInfo.OVERRIDE_NETWORK_TYPE_LTE_ADVANCED_PRO,
    TelephonyDisplayInfo.OVERRIDE_NETWORK_TYPE_LTE_CA,
    -> "LTE+"
    else -> when (networkType) {
        TelephonyManager.NETWORK_TYPE_NR -> "5G"
        TelephonyManager.NETWORK_TYPE_LTE -> "LTE"
        TelephonyManager.NETWORK_TYPE_HSPAP -> "H+"
        TelephonyManager.NETWORK_TYPE_HSPA,
        TelephonyManager.NETWORK_TYPE_HSDPA,
        TelephonyManager.NETWORK_TYPE_HSUPA,
        -> "H"
        TelephonyManager.NETWORK_TYPE_UMTS,
        TelephonyManager.NETWORK_TYPE_TD_SCDMA,
        TelephonyManager.NETWORK_TYPE_EVDO_0,
        TelephonyManager.NETWORK_TYPE_EVDO_A,
        TelephonyManager.NETWORK_TYPE_EVDO_B,
        TelephonyManager.NETWORK_TYPE_EHRPD,
        -> "3G"
        TelephonyManager.NETWORK_TYPE_EDGE -> "E"
        TelephonyManager.NETWORK_TYPE_GPRS,
        TelephonyManager.NETWORK_TYPE_GSM,
        TelephonyManager.NETWORK_TYPE_CDMA,
        TelephonyManager.NETWORK_TYPE_1xRTT,
        -> "2G"
        else -> null
    }
}
