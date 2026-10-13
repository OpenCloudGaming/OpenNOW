package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import kotlinx.serialization.decodeFromString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FirstInstallDefaultsTest {
    @Test
    fun freshInstallUsesSelectedProfileAndPersistsItOnce() {
        for (playStoreRelease in listOf(false, true)) {
            val writes = mutableListOf<AppSettings>()
            val initial = loadAndroidSettings(null, playStoreRelease, writes::add)
            assertEquals(firstInstallSettings(playStoreRelease).normalizedForAndroid(), initial)
            assertEquals(listOf(initial), writes)
            assertTrue(initial.autoCheckForUpdates)
            assertFalse(initial.localAppsEnabled)
        }
    }

    @Test
    fun savedSettingsAreNotReplacedWhenDistributionChanges() {
        val saved = firstInstallSettings(false).copy(
            launchPage = AppLaunchPage.Library,
            autoCheckForUpdates = false,
            showSessionReportAfterStream = false,
        )
        val raw = OpenNowJson.encodeToString(saved)
        val writes = mutableListOf<AppSettings>()

        val loaded = loadAndroidSettings(raw, playStoreRelease = true, persistInitialOrMigration = writes::add)

        assertEquals(saved.normalizedForAndroid(), loaded)
        assertTrue(writes.isEmpty())
    }

    @Test
    fun startingProfilesPreserveCurrentUserVisibleDefaults() {
        val previousDefault = AppSettings().withCurrentNvstOptInDefault()
            .withCurrentStreamPresentationDefaults().normalizedForAndroid()
        assertEquals(previousDefault.copy(stream = previousDefault.stream.copy(
            videoOutput = StreamVideoOutput.MediaCodecSurface, colorQuality = ColorQuality.EightBit420)),
            loadAndroidSettings(null, playStoreRelease = false))
        assertEquals(previousDefault.copy(stream = previousDefault.stream.copy(videoOutput = StreamVideoOutput.WebRtcTexture)),
            loadAndroidSettings(null, playStoreRelease = true))
    }

    @Test fun outputChoiceSurvivesSerializationPresetAndDistributionChanges() {
        for (output in StreamVideoOutput.entries) {
            val saved = firstInstallSettings(false).copy(stream = firstInstallSettings(false).stream.copy(videoOutput = output))
            val loaded = loadAndroidSettings(OpenNowJson.encodeToString(saved), playStoreRelease = true)
            assertEquals(output, loaded.stream.videoOutput)
            val recommended = StreamSettings().withUserStreamOptionsFrom(loaded.stream)
            assertEquals(output, recommended.videoOutput)
        }
    }

    @Test fun oldSavedStreamsWithoutOutputChoiceFollowBuildDefaults() {
        val old = OpenNowJson.decodeFromString<StreamSettings>("""{"colorQuality":"8bit_420"}""")
        assertEquals(StreamVideoOutput.Default, old.videoOutput)
        assertTrue(shouldPreferDirectSdrSurface(old, playStoreRelease = false))
        assertFalse(shouldPreferDirectSdrSurface(old, playStoreRelease = true))
        assertTrue(shouldPreferDirectSdrSurface(firstInstallSettings(false).stream, playStoreRelease = false))
        assertFalse(shouldPreferDirectSdrSurface(firstInstallSettings(true).stream.copy(colorQuality = ColorQuality.EightBit420),
            playStoreRelease = true))
    }
}
