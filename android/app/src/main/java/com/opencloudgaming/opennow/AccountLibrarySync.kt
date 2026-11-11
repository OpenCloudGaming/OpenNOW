package com.opencloudgaming.opennow

private data class LinkedLibrarySnapshot(
    val userIdentifier: String?,
    val userDisplayName: String?,
    val syncedGameCount: Int?,
    val syncState: String?,
    val syncDate: String?,
)

/** Bounded post-link polling also refreshes library membership, even without sync metadata. */
internal suspend fun refreshLibraryAfterAccountLink(
    store: String,
    initialConnectors: List<AccountConnector>,
    isCurrentScope: () -> Boolean,
    pause: suspend (Long) -> Unit,
    loadConnectors: suspend () -> List<AccountConnector>?,
    refreshLibrary: suspend () -> Unit,
) {
    fun List<AccountConnector>.linkedStore() = firstOrNull {
        normalizeGameStore(it.store) == normalizeGameStore(store) && it.isLinked
    }?.let { LinkedLibrarySnapshot(it.userIdentifier, it.userDisplayName, it.syncedGameCount, it.syncState, it.syncDate) }
    var previous = initialConnectors.linkedStore()
    repeat(6) { attempt ->
        pause(if (attempt == 0) 5_000L else 10_000L)
        if (!isCurrentScope()) return
        val connectors = loadConnectors() ?: return@repeat
        if (!isCurrentScope()) return
        val linked = connectors.linkedStore()
        // Some providers report only linking data. Refresh again at the end so a delayed
        // library sync is not missed merely because its status/count/date were absent.
        if (linked != null && (linked != previous || attempt == 0 || attempt == 5)) {
            refreshLibrary()
        }
        previous = linked
    }
}
