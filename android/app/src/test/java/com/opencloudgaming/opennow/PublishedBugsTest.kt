package com.opencloudgaming.opennow

import java.util.UUID
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PublishedBugsTest {
    private val reporter = androidBugReportReporterId("published-bugs-test")
    private val bugJson = """{"id":"one","title":"Video freezes","body":"**Known issue**","status":"awaiting_review","progress":85,"fixedVersion":"2.3.3","votes":12,"voted":true,"logCount":4,"updatedAt":123} """.trim()

    @Test
    fun parsesPublishedStatusProgressAndPersonalVote() {
        val page = parseAndroidPublishedBugPage("""{"data":{"bugs":[$bugJson],"nextCursor":"one"}}""")
        val bug = page.bugs.single()
        assertEquals("awaiting_review", bug.status)
        assertEquals(85, bug.progress)
        assertTrue(bug.voted)
        assertEquals(12, bug.votes)
        assertEquals("one", page.nextCursor)
        assertEquals("**Known issue**", bug.body)
        assertEquals(null, parseAndroidPublishedBugPage("""{"data":{"bugs":[],"nextCursor":null}}""").nextCursor)
    }

    @Test
    fun logCollectionDefaultsOpenForOlderServersAndHonorsPausedPosts() {
        val open = parseAndroidPublishedBugPage("""{"data":{"bugs":[$bugJson],"nextCursor":null}}""").bugs.single()
        assertTrue(open.acceptingLogs)
        val paused = bugJson.replace("\"logCount\":4", "\"logCount\":4,\"acceptingLogs\":false")
        assertFalse(parseAndroidPublishedBugPage("""{"data":{"bugs":[$paused],"nextCursor":null}}""").bugs.single().acceptingLogs)
    }

    @Test
    fun invalidPayloadFailsWithoutReplacingTheListWithPartialData() {
        for (body in listOf("<html>Proxy failure</html>", "{}", """{"data":{"bugs":[{}]}}""")) {
            assertTrue(runCatching { parseAndroidPublishedBugPage(body) }.exceptionOrNull() is AndroidBugReportUploadException)
        }
    }

    @Test
    fun logUploadOnlyIncludesConsentIdAndDiagnostics() {
        val id = UUID.randomUUID().toString()
        val request = buildPublishedBugLogRequest(
            reporter, "bug/with space", id,
            AndroidBugReportAttachment("diagnostics.txt", "text/plain", "Sanitized log".toByteArray()),
            endpoint = "https://example.com/published-bugs",
        )
        assertEquals("POST", request.method)
        assertEquals(reporter, request.header(ANDROID_BUG_REPORT_REPORTER_HEADER))
        assertEquals("/published-bugs/bug%2Fwith%20space/logs", request.url.encodedPath)
        val buffer = Buffer()
        request.body!!.writeTo(buffer)
        val body = buffer.readUtf8()
        assertTrue(body.contains(id))
        assertTrue(body.contains("name=\"termsAccepted\""))
        assertTrue(body.contains("filename=\"diagnostics.txt\""))
        assertFalse(body.contains("name=\"comment\""))
        assertFalse(body.contains("name=\"description\""))
    }

    @Test
    fun voteUsesAnExplicitIdempotentValueAndReturnsServerCount() = runBlocking {
        val http = OkHttpClient.Builder().addInterceptor { chain ->
            val request = chain.request()
            assertEquals("PUT", request.method)
            assertEquals("/published-bugs/one/vote", request.url.encodedPath)
            val buffer = Buffer()
            request.body!!.writeTo(buffer)
            assertEquals("""{"voted":true}""", buffer.readUtf8())
            Response.Builder().request(request).protocol(Protocol.HTTP_1_1).code(200).message("OK")
                .body("""{"ok":true,"data":$bugJson}""".toResponseBody()).build()
        }.build()
        val voted = voteAndroidPublishedBug(http, reporter, "one", true, "https://example.com/published-bugs")
        assertTrue(voted.voted)
        assertEquals(12, voted.votes)
    }

    @Test
    fun rejectedAndOversizedResponsesFailClearly() = runBlocking {
        val rejected = OkHttpClient.Builder().addInterceptor { chain ->
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(429).message("Limited")
                .body("""{"ok":false,"error":{"code":"LIMITED","message":"Try later","retryable":true}}""".toResponseBody()).build()
        }.build()
        val error = runCatching { fetchAndroidPublishedBugs(rejected, reporter) }.exceptionOrNull() as AndroidBugReportUploadException
        assertEquals("LIMITED", error.serverCode)
        assertTrue(error.retryable == true)
        val oversized = OkHttpClient.Builder().addInterceptor { chain ->
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200).message("OK")
                .body("x".repeat(513 * 1024).toResponseBody()).build()
        }.build()
        assertTrue(runCatching { fetchAndroidPublishedBugs(oversized, reporter) }.exceptionOrNull() is AndroidBugReportUploadException)
    }
}
