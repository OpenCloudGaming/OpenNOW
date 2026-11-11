package com.opencloudgaming.opennow

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import java.io.IOException
import java.net.SocketTimeoutException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidAuthRefreshTest {
    @Test
    fun expiredCredentialsAreKeptForOfflineTimeoutServerAndUnknownFailures() {
        val rejected = AuthRefreshException(400, "invalid_grant")
        for (failure in listOf(
            IOException("Offline"), SocketTimeoutException("Timed out"),
            AuthRefreshException(503, null), AuthRefreshException(429, "temporarily_unavailable"),
            AuthRefreshException(401, null), AuthRefreshException(400, "invalid_client"),
            IllegalStateException("Invalid user-info response"),
        )) {
            assertFalse(refreshCredentialsRejected(listOf(failure)))
            assertFalse(refreshCredentialsRejected(listOf(rejected, failure)))
        }
        assertFalse(refreshCredentialsRejected(emptyList()))
    }

    @Test
    fun onlyExplicitCredentialRejectionsInvalidateEveryRefreshMechanism() {
        val failures = listOf(
            runCatching { authRefreshResponse(400, """{"error":"invalid_grant"}""") }.exceptionOrNull(),
            runCatching { authRefreshResponse(401, """{"error":"invalid_token"}""") }.exceptionOrNull(),
        ).map { it as AuthRefreshException }
        assertTrue(refreshCredentialsRejected(failures))
    }

    @Test
    fun refreshResponseDoesNotExposeServerPayloadInErrorMessage() {
        val failure = runCatching {
            authRefreshResponse(503, """{"error":"temporarily_unavailable","detail":"private-token"}""")
        }.exceptionOrNull()
        assertTrue(failure is AuthRefreshException)
        assertFalse(failure?.message.orEmpty().contains("private-token"))
        assertFalse((failure as AuthRefreshException).credentialsRejected)
    }

    @Test
    fun malformedSuccessfulRefreshIsRetryableRatherThanCredentialRejection() {
        val failure = runCatching { authRefreshResponse(200, "not JSON") }.exceptionOrNull()
        assertTrue(failure is IOException)
        assertFalse(refreshCredentialsRejected(listOf(failure as Exception)))
        assertEquals("access-token", authRefreshResponse(200, """{"access_token":"access-token"}""")["access_token"].toString().trim('"'))
    }

    @Test
    fun legacySessionTriesBothKnownAuthClients() {
        assertEquals(
            listOf("browser-client", "device-client"),
            authenticationRefreshClientIds(
                savedClientId = null,
                browserClientId = "browser-client",
                deviceClientId = "device-client",
            ),
        )
    }

    @Test
    fun savedAuthClientIsTriedFirstWithoutDuplicates() {
        assertEquals(
            listOf("device-client", "browser-client"),
            authenticationRefreshClientIds(
                savedClientId = "device-client",
                browserClientId = "browser-client",
                deviceClientId = "device-client",
            ),
        )
    }

    @Test
    fun freshAccessAndClientTokensDoNotNeedBackgroundRefresh() {
        val now = 1_000_000L
        val tokens = tokens(
            expiresAt = now + TOKEN_REFRESH_WINDOW_MS + 1L,
            clientTokenExpiresAt = now + CLIENT_TOKEN_REFRESH_WINDOW_MS + 1L,
        )

        assertFalse(tokens.needsBackgroundRefresh(now))
    }

    @Test
    fun accessTokenInsideRefreshWindowNeedsBackgroundRefresh() {
        val now = 1_000_000L
        val tokens = tokens(
            expiresAt = now + TOKEN_REFRESH_WINDOW_MS - 1L,
            clientTokenExpiresAt = now + CLIENT_TOKEN_REFRESH_WINDOW_MS + 1L,
        )

        assertTrue(tokens.needsBackgroundRefresh(now))
    }

    @Test
    fun missingOrExpiringClientTokenNeedsBackgroundRefresh() {
        val now = 1_000_000L
        val missingClientToken = tokens(
            expiresAt = now + TOKEN_REFRESH_WINDOW_MS + 1L,
            clientToken = null,
            clientTokenExpiresAt = null,
        )
        val expiringClientToken = tokens(
            expiresAt = now + TOKEN_REFRESH_WINDOW_MS + 1L,
            clientTokenExpiresAt = now + CLIENT_TOKEN_REFRESH_WINDOW_MS - 1L,
        )

        assertTrue(missingClientToken.needsBackgroundRefresh(now))
        assertTrue(expiringClientToken.needsBackgroundRefresh(now))
    }

    @Test
    fun bestEffortRefreshFallsBackWhenRefreshFails() = runBlocking {
        val fallback = session("fallback-token")
        var reported: Throwable? = null

        val result = refreshedSessionOrFallback(
            fallback = fallback,
            refresh = { error("Token refresh failed") },
            onFailure = { reported = it },
        )

        assertSame(fallback, result)
        assertEquals("Token refresh failed", reported?.message)
    }

    @Test
    fun bestEffortRefreshStillPropagatesCancellation() = runBlocking {
        val cancellation = CancellationException("cancelled")

        val thrown = runCatching {
            refreshedSessionOrFallback(
                fallback = session("fallback-token"),
                refresh = { throw cancellation },
            )
        }.exceptionOrNull()

        assertSame(cancellation, thrown)
    }

    @Test
    fun launchUsesTokensReloadedFromBackgroundRefresh() = runBlocking {
        val staleUiSession = session("expired-ui-token")
        val persistedSession = session("current-worker-token").let {
            it.copy(tokens = it.tokens.copy(expiresAt = Long.MAX_VALUE))
        }

        val result = authSessionForLaunch(
            expected = staleUiSession,
            load = { persistedSession },
            refresh = { error("Fresh saved tokens must not cause a network refresh") },
        )

        assertSame(persistedSession, result)
    }

    @Test
    fun launchRefreshesOnlyInsideExistingExpiryWindow() = runBlocking {
        val now = 1_000_000L
        val expected = session("saved-token")
        var refreshCalls = 0
        for ((remainingMs, shouldRefresh) in listOf(
            TOKEN_REFRESH_WINDOW_MS + 1L to false,
            TOKEN_REFRESH_WINDOW_MS to false,
            TOKEN_REFRESH_WINDOW_MS - 1L to true,
            0L to true,
        )) {
            val saved = expected.copy(tokens = expected.tokens.copy(expiresAt = now + remainingMs))
            val before = refreshCalls
            authSessionForLaunch(
                expected = expected,
                load = { saved },
                refresh = { refreshCalls++; saved },
                nowMs = now,
            )
            assertEquals(if (shouldRefresh) 1 else 0, refreshCalls - before)
        }
    }

    @Test
    fun launchDoesNotFallBackToStaleTokensWhenRefreshFails() = runBlocking {
        val failure = IllegalStateException("Token refresh failed")

        val thrown = runCatching {
            authSessionForLaunch(session("stale-token"), load = { session("expired-token") }, refresh = { throw failure })
        }.exceptionOrNull()

        assertSame(failure, thrown)
    }

    @Test
    fun launchRejectsSignOutAndAccountOrProviderSwitchDuringRefresh() = runBlocking {
        val expected = session("stale-token")
        val switchedUser = expected.copy(user = expected.user.copy(userId = "other-user"))
        val switchedProvider = expected.copy(provider = expected.provider.copy(idpId = "other-provider"))

        for (restored in listOf(null, switchedUser, switchedProvider)) {
            val failure = runCatching {
                authSessionForLaunch(expected, load = { expected }, refresh = { restored })
            }.exceptionOrNull()

            assertTrue(failure is IllegalStateException)
            val loadFailure = runCatching {
                authSessionForLaunch(expected, load = { restored }, refresh = { error("Must not refresh another account") })
            }.exceptionOrNull()
            assertTrue(loadFailure is IllegalStateException)
        }
    }

    @Test
    fun launchRefreshPropagatesCancellation() = runBlocking {
        val cancellation = CancellationException("cancelled")

        val thrown = runCatching {
            authSessionForLaunch(session("stale-token"), load = { session("expired-token") }, refresh = { throw cancellation })
        }.exceptionOrNull()

        assertSame(cancellation, thrown)
    }

    private fun session(accessToken: String): AuthSession = AuthSession(
        provider = LoginProvider(
            idpId = "test-idp",
            code = "TEST",
            displayName = "Test",
            streamingServiceUrl = "https://example.invalid",
        ),
        tokens = AuthTokens(
            accessToken = accessToken,
            expiresAt = 0L,
        ),
        user = AuthUser(
            userId = "test-user",
            displayName = "Test user",
            membershipTier = "FREE",
        ),
    )

    private fun tokens(
        expiresAt: Long,
        clientToken: String? = "client-token",
        clientTokenExpiresAt: Long?,
    ): AuthTokens = AuthTokens(
        accessToken = "access-token",
        refreshToken = "refresh-token",
        idToken = "id-token",
        expiresAt = expiresAt,
        clientToken = clientToken,
        clientTokenExpiresAt = clientTokenExpiresAt,
    )
}
