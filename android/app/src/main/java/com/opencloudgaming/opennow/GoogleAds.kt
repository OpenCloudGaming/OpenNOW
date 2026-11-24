package com.opencloudgaming.opennow

import android.app.Activity
import android.content.Context
import android.util.Log
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.withResumed
import com.google.android.gms.ads.MobileAds
import com.google.android.ump.ConsentInformation
import com.google.android.ump.ConsentRequestParameters
import com.google.android.ump.FormError
import com.google.android.ump.UserMessagingPlatform
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

private const val ADS_TAG = "GoogleAds"
private val adsInitializing = AtomicBoolean()
private val adsReady = CompletableDeferred<Boolean>()

@Composable
internal fun rememberAdConsent(activity: ComponentActivity, refreshDelayMillis: Long = 0): AdConsent {
    val consent = viewModel<AdConsent>(viewModelStoreOwner = activity)
    LaunchedEffect(activity, consent, refreshDelayMillis) {
        delay(refreshDelayMillis)
        activity.lifecycle.withResumed { consent.refresh(activity) }
    }
    return consent
}

@Composable
internal fun AdPrivacyChoices(activity: Activity, consent: AdConsent) {
    if (!consent.showPrivacyChoices) return
    TextButton(
        onClick = { consent.openPrivacyChoices(activity) },
        enabled = !consent.showingForm,
    ) {
        Text(stringResource(R.string.store_ad_privacy_choices))
    }
}

internal class AdConsent : ViewModel() {
    private var refreshStarted = false
    private var refreshFailed = false
    private var needsConsent = false
    var refreshing by mutableStateOf(false)
        private set
    var canRequestAds by mutableStateOf(false)
        private set
    var showPrivacyChoices by mutableStateOf(false)
        private set
    var showingForm by mutableStateOf(false)
        private set

    fun refresh(activity: Activity, retryFailure: Boolean = false) {
        if (refreshing || (refreshStarted && !(retryFailure && refreshFailed))) return
        refreshStarted = true
        refreshing = true
        val consent = UserMessagingPlatform.getConsentInformation(activity)
        consent.requestConsentInfoUpdate(
            activity,
            ConsentRequestParameters.Builder().build(),
            {
                refreshFailed = false
                refreshing = false
                update(consent)
            },
            { error ->
                Log.d(ADS_TAG, "Consent update failed (${error.errorCode})")
                refreshFailed = true
                refreshing = false
                update(consent)
            },
        )
    }

    fun openPrivacyChoices(activity: Activity) {
        if (refreshing || showingForm || activity.isFinishing || activity.isDestroyed) return
        val consent = UserMessagingPlatform.getConsentInformation(activity)
        showingForm = true
        val onDismissed: (FormError?) -> Unit = { error ->
            if (error != null && !activity.isDestroyed) {
                Toast.makeText(activity, R.string.store_ad_privacy_unavailable, Toast.LENGTH_SHORT).show()
            }
            update(consent)
            showingForm = false
        }
        if (needsConsent) {
            UserMessagingPlatform.loadAndShowConsentFormIfRequired(activity, onDismissed)
        } else {
            UserMessagingPlatform.showPrivacyOptionsForm(activity, onDismissed)
        }
    }

    private fun update(consent: ConsentInformation) {
        needsConsent = consent.consentStatus == ConsentInformation.ConsentStatus.REQUIRED
        canRequestAds = !needsConsent && consent.canRequestAds()
        showPrivacyChoices = (needsConsent && consent.isConsentFormAvailable) ||
            consent.privacyOptionsRequirementStatus == ConsentInformation.PrivacyOptionsRequirementStatus.REQUIRED
    }
}

internal suspend fun initializeGoogleAds(app: Context): Boolean {
    if (!adsInitializing.get()) {
        withContext(Dispatchers.IO) {
            if (adsInitializing.compareAndSet(false, true)) {
                try {
                    MobileAds.initialize(app) { adsReady.complete(true) }
                } catch (error: RuntimeException) {
                    Log.w(ADS_TAG, "Mobile Ads initialization failed", error)
                    adsReady.complete(false)
                }
            }
        }
    }
    return adsReady.await()
}
