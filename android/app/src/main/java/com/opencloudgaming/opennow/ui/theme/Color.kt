package com.opencloudgaming.opennow.ui.theme

import androidx.compose.ui.graphics.Color
import com.opencloudgaming.opennow.StreamQualityLevel

/**
 * The single source of truth for every colour in the app.
 *
 * Before this existed the palette was declared twice (once in `OpenNowScreens.kt`, once in
 * `OpenNowSettingsScreens.kt` with identical hex under different names) and another ~75 one-off
 * `Color(0x..)` literals were scattered inline. Anything that appears more than once belongs here.
 */
object OpenNowPalette {
    // Core surfaces (OpenNOW V3 "console" look: deep navy, glass, white seams)
    val Background = Color(0xff0b0f1a)
    val Panel = Color(0xff141826)
    val PanelAlt = Color(0xff1b2030)

    // Text
    val TextPrimary = Color(0xfff5f7fb)
    val TextMuted = Color(0xff9aa3b5)

    // V3 glass chrome and pastel accents, shared by the top screen and the bottom-screen deck.
    /** Pill and panel fill over art or video. */
    val Glass = Color(0x9e0e1018)
    val GlassStrong = Color(0xd10e1018)
    /** 14% white hairline around glass pills and tiles. */
    val Seam = Color.White.copy(alpha = 0.14f)
    /** Controller focus ring: sky ring separated from the tile by a navy gap. */
    val FocusRing = Color(0xff7fd4ff)
    val PastelViolet = Color(0xffa78bfa)
    val PastelSky = Color(0xff7fd4ff)
    val PastelYellow = Color(0xffffd166)
    val PastelMint = Color(0xff6ee7b7)
    val PastelCoral = Color(0xffff8a80)

    /** Sits on top of the accent — near-black so bright accents stay legible. */
    val OnAccent = Color(0xff08090c)

    // Accents (mirrors UiAccent in Models.kt)
    val AccentDefault = Color(0xff7fd4ff)
    val AccentDefaultSecondary = Color(0xffffffff)
    val AccentPixel = Color(0xff8ab4f8)
    val AccentHotPink = Color(0xffff4fb8)
    val AccentLime = Color(0xffc7ef6b)
    val AccentCoral = Color(0xffff8d7a)
    val AccentViolet = Color(0xffc7a4ff)
    /** Reserved for the animated Absolute Cinema focus/hover energy, not ordinary theme chrome. */
    val AccentCinemaOrange = Color(0xffff6a2b)
    val AccentCinemaBlue = Color(0xff42c9ff)
    val AccentSwitchRed = Color(0xffff4554)
    val AccentSwitchBlue = Color(0xff66d9ff)

    // Chrome
    /** Translucent wash behind the top bar so content scrolls under it legibly. */
    val ChromeScrim = Color.Black.copy(alpha = 0.16f)

    // Feedback
    val ErrorContainer = Color(0xff33181c)
    val OnErrorContainer = Color(0xffffb8bf)

    /**
     * The quality ladder, shared by the in-stream stats pill and the post-session report so the two
     * stop disagreeing about what "bad" looks like. Good deliberately has no tint of its own —
     * colouring the normal case just makes the abnormal one harder to spot.
     */
    val StatusGood = PastelMint
    val StatusFair = Color(0xffffc95a)
    val StatusPoor = PastelCoral

    /** Advisory notices that are neither an error nor a quality reading — privacy disclosures. */
    val StatusNotice = Color(0xffffc266)

    // Chrome that sits on top of live video
    /**
     * Panel fill. Deliberately not opaque — the whole point of the controls panel is to be usable
     * without leaving the game — but firm enough that TextMuted still clears 4.5:1 over bright
     * gameplay, which it does not at the old 0.93.
     */
    val PanelOverVideo = Panel.copy(alpha = 0.96f)

    /**
     * Row fills inside a panel over video. Opaque tones rather than translucent white, which used
     * to composite differently against every frame of the game behind it.
     */
    val PanelRowRest = Color(0xff1b2131)
    val PanelRowFocused = Color(0xff283044)

    /** Hairline that keeps an overlay's edge visible against a bright frame. */
    val PanelHairline = Seam

    /** Full-screen wash behind a stream overlay. */
    val StreamScrim = Color.Black.copy(alpha = 0.55f)

    // Imagery
    /** Backdrop for box art that is still loading, empty, or failed. */
    val ImagePlaceholder = Color(0xff111626)

    /** Base tone the shimmer band sweeps across. */
    val ShimmerBase = Color(0xff0f1422)

    /** Backdrop behind the catalog wallpaper. */
    val WallpaperBackdrop = Color(0xff080b14)
}

/**
 * Colour for a quality reading, or `null` when the metric is fine and should simply render in the
 * normal text colour. Returning null rather than a "good" green is deliberate: if every number is
 * tinted, none of them stand out.
 */
fun StreamQualityLevel.tint(): Color? = when (this) {
    StreamQualityLevel.Good -> null
    StreamQualityLevel.Fair -> OpenNowPalette.StatusFair
    StreamQualityLevel.Poor -> OpenNowPalette.StatusPoor
}
