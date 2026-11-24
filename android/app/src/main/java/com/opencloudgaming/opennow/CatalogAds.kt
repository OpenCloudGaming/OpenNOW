package com.opencloudgaming.opennow

import android.content.Context
import android.graphics.drawable.GradientDrawable
import android.util.Log
import android.view.LayoutInflater
import android.widget.ImageView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridItemScope
import androidx.compose.foundation.lazy.grid.LazyGridLayoutInfo
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
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
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
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
private const val CATALOG_AD_LOOK_AHEAD = TARGET_GAMES_BETWEEN_ADS * 2
private const val MAX_INLINE_AD_REQUESTS = 2
private val CATALOG_AD_LIFETIME = 55.minutes
private val CATALOG_AD_RETRY_DELAY = 30.seconds

@Stable
internal class CatalogAdAccess(val activity: ComponentActivity, val consent: AdConsent) {
    val storeTopAd = mutableStateOf<NativeAd?>(null)
    val libraryTopAd = mutableStateOf<NativeAd?>(null)
    val canShowAds: Boolean get() = consent.canRequestAds && !consent.showingForm
}

@Stable
internal class CatalogAdSlots(gameCount: Int) {
    val stops = TARGET_GAMES_BETWEEN_ADS..gameCount step TARGET_GAMES_BETWEEN_ADS
    private val ads = stops.associateWith { mutableStateOf<NativeAd?>(null) }

    operator fun get(stop: Int): NativeAd? = ads[stop]?.value

    fun set(stop: Int, ad: NativeAd?) {
        ads.getValue(stop).value = ad
    }
}

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
internal fun rememberCatalogAdAccess(enabled: Boolean, preloadTopAds: Boolean): CatalogAdAccess? {
    if (!enabled) return null
    val activity = LocalActivity.current as? ComponentActivity ?: return null
    val consent = rememberAdConsent(activity)
    val access = remember(activity, consent) { CatalogAdAccess(activity, consent) }
    if (preloadTopAds && access.canShowAds) {
        // Keep a separate creative ready for each tab, even before that tab composes its grid.
        LaunchedEffect(access) {
            launch { keepTopCatalogAdLoaded(activity, access.storeTopAd) }
            launch { keepTopCatalogAdLoaded(activity, access.libraryTopAd) }
        }
    }
    return access
}

@Composable
internal fun CatalogAdPrivacyOptions(access: CatalogAdAccess?) {
    if (access != null) AdPrivacyChoices(access.activity, access.consent)
}

internal fun catalogAdPrefetchStops(gameCount: Int, visible: IntRange): IntProgression {
    val firstStop = ((visible.first - 1) / TARGET_GAMES_BETWEEN_ADS + 1) * TARGET_GAMES_BETWEEN_ADS
    return firstStop..minOf(gameCount, visible.last + CATALOG_AD_LOOK_AHEAD) step TARGET_GAMES_BETWEEN_ADS
}

internal fun catalogGameGridIndex(gameIndex: Int, hasAdSlots: Boolean, headerCount: Int = 0): Int =
    headerCount + gameIndex + if (hasAdSlots) gameIndex / TARGET_GAMES_BETWEEN_ADS else 0

private suspend fun keepTopCatalogAdLoaded(activity: ComponentActivity, slot: MutableState<NativeAd?>) {
    while (true) {
        val ad = requestCatalogNativeAd(activity)
        if (ad == null) {
            delay(CATALOG_AD_RETRY_DELAY)
            continue
        }
        try {
            slot.value = ad
            delay(CATALOG_AD_LIFETIME)
        } finally {
            slot.value = null
            ad.destroy()
        }
    }
}

@Composable
internal fun rememberCatalogNativeAds(
    access: CatalogAdAccess?,
    gameCount: Int,
    grid: LazyGridState,
): CatalogAdSlots? {
    if (access?.canShowAds != true || gameCount < TARGET_GAMES_BETWEEN_ADS) return null
    val activity = access.activity
    val slots = remember(access, grid, gameCount) { CatalogAdSlots(gameCount) }

    LaunchedEffect(slots) {
        var wanted by mutableStateOf(catalogAdPrefetchStops(gameCount, 1..1))
        var loading by mutableIntStateOf(0)
        val expirations = mutableMapOf<Int, Job>()

        fun evict(stop: Int) {
            expirations.remove(stop)?.cancel()
            val ad = slots[stop] ?: return
            slots.set(stop, null)
            ad.destroy()
        }

        try {
            coroutineScope {
                launch {
                    snapshotFlow {
                        val visible = visibleCatalogGameCounts(grid.layoutInfo, gameCount) ?: (1..1)
                        catalogAdPrefetchStops(gameCount, visible)
                    }.collect { window ->
                        wanted = window
                        val firstRetained = window.first - TARGET_GAMES_BETWEEN_ADS
                        expirations.keys.filter { it !in firstRetained..window.last }.forEach(::evict)
                    }
                }
                repeat(MAX_INLINE_AD_REQUESTS) {
                    launch {
                        while (true) {
                            snapshotFlow { wanted.count { slots[it] == null } > loading }.first { it }
                            if (wanted.count { slots[it] == null } <= loading) continue
                            loading++
                            val ad = try {
                                requestCatalogNativeAd(activity)
                            } finally {
                                loading--
                            }
                            if (ad == null) {
                                delay(CATALOG_AD_RETRY_DELAY)
                                continue
                            }
                            // A fling can cross several slots during one request. Fill the next
                            // slot the player can see with the result.
                            val stop = wanted.firstOrNull { slots[it] == null }
                            if (stop == null) {
                                ad.destroy()
                                continue
                            }
                            slots.set(stop, ad)
                            expirations[stop] = launch {
                                delay(CATALOG_AD_LIFETIME)
                                expirations.remove(stop)
                                slots.set(stop, null)
                                ad.destroy()
                            }
                        }
                    }
                }
            }
        } finally {
            expirations.keys.toList().forEach(::evict)
        }
    }
    return slots
}

internal fun visibleCatalogGameCounts(layout: LazyGridLayoutInfo, gameCount: Int): IntRange? {
    // catalogGameItems places all games and inline ads after the headers.
    val headerCount = layout.totalItemsCount - gameCount - gameCount / TARGET_GAMES_BETWEEN_ADS
    if (headerCount < 0) return null
    var first = Int.MAX_VALUE
    var last = 0
    for (item in layout.visibleItemsInfo) {
        val itemCount = item.index - headerCount + 1
        if (itemCount <= 0) continue
        val position = itemCount - itemCount / (TARGET_GAMES_BETWEEN_ADS + 1)
        first = minOf(first, position)
        last = maxOf(last, position)
    }
    return (first..last).takeUnless { it.isEmpty() }
}

internal fun LazyGridScope.catalogTopAdItem(access: CatalogAdAccess?, page: AppPage) {
    if (access?.canShowAds != true) return
    // The leading key stays in place so a filled ad can appear at the top without a scroll reset.
    item(key = CATALOG_TOP_AD_KEY, span = { GridItemSpan(maxLineSpan) }, contentType = CATALOG_AD_CONTENT_TYPE) {
        CatalogTopAdRow(access, page)
    }
}

@Composable
internal fun CatalogTopAdRow(access: CatalogAdAccess?, page: AppPage, modifier: Modifier = Modifier) {
    if (access?.canShowAds != true) return
    val slot = if (page == AppPage.Library) access.libraryTopAd else access.storeTopAd
    CatalogAdRow(slot.value, compact = true, modifier = modifier)
}

/** Appends the games and inline ads after all headers so prefetch can read their grid indices. */
internal fun LazyGridScope.catalogGameItems(
    games: List<GameInfo>,
    ads: CatalogAdSlots?,
    gameContentType: String,
    gameContent: @Composable LazyGridItemScope.(GameInfo) -> Unit,
) {
    var start = 0
    if (ads != null) for (stop in ads.stops) {
        items(games.subList(start, stop), key = { it.id }, contentType = { gameContentType }, itemContent = gameContent)
        item(
            key = "$CATALOG_AD_KEY_PREFIX$stop",
            span = { GridItemSpan(maxLineSpan) },
            contentType = CATALOG_AD_CONTENT_TYPE,
        ) {
            CatalogAdRow(ads[stop], compact = false)
        }
        start = stop
    }
    items(games.subList(start, games.size), key = { it.id }, contentType = { gameContentType }, itemContent = gameContent)
}

@Composable
internal fun CatalogAdRow(ad: NativeAd?, compact: Boolean, modifier: Modifier = Modifier) {
    if (compact && ad == null) return
    val fontScale = LocalDensity.current.fontScale.coerceAtLeast(1f)
    val height = if (compact) 84.dp else 136.dp
    val row = modifier.fillMaxWidth().height(height * fontScale)
    if (ad == null) {
        Spacer(row)
    } else {
        CatalogNativeAdCard(ad, compact, row)
    }
}

private suspend fun requestCatalogNativeAd(activity: ComponentActivity): NativeAd? {
    val lifecycle = activity.lifecycle
    lifecycle.withResumed {}
    val app = activity.applicationContext
    if (withTimeoutOrNull(25.seconds) { initializeGoogleAds(app) } != true) return null
    var loaded: NativeAd? = null
    var completed = false
    try {
        completed = withTimeoutOrNull(25.seconds) {
            loaded = loadCatalogNativeAd(app, lifecycle)
            true
        } == true
        return loaded.takeIf { completed }
    } finally {
        if (!completed) loaded?.destroy()
    }
}

private suspend fun loadCatalogNativeAd(
    app: Context,
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
                lifecycle.withResumed {
                    loader.loadAd(AdRequest.Builder().build())
                }
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
private fun CatalogNativeAdCard(ad: NativeAd, compact: Boolean, modifier: Modifier) = key(ad, compact) {
    val colors = MaterialTheme.colorScheme
    val layout = if (compact) R.layout.store_compact_native_ad else R.layout.store_native_ad
    AndroidView(
        factory = { context ->
            val card = LayoutInflater.from(context).inflate(layout, null) as NativeAdView
            card.background = GradientDrawable().apply { cornerRadius = 12 * card.resources.displayMetrics.density }
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
        modifier = modifier,
        onRelease = { it.destroy() },
        update = { card ->
            (card.background as GradientDrawable).setColor(colors.surfaceContainerHigh.toArgb())
            card.findViewById<TextView>(R.id.store_ad_attribution_label).setTextColor(colors.onSurfaceVariant.toArgb())
            card.findViewById<TextView>(R.id.store_ad_headline).setTextColor(colors.onSurface.toArgb())
            card.findViewById<TextView>(R.id.store_ad_action).setTextColor(colors.primary.toArgb())
        },
    )
}
