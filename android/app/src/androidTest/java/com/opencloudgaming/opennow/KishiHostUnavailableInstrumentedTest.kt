package com.opencloudgaming.opennow

import android.content.Context
import android.content.ContextWrapper
import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assume.assumeTrue
import org.junit.Test

class KishiHostUnavailableInstrumentedTest {
    @Test fun openingOnADeviceWithoutUsbServiceDoesNotCrashOrRequestPermission() {
        assumeTrue(Build.VERSION.SDK_INT >= 26)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = object : ContextWrapper(instrumentation.targetContext) {
            override fun getSystemService(name: String): Any? =
                if (name == Context.USB_SERVICE) null else super.getSystemService(name)
        }
        instrumentation.runOnMainSync {
            val manager = KishiHapticsManager(context)
            try {
                manager.onAppForegrounded()
                manager.configure(true, 40)
                manager.onAppBackgrounded()
                manager.onAppForegrounded()
                assertEquals("USB host unavailable on this device", manager.status.value)
                assertNull(manager.inputDeviceId())
            } finally {
                manager.configure(false, 40)
            }
        }
    }
}
