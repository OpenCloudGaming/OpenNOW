package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamResolutionTest {
    @Test
    fun smartSessionLimitsMatchMembershipTier() {
        val free = smartSessionLimitFor(SubscriptionInfo(membershipTier = "FREE"), null)
        val performance = smartSessionLimitFor(SubscriptionInfo(membershipTier = "PERFORMANCE"), null)
        val ultimate = smartSessionLimitFor(SubscriptionInfo(membershipTier = "ULTIMATE"), null)

        assertEquals(1, free.limitHours)
        assertEquals(SessionTimerMode.Countdown, free.mode)
        assertEquals(6, performance.limitHours)
        assertEquals(SessionTimerMode.Stopwatch, performance.mode)
        assertEquals(8, ultimate.limitHours)
        assertEquals(SessionTimerMode.Stopwatch, ultimate.mode)
    }

    @Test
    fun paidMonthlyUsageFallsBackToHundredHourLimit() {
        val subscription = SubscriptionInfo(membershipTier = "ULTIMATE", usedHours = 37.25)

        assertEquals(100.0, monthlyHourLimitFor(subscription, null) ?: 0.0, 0.001)
        assertEquals(62.75, monthlyHoursRemainingFor(subscription, null) ?: 0.0, 0.001)
    }

    @Test
    fun sessionWarningsFireAtMostRelevantCrossedThreshold() {
        assertEquals(null, sessionWarningThresholdCrossed(null, 30 * 60))
        assertEquals(30 * 60, sessionWarningThresholdCrossed(30 * 60 + 1, 30 * 60))
        assertEquals(5 * 60, sessionWarningThresholdCrossed(10 * 60 + 1, 5 * 60 - 1))
        assertEquals(null, sessionWarningThresholdCrossed(5 * 60, 5 * 60 - 1))
    }

    @Test
    fun streamResolutionPixelsKeepsSelected1080pFor16By9() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "16:9")

        assertEquals(1920 to 1080, streamResolutionPixels(settings))
    }

    @Test
    fun runtimeResolutionMismatchIsDiagnosticForServerFallbackModes() {
        assertEquals(
            StreamResolutionMismatch(actualResolution = "1152x720", expectedResolution = "1280x720"),
            streamRuntimeResolutionMismatch(
                StreamSettings(resolution = "1280x720", aspectRatio = "16:9"),
                "1152x720",
            ),
        )
        assertEquals(
            StreamResolutionMismatch(actualResolution = "1366x768", expectedResolution = "1680x720"),
            streamRuntimeResolutionMismatch(
                StreamSettings(resolution = "1680x720", aspectRatio = "21:9"),
                "1366x768",
            ),
        )
    }

    @Test
    fun runtimeGameResolutionChangeRemainsDiagnosticOnly() {
        val mismatch = streamRuntimeResolutionMismatch(
            StreamSettings(resolution = "1920x1080", aspectRatio = "16:9"),
            actualResolution = "1280x720",
            serverNegotiatedResolution = "1920x1080",
        )

        assertEquals(
            StreamResolutionMismatch(
                actualResolution = "1280x720",
                expectedResolution = "1920x1080",
            ),
            mismatch,
        )
        assertEquals(false, mismatch?.isServerNegotiatedFallback)
    }

    @Test
    fun runtimeResolutionMismatchMarksServerNegotiatedFallback() {
        val mismatch = streamRuntimeResolutionMismatch(
            StreamSettings(resolution = "1680x720", aspectRatio = "21:9"),
            actualResolution = "1366x768",
            serverNegotiatedResolution = "1366x768",
        )

        assertEquals(
            StreamResolutionMismatch(
                actualResolution = "1366x768",
                expectedResolution = "1680x720",
                serverNegotiatedResolution = "1366x768",
            ),
            mismatch,
        )
        assertEquals(true, mismatch?.isServerNegotiatedFallback)
    }

    @Test
    fun runtimeResolutionMismatchIgnoresExactAndMissingStats() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9")

        assertEquals(null, streamRuntimeResolutionMismatch(settings, null))
        assertEquals(null, streamRuntimeResolutionMismatch(settings, "1680x720"))
        assertEquals(null, streamRuntimeResolutionMismatch(settings, "2x2"))
    }

    @Test
    fun activeStreamModeSeparatesServerFallbackFromLaterProviderOrGameChange() {
        val requested = StreamSettings(
            resolution = "1680x720",
            aspectRatio = "21:9",
            codec = VideoCodec.AV1,
        )

        val serverFallback = activeStreamModeStatus(
            requestedSettings = requested,
            transportSettings = requested,
            decodedResolution = "1366x768",
            serverNegotiatedResolution = "1366x768",
        )
        val laterModeChange = activeStreamModeStatus(
            requestedSettings = requested,
            transportSettings = requested,
            decodedResolution = "1230x768",
            serverNegotiatedResolution = "1366x768",
        )
        val finalServerMode = activeStreamModeStatus(
            requestedSettings = requested,
            transportSettings = requested,
            decodedResolution = "1230x768",
            serverNegotiatedResolution = "1366x768",
            serverFinalSelectedResolution = "1230x768",
        )

        assertEquals(StreamResolutionChangeSource.ServerNegotiatedFallback, serverFallback?.resolutionSource)
        assertEquals("1366x768", serverFallback?.displayedResolution)
        assertEquals(false, serverFallback?.safeVideoRecoveryActive)
        assertEquals(
            ActiveStreamModeDisplayChange("Resolution", "1680x720", "1366x768", ActiveStreamModeChangeKind.Resolution),
            serverFallback?.let(::activeStreamModeDisplayChanges)?.first(),
        )
        assertEquals(StreamResolutionChangeSource.ProviderOrGameModeChange, laterModeChange?.resolutionSource)
        assertEquals("1230x768", laterModeChange?.displayedResolution)
        assertEquals(false, laterModeChange?.safeVideoRecoveryActive)
        assertEquals(StreamResolutionChangeSource.ServerNegotiatedFallback, finalServerMode?.resolutionSource)
        assertEquals("1230x768", finalServerMode?.serverFinalSelectedResolution)
    }

    @Test
    fun activeStreamModeWaitsForDecodedVideoBeforeReportingProvisionalServerFallback() {
        val requested = StreamSettings(resolution = "1376x590", aspectRatio = "21:9")

        assertEquals(
            null,
            activeStreamModeStatus(
                requestedSettings = requested,
                transportSettings = requested,
                decodedResolution = null,
                serverNegotiatedResolution = "1680x720",
            ),
        )
        assertEquals(
            null,
            activeStreamModeStatus(
                requestedSettings = requested,
                transportSettings = requested,
                decodedResolution = "1376x590",
                serverNegotiatedResolution = "1680x720",
            ),
        )
    }

    @Test
    fun activeStreamModeSurfacesClientSafeRecoveryWithoutInventingResolutionChange() {
        val requested = StreamSettings(
            resolution = "3840x2160",
            aspectRatio = "16:9",
            fps = 120,
            maxBitrateMbps = 150,
            codec = VideoCodec.AV1,
            colorQuality = ColorQuality.TenBit420,
        )
        val recovery = requested.androidSafeVideoFallback()

        val status = activeStreamModeStatus(
            requestedSettings = requested,
            transportSettings = recovery,
            decodedResolution = "3840x2160",
            serverNegotiatedResolution = "3840x2160",
        )

        assertEquals(null, status?.resolutionSource)
        assertEquals(true, status?.safeVideoRecoveryActive)
        assertEquals(VideoCodec.H264, status?.transportCodec)
        assertEquals("3840x2160", status?.requestedResolution)
        assertEquals("3840x2160", status?.displayedResolution)
        assertEquals(
            listOf("Codec", "FPS", "Color"),
            status?.let(::activeStreamModeDisplayChanges)?.map { it.label },
        )
        assertEquals(
            ActiveStreamModeDisplayChange("Codec", "AV1", "H264", ActiveStreamModeChangeKind.Codec),
            status?.let(::activeStreamModeDisplayChanges)?.first(),
        )
        assertFalse(status?.let(::activeStreamModeDisplayChanges).orEmpty().any { it.label == "Resolution" })
        val reason = "AV1 was requested but WebRTC did not negotiate it; restarting with safe H264 profile"
        assertEquals(
            "WebRTC could not negotiate the requested AV1 codec for this connection, so OpenNOW retried the local video transport with H264.",
            status?.let { activeStreamModeCauseAssessment(it, reason).summary },
        )
        val report = status?.let { activeStreamModeDeveloperReport(it, reason) }
        assertEquals("Automatic stream change: Codec AV1 to H264", report?.title)
        assertTrue(report?.description.orEmpty().contains("- Codec: AV1 -> H264"))
        assertTrue(report?.description.orEmpty().contains("Recorded recovery event:"))
    }

    @Test
    fun activeStreamModeExplainsServerSelectedResolution() {
        val requested = StreamSettings(resolution = "1680x720", aspectRatio = "21:9")
        val status = requireNotNull(
            activeStreamModeStatus(
                requestedSettings = requested,
                transportSettings = requested,
                decodedResolution = "1366x768",
                serverNegotiatedResolution = "1366x768",
            ),
        )

        assertEquals(
            "The cloud server selected 1366x768 instead of the requested 1680x720. This was a server/session negotiation decision, not a change to your saved setting.",
            activeStreamModeCauseAssessment(status, null).summary,
        )
    }

    @Test
    fun streamRendererAspectRatioUsesSelectedResolution() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9")

        assertEquals(1680f / 720f, streamRendererAspectRatio(settings), 0.0001f)
    }

    @Test
    fun fixedSizeSurfaceIsDisabledWhenDecodedFrameReachesViewportBoundary() {
        assertFalse(
            shouldUseFixedSizeStreamSurface(
                videoWidth = 2560,
                videoHeight = 1440,
                rotation = 0,
                viewWidth = 2994,
                viewHeight = 1440,
            ),
        )
        assertFalse(
            shouldUseFixedSizeStreamSurface(
                videoWidth = 2560,
                videoHeight = 1440,
                rotation = 0,
                viewWidth = 2340,
                viewHeight = 1080,
            ),
        )
    }

    @Test
    fun streamRendererUsesAuthoritativeServerFallbackAspectRatio() {
        val settings = StreamSettings(resolution = "5120x2160", aspectRatio = "21:9")

        assertEquals(
            1920f / 1080f,
            streamRendererAspectRatio(
                settings = settings,
                decodedResolution = "1920x1080",
                serverNegotiatedResolution = "1920x1080",
            ),
            0.0001f,
        )
    }

    @Test
    fun fixedSizeSurfaceRemainsEnabledForSmallerDecodedFrames() {
        assertTrue(
            shouldUseFixedSizeStreamSurface(
                videoWidth = 1920,
                videoHeight = 1080,
                rotation = 0,
                viewWidth = 2560,
                viewHeight = 1440,
            ),
        )
        assertTrue(
            shouldUseFixedSizeStreamSurface(
                videoWidth = 1080,
                videoHeight = 1920,
                rotation = 90,
                viewWidth = 2560,
                viewHeight = 1440,
            ),
        )
    }

    @Test
    fun transientDecodedGeometryDoesNotResizeSelectedViewport() {
        val settings = StreamSettings(resolution = "5120x2160", aspectRatio = "21:9")

        assertEquals(
            5120f / 2160f,
            streamRendererAspectRatio(
                settings = settings,
                decodedResolution = "1920x1080",
                serverNegotiatedResolution = "2560x1080",
            ),
            0.0001f,
        )
    }

    @Test
    fun a56NegotiatedUltrawideFallbackKeepsItsSourceAspectRatio() {
        val settings = StreamSettings(resolution = "5120x2160", aspectRatio = "21:9")

        assertEquals(
            2560f / 1080f,
            streamRendererAspectRatio(
                settings = settings,
                decodedResolution = "2560x1080",
                serverNegotiatedResolution = "2560x1080",
            ),
            0.0001f,
        )
    }

    @Test
    fun widePhoneStretchScalesOnlyWidthWithoutCropping() {
        val scale = streamStretchScale(
            enabled = true,
            viewportAspectRatio = 2400f / 1080f,
            streamAspectRatio = 1280f / 720f,
        )

        assertEquals(1.25f, scale.first, 0.0001f)
        assertEquals(1f, scale.second, 0.0001f)
    }

    @Test
    fun stretchUsesDecodedAspectWhenGameChangesItsOutputMode() {
        val selectedAspect = 1920f / 1080f
        val decodedAspect = streamStretchContentAspectRatio(
            selectedAspectRatio = selectedAspect,
            decodedResolution = "1728x1080",
        )
        val scale = streamStretchScale(
            enabled = true,
            viewportAspectRatio = selectedAspect,
            streamAspectRatio = decodedAspect,
        )

        assertEquals(1.6f, decodedAspect, 0.0001f)
        assertEquals(10f / 9f, scale.first, 0.0001f)
        assertEquals(1f, scale.second, 0.0001f)
    }

    @Test
    fun stretchFallsBackToSelectedAspectUntilDecodedModeIsKnown() {
        val selectedAspect = 16f / 9f

        assertEquals(
            selectedAspect,
            streamStretchContentAspectRatio(selectedAspect, decodedResolution = null),
            0.0001f,
        )
        assertEquals(
            selectedAspect,
            streamStretchContentAspectRatio(selectedAspect, decodedResolution = "invalid"),
            0.0001f,
        )
    }

    @Test
    fun pinchZoomIsDisabledWhileTouchControllerIsVisible() {
        assertFalse(
            streamPinchZoomEnabled(
                touchMouseEnabled = true,
                touchControllerVisible = true,
            ),
        )
        assertTrue(
            streamPinchZoomEnabled(
                touchMouseEnabled = true,
                touchControllerVisible = false,
            ),
        )
        assertFalse(
            streamPinchZoomEnabled(
                touchMouseEnabled = false,
                touchControllerVisible = false,
            ),
        )
    }

    @Test
    fun streamResolutionPixelsMaps1080pTierToUltrawideMode() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "21:9")

        assertEquals(2560 to 1080, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsMaps1080pTierToNineteenPointFiveByNinePhoneMode() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "19.5:9")

        assertEquals(2340 to 1080, streamResolutionPixels(settings))
        assertEquals(
            "2340x1080",
            normalizeStreamResolutionForAspect("1920x1080", "19.5:9"),
        )
    }

    @Test
    fun persistedUnsupportedAspectFallsBackToSupportedSixteenByNineMode() {
        val adjusted = StreamSettings(resolution = "1600x720", aspectRatio = "20:9")
            .withResolutionAllowed(SubscriptionInfo(membershipTier = "FREE"), null)

        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals("1280x720", adjusted.resolution)
    }

    @Test
    fun freePlanResolutionNormalizationKeepsFreeUltrawide() {
        val freeSubscription = SubscriptionInfo(membershipTier = "FREE")

        assertEquals(
            "1680x720",
            normalizeStreamResolutionForAspectAndPlan("1920x1080", "21:9", freeSubscription, null),
        )
        val adjusted = StreamSettings(resolution = "2560x1080", aspectRatio = "21:9")
            .withResolutionAllowed(freeSubscription, null)
        assertEquals("1680x720", adjusted.resolution)
        assertEquals("21:9", adjusted.aspectRatio)

        val selectedWhd = StreamSettings(resolution = "1680x720", aspectRatio = "21:9")
            .withResolutionAllowed(freeSubscription, null)
        assertEquals("1680x720", selectedWhd.resolution)
        assertEquals("21:9", selectedWhd.aspectRatio)

        val selectedPortalMode = StreamSettings(resolution = "1376x640", aspectRatio = "19.5:9")
            .withResolutionAllowed(freeSubscription, null)
        assertEquals("1376x640", selectedPortalMode.resolution)
        assertEquals("19.5:9", selectedPortalMode.aspectRatio)
    }

    @Test
    fun freePlanPhoneResolutionFallbackPreservesAspectAndUsesAndroidIdentity() {
        val adjusted = StreamSettings(resolution = "2340x1080", aspectRatio = "19.5:9", fps = 120)
            .eligibleForAndroidLaunch(SubscriptionInfo(membershipTier = "FREE"), null, androidTvProfile = false)

        assertEquals("1376x640", adjusted.resolution)
        assertEquals("19.5:9", adjusted.aspectRatio)
        assertEquals(60, adjusted.fps)
        assertTrue(adjusted.requiresNativeAndroidCloudMatchMode())
    }

    @Test
    fun customResolutionPixelsArePreservedForLaunch() {
        val settings = StreamSettings(resolution = "1728x720", aspectRatio = "21:9")

        assertEquals(1728 to 720, streamResolutionPixels(settings))
    }

    @Test
    fun freePlanPreservesCustomUltrawideResolutionInsidePixelBounds() {
        val adjusted = StreamSettings(resolution = "1728x720", aspectRatio = "21:9")
            .withResolutionAllowed(SubscriptionInfo(membershipTier = "FREE"), null)

        assertEquals("1728x720", adjusted.resolution)
        assertEquals("21:9", adjusted.aspectRatio)
    }

    @Test
    fun freePlanClampsUltimateUltrawideToFreeUltrawide() {
        val adjusted = StreamSettings(resolution = "5120x2160", aspectRatio = "21:9")
            .withResolutionAllowed(SubscriptionInfo(membershipTier = "FREE"), null)

        assertEquals("1680x720", adjusted.resolution)
        assertEquals("21:9", adjusted.aspectRatio)
    }

    @Test
    fun freePlanResolutionGuardFallsBackWhenAspectHasNoAvailableMode() {
        val adjusted = StreamSettings(resolution = "3840x1080", aspectRatio = "32:9")
            .withResolutionAllowed(SubscriptionInfo(membershipTier = "FREE"), null)

        assertEquals("1920x1080", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
    }

    @Test
    fun streamResolutionPixelsMaps1440pTierToUltrawideMode() {
        val settings = StreamSettings(resolution = "2560x1440", aspectRatio = "21:9")

        assertEquals(3440 to 1440, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsKeepsSelected4kFor16By9() {
        val settings = StreamSettings(resolution = "3840x2160", aspectRatio = "16:9")

        assertEquals(3840 to 2160, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsMapsSelectedTierForTallerAspectRatio() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "16:10")

        assertEquals(1920 to 1200, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsKeepsExactStoredUltrawideMode() {
        val settings = StreamSettings(resolution = "3440x1440", aspectRatio = "21:9")

        assertEquals(3440 to 1440, streamResolutionPixels(settings))
    }

    @Test
    fun portalGeometrySurvivesAndroidSettingsNormalization() {
        val settings = StreamSettings(resolution = "1376x640", aspectRatio = "19.5:9")

        val normalized = settings.withAndroidSettingsAvailability()
        assertEquals("1376x640", normalized.resolution)
        assertEquals("19.5:9", normalized.aspectRatio)
        assertEquals(1376 to 640, streamResolutionPixels(normalized))
    }

    @Test
    fun streamResolutionOptionsIncludeAndroidSupportedModes() {
        assertEquals(
            listOf("1280x720", "1366x768", "1600x900", "1920x1080", "2560x1440", "3840x2160", "5120x2880"),
            streamResolutionOptionsForAspect("16:9"),
        )
        assertEquals(listOf("1024x768", "1112x834", "1600x1200"), streamResolutionOptionsForAspect("4:3"))
        assertEquals(listOf("1280x1024"), streamResolutionOptionsForAspect("5:4"))
        assertEquals(listOf("1376x640", "2340x1080"), streamResolutionOptionsForAspect("19.5:9"))
        assertEquals(emptyList<String>(), streamResolutionOptionsForAspect("20:9"))
        assertEquals(listOf("1376x590", "1680x720", "2560x1080", "3440x1440", "5120x2160"), streamResolutionOptionsForAspect("21:9"))
        assertEquals(listOf("3840x1080", "5120x1440"), streamResolutionOptionsForAspect("32:9"))
    }

    @Test
    fun streamResolutionPixelsMaps4kTierToSupported16By10Mode() {
        val settings = StreamSettings(resolution = "3840x2160", aspectRatio = "16:10")

        assertEquals(3456 to 2160, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionChoicesGatePriorityAndUltimateModes() {
        val freeSubscription = SubscriptionInfo(membershipTier = "FREE")
        val prioritySubscription = SubscriptionInfo(membershipTier = "PRIORITY")
        val ultimateSubscription = SubscriptionInfo(membershipTier = "ULTIMATE")
        val fhd = streamResolutionChoicesForAspect("16:9").first { it.value == "1920x1080" }
        val phoneFhd = streamResolutionChoicesForAspect("19.5:9").first { it.value == "2340x1080" }
        val portal = streamResolutionChoicesForAspect("19.5:9").first { it.value == "1376x640" }
        assertTrue(portal.isAvailableFor(freeSubscription, null))
        val lowUltrawide = streamResolutionChoicesForAspect("21:9").first { it.value == "1376x590" }
        val whd = streamResolutionChoicesForAspect("21:9").first { it.value == "1680x720" }
        val wfhd = streamResolutionChoicesForAspect("21:9").first { it.value == "2560x1080" }
        val qhd = streamResolutionChoicesForAspect("16:9").first { it.value == "2560x1440" }
        val fourK = streamResolutionChoicesForAspect("16:9").first { it.value == "3840x2160" }
        val fiveK = streamResolutionChoicesForAspect("16:9").first { it.value == "5120x2880" }

        assertEquals(true, fhd.isAvailableFor(freeSubscription, null))
        assertEquals(false, phoneFhd.isAvailableFor(freeSubscription, null))
        assertEquals(true, lowUltrawide.isAvailableFor(freeSubscription, null))
        assertEquals(true, whd.isAvailableFor(freeSubscription, null))
        assertEquals(false, wfhd.isAvailableFor(freeSubscription, null))
        assertEquals(false, qhd.isAvailableFor(freeSubscription, null))
        assertEquals(true, fhd.isAvailableFor(prioritySubscription, null))
        assertEquals(true, phoneFhd.isAvailableFor(prioritySubscription, null))
        assertEquals(true, lowUltrawide.isAvailableFor(prioritySubscription, null))
        assertEquals(true, whd.isAvailableFor(prioritySubscription, null))
        assertEquals(true, wfhd.isAvailableFor(prioritySubscription, null))
        assertEquals(true, qhd.isAvailableFor(prioritySubscription, null))
        assertEquals(false, fourK.isAvailableFor(prioritySubscription, null))
        assertEquals(false, fiveK.isAvailableFor(prioritySubscription, null))
        assertEquals(true, fourK.isAvailableFor(ultimateSubscription, null))
        assertEquals(true, fiveK.isAvailableFor(ultimateSubscription, null))
    }

    @Test
    fun streamFpsCapsFollowMembershipPlan() {
        val requested = StreamSettings(fps = 360)

        assertEquals(60, requested.withFpsAllowed(SubscriptionInfo(membershipTier = "FREE"), null).fps)
        assertEquals(60, requested.withFpsAllowed(SubscriptionInfo(membershipTier = "PERFORMANCE"), null).fps)
        assertEquals(360, requested.withFpsAllowed(SubscriptionInfo(membershipTier = "ULTIMATE"), null).fps)
        assertEquals(360, maxStreamFpsFor(null, "ULTIMATE"))
    }

    @Test
    fun fpsPlanCapsDoNotChangeSelectedResolution() {
        val freeRequested = StreamSettings(resolution = "1920x1200", aspectRatio = "16:10", fps = 240)
        val ultimateRequested = StreamSettings(resolution = "3840x2160", aspectRatio = "16:9", fps = 360)

        assertEquals(
            freeRequested.copy(fps = 60),
            freeRequested.withFpsAllowed(SubscriptionInfo(membershipTier = "FREE"), null),
        )
        assertEquals(
            ultimateRequested,
            ultimateRequested.withFpsAllowed(SubscriptionInfo(membershipTier = "ULTIMATE"), null),
        )
    }

    @Test
    fun fpsPlanCapsPreserveEveryKnownResolutionAndCodec() {
        val freeSubscription = SubscriptionInfo(membershipTier = "FREE")
        val ultimateSubscription = SubscriptionInfo(membershipTier = "ULTIMATE")

        STREAM_RESOLUTION_OPTIONS.forEach { option ->
            VideoCodec.entries.forEach { codec ->
                val requested = StreamSettings(
                    resolution = option.value,
                    aspectRatio = option.aspectRatio,
                    fps = 360,
                    codec = codec,
                )

                assertEquals(
                    "Free ${option.value} $codec",
                    requested.copy(fps = 60),
                    requested.withFpsAllowed(freeSubscription, null),
                )
                assertEquals(
                    "Ultimate ${option.value} $codec",
                    requested,
                    requested.withFpsAllowed(ultimateSubscription, null),
                )
            }
        }
    }

    @Test
    fun hdrIsAvailableForPerformanceAndUltimatePlans() {
        val requested = StreamSettings(codec = VideoCodec.H265, hdrEnabled = true)

        assertEquals(false, requested.withHdrAllowed(SubscriptionInfo(membershipTier = "FREE"), null).hdrEnabled)
        assertEquals(true, requested.withHdrAllowed(SubscriptionInfo(membershipTier = "PERFORMANCE"), null).hdrEnabled)
        assertEquals(true, requested.withHdrAllowed(SubscriptionInfo(membershipTier = "PRIORITY"), null).hdrEnabled)
        assertEquals(true, requested.withHdrAllowed(SubscriptionInfo(membershipTier = "ULTIMATE"), null).hdrEnabled)
        assertEquals(true, hasHdrStreamingPlan(null, "PERFORMANCE"))
    }

    @Test
    fun handheldHdrTransportIsEligiblePendingDisplayAndDecoderProbe() {
        val adjusted = StreamSettings(
            resolution = "1920x1080",
            fps = 60,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
        ).withAndroidHdrCompatibility(androidTvProfile = false)

        assertEquals(true, adjusted.hdrEnabled)
        assertEquals(ColorQuality.TenBit420, adjusted.colorQuality)
    }

    @Test
    fun androidTvHdrRequiresShieldClassH265Mode() {
        val supported = StreamSettings(
            resolution = "3840x2160",
            fps = 60,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
        )

        assertEquals(true, supported.hdrAvailableForAndroid(androidTvProfile = true))
        assertEquals(false, supported.copy(fps = 120).hdrAvailableForAndroid(androidTvProfile = true))
        assertEquals(false, supported.copy(resolution = "5120x2880").hdrAvailableForAndroid(androidTvProfile = true))
        assertEquals(false, supported.copy(codec = VideoCodec.H264).hdrAvailableForAndroid(androidTvProfile = true))
    }

    @Test
    fun authenticatedUltimateTierWinsWhenSubscriptionPayloadDefaultsToFree() {
        val incompleteSubscription = SubscriptionInfo(membershipTier = "FREE")
        val fiveK = streamResolutionChoicesForAspect("16:9").first { it.value == "5120x2880" }
        val requested = StreamSettings(resolution = "5120x2880", aspectRatio = "16:9", fps = 360)

        assertEquals(true, fiveK.isAvailableFor(incompleteSubscription, "ULTIMATE"))
        assertEquals(360, maxStreamFpsFor(incompleteSubscription, "ULTIMATE"))
        assertEquals(
            requested,
            requested
                .withResolutionAllowed(incompleteSubscription, "ULTIMATE")
                .withFpsAllowed(incompleteSubscription, "ULTIMATE"),
        )
    }

    @Test
    fun entitledResolutionDoesNotBypassMembershipPlanGate() {
        val subscription = SubscriptionInfo(
            membershipTier = "FREE",
            entitledResolutions = listOf(EntitledResolution(width = 3840, height = 2160, fps = 60)),
        )
        val fourK = streamResolutionChoicesForAspect("16:9").first { it.value == "3840x2160" }

        assertEquals(false, fourK.isAvailableFor(subscription, null))
    }

    @Test
    fun activeSessionRejectsUnexpectedResolutionBeforeReuse() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val stale = activeSession(
            resolution = "1680x1050",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(settings),
        )

        assertEquals(false, stale.matchesStreamSettings(settings))
    }

    @Test
    fun activeSessionRejectsServerFallbackResolutionBeforeReuse() {
        val ultrawide = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val hd = StreamSettings(resolution = "1280x720", aspectRatio = "16:9", fps = 60)

        assertEquals(
            false,
            activeSession(
                resolution = "1366x768",
                fps = 60,
                settingsSignature = streamSettingsSessionSignature(ultrawide),
            ).matchesStreamSettings(ultrawide),
        )
        assertEquals(
            false,
            activeSession(
                resolution = "1280x720",
                fps = 60,
                settingsSignature = streamSettingsSessionSignature(ultrawide),
            ).matchesStreamSettings(ultrawide),
        )
        assertEquals(
            false,
            activeSession(
                resolution = "1152x720",
                fps = 60,
                settingsSignature = streamSettingsSessionSignature(hd),
            ).matchesStreamSettings(hd),
        )
    }

    @Test
    fun activeSessionMatchesRequestedUltrawideResolutionBeforeReuse() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val active = activeSession(
            resolution = "1680x720",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(settings),
        )

        assertEquals(true, active.matchesStreamSettings(settings))
    }

    @Test
    fun activeSessionWithoutOpenNowSettingsSignatureIsNotReusedForLaunch() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val active = activeSession(resolution = "1680x720", fps = 60)

        assertEquals(false, active.matchesStreamSettings(settings))
    }

    @Test
    fun activeSessionWithDifferentOpenNowSettingsSignatureIsNotReusedForLaunch() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60, codec = VideoCodec.H265, maxBitrateMbps = 150)
        val otherSettings = settings.copy(codec = VideoCodec.H264, maxBitrateMbps = 75)
        val active = activeSession(
            resolution = "1680x720",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(otherSettings),
        )

        assertEquals(false, active.matchesStreamSettings(settings))
    }

    @Test
    fun recoveryReclaimsExactUltrawideSessionAfterLocalCodecFallback() {
        val original = StreamSettings(
            resolution = "1680x720",
            aspectRatio = "21:9",
            fps = 60,
            codec = VideoCodec.AV1,
        )
        val safeFallback = original.androidSafeVideoFallback()
        val running = activeSession(
            sessionId = "running-session",
            resolution = "1680x720",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(original),
        )

        assertEquals(false, running.matchesStreamSettings(safeFallback))
        assertEquals(
            "running-session",
            activeSessionRecoveryCandidate(
                sessions = listOf(running),
                previousSessionId = "running-session",
                launchAppId = running.appId,
                settings = safeFallback,
            )?.sessionId,
        )
    }

    @Test
    fun recoveryReconstructsKnownFreshSessionWithoutCachedActiveGeometry() {
        val settings = StreamSettings(
            resolution = "1376x590",
            aspectRatio = "21:9",
            fps = 60,
            maxBitrateMbps = 7,
        )
        val running = SessionInfo(
            sessionId = "running-session",
            status = 2,
            streamingBaseUrl = "https://alliance.example",
            serverIp = "streamer.example",
            signalingServer = "streamer.example:443",
            signalingUrl = "wss://streamer.example:443/nvst/",
        )

        val candidate = knownSessionRecoveryCandidate(
            session = running,
            appId = 101808711,
            fallbackActive = null,
            settings = settings,
        )

        assertEquals("running-session", candidate?.sessionId)
        assertEquals("1376x590", candidate?.resolution)
        assertEquals(60, candidate?.fps)
        assertEquals(streamSettingsSessionSignature(settings), candidate?.settingsSignature)
        assertTrue(candidate?.matchesStreamGeometry(settings) == true)
    }

    @Test
    fun recoveryDoesNotReclaimExactSessionWithDifferentGeometry() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val stale = activeSession(
            sessionId = "running-session",
            resolution = "1920x1080",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(settings),
        )

        assertEquals(
            null,
            activeSessionRecoveryCandidate(
                sessions = listOf(stale),
                previousSessionId = "running-session",
                launchAppId = null,
                settings = settings,
            ),
        )
    }

    @Test
    fun recoveryKeepsStrictSignatureMatchingForOtherSessions() {
        val original = StreamSettings(
            resolution = "1680x720",
            aspectRatio = "21:9",
            fps = 60,
            codec = VideoCodec.AV1,
        )
        val safeFallback = original.androidSafeVideoFallback()
        val otherSession = activeSession(
            sessionId = "other-session",
            resolution = "1680x720",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(original),
        )

        assertEquals(
            null,
            activeSessionRecoveryCandidate(
                sessions = listOf(otherSession),
                previousSessionId = "previous-session",
                launchAppId = otherSession.appId,
                settings = safeFallback,
            ),
        )
    }

    @Test
    fun activeSessionWithUnknownMonitorModeIsNotReusedForLaunch() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val active = activeSession(
            resolution = null,
            fps = null,
            settingsSignature = streamSettingsSessionSignature(settings),
        )

        assertEquals(false, active.matchesStreamSettings(settings))
    }

    @Test
    fun activeSessionWithUnknownRefreshRateIsNotReusedForLaunch() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val active = activeSession(
            resolution = "1680x720",
            fps = null,
            settingsSignature = streamSettingsSessionSignature(settings),
        )

        assertEquals(false, active.matchesStreamSettings(settings))
    }

    @Test
    fun activeSessionConflictPrefersSameRequestedAppBeforePolling() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val otherReady = activeSession(
            sessionId = "other-ready",
            appId = 200,
            resolution = "1680x720",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(settings),
        )
        val sameAppLaunching = activeSession(
            sessionId = "same-launching",
            appId = 100,
            status = 1,
            resolution = "1680x720",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(settings),
        )

        assertEquals(
            "same-launching",
            activeSessionLaunchConflict(listOf(otherReady, sameAppLaunching), launchAppId = 100, settings = settings)?.sessionId,
        )
    }

    @Test
    fun activeSessionConflictStillReturnsMismatchedExistingSessionForUserChoice() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60)
        val mismatched = activeSession(
            sessionId = "mismatched",
            appId = 100,
            resolution = "1920x1080",
            fps = 60,
            settingsSignature = streamSettingsSessionSignature(StreamSettings(resolution = "1920x1080", aspectRatio = "16:9", fps = 60)),
        )

        assertEquals(
            "mismatched",
            activeSessionLaunchConflict(listOf(mismatched), launchAppId = 100, settings = settings)?.sessionId,
        )
    }

    private fun activeSession(
        sessionId: String = "session",
        appId: Int = 100,
        status: Int = 2,
        resolution: String?,
        fps: Int?,
        settingsSignature: String? = null,
    ): ActiveSessionInfo =
        ActiveSessionInfo(
            sessionId = sessionId,
            appId = appId,
            status = status,
            serverIp = "127.0.0.1",
            signalingUrl = "wss://127.0.0.1/nvst/",
            resolution = resolution,
            fps = fps,
            settingsSignature = settingsSignature,
        )
}
