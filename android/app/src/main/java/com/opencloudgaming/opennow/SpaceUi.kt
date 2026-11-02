package com.opencloudgaming.opennow

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.focusable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.grid.itemsIndexed as gridItemsIndexed
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.Explore
import androidx.compose.material.icons.outlined.VideoLibrary
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.*
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first

internal val SpaceInk = Color(0xFF080E18)
internal val SpaceWhite = Color(0xFFF4F7FC)
internal val SpaceMuted = Color(0xFFB0BCCC)
internal val SpaceIce = Color(0xFFB8E6EF)

/** The experimental preference selects the shell; live video keeps its streaming surface. */
internal fun shouldUseSpaceUi(enabled: Boolean, page: AppPage): Boolean =
    enabled && page != AppPage.Stream

/** Browsing uses immersive bars only in landscape; stream bars retain their existing owner. */
internal fun shouldHideSpaceUiSystemBars(enabled: Boolean, landscape: Boolean, page: AppPage): Boolean =
    shouldUseSpaceUi(enabled, page) && landscape

@Composable
internal fun SpaceUiSettingsToggle(checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    SettingSwitch(
        label = stringResource(R.string.space_ui_name),
        checked = checked,
        onCheckedChange = onCheckedChange,
    )
}

/** Shares catalog derivations with the regular shell; no second catalog or launch owner. */
internal fun spaceUiGames(state: OpenNowUiState): List<GameInfo> = if (state.page == AppPage.Library) {
    val terms = searchTermsFor(state.librarySearch)
    sortLibraryGames(favoriteOrderedGames(state.libraryGames, state.settings.favoriteGameIds), state.librarySortId)
        .filter { gameMatchesSearch(it, terms) && gameMatchesLibraryFilters(it, state.libraryFilterIds, state.settings.demoMode) }
} else {
    filterCatalogGamesForLocalControls(state.games.ifEmpty { state.catalogResult.games }, state.catalogFilterIds)
}

@Composable
internal fun SpaceUiScreen(
    state: OpenNowUiState,
    onNavigate: (AppPage) -> Unit,
    onSelectGame: (GameInfo) -> Unit,
    onRefresh: () -> Unit,
    onSearchChange: (String) -> Unit,
    onResumeSession: () -> Unit,
    inputEnabled: Boolean,
    settingsContent: @Composable () -> Unit,
    catalogControls: @Composable () -> Unit = {},
    libraryContent: @Composable () -> Unit = {},
    queueContent: @Composable (compact: Boolean) -> Unit = {},
    onOpenLocalApps: () -> Unit = { onNavigate(AppPage.Settings) },
    onSpaceUiChange: (Boolean) -> Unit = {},
) {
    val games = remember(state.page, state.games, state.catalogResult.games, state.libraryGames,
        state.catalogFilterIds, state.libraryFilterIds, state.librarySearch, state.librarySortId,
        state.settings.favoriteGameIds, state.settings.demoMode) { spaceUiGames(state) }
    var storeSelectedId by rememberSaveable { mutableStateOf<String?>(null) }
    var librarySelectedId by rememberSaveable { mutableStateOf<String?>(null) }
    val selectedId = if (state.page == AppPage.Library) librarySelectedId else storeSelectedId
    val game = games.firstOrNull { it.id == selectedId } ?: games.firstOrNull()
    var searchOpen by rememberSaveable(state.page) { mutableStateOf(false) }
    val query = if (state.page == AppPage.Library) state.librarySearch else state.catalogSearch
    val listState = rememberLazyListState()
    val discoveryScroll = rememberScrollState()
    var discoverCatalog by rememberSaveable { mutableStateOf(false) }
    BackHandler(enabled = inputEnabled && state.page == AppPage.Home && discoverCatalog) { discoverCatalog = false }
    LaunchedEffect(discoverCatalog) { if (!discoverCatalog) discoveryScroll.scrollTo(0) }
    val storeGridState = rememberLazyGridState()
    val libraryGridState = rememberLazyGridState()
    val gridState = if (state.page == AppPage.Library) libraryGridState else storeGridState
    val portraitLayout = LocalConfiguration.current.screenWidthDp < LocalConfiguration.current.screenHeightDp
    val profileRequester = remember { FocusRequester() }
    val selectedRequester = remember { FocusRequester() }
    val navigationRequester = remember { FocusRequester() }
    val searchRequester = remember { FocusRequester() }
    val settingsEntryRequester = remember { FocusRequester() }
    val reduceMotion = LocalReduceMotion.current
    val context = LocalContext.current
    var backdropUrl by remember { mutableStateOf(game?.let { gameTvBannerImageUrl(context, it) }) }
    LaunchedEffect(game?.id, reduceMotion, discoverCatalog) {
        if (discoverCatalog && state.page == AppPage.Home) { backdropUrl = null; return@LaunchedEffect }
        // Let fast D-pad traversal settle before starting another full-width artwork request.
        if (!reduceMotion) delay(100)
        backdropUrl = game?.let { gameTvBannerImageUrl(context, it) }
    }
    val family = rememberPhysicalControllerFamily(enabled = true)
    val keyboardNavigation = family != null || LocalInputModeManager.current.inputMode == InputMode.Keyboard
    val pages = remember { listOf(AppPage.Home, AppPage.Library, AppPage.Settings) }
    val searchVisible = searchOpen || query.isNotBlank()
    val windowFocused = LocalWindowInfo.current.isWindowFocused
    LaunchedEffect(state.page, games.isNotEmpty(), inputEnabled, windowFocused, portraitLayout, keyboardNavigation, discoverCatalog) {
        if (inputEnabled && windowFocused && (!portraitLayout || keyboardNavigation)) {
            if (searchVisible && state.page != AppPage.Settings && !(discoverCatalog && state.page == AppPage.Home)) {
                requestFocusWithRetry(searchRequester, initialDelayMs = 0L)
            } else if (games.isNotEmpty() && state.page != AppPage.Settings) {
                val index = games.indexOfFirst { it.id == game?.id }.coerceAtLeast(0)
                if (state.page == AppPage.Library || discoverCatalog) {
                    gridState.scrollToItem(index + if (portraitLayout && state.page == AppPage.Library) 1 else 0)
                    snapshotFlow { gridState.layoutInfo.visibleItemsInfo.any { it.key == game?.id } }.first { it }
                } else {
                    listState.scrollToItem(index)
                    snapshotFlow { listState.layoutInfo.visibleItemsInfo.any { it.key == game?.id } }.first { it }
                }
                requestFocusWithRetry(selectedRequester, initialDelayMs = 0L)
            } else {
                requestFocusWithRetry(navigationRequester, initialDelayMs = 0L)
            }
        }
    }
    LaunchedEffect(searchOpen) {
        if (searchOpen) {
            requestFocusWithRetry(searchRequester)
        }
    }
    val fastNavigation = rememberSpaceFastNavigation(listState, game?.id,
        enabled = inputEnabled && state.page == AppPage.Home && !discoverCatalog && !reduceMotion)
    CompositionLocalProvider(LocalSpaceFastNavigation provides fastNavigation) {
    BoxWithConstraints(
        Modifier.fillMaxSize().testTag("space-ui").spaceSafeViewport()
            .onPreviewKeyEvent { event ->
                if (!inputEnabled || event.key !in listOf(Key.ButtonL1, Key.ButtonR1)) false
                else {
                    if (event.type == KeyEventType.KeyUp) {
                        val delta = if (event.key == Key.ButtonR1) 1 else -1
                        onNavigate(pages[(pages.indexOf(state.page) + delta + pages.size) % pages.size])
                    }
                    true
                }
            },
    ) {
        val compact = maxHeight < 560.dp
        val portrait = maxWidth < maxHeight
        val iconNavigation = maxWidth < 600.dp && !portrait
        val compactQueue = portrait || maxWidth < 700.dp
        val inset = if (compact) 22.dp else if (portrait) 20.dp else 36.dp
        SpaceSceneBackdrop(backdropUrl.takeUnless { portrait || state.page == AppPage.Settings })
        Column(Modifier.fillMaxSize().testTag("space-scene-content").padding(horizontal = inset, vertical = if (compact) 10.dp else 20.dp)) {
            Row(Modifier.fillMaxWidth().focusGroup().testTag("space-top-rail"), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(if (iconNavigation) 4.dp else 10.dp)) {
                TopBarProfileMenu(
                    state = state, focusRequester = profileRequester, nextFocusRequester = navigationRequester,
                    onOpenSettings = { onNavigate(AppPage.Settings) }, onOpenLocalApps = onOpenLocalApps,
                    onSpaceUiChange = onSpaceUiChange,
                )
                if (portrait) Spacer(Modifier.weight(1f))
                else SpaceNavigationTabs(state.page, pages, false, navigationRequester, onNavigate, Modifier.weight(1f), settingsEntryRequester,
                    iconsOnly = iconNavigation)
                queueContent(compactQueue)
                if (state.page != AppPage.Settings) {
                    SpaceControl(stringResource(R.string.nav_search), horizontalPadding = 12.dp, onClick = { searchOpen = !searchOpen }) {
                        Icon(Icons.Outlined.Search, null, Modifier.size(20.dp))
                    }
                    SpaceControl(stringResource(R.string.space_ui_refresh), horizontalPadding = 12.dp, onClick = onRefresh) {
                        Icon(Icons.Outlined.Refresh, null, Modifier.size(20.dp))
                    }
                }
            }
            if (portrait) {
                SpaceNavigationTabs(state.page, pages, true, navigationRequester, onNavigate,
                    Modifier.fillMaxWidth().padding(top = 10.dp, bottom = 12.dp), settingsEntryRequester)
            }
            if (state.page == AppPage.Settings) {
                Box(Modifier.weight(1f).spaceArrival(state.page).padding(top = 12.dp).clip(RoundedCornerShape(24.dp)).background(SpaceInk.copy(alpha = .85f))) {
                    CompositionLocalProvider(LocalSpaceSettingsEntry provides settingsEntryRequester) { settingsContent() }
                }
            } else if (portrait && state.page == AppPage.Library) {
                if (searchVisible) {
                    NativeSearchField(query, onSearchChange, stringResource(R.string.nav_search),
                        searching = state.loadingGames, focusRequester = searchRequester, gamepadImeMode = family != null,
                        modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp))
                }
                SpacePortraitCatalog(state, games, game, gridState, selectedRequester,
                    modifier = Modifier.weight(1f).spaceArrival(state.page),
                    onFocused = { id ->
                        if (state.page == AppPage.Library) librarySelectedId = id else storeSelectedId = id
                    }, onSelectGame = onSelectGame, onRefresh = onRefresh,
                    catalogControls = catalogControls, libraryContent = libraryContent,
                    onResumeSession = onResumeSession)
            } else if (state.page == AppPage.Library) {
                if (searchVisible) {
                    NativeSearchField(query, onSearchChange, stringResource(R.string.nav_search),
                        focusRequester = searchRequester, gamepadImeMode = true,
                        modifier = Modifier.padding(top = 10.dp).fillMaxWidth())
                }
                Box(Modifier.weight(1f).spaceArrival(state.page)) {
                    SpaceLibraryBody(state, games, game, gridState, selectedRequester,
                        compact = compact, portrait = portrait,
                        onFocused = { librarySelectedId = it }, onSelectGame = onSelectGame,
                        onRefresh = onRefresh, catalogControls = catalogControls, libraryContent = libraryContent)
                }
            } else if (discoverCatalog) {
                if (searchVisible) NativeSearchField(query, onSearchChange, stringResource(R.string.nav_search),
                    searching = state.loadingGames, focusRequester = searchRequester, gamepadImeMode = family != null,
                    modifier = Modifier.padding(top = 10.dp).fillMaxWidth())
                SpaceDiscoverCatalog(games, game?.id, storeGridState, selectedRequester, compact, state.settings.showPremiumMarker,
                    Modifier.weight(1f), onFocused = { storeSelectedId = it }, onSelectGame = onSelectGame,
                    onBack = { discoverCatalog = false }, catalogControls = catalogControls)
            } else {
                if (searchVisible) {
                    NativeSearchField(query = query, onQueryChange = onSearchChange,
                        placeholder = stringResource(R.string.nav_search), searching = state.loadingGames,
                        focusRequester = searchRequester, gamepadImeMode = true,
                        modifier = Modifier.padding(top = 10.dp).fillMaxWidth(if (portrait) 1f else .6f))
                }
                state.error?.takeIf { it.isNotBlank() }?.let { error ->
                    Text(error, color = Color(0xFFFFB5A9), fontSize = 12.sp, maxLines = 2,
                        overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 8.dp))
                }
                BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()
                    .expandSpaceCatalogOnSwipe(inputEnabled && games.isNotEmpty()) { discoverCatalog = true }) {
                    // Budget the full rail, its focus gutters and a touch-sized catalog action first.
                    val railArtworkSize = if (compact) (maxHeight * .30f).coerceIn(72.dp, 98.dp) else 134.dp
                    val heroHeight = (maxHeight - railArtworkSize - 116.dp).coerceAtLeast(if (compact) 68.dp else 150.dp)
                    val showHero = maxHeight >= 260.dp
                    val showHeroDescription = maxHeight >= 340.dp
                    val showCatalogPosition = maxWidth >= 480.dp
                    Column(Modifier.fillMaxSize().testTag("space-discover-scroll")
                        .spaceArrival("discover-rail").verticalScroll(discoveryScroll).padding(bottom = 16.dp)) {
                        Column(Modifier.heightIn(min = if (showHero || games.isEmpty()) heroHeight else 0.dp)
                            .padding(top = if (!showHero) 0.dp else if (compact) 14.dp else if (portrait) 20.dp else 32.dp)) {
                            if (state.activeSession != null) SpaceControl(stringResource(R.string.action_resume), onClick = onResumeSession)
                            if (game != null) {
                                if (showHero) SpaceDiscoveryHero(game, compact, portrait, showDescription = showHeroDescription)
                            }
                            else if (state.loadingGames) CircularProgressIndicator(color = SpaceIce, modifier = Modifier.size(28.dp))
                            else {
                                Text(stringResource(R.string.no_games_loaded), color = SpaceWhite, fontSize = 26.sp)
                                Text(stringResource(R.string.space_ui_empty), color = SpaceMuted, modifier = Modifier.padding(top = 12.dp))
                                SpaceControl(label = stringResource(R.string.space_ui_refresh), onClick = onRefresh)
                            }
                        }
                        Row(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                                if (games.isNotEmpty()) {
                                    val browseLabel = stringResource(R.string.space_ui_browse_all)
                                    SpaceControl(browseLabel, modifier = Modifier.testTag("space-browse-all"), minHeight = 48.dp,
                                        enabled = inputEnabled, onClick = { discoverCatalog = true }) {
                                        Icon(Icons.Outlined.GridView, contentDescription = null, modifier = Modifier.size(18.dp))
                                        Spacer(Modifier.width(8.dp))
                                        Text(browseLabel, fontSize = 13.sp)
                                    }
                                } else Text(stringResource(R.string.nav_store), color = SpaceMuted, fontSize = 11.sp, letterSpacing = 2.sp)
                                Spacer(Modifier.weight(1f))
                                if (showCatalogPosition) Text("${if (game == null) 0 else games.indexOf(game) + 1} / ${games.size}", color = SpaceMuted, fontSize = 11.sp)
                                Spacer(Modifier.width(12.dp))
                                catalogControls()
                        }
                        if (games.isNotEmpty()) {
                            LazyRow(state = listState, modifier = Modifier.fillMaxWidth().focusGroup().testTag("space-game-rail")
                                .onPreviewKeyEvent { event ->
                                    if (inputEnabled && event.key == Key.DirectionDown) {
                                        if (event.type == KeyEventType.KeyDown) discoverCatalog = true
                                        true
                                    } else false
                                },
                                contentPadding = PaddingValues(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(0.dp)) {
                                itemsIndexed(games, key = { _, item -> item.id }) { index, item ->
                                    SpaceGameTile(game = item, selected = game?.id == item.id,
                                        compact = compact, reduceMotion = reduceMotion,
                                        showPremiumMarker = state.settings.showPremiumMarker,
                                        artworkSize = railArtworkSize,
                                        modifier = Modifier.spaceArrival(item.id, (index.coerceAtMost(5) * 24)).then(if (game?.id == item.id) Modifier.focusRequester(selectedRequester) else Modifier),
                                        onFocused = {
                                            if (state.page == AppPage.Library) librarySelectedId = item.id
                                            else storeSelectedId = item.id
                                        }, onClick = { onSelectGame(item) })
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    }
}

@Composable
private fun SpaceNavigationTabs(page: AppPage, pages: List<AppPage>, portrait: Boolean, requester: FocusRequester,
    onNavigate: (AppPage) -> Unit, modifier: Modifier = Modifier, settingsEntryRequester: FocusRequester, iconsOnly: Boolean = false) {
    Row(modifier.then(if (portrait) Modifier else Modifier.horizontalScroll(rememberScrollState())),
        horizontalArrangement = Arrangement.spacedBy(if (portrait || iconsOnly) 4.dp else 14.dp)) {
        pages.forEach { destination ->
            SpaceControl(stringResource(when (destination) {
                AppPage.Home -> R.string.space_ui_discover
                AppPage.Library -> R.string.nav_library
                else -> R.string.nav_settings
            }), active = page == destination, showFocusOutline = page != AppPage.Settings,
                horizontalPadding = if (iconsOnly) 12.dp else 16.dp,
                modifier = (if (portrait) Modifier.weight(1f) else Modifier).spaceSelectionMark(page == destination).then(
                    if (page == destination) Modifier.focusRequester(requester) else Modifier)
                    .onPreviewKeyEvent { event ->
                        if (page == AppPage.Settings && event.key == Key.DirectionDown && event.type == KeyEventType.KeyDown) {
                            runCatching { settingsEntryRequester.requestFocus() }.getOrDefault(false)
                        } else false
                    },
                onClick = { onNavigate(destination) }, content = if (iconsOnly) {
                    {
                        Icon(when (destination) {
                            AppPage.Home -> Icons.Outlined.Explore
                            AppPage.Library -> Icons.Outlined.VideoLibrary
                            else -> Icons.Outlined.Settings
                        }, contentDescription = null, modifier = Modifier.size(20.dp))
                    }
                } else null)
        }
    }
}

@Composable
private fun SpaceDiscoveryHero(game: GameInfo, compact: Boolean, portrait: Boolean, showDescription: Boolean) {
    val title: @Composable () -> Unit = {
        Text(game.title, color = SpaceWhite, fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.Light,
            fontSize = if (compact) 32.sp else if (portrait) 38.sp else 54.sp,
            lineHeight = if (compact) 36.sp else if (portrait) 44.sp else 58.sp,
            maxLines = if (compact) 1 else 2, overflow = TextOverflow.Ellipsis)
        val metadata = storeHeroSubtitle(game).orEmpty()
        if (metadata.isNotBlank()) Text(metadata, color = SpaceIce, fontSize = 12.sp,
            modifier = Modifier.padding(top = 8.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (showDescription) gameDescriptionForDetails(game)?.let { description ->
            Text(description, color = SpaceMuted, fontSize = 14.sp, lineHeight = 22.sp,
                maxLines = 2, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag("space-discover-details").padding(top = 14.dp))
        }
    }
    Column(Modifier.fillMaxWidth(if (portrait) 1f else .72f).spaceArrival(game.id)) { title() }

}

@Composable
private fun SpaceLibraryBody(
    state: OpenNowUiState,
    games: List<GameInfo>,
    selectedGame: GameInfo?,
    gridState: LazyGridState,
    selectedRequester: FocusRequester,
    compact: Boolean,
    portrait: Boolean,
    onFocused: (String) -> Unit,
    onSelectGame: (GameInfo) -> Unit,
    onRefresh: () -> Unit,
    catalogControls: @Composable () -> Unit,
    libraryContent: @Composable () -> Unit,
) {
    val reduceMotion = LocalReduceMotion.current
    val overview: @Composable () -> Unit = {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(end = if (portrait) 0.dp else 24.dp),
            verticalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 20.dp)) {
            Text(stringResource(R.string.space_ui_collection), color = SpaceIce, fontSize = 11.sp, letterSpacing = 3.sp)
            Text(selectedGame?.title ?: stringResource(R.string.nav_library), color = SpaceWhite,
                fontWeight = FontWeight.Light, fontSize = if (portrait || compact) 26.sp else 40.sp,
                lineHeight = if (portrait || compact) 32.sp else 46.sp, maxLines = 3, overflow = TextOverflow.Ellipsis)
            if (!portrait && !compact) selectedGame?.let(::gameDescriptionForDetails)?.let { description ->
                Text(description, color = SpaceMuted, fontSize = 14.sp, lineHeight = 23.sp,
                    maxLines = 4, overflow = TextOverflow.Ellipsis)
            }
            libraryContent()
        }
    }
    val collection: @Composable () -> Unit = {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.library_count, games.size), color = SpaceMuted, fontSize = 12.sp)
                Spacer(Modifier.weight(1f))
                catalogControls()
            }
            state.error?.let { Text(it, color = MaterialTheme.colorScheme.error, maxLines = 2, overflow = TextOverflow.Ellipsis) }
            if (games.isEmpty()) {
                Text(stringResource(R.string.no_games_loaded), color = SpaceWhite, fontSize = 20.sp)
                Text(stringResource(R.string.space_ui_empty), color = SpaceMuted, fontSize = 13.sp)
                SpaceControl(stringResource(R.string.space_ui_refresh), onClick = onRefresh)
            } else LazyVerticalGrid(columns = GridCells.Fixed(4), state = gridState,
                contentPadding = PaddingValues(6.dp), verticalArrangement = Arrangement.spacedBy(0.dp),
                horizontalArrangement = Arrangement.spacedBy(0.dp), modifier = Modifier.weight(1f).testTag("space-library-grid")) {
                gridItemsIndexed(games, key = { _, game -> game.id }) { _, game ->
                    SpaceGameTile(game, selectedGame?.id == game.id, compact, reduceMotion,
                        showPremiumMarker = state.settings.showPremiumMarker,
                        modifier = Modifier.fillMaxWidth().then(
                            if (selectedGame?.id == game.id) Modifier.focusRequester(selectedRequester) else Modifier),
                        onFocused = { onFocused(game.id) }, onClick = { onSelectGame(game) }, fillWidth = true)
                }
            }
        }
    }
    if (portrait) Column(Modifier.fillMaxSize().padding(top = 20.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
        overview()
        Box(Modifier.weight(1f)) { collection() }
    } else Row(Modifier.fillMaxSize().padding(top = if (compact) 16.dp else 40.dp)) {
        Box(Modifier.weight(.34f)) { overview() }
        Box(Modifier.weight(.66f)) { collection() }
    }
}

@Composable
internal fun SpaceGameTile(game: GameInfo, selected: Boolean, compact: Boolean, reduceMotion: Boolean,
    showPremiumMarker: Boolean, modifier: Modifier, onFocused: () -> Unit, onClick: () -> Unit,
    portraitArtwork: Boolean = false, fillWidth: Boolean = false,
    artworkSize: Dp = if (compact) 98.dp else 134.dp) {
    val premiumMarker = showPremiumMarker && game.hasPremiumMembershipMarker()
    val premiumDescription = stringResource(R.string.catalog_premium_marker)
    var focused by remember { mutableStateOf(false) }
    val interaction = rememberSpaceInteraction(focused, tile = true, reduceMotion = reduceMotion)
    val latestOnFocused by rememberUpdatedState(onFocused)
    LaunchedEffect(interaction.hovered) { if (interaction.hovered) latestOnFocused() }
    val shape = RoundedCornerShape(14.dp)
    val focusManager = LocalFocusManager.current
    // The focus target includes the frame's gutter, so scroll-to-focus cannot crop its glow at a list edge.
    Box(modifier.onFocusChanged { focused = it.isFocused; if (it.isFocused) onFocused() }
        .hoverable(interaction.source)
        .onPreviewKeyEvent { handleDpadFocusMove(it, focusManager) }
        .focusProperties { canFocus = true }
        .clickable(interactionSource = interaction.source, indication = null, onClick = {
            latestOnFocused()
            onClick()
        })
        .semantics {
            contentDescription = if (premiumMarker) "${game.title}, $premiumDescription" else game.title
            this.selected = selected
        }
        .testTag("space-game-${game.id}").padding(12.dp)) {
        Box((if (portraitArtwork || fillWidth) Modifier.fillMaxWidth().aspectRatio(if (portraitArtwork) 628f / 888f else 1f)
            else Modifier.size(artworkSize))
            .testTag("space-game-art-${game.id}").then(interaction.modifier)
            .spaceSelectionMark(selected)
            .spaceFocusFrame(focused || interaction.hovered, 14.dp, hovered = interaction.hovered)
            .border(if (selected) 2.dp else 1.dp, if (selected) SpaceIce.copy(alpha = .8f) else SpaceWhite.copy(alpha = .16f), shape)) {
            Box(Modifier.fillMaxSize().clip(shape).background(SpaceInk)) {
                UrlImage(catalogCardImageUrl(game, tvProfile = !portraitArtwork, requestWidth = if (portraitArtwork) 512 else 272), Modifier.fillMaxSize())
                Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent, SpaceInk.copy(alpha = .92f)))))
                Box(Modifier.fillMaxSize().background(Brush.linearGradient(listOf(SpaceWhite.copy(alpha = .12f * interaction.emphasis), Color.Transparent))))
                Text(game.title, color = SpaceWhite, fontSize = if (portraitArtwork) 14.sp else if (compact) 10.sp else 12.sp,
                    lineHeight = if (portraitArtwork) 19.sp else 15.sp, fontWeight = FontWeight.Medium,
                    maxLines = if (compact && artworkSize < 80.dp && !portraitArtwork) 1 else 2, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.align(Alignment.BottomStart).padding(if (compact && artworkSize < 80.dp && !portraitArtwork) 8.dp else 10.dp))
                if (premiumMarker) PremiumMembershipMarker(Modifier.align(Alignment.TopEnd).padding(6.dp))
            }
        }
    }
}

@Composable
internal fun SpaceControl(label: String, modifier: Modifier = Modifier, active: Boolean = false,
    filled: Boolean = false, rectangular: Boolean = false, enabled: Boolean = true, showFocusOutline: Boolean = true,
    minHeight: Dp = if (filled) 52.dp else 48.dp, horizontalPadding: Dp = if (filled) 24.dp else 16.dp,
    onClick: () -> Unit, content: (@Composable RowScope.() -> Unit)? = null) {
    var focused by remember { mutableStateOf(false) }
    val interaction = rememberSpaceInteraction(focused, enabled = enabled, cornerRadius = if (rectangular) 4.dp else minHeight / 2,
        showFocusOutline = showFocusOutline)
    val shape = RoundedCornerShape(if (rectangular) 4.dp else 50.dp)
    val focusManager = LocalFocusManager.current
    Row(modifier.defaultMinSize(minHeight = minHeight, minWidth = if (filled) 132.dp else 48.dp)
        .then(interaction.modifier)
        .onFocusChanged { focused = it.isFocused }
        .hoverable(interaction.source, enabled = enabled)
        .graphicsLayer { alpha = if (enabled) 1f else .38f }
        .clip(shape).background(when { filled -> SpaceWhite; else -> SpaceWhite.copy(alpha = (if (active) .12f else 0f) + .13f * interaction.emphasis) })
        .border(1.dp, SpaceIce.copy(alpha = if (showFocusOutline) interaction.emphasis else 0f), shape)
        .onPreviewKeyEvent { event ->
            handleDpadFocusMove(event, focusManager)
        }
        .focusProperties { canFocus = enabled }
        .clickable(interactionSource = interaction.source, indication = null, enabled = enabled, onClick = onClick)
        .semantics { contentDescription = label; selected = active }
        .padding(horizontal = horizontalPadding, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
        androidx.compose.runtime.CompositionLocalProvider(androidx.compose.material3.LocalContentColor provides if (filled) SpaceInk else SpaceWhite) {
            if (content != null) content()
            else Text(label, color = if (filled) SpaceInk else SpaceWhite, fontSize = 13.sp, fontWeight = if (active) FontWeight.SemiBold else FontWeight.Normal)
        }
    }
}
