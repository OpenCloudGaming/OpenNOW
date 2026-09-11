package com.opencloudgaming.opennow

import android.app.Application
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.util.Log
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.SingletonImageLoader
import coil3.disk.DiskCache
import coil3.disk.directory
import coil3.memory.MemoryCache
import coil3.network.okhttp.OkHttpNetworkFetcherFactory
import coil3.request.crossfade
import okio.Path.Companion.toOkioPath
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class OpenNowApplication : Application(), SingletonImageLoader.Factory {
    private val startupScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val startupDataReady = CompletableDeferred<Unit>()
    internal val httpClient by lazy(::defaultHttpClient)
    internal val authStore by lazy { AuthStore(this) }
    internal val authRepository by lazy { GfnAuthRepository(this, authStore, httpClient) }
    internal val localTvConnector by lazy { LocalTvConnector() }
    internal val diagnosticHistoryStore by lazy { DiagnosticHistoryStore(filesDir) }

    override fun onCreate() {
        super.onCreate()
        runCatching { diagnosticHistoryStore.beginAppRun() }
            .onFailure { error ->
                Log.w(OPENNOW_DEBUG_LOG_TAG, "Could not rotate diagnostic history", error)
            }

        startupScope.launch {
            runCatching {
                SettingsStore(this@OpenNowApplication).settings.value.also {
                    // Warm secure auth and run its one-time migration on the same background path.
                    authStore.state.value
                }
            }
            startupDataReady.complete(Unit)
            if (isTelevisionDevice()) {
                delay(TV_BACKGROUND_SERVICE_START_DELAY_MS)
            }
            withContext(Dispatchers.Main) {
                initializeBackgroundServices()
            }
        }
    }

    /**
     * Coil's default loader builds its own OkHttp client, which means a second connection pool,
     * dispatcher and thread pool alongside the one the API already uses — and no shared TLS session
     * reuse with the CDN. Handing it [httpClient] collapses that back to one, and the caches below
     * are sized deliberately rather than left to a fraction of whatever the device reports.
     */
    override fun newImageLoader(context: PlatformContext): ImageLoader =
        ImageLoader.Builder(context)
            .components {
                add(OkHttpNetworkFetcherFactory(callFactory = { httpClient }))
            }
            .memoryCache {
                // Poster art is re-shown constantly while scrolling a grid; this is the cache that
                // keeps a fling from re-decoding every bitmap it passes.
                MemoryCache.Builder()
                    .maxSizePercent(context, IMAGE_MEMORY_CACHE_FRACTION)
                    .build()
            }
            .diskCache {
                DiskCache.Builder()
                    .directory(cacheDir.resolve(IMAGE_DISK_CACHE_DIR).toOkioPath())
                    .maxSizeBytes(IMAGE_DISK_CACHE_BYTES)
                    .build()
            }
            // Artwork that is already in memory should appear instantly; a fade on every cell makes
            // a fast grid look slower than it is.
            .crossfade(false)
            .build()

    internal suspend fun awaitStartupData() {
        startupDataReady.await()
    }

    override fun onTerminate() {
        localTvConnector.close()
        super.onTerminate()
    }

    private fun initializeBackgroundServices() {
        AndroidAuthRefreshScheduler.schedule(this)
    }

    private fun isTelevisionDevice(): Boolean =
        packageManager.hasSystemFeature(PackageManager.FEATURE_LEANBACK) ||
            resources.configuration.uiMode and Configuration.UI_MODE_TYPE_MASK == Configuration.UI_MODE_TYPE_TELEVISION

    private companion object {
        const val TV_BACKGROUND_SERVICE_START_DELAY_MS = 2_500L
        const val IMAGE_MEMORY_CACHE_FRACTION = 0.25
        const val IMAGE_DISK_CACHE_DIR = "image_cache"
        const val IMAGE_DISK_CACHE_BYTES = 256L * 1024 * 1024
    }
}
