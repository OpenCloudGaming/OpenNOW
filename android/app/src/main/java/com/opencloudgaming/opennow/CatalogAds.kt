package com.opencloudgaming.opennow

import android.content.Context
import android.graphics.drawable.GradientDrawable
import android.util.Log
import android.view.LayoutInflater
import android.widget.ImageView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridItemScope
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.isVisible
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.withResumed
import com.google.android.gms.ads.AdListener
import com.google.android.gms.ads.AdLoader
import com.google.android.gms.ads.AdRequest
import com.google.android.gms.ads.LoadAdError
import com.google.android.gms.ads.nativead.MediaView
import com.google.android.gms.ads.nativead.NativeAd
import com.google.android.gms.ads.nativead.NativeAdView
import kotlin.coroutines.resume
import kotlin.time.TimeMark
import kotlin.time.TimeSource
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

private const val CATALOG_ADS_TAG = "CatalogAds"
private const val CATALOG_AD_KEY_PREFIX = "catalog-ad-after-"
private const val CATALOG_TOP_AD_KEY = "catalog-top-ad"
internal const val CATALOG_AD_CONTENT_TYPE = "catalog-native-ad"
private const val TARGET_GAMES_BETWEEN_ADS = 30
private const val GAMES_TO_PREFETCH_BEFORE_AD = 24
private const val MAX_CACHED_CATALOG_ADS = 3
private const val CATALOG_AD_SETTLE_MS = 350L
private val CATALOG_AD_RETRY_DELAY = 30.seconds

internal class CatalogAdAccess(val activity: ComponentActivity, val consent: AdConsent)

internal fun canShowCatalogAds(state: OpenNowUiState): Boolean {
    if ((state.page != AppPage.Home && state.page != AppPage.Library) || state.androidPictureInPictureActive) return false
    if (state.streamStatus == "idle") return true
    if (state.streamStatus != "queue" || !state.streamLaunchMinimized ||
        shouldWaitForQueueAdPlayback(state.streamSession?.adState)
    ) return false
    return when (queueLaunchStatus(state).kind) {
        QueueLaunchStatusKind.QueuePosition, QueueLaunchStatusKind.WaitingForRig -> true
        else -> false
    }
}

@Composable
internal fun rememberCatalogAdAccess(enabled: Boolean): CatalogAdAccess? {
    if (!enabled) return null
    val activity = LocalActivity.current as? ComponentActivity ?: return null
    val consent = rememberAdConsent(activity, refreshDelayMillis = CATALOG_AD_SETTLE_MS)
    return remember(activity, consent) { CatalogAdAccess(activity, consent) }
}

@Composable
internal fun CatalogAdPrivacyOptions(access: CatalogAdAccess?) {
    if (access != null) AdPrivacyChoices(access.activity, access.consent)
}

internal fun catalogAdInterval(columns: Int): Int =
    ((TARGET_GAMES_BETWEEN_ADS + columns - 1) / columns) * columns

internal fun catalogAdStops(
    gameCount: Int,
    interval: Int,
    visible: IntRange,
    lookAhead: Int,
    lookBehind: Int = 0,
): List<Int> {
    val firstGame = (visible.first - lookBehind).coerceAtLeast(1)
    val firstStop = ((firstGame - 1) / interval + 1) * interval
    return (firstStop..minOf(gameCount, visible.last + lookAhead) step interval).toList()
}

internal fun catalogGameGridIndex(gameIndex: Int, adStops: Set<Int>, headerCount: Int = 0): Int =
    headerCount + gameIndex + adStops.count { it <= gameIndex }

@Composable
internal fun rememberCatalogTopAd(
    access: CatalogAdAccess?,
    grid: LazyGridState,
    keepGridAtStart: Boolean = true,
): NativeAd? {
    if (access == null || !access.consent.canRequestAds || access.consent.showingForm) return null
    val activity = access.activity
    var ad by remember(activity, grid) { mutableStateOf<NativeAd?>(null) }
    LaunchedEffect(activity, grid) {
        val loaded = requestCatalogNativeAd(activity, grid) ?: return@LaunchedEffect
        try {
            grid.awaitCatalogIdle(activity.lifecycle)
            if (keepGridAtStart && !grid.canScrollBackward) {
                // Lazy grids keep the first visible game anchored by key on insertions.
                // Keep the viewport at the start when the banner arrives there.
                grid.requestScrollToItem(0)
            }
            ad = loaded
            delay(55.minutes)
            grid.awaitCatalogIdle(activity.lifecycle)
            ad = null
        } finally {
            loaded.destroy()
        }
    }
    return ad
}

@Composable
internal fun rememberCatalogNativeAds(
    access: CatalogAdAccess?,
    games: List<GameInfo>,
    grid: LazyGridState,
): Map<Int, NativeAd> {
    if (access == null || !access.consent.canRequestAds || access.consent.showingForm) return emptyMap()
    val columns by remember(grid) { derivedStateOf { grid.layoutInfo.maxSpan } }
    if (columns == 0) return emptyMap()
    val interval = catalogAdInterval(columns)
    if (games.size < interval) return emptyMap()
    val activity = access.activity
    val positions by rememberUpdatedState(remember(games) {
        games.withIndex().associate { (index, game) -> game.id to index + 1 }
    })
    val cached = remember(activity, grid, columns, games.size) { mutableStateMapOf<Int, NativeAd>() }

    LaunchedEffect(cached) {
        val retryAfter = mutableMapOf<Int, TimeMark>()
        val expirations = mutableMapOf<Int, Job>()

        fun evict(stop: Int) {
            expirations.remove(stop)?.cancel()
            cached.remove(stop)?.destroy()
        }

        try {
            snapshotFlow {
                if (grid.isScrollInProgress) null else visibleCatalogGameCounts(grid, positions)
            }.afterCatalogSettles().collect { visible ->
                if (grid.isScrollInProgress) return@collect
                val retained = (visible.first - interval)..(visible.last + interval)
                cached.keys.filterNot { it in retained }.forEach(::evict)
                retryAfter.entries.removeAll { it.value.hasPassedNow() }
                val lookAhead = maxOf(GAMES_TO_PREFETCH_BEFORE_AD, columns * 2)
                for (stop in catalogAdStops(games.size, interval, visible, lookAhead, lookBehind = columns)) {
                    if (stop in cached || retryAfter[stop]?.hasPassedNow() == false) continue
                    if (grid.isScrollInProgress) break
                    val latest = visibleCatalogGameCounts(grid, positions) ?: continue
                    if (stop !in (latest.first - columns)..(latest.last + lookAhead)) continue
                    if (BuildConfig.DEBUG) Log.d(CATALOG_ADS_TAG, "Requesting native ad after game $stop")
                    val ad = requestCatalogNativeAd(activity, grid)
                    if (ad == null) {
                        retryAfter[stop] = TimeSource.Monotonic.markNow() + CATALOG_AD_RETRY_DELAY
                        continue
                    }
                    retryAfter.remove(stop)
                    var cachedAd = false
                    try {
                        grid.awaitCatalogIdle(activity.lifecycle)
                        val current = visibleCatalogGameCounts(grid, positions) ?: continue
                        if (stop !in (current.first - interval)..(current.last + lookAhead)) continue
                        if (cached.size == MAX_CACHED_CATALOG_ADS) {
                            cached.keys.maxByOrNull { kotlin.math.abs(it - current.first) }?.let(::evict)
                        }
                        cached[stop] = ad
                        cachedAd = true
                        if (BuildConfig.DEBUG) Log.d(CATALOG_ADS_TAG, "Native ad ready after game $stop")
                        expirations[stop] = launch {
                            delay(55.minutes)
                            grid.awaitCatalogIdle(activity.lifecycle)
                            expirations.remove(stop)
                            cached.remove(stop)?.destroy()
                        }
                    } finally {
                        if (!cachedAd) ad.destroy()
                    }
                }
            }
        } finally {
            cached.values.forEach(NativeAd::destroy)
            cached.clear()
        }
    }
    return cached
}

@OptIn(FlowPreview::class)
private fun <T : Any> Flow<T?>.afterCatalogSettles(): Flow<T> =
    debounce { if (it == null) 0L else CATALOG_AD_SETTLE_MS }.filterNotNull()

internal suspend fun LazyGridState.awaitCatalogIdle(lifecycle: Lifecycle) {
    // A fast fling should get the frame budget even when a prefetched ad finishes mid-scroll.
    do {
        lifecycle.withResumed {}
        snapshotFlow {
            Unit.takeIf { !isScrollInProgress && layoutInfo.totalItemsCount > 0 }
        }.afterCatalogSettles().first()
    } while (isScrollInProgress || lifecycle.currentState != Lifecycle.State.RESUMED)
}

private fun visibleCatalogGameCounts(grid: LazyGridState, positions: Map<String, Int>): IntRange? {
    var first = Int.MAX_VALUE
    var last = 0
    for (item in grid.layoutInfo.visibleItemsInfo) {
        val key = item.key as? String ?: continue
        val gameCount = if (item.contentType == CATALOG_AD_CONTENT_TYPE) {
            key.removePrefix(CATALOG_AD_KEY_PREFIX).toIntOrNull()
        } else {
            positions[key]
        } ?: continue
        first = minOf(first, gameCount)
        last = maxOf(last, gameCount)
    }
    return (first..last).takeUnless { it.isEmpty() }
}

internal fun LazyGridScope.catalogGameItems(
    games: List<GameInfo>,
    ads: Map<Int, NativeAd>,
    gameContentType: String,
    topAd: NativeAd? = null,
    gameContent: @Composable LazyGridItemScope.(GameInfo) -> Unit,
) {
    if (topAd != null) {
        item(
            key = CATALOG_TOP_AD_KEY,
            span = { GridItemSpan(maxLineSpan) },
            contentType = CATALOG_AD_CONTENT_TYPE,
        ) {
            CatalogNativeAdCard(topAd, compact = true)
        }
    }
    var start = 0
    for ((stop, ad) in ads.toSortedMap()) {
        if (stop <= start || stop > games.size) continue
        items(games.subList(start, stop), key = { it.id }, contentType = { gameContentType }, itemContent = gameContent)
        item(
            key = "$CATALOG_AD_KEY_PREFIX$stop",
            span = { GridItemSpan(maxLineSpan) },
            contentType = CATALOG_AD_CONTENT_TYPE,
        ) {
            CatalogNativeAdCard(ad)
        }
        start = stop
    }
    items(games.subList(start, games.size), key = { it.id }, contentType = { gameContentType }, itemContent = gameContent)
}

private suspend fun requestCatalogNativeAd(activity: ComponentActivity, grid: LazyGridState): NativeAd? {
    val lifecycle = activity.lifecycle
    grid.awaitCatalogIdle(lifecycle)
    val app = activity.applicationContext
    if (withTimeoutOrNull(25.seconds) { initializeGoogleAds(app) } != true) return null
    if (grid.isScrollInProgress || lifecycle.currentState != Lifecycle.State.RESUMED) {
        grid.awaitCatalogIdle(lifecycle)
    }
    var loaded: NativeAd? = null
    var completed = false
    try {
        completed = withTimeoutOrNull(25.seconds) {
            loaded = loadCatalogNativeAd(app, grid, lifecycle)
            true
        } == true
        return loaded.takeIf { completed }
    } finally {
        if (!completed) loaded?.destroy()
    }
}

private suspend fun loadCatalogNativeAd(
    app: Context,
    grid: LazyGridState,
    lifecycle: Lifecycle,
): NativeAd? = suspendCancellableCoroutine { result ->
    CoroutineScope(result.context).launch(Dispatchers.IO) {
        try {
            val loader = AdLoader.Builder(app, BuildConfig.STORE_NATIVE_AD_UNIT_ID)
                .forNativeAd { ad ->
                    // This compact placement renders static assets only.
                    if (ad.mediaContent?.hasVideoContent() == true ||
                        ad.headline.isNullOrBlank() || ad.callToAction.isNullOrBlank()
                    ) {
                        if (BuildConfig.DEBUG) Log.d(CATALOG_ADS_TAG, "Native creative does not fit the image placement")
                        ad.destroy()
                        result.resume(null)
                    } else {
                        result.resume(ad) { _, undelivered, _ -> undelivered.destroy() }
                    }
                }
                .withAdListener(object : AdListener() {
                    override fun onAdFailedToLoad(error: LoadAdError) {
                        Log.d(CATALOG_ADS_TAG, "Native ad unavailable (${error.domain}/${error.code})")
                        result.resume(null)
                    }
                })
                .build()
            withContext(Dispatchers.Main.immediate) {
                if (grid.isScrollInProgress || lifecycle.currentState != Lifecycle.State.RESUMED) {
                    grid.awaitCatalogIdle(lifecycle)
                }
                loader.loadAd(AdRequest.Builder().build())
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: RuntimeException) {
            Log.w(CATALOG_ADS_TAG, "Native ad request failed", error)
            result.resume(null)
        }
    }
}

@Composable
internal fun CatalogNativeAdCard(ad: NativeAd, compact: Boolean = false) = key(ad, compact) {
    val colors = MaterialTheme.colorScheme
    val layout = if (compact) R.layout.store_compact_native_ad else R.layout.store_native_ad
    AndroidView(
        factory = { context ->
            val card = LayoutInflater.from(context).inflate(layout, null) as NativeAdView
            val headline = card.findViewById<TextView>(R.id.store_ad_headline)
            val icon = card.findViewById<ImageView>(R.id.store_ad_icon)
            val media = card.findViewById<MediaView>(R.id.store_ad_media)
            val action = card.findViewById<TextView>(R.id.store_ad_action)
            headline.text = ad.headline
            icon.setImageDrawable(ad.icon?.drawable)
            icon.isVisible = icon.drawable != null
            media.isVisible = ad.mediaContent?.mainImage != null
            media.setImageScaleType(ImageView.ScaleType.FIT_CENTER)
            action.text = ad.callToAction
            card.headlineView = headline
            if (icon.isVisible) card.iconView = icon
            if (media.isVisible) {
                card.mediaView = media
                media.mediaContent = ad.mediaContent
            }
            card.callToActionView = action
            card.setNativeAd(ad)
            card
        },
        modifier = Modifier.fillMaxWidth(),
        onRelease = { it.destroy() },
        update = { card ->
            card.background = GradientDrawable().apply {
                cornerRadius = 12 * card.resources.displayMetrics.density
                setColor(colors.surfaceContainerHigh.toArgb())
            }
            card.findViewById<TextView>(R.id.store_ad_attribution_label).setTextColor(colors.onSurfaceVariant.toArgb())
            card.findViewById<TextView>(R.id.store_ad_headline).setTextColor(colors.onSurface.toArgb())
            card.findViewById<TextView>(R.id.store_ad_action).setTextColor(colors.primary.toArgb())
        },
    )
}
