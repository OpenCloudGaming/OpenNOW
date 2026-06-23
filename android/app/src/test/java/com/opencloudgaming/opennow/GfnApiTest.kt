package com.opencloudgaming.opennow

import okhttp3.HttpUrl.Companion.toHttpUrl
import org.junit.Assert.assertEquals
import org.junit.Test

class GfnApiTest {
    @Test
    fun canonicalizesOldGamesGraphQlHost() {
        val url = canonicalizeGfnRequestUrl(
            "https://games.geforcenow.com/graphql?requestType=panels%2FMainV2".toHttpUrl(),
        )

        assertEquals("https", url.scheme)
        assertEquals("games.geforce.com", url.host)
        assertEquals("/graphql", url.encodedPath)
        assertEquals("panels/MainV2", url.queryParameter("requestType"))
    }

    @Test
    fun leavesCanonicalGamesGraphQlHostUnchanged() {
        val source = "https://games.geforce.com/graphql".toHttpUrl()

        assertEquals(source, canonicalizeGfnRequestUrl(source))
    }
}
