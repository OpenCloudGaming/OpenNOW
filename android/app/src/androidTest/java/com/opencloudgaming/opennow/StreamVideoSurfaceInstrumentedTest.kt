package com.opencloudgaming.opennow

import android.graphics.Color
import android.view.View
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.test.platform.app.InstrumentationRegistry
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.webrtc.EglBase
import org.webrtc.GlRectDrawer
import org.webrtc.JavaI420Buffer
import org.webrtc.PeerConnectionFactory
import org.webrtc.RendererCommon
import org.webrtc.VideoFrame

class StreamVideoSurfaceInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun stretchFillsTheViewportAcrossDecodedAspectChangesAndCanBeTurnedOff() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        PeerConnectionFactory.initialize(PeerConnectionFactory.InitializationOptions.builder(context).createInitializationOptions())
        val egl = EglBase.create()
        val stretch = mutableStateOf(false)
        var createdSurface: StreamVideoSurface? = null
        try {
            compose.setContent {
                Box(Modifier.size(160.dp, 72.dp).background(androidx.compose.ui.graphics.Color.Black)) {
                    AndroidView(
                        modifier = Modifier.fillMaxSize(),
                        factory = {
                            StreamVideoSurface(it, hdr = false).also { view ->
                                createdSurface = view
                                view.init(egl.eglBaseContext, object : RendererCommon.RendererEvents {
                                    override fun onFirstFrameRendered() = Unit
                                    override fun onFrameResolutionChanged(width: Int, height: Int, rotation: Int) = Unit
                                }, EglBase.CONFIG_PLAIN, GlRectDrawer())
                                view.setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
                            }
                        },
                        update = { it.updateStretchToFit(stretch.value) },
                    )
                }
            }
            val surface = compose.runOnIdle { checkNotNull(createdSurface) }
            compose.waitUntil(5_000) { surface.holder.surface.isValid && surface.width > 0 }
            val origin = IntArray(2)
            compose.runOnIdle { surface.getLocationOnScreen(origin) }
            val centerX = origin[0] + surface.width / 2
            val centerY = origin[1] + surface.height / 2
            val nearLeft = origin[0] + 3
            val nearTop = origin[1] + 3

            fun awaitVideoEdges(leftFilled: Boolean, topFilled: Boolean) {
                compose.waitUntil(5_000) {
                    val screenshot = instrumentation.uiAutomation.takeScreenshot() ?: return@waitUntil false
                    try {
                        val center = Color.red(screenshot.getPixel(centerX, centerY))
                        val left = Color.red(screenshot.getPixel(nearLeft, centerY))
                        val top = Color.red(screenshot.getPixel(centerX, nearTop))
                        center > 200 &&
                            (if (leftFilled) left > 200 else left < 20) &&
                            (if (topFilled) top > 200 else top < 20)
                    } finally {
                        screenshot.recycle()
                    }
                }
            }

            submitFrame(surface, 160, 90, white = true)
            awaitVideoEdges(leftFilled = false, topFilled = true)
            compose.runOnIdle { stretch.value = true }
            awaitVideoEdges(leftFilled = true, topFilled = true)

            submitFrame(surface, 256, 90, white = true)
            compose.runOnIdle { assertEquals(256 to 90, surface.currentDecodedSize()) }
            awaitVideoEdges(leftFilled = true, topFilled = true)
            compose.runOnIdle { stretch.value = false }
            awaitVideoEdges(leftFilled = true, topFilled = false)
        } finally {
            withContext(Dispatchers.IO) {
                createdSurface?.release()
                egl.release()
            }
        }
    }

    @Test fun retiringSurfaceDropsFramesBeforeReleasingItsEglWindow() = runBlocking {
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        PeerConnectionFactory.initialize(PeerConnectionFactory.InitializationOptions.builder(context).createInitializationOptions())
        val egl = EglBase.create()
        var createdSurface: StreamVideoSurface? = null
        try {
            compose.setContent {
                AndroidView(
                    modifier = Modifier.size(160.dp, 90.dp),
                    factory = {
                        StreamVideoSurface(it, hdr = false).also { view ->
                            createdSurface = view
                            view.init(egl.eglBaseContext, object : RendererCommon.RendererEvents {
                                override fun onFirstFrameRendered() = Unit
                                override fun onFrameResolutionChanged(width: Int, height: Int, rotation: Int) = Unit
                            }, EglBase.CONFIG_PLAIN, GlRectDrawer())
                        }
                    },
                )
            }
            val surface = compose.runOnIdle { checkNotNull(createdSurface) }
            compose.waitUntil(5_000) { surface.holder.surface.isValid }
            submitFrame(surface, 160, 90)
            assertEquals(160 to 90, surface.currentDecodedSize())

            compose.runOnIdle {
                surface.prepareForRelease()
                assertFalse(surface.acceptingFrames)
                assertEquals(View.VISIBLE, surface.visibility)
                assertTrue(surface.holder.surface.isValid)
            }
            submitFrame(surface, 128, 72)
            assertEquals(160 to 90, surface.currentDecodedSize())

            withContext(Dispatchers.IO) { surface.release() }
            compose.waitUntil(5_000) { surface.visibility == View.GONE }
        } finally {
            withContext(Dispatchers.IO) {
                createdSurface?.release()
                egl.release()
            }
        }
    }

    private fun submitFrame(surface: StreamVideoSurface, width: Int, height: Int, white: Boolean = false) {
        val buffer = JavaI420Buffer.allocate(width, height)
        if (white) {
            fun fill(plane: ByteBuffer, value: Int) {
                for (index in 0 until plane.capacity()) plane.put(index, value.toByte())
            }
            fill(buffer.dataY, 235)
            fill(buffer.dataU, 128)
            fill(buffer.dataV, 128)
        }
        val frame = VideoFrame(buffer, 0, System.nanoTime())
        try {
            surface.onFrame(frame)
        } finally {
            frame.release()
        }
    }
}
