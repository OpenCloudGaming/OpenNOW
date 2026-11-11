package com.opencloudgaming.opennow

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontListFontFamily
import androidx.compose.ui.text.font.FontLoadingStrategy
import androidx.compose.ui.unit.TextUnitType
import androidx.compose.ui.unit.isSpecified
import androidx.compose.ui.unit.lerp
import com.opencloudgaming.opennow.ui.theme.OpenNowTypography
import com.opencloudgaming.opennow.ui.theme.Inter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Guards the one thing about [OpenNowTypography] that fails at runtime rather than at compile time.
 *
 * `TextUnit` arithmetic throws `IllegalArgumentException: Cannot perform operation for Sp and Em`
 * when its operands use different units, and Material components lerp between typography styles —
 * `OutlinedTextField` interpolates its label between `bodyLarge` and `bodySmall`. A revision that
 * expressed tracking in `em` while leaving two styles on Material's `sp` defaults crashed the
 * Stream settings page as soon as a labelled text field was composed. Nothing in the type system
 * catches that, so it is checked here.
 */
class TypographyUnitsTest {

    @Test
    fun interWeightsDoNotLoadSynchronouslyDuringTextMeasurement() {
        val fonts = Inter as FontListFontFamily
        assertEquals(5, fonts.fonts.size)
        fonts.fonts.forEach { font ->
            assertEquals(FontLoadingStrategy.Async, font.loadingStrategy)
        }
    }

    private fun allStyles(typography: Typography): List<Pair<String, TextStyle>> = listOf(
        "displayLarge" to typography.displayLarge,
        "displayMedium" to typography.displayMedium,
        "displaySmall" to typography.displaySmall,
        "headlineLarge" to typography.headlineLarge,
        "headlineMedium" to typography.headlineMedium,
        "headlineSmall" to typography.headlineSmall,
        "titleLarge" to typography.titleLarge,
        "titleMedium" to typography.titleMedium,
        "titleSmall" to typography.titleSmall,
        "bodyLarge" to typography.bodyLarge,
        "bodyMedium" to typography.bodyMedium,
        "bodySmall" to typography.bodySmall,
        "labelLarge" to typography.labelLarge,
        "labelMedium" to typography.labelMedium,
        "labelSmall" to typography.labelSmall,
    )

    @Test
    fun everyStyleDeclaresLetterSpacingInSp() {
        allStyles(OpenNowTypography).forEach { (name, style) ->
            assertTrue(
                "$name has no explicit letterSpacing; it would inherit a unit we do not control",
                style.letterSpacing.isSpecified,
            )
            assertEquals(
                "$name must express letterSpacing in sp — mixing sp and em crashes TextUnit arithmetic",
                TextUnitType.Sp,
                style.letterSpacing.type,
            )
        }
    }

    @Test
    fun everyStylePairCanBeInterpolated() {
        // Exactly what OutlinedTextField does to its label, across every pair so a future edit to
        // any single style cannot reintroduce the crash.
        val styles = allStyles(OpenNowTypography)
        styles.forEach { (fromName, from) ->
            styles.forEach { (toName, to) ->
                runCatching { lerp(from.letterSpacing, to.letterSpacing, 0.5f) }
                    .onFailure { error ->
                        throw AssertionError("lerp($fromName, $toName) failed: ${error.message}")
                    }
            }
        }
    }

    @Test
    fun interpolatingAgainstMaterialDefaultsIsSafe() {
        // Material lerps against its own hardcoded styles too, not only against ours.
        val defaults = allStyles(Typography())
        allStyles(OpenNowTypography).forEach { (name, style) ->
            defaults.forEach { (defaultName, default) ->
                runCatching { lerp(style.letterSpacing, default.letterSpacing, 0.5f) }
                    .onFailure { error ->
                        throw AssertionError("lerp($name, Material $defaultName) failed: ${error.message}")
                    }
            }
        }
    }
}
