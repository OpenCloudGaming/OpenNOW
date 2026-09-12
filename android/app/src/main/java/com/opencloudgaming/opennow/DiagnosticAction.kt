package com.opencloudgaming.opennow

import kotlinx.coroutines.CancellationException

/** Includes the Android UI operation, not just preparation of its diagnostic text. */
internal suspend fun runDiagnosticAction(
    onFailure: (Exception) -> Unit,
    onFinished: () -> Unit = {},
    action: suspend () -> Unit,
) {
    try {
        action()
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        onFailure(error)
    } finally {
        onFinished()
    }
}
