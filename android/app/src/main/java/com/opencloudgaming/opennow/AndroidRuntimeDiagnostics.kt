package com.opencloudgaming.opennow

import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import kotlin.math.roundToInt
import kotlinx.serialization.Serializable

@Serializable
internal data class AndroidDeviceDiagnosticsSnapshot(
    val manufacturer: String,
    val brand: String,
    val model: String,
    val deviceCodename: String,
    val product: String,
    val hardware: String,
    val board: String,
    val androidRelease: String,
    val androidCodename: String,
    val androidSdk: Int,
    val targetSdk: Int,
    val securityPatch: String,
    val supportedAbis: List<String>,
    val is64BitRuntime: Boolean,
    val processorCount: Int,
    val totalMemoryMiB: Long?,
    val lowRamDevice: Boolean?,
    val displayWidthPixels: Int,
    val displayHeightPixels: Int,
    val densityDpi: Int,
    val smallestScreenWidthDp: Int,
    val formFactor: String,
    val emulator: Boolean,
) {
    fun debugSummary(): String = buildString {
        appendLine(
            "device.identity manufacturer=$manufacturer brand=$brand model=$model " +
                "codename=$deviceCodename product=$product formFactor=$formFactor emulator=$emulator",
        )
        appendLine(
            "android.os release=$androidRelease codename=$androidCodename sdk=$androidSdk " +
                "targetSdk=$targetSdk securityPatch=$securityPatch",
        )
        appendLine(
            "device.hardware hardware=$hardware board=$board abis=${supportedAbis.joinToString("|").ifBlank { "unknown" }} " +
                "runtimeBits=${if (is64BitRuntime) 64 else 32} processors=$processorCount " +
                "memoryMiB=${totalMemoryMiB ?: "unknown"} lowRam=${lowRamDevice ?: "unknown"}",
        )
        append(
            "device.display pixels=${displayWidthPixels}x$displayHeightPixels densityDpi=$densityDpi " +
                "smallestWidthDp=$smallestScreenWidthDp",
        )
    }
}

internal object AndroidDeviceDiagnostics {
    fun snapshot(context: Context): AndroidDeviceDiagnosticsSnapshot {
        val appContext = context.applicationContext
        val resources = appContext.resources
        val configuration = resources.configuration
        val metrics = resources.displayMetrics
        val activityManager = appContext.getSystemService(Context.ACTIVITY_SERVICE) as? ActivityManager
        val totalMemoryMiB = activityManager?.let { manager ->
            runCatching {
                val info = ActivityManager.MemoryInfo()
                manager.getMemoryInfo(info)
                info.totalMem.takeIf { it > 0L }?.div(DEVICE_BYTES_PER_MEBIBYTE)
            }.getOrNull()
        }
        val smallestWidthDp = configuration.smallestScreenWidthDp.coerceAtLeast(0)
        val tv = isAndroidTvProfile(appContext)
        return AndroidDeviceDiagnosticsSnapshot(
            manufacturer = diagnosticBuildValue(Build.MANUFACTURER),
            brand = diagnosticBuildValue(Build.BRAND),
            model = diagnosticBuildValue(Build.MODEL),
            deviceCodename = diagnosticBuildValue(Build.DEVICE),
            product = diagnosticBuildValue(Build.PRODUCT),
            hardware = diagnosticBuildValue(Build.HARDWARE),
            board = diagnosticBuildValue(Build.BOARD),
            androidRelease = diagnosticBuildValue(Build.VERSION.RELEASE),
            androidCodename = diagnosticBuildValue(Build.VERSION.CODENAME),
            androidSdk = Build.VERSION.SDK_INT,
            targetSdk = appContext.applicationInfo.targetSdkVersion,
            securityPatch = diagnosticBuildValue(Build.VERSION.SECURITY_PATCH),
            supportedAbis = Build.SUPPORTED_ABIS.map(::diagnosticBuildValue),
            is64BitRuntime = android.os.Process.is64Bit(),
            processorCount = Runtime.getRuntime().availableProcessors().coerceAtLeast(1),
            totalMemoryMiB = totalMemoryMiB,
            lowRamDevice = activityManager?.isLowRamDevice,
            displayWidthPixels = metrics.widthPixels.coerceAtLeast(0),
            displayHeightPixels = metrics.heightPixels.coerceAtLeast(0),
            densityDpi = metrics.densityDpi.coerceAtLeast(0),
            smallestScreenWidthDp = smallestWidthDp,
            formFactor = androidDeviceFormFactor(tv, smallestWidthDp),
            emulator = isProbablyAndroidEmulator(),
        )
    }
}

internal fun androidDeviceFormFactor(androidTv: Boolean, smallestScreenWidthDp: Int): String = when {
    androidTv -> "tv"
    smallestScreenWidthDp >= 600 -> "tablet"
    else -> "phone"
}

private fun diagnosticBuildValue(value: String?): String =
    value
        ?.trim()
        ?.takeIf { it.isNotEmpty() }
        ?.replace(Regex("\\s+"), "_")
        ?.take(MAX_DEVICE_DIAGNOSTIC_VALUE_CHARS)
        ?: "unknown"

private fun isProbablyAndroidEmulator(): Boolean {
    val fingerprint = Build.FINGERPRINT.lowercase()
    val model = Build.MODEL.lowercase()
    val manufacturer = Build.MANUFACTURER.lowercase()
    val brand = Build.BRAND.lowercase()
    val device = Build.DEVICE.lowercase()
    val product = Build.PRODUCT.lowercase()
    return fingerprint.startsWith("generic") ||
        fingerprint.startsWith("unknown") ||
        model.contains("google_sdk") ||
        model.contains("emulator") ||
        model.contains("android sdk built for") ||
        manufacturer.contains("genymotion") ||
        (brand.startsWith("generic") && device.startsWith("generic")) ||
        product.contains("sdk") ||
        product.contains("emulator") ||
        product.contains("simulator")
}

@Serializable
internal data class AndroidRuntimeDiagnosticsSnapshot(
    val batteryPercent: Int? = null,
    val batteryCharging: Boolean = false,
    val batteryTemperatureC: Float? = null,
    val thermalStatus: AndroidThermalStatus = AndroidThermalStatus.Unknown,
    val networkKind: AndroidNetworkKind = AndroidNetworkKind.Unknown,
    val networkSignalBars: Int? = null,
    val cellularGeneration: String? = null,
    val networkDownstreamKbps: Int? = null,
    val wifiFrequencyMhz: Int? = null,
    val wifiBand: AndroidWifiBand = AndroidWifiBand.Unknown,
) {
    fun debugSummary(): String {
        val temperature = batteryTemperatureC?.let { "%.1f".format(java.util.Locale.US, it) } ?: "unknown"
        return "battery=${batteryPercent?.toString() ?: "unknown"} charging=$batteryCharging batteryTempC=$temperature thermal=${thermalStatus.logValue} network=${networkKind.logValue} generation=${cellularGeneration ?: "unknown"} bars=${networkSignalBars?.toString() ?: "unknown"} downKbps=${networkDownstreamKbps ?: 0} wifiMhz=${wifiFrequencyMhz ?: 0} wifiBand=${wifiBand.logValue}"
    }
}

@Serializable
enum class AndroidNetworkKind(val label: String, val logValue: String) {
    Wifi("WiFi", "wifi"),
    Cellular("Cell", "cellular"),
    Ethernet("LAN", "ethernet"),
    Other("Net", "other"),
    None("Off", "none"),
    Unknown("Net", "unknown"),
}

@Serializable
enum class AndroidWifiBand(val label: String, val logValue: String) {
    TwoPointFourGhz("2.4 GHz", "2.4ghz"),
    FiveGhz("5 GHz", "5ghz"),
    SixGhz("6 GHz", "6ghz"),
    Unknown("Wi-Fi", "unknown"),
}

internal fun androidWifiBandForFrequency(frequencyMhz: Int?): AndroidWifiBand = when (frequencyMhz) {
    in 2_400..2_500 -> AndroidWifiBand.TwoPointFourGhz
    in 4_900..5_900 -> AndroidWifiBand.FiveGhz
    in 5_925..7_125 -> AndroidWifiBand.SixGhz
    else -> AndroidWifiBand.Unknown
}

internal @Serializable
enum class AndroidThermalStatus(val logValue: String) {
    Unknown("unknown"),
    None("none"),
    Light("light"),
    Moderate("moderate"),
    Severe("severe"),
    Critical("critical"),
    Emergency("emergency"),
    Shutdown("shutdown"),
}

internal object AndroidRuntimeDiagnostics {
    fun snapshot(context: Context): AndroidRuntimeDiagnosticsSnapshot {
        val appContext = context.applicationContext
        val battery = readBattery(appContext)
        val network = readNetwork(appContext)
        return AndroidRuntimeDiagnosticsSnapshot(
            batteryPercent = battery.percent,
            batteryCharging = battery.charging,
            batteryTemperatureC = battery.temperatureC,
            thermalStatus = readThermalStatus(appContext),
            networkKind = network.kind,
            networkSignalBars = network.signalBars,
            cellularGeneration = network.cellularGeneration,
            networkDownstreamKbps = network.downstreamKbps,
            wifiFrequencyMhz = network.wifiFrequencyMhz,
            wifiBand = androidWifiBandForFrequency(network.wifiFrequencyMhz),
        )
    }

    fun networkSnapshot(context: Context): AndroidRuntimeDiagnosticsSnapshot {
        val network = readNetwork(context.applicationContext)
        return AndroidRuntimeDiagnosticsSnapshot(
            networkKind = network.kind,
            networkSignalBars = network.signalBars,
            cellularGeneration = network.cellularGeneration,
            networkDownstreamKbps = network.downstreamKbps,
            wifiFrequencyMhz = network.wifiFrequencyMhz,
            wifiBand = androidWifiBandForFrequency(network.wifiFrequencyMhz),
        )
    }

    private fun readBattery(context: Context): BatteryDiagnostics {
        val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED), Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("DEPRECATION")
            context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        }
        val level = intent?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
        val scale = intent?.getIntExtra(BatteryManager.EXTRA_SCALE, -1) ?: -1
        val percent = if (level >= 0 && scale > 0) {
            ((level / scale.toFloat()) * 100f).roundToInt().coerceIn(0, 100)
        } else {
            null
        }
        val batteryStatus = intent?.getIntExtra(BatteryManager.EXTRA_STATUS, BatteryManager.BATTERY_STATUS_UNKNOWN)
            ?: BatteryManager.BATTERY_STATUS_UNKNOWN
        val plugged = intent?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0
        val temperatureTenths = intent?.getIntExtra(BatteryManager.EXTRA_TEMPERATURE, Int.MIN_VALUE) ?: Int.MIN_VALUE
        return BatteryDiagnostics(
            percent = percent,
            charging = batteryStatus == BatteryManager.BATTERY_STATUS_CHARGING ||
                batteryStatus == BatteryManager.BATTERY_STATUS_FULL ||
                plugged != 0,
            temperatureC = temperatureTenths.takeIf { it != Int.MIN_VALUE }?.let { it / 10f },
        )
    }

    private fun readThermalStatus(context: Context): AndroidThermalStatus {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return AndroidThermalStatus.Unknown
        val powerManager = context.getSystemService(Context.POWER_SERVICE) as? PowerManager ?: return AndroidThermalStatus.Unknown
        return when (powerManager.currentThermalStatus) {
            PowerManager.THERMAL_STATUS_NONE -> AndroidThermalStatus.None
            PowerManager.THERMAL_STATUS_LIGHT -> AndroidThermalStatus.Light
            PowerManager.THERMAL_STATUS_MODERATE -> AndroidThermalStatus.Moderate
            PowerManager.THERMAL_STATUS_SEVERE -> AndroidThermalStatus.Severe
            PowerManager.THERMAL_STATUS_CRITICAL -> AndroidThermalStatus.Critical
            PowerManager.THERMAL_STATUS_EMERGENCY -> AndroidThermalStatus.Emergency
            PowerManager.THERMAL_STATUS_SHUTDOWN -> AndroidThermalStatus.Shutdown
            else -> AndroidThermalStatus.Unknown
        }
    }

    private fun readNetwork(context: Context): NetworkDiagnostics {
        val connectivity = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
        val capabilities = connectivity?.getNetworkCapabilities(connectivity.activeNetwork)
        val kind = when {
            capabilities == null -> AndroidNetworkKind.None
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> AndroidNetworkKind.Wifi
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> AndroidNetworkKind.Cellular
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> AndroidNetworkKind.Ethernet
            else -> AndroidNetworkKind.Other
        }
        return NetworkDiagnostics(
            kind = kind,
            signalBars = if (kind == AndroidNetworkKind.Cellular) {
                CellularNetworkStatus.signalBars(context) ?: networkBars(capabilities)
            } else {
                networkBars(capabilities)
            },
            cellularGeneration = if (kind == AndroidNetworkKind.Cellular) CellularNetworkStatus.displayLabel(context) else null,
            downstreamKbps = capabilities?.linkDownstreamBandwidthKbps?.takeIf { it > 0 },
            wifiFrequencyMhz = if (kind == AndroidNetworkKind.Wifi) {
                wifiFrequencyMhz(context, capabilities)
            } else {
                null
            },
        )
    }

    @Suppress("DEPRECATION")
    private fun wifiFrequencyMhz(context: Context, capabilities: NetworkCapabilities?): Int? {
        val networkInfo = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            capabilities?.transportInfo as? WifiInfo
        } else {
            null
        }
        // WIFI_SERVICE must be resolved from the application context: on API < 24 holding it from an
        // Activity context leaks that Activity, and this module still ships to API 23.
        val wifiInfo = networkInfo
            ?: (context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager)?.connectionInfo
        return wifiInfo?.frequency?.takeIf { it > 0 }
    }

    private fun networkBars(capabilities: NetworkCapabilities?): Int? {
        if (capabilities == null) return 0
        val signalBars = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            networkBarsFromSignal(capabilities.signalStrength)
        } else {
            null
        }
        return signalBars ?: networkBarsFromBandwidth(capabilities.linkDownstreamBandwidthKbps)
    }

    private fun networkBarsFromSignal(signalStrength: Int): Int? {
        if (signalStrength == Int.MIN_VALUE) return null
        return when {
            signalStrength in 0..4 -> signalStrength
            signalStrength >= -55 -> 4
            signalStrength >= -67 -> 3
            signalStrength >= -80 -> 2
            else -> 1
        }
    }

    private fun networkBarsFromBandwidth(downstreamKbps: Int): Int? = when {
        downstreamKbps >= 25_000 -> 4
        downstreamKbps >= 10_000 -> 3
        downstreamKbps >= 3_000 -> 2
        downstreamKbps > 0 -> 1
        else -> null
    }

    private data class BatteryDiagnostics(
        val percent: Int?,
        val charging: Boolean,
        val temperatureC: Float?,
    )

    private data class NetworkDiagnostics(
        val kind: AndroidNetworkKind,
        val signalBars: Int?,
        val cellularGeneration: String?,
        val downstreamKbps: Int?,
        val wifiFrequencyMhz: Int?,
    )
}

private const val DEVICE_BYTES_PER_MEBIBYTE = 1024L * 1024L
private const val MAX_DEVICE_DIAGNOSTIC_VALUE_CHARS = 120
