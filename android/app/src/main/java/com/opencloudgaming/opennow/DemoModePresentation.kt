package com.opencloudgaming.opennow

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf

internal val LocalDemoMode = staticCompositionLocalOf { false }

@Composable
internal fun rememberDemoPresentation(state: OpenNowUiState): OpenNowUiState {
    if (!state.settings.demoMode) return state

    // Session/queue updates must not rebuild the combined catalog on every state emission.
    val library = remember(
        IdentityKey(state.games), IdentityKey(state.catalogResult.games),
        IdentityKey(state.newlyAddedGames), IdentityKey(state.libraryGames),
    ) {
        demoLibraryGames(state)
    }
    return remember(IdentityKey(state), IdentityKey(library)) { state.forDemoPresentation(library) }
}
