package com.opencloudgaming.opennow

import androidx.compose.foundation.hoverable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.outlined.FavoriteBorder
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.*
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun SpaceGameHub(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    removeLabel: String?,
    removeConfirmationText: String?,
    removeEnabled: Boolean,
    onRemove: (() -> Unit)?,
    onDismiss: () -> Unit,
    tvProfile: Boolean = false,
) {
    val context = LocalContext.current
    val launch = rememberGameDetailsLaunchActions(game, defaultVariantId, onPlay, onChooseStore, onPlayOnTv, onDismiss)
    val playRequester = remember { FocusRequester() }
    val galleryRequester = remember { FocusRequester() }
    val descriptionRequester = remember { FocusRequester() }
    val windowFocused = LocalWindowInfo.current.isWindowFocused
    var gallery by remember(game.id) { mutableStateOf(false) }
    val detailsScroll = rememberScrollState()
    val scrollScope = rememberCoroutineScope()
    val readScroll = remember(detailsScroll, scrollScope) { SpaceDetailsScroll(detailsScroll, scrollScope) }
    val scrollStep = with(LocalDensity.current) { 128.dp.toPx() }
    LaunchedEffect(game.id, gallery) { detailsScroll.scrollTo(0) }
    var confirmRemove by remember(game.id) { mutableStateOf(false) }
    val screenshots = remember(game) { (listOfNotNull(game.screenshotUrl) + game.screenshotUrls).distinct().take(6) }
    LaunchedEffect(game.id, gallery, windowFocused) {
        if (windowFocused) requestFocusWithRetry(if (gallery) galleryRequester else playRequester)
    }
    SpaceUiTheme {
        CompositionLocalProvider(LocalSpaceDetailsScroll provides readScroll) {
        BoxWithConstraints(Modifier.fillMaxSize().testTag("space-game-hub").spaceSafeViewport().onPreviewKeyEvent { event ->
            if (event.key !in listOf(Key.ButtonL1, Key.ButtonR1)) false
            else {
                // Consume both edges, switch once on release, and keep the underlying shell on its page.
                if (event.type == KeyEventType.KeyUp && screenshots.isNotEmpty() && !confirmRemove) gallery = !gallery
                true
            }
        }.onKeyEvent { readScroll.handleVerticalKey(it, scrollStep) }.lockedFocusGroup()) {
            val compact = maxHeight < 500.dp
            val narrow = maxWidth < 600.dp
            val portraitArtwork = shouldUseSpacePortraitArtwork(tvProfile, LocalConfiguration.current.smallestScreenWidthDp)
            val galleryWidth = minOf(if (narrow) 280.dp else 520.dp,
                (maxHeight - if (compact) 212.dp else 276.dp).coerceAtLeast(120.dp) * (16f / 9f))
            SpaceSceneBackdrop(gameTvBannerImageUrl(context, game))
            Column(Modifier.fillMaxSize().spaceArrival(game.id).padding(if (compact || narrow) 22.dp else 36.dp)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    SpaceBackButton(Modifier.testTag("space-hub-back"), onClick = onDismiss)
                    Spacer(Modifier.weight(1f))
                    if (screenshots.isNotEmpty() && !narrow) {
                        SpaceControl(stringResource(R.string.space_ui_overview), active = !gallery, onClick = { gallery = false })
                        SpaceControl(stringResource(R.string.space_ui_gallery), active = gallery, onClick = { gallery = true })
                    }
                }
                if (screenshots.isNotEmpty() && narrow) {
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        SpaceControl(stringResource(R.string.space_ui_overview), active = !gallery, onClick = { gallery = false })
                        SpaceControl(stringResource(R.string.space_ui_gallery), active = gallery, onClick = { gallery = true })
                    }
                }
                BoxWithConstraints(Modifier.weight(1f).fillMaxWidth().onGloballyPositioned { readScroll.viewport = it }) {
                val overviewHeight = (maxHeight - (if (compact) 20.dp else 40.dp) - 8.dp).coerceAtLeast(0.dp)
                Column(Modifier.fillMaxSize().testTag("space-hub-scroll").spaceArrival(gallery).verticalScroll(detailsScroll)
                    .padding(top = if (compact) 20.dp else 40.dp, bottom = 48.dp),
                    verticalArrangement = Arrangement.spacedBy(if (compact) 14.dp else 24.dp)) {
                    Column(Modifier.fillMaxWidth().heightIn(min = if (gallery) 0.dp else overviewHeight),
                        verticalArrangement = Arrangement.spacedBy(if (compact) 14.dp else 24.dp)) {
                        if (!gallery) {
                            Column(Modifier.fillMaxWidth().testTag("space-hub-hero"), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                                    if (portraitArtwork || (!narrow && !compact)) {
                                        UrlImage(catalogCardImageUrl(game, tvProfile = !portraitArtwork,
                                            requestWidth = if (portraitArtwork) 512 else 272),
                                            (if (portraitArtwork) Modifier.width(if (compact) 86.dp else 108.dp).aspectRatio(628f / 888f)
                                            else Modifier.size(128.dp)).testTag("space-hub-artwork").clip(RoundedCornerShape(20.dp)))
                                    }
                                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                        Text(game.title, color = SpaceWhite, fontSize = if (compact || narrow) 34.sp else 56.sp,
                                            lineHeight = if (compact || narrow) 40.sp else 62.sp, fontWeight = FontWeight.Light,
                                            maxLines = 3, overflow = TextOverflow.Ellipsis)
                                        game.publisherName?.takeIf { it.isNotBlank() }?.let { author ->
                                            Text(author, color = SpaceIce, fontSize = 14.sp, maxLines = 1,
                                                overflow = TextOverflow.Ellipsis, modifier = Modifier.testTag("space-hub-author"))
                                        }
                                        Row(Modifier.fillMaxWidth().testTag("space-hub-actions"),
                                            horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                            SpaceControl(stringResource(if (game.requiresMembershipUpgrade()) R.string.catalog_upgrade_to_play else R.string.action_play),
                                                Modifier.focusRequester(playRequester).spaceDetailsHeroAction().focusProperties { down = descriptionRequester }
                                                    .testTag("space-hub-play"), filled = true, minHeight = 52.dp,
                                                onClick = { launch(GameDetailsLaunchAction.Play) })
                                            SpaceIconButton(if (favorite) Icons.Filled.Favorite else Icons.Outlined.FavoriteBorder,
                                                stringResource(if (favorite) R.string.controller_action_unfavorite else R.string.controller_action_favorite),
                                                Modifier.testTag("space-hub-favorite").spaceDetailsHeroAction(), active = favorite, size = 52.dp, onClick = { onFavorite(game.id) })
                                            if (connectedTvName != null) SpaceControl(stringResource(R.string.action_play_on_tv, connectedTvName),
                                                Modifier.spaceDetailsHeroAction(), minHeight = 52.dp, onClick = { launch(GameDetailsLaunchAction.PlayOnTv) })
                                        }
                                    }
                                }
                                GameAvailabilityBanner(game.availability(defaultVariantId), compact = compact)
                            }
                        } else {
                            Text(stringResource(R.string.space_ui_gallery), color = SpaceWhite, fontWeight = FontWeight.Light, fontSize = 32.sp)
                            LazyRow(horizontalArrangement = Arrangement.spacedBy(18.dp), contentPadding = PaddingValues(horizontal = 24.dp, vertical = 20.dp)) {
                                itemsIndexed(screenshots, key = { _, url -> url }) { index, url ->
                                    var imageFocused by remember { mutableStateOf(false) }
                                    val focusManager = LocalFocusManager.current
                                    val interaction = rememberSpaceInteraction(imageFocused, tile = true)
                                    val description = stringResource(R.string.space_ui_gallery)
                                    UrlImage(gameTvBannerImageUrl(context, game.copy(tvBannerUrl = url)),
                                        Modifier.width(galleryWidth).aspectRatio(16f / 9f).testTag("space-gallery-image")
                                            .then(if (index == 0) Modifier.focusRequester(galleryRequester) else Modifier)
                                            .then(interaction.modifier).hoverable(interaction.source)
                                            .spaceFocusFrame(imageFocused || interaction.hovered, 20.dp, hovered = interaction.hovered)
                                            .clip(RoundedCornerShape(20.dp))
                                            .border(if (imageFocused || interaction.hovered) 2.dp else 0.dp, SpaceIce, RoundedCornerShape(20.dp))
                                        .onFocusChanged { imageFocused = it.isFocused }
                                            .onPreviewKeyEvent { handleDpadFocusMove(it, focusManager) }
                                            .focusable().semantics { contentDescription = description })
                                }
                            }
                        }
                        SpaceGameAbout(game, narrow, descriptionRequester,
                            previousRequester = if (gallery) galleryRequester else playRequester)
                    }
                    SpaceGameDetails(game, defaultVariantId)
                    if (removeLabel != null && removeEnabled && onRemove != null) {
                        SpaceControl(removeLabel, onClick = {
                            if (removeConfirmationText != null) confirmRemove = true else onRemove()
                        })
                    }

                }
                if (detailsScroll.canScrollForward) {
                    SpaceScrollCue(Modifier.align(Alignment.BottomEnd).padding(end = 4.dp, bottom = 12.dp))
                }
                }

            }
        }
        if (confirmRemove && onRemove != null) {
            AlertDialog(onDismissRequest = { confirmRemove = false },
                title = { Text(removeLabel.orEmpty()) }, text = { Text(removeConfirmationText.orEmpty()) },
                confirmButton = { TextButton(onClick = { confirmRemove = false; onRemove() }) { Text(stringResource(R.string.action_continue)) } },
                dismissButton = { TextButton(onClick = { confirmRemove = false }) { Text(stringResource(R.string.action_cancel)) } })
        }
    }
    }
}
