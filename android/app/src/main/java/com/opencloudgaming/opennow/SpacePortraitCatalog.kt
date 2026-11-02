package com.opencloudgaming.opennow

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.opencloudgaming.opennow.ui.theme.LocalReduceForegroundMotion

/** Library grid fallback for compact multi-window configurations. Discover stays a horizontal rail. */
@Composable
internal fun SpacePortraitCatalog(
    state: OpenNowUiState,
    games: List<GameInfo>,
    selectedGame: GameInfo?,
    gridState: LazyGridState,
    selectedRequester: FocusRequester,
    modifier: Modifier,
    onFocused: (String) -> Unit,
    onSelectGame: (GameInfo) -> Unit,
    onRefresh: () -> Unit,
    catalogControls: @Composable () -> Unit,
    libraryContent: @Composable () -> Unit,
    onResumeSession: () -> Unit,
) {
    val reduceMotion = LocalReduceForegroundMotion.current
    val library = state.page == AppPage.Library
    LazyVerticalGrid(columns = if (library) GridCells.Adaptive(132.dp) else GridCells.Fixed(2), state = gridState,
        modifier = modifier.fillMaxWidth().testTag(if (library) "space-library-grid" else "space-portrait-discover"),
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 20.dp),
        horizontalArrangement = Arrangement.spacedBy(if (library) 0.dp else 14.dp),
        verticalArrangement = Arrangement.spacedBy(if (library) 0.dp else 16.dp)) {
        item(key = "portrait-heading", span = { GridItemSpan(maxLineSpan) }) {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                if (state.activeSession != null) SpaceControl(stringResource(R.string.action_resume), onClick = onResumeSession)
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(if (library) R.string.nav_library else R.string.space_ui_discover),
                            color = SpaceWhite, fontSize = 22.sp, fontWeight = FontWeight.Medium)
                        if (library) Text(stringResource(R.string.library_count, games.size), color = SpaceMuted, fontSize = 12.sp)
                    }
                    catalogControls()
                }
                if (library) libraryContent()
                state.error?.let { Text(it, color = Color(0xFFFFB5A9), maxLines = 3) }
                if (games.isEmpty()) {
                    if (state.loadingGames) CircularProgressIndicator(Modifier.size(24.dp), color = SpaceIce)
                    else {
                        Text(stringResource(R.string.no_games_loaded), color = SpaceMuted)
                        SpaceControl(stringResource(R.string.space_ui_refresh), onClick = onRefresh)
                    }
                }
            }
        }
        items(games, key = { it.id }) { game ->
            SpaceGameTile(game, selectedGame?.id == game.id, compact = false, reduceMotion = reduceMotion,
                showPremiumMarker = state.settings.showPremiumMarker,
                modifier = Modifier.fillMaxWidth().then(
                    if (selectedGame?.id == game.id) Modifier.focusRequester(selectedRequester) else Modifier),
                onFocused = { onFocused(game.id) }, onClick = { onSelectGame(game) },
                portraitArtwork = !library, fillWidth = true)
        }
    }
}
