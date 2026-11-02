package com.opencloudgaming.opennow

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.text.font.FontWeight
import com.opencloudgaming.opennow.ui.controls.ControlRowStyle
import com.opencloudgaming.opennow.ui.controls.ControlSectionStyle
import com.opencloudgaming.opennow.ui.controls.LocalControlRowStyle
import com.opencloudgaming.opennow.ui.controls.LocalControlSectionStyle
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion

internal val LocalSpaceUiPresentation = staticCompositionLocalOf { false }

/** Classify the device/window independently of landscape width so phones retain square artwork. */
internal fun shouldUseSpacePortraitArtwork(tvProfile: Boolean, smallestScreenWidthDp: Int): Boolean =
    tvProfile || smallestScreenWidthDp >= 600

/** A single visual system across discovery, game hubs, settings, and session launch. */
@Composable
internal fun SpaceUiTheme(enabled: Boolean = true,
    physicalControllerConnected: Boolean = LocalSpaceControllerConnected.current, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (!enabled) MaterialTheme.colorScheme else MaterialTheme.colorScheme.copy(
            primary = SpaceWhite, onPrimary = SpaceInk, primaryContainer = Color(0xFF233140),
            onPrimaryContainer = SpaceWhite, background = SpaceInk, onBackground = SpaceWhite,
            surface = Color(0xFF101C2B), onSurface = SpaceWhite,
            surfaceVariant = Color(0xFF1D2C3E), onSurfaceVariant = SpaceMuted,
            secondary = SpaceIce, onSecondary = SpaceInk,
            secondaryContainer = Color(0xFF26313F), onSecondaryContainer = SpaceWhite,
            tertiary = SpaceIce, onTertiary = SpaceInk,
            tertiaryContainer = Color(0xFF23313D), onTertiaryContainer = SpaceWhite,
            surfaceTint = SpaceWhite, surfaceDim = SpaceInk, surfaceBright = Color(0xFF273545),
            surfaceContainerLowest = SpaceInk, surfaceContainerLow = Color(0xFF101722),
            surfaceContainer = Color(0xFF151F2C), surfaceContainerHigh = Color(0xFF1C2938),
            surfaceContainerHighest = Color(0xFF243343),
            outline = SpaceWhite.copy(alpha = .25f), outlineVariant = SpaceWhite.copy(alpha = .12f),
            inverseSurface = SpaceWhite, inverseOnSurface = SpaceInk, inversePrimary = Color(0xFF37576D),
        ),
        shapes = if (!enabled) MaterialTheme.shapes else MaterialTheme.shapes.copy(
            extraSmall = RoundedCornerShape(4.dp), small = RoundedCornerShape(6.dp),
            medium = RoundedCornerShape(8.dp), large = RoundedCornerShape(12.dp),
            extraLarge = RoundedCornerShape(16.dp),
        ),
    ) {
        val rows = ControlRowStyle.settings().copy(
            shape = RoundedCornerShape(4.dp), containerRest = SpaceWhite.copy(alpha = .035f),
            containerFocused = SpaceWhite.copy(alpha = .13f), borderRestWidth = 0.dp,
            horizontalPadding = 18.dp, verticalPadding = 16.dp,
            labelStyle = MaterialTheme.typography.bodyLarge, labelWeight = FontWeight.Medium,
            supportingColor = SpaceMuted, focusable = true, showFocusRing = true,
        )
        val sections = ControlSectionStyle.settings().copy(
            usesCard = false, titleStyle = MaterialTheme.typography.labelLarge,
            titleWeight = FontWeight.Medium, titleColor = SpaceMuted, itemSpacing = 2.dp,
        )
        CompositionLocalProvider(
            LocalSpaceControllerConnected provides physicalControllerConnected,
            LocalSpaceUiPresentation provides (enabled || LocalSpaceUiPresentation.current),
            androidx.compose.material3.LocalContentColor provides (if (enabled) SpaceWhite else androidx.compose.material3.LocalContentColor.current),
            LocalControlRowStyle provides (if (enabled) rows else LocalControlRowStyle.current),
            LocalControlSectionStyle provides (if (enabled) sections else LocalControlSectionStyle.current),
            LocalActiveSelectionColor provides (if (enabled) SpaceWhite else LocalActiveSelectionColor.current),
            LocalActiveSelectionSecondaryColor provides (if (enabled) SpaceWhite else LocalActiveSelectionSecondaryColor.current),
            LocalSelectionTintColor provides (if (enabled) SpaceWhite else LocalSelectionTintColor.current),
            LocalAbsoluteCinemaEffects provides (if (enabled) false else LocalAbsoluteCinemaEffects.current),
            LocalAbsoluteCinemaEverywhere provides (if (enabled) false else LocalAbsoluteCinemaEverywhere.current),
        ) { content() }
    }
}

/** Clip after applying insets: zooming artwork and entrance layers stay inside the camera-safe scene. */
@Composable
internal fun Modifier.spaceSafeViewport(insets: WindowInsets = WindowInsets.displayCutout): Modifier =
    background(SpaceInk).windowInsetsPadding(insets).clipToBounds()

@Composable
internal fun SpaceSceneBackdrop(url: String?) {
    val reduceMotion = LocalReduceMotion.current
    Box(Modifier.fillMaxSize().clipToBounds().background(SpaceInk)) {
        Crossfade(url, animationSpec = tween(if (reduceMotion) 0 else if (LocalSpaceFastNavigation.current) 80 else 480), label = "space-scene") { image ->
            if (image != null) UrlImage(image, Modifier.fillMaxSize().spaceArtworkArrival(image))
        }
        Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(listOf(SpaceInk.copy(alpha = .96f), SpaceInk.copy(alpha = .28f)))))
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(SpaceInk.copy(alpha = .36f), Color.Transparent, SpaceInk))))
        // Cached, static lighting gives unillustrated screens the same depth as game hubs.
        // No blur, particles, timers, or per-frame background work on phone/TV GPUs.
        Box(Modifier.fillMaxSize().drawWithCache {
            val horizon = Brush.radialGradient(
                listOf(Color(0xFF447BC2).copy(alpha = .17f), Color.Transparent),
                center = Offset(size.width * .85f, size.height * 1.12f),
                radius = size.maxDimension * .8f,
            )
            val overhead = Brush.radialGradient(
                listOf(SpaceIce.copy(alpha = .06f), Color.Transparent),
                center = Offset(size.width * .15f, -size.height * .3f),
                radius = size.maxDimension * .6f,
            )
            onDrawBehind { drawRect(horizon); drawRect(overhead) }
        })
    }
}
