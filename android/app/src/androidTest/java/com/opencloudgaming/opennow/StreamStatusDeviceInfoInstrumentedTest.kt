package com.opencloudgaming.opennow

import android.Manifest
import android.content.pm.PackageManager
import android.text.format.DateFormat
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.util.Date

class StreamStatusDeviceInfoInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun show(metrics: StreamStatsMetrics) {
        compose.setContent {
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                StreamStatsPill(
                    streamStats = StreamRuntimeStats(),
                    streamSettings = StreamSettings(),
                    style = StreamStatsStyle.Compact,
                    metrics = metrics,
                    sessionStartBatteryPercent = null,
                    sessionStartedAtMs = 0L,
                    sessionNowMs = 0L,
                    backgroundAlpha = 0.6f,
                    serverLocation = null,
                    keyboardButtonEnabled = false,
                    onKeyboardOpen = {},
                )
            }
        }
    }

    @Test
    fun clockRendersWithoutPlaytimeOrSessionTimer() {
        val context = compose.activity
        show(StreamStatsMetrics(fps = false, ping = false, battery = false, connection = false, clock = true))
        // Allow a minute rollover between rendering and the assertion.
        val now = System.currentTimeMillis()
        val descriptions = listOf(now, now - 60_000L).map {
            context.getString(R.string.stream_stats_clock_cd, DateFormat.getTimeFormat(context).format(Date(it)))
        }
        compose.waitUntil(5_000) {
            descriptions.any { compose.onAllNodesWithContentDescription(it).fetchSemanticsNodes().isNotEmpty() }
        }
    }

    @Test
    fun deniedSsidReadIsSafeAndDefaultsStayOff() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) != PackageManager.PERMISSION_GRANTED)
        assertFalse(AppSettings().streamStatsMetrics.clock)
        assertFalse(AppSettings().streamStatsMetrics.wifiSsid)
        assertNull(readStreamWifiName(context))
    }

    @Test
    fun connectedWifiNameAppearsWithPrecisePermission() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED)
        val ssid = readStreamWifiName(context)
        assertTrue("Connected emulator Wi-Fi should have a readable SSID", !ssid.isNullOrBlank())
        show(StreamStatsMetrics(fps = false, ping = false, battery = false, connection = false, wifiSsid = true))
        val description = context.getString(R.string.stream_stats_wifi_ssid_cd, ssid)
        compose.waitUntil(10_000) {
            compose.onAllNodesWithContentDescription(description).fetchSemanticsNodes().isNotEmpty()
        }
    }
}
