package com.opencloudgaming.opennow

import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.net.ConnectivityManager
import android.net.Uri
import android.os.SystemClock
import android.net.NetworkCapabilities
import androidx.annotation.StringRes
import android.view.KeyEvent
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.layout.LazyLayoutCacheWindow
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.Cast
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.MutableIntState
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.SingletonImageLoader
import coil3.request.ImageRequest
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.tint
import kotlin.math.abs
import kotlin.math.roundToInt

@OptIn(ExperimentalComposeUiApi::class, ExperimentalFoundationApi::class)
@Composable
internal fun HomeScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    hideChromeWhenScrolled: Boolean,
    controlsInTopBar: Boolean,
    topBarFocusRequester: FocusRequester?,
    searchRequested: Boolean,
    onSearchDismissed: () -> Unit,
    onScrollChromeHiddenChange: (Boolean) -> Unit,
) {
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val catalogGames = state.games.ifEmpty { state.catalogResult.games }
    val visibleGames = remember(catalogGames, state.catalogFilterIds) {
        filterCatalogGamesForLocalControls(catalogGames, state.catalogFilterIds)
    }
    val filterActive = state.catalogFilterIds.isNotEmpty()
    val searchingCatalog = state.loadingGames && state.catalogSearch.isNotBlank()
    // A 120 Hz panel has only 8.3 ms per frame. Keep its speculative composition window leaner
    // than 60/90 Hz instead of spending that tighter frame budget on a full extra viewport.
    val refreshRateHz = LocalView.current.display?.refreshRate ?: 60f
    val cacheFractions = remember(refreshRateHz) {
        catalogCacheWindowFractions(refreshRateHz)
    }
    val gridState = rememberLazyGridState(
        cacheWindow = LazyLayoutCacheWindow(
            aheadFraction = cacheFractions.first,
            behindFraction = cacheFractions.second,
        ),
    )
    val searchFocusRequester = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val haptics = LocalOpenNowHaptics.current
    val selectGameWithHaptic: (GameInfo) -> Unit = { game ->
        viewModel.selectGame(game)
        haptics?.play(HapticCue.Activate)
    }
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    val showSearch = searchRequested || state.catalogSearch.isNotBlank()
    val resultsOnly = showSearch || filterActive
    val physicalControllerConnected = rememberPhysicalControllerConnected(
        enabled = hideChromeWhenScrolled && !tvProfile,
    )
    val showScrollActions by remember(gridState) {
        derivedStateOf {
            gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 80
        }
    }
    val scrolledAwayFromTop by remember(gridState) {
        derivedStateOf {
            gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0
        }
    }
    val hideScrollChrome = shouldHideStoreChromeOnScroll(
        hideChromeWhenScrolled = hideChromeWhenScrolled,
        scrolledAwayFromTop = scrolledAwayFromTop,
        physicalControllerConnected = physicalControllerConnected,
    )
    LaunchedEffect(hideScrollChrome) {
        onScrollChromeHiddenChange(hideScrollChrome)
    }
    DisposableEffect(Unit) {
        onDispose { onScrollChromeHiddenChange(false) }
    }
    LaunchedEffect(searchRequested) {
        if (searchRequested) {
            delay(90)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        }
    }
    SwipeToRefreshContainer(
        refreshing = shouldShowCatalogRefreshIndicator(
            loadingGames = state.loadingGames,
            hasVisibleGames = visibleGames.isNotEmpty(),
        ),
        enabled = !tvProfile,
        showRefreshIndicator = !searchingCatalog,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            // Same one-margin rule as the Library: the grid owns its inset, so nothing above it
            // adds a second one. Store and Library must agree here or switching tabs shifts the
            // whole page sideways.
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(
                        top = storeScreenTopPadding(
                            controlsInTopBar = controlsInTopBar,
                            phoneLandscapeHero = landscapeLayout && !tvProfile && !resultsOnly,
                        ),
                        bottom = 12.dp,
                    ),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                AnimatedVisibility(visible = showSearch) {
                    NativeSearchField(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = OpenNowSpacing.ScreenEdge),
                        query = state.catalogSearch,
                        onQueryChange = { next ->
                            viewModel.setCatalogSearch(next)
                            if (next.isBlank()) onSearchDismissed()
                        },
                        placeholder = stringResource(R.string.search_games),
                        searching = searchingCatalog,
                        focusRequester = searchFocusRequester,
                        onOpen = {
                            if (gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0) {
                                scope.launch { gridState.animateScrollToItem(0) }
                            }
                        },
                    )
                }
                Box(
                    Modifier
                        .weight(1f)
                        .pointerInput(Unit) {
                            awaitEachGesture {
                                awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                                focusManager.clearFocus()
                                keyboardController?.hide()
                            }
                        },
                ) {
                    if (
                        shouldShowCatalogLoadingPlaceholder(
                            queryLoading = state.catalogQueryLoading,
                            loadingGames = state.loadingGames,
                            hasVisibleGames = visibleGames.isNotEmpty(),
                        )
                    ) {
                        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Box(Modifier.padding(horizontal = OpenNowSpacing.ScreenEdge)) {
                                StoreScrollableControls(
                                    state = state,
                                    onSortChange = viewModel::setCatalogSort,
                                    onFilterToggle = viewModel::toggleCatalogFilter,
                                    showToolbar = !controlsInTopBar,
                                )
                            }
                            if (resultsOnly) {
                                SectionHeader(
                                    title = stringResource(R.string.store_results),
                                    modifier = Modifier.padding(
                                        start = OpenNowSpacing.ScreenEdge,
                                        top = OpenNowSpacing.lg,
                                        end = OpenNowSpacing.ScreenEdge,
                                        bottom = OpenNowSpacing.sm,
                                    ),
                                )
                            }
                            RefreshingGamesPlaceholder(
                                settings = state.settings,
                                tvProfile = tvProfile,
                                storeLayout = !resultsOnly,
                                storeRailCount = if (resultsOnly) {
                                    0
                                } else {
                                    storeStartRailGroups(
                                        games = visibleGames,
                                        libraryGames = state.libraryGames,
                                        favoriteIds = state.settings.favoriteGameIds,
                                        queuedGameKeys = state.queuedGameKeys,
                                    ).visibleGroupCount.coerceAtLeast(1)
                                },
                                modifier = Modifier.weight(1f),
                            )
                        }
                    } else {
                        StoreGameGrid(
                            games = visibleGames,
                            favoriteIds = state.settings.favoriteGameIds,
                            settings = state.settings,
                            tvProfile = tvProfile,
                            state = state,
                            onSelect = selectGameWithHaptic,
                            onFavorite = viewModel::updateFavorites,
                            onPlay = viewModel::play,
                            onChooseStore = viewModel::chooseStore,
                            onSortChange = viewModel::setCatalogSort,
                            onFilterToggle = viewModel::toggleCatalogFilter,
                            onHideLandscapeNewGames = {
                                viewModel.updateSettings(
                                    state.settings.copy(landscapeNewGamesHero = false),
                                )
                            },
                            onClearSearch = {
                                viewModel.setCatalogSearch("")
                                onSearchDismissed()
                            },
                            onClearFilters = viewModel::clearCatalogFilters,
                            gridState = gridState,
                            showToolbar = !controlsInTopBar,
                            topFocusRequester = topBarFocusRequester,
                            resultsOnly = resultsOnly,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                    if (showScrollActions) {
                        Box(Modifier.align(Alignment.BottomEnd).padding(2.dp)) {
                            StoreScrollActionButton(
                                iconRes = R.drawable.ic_arrow_up,
                                contentDescription = stringResource(R.string.action_scroll_top),
                            ) {
                                scope.launch { gridState.animateScrollToItem(0) }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun StoreScrollableControls(
    state: OpenNowUiState,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    showToolbar: Boolean = true,
) {
    val filterOptions = rememberCatalogFilterOptions(
        remember(state.catalogResult.filterGroups) {
            catalogVisibleFilterGroups(state.catalogResult.filterGroups)
        },
    )
    val hasSelectedFilters = state.catalogFilterIds.isNotEmpty()
    val hasError = !state.error.isNullOrBlank()
    if (!showToolbar && !hasSelectedFilters && !hasError) return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (showToolbar) {
            StoreCatalogToolbar(
                state = state,
                onSortChange = onSortChange,
                onFilterToggle = onFilterToggle,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        SelectedFilterChips(options = filterOptions, selectedIds = state.catalogFilterIds, onToggle = onFilterToggle)
        InlineErrorNotice(error = state.error)
    }
}

@Composable
internal fun StoreCatalogToolbar(
    state: OpenNowUiState,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val filterOptions = rememberCatalogFilterOptions(
        remember(state.catalogResult.filterGroups) {
            catalogVisibleFilterGroups(state.catalogResult.filterGroups)
        },
    )
    Row(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SortPicker(
            options = state.catalogResult.sortOptions,
            selected = state.catalogSortId,
            onSelect = onSortChange,
            modifier = Modifier.width(if (compact) 118.dp else 172.dp),
            compact = compact,
        )
        if (filterOptions.isNotEmpty()) {
            FilterMenu(options = filterOptions, selectedIds = state.catalogFilterIds, onToggle = onFilterToggle, compact = compact)
        }
    }
}

@Composable
private fun InlineErrorNotice(error: String?) {
    if (error.isNullOrBlank()) return
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(12.dp),
        color = OpenNowPalette.ErrorContainer,
        tonalElevation = 0.dp,
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            Text(
                compactErrorTitle(error),
                color = OpenNowPalette.OnErrorContainer,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                compactErrorBody(error),
                color = OpenNowPalette.OnErrorContainer,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun compactErrorTitle(error: String): String =
    when {
        error.contains("DNS lookup failed", ignoreCase = true) -> "Network lookup failed"
        error.contains("Unable to resolve host", ignoreCase = true) -> "Network lookup failed"
        else -> "Something went wrong"
    }

private fun compactErrorBody(error: String): String =
    error
        .replace('\n', ' ')
        .replace(SEARCH_WHITESPACE_RUN, " ")
        .let { if (it.length > 180) "${it.take(177)}..." else it }

@Composable
private fun StoreScrollActionButton(iconRes: Int, contentDescription: String, onClick: () -> Unit) {
    Surface(
        shape = CircleShape,
        color = PanelAlt.copy(alpha = 0.96f),
        tonalElevation = 4.dp,
        shadowElevation = 4.dp,
    ) {
        IconButton(onClick = onClick, modifier = Modifier.size(44.dp)) {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = contentDescription,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(22.dp),
            )
        }
    }
}

@Composable
internal fun LibraryScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    hideChromeWhenScrolled: Boolean,
    controlsInTopBar: Boolean,
    topBarFocusRequester: FocusRequester?,
    searchRequested: Boolean,
    onSearchDismissed: () -> Unit,
    onScrollChromeHiddenChange: (Boolean) -> Unit,
) {
    val haptics = LocalOpenNowHaptics.current
    val selectGameWithHaptic: (GameInfo) -> Unit = { game ->
        viewModel.selectGame(game)
        haptics?.play(HapticCue.Activate)
    }
    val orderedGames = remember(state.libraryGames, state.settings.favoriteGameIds, state.librarySortId) {
        sortLibraryGames(
            favoriteOrderedGames(state.libraryGames, state.settings.favoriteGameIds),
            state.librarySortId,
        )
    }
    val touchFilterLabel = stringResource(R.string.catalog_filter_touch_controls)
    val filterOptions = remember(orderedGames, touchFilterLabel) {
        libraryStoreFilterOptions(orderedGames, touchFilterLabel)
    }
    val games = remember(orderedGames, state.librarySearch, state.libraryFilterIds) {
        val searchTerms = searchTermsFor(state.librarySearch)
        orderedGames.filter { game ->
            gameMatchesSearch(game, searchTerms) && gameMatchesLibraryFilters(game, state.libraryFilterIds)
        }
    }
    val gridState = rememberLazyGridState()
    val searchFocusRequester = remember { FocusRequester() }
    val localAppsFocusRequester = remember { FocusRequester() }
    val localAppsHeaderFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val showSearch = searchRequested || state.librarySearch.isNotBlank()
    val localAppsShelfVisible = BuildConfig.LOCAL_APP_LAUNCHER_SUPPORTED && state.settings.localAppsEnabled
    val localAppsCollapsed = state.settings.localAppsCollapsed
    val scrolledAwayFromTop by remember(gridState) {
        derivedStateOf {
            gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0
        }
    }
    val hideScrollChrome = hideChromeWhenScrolled && scrolledAwayFromTop
    LaunchedEffect(hideScrollChrome) {
        onScrollChromeHiddenChange(hideScrollChrome)
    }
    DisposableEffect(Unit) {
        onDispose { onScrollChromeHiddenChange(false) }
    }
    LaunchedEffect(searchRequested) {
        if (searchRequested) {
            delay(90)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        }
    }
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        enabled = !tvProfile,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            // Horizontal insets live on the children, not on this Column: the grid pads its own
            // content so cards can scroll under the edge, and matching that from the outside is
            // what left the local-apps row and the catalogue grid on two different margins.
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(
                        top = if (controlsInTopBar) 4.dp else 12.dp,
                        bottom = 12.dp,
                    ),
                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            ) {
                AnimatedVisibility(visible = showSearch) {
                    NativeSearchField(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = OpenNowSpacing.ScreenEdge),
                        query = state.librarySearch,
                        onQueryChange = { next ->
                            viewModel.setLibrarySearch(next)
                            if (next.isBlank()) onSearchDismissed()
                        },
                        placeholder = "Search library",
                        focusRequester = searchFocusRequester,
                    )
                }
                if (localAppsShelfVisible) {
                    LocalAppsShelf(
                        packageNames = state.settings.localAppPackageNames,
                        collapsed = localAppsCollapsed,
                        onCollapsedChange = viewModel::setLocalAppsCollapsed,
                        onAddPackage = viewModel::addLocalApp,
                        onRemovePackage = viewModel::removeLocalApp,
                        horizontalPadding = OpenNowSpacing.ScreenEdge,
                        headerFocusRequester = localAppsHeaderFocusRequester,
                        focusRequester = localAppsFocusRequester,
                        topFocusRequester = topBarFocusRequester,
                    )
                }
                LibraryFilterControls(
                    gameCount = games.size,
                    totalCount = state.libraryGames.size,
                    options = filterOptions,
                    selectedIds = state.libraryFilterIds,
                    onToggle = viewModel::toggleLibraryFilter,
                    showToolbar = !controlsInTopBar,
                    modifier = Modifier.padding(horizontal = OpenNowSpacing.ScreenEdge),
                )
                if (state.loadingGames && state.libraryGames.isEmpty()) {
                    RefreshingGamesPlaceholder(
                        settings = state.settings,
                        tvProfile = tvProfile,
                        topContentPadding = OpenNowSpacing.xs,
                        modifier = Modifier.weight(1f),
                    )
                } else {
                    GameGrid(
                        games,
                        state.settings.favoriteGameIds,
                        state.settings,
                        tvProfile,
                        selectGameWithHaptic,
                        viewModel::updateFavorites,
                        viewModel::play,
                        viewModel::chooseStore,
                        topFocusRequester = libraryGridUpFocusRequester(
                            shelfVisible = localAppsShelfVisible,
                            shelfCollapsed = localAppsCollapsed,
                            shelfTile = localAppsFocusRequester,
                            shelfHeader = localAppsHeaderFocusRequester,
                            topBar = topBarFocusRequester,
                        ),
                        modifier = Modifier.weight(1f),
                        gridState = gridState,
                        // Everything above already ends on the Column's 8dp gap, so the grid adds a
                        // hairline rather than its full 12dp inset: those two stacked were the
                        // paragraph-sized hole between the filter row and the first row of posters.
                        topContentPadding = OpenNowSpacing.xs,
                        emptyContent = {
                            val hasSearch = state.librarySearch.isNotBlank()
                            val hasFilters = state.libraryFilterIds.isNotEmpty()
                            if ((hasSearch || hasFilters) && state.libraryGames.isNotEmpty()) {
                                SearchEmptyState(
                                    title = stringResource(R.string.library_empty_search_title),
                                    message = when {
                                        hasSearch && hasFilters -> stringResource(R.string.library_empty_search_filters_body)
                                        hasSearch -> stringResource(R.string.library_empty_search_body)
                                        else -> stringResource(R.string.library_empty_filters_body)
                                    },
                                    onClearSearch = if (hasSearch) {
                                        {
                                            viewModel.setLibrarySearch("")
                                            onSearchDismissed()
                                        }
                                    } else {
                                        null
                                    },
                                    onClearFilters = if (hasFilters) {
                                        { viewModel.clearLibraryFilters() }
                                    } else {
                                        null
                                    },
                                )
                            } else {
                                Text(stringResource(R.string.no_games_loaded), color = TextMuted)
                            }
                        },
                    )
                }
            }
        }
    }
}

private fun PaddingValues.withTop(top: Dp?): PaddingValues =
    if (top == null) {
        this
    } else {
        PaddingValues(
            start = calculateStartPadding(LayoutDirection.Ltr),
            top = top,
            end = calculateEndPadding(LayoutDirection.Ltr),
            bottom = calculateBottomPadding(),
        )
    }

/**
 * Where "up" from the Library's first grid row lands.
 *
 * Folding the shelf hides its tiles, and a [FocusRequester] pointed at an element that is no longer
 * composed throws when it is requested — so the fold has to move the target to the header, which is
 * the one part of the shelf that is always on screen.
 */
internal fun <T> libraryGridUpFocusTarget(
    shelfVisible: Boolean,
    shelfCollapsed: Boolean,
    shelfTile: T,
    shelfHeader: T,
    topBar: T?,
): T? = when {
    !shelfVisible -> topBar
    shelfCollapsed -> shelfHeader
    else -> shelfTile
}

private fun libraryGridUpFocusRequester(
    shelfVisible: Boolean,
    shelfCollapsed: Boolean,
    shelfTile: FocusRequester,
    shelfHeader: FocusRequester,
    topBar: FocusRequester?,
): FocusRequester? = libraryGridUpFocusTarget(shelfVisible, shelfCollapsed, shelfTile, shelfHeader, topBar)

internal const val LIBRARY_SORT_DEFAULT = "library"
internal const val LIBRARY_SORT_RECENT = "recent"
internal const val LIBRARY_SORT_TITLE = "title"

@Composable
internal fun librarySortOptions(): List<CatalogSortOption> = listOf(
    CatalogSortOption(LIBRARY_SORT_DEFAULT, stringResource(R.string.library_sort_default), ""),
    CatalogSortOption(LIBRARY_SORT_RECENT, stringResource(R.string.library_sort_recent), ""),
    CatalogSortOption(LIBRARY_SORT_TITLE, stringResource(R.string.library_sort_title), ""),
)

internal fun sortLibraryGames(games: List<GameInfo>, sortId: String): List<GameInfo> =
    when (sortId) {
        LIBRARY_SORT_TITLE -> games.sortedBy { it.title.lowercase(Locale.US) }
        LIBRARY_SORT_RECENT -> games.sortedWith(
            compareByDescending<GameInfo> { it.recentPlaySortKey() != null }
                .thenByDescending { it.recentPlaySortKey() }
                .thenBy { it.title.lowercase(Locale.US) },
        )
        else -> games
    }

@Composable
internal fun LibraryFilterControls(
    gameCount: Int,
    totalCount: Int,
    options: List<CatalogFilterOption>,
    selectedIds: List<String>,
    onToggle: (String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
    showToolbar: Boolean = true,
    showSelectedChips: Boolean = true,
) {
    if (!showToolbar && (!showSelectedChips || selectedIds.isEmpty())) return
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (showToolbar) {
            Row(
                if (compact) Modifier else Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val countModifier = if (compact) Modifier else Modifier.weight(1f)
                Text(
                    text = if (gameCount == totalCount) {
                        stringResource(R.string.library_count, totalCount)
                    } else {
                        "$gameCount / ${stringResource(R.string.library_count, totalCount)}"
                    },
                    color = TextMuted,
                    style = if (compact) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Start,
                    modifier = countModifier,
                )
                if (options.isNotEmpty()) {
                    FilterMenu(options = options, selectedIds = selectedIds, onToggle = onToggle, compact = compact)
                }
            }
        }
        if (showSelectedChips) {
            SelectedFilterChips(options = options, selectedIds = selectedIds, onToggle = onToggle)
        }
    }
}

internal fun libraryStoreFilterOptions(
    games: List<GameInfo>,
    touchFilterLabel: String = "Touch controls",
): List<CatalogFilterOption> {
    val labelsById = linkedMapOf<String, String>()
    games.forEach { game ->
        libraryStoreFilterIds(game).forEach { (id, label) ->
            // Map.putIfAbsent is API 24; this module ships to 23 without core library desugaring.
            if (id !in labelsById) labelsById[id] = label
        }
    }
    val storeOptions = labelsById.entries
        .sortedBy { it.value.lowercase(Locale.US) }
        .map { (id, label) ->
            CatalogFilterOption(
                id = id,
                rawId = id.removePrefix(LIBRARY_STORE_FILTER_PREFIX),
                label = label,
                groupId = "library_store",
                groupLabel = "Launcher",
            )
        }
    val touchOption = if (games.any(::catalogClaimsTouchSupport)) {
        listOf(
            CatalogFilterOption(
                id = CATALOG_FILTER_TOUCHSCREEN,
                rawId = SUPPORTED_CONTROL_TOUCHSCREEN,
                label = touchFilterLabel,
                groupId = "supported_controls",
                groupLabel = "Controls",
            ),
        )
    } else {
        emptyList()
    }
    return touchOption + storeOptions
}

internal fun gameMatchesLibraryFilters(game: GameInfo, selectedIds: List<String>): Boolean {
    if (selectedIds.isEmpty()) return true
    if (CATALOG_FILTER_TOUCHSCREEN in selectedIds && !catalogClaimsTouchSupport(game)) return false
    val selectedStoreIds = selectedIds.filter { it.startsWith(LIBRARY_STORE_FILTER_PREFIX) }
    if (selectedStoreIds.isEmpty()) return true
    val gameFilterIds = libraryStoreFilterIds(game).map { it.first }.toSet()
    return selectedStoreIds.any { it in gameFilterIds }
}

internal fun filterCatalogGamesForLocalControls(games: List<GameInfo>, selectedIds: List<String>): List<GameInfo> =
    if (CATALOG_FILTER_TOUCHSCREEN in selectedIds) games.filter(::catalogClaimsTouchSupport) else games

private fun libraryStoreFilterIds(game: GameInfo): List<Pair<String, String>> {
    val labels = libraryStoreDisplayNames(game)
    return labels
        .mapNotNull { label ->
            val normalized = normalizeGameStore(label)
            if (normalized.isBlank()) return@mapNotNull null
            LIBRARY_STORE_FILTER_PREFIX + normalized to label
        }
        .distinctBy { it.first }
}

private const val LIBRARY_STORE_FILTER_PREFIX = "library_store:"

@Composable
private fun ActiveSessionResumeCard(
    state: OpenNowUiState,
    onResumeActiveSession: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val active = state.activeSession ?: return
    val game = activeSessionGame(state, active)
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(18.dp),
        color = PanelAlt.copy(alpha = 0.92f),
        tonalElevation = 3.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            UrlImage(
                game?.imageUrl,
                Modifier
                    .width(44.dp)
                    .height(58.dp)
                    .clip(RoundedCornerShape(10.dp)),
            )
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(stringResource(R.string.catalog_resume_cloud_session), color = TextPrimary, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    game?.title ?: stringResource(R.string.catalog_app_id, active.appId),
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    activeSessionSummary(active),
                    color = TextMuted,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Button(onClick = onResumeActiveSession, contentPadding = PaddingValues(horizontal = 14.dp, vertical = 8.dp)) {
                Text(stringResource(R.string.action_resume), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

internal fun activeSessionGame(state: OpenNowUiState, active: ActiveSessionInfo): GameInfo? =
    (state.games + state.libraryGames).firstOrNull { game ->
        game.launchAppId == active.appId.toString() ||
            game.variants.any { variant -> variant.id == active.appId.toString() }
    }

@Composable
internal fun activeSessionSummary(active: ActiveSessionInfo): String =
    listOfNotNull(
        when (active.status) {
            1 -> active.queuePosition?.takeIf { it > 0 }?.let { stringResource(R.string.queue_short_position, it) }
                ?: stringResource(R.string.common_starting)
            2, 3 -> stringResource(R.string.common_ready)
            else -> stringResource(R.string.common_active)
        },
        active.resolution,
        active.fps?.let { "${it} FPS" },
        active.gpuType,
        active.sessionId.take(8).takeIf { it.isNotBlank() }?.let { "Session $it" },
    ).joinToString(" - ")

@Composable
private fun SearchEmptyState(
    title: String,
    message: String,
    onClearSearch: (() -> Unit)? = null,
    onClearFilters: (() -> Unit)? = null,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            title,
            color = TextPrimary,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center,
        )
        Text(
            message,
            color = TextMuted,
            style = MaterialTheme.typography.bodyMedium,
            textAlign = TextAlign.Center,
        )
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            onClearSearch?.let { clearSearch ->
                OutlinedButton(onClick = clearSearch) {
                    Text(stringResource(R.string.search_clear), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            onClearFilters?.let { clearFilters ->
                OutlinedButton(onClick = clearFilters) {
                    Text(stringResource(R.string.action_clear_filters), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

@Composable
private fun RefreshingGamesPlaceholder(
    settings: AppSettings,
    tvProfile: Boolean,
    storeLayout: Boolean = false,
    storeRailCount: Int = 0,
    topContentPadding: Dp? = null,
    modifier: Modifier = Modifier,
) {
    GameGridSkeleton(
        settings = settings,
        tvProfile = tvProfile,
        storeLayout = storeLayout,
        storeRailCount = storeRailCount,
        topContentPadding = topContentPadding,
        modifier = modifier,
    )
}

internal val LocalShimmerOffset = staticCompositionLocalOf<State<Float>?> { null }
internal val LocalTvLoadingPulse = staticCompositionLocalOf<State<Float>?> { null }
internal val LocalTvLoadingProfile = staticCompositionLocalOf { false }
internal val LocalImageLoadingAnimationsEnabled = staticCompositionLocalOf { true }
internal val LocalCatalogImageRequestsPaused = staticCompositionLocalOf { false }
internal val LocalImageLoadingTracker = staticCompositionLocalOf<((Int) -> Unit)?> { null }
internal val LocalTouchControllerStyle = staticCompositionLocalOf { TouchControllerStyle.V1 }
internal val LocalSelectedCatalogGameId = staticCompositionLocalOf<String?> { null }
internal const val SHIMMER_CYCLE_DURATION_MS = 760

/** Keep decoded artwork mounted, but do not start new fetch/decode work in the middle of a fling. */
internal fun shouldStartCatalogImageRequest(requestsPaused: Boolean, imageAlreadyLoaded: Boolean): Boolean =
    !requestsPaused || imageAlreadyLoaded

/** Keep the lightweight shared placeholder clock independent from image request throttling. */
internal fun shouldAnimateCatalogLoading(loadingImageCount: Int, reduceMotion: Boolean): Boolean =
    loadingImageCount > 0 && !reduceMotion

/**
 * One loading animation drives every visible poster. Fetch/decode work can pause during a fling
 * without freezing the lightweight shared shimmer that tells the user those posters are loading.
 */
@Composable
private fun CatalogImageLoadingAnimationProvider(
    tvProfile: Boolean,
    imageRequestsPaused: Boolean,
    content: @Composable () -> Unit,
) {
    var loadingImageCount by remember { mutableIntStateOf(0) }
    val updateLoadingImageCount: (Int) -> Unit = remember {
        { delta -> loadingImageCount = (loadingImageCount + delta).coerceAtLeast(0) }
    }
    // The previous shared transition lived for as long as the grid was composed, even after every
    // image had loaded. On a 120 Hz display that kept the entire app scheduling frames while idle.
    // Start the one shared clock only while at least one visible image actually shows a shimmer.
    val animate = shouldAnimateCatalogLoading(loadingImageCount, LocalReduceMotion.current)
    val driver: State<Float>? = if (animate) {
        val transition = rememberInfiniteTransition(label = "catalog-image-loading")
        transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(
                    durationMillis = if (tvProfile) 900 else SHIMMER_CYCLE_DURATION_MS,
                    easing = LinearEasing,
                ),
                repeatMode = if (tvProfile) RepeatMode.Reverse else RepeatMode.Restart,
            ),
            label = "catalog-image-loading-driver",
        )
    } else {
        null
    }
    CompositionLocalProvider(
        LocalImageLoadingAnimationsEnabled provides animate,
        LocalCatalogImageRequestsPaused provides imageRequestsPaused,
        LocalImageLoadingTracker provides updateLoadingImageCount,
        LocalShimmerOffset provides driver.takeUnless { tvProfile },
        LocalTvLoadingPulse provides driver.takeIf { tvProfile },
        content = content,
    )
}

@Composable
private fun GameGridSkeleton(
    settings: AppSettings,
    tvProfile: Boolean,
    storeLayout: Boolean,
    storeRailCount: Int,
    topContentPadding: Dp?,
    modifier: Modifier = Modifier,
) {
    val compact = settings.compactGameCards
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = tvProfile || landscapeLayout)
    val controllerActionMode = catalogControllerActionMode(tvProfile, landscapeLayout, physicalControllerConnected)
    val artworkOnly = shouldUseArtworkOnlyCatalogCards(
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
    )

    val shimmerOffset: State<Float>?
    val tvPulse: State<Float>?
    // Under reduced motion the skeletons still show — they just stop animating. A never-ending
    // sweep is exactly the kind of movement the setting exists to stop.
    if (LocalReduceMotion.current) {
        shimmerOffset = null
        tvPulse = null
    } else if (tvProfile) {
        val transition = rememberInfiniteTransition(label = "loading-pulse-global")
        val pulse = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 900, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "loading-pulse-global",
        )
        shimmerOffset = null
        tvPulse = pulse
    } else {
        val transition = rememberInfiniteTransition(label = "shimmer-global")
        val shimmer = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = SHIMMER_CYCLE_DURATION_MS, easing = LinearEasing),
            ),
            label = "shimmer-offset-global",
        )
        shimmerOffset = shimmer
        tvPulse = null
    }

    CompositionLocalProvider(
        LocalShimmerOffset provides shimmerOffset,
        LocalTvLoadingPulse provides tvPulse,
    ) {
        BoxWithConstraints(modifier.fillMaxSize()) {
            val gridSpec = gameGridSpec(maxWidth, compact, landscapeLayout, settings, handheldLayout = !tvProfile)
            val contentPadding = gridSpec.contentPadding.withTop(
                when {
                    storeLayout && landscapeLayout && !tvProfile -> 0.dp
                    else -> topContentPadding
                },
            )
            val placeholderItems = remember(gridSpec.columnCount, storeLayout) {
                List(catalogSkeletonPlaceholderCount(gridSpec.columnCount, storeLayout)) { it }
            }
            LazyVerticalGrid(
                modifier = Modifier.fillMaxSize(),
                columns = gridSpec.cells,
                contentPadding = contentPadding,
                horizontalArrangement = Arrangement.spacedBy(gridSpec.horizontalSpacing),
                verticalArrangement = Arrangement.spacedBy(gridSpec.verticalSpacing),
                userScrollEnabled = false,
            ) {
                if (storeLayout) {
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        StoreStartRailsSkeleton(
                            settings = settings,
                            tvProfile = tvProfile,
                            railCount = storeRailCount,
                        )
                    }
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        SkeletonSectionHeader(
                            modifier = Modifier.padding(
                                top = OpenNowSpacing.lg,
                                bottom = OpenNowSpacing.sm,
                            ),
                        )
                    }
                }
                gridItems(placeholderItems, key = { it }) {
                    GameCardSkeleton(
                        expressiveUi = settings.expressiveUi,
                        tvProfile = tvProfile,
                        squareCard = gridSpec.squareCards,
                        thumbnailFavoriteOverlay = shouldShowCatalogFavoriteIcon(settings),
                        showCardTitles = !artworkOnly && shouldShowCatalogCardTitles(
                            tvProfile = tvProfile,
                            enabled = settings.showCardTitles,
                        ),
                    )
                }
            }
        }
    }
}

@Composable
private fun StoreStartRailsSkeleton(
    settings: AppSettings,
    tvProfile: Boolean,
    railCount: Int,
) {
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val showFeaturedHero = shouldShowStoreHero(
        tvProfile = tvProfile,
        landscape = landscapeLayout,
        landscapeEnabled = settings.landscapeNewGamesHero,
    )
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = if (landscapeLayout) 0.dp else 2.dp, bottom = 6.dp),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
    ) {
        if (showFeaturedHero) {
            StoreHeroSkeleton(
                settings = settings,
                tvProfile = tvProfile,
                landscapeLayout = landscapeLayout,
            )
        }
        repeat(railCount.coerceAtLeast(1)) {
            StoreRailSectionSkeleton(
                expressiveUi = settings.expressiveUi,
                tvProfile = tvProfile,
                landscapeLayout = landscapeLayout,
                cardScale = settings.posterSizeScale,
                showFavoriteIcon = shouldShowCatalogFavoriteIcon(settings),
            )
        }
    }
}

/** Mirrors the configured Coming next hero so the first loaded frame does not reshape the Store. */
@Composable
private fun StoreHeroSkeleton(
    settings: AppSettings,
    tvProfile: Boolean,
    landscapeLayout: Boolean,
) {
    val shape = RoundedCornerShape(if (settings.expressiveUi) 24.dp else 16.dp)
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = if (landscapeLayout && !tvProfile) 0.dp else 6.dp),
        verticalArrangement = Arrangement.spacedBy(
            if (landscapeLayout && !tvProfile) OpenNowSpacing.sm else OpenNowSpacing.md,
        ),
    ) {
        SkeletonSectionHeader(
            showSubtitle = true,
            reserveTrailingAction = landscapeLayout && !tvProfile,
        )
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(heroAspectRatio(tvProfile, landscapeLayout))
                .border(
                    1.dp,
                    storeHeroBorderColor(LocalGameCardBordersEnabled.current),
                    shape,
                ),
            shape = shape,
            color = Panel,
            tonalElevation = 0.dp,
            shadowElevation = 1.dp,
        ) {
            Box(Modifier.fillMaxSize().clip(shape)) {
                LoadingShimmer(Modifier.fillMaxSize())
                Column(
                    Modifier
                        .align(Alignment.BottomStart)
                        .fillMaxWidth(0.56f)
                        .padding(18.dp),
                    verticalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    SkeletonLine(widthFraction = 1f, height = 18.dp)
                    SkeletonLine(widthFraction = 0.52f, height = 10.dp)
                }
                if (shouldShowCatalogFavoriteIcon(settings)) {
                    SkeletonCircle(
                        size = 38.dp,
                        modifier = Modifier
                            .align(Alignment.TopEnd)
                            .padding(14.dp),
                    )
                }
                LoadingShimmer(
                    Modifier
                        .align(Alignment.BottomEnd)
                        .padding(14.dp)
                        .width(58.dp)
                        .height(19.dp)
                        .clip(RoundedCornerShape(999.dp)),
                )
            }
        }
    }
}

@Composable
private fun SkeletonSectionHeader(
    modifier: Modifier = Modifier,
    showSubtitle: Boolean = false,
    reserveTrailingAction: Boolean = false,
) {
    Row(
        modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CatalogSectionHeaderText(
            title = null,
            subtitle = null,
            showSubtitle = showSubtitle,
            modifier = Modifier.weight(1f),
        )
        if (reserveTrailingAction) {
            Spacer(Modifier.size(48.dp))
        }
    }
}

@Composable
private fun StoreRailSectionSkeleton(
    expressiveUi: Boolean,
    tvProfile: Boolean,
    landscapeLayout: Boolean,
    cardScale: Float,
    showFavoriteIcon: Boolean,
) {
    val spacing = OpenNowSpacing.md
    val contentInset = OpenNowSpacing.ScreenEdge
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SkeletonSectionHeader()
        BoxWithConstraints(
            Modifier
                .horizontalBleed(contentInset)
                .clipToBounds(),
        ) {
            val cardWidth = storeRailCardWidth(tvProfile, landscapeLayout, cardScale)
            val visibleCount = storeRailVisibleCardCount(
                availableWidthDp = (maxWidth - contentInset * 2).coerceAtLeast(1.dp).value,
                cardWidthDp = cardWidth.value,
                spacingDp = spacing.value,
            )
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = contentInset),
                horizontalArrangement = Arrangement.spacedBy(spacing),
            ) {
                repeat(visibleCount) {
                    StoreRailGameCardSkeleton(
                        width = cardWidth,
                        expressiveUi = expressiveUi,
                        tvProfile = tvProfile,
                        portraitCard = !tvProfile,
                        showFavoriteIcon = showFavoriteIcon,
                    )
                }
            }
        }
    }
}

@Composable
private fun StoreRailGameCardSkeleton(
    width: Dp,
    expressiveUi: Boolean,
    tvProfile: Boolean,
    portraitCard: Boolean,
    showFavoriteIcon: Boolean,
) {
    val shape = RoundedCornerShape(if (expressiveUi) 12.dp else 8.dp)
    Surface(
        modifier = Modifier
            .width(width)
            .padding(vertical = if (tvProfile) CATALOG_CONTROLLER_FOCUS_INSET else 0.dp)
            .aspectRatio(if (portraitCard) GAME_BOX_ART_ASPECT_RATIO else 1f)
            .border(
                1.dp,
                catalogCardBorderColor(
                    LocalActiveSelectionColor.current,
                    LocalGameCardBordersEnabled.current,
                ),
                shape,
            ),
        shape = shape,
        color = Color.Black,
        tonalElevation = 0.dp,
        shadowElevation = 1.dp,
    ) {
        Box(Modifier.fillMaxSize().clip(shape)) {
            LoadingShimmer(Modifier.fillMaxSize())
            if (showFavoriteIcon) {
                SkeletonCircle(
                    size = 34.dp,
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(6.dp),
                )
            }
        }
    }
}

/** Mirrors [GameCard]'s layout exactly, so nothing shifts when real content replaces it. */
@Composable
private fun GameCardSkeleton(
    expressiveUi: Boolean,
    tvProfile: Boolean,
    squareCard: Boolean,
    thumbnailFavoriteOverlay: Boolean,
    showCardTitles: Boolean,
) {
    val cardShape = RoundedCornerShape(if (expressiveUi) OpenNowRadius.md else OpenNowRadius.sm)
    Column(
        Modifier
            .fillMaxWidth()
            .padding(vertical = if (tvProfile) CATALOG_CONTROLLER_FOCUS_INSET else 0.dp),
    ) {
        Box(Modifier.catalogCardArtworkSize(squareCard)) {
            Card(
                modifier = Modifier.matchParentSize(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.58f)),
                shape = cardShape,
            ) {
                Box(Modifier.fillMaxSize()) {
                    LoadingShimmer(Modifier.fillMaxSize())
                    if (thumbnailFavoriteOverlay) {
                        SkeletonCircle(
                            size = 34.dp,
                            modifier = Modifier
                                .align(Alignment.TopStart)
                                .padding(6.dp),
                        )
                    }
                }
            }
        }
        if (showCardTitles) {
            CatalogCardCaption(title = null)
        }
    }
}

/** One artwork measurement path for loaded recommendation cards and their loading skeletons. */
private fun Modifier.catalogCardArtworkSize(squareCard: Boolean): Modifier =
    fillMaxWidth().aspectRatio(if (squareCard) 1f else GAME_BOX_ART_ASPECT_RATIO)

private const val CATALOG_CARD_TITLE_LINES = 2

/**
 * Uses the real title text measurement even while loading, so font scale and line height cannot
 * make a loaded recommendation card taller than the skeleton it replaces.
 */
@Composable
private fun CatalogCardCaption(title: String?) {
    Box(
        Modifier
            .fillMaxWidth()
            .padding(top = OpenNowSpacing.sm),
    ) {
        Text(
            text = title ?: " ",
            color = if (title == null) Color.Transparent else TextPrimary,
            style = MaterialTheme.typography.titleMedium,
            maxLines = CATALOG_CARD_TITLE_LINES,
            minLines = CATALOG_CARD_TITLE_LINES,
            overflow = TextOverflow.Ellipsis,
        )
        if (title == null) {
            Column(
                Modifier
                    .matchParentSize()
                    .padding(vertical = 5.dp),
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                SkeletonLine(widthFraction = 0.86f)
                SkeletonLine(widthFraction = 0.52f)
            }
        }
    }
}

@Composable
private fun SkeletonLine(
    widthFraction: Float,
    height: Dp = 9.dp,
    modifier: Modifier = Modifier,
) {
    LoadingShimmer(
        modifier
            .fillMaxWidth(widthFraction)
            .height(height)
            .clip(RoundedCornerShape(999.dp)),
    )
}

@Composable
private fun SkeletonCircle(size: Dp, modifier: Modifier = Modifier) {
    LoadingShimmer(
        modifier
            .size(size)
            .clip(CircleShape),
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SwipeToRefreshContainer(
    refreshing: Boolean,
    onRefresh: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    showRefreshIndicator: Boolean = true,
    content: @Composable () -> Unit,
) {
    if (!enabled) {
        Box(modifier) {
            content()
        }
        return
    }
    val pullRefreshState = rememberPullToRefreshState()
    PullToRefreshBox(
        isRefreshing = refreshing,
        onRefresh = onRefresh,
        modifier = modifier,
        state = pullRefreshState,
        indicator = {
            if (showRefreshIndicator) {
                PullToRefreshDefaults.Indicator(
                    state = pullRefreshState,
                    isRefreshing = refreshing,
                    modifier = Modifier.align(Alignment.TopCenter),
                )
            }
        },
    ) {
        content()
    }
}

@Composable
private fun GameGrid(
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    topFocusRequester: FocusRequester? = null,
    modifier: Modifier = Modifier,
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState = rememberLazyGridState(),
    /** Overrides the spec's top inset when the caller has already spaced the grid off what's above it. */
    topContentPadding: Dp? = null,
    emptyContent: (@Composable () -> Unit)? = null,
) {
    if (games.isEmpty()) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (emptyContent != null) {
                emptyContent()
            } else {
                Text(stringResource(R.string.no_games_loaded), color = TextMuted)
            }
        }
        return
    }
    val compact = settings.compactGameCards
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = tvProfile || landscapeLayout)
    val controllerActionMode = catalogControllerActionMode(tvProfile, landscapeLayout, physicalControllerConnected)
    val artworkOnly = shouldUseArtworkOnlyCatalogCards(tvProfile, controllerActionMode)
    val imageRequestsPaused by remember(gridState) {
        derivedStateOf { gridState.isScrollInProgress }
    }
    val favoriteIdSet = remember(favoriteIds) { favoriteIds.toHashSet() }
    val density = LocalDensity.current
    BoxWithConstraints(modifier.fillMaxSize()) {
        val gridSpec = gameGridSpec(maxWidth, compact, landscapeLayout, settings, handheldLayout = !tvProfile)
        val cardRequestWidth = catalogGridCardImageRequestWidth(
            availableWidth = maxWidth,
            gridSpec = gridSpec,
            density = density,
            tvProfile = tvProfile,
        )
        val contentPadding = gridSpec.contentPadding.withTop(topContentPadding)
        val firstRowGameIds = remember(games, gridSpec.columnCount) {
            games.take(gridSpec.columnCount).mapTo(mutableSetOf()) { it.id }
        }
        CatalogImageLoadingAnimationProvider(tvProfile, imageRequestsPaused) {
            CatalogFocusScope(enabled = tvProfile) {
                LazyVerticalGrid(
                    modifier = Modifier.fillMaxSize(),
                    state = gridState,
                    columns = gridSpec.cells,
                    contentPadding = contentPadding,
                    horizontalArrangement = Arrangement.spacedBy(gridSpec.horizontalSpacing),
                    verticalArrangement = Arrangement.spacedBy(gridSpec.verticalSpacing),
                ) {
                    gridItems(games, key = { it.id }, contentType = { "catalog-game" }) { game ->
                        GameCard(
                            game = game,
                            favorite = game.id in favoriteIdSet,
                            tvProfile = tvProfile,
                            expressiveUi = settings.expressiveUi,
                            liveSelectedOutlines = LocalActiveSelectionEnabled.current,
                            showCardTitles = !artworkOnly && shouldShowCatalogCardTitles(
                                tvProfile = tvProfile,
                                enabled = settings.showCardTitles,
                            ),
                            squareCard = gridSpec.squareCards,
                            imageRequestWidth = cardRequestWidth,
                            thumbnailFavoriteOverlay = shouldShowCatalogFavoriteIcon(settings),
                            controllerActionMode = controllerActionMode,
                            upFocusRequester = topFocusRequester.takeIf { game.id in firstRowGameIds },
                            onSelect = onSelect,
                            onFavorite = onFavorite,
                            onPlay = onPlay,
                            onChooseStore = onChooseStore,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun StoreGameGrid(
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    state: OpenNowUiState,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    onHideLandscapeNewGames: () -> Unit,
    onClearSearch: () -> Unit,
    onClearFilters: () -> Unit,
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState,
    showToolbar: Boolean = true,
    topFocusRequester: FocusRequester? = null,
    resultsOnly: Boolean = false,
    modifier: Modifier = Modifier,
) {
    if (games.isEmpty()) {
        Column(modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            StoreScrollableControls(state, onSortChange, onFilterToggle, showToolbar = showToolbar)
            if (resultsOnly) {
                SectionHeader(
                    title = stringResource(R.string.store_results),
                    modifier = Modifier.padding(top = OpenNowSpacing.lg, bottom = OpenNowSpacing.sm),
                )
            }
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                val hasSearch = state.catalogSearch.isNotBlank()
                val hasFilters = state.catalogFilterIds.isNotEmpty()
                if (hasSearch || hasFilters) {
                    SearchEmptyState(
                        title = stringResource(R.string.store_empty_search_title),
                        message = when {
                            hasSearch && hasFilters -> stringResource(R.string.store_empty_search_filters_body)
                            hasSearch -> stringResource(R.string.store_empty_search_body)
                            else -> stringResource(R.string.store_empty_filters_body)
                        },
                        onClearSearch = if (hasSearch) onClearSearch else null,
                        onClearFilters = if (hasFilters) onClearFilters else null,
                    )
                } else {
                    Text(stringResource(R.string.no_games_loaded), color = TextMuted)
                }
            }
        }
        return
    }
    val compact = settings.compactGameCards
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = tvProfile || landscapeLayout)
    val controllerActionMode = catalogControllerActionMode(tvProfile, landscapeLayout, physicalControllerConnected)
    val artworkOnly = shouldUseArtworkOnlyCatalogCards(tvProfile, controllerActionMode)
    val showControlsHeader = showToolbar || state.catalogFilterIds.isNotEmpty() || !state.error.isNullOrBlank()
    val showDiscoverySections = shouldShowStoreDiscoverySections(
        searchActive = state.catalogSearch.isNotBlank(),
        filterActive = state.catalogFilterIds.isNotEmpty(),
    )
    val imageRequestsPaused by remember(gridState) {
        derivedStateOf { gridState.isScrollInProgress }
    }
    val favoriteIdSet = remember(favoriteIds) { favoriteIds.toHashSet() }
    val density = LocalDensity.current
    BoxWithConstraints(modifier.fillMaxSize()) {
        val gridSpec = gameGridSpec(maxWidth, compact, landscapeLayout, settings, handheldLayout = !tvProfile)
        val cardRequestWidth = catalogGridCardImageRequestWidth(
            availableWidth = maxWidth,
            gridSpec = gridSpec,
            density = density,
            tvProfile = tvProfile,
        )
        val contentPadding = gridSpec.contentPadding.withTop(
            if (showDiscoverySections && landscapeLayout && !tvProfile) 0.dp else null,
        )
        val firstRowGameIds = remember(games, gridSpec.columnCount) {
            games.take(gridSpec.columnCount).mapTo(mutableSetOf()) { it.id }
        }
        CatalogImageLoadingAnimationProvider(tvProfile, imageRequestsPaused) {
            CatalogFocusScope(enabled = tvProfile) {
                LazyVerticalGrid(
                    modifier = Modifier.fillMaxSize(),
                    state = gridState,
                    columns = gridSpec.cells,
                    contentPadding = contentPadding,
                    horizontalArrangement = Arrangement.spacedBy(gridSpec.horizontalSpacing),
                    verticalArrangement = Arrangement.spacedBy(gridSpec.verticalSpacing),
                ) {
                    if (showControlsHeader) {
                        item(span = { GridItemSpan(maxLineSpan) }) {
                            StoreScrollableControls(state, onSortChange, onFilterToggle, showToolbar = showToolbar)
                        }
                    }
                    if (showDiscoverySections) {
                        item(span = { GridItemSpan(maxLineSpan) }) {
                            StoreStartRails(
                                games = games,
                                newlyAddedGames = state.newlyAddedGames,
                                libraryGames = state.libraryGames,
                                favoriteIds = favoriteIds,
                                queuedGameKeys = state.queuedGameKeys,
                                settings = settings,
                                tvProfile = tvProfile,
                                controllerActionMode = controllerActionMode,
                                topFocusRequester = topFocusRequester,
                                onSelect = onSelect,
                                onFavorite = onFavorite,
                                onPlay = onPlay,
                                onChooseStore = onChooseStore,
                                onHideLandscapeNewGames = onHideLandscapeNewGames,
                            )
                        }
                    }
                    if (games.isNotEmpty()) {
                        item(span = { GridItemSpan(maxLineSpan) }) {
                            SectionHeader(
                                title = stringResource(
                                    if (showDiscoverySections) R.string.store_recommendations else R.string.store_results,
                                ),
                                modifier = Modifier.padding(top = OpenNowSpacing.lg, bottom = OpenNowSpacing.sm),
                            )
                        }
                    }
                    gridItems(games, key = { it.id }, contentType = { "store-game" }) { game ->
                        GameCard(
                            game = game,
                            favorite = game.id in favoriteIdSet,
                            tvProfile = tvProfile,
                            expressiveUi = settings.expressiveUi,
                            liveSelectedOutlines = LocalActiveSelectionEnabled.current,
                            showCardTitles = !artworkOnly && shouldShowCatalogCardTitles(
                                tvProfile = tvProfile,
                                enabled = settings.showCardTitles,
                            ),
                            squareCard = gridSpec.squareCards,
                            imageRequestWidth = cardRequestWidth,
                            thumbnailFavoriteOverlay = shouldShowCatalogFavoriteIcon(settings),
                            controllerActionMode = controllerActionMode,
                            upFocusRequester = topFocusRequester.takeIf {
                                !showDiscoverySections && game.id in firstRowGameIds
                            },
                            onSelect = onSelect,
                            onFavorite = onFavorite,
                            onPlay = onPlay,
                            onChooseStore = onChooseStore,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun StoreStartRails(
    games: List<GameInfo>,
    newlyAddedGames: List<GameInfo>,
    libraryGames: List<GameInfo>,
    favoriteIds: List<String>,
    queuedGameKeys: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    topFocusRequester: FocusRequester?,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onHideLandscapeNewGames: () -> Unit,
) {
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val showFeaturedHero = shouldShowStoreHero(
        tvProfile = tvProfile,
        landscape = landscape,
        landscapeEnabled = settings.landscapeNewGamesHero,
    )
    val startRails = remember(games, libraryGames, favoriteIds, queuedGameKeys) {
        storeStartRailGroups(games, libraryGames, favoriteIds, queuedGameKeys)
    }
    val featured = remember(newlyAddedGames, startRails, showFeaturedHero) {
        if (showFeaturedHero) {
            newlyAddedStoreHeroGames(
                games = newlyAddedGames,
                excludedGames = startRails.allGames,
            )
        } else {
            emptyList()
        }
    }
    var confirmHideLandscapeNewGames by remember { mutableStateOf(false) }
    if (startRails.isEmpty && featured.isEmpty()) return
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = if (landscape) 0.dp else 2.dp, bottom = 6.dp),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
    ) {
        // The exact same hero leads on handheld and TV. Its aspect ratio adapts to the surface,
        // but its feed, interaction model, and visual treatment remain shared.
        if (featured.isNotEmpty()) {
            StoreComingNextCarousel(
                title = stringResource(R.string.catalog_sort_new_games),
                games = featured,
                favoriteIds = favoriteIds,
                settings = settings,
                tvProfile = tvProfile,
                controllerActionMode = controllerActionMode,
                upFocusRequester = topFocusRequester,
                onSelect = onSelect,
                onFavorite = onFavorite,
                onPlay = onPlay,
                onChooseStore = onChooseStore,
                trailing = if (landscape && !tvProfile) {
                    {
                        IconButton(onClick = { confirmHideLandscapeNewGames = true }) {
                            Icon(
                                imageVector = Icons.Outlined.Close,
                                contentDescription = stringResource(R.string.store_landscape_new_games_hide),
                                tint = Color.White,
                            )
                        }
                    }
                } else {
                    null
                },
            )
        }
        StoreStartRail(
            R.string.store_continue_playing,
            startRails.continuePlaying,
            favoriteIds,
            settings,
            tvProfile,
            controllerActionMode,
            topFocusRequester,
            onSelect,
            onFavorite,
            onPlay,
            onChooseStore,
        )
        StoreStartRail(
            R.string.store_in_queue,
            startRails.inQueue,
            favoriteIds,
            settings,
            tvProfile,
            controllerActionMode,
            topFocusRequester.takeIf { featured.isEmpty() && startRails.continuePlaying.isEmpty() },
            onSelect,
            onFavorite,
            onPlay,
            onChooseStore,
        )
        StoreStartRail(
            R.string.store_favorites,
            startRails.favorites,
            favoriteIds,
            settings,
            tvProfile,
            controllerActionMode,
            topFocusRequester.takeIf {
                featured.isEmpty() && startRails.continuePlaying.isEmpty() && startRails.inQueue.isEmpty()
            },
            onSelect,
            onFavorite,
            onPlay,
            onChooseStore,
        )
    }
    if (confirmHideLandscapeNewGames) {
        AlertDialog(
            onDismissRequest = { confirmHideLandscapeNewGames = false },
            title = { Text(stringResource(R.string.store_landscape_new_games_hide_title)) },
            text = { Text(stringResource(R.string.store_landscape_new_games_hide_body)) },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirmHideLandscapeNewGames = false
                        onHideLandscapeNewGames()
                    },
                ) {
                    Text(stringResource(R.string.common_dont_show_again))
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmHideLandscapeNewGames = false }) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }
}

internal fun shouldShowStoreDiscoverySections(searchActive: Boolean, filterActive: Boolean): Boolean =
    !searchActive && !filterActive

internal fun shouldShowStoreHero(
    tvProfile: Boolean,
    landscape: Boolean,
    landscapeEnabled: Boolean = true,
): Boolean = tvProfile || !landscape || landscapeEnabled

internal fun shouldShowCatalogLoadingPlaceholder(
    queryLoading: Boolean,
    loadingGames: Boolean,
    hasVisibleGames: Boolean,
): Boolean = queryLoading || (loadingGames && !hasVisibleGames)

/** Background cache/network refreshes stay silent once the Store already has usable cards. */
internal fun shouldShowCatalogRefreshIndicator(loadingGames: Boolean, hasVisibleGames: Boolean): Boolean =
    loadingGames && !hasVisibleGames

internal fun shouldHideStoreChromeOnScroll(
    hideChromeWhenScrolled: Boolean,
    scrolledAwayFromTop: Boolean,
    physicalControllerConnected: Boolean,
): Boolean = hideChromeWhenScrolled && scrolledAwayFromTop && !physicalControllerConnected

internal fun storeScreenTopPadding(controlsInTopBar: Boolean, phoneLandscapeHero: Boolean): Dp = when {
    phoneLandscapeHero && controlsInTopBar -> 0.dp
    controlsInTopBar -> 4.dp
    else -> 12.dp
}

/** Small wrapper so Store start rails share one section implementation. */
@Composable
private fun StoreStartRail(
    @StringRes titleRes: Int,
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    upFocusRequester: FocusRequester?,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    if (games.isEmpty()) return
    StoreRailSection(
        title = stringResource(titleRes),
        games = games,
        favoriteIds = favoriteIds,
        settings = settings,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
        upFocusRequester = upFocusRequester,
        onSelect = onSelect,
        onFavorite = onFavorite,
        onPlay = onPlay,
        onChooseStore = onChooseStore,
    )
}

/**
 * The one heading treatment used by every catalog section — rails, the hero, and the
 * recommendations grid — so a section title looks the same wherever it appears. Previously each
 * of those sites styled its own `Text` and they had drifted apart.
 */
@Composable
private fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CatalogSectionHeaderText(
            title = title,
            subtitle = subtitle,
            showSubtitle = !subtitle.isNullOrBlank(),
            modifier = Modifier.weight(1f),
        )
        trailing?.invoke()
    }
}

/** Real and loading section headers share the same text metrics and therefore the same height. */
@Composable
private fun CatalogSectionHeaderText(
    title: String?,
    subtitle: String?,
    showSubtitle: Boolean,
    modifier: Modifier = Modifier,
) {
    Column(modifier) {
        Box(Modifier.fillMaxWidth()) {
            Text(
                text = title ?: " ",
                color = if (title == null) Color.Transparent else TextPrimary,
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (title == null) {
                SkeletonLine(
                    widthFraction = 0.34f,
                    height = 15.dp,
                    modifier = Modifier.align(Alignment.CenterStart),
                )
            }
        }
        if (showSubtitle) {
            Box(Modifier.fillMaxWidth()) {
                Text(
                    text = subtitle ?: " ",
                    color = if (subtitle == null) Color.Transparent else TextMuted,
                    style = MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (subtitle == null) {
                    SkeletonLine(
                        widthFraction = 0.48f,
                        height = 9.dp,
                        modifier = Modifier.align(Alignment.CenterStart),
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun StoreComingNextCarousel(
    title: String,
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    upFocusRequester: FocusRequester?,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    trailing: (@Composable () -> Unit)? = null,
) {
    if (games.isEmpty()) return
    val context = LocalContext.current
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    var page by remember(games) { mutableIntStateOf(0) }
    var focused by remember { mutableStateOf(false) }
    val enhancedControllerFocus = shouldShowEnhancedControllerFocus(
        focused = focused,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
    )
    val selectedGameId = LocalSelectedCatalogGameId.current
    val reduceMotion = LocalReduceMotion.current
    val storeScrolling = LocalCatalogImageRequestsPaused.current
    val carouselProgress = remember { Animatable(0f) }
    var carouselDragPx by remember { mutableFloatStateOf(0f) }
    val swipeThresholdPx = with(LocalDensity.current) { 48.dp.toPx() }
    val carouselDragState = rememberDraggableState { delta -> carouselDragPx += delta }
    LaunchedEffect(games, page, focused, reduceMotion, storeScrolling) {
        // Never auto-advance under the reader's hands: not while focused, and not at all when the
        // user has asked for reduced motion. Vertical Store motion also gets the full frame budget.
        carouselProgress.snapTo(0f)
        if (shouldAnimateStoreHero(games.size, focused, reduceMotion, storeScrolling)) {
            carouselProgress.animateTo(
                targetValue = 1f,
                animationSpec = tween(
                    durationMillis = HERO_CAROUSEL_ADVANCE_MS.toInt(),
                    easing = LinearEasing,
                ),
            )
            page = (page + 1) % games.size
        } else if (games.size <= 1 || reduceMotion) {
            carouselProgress.snapTo(1f)
        }
    }
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = if (landscape && !tvProfile) 0.dp else 6.dp),
        verticalArrangement = Arrangement.spacedBy(
            if (landscape && !tvProfile) OpenNowSpacing.sm else OpenNowSpacing.md,
        ),
    ) {
        SectionHeader(
            title = title,
            subtitle = stringResource(R.string.store_coming_next_subtitle),
            trailing = trailing,
        )
        AnimatedContent(
            targetState = page,
            transitionSpec = {
                fadeIn(tween(if (reduceMotion) 0 else OpenNowMotion.DurationStandard)) togetherWith
                    fadeOut(tween(if (reduceMotion) 0 else OpenNowMotion.DurationFast))
            },
            label = "coming-next-carousel",
        ) { targetPage ->
            val featured = games[targetPage.coerceIn(games.indices)]
            val selected = featured.id == selectedGameId
            val selectedOutline = shouldShowActiveSelectionOutline(selected, LocalActiveSelectionEnabled.current)
            val shape = RoundedCornerShape(if (settings.expressiveUi) 24.dp else 16.dp)
            val transitionRegistry = LocalGameDetailsTransitionRegistry.current
            val transitionBounds = remember(featured.id) { arrayOfNulls<Rect>(1) }
            val selectFromHero = {
                transitionBounds[0]?.let {
                    transitionRegistry?.record(featured.id, it, GameDetailsTransitionKind.Hero)
                }
                onSelect(featured)
            }
            Box(
                Modifier
                    .fillMaxWidth()
                    // Aspect ratio rather than a fixed height, so the hero scales with the screen
                    // instead of dominating a small phone and looking stunted on a tablet.
                    .aspectRatio(heroAspectRatio(tvProfile, landscape))
                    .draggable(
                        state = carouselDragState,
                        orientation = Orientation.Horizontal,
                        enabled = games.size > 1,
                        onDragStarted = { carouselDragPx = 0f },
                        onDragStopped = {
                            if (abs(carouselDragPx) >= swipeThresholdPx) {
                                page = if (carouselDragPx < 0f) {
                                    (page + 1) % games.size
                                } else {
                                    (page - 1 + games.size) % games.size
                                }
                            }
                            carouselDragPx = 0f
                        },
                    ),
            ) {
                Surface(
                    modifier = Modifier
                        .matchParentSize()
                        .onGloballyPositioned { transitionBounds[0] = it.boundsInWindow() }
                        .then(
                            upFocusRequester?.let { requester ->
                                Modifier.focusProperties { up = requester }
                            } ?: Modifier,
                        )
                        .onFocusChanged { focused = it.isFocused || it.hasFocus }
                        .focusMoveHaptics()
                        .border(
                            width = catalogCardBorderWidthDp(
                                controllerFocused = enhancedControllerFocus,
                                borderEffectsEnabled = LocalAbsoluteCinemaEffects.current,
                            ).dp,
                            color = storeHeroBorderColor(
                                gameBorderEnabled = LocalGameCardBordersEnabled.current,
                                controllerFocused = enhancedControllerFocus,
                                borderEffectsEnabled = LocalAbsoluteCinemaEffects.current,
                            ),
                            shape = shape,
                        )
                        .onPreviewKeyEvent { event ->
                            if (event.type != KeyEventType.KeyUp) return@onPreviewKeyEvent false
                            when {
                                controllerActionMode && event.key == Key.DirectionLeft && games.size > 1 -> {
                                    page = (page - 1 + games.size) % games.size
                                    true
                                }
                                controllerActionMode && event.key == Key.DirectionRight && games.size > 1 -> {
                                    page = (page + 1) % games.size
                                    true
                                }
                                controllerActionMode && handleCatalogControllerAction(
                                    event = event,
                                    onFavorite = { onFavorite(featured.id) },
                                    onPlay = { onPlay(featured) },
                                ) -> true
                                isTvActivateKey(event) -> {
                                    selectFromHero()
                                    true
                                }
                                else -> false
                            }
                        }
                        .focusable()
                        .combinedClickable(
                            onClick = selectFromHero,
                            onLongClick = { onChooseStore(featured) },
                            onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
                        ),
                    shape = shape,
                    color = Panel,
                    tonalElevation = if (focused) 5.dp else 0.dp,
                    shadowElevation = if (focused) 9.dp else 1.dp,
                ) {
                    Box(Modifier.fillMaxSize()) {
                        UrlImage(gameHeroImageUrl(context, featured), Modifier.fillMaxSize())
                        // Horizontal scrim carries the title block; the vertical one settles the art
                        // into the surface below so the hero reads as part of the page, not a sticker.
                        Box(
                            Modifier
                                .matchParentSize()
                                .background(
                                    Brush.horizontalGradient(
                                        listOf(Color.Black.copy(alpha = 0.88f), Color.Black.copy(alpha = 0.3f), Color.Transparent),
                                    ),
                                ),
                        )
                        Box(
                            Modifier
                                .matchParentSize()
                                .background(
                                    Brush.verticalGradient(
                                        0.45f to Color.Transparent,
                                        1f to Background.copy(alpha = 0.85f),
                                    ),
                                ),
                        )
                        Column(
                            Modifier
                                .align(Alignment.BottomStart)
                                .fillMaxWidth(0.74f)
                                .padding(18.dp),
                            verticalArrangement = Arrangement.spacedBy(5.dp),
                        ) {
                            Text(
                                featured.title,
                                color = Color.White,
                                style = when {
                                    // Across a room the hero title is the only thing readable at a
                                    // glance, so TV gets the display scale.
                                    tvProfile -> MaterialTheme.typography.displaySmall
                                    landscape -> MaterialTheme.typography.headlineSmall
                                    else -> MaterialTheme.typography.headlineSmall
                                },
                                maxLines = 2,
                                overflow = TextOverflow.Ellipsis,
                            )
                            storeHeroSubtitle(featured)?.let { subtitle ->
                                Text(
                                    subtitle,
                                    color = Color.White.copy(alpha = 0.72f),
                                    style = MaterialTheme.typography.labelMedium,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                        if (shouldShowCatalogFavoriteIcon(settings)) {
                            FavoriteIconButton(
                                favorite = featured.id in favoriteIds,
                                onClick = { onFavorite(featured.id) },
                                modifier = Modifier.align(Alignment.TopEnd).padding(14.dp),
                                size = 38.dp,
                            )
                        }
                        Surface(
                            modifier = Modifier
                                .align(Alignment.BottomEnd)
                                .padding(14.dp),
                            shape = RoundedCornerShape(999.dp),
                            color = Color.Black.copy(alpha = 0.58f),
                            contentColor = Color.White,
                            tonalElevation = 0.dp,
                        ) {
                            HeroCarouselProgress(
                                pageCount = games.size,
                                activePage = page,
                                activeProgress = { carouselProgress.value },
                                modifier = Modifier.padding(horizontal = 9.dp, vertical = 7.dp),
                            )
                        }
                    }
                }
                ControllerFocusFrame(
                    visible = enhancedControllerFocus || selectedOutline || (focused && LocalAbsoluteCinemaEffects.current),
                    cornerRadius = if (settings.expressiveUi) 24.dp else 16.dp,
                    tint = if (selectedOutline || LocalAbsoluteCinemaEffects.current) LocalActiveSelectionColor.current else Color.White,
                    secondaryTint = if (selectedOutline || LocalAbsoluteCinemaEffects.current) LocalActiveSelectionSecondaryColor.current else Color.White,
                )
            }
        }
    }
}

internal fun shouldAnimateStoreHero(
    pageCount: Int,
    focused: Boolean,
    reduceMotion: Boolean,
    storeScrolling: Boolean,
): Boolean = pageCount > 1 && !focused && !reduceMotion && !storeScrolling

@Composable
private fun HeroCarouselProgress(
    pageCount: Int,
    activePage: Int,
    activeProgress: () -> Float,
    modifier: Modifier = Modifier,
) {
    val activeColor = MaterialTheme.colorScheme.primary
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        repeat(pageCount) { index ->
            if (index == activePage) {
                Box(
                    Modifier
                        .width(22.dp)
                        .height(5.dp)
                        .clip(CircleShape)
                        .background(activeColor.copy(alpha = 0.28f)),
                ) {
                    Box(
                        Modifier
                            .matchParentSize()
                            .graphicsLayer {
                                scaleX = activeProgress().coerceIn(0f, 1f)
                                transformOrigin = TransformOrigin(0f, 0.5f)
                            }
                            .background(activeColor),
                    )
                }
            } else {
                Box(
                    Modifier
                        .width(6.dp)
                        .height(5.dp)
                        .clip(CircleShape)
                        .background(Color.White.copy(alpha = 0.38f)),
                )
            }
        }
    }
}

@Composable
private fun StoreRailSection(
    title: String,
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    upFocusRequester: FocusRequester?,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val railState = rememberLazyListState()
    val railScrolling by remember(railState) {
        derivedStateOf { railState.isScrollInProgress }
    }
    val parentImageRequestsPaused = LocalCatalogImageRequestsPaused.current
    val favoriteIdSet = remember(favoriteIds) { favoriteIds.toHashSet() }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
        SectionHeader(title = title)
        // The row breaks out of the grid's edge padding and re-applies it as content padding, so
        // cards scroll all the way under the screen edge instead of stopping short of it. The
        // header stays aligned to the content because the bleed is only on the row.
        BoxWithConstraints(Modifier.horizontalBleed(OpenNowSpacing.ScreenEdge)) {
            val spacing = OpenNowSpacing.md
            val contentInset = OpenNowSpacing.ScreenEdge
            // Use the persisted scale as an actual width multiplier. The old implementation used
            // it only to choose a whole-number card count, then stretched cards to fill the row;
            // most slider movements therefore appeared to do nothing. Matching the handheld rail
            // base to the adaptive grid also keeps Continue playing from towering over the grid.
            val cardWidth = storeRailCardWidth(
                tvProfile = tvProfile,
                landscapeLayout = landscapeLayout,
                cardScale = settings.posterSizeScale,
            )
            CompositionLocalProvider(
                LocalCatalogImageRequestsPaused provides (parentImageRequestsPaused || railScrolling),
            ) {
                CatalogFocusScope(enabled = tvProfile) {
                    LazyRow(
                        state = railState,
                        horizontalArrangement = Arrangement.spacedBy(spacing),
                        contentPadding = PaddingValues(horizontal = contentInset),
                    ) {
                        items(games, key = { storeRailGameKey(it) }) { game ->
                            StoreRailGameCard(
                                game = game,
                                favorite = game.id in favoriteIdSet,
                                tvProfile = tvProfile,
                                expressiveUi = settings.expressiveUi,
                                liveSelectedOutlines = LocalActiveSelectionEnabled.current,
                                showFavoriteIcon = shouldShowCatalogFavoriteIcon(settings),
                                width = cardWidth,
                                controllerActionMode = controllerActionMode,
                                upFocusRequester = upFocusRequester,
                                onSelect = onSelect,
                                onFavorite = onFavorite,
                                onPlay = onPlay,
                                onChooseStore = onChooseStore,
                            )
                        }
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun StoreRailGameCard(
    game: GameInfo,
    favorite: Boolean,
    tvProfile: Boolean,
    expressiveUi: Boolean,
    liveSelectedOutlines: Boolean,
    showFavoriteIcon: Boolean,
    width: Dp,
    controllerActionMode: Boolean,
    upFocusRequester: FocusRequester?,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val shape = RoundedCornerShape(if (expressiveUi) OpenNowRadius.md else OpenNowRadius.sm)
    val actionButtonSize = 34.dp
    val enhancedControllerFocus = shouldShowEnhancedControllerFocus(
        focused = focused,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
    )
    val selected = LocalSelectedCatalogGameId.current == game.id
    val selectedOutline = shouldShowActiveSelectionOutline(selected, liveSelectedOutlines)
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val observeHover = tvProfile || controllerActionMode || LocalAbsoluteCinemaEffects.current
    val hovered = if (observeHover) interaction.collectIsHoveredAsState().value else false
    val reduceMotion = LocalReduceMotion.current
    val cardScale by animateFloatAsState(
        targetValue = when {
            pressed -> 0.965f
            focused || hovered -> when {
                tvProfile -> 1.08f
                controllerActionMode -> 1f
                else -> 1.035f
            }
            else -> 1f
        },
        animationSpec = tween(
            durationMillis = if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
            easing = OpenNowMotion.EasingStandard,
        ),
        label = "rail-card-scale",
    )
    val dimAlpha = rememberCatalogCardAlpha(focused = focused, tvProfile = tvProfile)
    val transitionRegistry = LocalGameDetailsTransitionRegistry.current
    val transitionBounds = remember(game.id) { arrayOfNulls<Rect>(1) }
    val selectFromCard = {
        transitionBounds[0]?.let {
            transitionRegistry?.record(game.id, it, GameDetailsTransitionKind.Card)
        }
        onSelect(game)
    }
    Box(
        Modifier
            .width(width)
            .padding(vertical = if (tvProfile) CATALOG_CONTROLLER_FOCUS_INSET else 0.dp)
            .aspectRatio(if (tvProfile) 1f else GAME_BOX_ART_ASPECT_RATIO)
            .catalogCardTransform(scale = cardScale, alpha = dimAlpha)
            .onGloballyPositioned { transitionBounds[0] = it.boundsInWindow() }
            .semantics(mergeDescendants = true) {
                contentDescription = game.title
                role = Role.Button
            },
    ) {
        Surface(
            modifier = Modifier
                .matchParentSize()
                .then(
                    upFocusRequester?.let { requester ->
                        Modifier.focusProperties { up = requester }
                    } ?: Modifier,
                )
                .onFocusChanged { focused = it.isFocused || it.hasFocus }
                .focusMoveHaptics()
                .border(
                    width = catalogCardBorderWidthDp(
                        controllerFocused = enhancedControllerFocus,
                        borderEffectsEnabled = LocalAbsoluteCinemaEffects.current,
                    ).dp,
                    color = catalogCardBorderColor(
                        selectionColor = LocalSelectionTintColor.current,
                        gameBorderEnabled = LocalGameCardBordersEnabled.current,
                        controllerFocused = enhancedControllerFocus,
                        borderEffectsEnabled = LocalAbsoluteCinemaEffects.current,
                    ),
                    shape = shape,
                )
                .onPreviewKeyEvent { event ->
                    when {
                        controllerActionMode && handleCatalogControllerAction(
                            event = event,
                            onFavorite = { onFavorite(game.id) },
                            onPlay = { onPlay(game) },
                        ) -> true
                        isTvActivateKey(event) -> {
                            selectFromCard()
                            true
                        }
                        else -> handleDpadFocusMove(event, focusManager)
                    }
                }
                .focusable(interactionSource = interaction)
                .combinedClickable(
                    interactionSource = interaction,
                    indication = null,
                    onClick = selectFromCard,
                    onLongClick = { onChooseStore(game) },
                    onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
                ),
            shape = shape,
            color = OpenNowPalette.ImagePlaceholder,
            tonalElevation = if (focused) 4.dp else 0.dp,
            shadowElevation = if (focused) 8.dp else 1.dp,
        ) {
            Box(Modifier.fillMaxSize().clip(shape)) {
                UrlImage(
                    catalogCardImageUrl(game, tvProfile),
                    Modifier.fillMaxSize(),
                    // Crop everywhere — see the note in GameCard.
                    contentScale = ContentScale.Crop,
                )
                if (shouldOverlayCatalogCardTitle(tvProfile)) {
                    GameCardTitleOverlay(game.title)
                }
                if (showFavoriteIcon) {
                    FavoriteIconButton(
                        favorite = favorite,
                        onClick = { onFavorite(game.id) },
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .padding(6.dp),
                        size = actionButtonSize,
                    )
                }
            }
        }
        ControllerFocusFrame(
            visible = enhancedControllerFocus || selectedOutline || ((focused || hovered) && LocalAbsoluteCinemaEffects.current),
            cornerRadius = if (expressiveUi) 12.dp else 8.dp,
            tint = if (selectedOutline || LocalAbsoluteCinemaEffects.current) LocalActiveSelectionColor.current else Color.White,
            secondaryTint = if (selectedOutline || LocalAbsoluteCinemaEffects.current) LocalActiveSelectionSecondaryColor.current else Color.White,
        )
    }
}

/**
 * The three rails that open the store, kept distinct.
 *
 * These used to be flattened into one "Jump back in" rail of `queued + favorites + recent + owned`
 * capped at 14 items — which meant genuinely recently-played games sat third in priority and were
 * routinely pushed off-screen by favourites, and the tail was padded with owned games the user had
 * never launched. Owned games are the Library tab's job, so they are dropped here entirely.
 */
internal data class StoreStartRailGroups(
    val continuePlaying: List<GameInfo>,
    val inQueue: List<GameInfo>,
    val favorites: List<GameInfo>,
) {
    val allGames: List<GameInfo> get() = continuePlaying + inQueue + favorites
    val isEmpty: Boolean get() = continuePlaying.isEmpty() && inQueue.isEmpty() && favorites.isEmpty()
    val visibleGroupCount: Int
        get() = listOf(continuePlaying, inQueue, favorites).count { it.isNotEmpty() }
}

internal fun storeStartRailGroups(
    games: List<GameInfo>,
    libraryGames: List<GameInfo>,
    favoriteIds: List<String>,
    queuedGameKeys: List<String>,
): StoreStartRailGroups {
    val favoriteSet = favoriteIds.toSet()
    val combined = distinctStoreGames(libraryGames + games)
    val byKey = combined.associateBy(::storeRailGameKey)

    val continuePlaying = combined
        .filter { it.recentPlaySortKey() != null }
        .sortedByDescending { it.recentPlaySortKey() }
        .take(CONTINUE_PLAYING_RAIL_LIMIT)
    val continueKeys = continuePlaying.map(::storeRailGameKey).toSet()

    val inQueue = queuedGameKeys
        .mapNotNull(byKey::get)
        .filterNot { storeRailGameKey(it) in continueKeys }
        .take(STORE_RAIL_GAME_LIMIT)
    val shownKeys = continueKeys + inQueue.map(::storeRailGameKey)

    // Favourites already visible above would just be a second sighting of the same card.
    val favorites = combined
        .filter { it.id in favoriteSet }
        .filterNot { storeRailGameKey(it) in shownKeys }
        .take(STORE_RAIL_GAME_LIMIT)

    return StoreStartRailGroups(continuePlaying, inQueue, favorites)
}

/** Preserve the provider's New games added order without repeating the personal rails above it. */
internal fun newlyAddedStoreHeroGames(
    games: List<GameInfo>,
    excludedGames: List<GameInfo> = emptyList(),
): List<GameInfo> {
    val distinctGames = distinctStoreGames(games)
    val excludedKeys = excludedGames.mapTo(mutableSetOf(), ::storeRailGameKey)
    val nonRepeatingGames = distinctGames
        .filterNot { storeRailGameKey(it) in excludedKeys }
    return nonRepeatingGames.ifEmpty { distinctGames }.take(HERO_CAROUSEL_PAGE_LIMIT)
}

/** The hero identifies the game without repeating its storefront availability. */
internal fun storeHeroSubtitle(game: GameInfo): String? =
    game.publisherName?.trim()?.takeIf(String::isNotEmpty)

private fun GameInfo.recentPlaySortKey(): String? =
    listOfNotNull(
        lastPlayed?.takeIf { it.isNotBlank() },
        variants.mapNotNull { it.lastPlayedDate?.takeIf(String::isNotBlank) }.maxOrNull(),
    ).maxOrNull()

private fun distinctStoreGames(games: List<GameInfo>): List<GameInfo> {
    val byKey = linkedMapOf<String, GameInfo>()
    games.forEach { game ->
        // Map.putIfAbsent is API 24; this module ships to 23 without core library desugaring.
        val key = storeRailGameKey(game)
        if (key !in byKey) byKey[key] = game
    }
    return byKey.values.toList()
}

private fun storeRailGameKey(game: GameInfo): String =
    gameTrackingKey(game)

private const val STORE_RAIL_GAME_LIMIT = 14

/** Recently-played is a short list by nature — padding it out defeats the point of the rail. */
private const val CONTINUE_PLAYING_RAIL_LIMIT = 12

/** Six hero pages keep the weekly selection varied without turning the progress row into a rash of dots. */
private const val HERO_CAROUSEL_PAGE_LIMIT = 6

private const val HERO_CAROUSEL_ADVANCE_MS = 6_000L

/**
 * Wider on surfaces that are already wide, so the hero stays a banner rather than becoming a wall.
 */
private fun heroAspectRatio(tvProfile: Boolean, landscape: Boolean): Float = when {
    tvProfile -> 16f / 6f
    landscape -> 16f / 5f
    else -> 16f / 8f
}

/** The New games added hero keeps a white structural edge when game borders are enabled. */
internal fun storeHeroBorderColor(
    gameBorderEnabled: Boolean,
    controllerFocused: Boolean = false,
    borderEffectsEnabled: Boolean = false,
): Color =
    catalogCardBorderColor(
        selectionColor = Color.White,
        gameBorderEnabled = gameBorderEnabled,
        controllerFocused = controllerFocused,
        borderEffectsEnabled = borderEffectsEnabled,
    )

internal const val GAME_BOX_ART_ASPECT_RATIO = 628f / 888f

internal fun shouldInitiallyFocusGameDetailsPlay(tvProfile: Boolean): Boolean = tvProfile

private data class GameGridSpec(
    val cells: GridCells,
    /** Shared by loaded cards, skeleton rows, focus routing, and image request sizing. */
    val columnCount: Int,
    val horizontalSpacing: Dp,
    val verticalSpacing: Dp,
    val contentPadding: PaddingValues,
    val squareCards: Boolean,
)

/**
 * Number of catalog cards currently holding focus inside the surrounding grid or rail. A count
 * rather than a flag so that handing focus from one card to its neighbour — where the old card
 * reports losing focus in the same frame the new one reports gaining it — never dips to "nothing
 * is focused" and flickers the dim.
 */
private val LocalCatalogFocusCount = compositionLocalOf<MutableIntState?> { null }

/**
 * Scopes the focus count to one grid or one rail, so focusing a card in the grid doesn't dim the
 * rails above it.
 */
@Composable
private fun CatalogFocusScope(
    enabled: Boolean,
    content: @Composable () -> Unit,
) {
    if (!enabled) {
        content()
        return
    }
    val count = remember { mutableIntStateOf(0) }
    CompositionLocalProvider(LocalCatalogFocusCount provides count, content = content)
}

/** Alpha applied to unfocused cards while a sibling is focused. TV only. */
private const val TV_UNFOCUSED_CARD_ALPHA = 0.55f

/**
 * Registers this card's focus in the surrounding [CatalogFocusScope] and returns the alpha it
 * should draw at. Dimming the neighbours is what makes the focus cursor readable from across a
 * room — on TV a border and a scale change alone still leave a wall of equally bright artwork.
 */
@Composable
private fun rememberCatalogCardAlpha(focused: Boolean, tvProfile: Boolean): Float {
    // Phone cards never participate in sibling dimming. Returning before reading the focus scope
    // avoids installing a DisposableEffect on every poster composed during a fast fling.
    if (!tvProfile) return 1f
    val count = LocalCatalogFocusCount.current
    DisposableEffect(focused, count) {
        if (focused) count?.intValue = (count?.intValue ?: 0) + 1
        onDispose {
            if (focused) count?.intValue = ((count?.intValue ?: 1) - 1).coerceAtLeast(0)
        }
    }
    val anyFocused = (count?.intValue ?: 0) > 0
    val target = if (anyFocused && !focused) TV_UNFOCUSED_CARD_ALPHA else 1f
    val reduceMotion = LocalReduceMotion.current
    val alpha by animateFloatAsState(
        targetValue = target,
        animationSpec = tween(
            durationMillis = if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
            easing = OpenNowMotion.EasingStandard,
        ),
        label = "catalog-card-dim",
    )
    return alpha
}

/** Avoids allocating a graphics layer for every idle card in a long Store grid or rail. */
private fun Modifier.catalogCardTransform(scale: Float, alpha: Float): Modifier =
    if (scale == 1f && alpha == 1f) {
        this
    } else {
        graphicsLayer {
            scaleX = scale
            scaleY = scale
            this.alpha = alpha
        }
    }

/**
 * Lets a child extend [bleed] past its parent's bounds on both sides without reporting the extra
 * width upward — the standard way to make a horizontally scrolling row run edge to edge inside a
 * padded container.
 */
private fun Modifier.horizontalBleed(bleed: Dp): Modifier = this.layout { measurable, constraints ->
    val extra = bleed.roundToPx() * 2
    val placeable = measurable.measure(
        constraints.copy(
            maxWidth = if (constraints.hasBoundedWidth) constraints.maxWidth + extra else constraints.maxWidth,
        ),
    )
    val reportedWidth = (placeable.width - extra).coerceAtLeast(0)
    layout(reportedWidth, placeable.height) {
        placeable.place(-bleed.roundToPx(), 0)
    }
}

private fun storeRailCardWidth(
    tvProfile: Boolean,
    landscapeLayout: Boolean,
    cardScale: Float,
): Dp {
    val baseWidth = when {
        tvProfile -> 158.dp
        landscapeLayout -> GRID_CELL_WIDTH_LANDSCAPE
        else -> GRID_CELL_WIDTH_PORTRAIT
    }
    return scaledCatalogCardWidthDp(baseWidth.value, cardScale).dp
}

internal fun scaledCatalogCardWidthDp(baseCardWidthDp: Float, cardScale: Float): Float =
    baseCardWidthDp * cardScale.coerceIn(MIN_GAME_CARD_SCALE, MAX_GAME_CARD_SCALE)

/** Target widths at `posterSizeScale == 1`; the resolved count still adapts continuously to width. */
private val GRID_CELL_WIDTH_PORTRAIT = 96.dp
private val GRID_CELL_WIDTH_LANDSCAPE = 112.dp
private val GRID_CELL_WIDTH_TV = 158.dp

/** Compact mode shrinks the target cell rather than switching to a separate size table. */
private const val COMPACT_CELL_WIDTH_FACTOR = 0.88f
private val CATALOG_CONTROLLER_FOCUS_INSET = 8.dp

internal data class CatalogGridMetrics(
    val targetCellWidthDp: Float,
    val columnCount: Int,
)

/** Always produces complete placeholder rows for the exact resolved recommendation column count. */
internal fun catalogSkeletonPlaceholderCount(columnCount: Int, storeLayout: Boolean): Int =
    columnCount.coerceAtLeast(1) * if (storeLayout) 4 else 3

/**
 * Resolves the catalogue grid once for both real content and its skeleton.
 *
 * Keeping this as one settings-aware calculation is important: allowing [GridCells.Adaptive] to
 * resolve the loaded grid while a separate estimate sizes the skeleton can leave a partial final
 * row whenever density rounding or a card-size setting makes those decisions differ.
 */
internal fun catalogGridMetrics(
    availableWidthDp: Float,
    compact: Boolean,
    landscapeLayout: Boolean,
    posterSizeScale: Float,
    handheldLayout: Boolean,
): CatalogGridMetrics {
    val horizontalSpacingDp = if (compact) OpenNowSpacing.sm.value else OpenNowSpacing.GridGutter.value
    val horizontalPaddingDp = OpenNowSpacing.ScreenEdge.value
    val baseCellWidthDp = when {
        !handheldLayout -> GRID_CELL_WIDTH_TV.value
        landscapeLayout -> GRID_CELL_WIDTH_LANDSCAPE.value
        else -> GRID_CELL_WIDTH_PORTRAIT.value
    }
    val targetCellWidthDp = (
        baseCellWidthDp *
            posterSizeScale.coerceIn(MIN_GAME_CARD_SCALE, MAX_GAME_CARD_SCALE) *
            if (compact) COMPACT_CELL_WIDTH_FACTOR else 1f
        ).coerceIn(64f, 240f)
    val usableWidthDp = (availableWidthDp - horizontalPaddingDp * 2f)
        .coerceAtLeast(targetCellWidthDp)
    val columnCount = kotlin.math.floor(
        (usableWidthDp + horizontalSpacingDp) / (targetCellWidthDp + horizontalSpacingDp),
    ).toInt().coerceIn(1, 12)

    return CatalogGridMetrics(
        targetCellWidthDp = targetCellWidthDp,
        columnCount = columnCount,
    )
}

private fun gameGridSpec(
    maxWidth: androidx.compose.ui.unit.Dp,
    compact: Boolean,
    landscapeLayout: Boolean,
    settings: AppSettings,
    handheldLayout: Boolean,
): GameGridSpec {
    val horizontalSpacing = if (compact) OpenNowSpacing.sm else OpenNowSpacing.GridGutter
    val verticalSpacing = if (compact) OpenNowSpacing.md else OpenNowSpacing.GridRowGap
    val horizontalPadding = OpenNowSpacing.ScreenEdge

    val metrics = catalogGridMetrics(
        availableWidthDp = maxWidth.value,
        compact = compact,
        landscapeLayout = landscapeLayout,
        posterSizeScale = settings.posterSizeScale,
        handheldLayout = handheldLayout,
    )

    return GameGridSpec(
        // Fixed uses the exact count resolved above but still shares remaining width evenly, which
        // is the same responsive presentation as Adaptive without a second independent decision.
        cells = GridCells.Fixed(metrics.columnCount),
        columnCount = metrics.columnCount,
        horizontalSpacing = horizontalSpacing,
        verticalSpacing = verticalSpacing,
        contentPadding = PaddingValues(
            start = horizontalPadding,
            top = OpenNowSpacing.md,
            end = horizontalPadding,
            bottom = AppScrollEndSpacing,
        ),
        // TV grid cards match the TV rail cards, which have always been square — this is the shape
        // NVIDIA's tvCardImageUrl assets are cut for.
        squareCards = !handheldLayout,
    )
}

private fun catalogGridCardImageRequestWidth(
    availableWidth: Dp,
    gridSpec: GameGridSpec,
    density: androidx.compose.ui.unit.Density,
    tvProfile: Boolean,
): Int {
    val direction = LayoutDirection.Ltr
    val horizontalPadding = gridSpec.contentPadding.calculateStartPadding(direction) +
        gridSpec.contentPadding.calculateEndPadding(direction)
    val gaps = gridSpec.horizontalSpacing * (gridSpec.columnCount - 1).coerceAtLeast(0)
    val cardWidth = ((availableWidth - horizontalPadding - gaps) / gridSpec.columnCount)
        .coerceAtLeast(1.dp)
    val cardWidthPx = with(density) { cardWidth.roundToPx() }
    return catalogCardImageRequestWidth(cardWidthPx, tvProfile)
}

/**
 * Keeps phone-grid downloads close to the actual card width. The old unconditional 512 px request
 * made a 96 dp card on a high-density POCO decode roughly four times the pixels it displayed,
 * creating avoidable uploads and GC pressure during a 120 Hz fling.
 */
internal fun catalogCardImageRequestWidth(cardWidthPx: Int, tvProfile: Boolean): Int = when {
    tvProfile -> TV_CARD_IMAGE_REQUEST_WIDTH
    cardWidthPx <= 240 -> 256
    cardWidthPx <= 340 -> 384
    cardWidthPx <= 460 -> 512
    else -> 640
}

/** Bounded Store precomposition budget that protects the tighter high-refresh frame deadline. */
internal fun catalogCacheWindowFractions(refreshRateHz: Float): Pair<Float, Float> = when {
    refreshRateHz >= 110f -> 0.25f to 0.08f
    refreshRateHz >= 80f -> 0.4f to 0.17f
    else -> 0.33f to 0.17f
}

internal fun appContentEdgePaddingDp(
    settings: AppSettings,
    inStream: Boolean,
    tvProfile: Boolean,
): Float = if (inStream || !tvProfile) 0f else settings.tvSafeAreaPaddingDp.coerceIn(0f, 120f)

internal fun storeRailVisibleCardCount(
    availableWidthDp: Float,
    cardWidthDp: Float,
    spacingDp: Float,
): Int = kotlin.math.floor(
    (availableWidthDp + spacingDp) / (cardWidthDp.coerceAtLeast(1f) + spacingDp),
).toInt().coerceAtLeast(1)

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun GameCard(
    game: GameInfo,
    favorite: Boolean,
    tvProfile: Boolean,
    expressiveUi: Boolean,
    liveSelectedOutlines: Boolean,
    showCardTitles: Boolean,
    squareCard: Boolean,
    imageRequestWidth: Int = MOBILE_CARD_IMAGE_REQUEST_WIDTH,
    thumbnailFavoriteOverlay: Boolean,
    controllerActionMode: Boolean,
    upFocusRequester: FocusRequester? = null,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val cardShape = RoundedCornerShape(if (expressiveUi) OpenNowRadius.md else OpenNowRadius.sm)
    val handheldPosterCard = !tvProfile
    val launcherTile = handheldPosterCard && thumbnailFavoriteOverlay
    val overlayActionSize = if (launcherTile) 34.dp else 44.dp
    val overlayActionPadding = if (launcherTile) 6.dp else 8.dp
    val enhancedControllerFocus = shouldShowEnhancedControllerFocus(
        focused = focused,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
    )
    val selected = LocalSelectedCatalogGameId.current == game.id
    val selectedOutline = shouldShowActiveSelectionOutline(selected, liveSelectedOutlines)
    // Touch-handheld captions live outside the poster, so the artwork stays visually clean.
    val showCaption = handheldPosterCard && showCardTitles

    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    // Hover state is useful for a TV, controller, mouse, or the opt-in cinema treatment. A normal
    // touch phone cannot produce hover, so observing it on every grid item is pure churn.
    val observeHover = tvProfile || controllerActionMode || LocalAbsoluteCinemaEffects.current
    val hovered = if (observeHover) interaction.collectIsHoveredAsState().value else false
    val reduceMotion = LocalReduceMotion.current
    val cardScale by animateFloatAsState(
        targetValue = when {
            pressed -> 0.965f
            // A bigger lift on TV: from three metres a border change is nearly invisible, but a
            // card growing out of the grid is unmistakable.
            focused || hovered -> when {
                tvProfile -> 1.08f
                controllerActionMode -> 1f
                else -> 1.035f
            }
            else -> 1f
        },
        animationSpec = tween(
            durationMillis = if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
            easing = OpenNowMotion.EasingStandard,
        ),
        label = "game-card-scale",
    )
    val dimAlpha = rememberCatalogCardAlpha(focused = focused, tvProfile = tvProfile)
    val transitionRegistry = LocalGameDetailsTransitionRegistry.current
    val transitionBounds = remember(game.id) { arrayOfNulls<Rect>(1) }
    val selectFromCard = {
        transitionBounds[0]?.let {
            transitionRegistry?.record(game.id, it, GameDetailsTransitionKind.Card)
        }
        onSelect(game)
    }

    Column(
        Modifier
            .fillMaxWidth()
            .padding(vertical = if (tvProfile) CATALOG_CONTROLLER_FOCUS_INSET else 0.dp)
            .catalogCardTransform(scale = cardScale, alpha = dimAlpha)
            // One merged node per card. Without this TalkBack reads nothing at all here: UrlImage
            // passes a null contentDescription and phone cards carry no title text of their own.
            .semantics(mergeDescendants = true) {
                contentDescription = game.title
                role = Role.Button
            },
    ) {
        Box(Modifier.catalogCardArtworkSize(squareCard)) {
            Card(
                modifier = Modifier
                    .matchParentSize()
                    .onGloballyPositioned { transitionBounds[0] = it.boundsInWindow() }
                    .then(
                        upFocusRequester?.let { requester ->
                            Modifier.focusProperties { up = requester }
                        } ?: Modifier,
                    )
                    .onFocusChanged { focused = it.isFocused || it.hasFocus }
                    .focusMoveHaptics()
                    .border(
                        width = catalogCardBorderWidthDp(
                            controllerFocused = enhancedControllerFocus,
                            borderEffectsEnabled = LocalAbsoluteCinemaEffects.current,
                        ).dp,
                        color = catalogCardBorderColor(
                            selectionColor = LocalSelectionTintColor.current,
                            gameBorderEnabled = LocalGameCardBordersEnabled.current,
                            controllerFocused = enhancedControllerFocus,
                            borderEffectsEnabled = LocalAbsoluteCinemaEffects.current,
                        ),
                        shape = cardShape,
                    )
                    .onPreviewKeyEvent { event ->
                        when {
                            controllerActionMode && handleCatalogControllerAction(
                                event = event,
                                onFavorite = { onFavorite(game.id) },
                                onPlay = { onPlay(game) },
                            ) -> true
                            isTvActivateKey(event) -> {
                                selectFromCard()
                                true
                            }
                            else -> handleDpadFocusMove(event, focusManager)
                        }
                    }
                    .focusable(interactionSource = interaction),
                colors = CardDefaults.cardColors(
                    containerColor = if (expressiveUi) MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f) else Panel,
                ),
                elevation = CardDefaults.cardElevation(defaultElevation = if (focused) 8.dp else 0.dp),
                shape = cardShape,
            ) {
                Box(
                    Modifier
                        .fillMaxSize()
                        .combinedClickable(
                            interactionSource = interaction,
                            indication = null,
                            onClick = selectFromCard,
                            onLongClick = { onChooseStore(game) },
                            onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
                        ),
                ) {
                    UrlImage(
                        catalogCardImageUrl(game, tvProfile, imageRequestWidth),
                        Modifier.fillMaxSize(),
                        // Always Crop. The card is already locked to NVIDIA's box-art ratio, so for
                        // correctly-cut art this is identical to Fit; when the CDN returns something
                        // off-ratio, Fit pillarboxed it against a flat swatch and Crop simply trims.
                        contentScale = ContentScale.Crop,
                    )
                    if (shouldOverlayCatalogCardTitle(tvProfile)) {
                        GameCardTitleOverlay(game.title)
                    }
                    if (thumbnailFavoriteOverlay) {
                        FavoriteIconButton(
                            favorite = favorite,
                            onClick = { onFavorite(game.id) },
                            modifier = Modifier
                                .align(Alignment.TopStart)
                                .padding(overlayActionPadding),
                            size = overlayActionSize,
                        )
                    }
                }
            }
            ControllerFocusFrame(
                visible = enhancedControllerFocus || selectedOutline || ((focused || hovered) && LocalAbsoluteCinemaEffects.current),
                cornerRadius = if (expressiveUi) OpenNowRadius.md else OpenNowRadius.sm,
                tint = if (selectedOutline || LocalAbsoluteCinemaEffects.current) LocalActiveSelectionColor.current else Color.White,
                secondaryTint = if (selectedOutline || LocalAbsoluteCinemaEffects.current) LocalActiveSelectionSecondaryColor.current else Color.White,
            )
        }
        if (showCaption) {
            CatalogCardCaption(title = game.title)
        }
    }
}

internal fun catalogCardImageUrl(
    game: GameInfo,
    tvProfile: Boolean,
    requestWidth: Int = if (tvProfile) TV_CARD_IMAGE_REQUEST_WIDTH else MOBILE_CARD_IMAGE_REQUEST_WIDTH,
): String? {
    // Keep TV and handheld cards on the same GAME_BOX_ART source. Older caches can contain a
    // TV_BANNER in imageUrl, so apply the same validation on both surfaces and use the dedicated
    // TV artwork only as a compatibility fallback when no mobile poster exists.
    val mobileSource = game.imageUrl
        ?.takeIf { it.isNotBlank() }
        ?.takeIf { !it.contains("img.nvidiagrid.net") || it.contains("/GAME_BOX_ART_") }
    val source = mobileSource
        ?: game.tvCardImageUrl?.takeIf { tvProfile && it.isNotBlank() }
        ?: return null
    return optimizedNvidiaImageUrl(
        source,
        width = requestWidth,
    )
}

private const val TV_CARD_IMAGE_REQUEST_WIDTH = 272
private const val MOBILE_CARD_IMAGE_REQUEST_WIDTH = 512

@Suppress("UNUSED_PARAMETER")
internal fun shouldOverlayCatalogCardTitle(tvProfile: Boolean): Boolean = false

internal fun shouldUseArtworkOnlyCatalogCards(tvProfile: Boolean, controllerActionMode: Boolean): Boolean =
    tvProfile || controllerActionMode

internal fun catalogControllerActionMode(
    tvProfile: Boolean,
    landscapeLayout: Boolean,
    physicalControllerConnected: Boolean,
): Boolean = physicalControllerConnected && (tvProfile || landscapeLayout)

internal fun shouldShowCatalogFavoriteIcon(settings: AppSettings): Boolean =
    settings.showFavoriteIconOnGameCards

/** Titles may be captioned on touch handhelds; controller-first layouts suppress them upstream. */
internal fun shouldShowCatalogCardTitles(tvProfile: Boolean, enabled: Boolean): Boolean =
    enabled && !tvProfile

@Composable
private fun GameCardTitleOverlay(title: String) {
    Box(
        Modifier
            .fillMaxSize()
            .background(GameCardOverlayGradient),
        contentAlignment = Alignment.BottomStart,
    ) {
        Text(
            text = title,
            color = Color.White,
            fontWeight = FontWeight.ExtraBold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
        )
    }
}

private fun handleCatalogControllerAction(
    event: androidx.compose.ui.input.key.KeyEvent,
    onFavorite: () -> Unit,
    onPlay: () -> Unit,
): Boolean {
    if (event.type != KeyEventType.KeyUp) return false
    return when (event.key) {
        Key.ButtonX -> {
            onFavorite()
            true
        }
        Key.ButtonY -> {
            onPlay()
            true
        }
        else -> false
    }
}

internal fun launcherBadgeForStoreKey(storeKey: String?): LauncherBadge =
    when (storeKey) {
        "STEAM" -> LauncherBadge(R.drawable.ic_store_steam, "Steam", Color(0xff17324d))
        "EPIC", "EGS", "EPIC_GAMES_STORE" -> LauncherBadge(R.drawable.ic_store_epic, "Epic", Color(0xff111111))
        "HOYO", "HOYOVERSE", "HOYOPLAY", "HOYO_PLAY", "MIHOYO" -> LauncherBadge(R.drawable.ic_store_hoyo, "HoYo", Color(0xff2b62d9))
        "XBOX", "XBOX_GAME_PASS", "GAME_PASS" -> LauncherBadge(R.drawable.ic_store_xbox, "Xbox", Color(0xff107c10))
        "MICROSOFT", "MICROSOFT_STORE" -> LauncherBadge(R.drawable.ic_store_microsoft, "Microsoft Store", Color(0xff0067b8))
        "UBISOFT", "UBISOFT_CONNECT", "UPLAY" -> LauncherBadge(R.drawable.ic_store_ubisoft, "Ubisoft Connect", Color(0xff006efc))
        "EA", "EA_APP", "ORIGIN" -> LauncherBadge(R.drawable.ic_store_ea, "EA app", Color(0xffff4747))
        "GOG", "GOG.COM", "GOG_COM" -> LauncherBadge(R.drawable.ic_store_gog, "GOG", Color(0xff6a35a8))
        "BATTLENET", "BATTLE.NET", "BATTLE_NET", "BLIZZARD" -> LauncherBadge(R.drawable.ic_store_battlenet, "Battle.net", Color(0xff148eff))
        "RIOT", "RIOT_CLIENT", "RIOT_GAMES" -> LauncherBadge(R.drawable.ic_store_riot, "Riot", Color(0xffd13639))
        "ROCKSTAR", "ROCKSTAR_GAMES", "ROCKSTAR_GAMES_LAUNCHER" -> LauncherBadge(R.drawable.ic_store_rockstar, "Rockstar", Color(0xffffc400), Color(0xff111111))
        "NCSOFT", "NC_SOFT", "PURPLE" -> LauncherBadge(R.drawable.ic_tab_store, "NCSOFT", Color(0xffb4822d), Color(0xff111111))
        "GOOGLE_PLAY", "PLAY_STORE", "ANDROID" -> LauncherBadge(R.drawable.ic_store_google_play, "Google Play", Color(0xff0f9d58))
        "AMAZON", "AMAZON_GAMES" -> LauncherBadge(R.drawable.ic_store_amazon, "Amazon Games", Color(0xffff9900), Color(0xff111111))
        else -> LauncherBadge(R.drawable.ic_tab_store, "GeForce NOW", Color.Black.copy(alpha = 0.72f))
    }

private fun displayStoresForGame(game: GameInfo): String {
    val stores = displayStoresForVariants(game.variants).ifEmpty {
        game.availableStores.map(::gameStoreDisplayName)
    }.distinctBy { normalizeGameStore(it) }
    return stores.joinToString(", ").ifBlank { "GeForce NOW" }
}

@Composable
private fun ZortosPlayMark(
    modifier: Modifier = Modifier,
    ringColor: Color = MaterialTheme.colorScheme.primary,
    playColor: Color = ringColor,
) {
    Canvas(modifier) {
        val play = Path().apply {
            moveTo(size.width * 0.35f, size.height * 0.25f)
            lineTo(size.width * 0.35f, size.height * 0.75f)
            lineTo(size.width * 0.75f, size.height * 0.5f)
            close()
        }
        drawPath(play, playColor)
    }
}

@Composable
internal fun AnimatedLaunchOverlay(
    modifier: Modifier = Modifier,
    enterFromTop: Boolean = false,
    content: @Composable () -> Unit,
) {
    val visibleState = remember {
        MutableTransitionState(false).apply {
            targetState = true
        }
    }
    AnimatedVisibility(
        visibleState = visibleState,
        enter = fadeIn() +
            slideInVertically(initialOffsetY = { if (enterFromTop) -it / 4 else it / 4 }) +
            scaleIn(initialScale = 0.94f),
        exit = fadeOut() +
            slideOutVertically(targetOffsetY = { if (enterFromTop) -it / 4 else it / 4 }) +
            scaleOut(targetScale = 0.94f),
        modifier = modifier,
    ) {
        content()
    }
}

internal suspend fun requestFocusWithRetry(
    focusRequester: FocusRequester,
    initialDelayMs: Long = 80L,
    retryDelayMs: Long = 70L,
    attempts: Int = 4,
): Boolean {
    if (initialDelayMs > 0L) delay(initialDelayMs)
    repeat(attempts.coerceAtLeast(1)) { attempt ->
        if (runCatching { focusRequester.requestFocus() }.getOrDefault(false)) return true
        if (attempt + 1 < attempts) delay(retryDelayMs.coerceAtLeast(0L))
    }
    return false
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun GameDetailsSheet(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    fullScreen: Boolean,
    safeAreaPadding: Dp,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
) {
    val transitionRegistry = LocalGameDetailsTransitionRegistry.current
    val transitionOrigin = transitionRegistry?.originFor(game.id)
    val cardTransitionOrigin = transitionOrigin
        ?.takeIf { it.kind == GameDetailsTransitionKind.Card }
        ?.bounds
    val reduceMotion = LocalReduceMotion.current
    val containerProgress = remember(game.id) {
        Animatable(if (cardTransitionOrigin == null || reduceMotion) 1f else 0f)
    }
    LaunchedEffect(game.id, cardTransitionOrigin, reduceMotion) {
        if (cardTransitionOrigin == null || reduceMotion) {
            containerProgress.snapTo(1f)
        } else {
            containerProgress.snapTo(0f)
            containerProgress.animateTo(
                targetValue = 1f,
                animationSpec = tween(
                    durationMillis = OpenNowMotion.DurationStandard,
                    easing = OpenNowMotion.EasingStandard,
                ),
            )
        }
    }
    DisposableEffect(game.id, transitionRegistry) {
        onDispose { transitionRegistry?.clear(game.id) }
    }
    val gameFocusRequester = remember(game.id) { FocusRequester() }
    val playFocusRequester = remember(game.id) { FocusRequester() }
    LaunchedEffect(game.id, fullScreen) {
        val initialRequester = if (shouldInitiallyFocusGameDetailsPlay(tvProfile = fullScreen)) {
            playFocusRequester
        } else {
            gameFocusRequester
        }
        requestFocusWithRetry(initialRequester)
    }
    BackHandler(onBack = onDismiss)
    // Drag-to-dismiss for the phone sheet. Everyone reaches for this gesture on a bottom sheet and
    // previously nothing happened — there was no handle and no drag response at all. Implemented
    // here rather than by switching to ModalBottomSheet so the sheet keeps its lockedFocusGroup and
    // focus requesters, which the controller and TV navigation depend on.
    val density = LocalDensity.current
    var dragOffset by remember(game.id) { mutableFloatStateOf(0f) }
    var dismissRequested by remember(game.id) { mutableStateOf(false) }
    val dismissThresholdPx = with(density) { SHEET_DISMISS_DRAG_THRESHOLD.toPx() }
    val dismissGestureGate = remember(game.id) { SheetDismissGestureGate() }
    fun requestDismissOnce() {
        if (dismissRequested) return
        dismissRequested = true
        onDismiss()
    }
    fun settleSheetDrag(velocity: Float = 0f) {
        dismissGestureGate.reset()
        if (dragOffset > dismissThresholdPx || velocity > SHEET_DISMISS_FLING_VELOCITY) {
            requestDismissOnce()
        } else {
            dragOffset = 0f
        }
    }
    LaunchedEffect(dragOffset, fullScreen) {
        // Nested scrolling does not guarantee a fling callback on every OEM/input path. Closing as
        // soon as the sheet crosses the threshold prevents an off-screen sheet from leaving only
        // its modal scrim behind waiting for a second tap.
        if (!fullScreen && dragOffset > dismissThresholdPx) requestDismissOnce()
    }
    val dragState = rememberDraggableState { delta ->
        dragOffset = (dragOffset + delta).coerceAtLeast(0f)
    }
    val sheetNestedScroll = remember(game.id, fullScreen, dismissThresholdPx) {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: androidx.compose.ui.input.nestedscroll.NestedScrollSource): Offset {
                if (fullScreen || dragOffset <= 0f || available.y >= 0f) return Offset.Zero
                val consumed = available.y.coerceAtLeast(-dragOffset)
                dragOffset += consumed
                return Offset(0f, consumed)
            }

            override fun onPostScroll(
                consumed: Offset,
                available: Offset,
                source: androidx.compose.ui.input.nestedscroll.NestedScrollSource,
            ): Offset {
                if (fullScreen) return Offset.Zero
                val dismissDelta = dismissGestureGate.dismissDelta(
                    childConsumedY = consumed.y,
                    availableY = available.y,
                )
                if (dismissDelta <= 0f) return Offset.Zero
                dragOffset += dismissDelta
                return Offset(0f, dismissDelta)
            }

            override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
                if (!fullScreen && dragOffset > 0f) {
                    settleSheetDrag(available.y)
                } else {
                    dismissGestureGate.reset()
                }
                return Velocity.Zero
            }
        }
    }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .lockedFocusGroup()
            .clickable(onClick = onDismiss),
        contentAlignment = Alignment.BottomCenter,
    ) {
        val targetHeightPx = constraints.maxHeight.toFloat() * if (fullScreen) 1f else 0.92f
        val targetBounds = Rect(
            left = 0f,
            top = constraints.maxHeight.toFloat() - targetHeightPx,
            right = constraints.maxWidth.toFloat(),
            bottom = constraints.maxHeight.toFloat(),
        )
        val surfaceShape = if (fullScreen) {
            RoundedCornerShape(0.dp)
        } else {
            RoundedCornerShape(topStart = OpenNowRadius.xl, topEnd = OpenNowRadius.xl)
        }
        // Reading Animatable state inside graphicsLayer invalidates only the render layer. Reading
        // it in composition used to rebuild the complete details tree on every 120 Hz frame.
        Box(
            Modifier
                .matchParentSize()
                .graphicsLayer {
                    alpha = if (cardTransitionOrigin == null) {
                        0.72f
                    } else {
                        0.28f + (0.44f * containerProgress.value)
                    }
                }
                .background(Color.Black),
        )
        Surface(
            modifier = Modifier
                .then(
                    if (fullScreen) {
                        Modifier.fillMaxSize()
                    } else {
                        Modifier
                            .fillMaxWidth()
                            .fillMaxHeight(0.92f)
                            .offset { IntOffset(0, dragOffset.roundToInt()) }
                            .nestedScroll(sheetNestedScroll)
                            .draggable(
                                state = dragState,
                                orientation = Orientation.Vertical,
                                onDragStopped = { velocity -> settleSheetDrag(velocity) },
                            )
                    },
                )
                .graphicsLayer {
                    if (cardTransitionOrigin != null) {
                        val transform = gameDetailsContainerTransform(
                            source = cardTransitionOrigin,
                            target = targetBounds,
                            progress = containerProgress.value,
                        )
                        transformOrigin = TransformOrigin(0f, 0f)
                        scaleX = transform.scaleX
                        scaleY = transform.scaleY
                        translationX = transform.translationX
                        translationY = transform.translationY
                        clip = true
                        shape = surfaceShape
                    }
                }
                .clickable(onClick = {}),
            shape = surfaceShape,
            color = Panel,
            tonalElevation = 8.dp,
        ) {
            Column(Modifier.fillMaxSize()) {
                if (!fullScreen) {
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .padding(vertical = OpenNowSpacing.md),
                        contentAlignment = Alignment.Center,
                    ) {
                        Box(
                            Modifier
                                .size(width = 34.dp, height = 4.dp)
                                .clip(CircleShape)
                                .background(TextMuted.copy(alpha = 0.45f)),
                        )
                    }
                }
            BoxWithConstraints(
                Modifier
                    .fillMaxSize()
                    .padding(if (fullScreen) safeAreaPadding else 0.dp),
            ) {
                val aspect = if (maxHeight.value > 0f) maxWidth.value / maxHeight.value else 1f
                val landscapeTvLayout = maxWidth >= 720.dp && aspect >= 1.35f
                val phoneLandscapeLayout = landscapeTvLayout && minOf(maxWidth, maxHeight) < PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH
                if (landscapeTvLayout) {
                    GameDetailsLandscapeContent(
                        game = game,
                        favorite = favorite,
                        defaultVariantId = defaultVariantId,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                        onFavorite = onFavorite,
                        connectedTvName = connectedTvName,
                        onPlayOnTv = onPlayOnTv,
                        onDismiss = onDismiss,
                        gameFocusRequester = gameFocusRequester,
                        playFocusRequester = playFocusRequester,
                        shortHeight = maxHeight <= 620.dp,
                        imageActionsOverlay = phoneLandscapeLayout,
                    )
                } else {
                    GameDetailsScrollableContent(
                        game = game,
                        favorite = favorite,
                        defaultVariantId = defaultVariantId,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                        onFavorite = onFavorite,
                        connectedTvName = connectedTvName,
                        onPlayOnTv = onPlayOnTv,
                        onDismiss = onDismiss,
                        gameFocusRequester = gameFocusRequester,
                        playFocusRequester = playFocusRequester,
                    )
                }
            }
            }
        }
    }
}

/** How far the sheet must be dragged down before letting go dismisses it. */
private val SHEET_DISMISS_DRAG_THRESHOLD = 140.dp

/** A fast enough flick dismisses regardless of distance travelled. */
private const val SHEET_DISMISS_FLING_VELOCITY = 1_200f

/**
 * A scroll that began below the top may finish scrolling the details, but it cannot immediately
 * turn into a sheet dismissal. The reader must lift and start a fresh pull from the top.
 */
internal class SheetDismissGestureGate {
    private var childScrolledDuringGesture = false

    fun dismissDelta(childConsumedY: Float, availableY: Float): Float {
        if (childConsumedY > 0f) childScrolledDuringGesture = true
        return availableY.takeIf { it > 0f && !childScrolledDuringGesture } ?: 0f
    }

    fun reset() {
        childScrolledDuringGesture = false
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsLandscapeContent(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
    gameFocusRequester: FocusRequester,
    playFocusRequester: FocusRequester,
    shortHeight: Boolean,
    imageActionsOverlay: Boolean,
) {
    val description = gameDescriptionForDetails(game)
    val context = LocalContext.current
    val sideScrollState = rememberScrollState()
    val detailsSpacing = if (shortHeight) 8.dp else 10.dp
    var gameFocused by remember(game.id) { mutableStateOf(false) }
    val gameImageInteraction = remember(game.id) { MutableInteractionSource() }
    val gameImageHovered by gameImageInteraction.collectIsHoveredAsState()
    Row(
        Modifier
            .fillMaxSize()
            .padding(horizontal = if (shortHeight) 18.dp else 24.dp, vertical = if (shortHeight) 16.dp else 22.dp),
        horizontalArrangement = Arrangement.spacedBy(if (shortHeight) 16.dp else 22.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val imageShape = RoundedCornerShape(20.dp)
        Box(
            Modifier
                .weight(0.92f)
                .fillMaxHeight()
                .focusRequester(gameFocusRequester)
                .focusProperties { right = playFocusRequester }
                .onFocusChanged { gameFocused = it.isFocused }
                .hoverable(gameImageInteraction)
                .clickable {
                    onDismiss()
                    onPlay(game)
                },
        ) {
            Box(
                Modifier
                    .fillMaxSize()
                    .gameDetailsArtworkEntrance(game.id)
                    .border(
                        width = 1.dp,
                        color = catalogCardBorderColor(
                            LocalActiveSelectionColor.current,
                            LocalGameCardBordersEnabled.current,
                        ),
                        shape = imageShape,
                    )
                    .clip(imageShape),
            ) {
                UrlImage(gameHeroImageUrl(context, game), Modifier.fillMaxSize())
                GameImageTitleOverlay(
                    game = game,
                    compact = shortHeight,
                    reserveEndSpace = imageActionsOverlay,
                    modifier = Modifier.align(Alignment.BottomStart),
                )
                if (imageActionsOverlay) {
                    ImageCloseButton(
                        onClick = onDismiss,
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .padding(10.dp),
                    )
                }
                if (imageActionsOverlay) {
                    Column(
                        modifier = Modifier
                            .align(Alignment.BottomEnd)
                            .padding(14.dp)
                            .width(150.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        connectedTvName?.let {
                            OutlinedButton(
                                onClick = {
                                    onDismiss()
                                    onPlayOnTv(game)
                                },
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(stringResource(R.string.action_play_on_tv_generic), maxLines = 1)
                            }
                        }
                        LongPressPlayButton(
                            onClick = {
                                onDismiss()
                                onPlay(game)
                            },
                            onLongClick = {
                                onDismiss()
                                onChooseStore(game)
                            },
                            modifier = Modifier
                                .fillMaxWidth(),
                            focusRequester = playFocusRequester,
                        )
                    }
                }
            }
            AbsoluteCinemaEverywhereFrame(
                visible = gameFocused || gameImageHovered,
                cornerRadius = 20.dp,
            )
        }

        Column(
            Modifier
                .weight(1.08f)
                .fillMaxHeight(),
            verticalArrangement = Arrangement.spacedBy(detailsSpacing),
        ) {
            if (imageActionsOverlay) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .verticalScroll(sideScrollState),
                    verticalArrangement = Arrangement.spacedBy(detailsSpacing),
                ) {
                    GameDetailsCompactInfoContent(
                        game = game,
                        defaultVariantId = defaultVariantId,
                        description = description,
                    )
                }
            } else {
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .verticalScroll(sideScrollState),
                    verticalArrangement = Arrangement.spacedBy(detailsSpacing),
                ) {
                    GameDetailsCompactInfoContent(
                        game = game,
                        defaultVariantId = defaultVariantId,
                        description = description,
                    )
                }
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    var dismissFocused by remember { mutableStateOf(false) }
                    val accent = MaterialTheme.colorScheme.primary
                    OutlinedButton(
                        onClick = onDismiss,
                        border = BorderStroke(1.dp, if (dismissFocused) accent else MaterialTheme.colorScheme.outline),
                        modifier = Modifier
                            .weight(1f)
                            .height(48.dp)
                            .onFocusChanged { dismissFocused = it.isFocused }
                    ) {
                        Text(
                            stringResource(R.string.action_dismiss),
                            color = if (dismissFocused) accent else TextPrimary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                    LongPressPlayButton(
                        onClick = {
                            onDismiss()
                            onPlay(game)
                        },
                        onLongClick = {
                            onDismiss()
                            onChooseStore(game)
                        },
                        modifier = Modifier.weight(1f),
                        focusRequester = playFocusRequester,
                    )
                    connectedTvName?.let {
                        OutlinedButton(
                            onClick = {
                                onDismiss()
                                onPlayOnTv(game)
                            },
                            modifier = Modifier.weight(1f).height(48.dp),
                        ) {
                            Text(stringResource(R.string.action_play_on_tv_generic), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun GameDetailsCompactInfoContent(
    game: GameInfo,
    defaultVariantId: String?,
    description: String?,
) {
    OwnershipStatusRow(game = game, compact = true)
    GameGenreChips(game = game, compact = true)
    GameScreenshotGallery(game = game, compact = true)
    GameDescriptionDisclosure(
        description = description,
        compact = true,
    )
    CompactDetailRows(game)
    LaunchOptionsList(
        game = game,
        defaultVariantId = defaultVariantId,
        compact = true,
    )
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsScrollableContent(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
    gameFocusRequester: FocusRequester,
    playFocusRequester: FocusRequester,
) {
    val context = LocalContext.current
    var gameFocused by remember(game.id) { mutableStateOf(false) }
    val gameImageInteraction = remember(game.id) { MutableInteractionSource() }
    val gameImageHovered by gameImageInteraction.collectIsHoveredAsState()
    Column(Modifier.fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.weight(1f),
            contentPadding = PaddingValues(bottom = 18.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            item {
                val imageShape = RoundedCornerShape(OpenNowRadius.lg)
                Box(
                    Modifier
                        .fillMaxWidth()
                        // Scales with the screen instead of being pinned at 220dp, which was
                        // cramped on a tablet and oversized on a small phone.
                        .aspectRatio(16f / 9f)
                        .padding(horizontal = 10.dp, vertical = 6.dp)
                        .focusRequester(gameFocusRequester)
                        .focusProperties { down = playFocusRequester }
                        .onFocusChanged { gameFocused = it.isFocused }
                        .hoverable(gameImageInteraction)
                        .clickable {
                            onDismiss()
                            onPlay(game)
                        },
                ) {
                    Box(
                        Modifier
                            .fillMaxSize()
                            .gameDetailsArtworkEntrance(game.id)
                            .border(
                                width = 1.dp,
                                color = catalogCardBorderColor(
                                    LocalActiveSelectionColor.current,
                                    LocalGameCardBordersEnabled.current,
                                ),
                                shape = imageShape,
                            )
                            .clip(imageShape),
                    ) {
                        UrlImage(
                            gameHeroImageUrl(context, game),
                            Modifier.fillMaxSize(),
                        )
                        // Guarantees the title overlay stays legible over bright key art.
                        Box(
                            Modifier
                                .matchParentSize()
                                .background(
                                    Brush.verticalGradient(
                                        0.4f to Color.Transparent,
                                        1f to Color.Black.copy(alpha = 0.75f),
                                    ),
                                ),
                        )
                        GameImageTitleOverlay(
                            game = game,
                            compact = false,
                            reserveEndSpace = false,
                            modifier = Modifier.align(Alignment.BottomStart),
                        )
                    }
                    AbsoluteCinemaEverywhereFrame(
                        visible = gameFocused || gameImageHovered,
                        cornerRadius = OpenNowRadius.lg,
                    )
                }
            }
            item {
                Column(Modifier.padding(horizontal = 18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    val description = gameDescriptionForDetails(game)
                    OwnershipStatusRow(game = game, compact = false)
                    GameGenreChips(game = game, compact = false)
                    GameScreenshotGallery(game = game, compact = false)
                    GameDescriptionDisclosure(
                        description = description,
                        compact = false,
                    )
                    DetailRows(game)
                    LaunchOptionsList(
                        game = game,
                        defaultVariantId = defaultVariantId,
                        compact = false,
                    )
                }
            }
        }
        Surface(color = Panel.copy(alpha = 0.98f), tonalElevation = 8.dp) {
            // Play is the point of the screen, so it takes the width. Dismiss and the secondary
            // actions become fixed-size icons rather than equal-weight buttons that squeezed Play
            // down to a third of the bar whenever a TV was connected.
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 14.dp, vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                var dismissFocused by remember { mutableStateOf(false) }
                val accent = MaterialTheme.colorScheme.primary
                IconButton(
                    onClick = onDismiss,
                    modifier = Modifier
                        .size(48.dp)
                        .onFocusChanged { dismissFocused = it.isFocused },
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_clear),
                        contentDescription = stringResource(R.string.action_dismiss),
                        tint = if (dismissFocused) accent else TextMuted,
                    )
                }
                // favorite/onFavorite were already threaded into this composable but never used —
                // on phones the only way to favourite a game was from the grid.
                FavoriteIconButton(
                    favorite = favorite,
                    onClick = { onFavorite(game.id) },
                    size = 48.dp,
                )
                LongPressPlayButton(
                    onClick = {
                        onDismiss()
                        onPlay(game)
                    },
                    onLongClick = {
                        onDismiss()
                        onChooseStore(game)
                    },
                    modifier = Modifier.weight(1f),
                    focusRequester = playFocusRequester,
                )
                connectedTvName?.let { tvName ->
                    IconButton(
                        onClick = {
                            onDismiss()
                            onPlayOnTv(game)
                        },
                        modifier = Modifier.size(48.dp),
                    ) {
                        Icon(
                            imageVector = Icons.Outlined.Cast,
                            contentDescription = stringResource(R.string.action_play_on_tv, tvName),
                            tint = TextPrimary,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun LaunchOptionsList(
    game: GameInfo,
    defaultVariantId: String?,
    compact: Boolean,
) {
    val variants = launchableGameVariants(game.variants)
    if (variants.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 8.dp)) {
        Text(
            stringResource(R.string.store_selector_launchers),
            color = TextMuted,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
        )
        variants.take(if (compact) 3 else variants.size).forEach { variant ->
            val isDefault = variant.id == defaultVariantId
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(if (compact) 12.dp else 14.dp),
                color = if (isDefault) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f) else PanelAlt,
                contentColor = TextPrimary,
            ) {
                Row(
                    Modifier.padding(horizontal = if (compact) 10.dp else 12.dp, vertical = if (compact) 8.dp else 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    ConnectorStoreIcon(
                        launcherBadgeForStoreKey(splitGameStoreKeys(variant.store).firstOrNull()),
                    )
                    Column(Modifier.weight(1f)) {
                        Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        val details = variantDetailsText(variant)
                        Text(
                            if (isDefault) {
                                listOf(stringResource(R.string.store_selector_default), details).filter { it.isNotBlank() }.joinToString(" - ")
                            } else {
                                details.ifBlank { stringResource(R.string.store_selector_available_launcher) }
                            },
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = if (compact) 1 else 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LongPressPlayButton(
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
    focusRequester: FocusRequester? = null,
) {
    val controllerFocusEnabled = LocalControllerFocusEnabled.current
    var focused by remember { mutableStateOf(false) }
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val pressed by interaction.collectIsPressedAsState()
    val controllerFocused = focused && controllerFocusEnabled
    val shape = RoundedCornerShape(999.dp)
    val accent = MaterialTheme.colorScheme.primary
    val focusScale = animateFloatAsState(
        targetValue = if (pressed) 0.95f else gameDetailsPlayFocusScale(controllerFocused),
        animationSpec = tween(
            durationMillis = OpenNowMotion.DurationFast,
            easing = OpenNowMotion.EasingStandard,
        ),
        label = "game-details-play-focus-scale",
    )
    val containerColor by animateColorAsState(
        targetValue = if (controllerFocused) Color.White else accent,
        animationSpec = tween(durationMillis = 120),
        label = "game-details-play-focus-color",
    )
    Box(
        modifier = modifier.graphicsLayer {
            scaleX = focusScale.value
            scaleY = focusScale.value
        },
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .height(48.dp)
                .then(
                    focusRequester?.let { requester -> Modifier.focusRequester(requester) }
                        ?: Modifier,
                )
                .onFocusChanged { focusState -> focused = focusState.isFocused }
                .hoverable(interaction)
                .onPreviewKeyEvent { event ->
                    if (isTvActivateKey(event)) {
                        onClick()
                        true
                    } else {
                        false
                    }
                }
                .focusable()
                .combinedClickable(
                    interactionSource = interaction,
                    indication = null,
                    onClick = onClick,
                    onLongClick = onLongClick,
                    onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
                )
                .then(
                    if (controllerFocused) {
                        Modifier.border(
                            width = gameDetailsPlayFocusBorderWidthDp(controllerFocused).dp,
                            color = accent,
                            shape = shape,
                        )
                    } else {
                        Modifier
                    },
                ),
            shape = shape,
            color = containerColor,
            tonalElevation = 0.dp,
            shadowElevation = if (controllerFocused) 12.dp else 0.dp,
        ) {
            Row(
                Modifier.fillMaxSize().padding(horizontal = 18.dp),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ZortosPlayMark(
                    modifier = Modifier.size(20.dp),
                    ringColor = Color.Black,
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    stringResource(R.string.action_play),
                    color = Color.Black,
                    fontWeight = if (controllerFocused) FontWeight.ExtraBold else FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        ControllerFocusFrame(
            visible =
                (controllerFocused && LocalAbsoluteCinemaEffects.current) ||
                    (hovered && LocalAbsoluteCinemaEverywhere.current),
            cornerRadius = 24.dp,
            tint = LocalActiveSelectionColor.current,
            secondaryTint = LocalActiveSelectionSecondaryColor.current,
        )
    }
}

internal fun gameDetailsPlayFocusScale(focused: Boolean): Float = if (focused) 1.06f else 1f

internal fun gameDetailsPlayFocusBorderWidthDp(focused: Boolean): Float = if (focused) 4f else 0f

private fun variantDetailsText(variant: GameVariant): String =
    listOfNotNull(
        variant.libraryStatus?.takeIf { it.isNotBlank() }?.let(::formatGameMetadataLabel),
        variant.supportedControls.takeIf { it.isNotEmpty() }?.joinToString(", ") { formatGameMetadataLabel(it) },
        variant.lastPlayedDate?.takeIf { it.isNotBlank() }?.let { "Last played $it" },
    ).joinToString(" - ")

@Composable
private fun ImageCloseButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    var focused by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    Surface(
        modifier = modifier
            .size(44.dp)
            .onFocusChanged { focused = it.isFocused }
            .border(
                width = 2.dp,
                color = if (focused) accent else Color.Transparent,
                shape = CircleShape
            )
            .clickable(onClick = onClick),
        shape = CircleShape,
        color = Color.Black.copy(alpha = 0.58f),
        tonalElevation = 3.dp,
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Icon(
                painter = painterResource(R.drawable.ic_clear),
                contentDescription = stringResource(R.string.action_cancel),
                tint = TextPrimary,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

@Composable
private fun FavoriteIconButton(favorite: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, size: Dp = 44.dp) {
    val label = stringResource(if (favorite) R.string.action_saved else R.string.action_save)
    var focused by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    Surface(
        modifier = modifier
            .minimumInteractiveComponentSize()
            .size(size)
            .onFocusChanged { focused = it.isFocused }
            .semantics {
                contentDescription = label
                role = Role.Button
            }
            .clickable(onClick = onClick)
            .focusable()
            .then(
                if (focused) Modifier.border(2.dp, accent, CircleShape) else Modifier
            ),
        shape = CircleShape,
        color = Color.Black.copy(alpha = 0.35f),
        tonalElevation = 0.dp,
        border = if (LocalAbsoluteCinemaEffects.current) BorderStroke(1.dp, accent) else null,
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Icon(
                painter = painterResource(if (favorite) R.drawable.ic_save_filled else R.drawable.ic_save),
                contentDescription = null,
                tint = if (favorite) MaterialTheme.colorScheme.primary else TextPrimary,
                modifier = Modifier.size(size * 0.5f),
            )
        }
    }
}

internal fun gameDescriptionForDetails(game: GameInfo): String? =
    game.description?.takeIf { it.isNotBlank() }
        ?: game.longDescription?.takeIf { it.isNotBlank() }

private fun gameHeroImageUrl(context: Context, game: GameInfo?): String? {
    val url = game?.screenshotUrl?.takeIf { it.isNotBlank() }
        ?: game?.tvBannerUrl?.takeIf { it.isNotBlank() }
        ?: game?.screenshotUrls?.firstOrNull { it.isNotBlank() }
        ?: game?.imageUrl?.takeIf { it.isNotBlank() }
        ?: return null
    return optimizedNvidiaImageUrl(url, wideImageRequestWidth(context))
}

internal fun gameTvBannerImageUrl(context: Context, game: GameInfo?): String? {
    val url = game?.tvBannerUrl?.takeIf { it.isNotBlank() }
        ?: game?.screenshotUrl?.takeIf { it.isNotBlank() }
        ?: game?.imageUrl?.takeIf { it.isNotBlank() }
        ?: return null
    return optimizedNvidiaImageUrl(url, wideImageRequestWidth(context))
}

private fun optimizedNvidiaImageUrl(url: String, width: Int): String {
    if (!url.contains("img.nvidiagrid.net")) return url
    val base = url
        .substringBefore(";f=")
        .substringBefore(";w=")
        .substringBefore(";h=")
        .substringBefore(";dpr=")
    return "$base;f=webp;w=$width"
}

/**
 * Cached because the measurement below is a binder round trip to the system server, and the URL
 * builders that need it are called from composable bodies — once per artwork, per recomposition.
 * A carousel that re-runs on a timer turned that into a steady drip of IPC for a number that only
 * changes when the network does.
 */
private object ImageRequestWidthCache {
    private const val TTL_MS = 30_000L

    @Volatile
    private var cachedWidth = 0

    @Volatile
    private var cachedAtMs = 0L

    @Volatile
    private var cachedDisplayWidth = 0

    fun width(context: Context): Int {
        val now = SystemClock.elapsedRealtime()
        val displayWidth = context.resources.displayMetrics.widthPixels
        val cached = cachedWidth
        if (cached != 0 && displayWidth == cachedDisplayWidth && now - cachedAtMs < TTL_MS) return cached
        val measured = measureWideImageRequestWidth(context, displayWidth)
        cachedWidth = measured
        cachedDisplayWidth = displayWidth
        cachedAtMs = now
        return measured
    }
}

private fun wideImageRequestWidth(context: Context): Int = ImageRequestWidthCache.width(context)

private fun measureWideImageRequestWidth(context: Context, displayWidth: Int): Int {
    val connectivity = context.applicationContext.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
    val capabilities = connectivity?.getNetworkCapabilities(connectivity.activeNetwork)
    val downstreamKbps = capabilities?.linkDownstreamBandwidthKbps ?: 0
    val networkWidth = when {
        downstreamKbps >= 25_000 -> 1920
        downstreamKbps in 10_000 until 25_000 -> 1600
        downstreamKbps in 3_000 until 10_000 -> 1280
        downstreamKbps in 1 until 3_000 -> 960
        capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true -> 1600
        capabilities?.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) == true -> 960
        else -> 1280
    }
    return boundedWideImageRequestWidth(networkWidth, displayWidth)
}

/**
 * Keeps detail and hero decodes near the physical display size while retaining a little headroom
 * for crop and scale. Fixed buckets also preserve CDN and disk-cache reuse across nearby devices.
 */
internal fun boundedWideImageRequestWidth(networkWidth: Int, displayWidth: Int): Int {
    if (displayWidth <= 0) return networkWidth
    val displayTarget = when {
        displayWidth <= 720 -> 960
        displayWidth <= 1080 -> 1280
        displayWidth <= 1440 -> 1600
        else -> 1920
    }
    return minOf(networkWidth, displayTarget)
}

@Composable
private fun GameImageTitleOverlay(
    game: GameInfo,
    compact: Boolean,
    reserveEndSpace: Boolean,
    modifier: Modifier = Modifier,
) {
    val textShadow = Shadow(
        color = Color.Black,
        offset = Offset(0f, 3f),
        blurRadius = 14f,
    )
    Column(
        modifier
            .fillMaxWidth()
            .padding(
                start = if (compact) 12.dp else 16.dp,
                top = if (compact) 9.dp else 12.dp,
                end = if (reserveEndSpace) 154.dp else if (compact) 12.dp else 16.dp,
                bottom = if (compact) 10.dp else 14.dp,
            ),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Text(
            game.title,
            color = TextPrimary,
            style = (if (compact) MaterialTheme.typography.titleLarge else MaterialTheme.typography.headlineSmall).copy(
                shadow = textShadow,
            ),
            fontWeight = FontWeight.Bold,
            maxLines = if (compact) 2 else 2,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            game.publisherName?.takeIf { it.isNotBlank() } ?: stringResource(R.string.catalog_unknown_publisher),
            color = TextPrimary.copy(alpha = 0.88f),
            style = MaterialTheme.typography.bodyMedium.copy(shadow = textShadow),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun GameTitleBlock(game: GameInfo, compact: Boolean) {
    Column(verticalArrangement = Arrangement.spacedBy(if (compact) 3.dp else 5.dp)) {
        Text(
            game.title,
            color = TextPrimary,
            style = if (compact) MaterialTheme.typography.titleLarge else MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
            maxLines = if (compact) 2 else 3,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            game.publisherName?.takeIf { it.isNotBlank() } ?: stringResource(R.string.catalog_unknown_publisher),
            color = TextMuted,
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun OwnershipStatusRow(game: GameInfo, compact: Boolean) {
    val ownedStores = ownedStoreLabels(game)
    val shape = RoundedCornerShape(if (compact) 12.dp else 14.dp)
    if (ownedStores.isEmpty()) {
        val availableStores = availableStoreLabels(game)
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = shape,
            color = Color(0xff4a1216),
            tonalElevation = 0.dp,
        ) {
            Row(
                modifier = Modifier.padding(horizontal = 12.dp, vertical = if (compact) 8.dp else 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    stringResource(R.string.catalog_not_owned),
                    color = OpenNowPalette.OnErrorContainer,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.weight(1f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                availableStores.forEach { store ->
                    ConnectorStoreIcon(launcherBadgeForStoreKey(normalizeGameStore(store)))
                }
            }
        }
        return
    }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        ownedStores.forEach { store ->
            val badge = launcherBadgeForStoreKey(normalizeGameStore(store))
            Surface(
                shape = shape,
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.16f),
                tonalElevation = 0.dp,
            ) {
                Row(
                    Modifier.padding(horizontal = 10.dp, vertical = if (compact) 6.dp else 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    ConnectorStoreIcon(badge)
                    Text(
                        "Owned on $store",
                        color = TextPrimary,
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

private fun ownedStoreLabels(game: GameInfo): List<String> =
    libraryStoreDisplayNames(game).ifEmpty {
        if (isGameInLibrary(game)) listOf("GeForce NOW") else emptyList()
    }

private fun availableStoreLabels(game: GameInfo): List<String> =
    displayStoresForVariants(game.variants).ifEmpty {
        game.availableStores.map(::gameStoreDisplayName)
    }
        .map(String::trim)
        .filter { it.isNotBlank() && !it.equals("none", ignoreCase = true) }
        .distinctBy(::normalizeGameStore)

@Composable
private fun GameGenreChips(game: GameInfo, compact: Boolean) {
    val genres = game.genres
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .map(::formatGameMetadataLabel)
        .filterNot(::isNoisyGameTag)
        .distinctBy { it.lowercase(Locale.US) }
        .take(if (compact) 12 else 20)
    if (genres.isEmpty()) return
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 7.dp),
        contentPadding = PaddingValues(end = if (compact) 6.dp else 8.dp),
    ) {
        items(genres, key = { it }) { label ->
            AssistChip(onClick = {}, label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
        }
    }
}

@Composable
private fun GameScreenshotGallery(game: GameInfo, compact: Boolean) {
    val screenshots = game.screenshotUrls
        .map(String::trim)
        .filter(String::isNotBlank)
        .distinct()
    if (screenshots.isEmpty()) return
    val context = LocalContext.current
    val requestWidth = remember(context) { wideImageRequestWidth(context).coerceAtLeast(960) }
    var fullscreenIndex by remember(screenshots) { mutableStateOf<Int?>(null) }
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(if (compact) 7.dp else 9.dp),
    ) {
        Text(
            stringResource(R.string.catalog_screenshots),
            color = TextPrimary,
            style = if (compact) MaterialTheme.typography.labelLarge else MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
        )
        LazyRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
            contentPadding = PaddingValues(end = 8.dp),
        ) {
            itemsIndexed(screenshots, key = { _, screenshot -> screenshot }) { index, screenshot ->
                val hoverInteraction = remember(screenshot) { MutableInteractionSource() }
                val hovered by hoverInteraction.collectIsHoveredAsState()
                Box(
                    modifier = Modifier
                        .width(if (compact) 224.dp else 288.dp)
                        .aspectRatio(16f / 9f),
                ) {
                    Surface(
                        modifier = Modifier
                            .matchParentSize()
                            .hoverable(hoverInteraction)
                            .clickable { fullscreenIndex = index },
                        shape = RoundedCornerShape(if (compact) 12.dp else 14.dp),
                        color = Color.Black,
                        border = if (LocalAbsoluteCinemaEffects.current) {
                            BorderStroke(1.dp, LocalActiveSelectionColor.current)
                        } else {
                            null
                        },
                    ) {
                        UrlImage(
                            url = optimizedNvidiaImageUrl(screenshot, requestWidth),
                            modifier = Modifier.fillMaxSize(),
                            contentScale = ContentScale.Fit,
                        )
                    }
                    AbsoluteCinemaEverywhereFrame(
                        visible = hovered,
                        cornerRadius = if (compact) 12.dp else 14.dp,
                    )
                }
            }
        }
    }
    fullscreenIndex?.let { initialIndex ->
        FullscreenScreenshotViewer(
            screenshots = screenshots,
            initialIndex = initialIndex,
            requestWidth = requestWidth.coerceAtLeast(1920),
            onDismiss = { fullscreenIndex = null },
        )
    }
}

@Composable
private fun FullscreenScreenshotViewer(
    screenshots: List<String>,
    initialIndex: Int,
    requestWidth: Int,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    var index by remember(screenshots, initialIndex) { mutableIntStateOf(initialIndex.coerceIn(screenshots.indices)) }
    var horizontalDrag by remember { mutableFloatStateOf(0f) }
    val swipeThreshold = with(LocalDensity.current) { 56.dp.toPx() }
    val dragState = rememberDraggableState { delta -> horizontalDrag += delta }
    LaunchedEffect(screenshots, index, requestWidth) {
        val imageLoader = SingletonImageLoader.get(context)
        listOf(index - 1, index + 1)
            .filter { it in screenshots.indices }
            .forEach { adjacentIndex ->
                imageLoader.execute(
                    ImageRequest.Builder(context)
                        .data(optimizedNvidiaImageUrl(screenshots[adjacentIndex], requestWidth))
                        .build(),
                )
            }
    }
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(
            dismissOnBackPress = true,
            dismissOnClickOutside = false,
            usePlatformDefaultWidth = false,
        ),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color.Black)
                .draggable(
                    state = dragState,
                    orientation = Orientation.Horizontal,
                    enabled = screenshots.size > 1,
                    onDragStarted = { horizontalDrag = 0f },
                    onDragStopped = {
                        if (abs(horizontalDrag) >= swipeThreshold) {
                            index = if (horizontalDrag < 0f) {
                                (index + 1).coerceAtMost(screenshots.lastIndex)
                            } else {
                                (index - 1).coerceAtLeast(0)
                            }
                        }
                        horizontalDrag = 0f
                    },
                )
                .onPreviewKeyEvent { event ->
                    if (event.type != KeyEventType.KeyUp) return@onPreviewKeyEvent false
                    when (event.key) {
                        Key.DirectionLeft -> {
                            index = (index - 1).coerceAtLeast(0)
                            true
                        }
                        Key.DirectionRight -> {
                            index = (index + 1).coerceAtMost(screenshots.lastIndex)
                            true
                        }
                        else -> false
                    }
                },
            contentAlignment = Alignment.Center,
        ) {
            UrlImage(
                url = optimizedNvidiaImageUrl(screenshots[index], requestWidth),
                modifier = Modifier
                    .fillMaxSize()
                    .padding(12.dp),
                contentScale = ContentScale.Fit,
            )
            if (screenshots.size > 1) {
                Surface(
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .padding(top = 18.dp),
                    shape = CircleShape,
                    color = Color.Black.copy(alpha = 0.64f),
                ) {
                    Text(
                        "${index + 1} / ${screenshots.size}",
                        color = Color.White,
                        style = MaterialTheme.typography.labelLarge,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                    )
                }
            }
            ImageCloseButton(
                onClick = onDismiss,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(18.dp),
            )
        }
    }
}

@Composable
private fun GameDescriptionDisclosure(
    description: String?,
    compact: Boolean,
) {
    var expanded by remember(description) { mutableStateOf(true) }
    val text = description?.takeIf { it.isNotBlank() } ?: stringResource(R.string.catalog_no_description)
    var focused by remember { mutableStateOf(false) }
    val hoverInteraction = remember { MutableInteractionSource() }
    val hovered by hoverInteraction.collectIsHoveredAsState()
    val shape = RoundedCornerShape(if (compact) 12.dp else 14.dp)
    Box(Modifier.fillMaxWidth()) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .onFocusChanged { focused = it.isFocused }
                .hoverable(hoverInteraction)
                .clickable { expanded = !expanded },
            shape = shape,
            color = if (focused) PanelAlt.copy(alpha = 0.85f) else PanelAlt,
            border = if (focused && LocalAbsoluteCinemaEffects.current) {
                BorderStroke(1.dp, LocalActiveSelectionColor.current)
            } else {
                null
            },
            tonalElevation = 0.dp,
        ) {
            Column(Modifier.padding(horizontal = 12.dp, vertical = if (compact) 8.dp else 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        stringResource(R.string.catalog_description),
                        color = TextPrimary,
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = { expanded = !expanded }, modifier = Modifier.size(36.dp)) {
                        Icon(
                            painter = painterResource(R.drawable.ic_chevron_right),
                            contentDescription = if (expanded) {
                                stringResource(R.string.control_hide_description)
                            } else {
                                stringResource(R.string.control_show_description)
                            },
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier
                                .size(20.dp)
                                .graphicsLayer(rotationZ = if (expanded) 90f else 0f),
                        )
                    }
                }
                if (expanded) {
                    Text(
                        text,
                        color = if (description == null) TextMuted else TextPrimary,
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = if (compact) 8 else Int.MAX_VALUE,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        AbsoluteCinemaEverywhereFrame(
            visible = focused || hovered,
            cornerRadius = if (compact) 12.dp else 14.dp,
        )
    }
}

private fun formatGameMetadataLabel(raw: String): String {
    val compact = raw.trim()
        .removePrefix("GFN_")
        .removePrefix("GAME_")
        .replace(METADATA_SEPARATOR_RUN, " ")
        .replace(SEARCH_WHITESPACE_RUN, " ")
        .trim()
    if (compact.isBlank()) return ""
    val lower = compact.lowercase(Locale.US)
    return when (lower) {
        "full game" -> "Full game"
        "single player" -> "Single-player"
        "multi player", "multiplayer" -> "Multiplayer"
        "controller", "gamepad" -> "Controller"
        "keyboard mouse", "mouse keyboard" -> "Mouse and keyboard"
        else -> compact.split(" ").joinToString(" ") { word ->
            if (word.length <= 3 && word.all { it.isUpperCase() || it.isDigit() }) {
                word
            } else {
                word.lowercase(Locale.US).replaceFirstChar { char -> char.titlecase(Locale.US) }
            }
        }
    }
}

private fun isNoisyGameTag(label: String): Boolean {
    val normalized = label.trim().lowercase(Locale.US)
    return normalized.isBlank() ||
        normalized == "unknown" ||
        normalized == "gfn" ||
        normalized == "nvidia" ||
        normalized.contains("sku based tag") ||
        normalized.contains("catalog")
}

@Composable
private fun CompactDetailRows(game: GameInfo) {
    val allRows = gameDetailRows(game)
    val rows = (allRows.take(4) + allRows.drop(4).filter { it.actionUrl != null }).distinct()
    if (rows.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        rows.forEach { row ->
            DetailRow(row = row, compact = true)
        }
    }
}

@Composable
private fun DetailRows(game: GameInfo) {
    val rows = gameDetailRows(game)
    if (rows.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        rows.forEach { row ->
            DetailRow(row = row, compact = false)
        }
    }
}

private data class GameDetailRow(
    val label: String,
    val value: String,
    val copyValue: String = value,
    val actionUrl: String? = null,
)

internal data class GameStoreDetail(
    val label: String,
    val url: String?,
)

internal fun validExternalStoreUrl(rawUrl: String?): String? {
    val value = rawUrl?.trim()?.takeIf { it.isNotBlank() } ?: return null
    val uri = runCatching { java.net.URI(value) }.getOrNull() ?: return null
    return value.takeIf {
        uri.scheme.equals("https", ignoreCase = true) && !uri.host.isNullOrBlank()
    }
}

internal fun gameStoreDetails(game: GameInfo): List<GameStoreDetail> {
    val variantDetails = launchableGameVariants(game.variants)
        .map { variant ->
            GameStoreDetail(
                label = gameStoreDisplayName(variant.store),
                url = validExternalStoreUrl(variant.storeUrl),
            )
        }
        .filter { it.label.isNotBlank() }
        .distinctBy { normalizeGameStore(it.label) }
    if (variantDetails.none { it.url != null }) {
        return game.availableStores
            .map(::gameStoreDisplayName)
            .distinctBy(::normalizeGameStore)
            .takeIf { it.isNotEmpty() }
            ?.let { stores -> listOf(GameStoreDetail(stores.joinToString(", "), null)) }
            .orEmpty()
    }

    val variantStoreKeys = variantDetails.mapTo(mutableSetOf()) { normalizeGameStore(it.label) }
    val fallbackDetails = game.availableStores
        .map(::gameStoreDisplayName)
        .filter { normalizeGameStore(it) !in variantStoreKeys }
        .distinctBy(::normalizeGameStore)
        .map { GameStoreDetail(it, null) }
    return variantDetails + fallbackDetails
}

private fun gameDetailRows(game: GameInfo): List<GameDetailRow> = buildList {
    addAll(
        listOfNotNull(
            game.playabilityState?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Status", formatGameMetadataLabel(it)) },
            game.publisherName?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Publisher", it) },
            game.playType?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Play type", formatGameMetadataLabel(it)) },
            supportedControlLabels(game).takeIf { it.isNotEmpty() }?.joinToString(", ")?.let { GameDetailRow("Controls", it) },
            game.featureLabels
                .map(::formatGameMetadataLabel)
                .filterNot(::isNoisyGameTag)
                .filterNot { feature -> game.genres.any { genre -> feature.equals(formatGameMetadataLabel(genre), ignoreCase = true) } }
                .distinctBy { it.lowercase(Locale.US) }
                .take(8)
                .takeIf { it.isNotEmpty() }
                ?.joinToString(", ")
                ?.let { GameDetailRow("Features", it) },
            game.membershipTierLabel?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Membership", formatGameMetadataLabel(it)) },
            game.contentRatings.takeIf { it.isNotEmpty() }?.joinToString(", ")?.let { GameDetailRow("Rating", it) },
            game.lastPlayed?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Last played", it) },
        ),
    )
    val stores = gameStoreDetails(game)
    addAll(stores.map { store ->
        GameDetailRow(
            label = if (store.url == null && stores.size == 1) "Stores" else "Store",
            value = store.label,
            actionUrl = store.url,
        )
    })
    gameAppIdForDetails(game)?.let { add(GameDetailRow("App ID", it)) }
}

internal fun supportedControlLabels(game: GameInfo): List<String> =
    game.variants
        .flatMap { it.supportedControls }
        .map(::formatGameMetadataLabel)
        .filter(String::isNotBlank)
        .distinctBy { it.lowercase(Locale.US) }

private fun gameAppIdForDetails(game: GameInfo): String? =
    game.launchAppId?.takeIf { it.isNotBlank() }
        ?: game.variants.firstNotNullOfOrNull { variant -> variant.id.takeIf { it.isNotBlank() && it.all(Char::isDigit) } }
        ?: game.uuid?.takeIf { it.isNotBlank() }
        ?: game.id.takeIf { it.isNotBlank() }

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DetailRow(row: GameDetailRow, compact: Boolean) {
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    val shape = RoundedCornerShape(if (compact) 10.dp else 12.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(PanelAlt)
            .combinedClickable(
                role = if (row.actionUrl != null) Role.Button else null,
                onClick = {
                    val url = row.actionUrl ?: return@combinedClickable
                    runCatching {
                        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
                    }.onFailure {
                        Toast.makeText(context, context.getString(R.string.error_open_store_page), Toast.LENGTH_SHORT).show()
                    }
                },
                onLongClick = {
                    clipboard.setText(AnnotatedString(row.copyValue))
                    Toast.makeText(context, "${row.label} copied", Toast.LENGTH_SHORT).show()
                },
            )
            .padding(horizontal = if (compact) 10.dp else 12.dp, vertical = if (compact) 7.dp else 10.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
    ) {
        Text(
            row.label,
            color = TextMuted,
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.width(if (compact) 82.dp else 92.dp),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            row.value,
            color = TextPrimary,
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.weight(1f),
            maxLines = if (compact) 1 else 2,
            overflow = TextOverflow.Ellipsis,
        )
        if (row.actionUrl != null) {
            Icon(
                imageVector = Icons.AutoMirrored.Outlined.OpenInNew,
                contentDescription = "Open ${row.value} store page",
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(if (compact) 17.dp else 19.dp),
            )
        }
    }
}

private val SEARCH_WHITESPACE_RUN = Regex("\\s+")
private val METADATA_SEPARATOR_RUN = Regex("[_-]+")

/**
 * Splits a raw search box value into the terms [gameMatchesSearch] tests against.
 *
 * Filtering runs this once per query rather than once per game: the old shape compiled a fresh
 * `Regex` and re-split the query inside the per-game predicate, so a keystroke over a large
 * library paid for both several thousand times.
 */
internal fun searchTermsFor(query: String): List<String> {
    val normalized = query.trim().lowercase()
    if (normalized.isEmpty()) return emptyList()
    return normalized.split(SEARCH_WHITESPACE_RUN)
}

internal fun gameMatchesSearch(game: GameInfo, terms: List<String>): Boolean {
    if (terms.isEmpty()) return true
    val haystack = buildString {
        append(game.title).append(' ')
        append(game.description.orEmpty()).append(' ')
        append(game.longDescription.orEmpty()).append(' ')
        append(game.publisherName.orEmpty()).append(' ')
        append(game.genres.joinToString(" ")).append(' ')
        append(game.featureLabels.joinToString(" ")).append(' ')
        append(displayStoresForGame(game))
    }.lowercase()
    return terms.all { it in haystack }
}

internal fun gameMatchesSearch(game: GameInfo, query: String): Boolean =
    gameMatchesSearch(game, searchTermsFor(query))

internal fun favoriteOrderedGames(games: List<GameInfo>, favoriteIds: List<String>): List<GameInfo> {
    val favorites = games.filter { it.id in favoriteIds }
    return if (favorites.isNotEmpty()) favorites + games.filterNot { it.id in favoriteIds } else games
}

@Composable
@OptIn(ExperimentalLayoutApi::class)
internal fun StoreLaunchSelector(
    game: GameInfo,
    defaultVariantId: String?,
    onLaunch: (GameInfo, GameVariant) -> Unit,
    onSetDefaultStore: (String, String?) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val variants = remember(game) { launchableGameVariants(game.variants) }
    val context = LocalContext.current
    val initialVariantId = remember(game.id, defaultVariantId, variants) {
        defaultVariantId?.takeIf { savedId -> variants.any { it.id == savedId } }
            ?: variants.firstOrNull()?.id
    }
    var selectedVariantId by remember(game.id, initialVariantId) { mutableStateOf(initialVariantId) }
    var rememberDefaultStore by remember(game.id, defaultVariantId) { mutableStateOf(defaultVariantId != null) }
    val selectedVariant = variants.firstOrNull { it.id == selectedVariantId }
    val continueFocusRequester = remember(game.id) { FocusRequester() }
    BackHandler(onBack = onDismiss)
    LaunchedEffect(game.id, variants.size) {
        if (variants.isNotEmpty()) {
            requestFocusWithRetry(continueFocusRequester)
        }
    }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .lockedFocusGroup()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
    ) {
        val phoneLandscape = isPhoneLandscape(maxWidth, maxHeight)
        val landscape = maxWidth > maxHeight
        Box(
            Modifier.fillMaxSize(),
            contentAlignment = if (phoneLandscape) Alignment.CenterEnd else Alignment.Center,
        ) {
            Card(
                modifier = modifier
                    .then(
                        if (phoneLandscape) {
                            Modifier
                                .padding(end = 12.dp)
                                .fillMaxWidth(0.9f)
                                .fillMaxHeight(0.9f)
                        } else {
                            Modifier
                                .fillMaxWidth(if (landscape) 0.78f else 0.92f)
                                .fillMaxHeight(if (landscape) 0.86f else 0.64f)
                        },
                    ),
                colors = CardDefaults.cardColors(containerColor = Panel, contentColor = TextPrimary),
                shape = RoundedCornerShape(22.dp),
            ) {
                if (phoneLandscape) {
                    Row(
                        Modifier.fillMaxSize().padding(14.dp),
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        LaunchGameSummary(
                            game = game,
                            subtitle = stringResource(R.string.store_selector_choose_launcher),
                            modifier = Modifier
                                .width(190.dp)
                                .fillMaxHeight(),
                        )
                        StoreLaunchOptionsColumn(
                            variants = variants,
                            selectedVariantId = selectedVariantId,
                            defaultVariantId = defaultVariantId,
                            rememberDefaultStore = rememberDefaultStore,
                            selectedVariant = selectedVariant,
                            continueFocusRequester = continueFocusRequester,
                            onSelectVariant = { selectedVariantId = it },
                            onRememberDefaultStoreChange = { rememberDefaultStore = it },
                            onDismiss = onDismiss,
                            onContinue = { variant ->
                                if (rememberDefaultStore || defaultVariantId != null) {
                                    onSetDefaultStore(game.id, if (rememberDefaultStore) variant.id else null)
                                }
                                if (rememberDefaultStore) {
                                    Toast.makeText(context, context.getString(R.string.store_selector_long_press_tip), Toast.LENGTH_LONG).show()
                                }
                                onLaunch(game, variant)
                            },
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxHeight(),
                        )
                    }
                } else {
                    Column(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            UrlImage(
                                game.imageUrl,
                                Modifier
                                    .width(58.dp)
                                    .height(76.dp)
                                    .clip(RoundedCornerShape(12.dp)),
                            )
                            Spacer(Modifier.width(12.dp))
                            Column(Modifier.weight(1f)) {
                                Text(game.title, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text(stringResource(R.string.store_selector_choose_launcher), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                        StoreLaunchOptionsColumn(
                            variants = variants,
                            selectedVariantId = selectedVariantId,
                            defaultVariantId = defaultVariantId,
                            rememberDefaultStore = rememberDefaultStore,
                            selectedVariant = selectedVariant,
                            continueFocusRequester = continueFocusRequester,
                            onSelectVariant = { selectedVariantId = it },
                            onRememberDefaultStoreChange = { rememberDefaultStore = it },
                            onDismiss = onDismiss,
                            onContinue = { variant ->
                                if (rememberDefaultStore || defaultVariantId != null) {
                                    onSetDefaultStore(game.id, if (rememberDefaultStore) variant.id else null)
                                }
                                if (rememberDefaultStore) {
                                    Toast.makeText(context, context.getString(R.string.store_selector_long_press_tip), Toast.LENGTH_LONG).show()
                                }
                                onLaunch(game, variant)
                            },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun LaunchGameSummary(game: GameInfo, subtitle: String, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        UrlImage(
            game.imageUrl,
            Modifier
                .fillMaxWidth()
                .weight(1f)
                .clip(RoundedCornerShape(16.dp)),
        )
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(game.title, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(subtitle, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun StoreLaunchOptionsColumn(
    variants: List<GameVariant>,
    selectedVariantId: String?,
    defaultVariantId: String?,
    rememberDefaultStore: Boolean,
    selectedVariant: GameVariant?,
    continueFocusRequester: FocusRequester,
    onSelectVariant: (String) -> Unit,
    onRememberDefaultStoreChange: (Boolean) -> Unit,
    onDismiss: () -> Unit,
    onContinue: (GameVariant) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LazyColumn(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(variants, key = { it.id }) { variant ->
                StoreLaunchVariantRow(
                    variant = variant,
                    selected = variant.id == selectedVariantId,
                    savedDefault = variant.id == defaultVariantId,
                    onClick = { onSelectVariant(variant.id) },
                )
            }
        }
        var checkFocused by remember { mutableStateOf(false) }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .onFocusChanged { checkFocused = it.isFocused }
                .background(if (checkFocused) Color.White.copy(alpha = 0.08f) else Color.Transparent)
                .border(
                    width = 1.dp,
                    color = if (checkFocused) MaterialTheme.colorScheme.primary else Color.Transparent,
                    shape = RoundedCornerShape(14.dp)
                )
                .clickable { onRememberDefaultStoreChange(!rememberDefaultStore) }
                .padding(vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Checkbox(
                checked = rememberDefaultStore,
                onCheckedChange = onRememberDefaultStoreChange,
            )
            Text(
                stringResource(R.string.store_selector_default_checkbox),
                color = TextPrimary,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.weight(1f),
            )
        }
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedButton(onClick = onDismiss, modifier = Modifier.weight(1f)) {
                Text(stringResource(R.string.action_cancel))
            }
            Button(
                onClick = {
                    val variant = selectedVariant ?: return@Button
                    onContinue(variant)
                },
                enabled = selectedVariant != null,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(continueFocusRequester),
            ) {
                Text(stringResource(R.string.action_continue), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun StoreLaunchVariantRow(
    variant: GameVariant,
    selected: Boolean,
    savedDefault: Boolean,
    onClick: () -> Unit,
) {
    val badge = launcherBadgeForStoreKey(splitGameStoreKeys(variant.store).firstOrNull())
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(14.dp)
    Box(Modifier.fillMaxWidth()) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .onFocusChanged { focused = it.isFocused }
                .border(
                    width = if (focused || selected) 2.dp else 1.dp,
                    color = cinemaBorderColor(
                        LocalAbsoluteCinemaEffects.current,
                        LocalActiveSelectionColor.current,
                    ),
                    shape = shape,
                )
                .clickable { onClick() },
            shape = shape,
            color = if (focused) Color.White.copy(alpha = 0.12f) else if (selected) LocalSelectionTintColor.current.copy(alpha = 0.18f) else PanelAlt,
            contentColor = TextPrimary,
        ) {
            Row(
                Modifier.padding(14.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                ConnectorStoreIcon(badge)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(
                        gameStoreDisplayName(variant.store),
                        fontWeight = if (selected) FontWeight.ExtraBold else FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    val details = listOf(
                        if (savedDefault) stringResource(R.string.store_selector_default) else "",
                        variantDetailsText(variant),
                    ).filter { it.isNotBlank() }.joinToString(" - ")
                    if (details.isNotBlank()) {
                        Text(details, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
                if (selected) {
                    Text(
                        stringResource(R.string.store_selector_selected),
                        color = LocalSelectionTintColor.current,
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.ExtraBold,
                        maxLines = 1,
                    )
                }
            }
        }
        ControllerFocusFrame(
            visible = selected && LocalActiveSelectionEnabled.current,
            cornerRadius = 14.dp,
            tint = LocalActiveSelectionColor.current,
            secondaryTint = LocalActiveSelectionSecondaryColor.current,
        )
    }
}
