package com.opencloudgaming.opennow

import android.app.Application
import com.posthog.PostHog
import com.posthog.android.PostHogAndroid
import com.posthog.android.PostHogAndroidConfig

internal object OpenNowAnalytics {
    fun setup(application: Application, settings: AppSettings) {
        val token = BuildConfig.POSTHOG_PROJECT_TOKEN.trim()
        if (token.isEmpty()) return

        val config = PostHogAndroidConfig(
            apiKey = token,
            host = BuildConfig.POSTHOG_HOST,
        ).apply { applyOpenNowSettings(settings) }

        runCatching {
            PostHogAndroid.setup(application, config)
            applyOptOut(settings.analyticsOptOut)
        }
    }

    fun applyOptOut(optedOut: Boolean) {
        runCatching {
            if (optedOut) {
                PostHog.optOut()
            } else {
                PostHog.optIn()
            }
        }
    }

    fun identify(session: AuthSession) {
        runCatching {
            PostHog.identify(
                distinctId = session.user.userId,
                userProperties = mapOf(
                    "display_name" to session.user.displayName,
                    "membership_tier" to (session.user.membershipTier ?: ""),
                    "provider" to session.provider.code,
                ),
            )
        }
    }

    fun capture(event: String, properties: Map<String, Any>? = null) {
        runCatching {
            PostHog.capture(
                event = event,
                properties = properties,
            )
        }
    }

    fun reset() {
        runCatching {
            val optedOut = PostHog.isOptOut()
            PostHog.reset()
            applyOptOut(optedOut)
        }
    }
}

internal fun PostHogAndroidConfig.applyOpenNowSettings(settings: AppSettings) {
    optOut = settings.analyticsOptOut
    captureApplicationLifecycleEvents = true
    captureDeepLinks = true
    captureScreenViews = true
    sessionReplay = true
    sessionReplayConfig.apply {
        maskAllTextInputs = true
        maskAllImages = true
        screenshot = true
    }
    errorTrackingConfig.autoCapture = true
}
