package com.opencloudgaming.opennow

import android.graphics.Bitmap
import android.net.Uri
import android.os.Looper
import android.os.SystemClock
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.compose.ui.text.font.AndroidFont
import androidx.compose.ui.text.font.FontListFontFamily
import com.opencloudgaming.opennow.ui.theme.Inter
import coil3.EventListener
import coil3.SingletonImageLoader
import coil3.decode.DataSource
import coil3.request.ImageRequest
import coil3.request.SuccessResult
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.webrtc.DataChannel

@RunWith(AndroidJUnit4::class)
class AnrWorkIsolationInstrumentedTest {
    @Test
    fun bundledInterWeightsLoadFromMainThroughTheWorkerBoundary() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        for (font in (Inter as FontListFontFamily).fonts) {
            val androidFont = font as AndroidFont
            // The loader asserts its worker invariant before touching ResourcesCompat or Paint.
            val typeface = withContext(Dispatchers.Main) {
                androidFont.typefaceLoader.awaitLoad(context, androidFont)
            }
            assertNotNull("Bundled Inter weight ${font.weight} must load", typeface)
        }
    }

    @Test
    fun androidUriMappingRunsOffMainIncludingMemoryCacheHits() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File.createTempFile("artwork-anr-", ".png", context.cacheDir)
        val bitmap = Bitmap.createBitmap(32, 32, Bitmap.Config.ARGB_8888)
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
        val mapCalls = AtomicInteger()
        val mappedOnMain = AtomicBoolean()
        // Retain the actual application defaults and cache; attach only a thread observer.
        val loader = SingletonImageLoader.get(context).newBuilder()
            .eventListener(object : EventListener() {
                override fun mapStart(request: ImageRequest, input: Any) {
                    mapCalls.incrementAndGet()
                    if (Looper.myLooper() == Looper.getMainLooper()) mappedOnMain.set(true)
                }
            })
            .build()
        try {
            val request = ImageRequest.Builder(context).data(Uri.fromFile(file)).size(32, 32).build()
            val first = withContext(Dispatchers.Main) { loader.execute(request) }
            assertTrue("Local artwork must still decode: $first", first is SuccessResult)
            val second = withContext(Dispatchers.Main) { loader.execute(request) }
            assertTrue(second is SuccessResult)
            assertEquals(DataSource.MEMORY_CACHE, (second as SuccessResult).dataSource)
            assertEquals(2, mapCalls.get())
            assertFalse("Mapping must not run in Compose's measure/main thread", mappedOnMain.get())
        } finally {
            loader.shutdown()
            file.delete()
        }
    }

    @Test
    fun rejectedOrThrowingInputSendNeverSwitchesToMainAndKeepsQueueBounded() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        for (throwFirst in listOf(false, true)) {
            val secondSendEntered = CountDownLatch(1)
            val releaseSend = CountDownLatch(1)
            val jniOnMain = AtomicBoolean()
            val calls = AtomicInteger()
            // Override every invoked JNI method. The sentinel handle never crosses into native.
            val channel = object : DataChannel(1L) {
                private fun recordThread() {
                    if (Looper.myLooper() == Looper.getMainLooper()) jniOnMain.set(true)
                }
                override fun state(): State { recordThread(); return State.OPEN }
                override fun bufferedAmount(): Long { recordThread(); return 0L }
                override fun send(buffer: Buffer): Boolean {
                    recordThread()
                    if (calls.incrementAndGet() == 1) {
                        if (throwFirst) throw IllegalStateException("injected transport failure")
                        return false
                    }
                    secondSendEntered.countDown()
                    releaseSend.await(3, TimeUnit.SECONDS)
                    return true
                }
            }
            lateinit var client: NativeStreamClient
            instrumentation.runOnMainSync {
                WebRtcRuntime.ensureInitialized(instrumentation.targetContext)
                client = NativeStreamClient(instrumentation.targetContext, false, {}, {})
            }
            fun field(name: String) = NativeStreamClient::class.java.getDeclaredField(name).apply { isAccessible = true }
            val pending = field("pendingInputSends").get(client) as AtomicInteger
            val send = NativeStreamClient::class.java.getDeclaredMethod(
                "sendInput", ByteArray::class.java, Boolean::class.javaPrimitiveType,
                Boolean::class.javaPrimitiveType, String::class.java,
            ).apply { isAccessible = true }
            fun sendPacket() = send.invoke(client, ByteArray(8), false, true, "anr.regression") as Boolean
            try {
                field("reliableInput").set(client, channel)
                field("reliableInputState").set(client, DataChannel.State.OPEN)
                instrumentation.runOnMainSync { assertTrue(sendPacket()) }
                val deadline = SystemClock.elapsedRealtime() + 3_000
                while (pending.get() != 0 && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(5)
                assertEquals("First failed send must drain", 0, pending.get())
                instrumentation.runOnMainSync { assertTrue(sendPacket()) }
                assertTrue(secondSendEntered.await(3, TimeUnit.SECONDS))
                // A stalled native send must leave Main responsive and retain the queue limit.
                instrumentation.runOnMainSync {
                    repeat(255) { assertTrue(sendPacket()) }
                    assertFalse("Queue must reject excess packets", sendPacket())
                }
                assertFalse("Transport failure must never enable UI-thread JNI", jniOnMain.get())
            } finally {
                releaseSend.countDown()
                instrumentation.runOnMainSync { client.release() }
            }
        }
    }
}
