package com.opencloudgaming.opennow

import android.view.KeyEvent
import androidx.annotation.StringRes
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Backspace
import androidx.compose.material.icons.automirrored.rounded.KeyboardReturn
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.DarkMode
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.QueryStats
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.ShoppingBag
import androidx.compose.material.icons.rounded.Star
import androidx.compose.material.icons.rounded.StarBorder
import androidx.compose.material.icons.rounded.TouchApp
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.TextButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.numeric
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt

// OpenNOW V3 tokens, as drawn on the AYN Tour boards.
private val DeckBackground = OpenNowPalette.Background
private val DeckTile = Color.White.copy(alpha = 0.06f)
private val DeckTileStrong = Color.White.copy(alpha = 0.12f)
private val DeckSeam = OpenNowPalette.Seam
private val DeckMuted = Color.White.copy(alpha = 0.6f)
/** The user's accent, so the deck follows the same theme as the top screen. */
private val DeckAccent: Color
    @Composable @ReadOnlyComposable get() = MaterialTheme.colorScheme.primary
private val DeckMint = OpenNowPalette.PastelMint
private val DeckYellow = OpenNowPalette.PastelYellow
private val DeckCoral = OpenNowPalette.PastelCoral
private val DeckViolet = OpenNowPalette.PastelViolet

private const val HOLD_TO_END_MS = 1_200
private const val HOLD_TO_CHOOSE_STORE_MS = 650
private const val TRACKPAD_TAP_MAX_MS = 250L
private const val TRACKPAD_WHEEL_STEP_PX = 36f
private const val TRACKPAD_WHEEL_DELTA = 120
private const val PING_HISTORY_SAMPLES = 60

private enum class StreamPanel { Deck, Trackpad, Keyboard, Off }

@Composable
fun DualScreenDeck(onAction: (DualScreenAction) -> Unit) {
    val snapshot by DualScreenBridge.snapshot.collectAsState()
    val current = snapshot
    OpenNowTheme(settings = current?.settings ?: AppSettings(), physicalControllerConnected = false) {
        val hosted = DualScreenBridge.hostedContent.value
        if (hosted != null && current != null) {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density * hosted.scale, density.fontScale)) {
                Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
                    hosted.content()
                }
            }
            return@OpenNowTheme
        }
        BoxWithConstraints(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
            val settings = current?.settings ?: AppSettings()
            val browsing = current?.phase != DualScreenPhase.Streaming && current?.phase != DualScreenPhase.Launching
            // Same backdrop choice as the top screen: the user's wallpaper, else ambient, else an accent glow.
            when {
                browsing && current != null && shouldShowAppWallpaper(current.page, inStream = false, settings) -> {
                    CatalogWallpaperBackdrop(settings, tvProfile = false, width = maxWidth, height = maxHeight)
                    Box(Modifier.matchParentSize().background(DeckBackground.copy(alpha = 0.55f)))
                }
                browsing && settings.ambientBackgroundEnabled -> AmbientBackground()
                else -> Box(
                    Modifier.matchParentSize().background(
                        Brush.radialGradient(
                            listOf(DeckAccent.copy(alpha = 0.16f), Color.Transparent),
                            center = Offset(600f, 0f),
                            radius = 900f,
                        ),
                    ),
                )
            }
            Box(Modifier.fillMaxSize().padding(12.dp)) {
                when (current?.phase) {
                    null, DualScreenPhase.SignedOut -> SignedOutDeck(onAction)
                    DualScreenPhase.Browse -> if (current.page == AppPage.Settings) {
                        SettingsInspectorDeck(current, onAction)
                    } else {
                        HomeDeck(current, onAction)
                    }
                    DualScreenPhase.Launching -> LaunchingDeck(current, onAction)
                    DualScreenPhase.Streaming -> StreamingDeck(current, onAction)
                }
            }
        }
    }
}

// Signed out (board 30) ----------------------------------------------------------------------

@Composable
private fun SignedOutDeck(onAction: (DualScreenAction) -> Unit) {
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        DeckText(stringResource(R.string.dual_signed_out_title), 24.sp, FontWeight.Black)
        DeckText(stringResource(R.string.dual_signed_out_body), 13.sp, FontWeight.SemiBold, DeckMuted)
        Spacer(Modifier.weight(1f))
        PillButton(stringResource(R.string.dual_sign_in_nvidia), primary = true, height = 60.dp, modifier = Modifier.fillMaxWidth()) {
            onAction(DualScreenAction.SignIn)
        }
        PillButton(stringResource(R.string.dual_sign_in_other_device), height = 52.dp, modifier = Modifier.fillMaxWidth()) {
            onAction(DualScreenAction.SignInWithCode)
        }
    }
}

// Home: search, Up next, your rig, preset, tabs (board 01) -----------------------------------

@Composable
private fun HomeDeck(snapshot: DualScreenSnapshot, onAction: (DualScreenAction) -> Unit) {
    var typing by remember { mutableStateOf(false) }
    val libraryPage = snapshot.page == AppPage.Library
    val publishedQuery = if (libraryPage) snapshot.librarySearch else snapshot.catalogSearch
    // Keys can land faster than the ViewModel round trip; the draft is the source while typing.
    var draft by remember { mutableStateOf(publishedQuery) }
    if (!typing && draft != publishedQuery) draft = publishedQuery
    val query = if (typing) draft else publishedQuery
    val search = { next: String ->
        draft = next
        onAction(DualScreenAction.Search(next))
    }
    var sortFilterOpen by remember(snapshot.page) { mutableStateOf(false) }
    val activeFilters = if (libraryPage) snapshot.libraryFilterIds.size else snapshot.catalogFilterIds.size
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            SearchField(
                query = query,
                hint = stringResource(if (libraryPage) R.string.dual_search_library_hint else R.string.dual_search_store_hint),
                active = typing,
                onTap = {
                    sortFilterOpen = false
                    typing = !typing
                },
                onClear = { search("") },
                modifier = Modifier.weight(1f),
            )
            SortFilterButton(activeCount = activeFilters, open = sortFilterOpen) {
                typing = false
                sortFilterOpen = !sortFilterOpen
            }
        }
        if (sortFilterOpen) {
            SortFilterPanel(snapshot, Modifier.weight(1f), onAction)
            DeckTabBar(snapshot.page, onSearch = { typing = true }, onAction = onAction)
            return@Column
        }
        if (typing) {
            DeckKeyboard(
                modifier = Modifier.weight(1f),
                onText = { search(draft + it) },
                onKey = { keyCode ->
                    when (keyCode) {
                        KeyEvent.KEYCODE_DEL -> search(draft.dropLast(1))
                        KeyEvent.KEYCODE_ENTER -> typing = false
                    }
                },
                enterLabel = stringResource(R.string.dual_search_done),
            )
            return@Column
        }
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Column(Modifier.weight(1.45f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                SectionLabel(stringResource(R.string.dual_up_next))
                if (snapshot.upNext.isEmpty()) {
                    DeckText(stringResource(R.string.dual_up_next_empty), 12.sp, FontWeight.SemiBold, DeckMuted, modifier = Modifier.weight(1f))
                } else {
                    snapshot.upNext.forEachIndexed { index, game ->
                        UpNextRow(game, highlighted = index == 0, onOpen = { onAction(DualScreenAction.OpenGame(game)) }) {
                            onAction(DualScreenAction.Play(game))
                        }
                    }
                    Spacer(Modifier.weight(1f))
                }
                PresetSelector(snapshot.streamPreset, onAction)
            }
            RigCard(snapshot, Modifier.weight(1f).fillMaxHeight())
        }
        DeckTabBar(snapshot.page, onSearch = { typing = true }, onAction = onAction)
    }
}

// Sort and filter (board 06): the whole surface lives here so the top screen stays art only. --

@Composable
private fun SortFilterButton(activeCount: Int, open: Boolean, onClick: () -> Unit) {
    val label = if (activeCount == 0) {
        stringResource(R.string.catalog_sort_filter)
    } else {
        stringResource(R.string.catalog_sort_filter_active, activeCount)
    }
    Box(
        Modifier
            .size(44.dp)
            .clip(CircleShape)
            .background(if (open) Color.White else Color.White.copy(alpha = 0.08f))
            .border(1.dp, if (open) Color.Transparent else DeckSeam, CircleShape)
            .semantics { contentDescription = label }
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_sort_filter),
            contentDescription = null,
            tint = if (open) DeckBackground else Color.White,
            modifier = Modifier.size(20.dp),
        )
        if (activeCount > 0) {
            Box(
                Modifier
                    .align(Alignment.TopEnd)
                    .size(16.dp)
                    .clip(CircleShape)
                    .background(DeckAccent),
                contentAlignment = Alignment.Center,
            ) {
                DeckText(activeCount.toString(), 9.sp, FontWeight.Black, DeckBackground)
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SortFilterPanel(snapshot: DualScreenSnapshot, modifier: Modifier, onAction: (DualScreenAction) -> Unit) {
    val libraryPage = snapshot.page == AppPage.Library
    val sortOptions = if (libraryPage) {
        librarySortOptions()
    } else {
        snapshot.catalogSortOptions.distinctBy { it.id }.ifEmpty {
            listOf(CatalogSortOption(DEFAULT_CATALOG_SORT_ID, stringResource(R.string.catalog_sort_popular), ""))
        }
    }
    val selectedSort = if (libraryPage) snapshot.librarySortId else snapshot.catalogSortId
    val selectedFilters = if (libraryPage) snapshot.libraryFilterIds else snapshot.catalogFilterIds
    val touchLabel = stringResource(R.string.catalog_filter_touch_controls)
    val filterOptions = if (libraryPage) {
        remember(snapshot.libraryGames, touchLabel) { libraryStoreFilterOptions(snapshot.libraryGames, touchLabel) }
    } else {
        rememberCatalogFilterOptions(snapshot.catalogFilterGroups)
    }
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(DeckTile)
            .border(1.dp, DeckSeam, RoundedCornerShape(20.dp))
            .verticalScroll(rememberScrollState())
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            DeckText(stringResource(R.string.catalog_sort_filter), 18.sp, FontWeight.Black, modifier = Modifier.weight(1f))
            if (selectedFilters.isNotEmpty()) {
                SmallPill(stringResource(R.string.dual_filters_reset, selectedFilters.size)) {
                    onAction(DualScreenAction.ClearFilters)
                }
            }
        }
        SectionLabel(stringResource(R.string.catalog_sort_section))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            sortOptions.forEach { option ->
                DeckChip(
                    label = catalogSortDisplayLabel(option.id, option.label),
                    selected = option.id == selectedSort,
                    selectedColor = Color.White,
                ) {
                    onAction(DualScreenAction.SetSort(option.id))
                }
            }
        }
        filterOptions.groupBy { it.groupLabel }.forEach { (group, options) ->
            SectionLabel("${stringResource(R.string.catalog_filter_section)} · $group")
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                options.forEach { option ->
                    DeckChip(label = option.label, selected = option.id in selectedFilters, selectedColor = DeckAccent, check = true) {
                        onAction(DualScreenAction.ToggleFilter(option.id))
                    }
                }
            }
        }
    }
}

@Composable
private fun DeckChip(label: String, selected: Boolean, selectedColor: Color, check: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier
            .height(38.dp)
            .clip(CircleShape)
            .background(if (selected) selectedColor else Color.White.copy(alpha = 0.06f))
            .border(1.dp, if (selected) Color.Transparent else DeckSeam, CircleShape)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (selected && check) {
            Icon(Icons.Rounded.Check, contentDescription = null, tint = DeckBackground, modifier = Modifier.size(14.dp))
            Spacer(Modifier.width(4.dp))
        }
        DeckText(label, 13.sp, FontWeight.Black, if (selected) DeckBackground else Color.White, maxLines = 1)
    }
}

@Composable
private fun UpNextRow(game: GameInfo, highlighted: Boolean, onOpen: () -> Unit, onPlay: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(50.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(if (highlighted) DeckTileStrong else DeckTile)
            .border(1.dp, if (highlighted) Color.White.copy(alpha = 0.22f) else DeckSeam, RoundedCornerShape(16.dp))
            .clickable(onClick = onOpen)
            .padding(horizontal = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        GameArt(game, Modifier.size(38.dp), radius = 11.dp)
        Column(Modifier.weight(1f)) {
            DeckText(game.title, 13.sp, FontWeight.Black, maxLines = 1)
            game.lastPlayed?.let { DeckText(it.take(10), 11.sp, FontWeight.Bold, DeckMuted, maxLines = 1) }
        }
        Box(
            Modifier
                .size(32.dp)
                .clip(CircleShape)
                .background(if (highlighted) Color.White else DeckTileStrong)
                .clickable(onClick = onPlay),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.PlayArrow, null, tint = if (highlighted) DeckBackground else Color.White, modifier = Modifier.size(18.dp))
        }
    }
}

@Composable
private fun RigCard(snapshot: DualScreenSnapshot, modifier: Modifier) {
    Column(
        modifier
            .clip(RoundedCornerShape(20.dp))
            .background(Color.White.copy(alpha = 0.04f))
            .border(1.dp, DeckSeam, RoundedCornerShape(20.dp))
            .padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SectionLabel(stringResource(R.string.dual_your_rig))
        val name = snapshot.accountName.orEmpty()
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(
                Modifier.size(30.dp).clip(CircleShape).background(DeckViolet).border(2.dp, Color.White, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                DeckText(name.take(1).uppercase(Locale.getDefault()), 13.sp, FontWeight.Black, DeckBackground)
            }
            DeckText(name, 14.sp, FontWeight.Black, maxLines = 1)
        }
        RigLine(DeckYellow, stringResource(R.string.dual_rig_plan), membershipTierLabel(snapshot.membershipTier), hoursLabel(snapshot))
        RigLine(
            DeckMint,
            stringResource(R.string.dual_rig_region),
            snapshot.regionName ?: stringResource(R.string.dual_region_auto),
            snapshot.regionPingMs?.let { stringResource(R.string.dual_ping_ms, it.toInt()) },
        )
    }
}

@Composable
private fun RigLine(dot: Color, label: String, value: String, detail: String?) {
    Column(Modifier.fillMaxWidth().padding(top = 2.dp), verticalArrangement = Arrangement.spacedBy(1.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(dot))
            DeckText(label.uppercase(Locale.getDefault()), 9.sp, FontWeight.Black, DeckMuted)
        }
        DeckText(value, 14.sp, FontWeight.Black, maxLines = 1)
        detail?.let { DeckText(it, 11.sp, FontWeight.Bold, DeckMuted, maxLines = 1) }
    }
}

@Composable
private fun hoursLabel(snapshot: DualScreenSnapshot): String? = when {
    snapshot.unlimitedHours -> stringResource(R.string.dual_unlimited)
    snapshot.remainingHours != null -> stringResource(
        R.string.dual_hours_left,
        String.format(Locale.getDefault(), "%.1f", snapshot.remainingHours),
    )
    else -> null
}

@Composable
private fun SearchField(
    query: String,
    hint: String,
    active: Boolean,
    onTap: () -> Unit,
    onClear: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier
            .height(44.dp)
            .clip(CircleShape)
            .background(Color.White.copy(alpha = if (active) 0.10f else 0.08f))
            .border(if (active) 2.dp else 1.dp, if (active) DeckAccent else DeckSeam, CircleShape)
            .clickable(onClick = onTap)
            .padding(start = 14.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Rounded.Search, contentDescription = null, tint = Color.White, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(10.dp))
        DeckText(
            text = query.ifEmpty { hint },
            size = 15.sp,
            weight = FontWeight.Black,
            color = if (query.isEmpty()) DeckMuted else Color.White,
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        if (query.isNotEmpty()) {
            SmallPill(stringResource(R.string.dual_search_clear), onClick = onClear)
        } else {
            ButtonGlyph("Y", dark = false, size = 30.dp)
        }
    }
}

@Composable
private fun PresetSelector(current: StreamPreset, onAction: (DualScreenAction) -> Unit) {
    val presets = listOf(
        StreamPreset.LowDataSaver to R.string.dual_preset_data_saver,
        StreamPreset.Recommended to R.string.dual_preset_auto,
        StreamPreset.High to R.string.dual_preset_high,
    )
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SectionLabel(stringResource(R.string.dual_stream_preset))
        Segmented(
            options = presets.map { stringResource(it.second) },
            selected = presets.indexOfFirst { it.first == current },
            height = 36.dp,
            onSelect = { onAction(DualScreenAction.ApplyPreset(presets[it].first)) },
        )
    }
}

@Composable
private fun DeckTabBar(page: AppPage, onSearch: () -> Unit, onAction: (DualScreenAction) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(58.dp)
            .clip(CircleShape)
            .background(OpenNowPalette.GlassStrong)
            .border(1.dp, DeckSeam, CircleShape)
            .padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        DeckTab(page == AppPage.Home, Icons.Rounded.ShoppingBag, DeckYellow, R.string.nav_store) {
            onAction(DualScreenAction.Navigate(AppPage.Home))
        }
        DeckTab(false, Icons.Rounded.Search, DeckViolet, R.string.nav_search, onClick = onSearch)
        DeckTab(page == AppPage.Library, Icons.Rounded.GridView, OpenNowPalette.PastelSky, R.string.nav_library) {
            onAction(DualScreenAction.Navigate(AppPage.Library))
        }
        DeckTab(page == AppPage.Settings, Icons.Rounded.Settings, DeckCoral, R.string.nav_settings) {
            onAction(DualScreenAction.Navigate(AppPage.Settings))
        }
    }
}

@Composable
private fun RowScope.DeckTab(selected: Boolean, icon: ImageVector, tint: Color, @StringRes label: Int, onClick: () -> Unit) {
    Column(
        Modifier
            .weight(1f)
            .fillMaxHeight()
            .clip(CircleShape)
            .background(if (selected) DeckAccent.copy(alpha = 0.22f) else Color.Transparent)
            .clickable(onClick = onClick),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(22.dp))
        DeckText(stringResource(label), 10.sp, FontWeight.Black, if (selected) Color.White else DeckMuted)
    }
}

/** Tap plays; holding fills the button and opens the store chooser, like the details sheet. */
@Composable
private fun HoldPlayButton(label: String, hint: String?, onTap: () -> Unit, onHeld: () -> Unit) {
    val holdFill = DeckAccent.copy(alpha = 0.55f)
    val progress = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    Box(
        Modifier
            .fillMaxWidth()
            .height(66.dp)
            .clip(RoundedCornerShape(22.dp))
            .background(Color.White)
            .drawBehind { drawRect(holdFill, size = size.copy(width = size.width * progress.value)) }
            .pointerInput(hint) {
                detectTapGestures(
                    onPress = {
                        if (hint == null) {
                            if (tryAwaitRelease()) onTap()
                            return@detectTapGestures
                        }
                        var held = false
                        val hold = scope.launch {
                            progress.animateTo(1f, tween(HOLD_TO_CHOOSE_STORE_MS, easing = LinearEasing))
                            held = true
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            onHeld()
                        }
                        val released = tryAwaitRelease()
                        hold.cancel()
                        scope.launch { progress.snapTo(0f) }
                        if (released && !held) onTap()
                    },
                )
            },
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ButtonGlyph("A", dark = true)
                DeckText(label, 20.sp, FontWeight.Black, DeckBackground)
            }
            hint?.let { DeckText(it, 10.sp, FontWeight.Bold, DeckBackground.copy(alpha = 0.6f)) }
        }
    }
}

// Game details controls (board 08): the top screen is the poster, this is the control panel. ---

/** Hosted on the bottom screen by the top screen's details overlay, which owns every callback. */
@Composable
internal fun DualScreenGameDetailsPanel(
    game: GameInfo,
    favorite: Boolean,
    playStore: String?,
    details: List<Pair<String, String>>,
    connectedTvName: String?,
    removeLabel: String?,
    removeConfirmationText: String?,
    removeEnabled: Boolean,
    onPlay: () -> Unit,
    onChooseStore: () -> Unit,
    onFavorite: () -> Unit,
    onPlayOnTv: () -> Unit,
    onRemove: (() -> Unit)?,
    onClose: () -> Unit,
) {
    val multipleStores = launchableGameVariants(game.variants).map { it.store }.distinct().size > 1
    var confirmRemove by remember(game.id) { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        HoldPlayButton(
            label = playStore?.let { stringResource(R.string.dual_play_on_store, it) } ?: stringResource(R.string.dual_play),
            hint = stringResource(R.string.dual_play_hold_hint).takeIf { multipleStores },
            onTap = onPlay,
            onHeld = onChooseStore,
        )
        Row(Modifier.height(46.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            PillButton(
                stringResource(if (favorite) R.string.dual_favorited else R.string.dual_favorite),
                icon = if (favorite) Icons.Rounded.Star else Icons.Rounded.StarBorder,
                iconTint = DeckYellow,
                modifier = Modifier.weight(1f),
                onClick = onFavorite,
            )
            if (connectedTvName != null) {
                PillButton(stringResource(R.string.action_play_on_tv_generic), modifier = Modifier.weight(1.4f), onClick = onPlayOnTv)
            }
            if (removeLabel != null && onRemove != null) {
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .clip(CircleShape)
                        .border(2.dp, DeckCoral, CircleShape)
                        .semantics { contentDescription = removeLabel }
                        .clickable(enabled = removeEnabled) { confirmRemove = true }
                        .padding(horizontal = 12.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    DeckText(stringResource(R.string.library_remove_local_app_confirm), 14.sp, FontWeight.Black, DeckCoral, maxLines = 1)
                }
            }
            Box(
                Modifier
                    .size(46.dp)
                    .clip(CircleShape)
                    .background(Color.White.copy(alpha = 0.08f))
                    .border(1.dp, DeckSeam, CircleShape)
                    .clickable(onClick = onClose),
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Rounded.Close, contentDescription = stringResource(R.string.dual_close), tint = Color.White, modifier = Modifier.size(20.dp))
            }
        }
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .clip(RoundedCornerShape(20.dp))
                .background(Color.White.copy(alpha = 0.04f))
                .border(1.dp, DeckSeam, RoundedCornerShape(20.dp))
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 14.dp, vertical = 8.dp),
        ) {
            SectionLabel(stringResource(R.string.dual_details), Modifier.padding(vertical = 4.dp))
            details.forEach { (label, value) ->
                Row(
                    Modifier.fillMaxWidth().padding(vertical = 7.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    DeckText(label, 12.sp, FontWeight.Bold, DeckMuted)
                    Spacer(Modifier.width(12.dp))
                    DeckText(value, 12.sp, FontWeight.Black, textAlign = TextAlign.End, maxLines = 2)
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(DeckSeam))
            }
        }
    }
    if (confirmRemove && removeLabel != null && onRemove != null) {
        AlertDialog(
            onDismissRequest = { confirmRemove = false },
            title = { Text(stringResource(R.string.store_remove_confirm_title)) },
            text = { Text(removeConfirmationText ?: removeLabel) },
            confirmButton = {
                TextButton(onClick = {
                    confirmRemove = false
                    onRemove()
                }) { Text(stringResource(R.string.library_remove_local_app_confirm)) }
            },
            dismissButton = {
                TextButton(onClick = { confirmRemove = false }) { Text(stringResource(R.string.action_cancel)) }
            },
        )
    }
}

// Store and server pickers (boards 09 and 05): the whole choice lives under the thumbs. ---------

/** Hosted on the bottom screen by the top screen, which owns the selection and the launch. */
@Composable
internal fun DualScreenStorePicker(
    variants: List<GameVariant>,
    selectedVariantId: String?,
    defaultVariantId: String?,
    rememberDefault: Boolean,
    onSelect: (String) -> Unit,
    onRememberDefaultChange: (Boolean) -> Unit,
    onCancel: () -> Unit,
    onPlay: () -> Unit,
) {
    val selected = variants.firstOrNull { it.id == selectedVariantId }
    DeckPickerScaffold(
        confirmLabel = selected?.let { stringResource(R.string.dual_play_on_store, gameStoreDisplayName(it.store)) }
            ?: stringResource(R.string.dual_play),
        confirmEnabled = selected != null,
        footnote = stringResource(R.string.store_selector_long_press_tip),
        onCancel = onCancel,
        onConfirm = onPlay,
        footer = {
            Row(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(18.dp))
                    .background(DeckTile)
                    .clickable { onRememberDefaultChange(!rememberDefault) }
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                DeckToggle(rememberDefault)
                DeckText(stringResource(R.string.store_selector_default_checkbox), 12.sp, FontWeight.Black, maxLines = 1, modifier = Modifier.weight(1f))
            }
        },
    ) {
        SectionLabel(stringResource(R.string.store_selector_launchers))
        variants.forEach { variant ->
            val owned = isOwnedLibraryStatus(variant.libraryStatus)
            DeckPickerRow(selected = variant.id == selectedVariantId, height = 54.dp, onClick = { onSelect(variant.id) }) {
                ConnectorStoreIcon(launcherBadgeForStoreKey(splitGameStoreKeys(variant.store).firstOrNull()))
                Column(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        DeckText(gameStoreDisplayName(variant.store), 16.sp, FontWeight.Black, maxLines = 1)
                        if (variant.id == defaultVariantId) {
                            DeckText(
                                stringResource(R.string.store_selector_default).uppercase(Locale.getDefault()),
                                9.sp,
                                FontWeight.Black,
                                DeckBackground,
                                modifier = Modifier.clip(CircleShape).background(DeckMint).padding(horizontal = 7.dp, vertical = 2.dp),
                            )
                        }
                    }
                    DeckText(
                        stringResource(if (owned) R.string.dual_store_owned else R.string.store_selector_available_launcher),
                        11.sp,
                        FontWeight.Bold,
                        if (owned) DeckMint else DeckMuted,
                    )
                }
                DeckRadio(variant.id == selectedVariantId)
            }
        }
    }
}

@Composable
internal fun DualScreenServerPicker(
    picker: PrintedWastePickerModel,
    loading: Boolean,
    error: String?,
    onRetry: () -> Unit,
    onAuto: () -> Unit,
    onCancel: () -> Unit,
) {
    val selected = picker.locations.firstOrNull { it.primary.zoneId == picker.selectedZoneId }
    DeckPickerScaffold(
        confirmLabel = selected?.let { stringResource(R.string.dual_launch_in, it.title) } ?: stringResource(R.string.action_launch),
        confirmEnabled = !loading && picker.selectedZone != null,
        footnote = null,
        onCancel = onCancel,
        onConfirm = picker.launch,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SectionLabel(stringResource(R.string.dual_choose_server), Modifier.weight(1f))
            DeckText(
                stringResource(R.string.dual_preset_auto),
                12.sp,
                FontWeight.Black,
                DeckAccent,
                modifier = Modifier.clip(CircleShape).clickable(onClick = onAuto).padding(horizontal = 10.dp, vertical = 4.dp),
            )
        }
        when {
            loading -> Column(
                Modifier.fillMaxWidth().padding(vertical = 40.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                CircularProgressIndicator(color = DeckAccent, strokeWidth = 3.dp, modifier = Modifier.size(28.dp))
                DeckText(stringResource(R.string.catalog_checking_queues), 12.sp, FontWeight.Bold, DeckMuted)
            }
            error != null -> Column(
                Modifier.fillMaxWidth().padding(vertical = 30.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                DeckText(error, 13.sp, FontWeight.Bold, DeckCoral, textAlign = TextAlign.Center)
                SmallPill(stringResource(R.string.action_retry), onClick = onRetry)
            }
            else -> picker.regionGroups.forEach { (region, locations) ->
                SectionLabel(region, Modifier.padding(top = 4.dp))
                locations.forEach { location ->
                    val ping = location.primary.pingMs?.toInt()
                    val queue = location.primary.zone.QueuePosition
                    DeckPickerRow(
                        selected = location.primary.zoneId == picker.selectedZoneId,
                        height = 52.dp,
                        onClick = { picker.select(location.primary.zoneId) },
                    ) {
                        SignalBars(ping)
                        DeckText(location.title, 15.sp, FontWeight.Black, maxLines = 1, modifier = Modifier.weight(1f))
                        if (location == picker.autoLocation) {
                            DeckText(
                                stringResource(R.string.dual_best_route).uppercase(Locale.getDefault()),
                                9.sp,
                                FontWeight.Black,
                                DeckBackground,
                                modifier = Modifier.clip(CircleShape).background(DeckMint).padding(horizontal = 7.dp, vertical = 2.dp),
                            )
                        }
                        DeckText(
                            ping?.let { stringResource(R.string.dual_ping_ms, it) } ?: stringResource(R.string.queue_checking),
                            12.sp,
                            FontWeight.Bold,
                            DeckMuted,
                        )
                        DeckText(
                            if (queue <= 0) stringResource(R.string.dual_no_queue) else "#$queue",
                            12.sp,
                            FontWeight.Black,
                            if (queue <= 0) DeckMint else queueColor(queue),
                            textAlign = TextAlign.End,
                            modifier = Modifier.width(64.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun DeckPickerScaffold(
    confirmLabel: String,
    confirmEnabled: Boolean,
    footnote: String?,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    footer: (@Composable () -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(6.dp),
            content = content,
        )
        footer?.invoke()
        Row(Modifier.height(50.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PillButton(stringResource(R.string.action_cancel), modifier = Modifier.weight(0.42f), height = 50.dp, onClick = onCancel)
            Row(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .clip(CircleShape)
                    .background(if (confirmEnabled) Color.White else Color.White.copy(alpha = 0.3f))
                    .clickable(enabled = confirmEnabled, onClick = onConfirm)
                    .padding(horizontal = 14.dp),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ButtonGlyph("A", dark = true)
                Spacer(Modifier.width(8.dp))
                DeckText(confirmLabel, 16.sp, FontWeight.Black, DeckBackground, maxLines = 1)
            }
        }
        footnote?.let {
            DeckText(it, 10.sp, FontWeight.Bold, DeckMuted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun DeckPickerRow(
    selected: Boolean,
    onClick: () -> Unit,
    height: Dp = 64.dp,
    content: @Composable RowScope.() -> Unit,
) {
    val shape = RoundedCornerShape(20.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .height(height)
            .clip(shape)
            .background(if (selected) DeckTileStrong else DeckTile)
            .border(if (selected) 2.dp else 1.dp, if (selected) DeckAccent else DeckSeam, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        content = content,
    )
}

@Composable
private fun DeckRadio(selected: Boolean) {
    Box(
        Modifier
            .size(22.dp)
            .clip(CircleShape)
            .border(2.dp, if (selected) DeckAccent else DeckMuted, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        if (selected) Box(Modifier.size(12.dp).clip(CircleShape).background(DeckAccent))
    }
}

@Composable
private fun DeckToggle(on: Boolean) {
    Box(
        Modifier
            .width(44.dp)
            .height(26.dp)
            .clip(CircleShape)
            .background(if (on) DeckMint else Color.White.copy(alpha = 0.18f))
            .padding(3.dp),
        contentAlignment = if (on) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Box(Modifier.size(20.dp).clip(CircleShape).background(if (on) DeckBackground else Color.White))
    }
}

/** Four bars, lit by ping, like board 05's region list. */
@Composable
private fun SignalBars(ping: Int?) {
    val lit = when {
        ping == null -> 0
        ping <= 30 -> 4
        ping <= 60 -> 3
        ping <= 100 -> 2
        else -> 1
    }
    val color = pingColor(ping)
    Row(Modifier.height(16.dp), horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.Bottom) {
        repeat(4) { index ->
            Box(
                Modifier
                    .width(4.dp)
                    .fillMaxHeight((index + 1) / 4f)
                    .clip(RoundedCornerShape(1.dp))
                    .background(if (index < lit) color else Color.White.copy(alpha = 0.2f)),
            )
        }
    }
}

// Launching and queue (boards 05 and 21) -------------------------------------------------------

@Composable
private fun LaunchingDeck(snapshot: DualScreenSnapshot, onAction: (DualScreenAction) -> Unit) {
    val settings = snapshot.sessionSettings
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            snapshot.streamGame?.let { GameArt(it, Modifier.size(56.dp), radius = 16.dp) }
            Column(Modifier.weight(1f)) {
                SectionLabel(
                    stringResource(if (snapshot.queuePosition != null) R.string.dual_in_queue else R.string.dual_starting),
                    color = DeckYellow,
                )
                DeckText(snapshot.gameTitle.orEmpty(), 17.sp, FontWeight.Black, maxLines = 1)
                if (snapshot.launchPhase.isNotBlank()) DeckText(snapshot.launchPhase, 12.sp, FontWeight.Bold, DeckMuted, maxLines = 1)
            }
            snapshot.queuePosition?.takeIf { it > 0 }?.let { position ->
                DeckText("#$position", 40.sp, FontWeight.Black)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ProfileTile(streamResolutionShortLabel(settings.resolution), R.string.dual_profile_resolution)
            ProfileTile(settings.fps.toString(), R.string.dual_stat_fps)
            ProfileTile(settings.codec.name, R.string.dual_profile_codec)
            ProfileTile(settings.maxBitrateMbps.toString(), R.string.dual_stat_bitrate)
        }
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            SectionLabel(stringResource(R.string.dual_settings_play_mode))
            PlayModeSelector(snapshot.playMode, onAction)
        }
        snapshot.regionName?.let { region ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(DeckTile).padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Box(Modifier.size(8.dp).clip(CircleShape).background(DeckMint))
                DeckText(region, 13.sp, FontWeight.Black, modifier = Modifier.weight(1f), maxLines = 1)
                snapshot.regionPingMs?.let { DeckText(stringResource(R.string.dual_ping_ms, it.toInt()), 12.sp, FontWeight.Bold, DeckMuted) }
            }
        }
        Spacer(Modifier.weight(1f))
        Row(Modifier.height(48.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            if (snapshot.launchMinimized) {
                PillButton(stringResource(R.string.dual_show_launch), primary = true, modifier = Modifier.weight(1f)) { onAction(DualScreenAction.RestoreLaunch) }
            }
            PillButton(stringResource(R.string.dual_cancel_launch), modifier = Modifier.weight(1f)) { onAction(DualScreenAction.CancelLaunch) }
        }
    }
}

private fun streamResolutionShortLabel(resolution: String): String =
    resolution.substringAfter('x', "").takeIf { it.isNotEmpty() }?.let { "${it}p" } ?: resolution

@Composable
private fun RowScope.ProfileTile(value: String, @StringRes label: Int) {
    Column(
        Modifier.weight(1f).clip(RoundedCornerShape(14.dp)).background(DeckTile).padding(horizontal = 10.dp, vertical = 7.dp),
    ) {
        DeckText(value, 18.sp, FontWeight.Black, maxLines = 1)
        DeckText(stringResource(label).uppercase(Locale.getDefault()), 9.sp, FontWeight.Black, DeckMuted, maxLines = 1)
    }
}

@Composable
private fun PlayModeSelector(current: BottomScreenPlayMode, onAction: (DualScreenAction) -> Unit) {
    val modes = listOf(
        BottomScreenPlayMode.StreamDeck to R.string.dual_play_mode_deck,
        BottomScreenPlayMode.Trackpad to R.string.dual_play_mode_trackpad,
        BottomScreenPlayMode.Off to R.string.dual_play_mode_off,
    )
    Segmented(
        options = modes.map { stringResource(it.second) },
        selected = modes.indexOfFirst { it.first == current },
        height = 38.dp,
        onSelect = { onAction(DualScreenAction.SetPlayMode(modes[it].first)) },
    )
}

// Streaming (board 03) --------------------------------------------------------------------------

@Composable
private fun StreamingDeck(snapshot: DualScreenSnapshot, onAction: (DualScreenAction) -> Unit) {
    var panel by remember(snapshot.playMode) {
        mutableStateOf(
            when (snapshot.playMode) {
                BottomScreenPlayMode.StreamDeck -> StreamPanel.Deck
                BottomScreenPlayMode.Trackpad -> StreamPanel.Trackpad
                BottomScreenPlayMode.Off -> StreamPanel.Off
            },
        )
    }
    // Kept here so the latency trace survives switching between deck panels.
    val pingHistory = remember { mutableStateListOf<Int>() }
    LaunchedEffect(snapshot.stats) {
        snapshot.stats?.pingMs?.let { ping ->
            pingHistory.add(ping)
            while (pingHistory.size > PING_HISTORY_SAMPLES) pingHistory.removeAt(0)
        }
    }
    when (panel) {
        StreamPanel.Deck -> StreamDeckPanel(snapshot, pingHistory, onAction) { panel = it }
        StreamPanel.Trackpad -> InputPanel(panel, onSelect = { panel = it }) { TrackpadSurface(Modifier.weight(1f)) }
        StreamPanel.Keyboard -> InputPanel(panel, onSelect = { panel = it }) {
            DeckKeyboard(
                modifier = Modifier.weight(1f),
                onText = NativeStreamInputRouter::sendCompanionText,
                onKey = NativeStreamInputRouter::sendCompanionKey,
                extraKeys = listOf("Esc" to KeyEvent.KEYCODE_ESCAPE, "Tab" to KeyEvent.KEYCODE_TAB),
            )
        }
        StreamPanel.Off -> Box(
            Modifier.fillMaxSize().background(Color.Black).clickable { panel = StreamPanel.Deck },
            contentAlignment = Alignment.BottomCenter,
        ) {
            DeckText(stringResource(R.string.dual_wake_hint), 12.sp, FontWeight.Bold, Color.White.copy(alpha = 0.25f))
        }
    }
}

@Composable
private fun StreamDeckPanel(
    snapshot: DualScreenSnapshot,
    pingHistory: List<Int>,
    onAction: (DualScreenAction) -> Unit,
    openPanel: (StreamPanel) -> Unit,
) {
    val stats = snapshot.stats
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(7.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            snapshot.streamGame?.let { GameArt(it, Modifier.size(36.dp), radius = 11.dp) }
            Column(Modifier.weight(1f)) {
                DeckText(snapshot.gameTitle.orEmpty(), 15.sp, FontWeight.Black, maxLines = 1)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    Box(Modifier.size(7.dp).clip(CircleShape).background(DeckMint))
                    DeckText(
                        listOfNotNull(stringResource(R.string.dual_live), snapshot.regionName).joinToString(" · "),
                        11.sp, FontWeight.Bold, DeckMuted, maxLines = 1,
                    )
                }
            }
            Column(horizontalAlignment = Alignment.End) {
                SessionClock(snapshot.sessionStartedAtMs)
                hoursLabel(snapshot)?.let { DeckText(it, 10.sp, FontWeight.Black, DeckYellow) }
            }
        }
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(18.dp))
                .background(Color.White.copy(alpha = 0.05f))
                .border(1.dp, DeckSeam, RoundedCornerShape(18.dp))
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row {
                Metric(R.string.dual_stat_ping, stats?.pingMs?.toString(), "ms", pingColor(stats?.pingMs))
                Metric(R.string.dual_stat_fps, (stats?.fps ?: stats?.decodedFps)?.toString(), snapshot.targetFps?.let { "/$it" }, Color.White)
                Metric(R.string.dual_stat_bitrate_label, stats?.bitrateKbps?.let { (it / 1000.0).roundToInt().toString() }, "Mbps", Color.White)
                Metric(
                    R.string.dual_stat_loss_label,
                    stats?.packetLossPct?.let { String.format(Locale.getDefault(), "%.1f", it) },
                    "%",
                    if ((stats?.packetLossPct ?: 0.0) >= 1.0) DeckCoral else DeckMint,
                )
            }
            Sparkline(pingHistory, Modifier.fillMaxWidth().height(26.dp))
        }
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ActionTile(R.string.dual_action_stream_menu, Icons.Rounded.Tune, DeckAccent, Modifier.weight(1f)) { onAction(DualScreenAction.OpenStreamMenu) }
            ActionTile(R.string.dual_action_stats, Icons.Rounded.QueryStats, DeckViolet, Modifier.weight(1f)) { onAction(DualScreenAction.ToggleStatsOverlay) }
            ActionTile(R.string.dual_action_keyboard, Icons.Rounded.Keyboard, DeckYellow, Modifier.weight(1f)) { openPanel(StreamPanel.Keyboard) }
        }
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ActionTile(R.string.dual_action_trackpad, Icons.Rounded.TouchApp, DeckMint, Modifier.weight(1f)) { openPanel(StreamPanel.Trackpad) }
            ActionTile(R.string.dual_action_deck_off, Icons.Rounded.DarkMode, Color.White.copy(alpha = 0.85f), Modifier.weight(1f)) { openPanel(StreamPanel.Off) }
        }
        HoldToEndButton { onAction(DualScreenAction.EndSession) }
    }
}

@Composable
private fun SessionClock(startedAtMs: Long?) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(startedAtMs) {
        while (startedAtMs != null) {
            now = System.currentTimeMillis()
            delay(1_000)
        }
    }
    val text = startedAtMs?.let {
        val seconds = ((now - it) / 1000).coerceAtLeast(0)
        String.format(Locale.US, "%d:%02d:%02d", seconds / 3600, (seconds / 60) % 60, seconds % 60)
    } ?: "–"
    Text(
        text,
        color = Color.White,
        fontSize = 16.sp,
        fontWeight = FontWeight.Bold,
        style = MaterialTheme.typography.bodyMedium.numeric(),
    )
}

@Composable
private fun RowScope.Metric(@StringRes label: Int, value: String?, unit: String?, color: Color) {
    Column(Modifier.weight(1f)) {
        DeckText(stringResource(label).uppercase(Locale.getDefault()), 9.sp, FontWeight.Black, DeckMuted, maxLines = 1)
        Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            DeckText(value ?: "–", 20.sp, FontWeight.Black, color, maxLines = 1)
            if (value != null && unit != null) {
                DeckText(unit, 10.sp, FontWeight.Bold, DeckMuted, maxLines = 1, modifier = Modifier.padding(bottom = 3.dp))
            }
        }
    }
}

@Composable
private fun Sparkline(samples: List<Int>, modifier: Modifier) {
    Canvas(modifier) {
        if (samples.size < 2) {
            drawLine(DeckMint.copy(alpha = 0.4f), Offset(0f, size.height / 2f), Offset(size.width, size.height / 2f), strokeWidth = 2.dp.toPx())
            return@Canvas
        }
        val max = samples.max().coerceAtLeast(1) * 1.25f
        val stepX = size.width / (PING_HISTORY_SAMPLES - 1)
        val startX = size.width - stepX * (samples.size - 1)
        val line = Path()
        samples.forEachIndexed { index, value ->
            val x = startX + stepX * index
            val y = size.height - (value / max) * size.height
            if (index == 0) line.moveTo(x, y) else line.lineTo(x, y)
        }
        val fill = Path().apply {
            addPath(line)
            lineTo(size.width, size.height)
            lineTo(startX, size.height)
            close()
        }
        drawPath(fill, Brush.verticalGradient(listOf(DeckMint.copy(alpha = 0.35f), Color.Transparent)))
        drawPath(line, DeckMint, style = Stroke(width = 2.dp.toPx(), join = StrokeJoin.Round))
    }
}

private fun pingColor(ping: Int?): Color = when {
    ping == null -> Color.White
    ping <= 40 -> DeckMint
    ping <= 80 -> DeckYellow
    else -> DeckCoral
}

@Composable
private fun ActionTile(@StringRes label: Int, icon: ImageVector, accent: Color, modifier: Modifier, onClick: () -> Unit) {
    Column(
        modifier
            .fillMaxHeight()
            .clip(RoundedCornerShape(18.dp))
            .background(DeckTile)
            .border(1.dp, DeckSeam, RoundedCornerShape(18.dp))
            .clickable(onClick = onClick)
            .padding(10.dp),
        verticalArrangement = Arrangement.SpaceBetween,
    ) {
        Box(Modifier.size(30.dp).clip(RoundedCornerShape(10.dp)).background(accent), contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = null, tint = DeckBackground, modifier = Modifier.size(18.dp))
        }
        DeckText(stringResource(label), 13.sp, FontWeight.Black, maxLines = 1)
    }
}

@Composable
private fun HoldToEndButton(onConfirmed: () -> Unit) {
    val progress = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    Box(
        Modifier
            .fillMaxWidth()
            .height(46.dp)
            .clip(CircleShape)
            .border(2.dp, DeckCoral, CircleShape)
            .drawBehind { drawRect(DeckCoral.copy(alpha = 0.3f), size = size.copy(width = size.width * progress.value)) }
            .pointerInput(Unit) {
                detectTapGestures(
                    onPress = {
                        val hold = scope.launch {
                            progress.animateTo(1f, tween(HOLD_TO_END_MS, easing = LinearEasing))
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            onConfirmed()
                        }
                        tryAwaitRelease()
                        if (hold.isActive) {
                            hold.cancel()
                            scope.launch { progress.animateTo(0f, tween(150)) }
                        }
                    },
                )
            },
        contentAlignment = Alignment.Center,
    ) {
        DeckText(
            stringResource(if (progress.value > 0f) R.string.dual_keep_holding else R.string.dual_hold_to_end),
            14.sp, FontWeight.Black, DeckCoral,
        )
    }
}

// Trackpad and keyboard (board 04) ------------------------------------------------------------

@Composable
private fun InputPanel(current: StreamPanel, onSelect: (StreamPanel) -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val modes = listOf(
        StreamPanel.Trackpad to R.string.dual_action_trackpad,
        StreamPanel.Keyboard to R.string.dual_action_keyboard,
        StreamPanel.Deck to R.string.dual_play_mode_deck,
    )
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Segmented(
            options = modes.map { stringResource(it.second) },
            selected = modes.indexOfFirst { it.first == current },
            height = 36.dp,
            onSelect = { onSelect(modes[it].first) },
        )
        if (current == StreamPanel.Trackpad) {
            Row(Modifier.height(40.dp), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                listOf(
                    "Esc" to KeyEvent.KEYCODE_ESCAPE,
                    "Tab" to KeyEvent.KEYCODE_TAB,
                    "Space" to KeyEvent.KEYCODE_SPACE,
                    "Enter" to KeyEvent.KEYCODE_ENTER,
                ).forEach { (label, keyCode) ->
                    KeyCap(label, Modifier.weight(1f)) { NativeStreamInputRouter.sendCompanionKey(keyCode) }
                }
                KeyCap(null, Modifier.weight(1f), icon = Icons.AutoMirrored.Rounded.Backspace) { NativeStreamInputRouter.sendCompanionKey(KeyEvent.KEYCODE_DEL) }
            }
        }
        content()
        if (current == StreamPanel.Trackpad) {
            Row(Modifier.height(46.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                PillButton(stringResource(R.string.dual_left_click), modifier = Modifier.weight(1f)) { NativeStreamInputRouter.sendCompanionClick(secondary = false) }
                PillButton(stringResource(R.string.dual_right_click), modifier = Modifier.weight(1f)) { NativeStreamInputRouter.sendCompanionClick(secondary = true) }
            }
        }
    }
}

/**
 * Relative trackpad: one finger moves the remote cursor, a quick tap clicks, a two-finger tap
 * right-clicks, and a two-finger drag or the side strip scrolls. Everything goes through the
 * stream's touch-mouse path, so no button stays held when the gesture ends.
 */
@Composable
private fun TrackpadSurface(modifier: Modifier) {
    val touchSlop = LocalViewConfiguration.current.touchSlop
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(
            Modifier
                .weight(1f)
                .fillMaxHeight()
                .clip(RoundedCornerShape(20.dp))
                .background(Color.White.copy(alpha = 0.04f))
                .drawBehind {
                    val step = 18.dp.toPx()
                    val dot = 1.2.dp.toPx()
                    var y = step / 2f
                    while (y < size.height) {
                        var x = step / 2f
                        while (x < size.width) {
                            drawCircle(Color.White.copy(alpha = 0.14f), dot, Offset(x, y))
                            x += step
                        }
                        y += step
                    }
                }
                .border(1.dp, DeckSeam, RoundedCornerShape(20.dp))
                .pointerInput(touchSlop) {
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        val startedAt = down.uptimeMillis
                        var maxPointers = 1
                        var travelled = 0f
                        var carryX = 0f
                        var carryY = 0f
                        var wheelCarry = 0f
                        while (true) {
                            val event = awaitPointerEvent()
                            val pressed = event.changes.filter { it.pressed }
                            maxPointers = maxOf(maxPointers, pressed.size)
                            if (pressed.isEmpty()) {
                                val quick = event.changes.first().uptimeMillis - startedAt <= TRACKPAD_TAP_MAX_MS
                                if (quick && travelled < touchSlop) {
                                    NativeStreamInputRouter.sendCompanionClick(secondary = maxPointers >= 2)
                                }
                                break
                            }
                            val delta = pressed.first().positionChange()
                            travelled += abs(delta.x) + abs(delta.y)
                            if (pressed.size >= 2) {
                                wheelCarry += delta.y
                                while (abs(wheelCarry) >= TRACKPAD_WHEEL_STEP_PX) {
                                    val direction = if (wheelCarry > 0) -1 else 1
                                    NativeStreamInputRouter.sendCompanionWheel(direction * TRACKPAD_WHEEL_DELTA)
                                    wheelCarry -= -direction * TRACKPAD_WHEEL_STEP_PX
                                }
                            } else if (maxPointers == 1) {
                                carryX += delta.x
                                carryY += delta.y
                                val dx = carryX.toInt()
                                val dy = carryY.toInt()
                                if (dx != 0 || dy != 0) {
                                    NativeStreamInputRouter.sendCompanionMouseMove(dx, dy)
                                    carryX -= dx
                                    carryY -= dy
                                }
                            }
                            event.changes.forEach { it.consume() }
                        }
                    }
                },
            contentAlignment = Alignment.BottomCenter,
        ) {
            DeckText(
                stringResource(R.string.dual_trackpad_hint),
                10.sp, FontWeight.Bold, Color.White.copy(alpha = 0.4f),
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(8.dp),
            )
        }
        var stripCarry by remember { mutableFloatStateOf(0f) }
        Box(
            Modifier
                .width(40.dp)
                .fillMaxHeight()
                .clip(RoundedCornerShape(20.dp))
                .background(Color.White.copy(alpha = 0.06f))
                .border(1.dp, DeckSeam, RoundedCornerShape(20.dp))
                .pointerInput(Unit) {
                    detectVerticalDragGestures(onDragEnd = { stripCarry = 0f }) { change, dragAmount ->
                        change.consume()
                        stripCarry += dragAmount
                        while (abs(stripCarry) >= TRACKPAD_WHEEL_STEP_PX) {
                            val direction = if (stripCarry > 0) -1 else 1
                            NativeStreamInputRouter.sendCompanionWheel(direction * TRACKPAD_WHEEL_DELTA)
                            stripCarry -= -direction * TRACKPAD_WHEEL_STEP_PX
                        }
                    }
                },
            contentAlignment = Alignment.Center,
        ) {
            Box(Modifier.width(4.dp).height(56.dp).clip(CircleShape).background(Color.White.copy(alpha = 0.3f)))
        }
    }
}

/** Touch keyboard shared by search and in-stream typing. Never requests IME focus. */
@Composable
private fun DeckKeyboard(
    modifier: Modifier,
    onText: (String) -> Unit,
    onKey: (Int) -> Unit,
    extraKeys: List<Pair<String, Int>> = emptyList(),
    enterLabel: String? = null,
) {
    var shifted by remember { mutableStateOf(false) }
    val rows = listOf("1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm")
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(5.dp)) {
        rows.forEachIndexed { index, row ->
            Row(Modifier.weight(1f).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                if (index == 2) Spacer(Modifier.weight(0.5f))
                if (index == 3) {
                    KeyCap("⇧", Modifier.weight(1.5f), tone = if (shifted) KeyTone.Accent else KeyTone.Muted) { shifted = !shifted }
                }
                row.forEach { char ->
                    val text = if (shifted) char.uppercaseChar().toString() else char.toString()
                    KeyCap(text, Modifier.weight(1f)) {
                        onText(text)
                        if (shifted) shifted = false
                    }
                }
                if (index == 2) Spacer(Modifier.weight(0.5f))
                if (index == 3) {
                    KeyCap(null, Modifier.weight(1.5f), tone = KeyTone.Muted, icon = Icons.AutoMirrored.Rounded.Backspace) { onKey(KeyEvent.KEYCODE_DEL) }
                }
            }
        }
        Row(Modifier.weight(1f).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            extraKeys.forEach { (label, keyCode) -> KeyCap(label, Modifier.weight(1.2f), tone = KeyTone.Muted) { onKey(keyCode) } }
            KeyCap(stringResource(R.string.dual_key_space), Modifier.weight(4f), tone = KeyTone.Muted) { onText(" ") }
            KeyCap(
                enterLabel ?: stringResource(R.string.dual_key_enter),
                Modifier.weight(2f),
                tone = KeyTone.Accent,
                icon = Icons.AutoMirrored.Rounded.KeyboardReturn.takeIf { enterLabel == null },
            ) { onKey(KeyEvent.KEYCODE_ENTER) }
        }
    }
}

private enum class KeyTone { Normal, Muted, Accent }

@Composable
private fun KeyCap(label: String?, modifier: Modifier, tone: KeyTone = KeyTone.Normal, icon: ImageVector? = null, onClick: () -> Unit) {
    val background = when (tone) {
        KeyTone.Normal -> Color.White.copy(alpha = 0.11f)
        KeyTone.Muted -> Color.White.copy(alpha = 0.06f)
        KeyTone.Accent -> DeckAccent
    }
    val content = if (tone == KeyTone.Accent) DeckBackground else Color.White
    Box(
        modifier
            .fillMaxHeight()
            // V3 keycap: a darker 2 dp base under the cap.
            .drawBehind {
                drawRoundRect(Color.Black.copy(alpha = 0.45f), topLeft = Offset(0f, 2.dp.toPx()), cornerRadius = CornerRadius(12.dp.toPx()))
            }
            .clip(RoundedCornerShape(12.dp))
            .background(background)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = label, tint = content, modifier = Modifier.size(18.dp))
        } else if (label != null) {
            DeckText(label, 15.sp, FontWeight.Black, content, maxLines = 1)
        }
    }
}

// Settings inspector (boards 12–20) ----------------------------------------------------------

/** Big touch controls for whichever setting has controller focus on the top screen. */
@Composable
private fun SettingsInspectorDeck(snapshot: DualScreenSnapshot, onAction: (DualScreenAction) -> Unit) {
    val inspection = DualScreenBridge.inspection.value?.second
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .clip(RoundedCornerShape(24.dp))
                .background(Color.White.copy(alpha = 0.05f))
                .border(1.dp, if (inspection != null) DeckAccent.copy(alpha = 0.45f) else DeckSeam, RoundedCornerShape(24.dp))
                .padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (inspection == null) {
                Spacer(Modifier.weight(1f))
                DeckText(stringResource(R.string.dual_settings_hint_title), 18.sp, FontWeight.Black)
                DeckText(stringResource(R.string.dual_settings_hint_body), 13.sp, FontWeight.SemiBold, DeckMuted)
                Spacer(Modifier.weight(1f))
            } else {
                SectionLabel(stringResource(R.string.dual_settings_eyebrow), color = DeckAccent)
                DeckText(inspection.label, 21.sp, FontWeight.Black, maxLines = 2)
                inspection.description?.takeIf { it.isNotBlank() }?.let {
                    DeckText(it, 12.sp, FontWeight.SemiBold, DeckMuted, maxLines = 3)
                }
                when (inspection) {
                    is SettingInspection.Switch -> Segmented(
                        options = listOf(stringResource(R.string.dual_switch_off), stringResource(R.string.dual_switch_on)),
                        selected = if (inspection.checked) 1 else 0,
                        height = 48.dp,
                        onSelect = { index -> if (inspection.enabled) inspection.onCheckedChange(index == 1) },
                    )
                    is SettingInspection.Choice -> Column(
                        Modifier.weight(1f).verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        inspection.options.forEach { option ->
                            ChoiceOption(
                                label = option.label,
                                badge = option.badge,
                                selected = option.label == inspection.selectedLabel,
                                enabled = option.enabled,
                            ) { inspection.onSelect(option.value) }
                        }
                    }
                    is SettingInspection.Slider -> SliderInspector(inspection)
                }
            }
        }
        DeckTabBar(snapshot.page, onSearch = { onAction(DualScreenAction.Navigate(AppPage.Home)) }, onAction = onAction)
    }
}

@Composable
private fun ChoiceOption(label: String, badge: String?, selected: Boolean, enabled: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(46.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(if (selected) Color.White else Color.White.copy(alpha = 0.06f))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        DeckText(
            label, 15.sp, FontWeight.Black,
            when {
                selected -> DeckBackground
                enabled -> Color.White
                else -> DeckMuted
            },
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        badge?.let { DeckText(it, 11.sp, FontWeight.Bold, if (selected) DeckBackground.copy(alpha = 0.6f) else DeckMuted) }
    }
}

@Composable
private fun ColumnScope.SliderInspector(inspection: SettingInspection.Slider) {
    val stepCount = if (inspection.step > 0f) ((inspection.max - inspection.min) / inspection.step).roundToInt() else 0
    fun nudge(direction: Int) {
        inspection.onChange((inspection.value + direction * inspection.step).coerceIn(inspection.min, inspection.max))
    }
    DeckText(inspection.valueText, 36.sp, FontWeight.Black, DeckAccent)
    Slider(
        value = inspection.value,
        onValueChange = inspection.onChange,
        valueRange = inspection.min..inspection.max,
        steps = if (stepCount in 2..200) stepCount - 1 else 0,
        colors = SliderDefaults.colors(
            thumbColor = Color.White,
            activeTrackColor = DeckAccent,
            inactiveTrackColor = Color.White.copy(alpha = 0.14f),
            activeTickColor = Color.Transparent,
            inactiveTickColor = Color.Transparent,
        ),
    )
    Spacer(Modifier.weight(1f))
    Row(Modifier.height(48.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        PillButton("−", modifier = Modifier.weight(1f)) { nudge(-1) }
        PillButton("+", modifier = Modifier.weight(1f)) { nudge(1) }
    }
}

// Shared V3 primitives -------------------------------------------------------------------------

@Composable
private fun DeckText(
    text: String,
    size: TextUnit,
    weight: FontWeight,
    color: Color = Color.White,
    modifier: Modifier = Modifier,
    maxLines: Int = Int.MAX_VALUE,
    textAlign: TextAlign? = null,
) {
    Text(
        text = text,
        modifier = modifier,
        color = color,
        fontSize = size,
        fontWeight = weight,
        lineHeight = size * 1.2f,
        maxLines = maxLines,
        overflow = TextOverflow.Ellipsis,
        textAlign = textAlign,
        style = MaterialTheme.typography.bodyMedium,
    )
}

@Composable
private fun SectionLabel(text: String, modifier: Modifier = Modifier, color: Color = DeckMuted) {
    DeckText(text.uppercase(Locale.getDefault()), 10.sp, FontWeight.Black, color, modifier = modifier.padding(start = 2.dp), maxLines = 1)
}

@Composable
private fun GameArt(game: GameInfo, modifier: Modifier, radius: Dp) {
    val shape = RoundedCornerShape(radius)
    Box(modifier.clip(shape).background(OpenNowPalette.ImagePlaceholder).border(1.5.dp, Color.White.copy(alpha = 0.85f), shape)) {
        val url = game.imageUrl ?: game.tvCardImageUrl ?: game.screenshotUrl
        if (url != null) {
            AsyncImage(model = url, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.matchParentSize())
        }
    }
}

@Composable
private fun ButtonGlyph(letter: String, dark: Boolean, size: Dp = 24.dp) {
    Box(
        Modifier.size(size).clip(CircleShape).background(if (dark) DeckBackground else Color.White),
        contentAlignment = Alignment.Center,
    ) {
        DeckText(letter, 12.sp, FontWeight.Black, if (dark) Color.White else DeckBackground)
    }
}

@Composable
private fun Segmented(options: List<String>, selected: Int, height: Dp = 40.dp, onSelect: (Int) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(height)
            .clip(CircleShape)
            .background(DeckTile)
            .border(1.dp, DeckSeam, CircleShape)
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        options.forEachIndexed { index, label ->
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .clip(CircleShape)
                    .background(if (index == selected) Color.White else Color.Transparent)
                    .clickable { onSelect(index) },
                contentAlignment = Alignment.Center,
            ) {
                DeckText(label, 12.sp, FontWeight.Black, if (index == selected) DeckBackground else Color.White.copy(alpha = 0.8f), maxLines = 1)
            }
        }
    }
}

@Composable
private fun SmallPill(label: String, onClick: () -> Unit) {
    Box(
        Modifier
            .height(32.dp)
            .clip(CircleShape)
            .background(DeckTileStrong)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        DeckText(label, 12.sp, FontWeight.Black)
    }
}

@Composable
private fun PillButton(
    label: String,
    modifier: Modifier = Modifier,
    primary: Boolean = false,
    height: Dp = 46.dp,
    icon: ImageVector? = null,
    iconTint: Color = Color.White,
    onClick: () -> Unit,
) {
    Row(
        modifier
            .height(height)
            .clip(CircleShape)
            .background(if (primary) Color.White else Color.White.copy(alpha = 0.08f))
            .border(1.dp, if (primary) Color.Transparent else DeckSeam, CircleShape)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        icon?.let {
            Icon(it, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
        }
        DeckText(label, 14.sp, FontWeight.Black, if (primary) DeckBackground else Color.White, maxLines = 1)
    }
}
