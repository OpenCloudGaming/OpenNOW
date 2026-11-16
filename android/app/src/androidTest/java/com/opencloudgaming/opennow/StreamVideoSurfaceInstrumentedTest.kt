package com.opencloudgaming.opennow

import android.view.View
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
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

    private fun submitFrame(surface: StreamVideoSurface, width: Int, height: Int) {
        val frame = VideoFrame(JavaI420Buffer.allocate(width, height), 0, System.nanoTime())
        try {
            surface.onFrame(frame)
        } finally {
            frame.release()
        }
    }
}
