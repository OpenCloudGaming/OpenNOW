package com.opencloudgaming.opennow

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class DiagnosticApiTest {
    private fun entry(time: Long, url: String = "https://prod.example/v2/session/one", status: Int = 200, method: String = "GET") = DiagnosticApiEntry(
        timestampMs = time, method = method, url = url, statusCode = status, elapsedMs = 30,
        requestBytes = null, responseChars = 2, request = DiagnosticApiBody(""), response = DiagnosticApiBody("{}"),
    )

    @Test
    fun pollsRemainIndividualAndChangedSuccessPayloadsSurvive() {
        val buffer = DiagnosticApiBuffer()
        repeat(3) { i -> buffer.record(entry(i.toLong()).copy(response = DiagnosticApiBody("{\"state\":$i}"), elapsedMs = 10L + i)) }
        val records = buffer.capture()
        assertEquals(3, records.size)
        assertEquals(listOf(0, 1, 2), records.map { it.response.json.jsonObject.getValue("state").jsonPrimitive.int })
        assertEquals(listOf(10L, 11L, 12L), records.map { it.elapsedMs })
        assertTrue(records.all { it.count == 1 })
    }

    @Test
    fun recordAndCharacterBudgetsExposeEviction() {
        val buffer = DiagnosticApiBuffer(capacity = 10, maxCapturedChars = 50)
        repeat(100) { buffer.record(entry(it.toLong()).copy(response = DiagnosticApiBody("1234567890"))) }
        val snapshot = buffer.snapshot()
        assertEquals(100L, snapshot.totalRecorded)
        assertEquals(95L, snapshot.evictedRecords)
        assertEquals(50L, snapshot.capturedChars)
        assertEquals(95L, snapshot.entries.first().timestampMs)
        assertEquals(95L, snapshot.retentionJson().getValue("evictedRecords").jsonPrimitive.long)
    }

    @Test
    fun failuresAndSessionCreationSurviveCatalogTraffic() {
        val buffer = DiagnosticApiBuffer(4)
        buffer.record(entry(1, status = 500, method = "POST"))
        buffer.record(entry(2, status = 500, method = "POST"))
        buffer.record(entry(3, method = "POST"))
        repeat(100) { buffer.record(entry(4L + it, url = "https://games.example/graphql", method = "POST")) }
        val records = buffer.capture()
        assertEquals(4, records.size)
        assertEquals(2, records.count { it.statusCode == 500 })
        assertTrue(records.any { it.timestampMs == 3L })
    }

    @Test
    fun capturesAreImmutableWhenNewPollsArrive() {
        val buffer = DiagnosticApiBuffer()
        buffer.record(entry(1))
        val before = buffer.capture()
        buffer.record(entry(2))
        assertEquals(1, before.single().count)
        assertEquals(2, buffer.capture().size)
    }

    @Test
    fun providerErrorsInsideHttp200AreNotOverwrittenBySuccessfulPolls() {
        val buffer = DiagnosticApiBuffer()
        buffer.record(entry(1).copy(response = DiagnosticApiBody("""{"requestStatus":{"statusCode":89}}""")))
        buffer.record(entry(2))
        buffer.record(entry(3))
        assertEquals(3, buffer.capture().size)
        assertTrue(buffer.capture().first().response.applicationFailure)
        assertEquals(1, buffer.capture().last().count)
    }

    @Test
    fun apiDataRetainsFullArraysNullsLongStringsAndProviderDetails() {
        val source = buildJsonObject {
            put("metaData", buildJsonArray { repeat(25) { add(buildJsonObject { put("key", "region-$it"); put("value", "endpoint-$it") }) } })
            put("attributes", buildJsonArray { add(buildJsonObject { put("key", "STORAGE_METRO_REGION"); put("textValue", "NP-SOFMR-DC") }) })
            put("requestStatus", buildJsonObject { put("statusCode", 89); put("statusDescription", "USER_STORAGE_NOT_AVAILABLE CA8C3011") })
            put("items", buildJsonArray { repeat(100) { add(buildJsonObject { put("title", "Game $it"); put("unused", JsonNull) }) } })
            put("accessToken", "secret-token")
            put("longDetail", "x".repeat(20_000))
        }
        val body = DiagnosticApiBody(source.toString())
        val data = body.json.jsonObject
        assertSame(body.json, body.json)
        assertEquals(25, data.getValue("metaData").jsonArray.size)
        assertEquals(89, data.getValue("requestStatus").jsonObject.getValue("statusCode").jsonPrimitive.int)
        assertEquals(100, data.getValue("items").jsonArray.size)
        assertEquals(20_000, data.getValue("longDetail").jsonPrimitive.content.length)
        assertTrue(data.toString().contains("NP-SOFMR-DC"))
        assertFalse(data.toString().contains("secret-token"))
        assertEquals(JsonNull, data.getValue("items").jsonArray[99].jsonObject.getValue("unused"))
    }

    @Test
    fun queryVariablesAndHeadersRemainStructuredAndSecretsAreRedacted() {
        val request = okhttp3.Request.Builder().url(okhttp3.HttpUrl.Builder().scheme("https").host("example.invalid")
            .addQueryParameter("variables", """{"gameId":42,"filter":{"name":"Trove"},"accessToken":"query-secret"}""")
            .addQueryParameter("access_token", "plain-secret").addQueryParameter("tag", "one").addQueryParameter("tag", "two").build())
            .header("Authorization", "Bearer header-secret").header("X-Client-Version", "1.6.8").build()
        val query = DiagnosticApiBody(diagnosticRequestQuery(request), decodeQueryJson = true).json.jsonObject
        assertEquals(42, query.getValue("variables").jsonArray[0].jsonObject.getValue("gameId").jsonPrimitive.int)
        assertEquals(2, query.getValue("tag").jsonArray.size)
        assertFalse(query.toString().contains("query-secret"))
        assertFalse(query.toString().contains("plain-secret"))
        val headers = DiagnosticApiBody(diagnosticHeaders(request.headers)).json
        assertFalse(headers.toString().contains("header-secret"))
        assertTrue(headers.toString().contains("1.6.8"))
    }

    @Test
    fun formEncodedBodiesRemainQuotedJsonStringsAndSecretsAreRedacted() {
        val body = DiagnosticApiBody(
            "grant_type=client_credentials&client_token=private-client-token&sub=private-account",
        )
        val entry = entry(1, method = "POST").copy(request = body)
        val serialized = entry.toJson().toString()
        val parsed = Json.parseToJsonElement(serialized).jsonObject
        val request = parsed.getValue("request").jsonPrimitive

        assertTrue(request.isString)
        assertTrue(request.content.startsWith("grant_type=client_credentials"))
        assertFalse(serialized.contains("private-client-token"))
        assertFalse(serialized.contains("private-account"))
        assertTrue(request.content.contains("client_token=[redacted]"))
        assertTrue(request.content.contains("sub=[redacted]"))
    }

    @Test
    fun onlyVerboseGameCopyIsStrippedWhileErrorsAndMetadataStayComplete() {
        val longText = "x".repeat(4000)
        val source = buildJsonObject {
            put("apps", buildJsonArray { repeat(20) { add(buildJsonObject {
                put("id", it); put("title", "Game $it"); put("longDescription", longText)
                put("gfn", buildJsonObject { put("playType", "INSTALL_TO_PLAY") })
            }) } })
            put("requestStatus", buildJsonObject { put("statusDescription", longText) })
            put("errors", buildJsonArray { add(buildJsonObject { put("description", longText) }) })
        }
        val result = DiagnosticApiBody(source.toString(), stripCatalogText = true).json.jsonObject
        assertEquals(20, result.getValue("apps").jsonArray.size)
        assertTrue(result.getValue("apps").jsonArray.last().jsonObject.getValue("longDescription").jsonPrimitive.content.contains("omitted catalog text"))
        assertEquals(longText, result.getValue("requestStatus").jsonObject.getValue("statusDescription").jsonPrimitive.content)
        assertEquals(longText, result.getValue("errors").jsonArray.single().jsonObject.getValue("description").jsonPrimitive.content)
        assertTrue(result.toString().contains("INSTALL_TO_PLAY"))
    }

    @Test
    fun hugeApiResponsesAreBoundedBeforeJsonParsingAndMarkTruncation() {
        val body = DiagnosticApiBody("[" + "a".repeat(1_000_000) + "END]")
        val data = body.json.jsonObject
        assertTrue(data.getValue("truncated").jsonPrimitive.boolean)
        assertEquals(1_000_005, data.getValue("originalChars").jsonPrimitive.int)
        assertTrue(data.toString().length < DIAGNOSTIC_API_BODY_MAX_CHARS + 1000)
        assertTrue(data.toString().contains("END]"))
    }
}
