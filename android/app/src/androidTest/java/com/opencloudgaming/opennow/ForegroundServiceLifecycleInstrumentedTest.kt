package com.opencloudgaming.opennow

import android.app.ActivityManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.SystemClock
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.SdkSuppress
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@SdkSuppress(minSdkVersion = 26)
class ForegroundServiceLifecycleInstrumentedTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext

    @Test
    fun stopOnlyColdStartsAndRapidRestartsHonorForegroundHandshake() {
        // Keep a visible activity so this exercises the handshake, without relying on an
        // exemption from Android 12+ restrictions on starting services in the background.
        ActivityScenario.launch(MainActivity::class.java).use {
            val services = listOf(
                Triple(AndroidStreamKeepAliveService::class.java,
                    "com.opencloudgaming.opennow.stream.START", "com.opencloudgaming.opennow.stream.STOP"),
                Triple(AndroidQueueStatusService::class.java,
                    "com.opencloudgaming.opennow.queue.UPDATE", "com.opencloudgaming.opennow.queue.STOP"),
            )
            try {
                for ((service, start, stop) in services) {
                    context.stopService(Intent(context, service))
                    awaitService(service, present = false)
                    // STOP can be the first command after Android has destroyed an instance.
                    context.startForegroundService(Intent(context, service).setAction(stop))
                    instrumentation.waitForIdleSync()
                    awaitService(service, present = false)
                    repeat(10) {
                        context.startForegroundService(Intent(context, service).setAction(start))
                        awaitService(service, present = true)
                        context.startService(Intent(context, service).setAction(stop))
                        context.startForegroundService(Intent(context, service).setAction(start))
                        awaitService(service, present = true)
                    }
                    context.startService(Intent(context, service).setAction(stop))
                    awaitService(service, present = false)
                }
                // An unfulfilled foreground-start timeout is delivered asynchronously; allow
                // it to fire rather than ending the test immediately after stopSelf().
                SystemClock.sleep(12_000)
            } finally {
                services.forEach { (service, _, _) -> context.stopService(Intent(context, service)) }
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun awaitService(service: Class<out Service>, present: Boolean) {
        val manager = context.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
        val deadline = SystemClock.elapsedRealtime() + 5_000
        do {
            val running = manager.getRunningServices(Int.MAX_VALUE).firstOrNull {
                it.service.className == service.name
            }
            if (if (present) running?.foreground == true else running == null) return
            SystemClock.sleep(50)
        } while (SystemClock.elapsedRealtime() < deadline)
        assertTrue("${service.simpleName} foreground=$present was not reached", false)
    }
}
