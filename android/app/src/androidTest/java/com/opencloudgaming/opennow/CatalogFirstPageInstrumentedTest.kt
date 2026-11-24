package com.opencloudgaming.opennow

import java.util.Collections
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CatalogFirstPageInstrumentedTest {
    @Test
    fun firstPageIsPublishedBeforeTheRemainingPagesAndSupplementalMetadata() = runBlocking {
        val events = Collections.synchronizedList(mutableListOf<String>())
        val http = OkHttpClient.Builder().addInterceptor { chain ->
            val request = chain.request()
            val payload = Buffer().apply { request.body?.writeTo(this) }.readUtf8()
            val json = when {
                request.url.host == "static.nvidiagrid.net" -> {
                    events += "metadata"
                    "[]"
                }
                "GetFilterGroupAndSortOrderDefinitions" in payload -> """{"data":{}}"""
                "\"cursor\":\"\"" in payload -> """{"data":{"apps":{
                    "items":[{"id":"first","title":"First Game"}],
                    "pageInfo":{"hasNextPage":true,"endCursor":"next","totalCount":2}
                }}}"""
                else -> {
                    events += "next page"
                    """{"data":{"apps":{"items":[{"id":"second","title":"Second Game"}],
                        "pageInfo":{"hasNextPage":false,"totalCount":2}}}}"""
                }
            }
            Response.Builder()
                .request(request)
                .protocol(Protocol.HTTP_1_1)
                .code(200)
                .message("OK")
                .body(json.toResponseBody("application/json".toMediaType()))
                .build()
        }.build()
        val repository = GfnCatalogRepository(
            http = http,
            serverInfoRepository = GfnServerInfoRepository(fetch = { _, _ ->
                GfnServerInfo(vpcId = "NP-AMS-06", regions = emptyList())
            }),
        )
        val result = withTimeout(5_000) {
            repository.browseCatalog("token", "https://example.test/", searchQuery = "", onFirstPage = { first ->
                assertEquals(listOf("first"), first.games.map { it.id })
                assertTrue(first.hasNextPage)
                events += "first page ready"
            })
        }

        assertEquals(listOf("first page ready", "next page", "metadata"), events)
        assertEquals(listOf("first", "second"), result.games.map { it.id })
    }
}
