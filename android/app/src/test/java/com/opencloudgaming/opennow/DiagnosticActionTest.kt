package com.opencloudgaming.opennow

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.fail
import org.junit.Test

class DiagnosticActionTest {
    @Test
    fun androidOperationFailureAfterPreparationIsReportedAndClearsBusyState() = runBlocking {
        val failure = RuntimeException("Clipboard or document picker unavailable")
        val events = mutableListOf<String>()
        runDiagnosticAction(
            onFailure = { assertSame(failure, it); events += "failure" },
            onFinished = { events += "finished" },
        ) {
            yield()
            events += "prepared"
            throw failure
        }
        assertEquals(listOf("prepared", "failure", "finished"), events)
    }

    @Test
    fun cancellationPropagatesAndStillClearsBusyState() = runBlocking {
        val cancellation = CancellationException("Panel closed")
        var finished = 0
        try {
            runDiagnosticAction(
                onFailure = { fail("Cancellation was reported as an action error") },
                onFinished = { finished++ },
            ) { throw cancellation }
            fail("Cancellation was swallowed")
        } catch (error: CancellationException) {
            assertSame(cancellation, error)
        }
        assertEquals(1, finished)
    }

    @Test
    fun successfulActionCompletesBeforeCleanup() = runBlocking {
        val events = mutableListOf<String>()
        runDiagnosticAction(
            onFailure = { fail("Successful action reported an error") },
            onFinished = { events += "finished" },
        ) { events += "action" }
        assertEquals(listOf("action", "finished"), events)
    }
}
