package com.opencloudgaming.opennow

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test

class CatalogAdScrollingInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun adWorkWaitsForGridScrollingToFinish() {
        val lifecycle = compose.activity.lifecycle
        val scrolling = AtomicBoolean()
        val finishScroll = CompletableDeferred<Unit>()
        val gridSettled = AtomicBoolean()
        compose.setContent {
            val grid = rememberLazyGridState()
            LazyVerticalGrid(columns = GridCells.Fixed(3), state = grid) {
                items(60) { Box(Modifier.height(64.dp)) }
            }
            LaunchedEffect(grid) {
                grid.awaitCatalogIdle(lifecycle)
                launch(start = CoroutineStart.UNDISPATCHED) {
                    grid.scroll {
                        scrolling.set(true)
                        finishScroll.await()
                    }
                }
                // Simulate a native ad result while the grid owns the scroll gesture.
                grid.awaitCatalogIdle(lifecycle)
                gridSettled.set(true)
            }
        }

        try {
            compose.waitUntil(timeoutMillis = 5_000) { scrolling.get() }
            compose.mainClock.advanceTimeBy(1_000)
            assertFalse(gridSettled.get())
        } finally {
            finishScroll.complete(Unit)
        }
        compose.waitUntil(timeoutMillis = 5_000) { gridSettled.get() }
    }
}
