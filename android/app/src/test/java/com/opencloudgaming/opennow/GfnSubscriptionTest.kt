package com.opencloudgaming.opennow

import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GfnSubscriptionTest {
    private val repository = GfnSubscriptionRepository(OkHttpClient())

    @Test
    fun subscriptionRequestUsesEachAccountsCredentialsAndProviderRegion() {
        for ((token, userId, region) in listOf(
            Triple("first-user-jwt", "first-user", "NP-PDX-01"),
            Triple("second-user-jwt", "second+user&test=id", "NP-AMS-08"),
        )) {
            val request = buildSubscriptionRequest(token, userId, region)
            assertEquals("GET", request.method)
            assertEquals("mes.geforcenow.com", request.url.host)
            assertEquals("/v4/subscriptions", request.url.encodedPath)
            assertEquals("gfn_pc", request.url.queryParameter("serviceName"))
            assertEquals("en_GB", request.url.queryParameter("languageCode"))
            assertEquals(region, request.url.queryParameter("vpcId"))
            assertEquals(userId, request.url.queryParameter("userId"))
            assertEquals(4, request.url.querySize)
            assertEquals("GFNJWT $token", request.header("Authorization"))
            assertEquals("BROWSER", request.header("nv-client-type"))
            assertEquals("WEBRTC", request.header("nv-client-streamer"))
            assertEquals("2.0.89.154", request.header("nv-client-version"))
            assertEquals("MACOS", request.header("nv-device-os"))
            assertEquals("DESKTOP", request.header("nv-device-type"))
            assertEquals("APPLE", request.header("nv-device-make"))
            assertEquals("https://play.geforcenow.com", request.header("Origin"))
            assertEquals("no-cache", request.header("Cache-Control"))
            assertEquals("true", request.header("x-sw-cachebypass"))
        }
    }

    @Test
    fun suppliedFreeMembershipResponseConfirmsIntroWithoutCurrentSubscriptionState() {
        val response = checkNotNull(javaClass.getResource("/gfn/subscription-free.json")).readText()
        val subscription = repository.parseSubscriptionResponse(response)

        assertEquals("FREE", subscription.membershipTier)
        assertEquals("FREE", subscription.subscriptionType)
        assertTrue(subscription.isUnlimited)
        assertEquals(true, subscription.isGamePlayAllowed)
        assertEquals(0.0, subscription.totalHours, 0.0)
        assertTrue(subscription.entitledResolutions.any { it.width == 1920 && it.height == 1080 && it.fps == 60 })
        assertEquals(SetupGfnMembershipStatus.Playable, setupGfnMembershipStatus(subscription))
    }

    @Test
    fun explicitGameplayDecisionTakesPrecedenceOverStreamingSupport() {
        for (allowed in listOf(false, true)) {
            val subscription = repository.parseSubscriptionResponse(
                """{"membershipTier":"FREE","streamingSupport":"${if (allowed) "DISABLED" else "ENABLED"}","currentSubscriptionState":{"isGamePlayAllowed":$allowed}}""",
            )
            assertEquals(allowed, subscription.isGamePlayAllowed)
            assertEquals(
                if (allowed) SetupGfnMembershipStatus.Playable else SetupGfnMembershipStatus.Missing,
                setupGfnMembershipStatus(subscription),
            )
        }
    }

    @Test
    fun disabledStreamingSupportIsNotPlayableAndUnknownSupportStaysUnverified() {
        val disabled = repository.parseSubscriptionResponse("""{"membershipTier":"FREE","streamingSupport":"DISABLED"}""")
        assertEquals(false, disabled.isGamePlayAllowed)
        assertEquals(SetupGfnMembershipStatus.Missing, setupGfnMembershipStatus(disabled))
        for (response in listOf(
            """{"membershipTier":"FREE"}""",
            """{"membershipTier":"FREE","streamingSupport":"UNKNOWN"}""",
            """{}""",
        )) {
            val subscription = repository.parseSubscriptionResponse(response)
            assertNull(subscription.isGamePlayAllowed)
            assertEquals(SetupGfnMembershipStatus.Unverified, setupGfnMembershipStatus(subscription))
        }
    }

    @Test
    fun paidMembershipRetainsTimeAccountingAndStorageAddon() {
        val subscription = repository.parseSubscriptionResponse(
            """{
                "membershipTier":"ULTIMATE", "type":"PAID", "subType":"LIMITED",
                "allottedTimeInMinutes":6000, "purchasedTimeInMinutes":600,
                "rolledOverTimeInMinutes":300, "remainingTimeInMinutes":5400,
                "currentSubscriptionState":{"state":"ACTIVE","isGamePlayAllowed":true},
                "addons":[{"type":"STORAGE","subType":"PERMANENT_STORAGE","status":"OK",
                    "attributes":[{"key":"TOTAL_STORAGE_SIZE_IN_GB","textValue":"100"}]}]
            }""",
        )
        assertEquals("ULTIMATE", subscription.membershipTier)
        assertEquals(115.0, subscription.totalHours, 0.0)
        assertEquals(25.0, subscription.usedHours, 0.0)
        assertEquals(90.0, subscription.remainingHours, 0.0)
        assertEquals(100.0, checkNotNull(subscription.storageAddon?.sizeGb), 0.0)
        assertFalse(subscription.isUnlimited)
    }
}
