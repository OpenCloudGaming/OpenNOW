package com.opencloudgaming.opennow

import kotlinx.serialization.decodeFromString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AppSettingsDefaultsTest {
    @Test
    fun streamStatusBarDefaultsToConnectionEssentials() {
        val settings = AppSettings()
        val metrics = settings.streamStatsMetrics

        assertTrue(settings.showStatsOnLaunch)
        assertFalse(settings.hideStreamButtons)
        assertFalse(settings.streamKeyboardClearConfirmationDisabled)
        assertTrue(settings.externalMousePointerLock)
        assertFalse(settings.showFavoriteIconOnGameCards)
        assertFalse(settings.liveSelectedOutlines)
        assertFalse(settings.absoluteCinemaEffects)
        assertFalse(settings.absoluteCinemaEverywhere)
        assertFalse(settings.localAppsEnabled)
        assertFalse(settings.stretchStreamToFit)
        assertTrue(settings.ambientBackgroundEnabled)
        assertFalse(settings.systemWallpaperBackground)
        assertTrue(settings.localAppPackageNames.isEmpty())
        // The shelf opens on first sight; folding it is a choice the reader makes and keeps.
        assertFalse(settings.localAppsCollapsed)
        assertTrue(settings.landscapeNewGamesHero)
        // Rumble routing stays automatic until someone's hardware proves it needs forcing.
        assertEquals(HapticsOutputPreference.Auto, settings.hapticsOutput)
        assertEquals(TouchControllerStyle.V1, settings.androidTouch.touchControllerStyle)
        assertNull(settings.androidTouch.touchSkinTint)
        assertTrue(settings.androidTouch.touchButtonLabels)
        assertFalse(settings.androidTouch.gyroscopeEnabled)
        assertEquals(1f, settings.androidTouch.aimZoneScale, 0.0001f)
        assertEquals(1f, settings.androidTouch.aimZoneSensitivity, 0.0001f)
        assertEquals(1f, settings.androidTouch.faceButtonScale, 0.0001f)
        assertEquals(1f, settings.androidTouch.leftStickScale, 0.0001f)
        assertEquals(1f, settings.androidTouch.rightStickScale, 0.0001f)
        assertEquals(TouchControlGroup.entries.toSet(), settings.androidTouch.visibleControlGroups)
        assertEquals(TouchExtraButtonAction.Guide, settings.androidTouch.extraButtonAction(0))
        assertEquals(TouchExtraButtonAction.None, settings.androidTouch.extraButtonAction(3))
        // Developer options are a hidden gesture, never a shipped or migrated-in default.
        assertFalse(settings.developerOptionsUnlocked)
        assertEquals(StreamKeyboardButtonPosition(), settings.streamKeyboardButtonPosition)
        assertTrue(metrics.fps)
        assertTrue(metrics.ping)
        assertFalse(metrics.bitrate)
        assertTrue(metrics.battery)
        assertTrue(metrics.connection)
        assertFalse(metrics.resolution)
        assertFalse(metrics.codec)
        assertFalse(metrics.location)
        assertFalse(metrics.latency)
        assertFalse(metrics.packetLoss)
        assertEquals(4, metrics.enabledCount())
    }

    @Test
    fun olderSavedSettingsReceiveStatusBarDefaults() {
        val settings = OpenNowJson.decodeFromString<AppSettings>("{}")

        assertEquals(StreamStatsMetrics(), settings.streamStatsMetrics)
        assertTrue(settings.showStatsOnLaunch)
        assertFalse(settings.hideStreamButtons)
        assertFalse(settings.streamKeyboardClearConfirmationDisabled)
        assertTrue(settings.externalMousePointerLock)
        assertEquals(StreamKeyboardButtonPosition(), settings.streamKeyboardButtonPosition)
        assertEquals(CatalogBackgroundPreset.ColorfulAbstract, settings.catalogBackgroundPreset)
        assertFalse(settings.systemWallpaperBackground)
        assertFalse(settings.showFavoriteIconOnGameCards)
        assertFalse(settings.liveSelectedOutlines)
        assertFalse(settings.absoluteCinemaEffects)
        assertFalse(settings.absoluteCinemaEverywhere)
        assertFalse(settings.localAppsEnabled)
        assertFalse(settings.stretchStreamToFit)
        assertTrue(settings.localAppPackageNames.isEmpty())
        assertTrue(settings.landscapeNewGamesHero)
        // Developer options are a hidden gesture, never a shipped or migrated-in default.
        assertFalse(settings.developerOptionsUnlocked)
        assertFalse(settings.showSessionReportAfterStream)
        assertEquals(TouchJoystickMode.Fixed, settings.androidTouch.joystickMode)
        assertEquals(TouchAimMode.LockJoystick, settings.androidTouch.aimMode)
        assertEquals(0f, settings.androidTouch.joystickDeadZone, 0.0001f)
        assertEquals(TouchControlGroup.entries.toSet(), settings.androidTouch.visibleControlGroups)
        assertEquals(TouchExtraButtonAction.Guide, settings.androidTouch.extraButtonAction(0))
    }

    @Test
    fun streamKeyboardClearConfirmationDefaultsOnAndPreservesOptOut() {
        val defaulted = OpenNowJson.decodeFromString<AppSettings>("{}")
        val optedOut = OpenNowJson.decodeFromString<AppSettings>(
            """{"streamKeyboardClearConfirmationDisabled":true}""",
        )

        assertFalse(defaulted.streamKeyboardClearConfirmationDisabled)
        assertTrue(optedOut.streamKeyboardClearConfirmationDisabled)
    }

    @Test
    fun favoriteIconDefaultsOffAndPreservesExplicitOptIn() {
        val defaulted = OpenNowJson.decodeFromString<AppSettings>("{}")
        val optedIn = OpenNowJson.decodeFromString<AppSettings>(
            """{"showFavoriteIconOnGameCards":true}""",
        )

        assertFalse(defaulted.showFavoriteIconOnGameCards)
        assertTrue(optedIn.showFavoriteIconOnGameCards)
    }

    @Test
    fun liveSelectedOutlinesDefaultOffAndPreserveOptIn() {
        val defaulted = OpenNowJson.decodeFromString<AppSettings>("{}")
        val optedIn = OpenNowJson.decodeFromString<AppSettings>(
            """{"liveSelectedOutlines":true}""",
        )

        assertFalse(defaulted.liveSelectedOutlines)
        assertTrue(optedIn.liveSelectedOutlines)
    }

    @Test
    fun localAppsAreOptInAndSavedPackagesRemainCompatible() {
        val defaulted = OpenNowJson.decodeFromString<AppSettings>("{}")
        val optedIn = OpenNowJson.decodeFromString<AppSettings>(
            """{"localAppsEnabled":true,"localAppPackageNames":["com.epicgames.fortnite"]}""",
        )

        assertFalse(defaulted.localAppsEnabled)
        assertTrue(defaulted.localAppPackageNames.isEmpty())
        assertTrue(optedIn.localAppsEnabled)
        assertEquals(listOf("com.epicgames.fortnite"), optedIn.localAppPackageNames)
    }

    @Test
    fun absoluteCinemaAccentRemainsIndependentFromEffectToggle() {
        val cinema = OpenNowJson.decodeFromString<AppSettings>("""{"uiAccent":"AbsoluteCinema"}""").normalizedForAndroid()
        val switch = OpenNowJson.decodeFromString<AppSettings>("""{"uiAccent":"Switch"}""")

        assertEquals(UiAccent.AbsoluteCinema, cinema.uiAccent)
        assertFalse(cinema.absoluteCinemaEffects)
        assertEquals(UiAccent.Switch, switch.uiAccent)
    }

    @Test
    fun removedOrangeAccentMigratesToViolet() {
        val settings = OpenNowJson.decodeFromString<AppSettings>(
            """{"uiAccent":"Orange"}""",
        ).normalizedForAndroid()

        assertEquals(UiAccent.Violet, settings.uiAccent)
    }

    @Test
    fun crazyCinemaPersistsOnlyAsAnAbsoluteCinemaSuboption() {
        val enabled = OpenNowJson.decodeFromString<AppSettings>(
            """{"absoluteCinemaEffects":true,"absoluteCinemaEverywhere":true}""",
        ).normalizedForAndroid()
        val orphaned = OpenNowJson.decodeFromString<AppSettings>(
            """{"absoluteCinemaEverywhere":true}""",
        ).normalizedForAndroid()

        assertTrue(enabled.absoluteCinemaEverywhere)
        assertFalse(orphaned.absoluteCinemaEverywhere)
    }

    @Test
    fun catalogueSortAndFiltersSurviveSettingsDecode() {
        val settings = OpenNowJson.decodeFromString<AppSettings>(
            """{"catalogSortId":"latest","catalogFilterIds":["genre-action","opennow:supported-controls:touchscreen"],"librarySortId":"recent","libraryFilterIds":["library_store:steam"]}""",
        ).normalizedForAndroid()

        assertEquals("latest", settings.catalogSortId)
        assertEquals(listOf("genre-action", CATALOG_FILTER_TOUCHSCREEN), settings.catalogFilterIds)
        assertEquals(LIBRARY_SORT_RECENT, settings.librarySortId)
        assertEquals(listOf("library_store:steam"), settings.libraryFilterIds)
    }

    @Test
    fun legacyRelevanceDefaultMigratesOnceToMostPopular() {
        val migrated = OpenNowJson.decodeFromString<AppSettings>(
            """{"catalogSortId":"relevance"}""",
        ).normalizedForAndroid()
        val explicitRelevance = migrated.copy(catalogSortId = "relevance").normalizedForAndroid()

        assertEquals(DEFAULT_CATALOG_SORT_ID, migrated.catalogSortId)
        assertEquals(CATALOG_SORT_DEFAULT_VERSION, migrated.catalogSortDefaultVersion)
        assertEquals("relevance", explicitRelevance.catalogSortId)
    }

    @Test
    fun touchAimZoneIsOptInAndPersistsWhenSelected() {
        val defaulted = OpenNowJson.decodeFromString<AppSettings>("{}")
        val optedIn = OpenNowJson.decodeFromString<AppSettings>(
            """{"androidTouch":{"aimMode":"LockZone"}}""",
        )

        assertEquals(TouchAimMode.LockJoystick, defaulted.androidTouch.aimMode)
        assertEquals(TouchAimMode.LockZone, optedIn.androidTouch.aimMode)
    }

    @Test
    fun touchAimZoneCustomizationPersistsAndNormalizes() {
        val persisted = OpenNowJson.decodeFromString<AppSettings>(
            """{"androidTouch":{"aimZoneScale":1.25,"aimZoneSensitivity":1.75}}""",
        ).normalizedForAndroid()
        val invalid = AppSettings(
            androidTouch = AndroidTouchSettings(
                aimZoneScale = Float.NaN,
                aimZoneSensitivity = Float.POSITIVE_INFINITY,
            ),
        ).normalizedForAndroid()
        val bounded = AppSettings(
            androidTouch = AndroidTouchSettings(
                aimZoneScale = 9f,
                aimZoneSensitivity = 0.1f,
            ),
        ).normalizedForAndroid()

        assertEquals(1.25f, persisted.androidTouch.aimZoneScale, 0.0001f)
        assertEquals(1.75f, persisted.androidTouch.aimZoneSensitivity, 0.0001f)
        assertEquals(1f, invalid.androidTouch.aimZoneScale, 0.0001f)
        assertEquals(1f, invalid.androidTouch.aimZoneSensitivity, 0.0001f)
        assertEquals(1.5f, bounded.androidTouch.aimZoneScale, 0.0001f)
        assertEquals(0.25f, bounded.androidTouch.aimZoneSensitivity, 0.0001f)
    }

    @Test
    fun retiredLibraryHeroPreferenceDoesNotInvalidateSavedSettings() {
        val settings = OpenNowJson.decodeFromString<AppSettings>(
            """{"libraryHeroCarousel":false,"landscapeNewGamesHero":false}""",
        )

        assertFalse(settings.landscapeNewGamesHero)
    }

    @Test
    fun defaultsUseRecommendedProfileAndKeepOptionalMusicOff() {
        val settings = AppSettings()

        assertFalse(settings.nerdMode)
        assertEquals(CatalogBackgroundPreset.ColorfulAbstract, settings.catalogBackgroundPreset)
        assertTrue(settings.controllerUiSounds)
        assertTrue(settings.vibrationEnabled)
        assertEquals(AppLaunchPage.Store, settings.launchPage)
        assertEquals(StreamPreset.Recommended, settings.streamPreset)
        assertFalse(settings.streamIntroMusic)
        assertEquals(IntroMusicStartMode.Muted, settings.streamIntroStartMode)
        assertFalse(settings.queueReadyMusic)
        assertFalse(settings.showSessionReportAfterStream)
        assertFalse(settings.stream.streamSharpeningEnabled)
    }

    @Test
    fun legacyPhoneRumbleSettingControlsTheUnifiedVibrationToggle() {
        val disabled = OpenNowJson.decodeFromString<AppSettings>(
            """{"phoneRumbleFallback":false}""",
        )

        assertFalse(disabled.vibrationEnabled)
    }

    @Test
    fun olderSavedSettingsKeepStreamSharpeningDisabledUnlessExplicitlyEnabled() {
        val defaulted = OpenNowJson.decodeFromString<AppSettings>("{}")
        val optedIn = OpenNowJson.decodeFromString<AppSettings>(
            """{"stream":{"streamSharpeningEnabled":true}}""",
        )

        assertFalse(defaulted.stream.streamSharpeningEnabled)
        assertTrue(optedIn.stream.streamSharpeningEnabled)
    }

    @Test
    fun mouseLockDefaultsOnAndPreservesExplicitOptOut() {
        val defaulted = OpenNowJson.decodeFromString<AppSettings>("{}")
        val optedOut = OpenNowJson.decodeFromString<AppSettings>(
            """{"externalMousePointerLock":false}""",
        )

        assertTrue(defaulted.externalMousePointerLock)
        assertFalse(optedOut.externalMousePointerLock)
    }

    @Test
    fun phonePresentationKeepsExactGeometryByDefaultAndRunsOnce() {
        val migrated = AppSettings().withCurrentStreamPresentationDefaults()

        assertFalse(migrated.stretchStreamToFit)
        assertFalse(migrated.legacyCropStreamToFill)
        assertEquals(STREAM_PRESENTATION_PROFILE_VERSION, migrated.streamPresentationProfileVersion)

        // Already migrated: a later opt-in is the user's, and must survive every launch after it.
        val optedIn = migrated.copy(stretchStreamToFit = true)
        assertEquals(optedIn, optedIn.withCurrentStreamPresentationDefaults())
    }

    @Test
    fun existingInstallKeepsItsSavedStretchPreference() {
        val optedIn = AppSettings(stretchStreamToFit = true, streamPresentationProfileVersion = 2)
        val optedOut = AppSettings(stretchStreamToFit = false, streamPresentationProfileVersion = 2)

        assertTrue(optedIn.withCurrentStreamPresentationDefaults().stretchStreamToFit)
        assertFalse(optedOut.withCurrentStreamPresentationDefaults().stretchStreamToFit)
    }

    @Test
    fun tvPresentationKeepsExactGeometry() {
        // A TV panel and a 16:9 stream already agree; filling would be a no-op that misreports.
        val migrated = AppSettings().withCurrentStreamPresentationDefaults()

        assertFalse(migrated.legacyCropStreamToFill)
        assertFalse(migrated.stretchStreamToFit)
        assertEquals(STREAM_PRESENTATION_PROFILE_VERSION, migrated.streamPresentationProfileVersion)
    }

    @Test
    fun retiredAnalyticsKeysDoNotDiscardExistingSettings() {
        val settings = OpenNowJson.decodeFromString<AppSettings>(
            """{"analyticsOptOut":false,"analyticsConsentAsked":true,"setupFlowCompletedVersion":2,"showSessionReportAfterStream":false}""",
        )
        assertEquals(2, settings.setupFlowCompletedVersion)
        assertFalse(settings.showSessionReportAfterStream)
    }

    @Test
    fun sessionReportOptOutSurvivesSettingsSerialization() {
        val settings = OpenNowJson.decodeFromString<AppSettings>(
            """{"showSessionReportAfterStream":false}""",
        )

        assertFalse(settings.showSessionReportAfterStream)
    }

    @Test
    fun legacySessionReportOptInIsMigratedOffOnce() {
        val migrated = OpenNowJson.decodeFromString<AppSettings>(
            """{"showSessionReportAfterStream":true}""",
        ).normalizedForAndroid()

        assertFalse(migrated.showSessionReportAfterStream)
        assertEquals(SESSION_REPORT_DEFAULT_VERSION, migrated.sessionReportDefaultVersion)
    }

    @Test
    fun currentSessionReportOptInRemainsAvailable() {
        val optedIn = OpenNowJson.decodeFromString<AppSettings>(
            """{"showSessionReportAfterStream":true,"sessionReportDefaultVersion":1}""",
        ).normalizedForAndroid()

        assertTrue(optedIn.showSessionReportAfterStream)
    }

    @Test
    fun legacyPortalStreamModeMigratesToProviderTwentyOneByNineGeometry() {
        val normalized = AppSettings(
            stream = StreamSettings(
                resolution = "1376x640",
                aspectRatio = "19.5:9",
                fps = 120,
            ),
        ).normalizedForAndroid()

        assertEquals("1376x590", normalized.stream.resolution)
        assertEquals("21:9", normalized.stream.aspectRatio)
        assertEquals(120, normalized.stream.fps)
    }

    @Test
    fun persistedNonFiniteInputSettingsFallBackBeforeTheyReachMotionRounding() {
        val normalized = AppSettings(
            stream = StreamSettings(
                mouseSensitivity = Float.NaN,
                streamSharpeningAmount = Float.POSITIVE_INFINITY,
            ),
            posterSizeScale = Float.NaN,
            tvSafeAreaPaddingDp = Float.NEGATIVE_INFINITY,
            androidTouch = AndroidTouchSettings(
                opacity = Float.NaN,
                nativeTouchScrollScale = Float.POSITIVE_INFINITY,
                nativeTouchJitterThresholdDp = Float.NaN,
                offsets = mapOf("bad" to TouchOffset(Float.NaN, Float.POSITIVE_INFINITY)),
            ),
        ).normalizedForAndroid()

        assertEquals(1f, normalized.stream.mouseSensitivity, 0f)
        assertEquals(0.25f, normalized.stream.streamSharpeningAmount, 0f)
        assertEquals(1f, normalized.posterSizeScale, 0f)
        assertEquals(16f, normalized.tvSafeAreaPaddingDp, 0f)
        assertEquals(AndroidTouchSettings().opacity, normalized.androidTouch.opacity, 0f)
        assertEquals(1f, normalized.androidTouch.nativeTouchScrollScale, 0f)
        assertEquals(8f, normalized.androidTouch.nativeTouchJitterThresholdDp, 0f)
        assertEquals(TouchOffset(), normalized.androidTouch.offsets["bad"])
    }

    @Test
    fun fullyTransparentTouchControlsRemainInteractivePreference() {
        val normalized = AppSettings(
            androidTouch = AndroidTouchSettings(opacity = 0f),
        ).normalizedForAndroid()

        assertEquals(0f, normalized.androidTouch.opacity, 0f)
    }

    @Test
    fun keyboardButtonPositionIsKeptInsideTheStreamViewport() {
        val normalized = AppSettings(
            streamKeyboardButtonPosition = StreamKeyboardButtonPosition(
                horizontalFraction = Float.POSITIVE_INFINITY,
                verticalFraction = -0.25f,
            ),
        ).normalizedForAndroid()

        assertEquals(1f, normalized.streamKeyboardButtonPosition.horizontalFraction, 0f)
        assertEquals(0f, normalized.streamKeyboardButtonPosition.verticalFraction, 0f)
    }
}
