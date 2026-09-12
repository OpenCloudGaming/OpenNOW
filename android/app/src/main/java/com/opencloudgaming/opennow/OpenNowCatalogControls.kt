package com.opencloudgaming.opennow

import android.net.Uri
import android.os.SystemClock
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import coil3.compose.AsyncImage
import kotlinx.coroutines.launch
import java.io.File
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import kotlin.math.PI
import kotlin.math.max
import kotlin.math.sin

@Composable
private fun catalogSortDisplayLabel(sortId: String, fallback: String): String =
    when (catalogSortKind(sortId)) {
        CatalogSortKind.Relevance -> fallback
        CatalogSortKind.Popular -> stringResource(R.string.catalog_sort_popular)
        CatalogSortKind.NewlyAdded -> stringResource(R.string.catalog_sort_new_games)
        CatalogSortKind.LastPlayed -> stringResource(R.string.catalog_sort_last_played)
        CatalogSortKind.Other -> fallback
    }

@Composable
internal fun SortPicker(
    options: List<CatalogSortOption>,
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val labels = options.ifEmpty {
        listOf(CatalogSortOption(DEFAULT_CATALOG_SORT_ID, "Most Popular", ""))
    }
    val selectedLabel = labels.firstOrNull { it.id == selected }?.label ?: labels.first().label
    var expanded by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf(false) }
    BackHandler(enabled = expanded) { expanded = false }
    val controlShape = RoundedCornerShape(999.dp)
    val controlColor = Color.White.copy(alpha = 0.1f)
    Box(modifier) {
        OutlinedButton(
            onClick = { expanded = true },
            modifier = Modifier
                .fillMaxWidth()
                .height(if (compact) TopBarCompactControlHeight else 40.dp)
                .onFocusChanged { focused = it.isFocused || it.hasFocus },
            shape = controlShape,
            border = null,
            colors = ButtonDefaults.outlinedButtonColors(
                containerColor = controlColor,
                contentColor = TextPrimary,
            ),
            contentPadding = PaddingValues(horizontal = if (compact) 8.dp else 12.dp),
        ) {
            Text(
                "Sort: ${catalogSortDisplayLabel(selected, selectedLabel)}",
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = if (compact) MaterialTheme.typography.labelMedium else MaterialTheme.typography.labelLarge,
            )
        }
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 999.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
        )
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            labels.forEach { option ->
                DropdownMenuItem(
                    text = {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(if (option.id == selected) "✓" else "", modifier = Modifier.width(24.dp))
                            Text(catalogSortDisplayLabel(option.id, option.label))
                        }
                    },
                    onClick = {
                        expanded = false
                        onSelect(option.id)
                    },
                )
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun SelectedFilterChips(options: List<CatalogFilterOption>, selectedIds: List<String>, onToggle: (String) -> Unit) {
    val selectedOptions = options.filter { it.id in selectedIds }
    if (selectedOptions.isEmpty()) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        selectedOptions.take(4).forEach { option ->
            AssistChip(
                onClick = { onToggle(option.id) },
                label = { Text(option.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                border = BorderStroke(2.dp, Color.White.copy(alpha = 0.92f)),
            )
        }
        if (selectedOptions.size > 4) {
            AssistChip(onClick = {}, label = { Text("+${selectedOptions.size - 4}") })
        }
    }
}

private val CATALOG_VISIBLE_FILTER_GROUP_IDS = setOf("digital_store", "genre", "subscriptions")

internal fun catalogVisibleFilterGroups(groups: List<CatalogFilterGroup>): List<CatalogFilterGroup> =
    groups.filter { it.id in CATALOG_VISIBLE_FILTER_GROUP_IDS }

/**
 * The filter rows for [groups], plus OpenNOW's own touch-controls filter.
 *
 * Deduplicated by id. The provider hands back one flat id namespace across groups, so the same
 * option can legitimately appear under both `digital_store` and `subscriptions` — and the lists
 * built here are rendered by `LazyColumn`/`items(key = ...)`, which throws on a repeated key. That
 * crash only reproduces against accounts whose catalogue happens to carry an overlap, which is why
 * the invariant belongs here rather than at each call site.
 */
internal fun catalogFilterOptions(
    groups: List<CatalogFilterGroup>,
    touchFilterLabel: String,
    controlsGroupLabel: String,
): List<CatalogFilterOption> =
    (
        groups.flatMap { group -> group.options.take(if (group.id == "genre") 10 else group.options.size) } +
            CatalogFilterOption(
                id = CATALOG_FILTER_TOUCHSCREEN,
                rawId = SUPPORTED_CONTROL_TOUCHSCREEN,
                label = touchFilterLabel,
                groupId = "supported_controls",
                groupLabel = controlsGroupLabel,
            )
        ).distinctBy { it.id }

/**
 * Remembers [catalogFilterOptions] for the current catalogue.
 *
 * The uncached version ran on every recomposition — including every frame of a grid scroll — and
 * allocated a fresh list each time. That is invisible on a fast phone and is exactly the kind of
 * steady allocation that pushes a low-RAM device into continuous GC.
 */
@Composable
internal fun rememberCatalogFilterOptions(groups: List<CatalogFilterGroup>): List<CatalogFilterOption> {
    val touchFilterLabel = stringResource(R.string.catalog_filter_touch_controls)
    val controlsGroupLabel = stringResource(R.string.catalog_filter_controls_group)
    return remember(groups, touchFilterLabel, controlsGroupLabel) {
        catalogFilterOptions(groups, touchFilterLabel, controlsGroupLabel)
    }
}

@Composable
internal fun FilterMenu(
    options: List<CatalogFilterOption>,
    selectedIds: List<String>,
    onToggle: (String) -> Unit,
    compact: Boolean = false,
) {
    var expanded by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf(false) }
    val filterControlShape = RoundedCornerShape(999.dp)
    val filterControlColor = Color.White.copy(alpha = 0.1f)
    Box {
        OutlinedButton(
            onClick = { expanded = true },
            modifier = Modifier
                .height(if (compact) TopBarCompactControlHeight else 36.dp)
                .onFocusChanged { focused = it.isFocused || it.hasFocus },
            shape = filterControlShape,
            border = null,
            colors = ButtonDefaults.outlinedButtonColors(
                containerColor = filterControlColor,
                contentColor = TextPrimary,
            ),
            contentPadding = PaddingValues(horizontal = 10.dp),
        ) {
            Text(if (selectedIds.isEmpty()) "Filters" else "Filters ${selectedIds.size}", maxLines = 1, style = MaterialTheme.typography.labelMedium)
        }
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 999.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
        )
        if (expanded) {
            AlertDialog(
                onDismissRequest = { expanded = false },
                title = {
                    Text(
                        stringResource(R.string.catalog_filters),
                        fontWeight = FontWeight.Bold,
                        style = MaterialTheme.typography.titleMedium,
                        color = TextPrimary,
                    )
                },
                text = {
                    LazyColumn(
                        modifier = Modifier.fillMaxHeight(0.6f),
                        verticalArrangement = Arrangement.spacedBy(4.dp)
                    ) {
                        items(options) { option ->
                            val isSelected = option.id in selectedIds
                            var rowFocused by remember { mutableStateOf(false) }
                            Box(Modifier.fillMaxWidth()) {
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clip(RoundedCornerShape(8.dp))
                                        .onFocusChanged { rowFocused = it.isFocused || it.hasFocus }
                                        .background(if (rowFocused) Color.White.copy(alpha = 0.08f) else Color.Transparent)
                                        .clickable { onToggle(option.id) }
                                        .padding(horizontal = 8.dp, vertical = 6.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Checkbox(checked = isSelected, onCheckedChange = null)
                                    Spacer(Modifier.width(12.dp))
                                    Text(option.label, style = MaterialTheme.typography.bodyLarge, color = TextPrimary)
                                }
                                InteractionFocusFrame(
                                    visible = rowFocused,
                                    cornerRadius = 8.dp,
                                    cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
                                )
                            }
                        }
                    }
                },
                confirmButton = {
                    Button(onClick = { expanded = false }) {
                        Text(stringResource(R.string.stream_panel_done))
                    }
                }
            )
        }
    }
}

/** One compact top-bar entry point for the complete catalogue ordering and filtering surface. */
@Composable
internal fun CatalogSortFilterMenu(
    sortOptions: List<CatalogSortOption>,
    selectedSortId: String,
    filterOptions: List<CatalogFilterOption>,
    selectedFilterIds: List<String>,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    modifier: Modifier = Modifier,
    focusRequester: FocusRequester? = null,
    leadingFocusRequester: FocusRequester? = null,
) {
    // Same flat provider id namespace as the filter options — deduplicate before the keyed
    // `items` below turns a repeated id into a crash.
    val sorts = remember(sortOptions) {
        sortOptions.distinctBy { it.id }.ifEmpty {
            listOf(CatalogSortOption(DEFAULT_CATALOG_SORT_ID, "Most Popular", ""))
        }
    }
    var expanded by remember { mutableStateOf(false) }
    BackHandler(enabled = expanded) { expanded = false }
    val description = if (selectedFilterIds.isEmpty()) {
        stringResource(R.string.catalog_sort_filter)
    } else {
        stringResource(R.string.catalog_sort_filter_active, selectedFilterIds.size)
    }
    var focused by remember { mutableStateOf(false) }
    Box(modifier) {
        Surface(
            shape = RoundedCornerShape(14.dp),
            color = Color.White.copy(alpha = 0.1f),
            border = null,
        ) {
            IconButton(
                onClick = { expanded = true },
                modifier = Modifier
                    .size(40.dp)
                    .then(focusRequester?.let { Modifier.focusRequester(it) } ?: Modifier)
                    .then(
                        leadingFocusRequester?.let { leading ->
                            Modifier.focusProperties {
                                left = leading
                                up = leading
                            }
                        } ?: Modifier,
                    )
                    .onFocusChanged { focused = it.isFocused },
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_sort_filter),
                    contentDescription = description,
                    tint = if (selectedFilterIds.isEmpty()) TextPrimary else LocalSelectionTintColor.current,
                    modifier = Modifier.size(22.dp),
                )
            }
        }
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 14.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
        )
        if (expanded) {
            AlertDialog(
                onDismissRequest = { expanded = false },
                title = {
                    Text(
                        stringResource(R.string.catalog_sort_filter),
                        fontWeight = FontWeight.Bold,
                        style = MaterialTheme.typography.titleMedium,
                        color = TextPrimary,
                    )
                },
                text = {
                    LazyColumn(
                        modifier = Modifier.fillMaxHeight(0.68f),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        item {
                            Text(
                                stringResource(R.string.catalog_sort_section),
                                color = MaterialTheme.colorScheme.primary,
                                fontWeight = FontWeight.Bold,
                                style = MaterialTheme.typography.labelLarge,
                                modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                            )
                        }
                        items(sorts, key = { "sort:${it.id}" }) { option ->
                            val selected = option.id == selectedSortId
                            CatalogMenuChoiceRow(
                                label = catalogSortDisplayLabel(option.id, option.label),
                                selected = selected,
                                onClick = { onSortChange(option.id) },
                            )
                        }
                        if (filterOptions.isNotEmpty()) {
                            item {
                                Text(
                                    stringResource(R.string.catalog_filter_section),
                                    color = MaterialTheme.colorScheme.primary,
                                    fontWeight = FontWeight.Bold,
                                    style = MaterialTheme.typography.labelLarge,
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 12.dp),
                                )
                            }
                            items(filterOptions, key = { "filter:${it.id}" }) { option ->
                                val selected = option.id in selectedFilterIds
                                CatalogMenuChoiceRow(
                                    label = option.label,
                                    selected = selected,
                                    checkbox = true,
                                    onClick = { onFilterToggle(option.id) },
                                )
                            }
                        }
                    }
                },
                confirmButton = {
                    androidx.compose.material3.Button(onClick = { expanded = false }) {
                        Text(stringResource(R.string.stream_panel_done))
                    }
                },
            )
        }
    }
}

@Composable
private fun CatalogMenuChoiceRow(
    label: String,
    selected: Boolean,
    checkbox: Boolean = false,
    onClick: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(9.dp)
    Box(Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(shape)
                .onFocusChanged { focused = it.isFocused || it.hasFocus }
                .background(
                    when {
                        focused -> Color.White.copy(alpha = 0.1f)
                        selected -> LocalSelectionTintColor.current.copy(alpha = 0.12f)
                        else -> Color.Transparent
                    },
                )
                .clickable(onClick = onClick)
                .padding(horizontal = 8.dp, vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (checkbox) {
                Checkbox(checked = selected, onCheckedChange = null)
            } else {
                Text(if (selected) "✓" else "", modifier = Modifier.width(36.dp), color = LocalSelectionTintColor.current)
            }
            Spacer(Modifier.width(if (checkbox) 10.dp else 0.dp))
            Text(
                label,
                color = TextPrimary,
                fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                modifier = Modifier.weight(1f),
            )
        }
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 9.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
        )
    }
}

@Composable
internal fun PrintedWasteSelector(
    state: OpenNowUiState,
    game: GameInfo,
    viewModel: OpenNowViewModel,
    modifier: Modifier = Modifier,
) {
    BackHandler(onBack = viewModel::dismissPrintedWasteSelector)
    val zones = remember(state.printedWasteQueue, state.printedWasteMapping, state.printedWastePings) {
        state.printedWasteQueue
            .filter { (zoneId, _) -> isStandardPrintedWasteZone(zoneId) && state.printedWasteMapping[zoneId]?.nuked != true }
            .map { (zoneId, zone) ->
                val routingUrl = printedWasteZoneUrl(zoneId)
                PrintedWasteZoneOption(
                    zoneId = zoneId,
                    zone = zone,
                    routingUrl = routingUrl,
                    pingMs = state.printedWastePings[routingUrl],
                )
            }
    }
    val autoZone = remember(zones) { recommendedPrintedWasteZone(zones) }
    // One row per physical location rather than per server id — see PrintedWasteZones.kt.
    val regionGroups = remember(zones, state.printedWasteMapping) {
        val maxPing = zones.mapNotNull { it.pingMs }.maxOrNull()?.coerceAtLeast(1L) ?: 1L
        val maxQueue = zones.maxOfOrNull { it.zone.QueuePosition }?.coerceAtLeast(1) ?: 1
        printedWasteRegionGroups(
            printedWasteLocations(zones, state.printedWasteMapping),
            maxPing = maxPing,
            maxQueue = maxQueue,
        )
    }
    val locations = remember(regionGroups) { regionGroups.flatMap { it.second } }
    // Match by name, not by id: the recommendation and the fold use slightly different tiebreaks,
    // so the recommended server can end up as an alternate inside its location rather than its
    // primary. Matching on id there would drop the "best route" card entirely.
    val autoLocation = remember(locations, autoZone, state.printedWasteMapping) {
        val autoTitle = autoZone?.let {
            printedWasteZoneTitle(it.zoneId, state.printedWasteMapping[it.zoneId])
        }
        locations.firstOrNull { it.title == autoTitle }
    }
    var selectedZoneId by remember(game.id, locations) {
        mutableStateOf<String?>((autoLocation ?: locations.firstOrNull())?.primary?.zoneId)
    }
    val selectedZone = locations.firstOrNull { it.primary.zoneId == selectedZoneId }?.primary ?: autoZone
    val selectableZones = remember(locations) { locations.map { it.primary } }
    var pendingHigherPingLaunch by remember(game.id) {
        mutableStateOf<PrintedWasteZoneOption?>(null)
    }
    var dismissHigherPingWarning by remember(game.id) { mutableStateOf(false) }
    val launchSelectedZone: () -> Unit = {
        selectedZone?.let { zone ->
            if (
                !state.settings.higherPingWarningDismissed &&
                hasHigherPingThanClosestPrintedWasteZone(zone, selectableZones)
            ) {
                dismissHigherPingWarning = false
                pendingHigherPingLaunch = zone
            } else {
                viewModel.launchWithPrintedWaste(zone.routingUrl)
            }
        }
    }
    val context = LocalContext.current

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .lockedFocusGroup()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
    ) {
        val phoneLandscape = isPhoneLandscape(maxWidth, maxHeight)
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
                                .fillMaxWidth(0.94f)
                                .fillMaxHeight(0.82f)
                        },
                    ),
                colors = CardDefaults.cardColors(containerColor = Panel),
                shape = RoundedCornerShape(22.dp),
            ) {
                if (phoneLandscape) {
                    Row(
                        Modifier.fillMaxSize().padding(14.dp),
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        PrintedWasteGameSummary(
                            game = game,
                            modifier = Modifier
                                .width(190.dp)
                                .fillMaxHeight(),
                        )
                        PrintedWasteOptionsColumn(
                            state = state,
                            regionGroups = regionGroups,
                            locations = locations,
                            selectedZoneId = selectedZoneId,
                            selectedZone = selectedZone,
                            autoLocation = autoLocation,
                            showRecommendedCard = true,
                            onSelectZone = { selectedZoneId = it },
                            onRetry = viewModel::refreshPrintedWasteQueues,
                            onDismiss = viewModel::dismissPrintedWasteSelector,
                            onDefault = { viewModel.launchWithPrintedWaste(null) },
                            onLaunch = launchSelectedZone,
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxHeight(),
                        )
                    }
                } else {
                    Column(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            UrlImage(
                                gameTvBannerImageUrl(context, game),
                                Modifier
                                    .width(98.dp)
                                    .aspectRatio(16f / 9f)
                                    .clip(RoundedCornerShape(12.dp)),
                            )
                            Spacer(Modifier.width(12.dp))
                            Column(Modifier.weight(1f)) {
                                Text(game.title, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text(stringResource(R.string.catalog_free_tier_routing), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                        PrintedWasteOptionsColumn(
                            state = state,
                            regionGroups = regionGroups,
                            locations = locations,
                            selectedZoneId = selectedZoneId,
                            selectedZone = selectedZone,
                            autoLocation = autoLocation,
                            showRecommendedCard = true,
                            onSelectZone = { selectedZoneId = it },
                            onRetry = viewModel::refreshPrintedWasteQueues,
                            onDismiss = viewModel::dismissPrintedWasteSelector,
                            onDefault = { viewModel.launchWithPrintedWaste(null) },
                            onLaunch = launchSelectedZone,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }

    pendingHigherPingLaunch?.let { zone ->
        AlertDialog(
            onDismissRequest = {
                dismissHigherPingWarning = false
                pendingHigherPingLaunch = null
            },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(
                        stringResource(R.string.queue_higher_ping_warning),
                        color = Color(0xffff8d8d),
                        fontWeight = FontWeight.Bold,
                    )
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(12.dp))
                            .clickable { dismissHigherPingWarning = !dismissHigherPingWarning }
                            .padding(horizontal = 8.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Checkbox(
                            checked = dismissHigherPingWarning,
                            onCheckedChange = { dismissHigherPingWarning = it },
                        )
                        Text(
                            stringResource(R.string.queue_higher_ping_dont_show_again),
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        if (dismissHigherPingWarning) {
                            viewModel.updateSettings(
                                state.settings.copy(higherPingWarningDismissed = true),
                            )
                        }
                        dismissHigherPingWarning = false
                        pendingHigherPingLaunch = null
                        viewModel.launchWithPrintedWaste(zone.routingUrl)
                    },
                ) {
                    Text(stringResource(R.string.action_ok))
                }
            },
            dismissButton = {
                TextButton(
                    onClick = {
                        dismissHigherPingWarning = false
                        pendingHigherPingLaunch = null
                    },
                ) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }
}

@Composable
private fun PrintedWasteGameSummary(
    game: GameInfo,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        UrlImage(
            gameTvBannerImageUrl(context, game),
            Modifier
                .fillMaxWidth()
                .aspectRatio(16f / 9f)
                .clip(RoundedCornerShape(16.dp)),
        )
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(game.title, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(stringResource(R.string.catalog_free_tier_routing), color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1)
        }
    }
}

@Composable
private fun PrintedWasteOptionsColumn(
    state: OpenNowUiState,
    regionGroups: List<Pair<String, List<PrintedWasteLocation>>>,
    locations: List<PrintedWasteLocation>,
    selectedZoneId: String?,
    selectedZone: PrintedWasteZoneOption?,
    autoLocation: PrintedWasteLocation?,
    showRecommendedCard: Boolean,
    onSelectZone: (String) -> Unit,
    onRetry: () -> Unit,
    onDismiss: () -> Unit,
    onDefault: () -> Unit,
    onLaunch: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val zoneListState = rememberLazyListState()
    val zoneListFocusRequester = remember { FocusRequester() }
    val defaultFocusRequester = remember { FocusRequester() }
    val launchFocusRequester = remember { FocusRequester() }
    var zoneListFocused by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    fun selectZoneAt(index: Int) {
        val next = locations.getOrNull(index) ?: return
        onSelectZone(next.primary.zoneId)
        scope.launch {
            // Region headings are list items too, so a location's row sits further down than its
            // index among locations. Counting the headings above it keeps the scroll honest.
            val headingsAbove = regionGroups
                .runningFold(0) { acc, group -> acc + group.second.size }
                .indexOfFirst { it > index }
                .coerceAtLeast(1)
            zoneListState.animateScrollToItem(index + headingsAbove)
        }
    }
    LaunchedEffect(state.printedWasteLoading, state.printedWasteError, locations.size) {
        val initialFocusRequester = if (
            !state.printedWasteLoading &&
            state.printedWasteError == null &&
            locations.isNotEmpty()
        ) {
            launchFocusRequester
        } else {
            defaultFocusRequester
        }
        requestFocusWithRetry(initialFocusRequester)
    }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (state.printedWasteLoading) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
                    Text(stringResource(R.string.catalog_checking_queues), color = TextMuted)
                }
            }
        } else if (state.printedWasteError != null) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(state.printedWasteError, color = Color(0xffff9f9f))
                    OutlinedButton(onClick = onRetry) { Text(stringResource(R.string.action_retry)) }
                }
            }
        } else {
            if (showRecommendedCard) {
                autoLocation?.let {
                    RecommendedPrintedWasteCard(it)
                }
            }
            var listFocused by remember { mutableStateOf(false) }
            LazyColumn(
                state = zoneListState,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(zoneListFocusRequester)
                    .onFocusChanged { listFocused = it.isFocused }
                    .onPreviewKeyEvent { event ->
                        if (isTvActivateKey(event)) {
                            if (selectedZone != null) {
                                onLaunch()
                                true
                            } else {
                                false
                            }
                        } else if (event.type == KeyEventType.KeyDown) {
                            val selectedIndex = locations
                                .indexOfFirst { it.primary.zoneId == selectedZoneId }
                                .let { if (it >= 0) it else 0 }
                            when (event.key) {
                                Key.DirectionUp -> {
                                    if (selectedIndex > 0) {
                                        selectZoneAt(selectedIndex - 1)
                                        true
                                    } else {
                                        false
                                    }
                                }
                                Key.DirectionDown -> {
                                    if (selectedIndex < locations.lastIndex) {
                                        selectZoneAt(selectedIndex + 1)
                                        true
                                    } else {
                                        runCatching { launchFocusRequester.requestFocus() }.isSuccess
                                    }
                                }
                                else -> false
                            }
                        } else {
                            false
                        }
                    }
                    .focusable(),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                regionGroups.forEach { (region, regionLocations) ->
                    item(key = "region:$region") {
                        Text(
                            region,
                            color = TextMuted,
                            style = MaterialTheme.typography.labelMedium,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.padding(top = 6.dp, bottom = 2.dp),
                        )
                    }
                    items(regionLocations, key = { it.primary.zoneId }) { location ->
                        val isCurrent = location.primary.zoneId == selectedZoneId
                        PrintedWasteLocationRow(
                            location = location,
                            selected = isCurrent,
                            focused = isCurrent && listFocused,
                            listFocused = listFocused,
                            liveSelectedOutlines = LocalActiveSelectionEnabled.current,
                            onClick = { onSelectZone(location.primary.zoneId) },
                        )
                    }
                }
            }
        }

        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_cancel)) }
            OutlinedButton(
                onClick = onDefault,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(defaultFocusRequester),
            ) {
                Text(stringResource(R.string.store_selector_default), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Button(
                onClick = onLaunch,
                enabled = !state.printedWasteLoading && selectedZone != null,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(launchFocusRequester)
                    .focusProperties { up = zoneListFocusRequester },
            ) {
                Text(stringResource(R.string.action_launch), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun RecommendedPrintedWasteCard(location: PrintedWasteLocation) {
    val zoneOption = location.primary
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f),
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    stringResource(R.string.queue_best_route),
                    color = MaterialTheme.colorScheme.primary,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    location.title,
                    color = TextPrimary,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    printedWasteLocationDetail(location),
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            QueueMetricPill(
                stringResource(R.string.stream_statusbar_metric_ping),
                zoneOption.pingMs?.let { "$it ms" } ?: stringResource(R.string.queue_checking),
            )
            QueueMetricPill(
                stringResource(R.string.queue_metric_ahead),
                zoneOption.zone.QueuePosition.toString(),
                queueColor(zoneOption.zone.QueuePosition),
            )
        }
    }
}

/**
 * The secondary line under a location name: its region, the GPU it runs, and how many server ids
 * were folded into this one row.
 *
 * The alternate count is stated rather than hidden because it is capacity the player may care
 * about — three Southern California servers behind one row is a different proposition from one.
 */
@Composable
private fun printedWasteLocationDetail(location: PrintedWasteLocation): String {
    val parts = buildList {
        add(location.region)
        location.gpuTier?.let { add(it.label) }
        if (location.alternateCount > 0) {
            add(pluralStringResource(R.plurals.queue_location_servers, location.alternateCount + 1, location.alternateCount + 1))
        }
    }
    return parts.joinToString(" · ")
}

@Composable
private fun PrintedWasteLocationRow(
    location: PrintedWasteLocation,
    selected: Boolean,
    focused: Boolean,
    listFocused: Boolean,
    liveSelectedOutlines: Boolean,
    onClick: () -> Unit,
) {
    val zoneOption = location.primary
    val zone = zoneOption.zone
    val detail = printedWasteLocationDetail(location)
    Box(Modifier.fillMaxWidth()) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .focusProperties { canFocus = false }
                .clickable { onClick() },
            shape = RoundedCornerShape(12.dp),
            color = if (focused) {
                Color.White.copy(alpha = 0.16f)
            } else if (selected) {
                LocalSelectionTintColor.current.copy(alpha = 0.16f)
            } else {
                PanelAlt
            },
            tonalElevation = if (selected) 2.dp else 0.dp,
            border = if (selected && listFocused && !liveSelectedOutlines) {
                BorderStroke(2.dp, LocalSelectionTintColor.current)
            } else {
                null
            },
        ) {
            BoxWithConstraints(Modifier.fillMaxWidth()) {
                val compact = maxWidth < 520.dp
                val nameBlock: @Composable ColumnScope.() -> Unit = {
                    Text(
                        location.title,
                        fontWeight = FontWeight.Bold,
                        color = if (selected) LocalSelectionTintColor.current else TextPrimary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        detail,
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (compact) {
                    Column(
                        Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f), content = nameBlock)
                            if (selected) {
                                Text(
                                    stringResource(R.string.store_selector_selected),
                                    color = LocalSelectionTintColor.current,
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        }
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            PrintedWasteZoneMetrics(zoneOption)
                        }
                    }
                } else {
                    Row(
                        Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Column(Modifier.weight(1f), content = nameBlock)
                        PrintedWasteZoneMetrics(zoneOption)
                    }
                }
            }
        }
        ControllerFocusFrame(
            visible = shouldShowActiveSelectionOutline(selected, liveSelectedOutlines),
            cornerRadius = 12.dp,
            tint = LocalActiveSelectionColor.current,
            secondaryTint = LocalActiveSelectionSecondaryColor.current,
        )
    }
}

@Composable
private fun PrintedWasteZoneMetrics(zoneOption: PrintedWasteZoneOption) {
    val zone = zoneOption.zone
    QueueMetricPill(
        stringResource(R.string.stream_statusbar_metric_ping),
        zoneOption.pingMs?.let { "$it ms" } ?: "--",
        zoneOption.pingMs?.let(::pingColor) ?: TextMuted,
    )
    QueueMetricPill(
        stringResource(R.string.queue_metric_ahead),
        zone.QueuePosition.toString(),
        queueColor(zone.QueuePosition),
    )
    zone.eta?.let {
        QueueMetricPill(stringResource(R.string.queue_metric_wait), formatPrintedWasteWait(it))
    }
}

@Composable
private fun QueueMetricPill(
    label: String,
    value: String,
    valueColor: Color = TextPrimary,
) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = Color.Black.copy(alpha = 0.22f),
        border = if (LocalAbsoluteCinemaEffects.current) {
            BorderStroke(1.dp, LocalActiveSelectionColor.current.copy(alpha = 0.72f))
        } else {
            null
        },
    ) {
        Column(
            Modifier.padding(horizontal = 9.dp, vertical = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(label, color = TextMuted, style = MaterialTheme.typography.labelSmall)
            Text(value, color = valueColor, fontWeight = FontWeight.Bold, style = MaterialTheme.typography.labelMedium)
        }
    }
}

private fun queueColor(queue: Int): Color = when {
    queue <= 5 -> Green
    queue <= 20 -> Color(0xffc7ef6b)
    queue <= 45 -> Color(0xffffc95a)
    else -> Color(0xffff8d8d)
}

private fun pingColor(pingMs: Long): Color = when {
    pingMs <= 60L -> Green
    pingMs <= 120L -> Color(0xffc7ef6b)
    pingMs <= 180L -> Color(0xffffc95a)
    else -> Color(0xffff8d8d)
}

@Composable
internal fun ProviderPicker(providers: List<LoginProvider>, selected: LoginProvider, onSelect: (LoginProvider) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    BackHandler(enabled = expanded) { expanded = false }
    Box {
        OutlinedButton(onClick = { expanded = true }) { Text(selected.displayName) }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            providers.forEach { provider ->
                DropdownMenuItem(
                    text = { Text(provider.displayName) },
                    onClick = {
                        expanded = false
                        onSelect(provider)
                    },
                )
            }
        }
    }
}

private sealed interface UrlImageState {
    data object Empty : UrlImageState
    data object Loading : UrlImageState
    data object Failed : UrlImageState
    data object Loaded : UrlImageState
}

internal fun imageDataForSource(source: String): Any? {
    val key = source.trim()
    if (key.isBlank()) return null
    val uri = runCatching { Uri.parse(key) }.getOrNull() ?: return null
    val scheme = uri.scheme.orEmpty().lowercase(Locale.US)
    return when {
        scheme == "http" || scheme == "https" -> key
        scheme == "content" || scheme == "android.resource" || scheme == "file" -> uri
        scheme.isBlank() && key.startsWith("/") -> File(key)
        else -> uri
    }
}

@Composable
internal fun UrlImage(
    url: String?,
    modifier: Modifier = Modifier,
    fallbackUrl: String? = null,
    contentScale: ContentScale = ContentScale.Crop,
) {
    val source = url?.trim().orEmpty()
    val fallbackSource = fallbackUrl?.trim()?.takeIf { it.isNotBlank() && it != source }
    var activeSource by remember(source, fallbackSource) {
        mutableStateOf(source.takeIf { it.isNotBlank() } ?: fallbackSource)
    }
    var imageState by remember(source, fallbackSource) {
        mutableStateOf(if (activeSource == null) UrlImageState.Empty else UrlImageState.Loading)
    }
    val loadingTracker = LocalImageLoadingTracker.current
    val loading = imageState == UrlImageState.Loading
    val imageRequestsPaused = LocalCatalogImageRequestsPaused.current
    DisposableEffect(loadingTracker, loading) {
        if (loading) loadingTracker?.invoke(1)
        onDispose {
            if (loading) loadingTracker?.invoke(-1)
        }
    }
    val imageData = remember(activeSource) { activeSource?.let(::imageDataForSource) }
    LaunchedEffect(activeSource, imageData, fallbackSource, source) {
        if (activeSource == null) {
            imageState = UrlImageState.Empty
        } else if (imageData == null) {
            if (activeSource == source && fallbackSource != null) {
                activeSource = fallbackSource
                imageState = UrlImageState.Loading
            } else {
                imageState = UrlImageState.Failed
            }
        }
    }
    Box(modifier.background(OpenNowPalette.ImagePlaceholder), contentAlignment = Alignment.Center) {
        if (imageData != null && shouldStartCatalogImageRequest(imageRequestsPaused, imageState == UrlImageState.Loaded)) {
            key(activeSource) {
                AsyncImage(
                    model = imageData,
                    contentDescription = null,
                    modifier = Modifier.fillMaxSize(),
                    contentScale = contentScale,
                    onLoading = { imageState = UrlImageState.Loading },
                    onSuccess = { imageState = UrlImageState.Loaded },
                    onError = {
                        if (activeSource == source && fallbackSource != null) {
                            activeSource = fallbackSource
                            imageState = UrlImageState.Loading
                        } else {
                            imageState = UrlImageState.Failed
                        }
                    },
                )
            }
        }
        when (imageState) {
            UrlImageState.Loading -> LoadingShimmer(Modifier.fillMaxSize())
            UrlImageState.Loaded -> Unit
            UrlImageState.Empty,
            UrlImageState.Failed,
            -> OpenNowMark(42.dp)
        }
    }
}

@Composable
internal fun LoadingShimmer(modifier: Modifier = Modifier) {
    // Use the shared grid animation when available; the local fallback only runs while an
    // individual image placeholder is actually composed.
    // Using nullable avoids treating 0f (a valid animation start value) as "not provided".
    val animateLoading = LocalImageLoadingAnimationsEnabled.current && !LocalReduceMotion.current
    val sharedPulse = LocalTvLoadingPulse.current
    val localPulse = if (animateLoading && LocalTvLoadingProfile.current && sharedPulse == null) {
        val transition = rememberInfiniteTransition(label = "loading-pulse-local")
        val pulse = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 900, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "loading-pulse-local",
        )
        pulse
    } else {
        null
    }
    val pulse = sharedPulse ?: localPulse
    // Same rule as the shared driver above: no perpetual sweep under reduced motion.
    val shimmer = LocalShimmerOffset.current ?: if (pulse == null && animateLoading) run {
        val transition = rememberInfiniteTransition(label = "shimmer-local")
        val localOffset = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = SHIMMER_CYCLE_DURATION_MS, easing = LinearEasing),
            ),
            label = "shimmer-offset-local",
        )
        localOffset
    } else null
    val baseColor = OpenNowPalette.ShimmerBase
    val highlightColor1 = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.32f)
    val highlightColor2 = MaterialTheme.colorScheme.primary.copy(alpha = 0.18f)

    Spacer(
        modifier = modifier
            .background(baseColor)
            .drawBehind {
                if (pulse != null) {
                    drawRect(highlightColor1.copy(alpha = 0.08f + pulse.value * 0.18f))
                } else {
                    val width = size.width
                    val bandWidth = (width * 0.52f).coerceAtLeast(1f)
                    // Observe the transition for draw invalidations, but anchor the visible phase
                    // to device uptime so recreating an image loader cannot jump the band back.
                    val animationFrame = shimmer?.value
                    val bandStart = shimmerBandStartX(
                        progress = if (animationFrame == null) {
                            0f
                        } else {
                            shimmerProgressAtUptime(SystemClock.uptimeMillis())
                        },
                        containerWidth = width,
                        bandWidth = bandWidth,
                    )
                    // A horizontal shader gives the sweep exact bounds. At both repeat endpoints
                    // its transparent edge only touches the card, so Restart cannot paint a
                    // backward-moving frame on tall poster placeholders.
                    val brush = Brush.horizontalGradient(
                        colors = listOf(
                            Color.Transparent,
                            highlightColor1,
                            highlightColor2,
                            highlightColor1,
                            Color.Transparent,
                        ),
                        startX = bandStart,
                        endX = bandStart + bandWidth,
                    )
                    drawRect(brush)
                }
            }
    )
}

internal fun shimmerProgressAtUptime(uptimeMillis: Long): Float {
    val cycleDurationMs = SHIMMER_CYCLE_DURATION_MS.toLong()
    return Math.floorMod(uptimeMillis, cycleDurationMs).toFloat() / cycleDurationMs
}

internal fun shimmerBandStartX(progress: Float, containerWidth: Float, bandWidth: Float): Float {
    val safeWidth = containerWidth.coerceAtLeast(0f)
    val safeBandWidth = bandWidth.coerceAtLeast(1f)
    return -safeBandWidth +
        progress.coerceIn(0f, 1f) * (safeWidth + safeBandWidth)
}

@Composable
internal fun OpenNowMark(size: androidx.compose.ui.unit.Dp, modifier: Modifier = Modifier) {
    Image(
        painter = painterResource(R.drawable.opennow_logo_mark),
        contentDescription = "OpenNOW",
        modifier = modifier
            .width(size * 1.85f)
            .height(size),
        contentScale = ContentScale.Fit,
    )
}

@Composable
internal fun OpenNowAppIcon(
    size: androidx.compose.ui.unit.Dp,
    animate: Boolean = false,
) {
    val spin = if (animate) {
        val transition = rememberInfiniteTransition(label = "active-app-icon")
        transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 3_000, easing = LinearEasing),
            ),
            label = "active-app-icon-phase",
        )
    } else {
        null
    }
    val motionModifier = if (spin == null) {
        Modifier
    } else {
        // Read the animation state in the layer block so each frame only updates this GPU layer;
        // it does not remeasure the navigation rail or recompose the surrounding chrome.
        Modifier.graphicsLayer {
            val spinProgress = activeLogoSpinProgress(spin.value)
            val pulse = sin(spinProgress * PI.toFloat()).coerceAtLeast(0f)
            rotationY = spinProgress * 360f
            translationY = activeLogoFloatOffsetDp(spin.value).dp.toPx()
            cameraDistance = 12f * density
            scaleX = 1f + pulse * 0.035f
            scaleY = scaleX
        }
    }
    Image(
        painter = painterResource(R.drawable.opennow_icon),
        contentDescription = "OpenNOW",
        modifier = Modifier
            .size(size)
            .then(motionModifier),
        contentScale = ContentScale.Fit,
    )
}

internal fun activeLogoSpinProgress(cycleProgress: Float): Float =
    ((cycleProgress.coerceIn(0f, 1f) - 0.30f) / 0.18f).coerceIn(0f, 1f)

internal fun activeLogoFloatOffsetDp(cycleProgress: Float): Float =
    sin(cycleProgress.coerceIn(0f, 1f) * 2f * PI.toFloat()) * 2.5f

internal val ColorQuality.label: String
    get() = when (this) {
        ColorQuality.EightBit420 -> "8-bit 4:2:0"
        ColorQuality.EightBit444 -> "8-bit 4:4:4"
        ColorQuality.TenBit420 -> "10-bit 4:2:0"
        ColorQuality.TenBit444 -> "10-bit 4:4:4"
    }

internal val GameCardOverlayGradient = Brush.verticalGradient(
    colors = listOf(Color.Transparent, Color.Transparent, Color.Black.copy(alpha = 0.95f))
)
