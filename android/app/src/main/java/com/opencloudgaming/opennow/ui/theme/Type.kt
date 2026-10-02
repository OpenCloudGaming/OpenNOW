package com.opencloudgaming.opennow.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.opencloudgaming.opennow.R

/**
 * Inter Variable (numeric readouts), SIL Open Font License 1.1 (see `app/licenses/Inter-OFL-1.1.txt`).
 *
 * Chosen over Roboto for three reasons that matter to this app specifically: it ships tabular
 * figures and a slashed zero (the UI is full of numeric readouts), its tall x-height and open
 * apertures read far better at TV viewing distance, and one variable file covers every weight.
 *
 * `minSdk` is 23 but variable-font axes need API 26. On 23–25 the file loads at its default
 * instance and Compose applies synthetic bolding — an acceptable degradation on a shrinking
 * slice of devices.
 */
@OptIn(ExperimentalTextApi::class)
private fun interWeight(weight: FontWeight) = Font(
    resId = R.font.inter_variable,
    weight = weight,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
)

/** Nunito Variable, SIL Open Font License 1.1 (see `app/licenses/Nunito-OFL-1.1.txt`). The V3 display and UI face. */
private fun nunitoWeight(weight: FontWeight) = Font(
    resId = R.font.nunito_variable,
    weight = weight,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
)

val Nunito = FontFamily(
    nunitoWeight(FontWeight.Normal),
    nunitoWeight(FontWeight.Medium),
    nunitoWeight(FontWeight.SemiBold),
    nunitoWeight(FontWeight.Bold),
    nunitoWeight(FontWeight.ExtraBold),
    nunitoWeight(FontWeight.Black),
)

val Inter = FontFamily(
    interWeight(FontWeight.Normal),
    interWeight(FontWeight.Medium),
    interWeight(FontWeight.SemiBold),
    interWeight(FontWeight.Bold),
    interWeight(FontWeight.ExtraBold),
)

/**
 * Weight and tracking are baked into each style, so call sites stop appending
 * `fontWeight = FontWeight.ExtraBold` by hand — which is how the previous UI ended up with the
 * same visual role rendered at three different weights on three different screens.
 *
 * **Every `letterSpacing` here must be `sp`, and every style must set one explicitly.**
 * `TextUnit` arithmetic throws `IllegalArgumentException: Cannot perform operation for Sp and Em`
 * when the two operands use different units, and Material components lerp between typography
 * styles — `OutlinedTextField` animates its label between `bodyLarge` and `bodySmall`, for one.
 * An earlier revision expressed tracking in `em` but left `titleSmall` and `bodyLarge` on
 * Material's `sp` defaults, which crashed the Stream settings page the moment a text field with a
 * label was composed. Material also lerps against its own hardcoded `sp` styles, so `sp`
 * throughout is the only combination that cannot collide. `TypographyUnitsTest` enforces this.
 */
val OpenNowTypography = Typography().run {
    copy(
        displayLarge = displayLarge.copy(
            fontFamily = Nunito, fontWeight = FontWeight.Black,
            fontSize = 44.sp, lineHeight = 50.sp, letterSpacing = (-0.88).sp,
        ),
        displayMedium = displayMedium.copy(
            fontFamily = Nunito, fontWeight = FontWeight.Black,
            fontSize = 36.sp, lineHeight = 42.sp, letterSpacing = (-0.72).sp,
        ),
        displaySmall = displaySmall.copy(
            fontFamily = Nunito, fontWeight = FontWeight.Black,
            fontSize = 30.sp, lineHeight = 36.sp, letterSpacing = (-0.54).sp,
        ),
        headlineLarge = headlineLarge.copy(
            fontFamily = Nunito, fontWeight = FontWeight.Black,
            fontSize = 27.sp, lineHeight = 33.sp, letterSpacing = (-0.38).sp,
        ),
        headlineMedium = headlineMedium.copy(
            fontFamily = Nunito, fontWeight = FontWeight.ExtraBold,
            fontSize = 24.sp, lineHeight = 30.sp, letterSpacing = (-0.29).sp,
        ),
        headlineSmall = headlineSmall.copy(
            fontFamily = Nunito, fontWeight = FontWeight.ExtraBold,
            fontSize = 22.sp, lineHeight = 28.sp, letterSpacing = (-0.24).sp,
        ),
        titleLarge = titleLarge.copy(
            fontFamily = Nunito, fontWeight = FontWeight.ExtraBold,
            fontSize = 19.sp, lineHeight = 25.sp, letterSpacing = (-0.15).sp,
        ),
        titleMedium = titleMedium.copy(
            fontFamily = Nunito, fontWeight = FontWeight.ExtraBold,
            fontSize = 16.sp, lineHeight = 22.sp, letterSpacing = (-0.06).sp,
        ),
        titleSmall = titleSmall.copy(
            fontFamily = Nunito, fontWeight = FontWeight.Bold,
            fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.1.sp,
        ),
        bodyLarge = bodyLarge.copy(
            fontFamily = Nunito, fontWeight = FontWeight.SemiBold,
            fontSize = 16.sp, lineHeight = 24.sp, letterSpacing = 0.5.sp,
        ),
        bodyMedium = bodyMedium.copy(
            fontFamily = Nunito, fontWeight = FontWeight.SemiBold,
            fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.1.sp,
        ),
        bodySmall = bodySmall.copy(
            fontFamily = Nunito, fontWeight = FontWeight.SemiBold,
            fontSize = 12.sp, lineHeight = 17.sp, letterSpacing = 0.12.sp,
        ),
        labelLarge = labelLarge.copy(
            fontFamily = Nunito, fontWeight = FontWeight.ExtraBold,
            fontSize = 13.sp, lineHeight = 16.sp, letterSpacing = 0.26.sp,
        ),
        labelMedium = labelMedium.copy(
            fontFamily = Nunito, fontWeight = FontWeight.Bold,
            fontSize = 12.sp, lineHeight = 15.sp, letterSpacing = 0.3.sp,
        ),
        labelSmall = labelSmall.copy(
            fontFamily = Nunito, fontWeight = FontWeight.Bold,
            fontSize = 11.sp, lineHeight = 14.sp, letterSpacing = 0.44.sp,
        ),
    )
}

/**
 * Tabular, slashed-zero figures for anything that updates in place — queue position, FPS, bitrate,
 * latency, slider values. Proportional digits make those readouts visibly jitter on every tick.
 */
val OpenNowNumericStyle = TextStyle(
    fontFamily = Inter,
    fontFeatureSettings = "tnum, zero",
)

/** Applies tabular figures while keeping whatever size/weight the caller already chose. */
fun TextStyle.numeric(): TextStyle = copy(fontFeatureSettings = "tnum, zero")
