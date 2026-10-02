package com.opencloudgaming.opennow

import android.view.KeyEvent
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt

private val DeckBackground = Color(0xFF0B0F1A)
private val DeckTile = Color.White.copy(alpha = 0.06f)
private val DeckTileStrong = Color.White.copy(alpha = 0.12f)
private val DeckSeam = Color.White.copy(alpha = 0.14f)
private val DeckMuted = Color.White.copy(alpha = 0.6f)
private val DeckSky = Color(0xFF7FD4FF)
private val DeckMint = Color(0xFF6EE7B7)
private val DeckYellow = Color(0xFFFFD166)
private val DeckCoral = Color(0xFFFF8A80)

private const val HOLD_TO_END_MS = 1_200
private const val TRACKPAD_TAP_MAX_MS = 250L
private const val TRACKPAD_WHEEL_STEP_PX = 36f
private const val TRACKPAD_WHEEL_DELTA = 120

private enum class StreamPanel { Deck, Trackpad, Keyboard, Off }

@Composable
fun DualScreenDeck(onAction: (DualScreenAction) -> Unit) {
    val snapshot by DualScreenBridge.snapshot.collectAsState()
    val hosted = DualScreenBridge.hostedContent.value
    val currentSnapshot = snapshot
    if (hosted != null && currentSnapshot != null) {
        // Top-screen UI moved here (Stream Controls, store and server pickers). It needs the
        // app theme because it is the exact composable the top screen would otherwise draw.
        OpenNowTheme(settings = currentSnapshot.settings, physicalControllerConnected = false) {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density * hosted.scale, density.fontScale)) {
                Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
                    hosted.content()
                }
            }
        }
        return
    }
    Box(
        Modifier
            .fillMaxSize()
            .background(DeckBackground)
            .padding(12.dp),
    ) {
        val current = snapshot
        when (current?.phase) {
            null, DualScreenPhase.SignedOut -> SignedOutDeck()
            DualScreenPhase.Browse -> when {
                current.selectedGame != null -> GameActionsDeck(current, current.selectedGame, onAction)
                current.page == AppPage.Settings -> SettingsInspectorDeck(current, onAction)
                else -> BrowseDeck(current, onAction)
            }
            DualScreenPhase.Launching -> LaunchingDeck(current, onAction)
            DualScreenPhase.Streaming -> StreamingDeck(current, onAction)
        }
    }
}

@Composable
private fun SignedOutDeck() {
    Column(
        Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("OpenNOW", color = Color.White, fontSize = 30.sp, fontWeight = FontWeight.Black)
        Spacer(Modifier.height(8.dp))
        Text(stringResource(R.string.dual_signed_out_title), color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.Bold)
        Text(
            stringResource(R.string.dual_signed_out_body),
            color = DeckMuted,
            fontSize = 13.sp,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun BrowseDeck(snapshot: DualScreenSnapshot, onAction: (DualScreenAction) -> Unit) {
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
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (!typing) AccountHeader(snapshot)
        SearchField(
            query = query,
            hint = stringResource(if (libraryPage) R.string.dual_search_library_hint else R.string.dual_search_store_hint),
            active = typing,
            onTap = { typing = !typing },
            onClear = { search("") },
        )
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
            )
        } else {
            PresetSelector(snapshot.streamPreset, onAction)
            Spacer(Modifier.weight(1f))
            DeckTabBar(snapshot.page, onAction)
        }
    }
}

@Composable
private fun AccountHeader(snapshot: DualScreenSnapshot) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(DeckTile)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        val name = snapshot.accountName.orEmpty()
        Box(
            Modifier.size(32.dp).clip(CircleShape).background(Color(0xFFA78BFA)),
            contentAlignment = Alignment.Center,
        ) {
            Text(name.take(1).uppercase(Locale.getDefault()), color = DeckBackground, fontWeight = FontWeight.Black)
        }
        Column(Modifier.weight(1f)) {
            Text(name, color = Color.White, fontWeight = FontWeight.Bold, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            snapshot.membershipTier?.let { tier ->
                Text(tier.lowercase(Locale.getDefault()).replaceFirstChar { it.titlecase(Locale.getDefault()) }, color = DeckYellow, fontSize = 12.sp, fontWeight = FontWeight.Bold)
            }
        }
        val hours = when {
            snapshot.unlimitedHours -> stringResource(R.string.dual_unlimited)
            snapshot.remainingHours != null -> stringResource(
                R.string.dual_hours_left,
                String.format(Locale.getDefault(), "%.1f", snapshot.remainingHours),
            )
            else -> null
        }
        hours?.let { Text(it, color = DeckMuted, fontSize = 12.sp, fontWeight = FontWeight.Bold) }
    }
}

@Composable
private fun SearchField(query: String, hint: String, active: Boolean, onTap: () -> Unit, onClear: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(44.dp)
            .clip(CircleShape)
            .background(Color.White.copy(alpha = 0.08f))
            .border(if (active) 2.dp else 1.dp, if (active) DeckSky else DeckSeam, CircleShape)
            .clickable(onClick = onTap)
            .padding(start = 14.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(R.drawable.ic_search), contentDescription = null, tint = Color.White, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(10.dp))
        Text(
            text = query.ifEmpty { hint },
            color = if (query.isEmpty()) DeckMuted else Color.White,
            fontWeight = FontWeight.Bold,
            fontSize = 15.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (query.isNotEmpty()) {
            DeckChip(stringResource(R.string.dual_search_clear), onClick = onClear)
        }
    }
}

@Composable
private fun PresetSelector(current: StreamPreset, onAction: (DualScreenAction) -> Unit) {
    val presets = listOf(
        StreamPreset.LowDataSaver to R.string.dual_preset_data_saver,
        StreamPreset.Medium to R.string.dual_preset_medium,
        StreamPreset.Recommended to R.string.dual_preset_recommended,
        StreamPreset.High to R.string.dual_preset_high,
    )
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        DeckLabel(R.string.dual_stream_preset)
        Segmented(
            options = presets.map { stringResource(it.second) },
            selected = presets.indexOfFirst { it.first == current },
            onSelect = { onAction(DualScreenAction.ApplyPreset(presets[it].first)) },
        )
    }
}

@Composable
private fun DeckTabBar(page: AppPage, onAction: (DualScreenAction) -> Unit) {
    val tabs = listOf(
        Triple(AppPage.Home, R.drawable.ic_tab_store, R.string.nav_store),
        Triple(AppPage.Library, R.drawable.ic_tab_library, R.string.nav_library),
        Triple(AppPage.Settings, R.drawable.ic_tab_settings, R.string.nav_settings),
    )
    Row(
        Modifier
            .fillMaxWidth()
            .height(60.dp)
            .clip(CircleShape)
            .background(Color(0xD10E1018))
            .border(1.dp, DeckSeam, CircleShape)
            .padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        tabs.forEach { (target, icon, label) ->
            DeckTab(selected = page == target, icon = icon, label = label) {
                onAction(DualScreenAction.Navigate(target))
            }
        }
    }
}

@Composable
private fun RowScope.DeckTab(selected: Boolean, @DrawableRes icon: Int, @StringRes label: Int, onClick: () -> Unit) {
    Column(
        Modifier
            .weight(1f)
            .fillMaxHeight()
            .clip(CircleShape)
            .background(if (selected) DeckTileStrong else Color.Transparent)
            .clickable(onClick = onClick),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = Color.White, modifier = Modifier.size(22.dp))
        Text(stringResource(label), color = if (selected) Color.White else DeckMuted, fontSize = 11.sp, fontWeight = FontWeight.Black)
    }
}

@Composable
private fun LaunchingDeck(snapshot: DualScreenSnapshot, onAction: (DualScreenAction) -> Unit) {
    Column(
        Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(Modifier.weight(1f))
        Text(
            stringResource(R.string.dual_launching_title, snapshot.gameTitle.orEmpty()),
            color = Color.White,
            fontSize = 20.sp,
            fontWeight = FontWeight.Black,
            textAlign = TextAlign.Center,
        )
        snapshot.queuePosition?.takeIf { it > 0 }?.let { position ->
            Text(
                stringResource(R.string.dual_queue_position, position),
                color = DeckYellow,
                fontSize = 34.sp,
                fontWeight = FontWeight.Black,
            )
        }
        if (snapshot.launchPhase.isNotBlank()) {
            Text(snapshot.launchPhase, color = DeckMuted, fontSize = 14.sp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center)
        }
        Spacer(Modifier.weight(1f))
        DeckButton(stringResource(R.string.dual_cancel_launch), modifier = Modifier.fillMaxWidth()) {
            onAction(DualScreenAction.CancelLaunch)
        }
    }
}

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
    when (panel) {
        StreamPanel.Deck -> StreamDeckPanel(snapshot, onAction) { panel = it }
        StreamPanel.Trackpad -> TrackpadPanel { panel = StreamPanel.Deck }
        StreamPanel.Keyboard -> Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            DeckKeyboard(
                modifier = Modifier.weight(1f),
                onText = NativeStreamInputRouter::sendCompanionText,
                onKey = NativeStreamInputRouter::sendCompanionKey,
                extraKeys = listOf("Esc" to KeyEvent.KEYCODE_ESCAPE, "Tab" to KeyEvent.KEYCODE_TAB),
            )
            DeckButton(stringResource(R.string.dual_action_back), modifier = Modifier.fillMaxWidth()) { panel = StreamPanel.Deck }
        }
        StreamPanel.Off -> Box(
            Modifier
                .fillMaxSize()
                .background(Color.Black)
                .clickable { panel = StreamPanel.Deck },
            contentAlignment = Alignment.BottomCenter,
        ) {
            Text(stringResource(R.string.dual_wake_hint), color = Color.White.copy(alpha = 0.25f), fontSize = 12.sp)
        }
    }
}

@Composable
private fun StreamDeckPanel(
    snapshot: DualScreenSnapshot,
    onAction: (DualScreenAction) -> Unit,
    openPanel: (StreamPanel) -> Unit,
) {
    val stats = snapshot.stats
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            snapshot.gameTitle.orEmpty(),
            color = Color.White,
            fontSize = 16.sp,
            fontWeight = FontWeight.Black,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            StatTile(R.string.dual_stat_ping, stats?.pingMs?.toString(), pingColor(stats?.pingMs))
            StatTile(
                R.string.dual_stat_fps,
                (stats?.fps ?: stats?.decodedFps)?.toString(),
                Color.White,
                labelSuffix = snapshot.targetFps?.let { "/ $it" },
            )
            StatTile(R.string.dual_stat_bitrate, stats?.bitrateKbps?.let { (it / 1000.0).roundToInt().toString() }, Color.White)
            StatTile(
                R.string.dual_stat_loss,
                stats?.packetLossPct?.let { String.format(Locale.getDefault(), "%.1f", it) },
                if ((stats?.packetLossPct ?: 0.0) >= 1.0) DeckCoral else DeckMint,
            )
        }
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            DeckAction(R.string.dual_action_stream_menu, DeckSky, Modifier.weight(1f)) { onAction(DualScreenAction.OpenStreamMenu) }
            DeckAction(R.string.dual_action_stats, Color(0xFFA78BFA), Modifier.weight(1f)) { onAction(DualScreenAction.ToggleStatsOverlay) }
            DeckAction(R.string.dual_action_keyboard, DeckYellow, Modifier.weight(1f)) { openPanel(StreamPanel.Keyboard) }
        }
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            DeckAction(R.string.dual_action_trackpad, DeckMint, Modifier.weight(1f)) { openPanel(StreamPanel.Trackpad) }
            DeckAction(R.string.dual_action_deck_off, Color.White.copy(alpha = 0.7f), Modifier.weight(1f)) { openPanel(StreamPanel.Off) }
        }
        HoldToEndButton { onAction(DualScreenAction.EndSession) }
    }
}

private fun pingColor(ping: Int?): Color = when {
    ping == null -> Color.White
    ping <= 40 -> DeckMint
    ping <= 80 -> DeckYellow
    else -> DeckCoral
}

@Composable
private fun RowScope.StatTile(@StringRes label: Int, value: String?, color: Color, labelSuffix: String? = null) {
    Column(
        Modifier
            .weight(1f)
            .clip(RoundedCornerShape(14.dp))
            .background(DeckTile)
            .padding(horizontal = 10.dp, vertical = 6.dp),
    ) {
        Text(
            listOfNotNull(stringResource(label).uppercase(Locale.getDefault()), labelSuffix).joinToString(" "),
            color = DeckMuted,
            fontSize = 10.sp,
            fontWeight = FontWeight.Black,
            maxLines = 1,
        )
        Text(value ?: "–", color = color, fontSize = 20.sp, fontWeight = FontWeight.Black, maxLines = 1)
    }
}

@Composable
private fun DeckAction(@StringRes label: Int, accent: Color, modifier: Modifier, onClick: () -> Unit) {
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
        Box(Modifier.size(14.dp).clip(CircleShape).background(accent))
        Text(stringResource(label), color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Black)
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
            .height(48.dp)
            .clip(CircleShape)
            .border(2.dp, DeckCoral, CircleShape)
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
        Box(
            Modifier
                .align(Alignment.CenterStart)
                .fillMaxHeight()
                .fillMaxWidth(progress.value)
                .background(DeckCoral.copy(alpha = 0.3f)),
        )
        Text(
            stringResource(if (progress.value > 0f) R.string.dual_keep_holding else R.string.dual_hold_to_end),
            color = DeckCoral,
            fontSize = 14.sp,
            fontWeight = FontWeight.Black,
        )
    }
}

/**
 * Relative trackpad: one finger moves the remote cursor, a quick tap clicks, a two-finger tap
 * right-clicks, and a two-finger drag scrolls. Everything goes through the stream's touch-mouse
 * path, so no button stays held when the gesture ends.
 */
@Composable
private fun TrackpadPanel(onBack: () -> Unit) {
    val touchSlop = LocalViewConfiguration.current.touchSlop
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .clip(RoundedCornerShape(20.dp))
                .background(Color.White.copy(alpha = 0.04f))
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
            Text(
                stringResource(R.string.dual_trackpad_hint),
                color = Color.White.copy(alpha = 0.4f),
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(10.dp),
            )
        }
        Row(Modifier.height(48.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            DeckButton(stringResource(R.string.dual_left_click), Modifier.weight(1f)) { NativeStreamInputRouter.sendCompanionClick(secondary = false) }
            DeckButton(stringResource(R.string.dual_right_click), Modifier.weight(1f)) { NativeStreamInputRouter.sendCompanionClick(secondary = true) }
            DeckButton("Esc", Modifier.width(64.dp)) { NativeStreamInputRouter.sendCompanionKey(KeyEvent.KEYCODE_ESCAPE) }
            DeckButton(stringResource(R.string.dual_action_back), Modifier.width(80.dp), onClick = onBack)
        }
    }
}

/** Touch keyboard shared by Store search and in-stream typing. Never requests IME focus. */
@Composable
private fun DeckKeyboard(
    modifier: Modifier,
    onText: (String) -> Unit,
    onKey: (Int) -> Unit,
    extraKeys: List<Pair<String, Int>> = emptyList(),
) {
    var shifted by remember { mutableStateOf(false) }
    val rows = listOf("1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm")
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(5.dp)) {
        rows.forEachIndexed { index, row ->
            Row(Modifier.weight(1f).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                if (index == 3) {
                    KeyCap("⇧", Modifier.weight(1.5f), highlighted = shifted) { shifted = !shifted }
                }
                row.forEach { char ->
                    val text = if (shifted) char.uppercaseChar().toString() else char.toString()
                    KeyCap(text, Modifier.weight(1f)) {
                        onText(text)
                        if (shifted) shifted = false
                    }
                }
                if (index == 3) {
                    KeyCap("⌫", Modifier.weight(1.5f)) { onKey(KeyEvent.KEYCODE_DEL) }
                }
            }
        }
        Row(Modifier.weight(1f).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            extraKeys.forEach { (label, keyCode) ->
                KeyCap(label, Modifier.weight(1.2f)) { onKey(keyCode) }
            }
            KeyCap(stringResource(R.string.dual_key_space), Modifier.weight(4f)) { onText(" ") }
            KeyCap(stringResource(R.string.dual_key_enter), Modifier.weight(2f), highlighted = true) { onKey(KeyEvent.KEYCODE_ENTER) }
        }
    }
}

@Composable
private fun KeyCap(label: String, modifier: Modifier, highlighted: Boolean = false, onClick: () -> Unit) {
    Box(
        modifier
            .fillMaxHeight()
            .clip(RoundedCornerShape(10.dp))
            .background(if (highlighted) DeckSky else Color.White.copy(alpha = 0.11f))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = if (highlighted) DeckBackground else Color.White, fontSize = 16.sp, fontWeight = FontWeight.Black, maxLines = 1)
    }
}

@Composable
private fun Segmented(options: List<String>, selected: Int, onSelect: (Int) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(44.dp)
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
                Text(
                    label,
                    color = if (index == selected) DeckBackground else Color.White,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Black,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun DeckLabel(@StringRes label: Int) {
    Text(
        stringResource(label).uppercase(Locale.getDefault()),
        color = DeckMuted,
        fontSize = 11.sp,
        fontWeight = FontWeight.Black,
        modifier = Modifier.padding(start = 4.dp),
    )
}

@Composable
private fun DeckChip(label: String, onClick: () -> Unit) {
    Box(
        Modifier
            .height(32.dp)
            .clip(CircleShape)
            .background(DeckTileStrong)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Black)
    }
}

@Composable
private fun DeckButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Box(
        modifier
            .height(48.dp)
            .clip(CircleShape)
            .background(Color.White.copy(alpha = 0.08f))
            .border(1.dp, DeckSeam, CircleShape)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Black, maxLines = 1)
    }
}

/** Actions for the game whose details sheet is open on the top screen. */
@Composable
private fun GameActionsDeck(snapshot: DualScreenSnapshot, game: GameInfo, onAction: (DualScreenAction) -> Unit) {
    val favorite = game.id in snapshot.settings.favoriteGameIds
    val stores = game.variants.map { it.store }.distinct()
    val haptics = LocalHapticFeedback.current
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                game.title,
                color = Color.White,
                fontSize = 20.sp,
                fontWeight = FontWeight.Black,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            DeckChip(stringResource(R.string.dual_close)) { onAction(DualScreenAction.CloseGameDetails) }
        }
        Box(
            Modifier
                .fillMaxWidth()
                .height(72.dp)
                .clip(RoundedCornerShape(24.dp))
                .background(Color.White)
                .pointerInput(game.id) {
                    detectTapGestures(
                        onTap = { onAction(DualScreenAction.Play(game)) },
                        onLongPress = {
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            onAction(DualScreenAction.ChooseStore(game))
                        },
                    )
                },
            contentAlignment = Alignment.Center,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(stringResource(R.string.dual_play), color = DeckBackground, fontSize = 22.sp, fontWeight = FontWeight.Black)
                if (stores.size > 1) {
                    Text(stringResource(R.string.dual_play_hold_hint), color = DeckBackground.copy(alpha = 0.6f), fontSize = 11.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
        Row(Modifier.height(48.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            DeckButton(
                stringResource(if (favorite) R.string.dual_favorited else R.string.dual_favorite),
                Modifier.weight(1f),
            ) { onAction(DualScreenAction.ToggleFavorite(game.id)) }
            if (stores.size > 1) {
                DeckButton(stringResource(R.string.dual_choose_store), Modifier.weight(1f)) { onAction(DualScreenAction.ChooseStore(game)) }
            }
        }
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .clip(RoundedCornerShape(18.dp))
                .background(DeckTile)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 12.dp, vertical = 6.dp),
        ) {
            game.publisherName?.let { DetailLine(R.string.dual_detail_publisher, it) }
            game.membershipTierLabel?.let { DetailLine(R.string.dual_detail_membership, it) }
            if (stores.isNotEmpty()) DetailLine(R.string.dual_detail_stores, stores.joinToString(" · "))
            if (game.genres.isNotEmpty()) {
                Text(game.genres.joinToString(" · "), color = DeckMuted, fontSize = 12.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(vertical = 6.dp))
            }
            (game.description ?: game.longDescription)?.let {
                Text(it, color = Color.White.copy(alpha = 0.75f), fontSize = 13.sp, maxLines = 6, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun DetailLine(@StringRes label: Int, value: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), horizontalArrangement = Arrangement.SpaceBetween) {
        Text(stringResource(label), color = DeckMuted, fontSize = 13.sp, fontWeight = FontWeight.Bold)
        Spacer(Modifier.width(12.dp))
        Text(value, color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Black, textAlign = TextAlign.End, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Big touch controls for whichever setting has controller focus on the top screen. */
@Composable
private fun SettingsInspectorDeck(snapshot: DualScreenSnapshot, onAction: (DualScreenAction) -> Unit) {
    val inspection = DualScreenBridge.inspection.value?.second
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .clip(RoundedCornerShape(22.dp))
                .background(DeckTile)
                .border(1.dp, if (inspection != null) DeckSky.copy(alpha = 0.5f) else DeckSeam, RoundedCornerShape(22.dp))
                .padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (inspection == null) {
                Spacer(Modifier.weight(1f))
                Text(stringResource(R.string.dual_settings_hint_title), color = Color.White, fontSize = 17.sp, fontWeight = FontWeight.Black)
                Text(stringResource(R.string.dual_settings_hint_body), color = DeckMuted, fontSize = 13.sp)
                Spacer(Modifier.weight(1f))
            } else {
                Text(inspection.label, color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Black)
                inspection.description?.takeIf { it.isNotBlank() }?.let {
                    Text(it, color = DeckMuted, fontSize = 13.sp, maxLines = 4, overflow = TextOverflow.Ellipsis)
                }
                when (inspection) {
                    is SettingInspection.Switch -> Segmented(
                        options = listOf(stringResource(R.string.dual_switch_off), stringResource(R.string.dual_switch_on)),
                        selected = if (inspection.checked) 1 else 0,
                        onSelect = { index -> if (inspection.enabled) inspection.onCheckedChange(index == 1) },
                    )
                    is SettingInspection.Choice -> Column(
                        Modifier.weight(1f).verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        inspection.options.forEach { option ->
                            val selected = option.label == inspection.selectedLabel
                            Row(
                                Modifier
                                    .fillMaxWidth()
                                    .height(48.dp)
                                    .clip(RoundedCornerShape(14.dp))
                                    .background(if (selected) Color.White else Color.White.copy(alpha = 0.06f))
                                    .clickable(enabled = option.enabled) { inspection.onSelect(option.value) }
                                    .padding(horizontal = 14.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    option.label,
                                    color = when {
                                        selected -> DeckBackground
                                        option.enabled -> Color.White
                                        else -> DeckMuted
                                    },
                                    fontSize = 15.sp,
                                    fontWeight = FontWeight.Black,
                                    modifier = Modifier.weight(1f),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                option.badge?.let {
                                    Text(it, color = if (selected) DeckBackground.copy(alpha = 0.6f) else DeckMuted, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                                }
                            }
                        }
                    }
                    is SettingInspection.Slider -> SliderInspector(inspection)
                }
            }
        }
        DeckTabBar(snapshot.page, onAction)
    }
}

@Composable
private fun SliderInspector(inspection: SettingInspection.Slider) {
    val stepCount = if (inspection.step > 0f) ((inspection.max - inspection.min) / inspection.step).roundToInt() else 0
    fun nudge(direction: Int) {
        val next = (inspection.value + direction * inspection.step).coerceIn(inspection.min, inspection.max)
        inspection.onChange(next)
    }
    Text(inspection.valueText, color = DeckSky, fontSize = 34.sp, fontWeight = FontWeight.Black)
    Slider(
        value = inspection.value,
        onValueChange = inspection.onChange,
        valueRange = inspection.min..inspection.max,
        steps = if (stepCount in 2..200) stepCount - 1 else 0,
        colors = SliderDefaults.colors(
            thumbColor = Color.White,
            activeTrackColor = DeckSky,
            inactiveTrackColor = Color.White.copy(alpha = 0.14f),
            activeTickColor = Color.Transparent,
            inactiveTickColor = Color.Transparent,
        ),
    )
    Row(Modifier.height(48.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        DeckButton("−", Modifier.weight(1f)) { nudge(-1) }
        DeckButton("+", Modifier.weight(1f)) { nudge(1) }
    }
}
