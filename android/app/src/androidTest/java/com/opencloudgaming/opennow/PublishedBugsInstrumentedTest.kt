package com.opencloudgaming.opennow

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class PublishedBugsInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val sample = AndroidPublishedBug("one", "Video freezes after reconnecting", "**We are working on a fix.**\nRestart the stream to restore video. Share logs if you are affected.", "fixing", 60, "", 12, false, 4, 123)

    @Test fun publishedBugsHaveVoteAndConsentOnlyLogsAndKeepYourReportsReachable() {
        var voted = ""
        var uploaded = ""
        val state = mutableStateOf(AndroidPublishedBugsState(bugs = listOf(sample)))
        compose.setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Surface(Modifier.fillMaxSize(), color = SettingsPanel) {
                    Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp)) {
                        BugReportsSettings(state.value, {}, {}, { voted = it }, { uploaded = it }) { Text("Private reports inbox") }
                    }
                }
            }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val addLogs = context.getString(R.string.published_bugs_add_logs)
        val consent = context.getString(R.string.published_bugs_share_action)
        compose.onNodeWithText(context.getString(R.string.published_bugs_vote, 12)).performScrollTo().performClick()
        compose.runOnIdle { assertEquals("one", voted) }
        compose.onNodeWithText(addLogs).performScrollTo().performClick()
        compose.onNodeWithText(context.getString(R.string.published_bugs_share_title)).assertIsDisplayed()
        compose.onAllNodes(hasSetTextAction()).assertCountEquals(0)
        compose.runOnIdle { assertEquals("", uploaded) }
        compose.onNodeWithText(consent).performClick()
        compose.runOnIdle { assertEquals("one", uploaded) }
        compose.onNodeWithText(context.getString(R.string.published_bugs_details)).performScrollTo().performClick()
        compose.onNodeWithText(context.getString(R.string.published_bugs_back)).performScrollTo().assertIsDisplayed()
        compose.onAllNodes(hasSetTextAction()).assertCountEquals(0)
        compose.onNodeWithText(context.getString(R.string.bug_report_inbox_title)).performScrollTo().performClick()
        compose.onNodeWithText("Private reports inbox").assertIsDisplayed()
    }

    @Test fun publishedBugsRenderWithMaterialProgressAndAccessibleActions() {
        compose.setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Surface(Modifier.fillMaxSize(), color = SettingsPanel) {
                    Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp)) {
                        SettingsDetailHeader(title = "", summary = null, onBack = {})
                        BugReportsSettings(AndroidPublishedBugsState(bugs = listOf(sample)), {}, {}, {}, {}) { Text("Private reports inbox") }
                    }
                }
            }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.onNodeWithText(sample.title).assertIsDisplayed()
        compose.onNodeWithText(context.getString(R.string.settings_category_bug_reports)).assertDoesNotExist()
        compose.onNodeWithText(context.getString(R.string.settings_category_bug_reports_summary)).assertDoesNotExist()
        compose.onNodeWithText(context.getString(R.string.published_bugs_percent, 60)).assertIsDisplayed()
        compose.onNodeWithText(context.getString(R.string.published_bugs_add_logs)).performScrollTo().assertIsEnabled()
        compose.onRoot().captureToImage().asAndroidBitmap().let { bitmap ->
            File(context.getExternalFilesDir(null), "published-bugs.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
    @Test fun reviewExplainerAndPausedLogCollectionKeepVotingAvailable() {
        compose.setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Surface(Modifier.fillMaxSize(), color = SettingsPanel) {
                    Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp)) {
                        BugReportsSettings(AndroidPublishedBugsState(bugs = listOf(sample.copy(status = "awaiting_review", acceptingLogs = false))), {}, {}, {}, {}) { Text("Private reports inbox") }
                    }
                }
            }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.onNodeWithText(context.getString(R.string.published_bugs_add_logs)).performScrollTo().assertIsNotEnabled()
        compose.onNodeWithText(context.getString(R.string.published_bugs_vote, 12)).assertIsEnabled()
        compose.onNodeWithText(context.getString(R.string.published_bugs_details)).performScrollTo().performClick()
        compose.onNodeWithText(context.getString(R.string.published_bugs_review_title)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(context.getString(R.string.published_bugs_logs_paused)).performScrollTo().assertIsDisplayed()
    }
}
