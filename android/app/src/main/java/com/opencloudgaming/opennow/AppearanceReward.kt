package com.opencloudgaming.opennow

import android.content.Context
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import com.google.android.gms.ads.AdError
import com.google.android.gms.ads.FullScreenContentCallback
import com.google.android.gms.ads.LoadAdError
import com.google.android.gms.ads.rewarded.RewardedAd
import com.google.android.gms.ads.rewarded.RewardedAdLoadCallback
import kotlin.coroutines.resume
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeout

private const val REWARD_TAG = "AppearanceReward"

private enum class RewardOfferState {
    Idle, Loading, Showing, Unavailable,

    ;

    val busy: Boolean
        get() = this == Loading || this == Showing
}

@Composable
internal fun AppearanceUnlockOffer(onEarned: () -> Unit, isCurrentDestination: () -> Boolean = { true }) {
    val activity = LocalActivity.current as? ComponentActivity ?: return
    val consent = rememberAdConsent(activity)
    val scope = rememberCoroutineScope()
    val grantReward by rememberUpdatedState(onEarned)
    val stillOnDestination by rememberUpdatedState(isCurrentDestination)
    var offer by remember(activity) { mutableStateOf(RewardOfferState.Idle) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            text = stringResource(R.string.appearance_unlock_description),
            style = MaterialTheme.typography.bodyMedium,
        )
        FilledTonalButton(
            enabled = !offer.busy && !consent.refreshing && !consent.showingForm &&
                (consent.canRequestAds || !consent.showPrivacyChoices),
            onClick = {
                if (offer.busy || consent.refreshing || consent.showingForm) return@FilledTonalButton
                if (!consent.canRequestAds) {
                    offer = RewardOfferState.Unavailable
                    consent.refresh(activity)
                    return@FilledTonalButton
                }
                offer = RewardOfferState.Loading
                scope.launch {
                    val app = activity.applicationContext
                    val ad = if (initializeGoogleAds(app) && stillOnDestination() &&
                        activity.lifecycle.currentState == Lifecycle.State.RESUMED &&
                        !activity.isFinishing && consent.canRequestAds && !consent.showingForm
                    ) {
                        try {
                            withTimeout(GOOGLE_AD_LOAD_TIMEOUT) { loadAppearanceReward(app) }
                        } catch (_: TimeoutCancellationException) {
                            Log.d(REWARD_TAG, "Rewarded ad request timed out")
                            null
                        }
                    } else {
                        null
                    }
                    if (ad == null) {
                        offer = RewardOfferState.Unavailable
                        return@launch
                    }
                    // A late load cannot present over a different screen, backgrounded app or privacy form.
                    if (!stillOnDestination() || activity.lifecycle.currentState != Lifecycle.State.RESUMED ||
                        activity.isFinishing || !consent.canRequestAds || consent.showingForm
                    ) {
                        offer = RewardOfferState.Idle
                        return@launch
                    }
                    ad.fullScreenContentCallback = object : FullScreenContentCallback() {
                        override fun onAdDismissedFullScreenContent() {
                            offer = RewardOfferState.Idle
                        }

                        override fun onAdFailedToShowFullScreenContent(error: AdError) {
                            Log.d(REWARD_TAG, "Rewarded ad could not show (${error.domain}/${error.code})")
                            offer = RewardOfferState.Unavailable
                        }
                    }
                    offer = RewardOfferState.Showing
                    try {
                        ad.show(activity) { grantReward() }
                    } catch (error: RuntimeException) {
                        Log.w(REWARD_TAG, "Rewarded ad presentation failed", error)
                        offer = RewardOfferState.Unavailable
                    }
                }
            },
        ) {
            Text(
                stringResource(
                    if (offer == RewardOfferState.Loading) R.string.appearance_unlock_loading
                    else R.string.appearance_unlock_watch,
                ),
            )
        }
        AdPrivacyChoices(activity, consent)
        if (offer == RewardOfferState.Unavailable) {
            Text(
                text = stringResource(R.string.appearance_unlock_unavailable),
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

private suspend fun loadAppearanceReward(app: Context): RewardedAd? = suspendCancellableCoroutine { result ->
    try {
        RewardedAd.load(
            app,
            BuildConfig.COLOURS_REWARDED_AD_UNIT_ID,
            googleAdRequest(),
            object : RewardedAdLoadCallback() {
                override fun onAdLoaded(ad: RewardedAd) = result.resume(ad)

                override fun onAdFailedToLoad(error: LoadAdError) {
                    Log.d(REWARD_TAG, "Rewarded ad unavailable (${error.domain}/${error.code})")
                    result.resume(null)
                }
            },
        )
    } catch (error: RuntimeException) {
        Log.w(REWARD_TAG, "Rewarded ad request failed", error)
        result.resume(null)
    }
}
