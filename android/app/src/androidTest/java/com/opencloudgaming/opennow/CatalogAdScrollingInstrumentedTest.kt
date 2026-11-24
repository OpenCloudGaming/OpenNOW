package com.opencloudgaming.opennow

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToKey
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class CatalogAdScrollingInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun unfilledTopAndInlineAdsLeaveNoPlaceholder() {
        compose.setContent {
            Column {
                CatalogAdRow(null, compact = true, modifier = Modifier.testTag("top-ad"))
                CatalogAdRow(null, compact = false, modifier = Modifier.testTag("inline-ad"))
            }
        }

        compose.onNodeWithTag("top-ad", useUnmergedTree = true).assertDoesNotExist()
        compose.onNodeWithTag("inline-ad", useUnmergedTree = true).assertDoesNotExist()
    }

    @Test
    fun aFourColumnCatalogPlacesUnfilledSlotsAfterCompleteRows() {
        val games = List(65) { GameInfo(id = "game-$it", title = "Game $it") }
        val interval = catalogAdInterval(4)
        val ads = CatalogAdSlots(games.size, interval)
        lateinit var grid: LazyGridState
        compose.setContent {
            grid = rememberLazyGridState()
            LazyVerticalGrid(
                columns = GridCells.Fixed(4),
                state = grid,
                modifier = Modifier.fillMaxSize().testTag("ad-catalog"),
            ) {
                item(key = "catalog-header", span = { GridItemSpan(maxLineSpan) }) {
                    Box(Modifier.height(64.dp))
                }
                catalogGameItems(games, ads, "game") { Box(Modifier.height(64.dp)) }
            }
        }

        val catalog = compose.onNodeWithTag("ad-catalog")
        for ((stop, index) in listOf(32 to 33, 64 to 66)) {
            val adKey = "catalog-ad-after-$stop"
            catalog.performScrollToKey("game-${stop - 4}")
            compose.runOnIdle {
                val ad = grid.layoutInfo.visibleItemsInfo.first { it.key == adKey }
                assertEquals(index, ad.index)
                assertEquals(grid.layoutInfo.viewportSize.width, ad.size.width)
                assertEquals(0, ad.size.height)
                val previousGame = grid.layoutInfo.visibleItemsInfo.first { it.key == "game-${stop - 1}" }
                assertEquals(3, previousGame.column)
                assertEquals(68, grid.layoutInfo.totalItemsCount)
                val visibleGameCounts = grid.layoutInfo.visibleItemsInfo.mapNotNull { item ->
                    val key = item.key as String
                    when {
                        key.startsWith("game-") -> key.removePrefix("game-").toInt() + 1
                        key.startsWith("catalog-ad-after-") -> key.removePrefix("catalog-ad-after-").toInt()
                        else -> null
                    }
                }
                assertEquals(
                    visibleGameCounts.min()..visibleGameCounts.max(),
                    visibleCatalogGameCounts(grid.layoutInfo, games.size, interval),
                )
            }
        }
    }
}
