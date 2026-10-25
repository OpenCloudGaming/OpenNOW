package com.opencloudgaming.opennow

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.lifecycle.ViewModelProvider
import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class DesktopSessionIdentityInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun desktopIdentityIsDiscoverableAndMutuallyExclusiveWithWebsiteMode() {
        lateinit var viewModel: OpenNowViewModel
        compose.activityRule.scenario.onActivity { activity ->
            viewModel = ViewModelProvider(activity)[OpenNowViewModel::class.java]
            viewModel.updateStreamSettings { it.copy(browserClientMode = true, desktopClientMode = false) }
        }
        compose.setContent {
            val state by viewModel.state.collectAsState()
            OpenNowTheme(state.settings, physicalControllerConnected = false) {
                Surface(Modifier.fillMaxSize()) {
                    SettingsScreen(
                        state = state,
                        viewModel = viewModel,
                        tvProfile = false,
                        searchRequested = false,
                        searchQuery = "desktop",
                        backRequestToken = 0,
                        onSearchQueryChange = {},
                        onDetailRouteChange = {},
                    )
                }
            }
        }
        compose.onNodeWithText("Desktop session identity").performScrollTo().assertIsDisplayed().performClick()
        compose.waitUntil(10_000) { viewModel.state.value.settings.stream.desktopClientMode }
        compose.runOnIdle {
            assertTrue(viewModel.state.value.settings.stream.desktopClientMode)
            assertFalse(viewModel.state.value.settings.stream.browserClientMode)
            assertFalse(viewModel.state.value.settings.stream.experimentalNvst)
        }
        val image = compose.onRoot().captureToImage().asAndroidBitmap()
        File(compose.activity.getExternalFilesDir(null), "desktop-session-identity.png").outputStream().use {
            image.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        compose.onNodeWithText("Website client mode").performScrollTo().performClick()
        compose.waitUntil(10_000) { viewModel.state.value.settings.stream.browserClientMode }
        compose.runOnIdle { assertFalse(viewModel.state.value.settings.stream.desktopClientMode) }
    }
}
