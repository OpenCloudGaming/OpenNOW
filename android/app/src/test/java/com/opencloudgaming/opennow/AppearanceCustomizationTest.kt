package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppearanceCustomizationTest {
    @Test
    fun lockedAppearanceKeepsSavedColoursAndBackgroundDuringOtherEdits() {
        val saved = AppSettings(
            uiAccent = UiAccent.Violet,
            dynamicColor = true,
            nerdCatalogBackground = true,
            ambientBackgroundEnabled = false,
            catalogBackgroundPreset = CatalogBackgroundPreset.Original,
            nerdCatalogBackgroundUri = "content://images/saved",
            navigationRailBackgroundOpacity = 0.4f,
        )
        val next = AppSettings(showCardTitles = true)

        assertEquals(saved.copy(showCardTitles = true), next.withAppearanceFrom(saved))
    }

    @Test
    fun appearanceRequiresAnUnlockOnPhonesAndRemainsAvailableOnTv() {
        assertFalse(OpenNowUiState().canCustomizeAppearance)
        assertTrue(OpenNowUiState(appearanceCustomizationUnlocked = true).canCustomizeAppearance)
        assertTrue(OpenNowUiState(androidTvProfile = true).canCustomizeAppearance)
    }
}
