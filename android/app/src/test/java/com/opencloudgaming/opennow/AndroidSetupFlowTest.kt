package com.opencloudgaming.opennow

import kotlinx.serialization.decodeFromString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidSetupFlowTest {
    @Test
    fun `the subtle adaptive background is default and nothing stays distinct`() {
        val defaults = AppSettings()
        val nothing = defaults.withAppBackgroundChoice(AppBackgroundChoice.Nothing)
        val wallpaper = defaults.withAppBackgroundChoice(AppBackgroundChoice.Wallpaper)
        val systemWallpaper = defaults.withAppBackgroundChoice(AppBackgroundChoice.SystemWallpaper)

        assertEquals(AppBackgroundChoice.Default, appBackgroundChoiceFor(defaults))
        assertTrue(defaults.ambientBackgroundEnabled)
        assertEquals(AppBackgroundChoice.Nothing, appBackgroundChoiceFor(nothing))
        assertFalse(nothing.ambientBackgroundEnabled)
        assertFalse(nothing.nerdCatalogBackground)
        assertEquals(AppBackgroundChoice.Wallpaper, appBackgroundChoiceFor(wallpaper))
        assertTrue(wallpaper.nerdCatalogBackground)
        assertFalse(wallpaper.systemWallpaperBackground)
        assertEquals(AppBackgroundChoice.SystemWallpaper, appBackgroundChoiceFor(systemWallpaper))
        assertTrue(systemWallpaper.systemWallpaperBackground)
        assertFalse(systemWallpaper.nerdCatalogBackground)
        assertFalse(systemWallpaper.ambientBackgroundEnabled)
        assertFalse(shouldShowSystemWallpaperBackground(systemWallpaper, inStream = true))
        assertTrue(shouldShowSystemWallpaperBackground(systemWallpaper, inStream = false))
    }

    @Test
    fun `a fresh install runs setup and a finished one does not`() {
        assertTrue(shouldShowSetupFlow(AppSettings()))
        assertTrue(shouldShowSetupFlow(OpenNowJson.decodeFromString<AppSettings>("{}")))
        assertFalse(shouldShowSetupFlow(AppSettings().completingSetupFlow()))
    }

    @Test
    fun `raising the flow version brings existing installs back through setup`() {
        val completedOnAnOlderRelease = AppSettings(setupFlowCompletedVersion = SETUP_FLOW_VERSION - 1)

        assertTrue(shouldShowSetupFlow(completedOnAnOlderRelease))
    }

    @Test
    fun `running setup again from settings does not disturb the choices it made`() {
        val configured = AppSettings(
            uiAccent = UiAccent.Violet,
            nerdCatalogBackground = true,
            catalogBackgroundPreset = CatalogBackgroundPreset.AbsoluteCinema,
        ).completingSetupFlow()

        val restarted = configured.restartingSetupFlow()

        assertTrue(shouldShowSetupFlow(restarted))
        assertEquals(configured.copy(setupFlowCompletedVersion = 0), restarted)
    }

    @Test
    fun `steps run welcome to ready with no gaps at either end`() {
        assertEquals(
            listOf(
                SetupStep.Welcome,
                SetupStep.Appearance,
                SetupStep.Streaming,
                SetupStep.Play,
                SetupStep.Ready,
            ),
            setupSteps(),
        )
        assertNull(setupStepBefore(SetupStep.Welcome))
        assertNull(setupStepAfter(SetupStep.Ready))
        assertTrue(isFinalSetupStep(SetupStep.Ready))
        assertFalse(isFinalSetupStep(SetupStep.Play))

        var step = SetupStep.Welcome
        val walked = mutableListOf(step)
        while (!isFinalSetupStep(step)) {
            step = setupStepAfter(step)!!
            walked += step
        }
        assertEquals(setupSteps(), walked)
        assertEquals(SetupStep.Play, setupStepBefore(SetupStep.Ready))
    }

    @Test
    fun `the streaming step reflects the preset already in settings`() {
        assertEquals(
            SetupStreamingChoice.Recommended,
            setupStreamingChoiceFor(AppSettings(streamPreset = StreamPreset.Recommended)),
        )
        assertEquals(
            SetupStreamingChoice.Best,
            setupStreamingChoiceFor(AppSettings(streamPreset = StreamPreset.High)),
        )
        assertEquals(
            SetupStreamingChoice.DataSaver,
            setupStreamingChoiceFor(AppSettings(streamPreset = StreamPreset.LowDataSaver)),
        )
        listOf(StreamPreset.Custom, StreamPreset.Medium).forEach { preset ->
            assertEquals(
                SetupStreamingChoice.Custom,
                setupStreamingChoiceFor(AppSettings(streamPreset = preset)),
            )
        }
    }

    @Test
    fun `every streaming choice round-trips through its preset`() {
        SetupStreamingChoice.entries.forEach { choice ->
            val preset = setupStreamingPresetFor(choice)
            assertEquals(
                choice.name,
                choice,
                setupStreamingChoiceFor(AppSettings(streamPreset = preset)),
            )
        }
    }

    @Test
    fun `only the custom choice exposes the inline stream controls`() {
        SetupStreamingChoice.entries.forEach { choice ->
            assertEquals(
                choice.name,
                choice == SetupStreamingChoice.Custom,
                setupStreamingCustomControlsVisible(choice),
            )
        }
    }

    @Test
    fun `custom setup codec choices share settings availability and selection`() {
        val presentation = androidCodecChoicePresentation(
            stream = StreamSettings(codec = VideoCodec.H265),
            codecReport = null,
            comingSoonLabel = "Coming soon",
            unavailableLabel = "Unavailable",
        )

        assertEquals(VideoCodec.entries.map { it.name }, presentation.options.map { it.value })
        assertTrue(presentation.options.all { it.enabled })
        assertEquals(VideoCodec.H265.name, presentation.selectedLabel)
    }

    @Test
    fun `touch mouse choices write one authoritative mode`() {
        SetupTouchMouseChoice.entries.forEach { choice ->
            val settings = AppSettings().withSetupTouchMouseChoice(choice)

            assertEquals(choice, setupTouchMouseChoiceFor(settings))
            assertEquals(choice != SetupTouchMouseChoice.Off, settings.androidTouch.mousePad)
            assertEquals(choice == SetupTouchMouseChoice.Direct, settings.androidTouch.mouseDirectClick)
        }
    }

    @Test
    fun `a disabled finger mouse does not retain direct click`() {
        val settings = AppSettings(
            androidTouch = AndroidTouchSettings(mousePad = true, mouseDirectClick = true),
        ).withSetupTouchMouseChoice(SetupTouchMouseChoice.Off)

        assertFalse(settings.androidTouch.mousePad)
        assertFalse(settings.androidTouch.mouseDirectClick)
        assertEquals(SetupTouchMouseChoice.Off, setupTouchMouseChoiceFor(settings))
    }

    @Test
    fun `setup and stream controls share every status line item`() {
        assertEquals(
            listOf(
                StreamStatusItem.Keyboard,
                StreamStatusItem.Fps,
                StreamStatusItem.Ping,
                StreamStatusItem.Bitrate,
                StreamStatusItem.Battery,
                StreamStatusItem.Connection,
                StreamStatusItem.Resolution,
                StreamStatusItem.Codec,
                StreamStatusItem.Server,
                StreamStatusItem.Latency,
                StreamStatusItem.PacketLoss,
            ),
            StreamStatusItem.entries,
        )
    }

    @Test
    fun `every setup status item toggles only its own persisted value`() {
        val defaults = AppSettings()

        StreamStatusItem.entries.forEach { item ->
            val before = item.enabledIn(defaults)
            val changed = item.setEnabled(defaults, !before)

            assertEquals(item.name, !before, item.enabledIn(changed))
            StreamStatusItem.entries.filterNot { it == item }.forEach { untouched ->
                assertEquals(
                    "changing ${item.name} also changed ${untouched.name}",
                    untouched.enabledIn(defaults),
                    untouched.enabledIn(changed),
                )
            }
            assertEquals(item.name, defaults, item.setEnabled(changed, before))
        }
    }

    @Test
    fun `finishing setup changes nothing else about the settings`() {
        val before = AppSettings(
            uiAccent = UiAccent.Lime,
            streamPreset = StreamPreset.LowDataSaver,
            showStatsOnLaunch = false,
            showSessionReportAfterStream = false,
        )

        val after = before.completingSetupFlow()

        assertEquals(
            before,
            after.copy(setupFlowCompletedVersion = before.setupFlowCompletedVersion),
        )
    }
}
