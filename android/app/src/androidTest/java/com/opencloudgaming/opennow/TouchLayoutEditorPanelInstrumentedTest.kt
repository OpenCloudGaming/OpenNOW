package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import android.content.res.Configuration
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test

class TouchLayoutEditorPanelInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Before fun landscapeWindow() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
        }
    }

    private fun done() = compose.onNodeWithText("Done")
    private fun hint() = compose.onNodeWithText("Tap a button to customize it. Drag to move controls.")
    private fun viewport() = compose.onNodeWithTag("viewport").fetchSemanticsNode().boundsInRoot

    @Test fun draggingDoneMovesThePanelWithoutFinishingAndTapStillFinishes() {
        var finishes = 0
        compose.setContent {
            MaterialTheme {
                Box(Modifier.fillMaxSize().testTag("viewport")) {
                    TouchLayoutEditorPanel({ finishes++ }, Modifier.fillMaxSize())
                }
            }
        }
        val before = done().fetchSemanticsNode().boundsInRoot
        val hintBefore = hint().fetchSemanticsNode().boundsInRoot
        assertTrue("Done initially covers the middle", before.bottom < viewport().center.y)
        done().performTouchInput { swipe(center, center + Offset(100f, 120f), 600) }
        val after = done().fetchSemanticsNode().boundsInRoot
        val hintAfter = hint().fetchSemanticsNode().boundsInRoot
        assertTrue(after.left > before.left + 40f)
        assertTrue(after.top > before.top + 40f)
        assertEquals(after.top - before.top, hintAfter.top - hintBefore.top, 1f)
        compose.runOnIdle { assertEquals(0, finishes) }
        done().performTouchInput { click() }
        compose.runOnIdle { assertEquals(1, finishes) }
    }

    @Test fun draggingTheHintKeepsBothHintAndDoneWithinTheViewportAtEveryEdge() {
        compose.setContent {
            MaterialTheme {
                Box(Modifier.fillMaxSize().testTag("viewport")) {
                    TouchLayoutEditorPanel({}, Modifier.fillMaxSize())
                }
            }
        }
        val root = viewport()
        fun dragHintTo(destination: Offset) {
            val start = hint().fetchSemanticsNode().boundsInRoot.center - root.topLeft
            compose.onNodeWithTag("viewport").performTouchInput { swipe(start, destination, 600) }
            listOf(hint(), done()).forEach { node ->
                node.assertIsDisplayed()
                val semantics = node.fetchSemanticsNode()
                // Use the un-clipped size so a partially hidden panel cannot pass.
                val bounds = Rect(semantics.positionInRoot,
                    androidx.compose.ui.geometry.Size(semantics.size.width.toFloat(), semantics.size.height.toFloat()))
                assertTrue("Panel outside $root: $bounds", bounds.left >= root.left - 1f &&
                    bounds.top >= root.top - 1f && bounds.right <= root.right + 1f && bounds.bottom <= root.bottom + 1f)
            }
        }
        dragHintTo(Offset(root.width - 1f, root.height - 1f))
        dragHintTo(Offset(1f, 1f))
    }

    @Test fun centerControlCanBeDraggedWhileTheEditorPanelIsVisible() {
        var offset by mutableStateOf(TouchOffset())
        compose.setContent {
            MaterialTheme {
                Box(Modifier.fillMaxSize().testTag("viewport")) {
                    TouchControlGroup("center", true, offset.x.dp, offset.y.dp,
                        { x, y -> offset = TouchOffset(x, y) }, modifier = Modifier.align(Alignment.Center)) {
                        Box(Modifier.size(64.dp).testTag("control")) { Text("A") }
                    }
                    TouchLayoutEditorPanel({}, Modifier.fillMaxSize())
                }
            }
        }
        compose.onNodeWithTag("control").performTouchInput { swipe(center, center + Offset(140f, 60f), 600) }
        compose.runOnIdle { assertTrue("Center control did not move: $offset", offset.x > 0f) }
    }
}
