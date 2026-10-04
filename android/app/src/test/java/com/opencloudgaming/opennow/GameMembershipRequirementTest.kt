package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GameMembershipRequirementTest {

    private fun game(tierLabel: String?) = GameInfo(id = "1", title = "Test", membershipTierLabel = tierLabel)

    private fun subscription(tier: String) = SubscriptionInfo(membershipTier = tier)

    @Test
    fun aGameWithNoStatedTierNeverWarns() {
        assertNull(gameMembershipRequirement(game(null), subscription("FREE"), null))
        assertNull(gameMembershipRequirement(game("  "), subscription("FREE"), null))
    }

    @Test
    fun premiumMarkerTracksTheCatalogTierRegardlessOfAccountPlan() {
        assertFalse(game(null).hasPremiumMembershipMarker())
        assertFalse(game("Free").hasPremiumMembershipMarker())
        assertFalse(game("Day Pass").hasPremiumMembershipMarker())
        assertTrue(game("Performance").hasPremiumMembershipMarker())
        assertTrue(game("Premium").hasPremiumMembershipMarker())
        assertTrue(game("GeForce NOW Ultimate").hasPremiumMembershipMarker())
        assertNull(gameMembershipRequirement(game("Ultimate"), subscription("ULTIMATE"), null))
        assertTrue(game("Ultimate").hasPremiumMembershipMarker())
    }

    @Test
    fun aFreeTierGameNeverWarns() {
        assertNull(gameMembershipRequirement(game("Free"), subscription("FREE"), null))
    }

    @Test
    fun anUnrecognisedLabelStaysQuietRatherThanGuessing() {
        // A spurious gate in front of Play is worse than no gate at all.
        assertNull(gameMembershipRequirement(game("Day Pass"), subscription("FREE"), null))
    }

    @Test
    fun aFreeAccountIsWarnedAboutAnUltimateGame() {
        val requirement = gameMembershipRequirement(
            game("GeForce NOW Ultimate"),
            subscription("FREE"),
            null,
        )

        assertNotNull(requirement)
        assertEquals("Ultimate", requirement?.requiredPlanLabel)
        assertEquals("Free", requirement?.currentPlanLabel)
    }

    @Test
    fun aPerformanceAccountIsWarnedAboutUltimateButNotAboutPerformance() {
        assertNotNull(gameMembershipRequirement(game("Ultimate"), subscription("PERFORMANCE"), null))
        assertNull(gameMembershipRequirement(game("Performance"), subscription("PERFORMANCE"), null))
    }

    @Test
    fun anUltimateAccountIsNeverWarned() {
        assertNull(gameMembershipRequirement(game("Ultimate"), subscription("ULTIMATE"), null))
        assertNull(gameMembershipRequirement(game("Priority"), subscription("ULTIMATE"), null))
    }

    @Test
    fun theTierFromTheAuthSessionCountsWhenNoSubscriptionHasLoadedYet() {
        assertNull(gameMembershipRequirement(game("Ultimate"), null, "ULTIMATE"))
        assertNotNull(gameMembershipRequirement(game("Ultimate"), null, "FREE"))
    }
}
