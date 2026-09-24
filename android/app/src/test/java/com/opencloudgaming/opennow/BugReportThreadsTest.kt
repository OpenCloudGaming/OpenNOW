package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BugReportThreadsTest {
    @Test
    fun parsesPrivateReporterInboxWithStatusAndConversation() {
        val reports = parseAndroidBugReportThreads(
            """
            {
              "ok": true,
              "data": {
                "reports": [{
                  "id": "report-123",
                  "title": "Stream froze",
                  "description": "Video stopped after reconnecting.",
                  "status": "planned_next_update",
                  "resolutionNote": "A fix is queued for the next update.",
                  "versionName": "1.7.9",
                  "versionCode": "135",
                  "kind": "performance",
                  "area": "streaming",
                  "frequency": "sometimes",
                  "impact": "high",
                  "files": [{"name":"opennow.log"},{"name":"freeze.mp4"}],
                  "createdAt": 100,
                  "updatedAt": 300,
                  "comments": [
                    {"id":"one","authorRole":"reporter","kind":"comment","body":"I can reproduce this.","createdAt":200},
                    {"id":"two","authorRole":"admin","kind":"status","body":"Status changed.","createdAt":300}
                  ]
                }]
              }
            }
            """.trimIndent(),
        )

        assertEquals(1, reports.size)
        assertEquals("report-123", reports.single().id)
        assertEquals("planned_next_update", reports.single().status)
        assertEquals("A fix is queued for the next update.", reports.single().resolutionNote)
        assertEquals("performance", reports.single().kind)
        assertEquals(listOf("opennow.log", "freeze.mp4"), reports.single().files)
        assertEquals(listOf("reporter", "admin"), reports.single().comments.map { it.authorRole })
        assertEquals(listOf("comment", "status"), reports.single().comments.map { it.kind })
    }

    @Test
    fun completedAndDeclinedReportsAreClosed() {
        assertTrue(androidBugReportThreadClosed("completed"))
        assertTrue(androidBugReportThreadClosed("not_reproducible"))
        assertTrue(androidBugReportThreadClosed("wont_fix"))
        assertTrue(androidBugReportThreadClosed("closed_by_reporter"))
        assertTrue(!androidBugReportThreadClosed("needs_info"))
    }

    @Test
    fun malformedInboxDoesNotExposePartialServerPayloads() {
        val error = runCatching { parseAndroidBugReportThreads("<html>proxy error</html>") }.exceptionOrNull()
        assertTrue(error is AndroidBugReportUploadException)
        assertEquals("INVALID_RESPONSE", (error as AndroidBugReportUploadException).serverCode)
        assertTrue(parseAndroidBugReportThreads("{\"data\":{\"reports\":[{}]}}").isEmpty())
    }
}
