package com.opencloudgaming.opennow

import androidx.activity.ComponentActivity
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

class GameDetailsTransitionAnchorInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun transitionReadsCurrentBoundsAfterScrollingAndReleasesThemOnDetach() {
        val anchor = GameDetailsTransitionAnchor()
        val showCard = mutableStateOf(true)
        lateinit var scroll: ScrollState
        lateinit var scope: CoroutineScope
        compose.setContent {
            scroll = rememberScrollState()
            scope = rememberCoroutineScope()
            Column(Modifier.fillMaxSize().verticalScroll(scroll)) {
                Spacer(Modifier.height(120.dp))
                if (showCard.value) Box(Modifier.size(100.dp).then(anchor.modifier))
                Spacer(Modifier.height(1500.dp))
            }
        }

        var originalTop = 0f
        compose.runOnIdle {
            originalTop = requireNotNull(anchor.boundsInWindow()).top
            scope.launch { scroll.scrollTo(32) }
        }
        compose.waitForIdle()
        compose.runOnIdle {
            assertEquals(originalTop - 32f, requireNotNull(anchor.boundsInWindow()).top, 0.5f)
            showCard.value = false
        }
        compose.runOnIdle { assertNull(anchor.boundsInWindow()) }
    }
}
