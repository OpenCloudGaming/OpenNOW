package com.opencloudgaming.opennow

import android.app.Activity
import android.content.Context
import android.os.SystemClock
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
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.google.android.gms.ads.AdRequest
import com.google.android.gms.ads.MobileAds
import com.google.android.ump.ConsentInformation
import com.google.android.ump.ConsentRequestParameters
import com.google.android.ump.FormError
import com.google.android.ump.UserMessagingPlatform
import kotlin.coroutines.resume
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull

private const val ADS_TAG = "GoogleAds"
private val ADS_INIT_TIMEOUT = 35.seconds
private val ADS_RETRY_DELAY = 30.seconds
internal val GOOGLE_AD_LOAD_TIMEOUT = 35.seconds
private val adsInitializationScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
private val adsInitializationLock = Any()
private var adsInitialization: CompletableDeferred<Boolean>? = null
private var adsInitializationRetryAtMs = Long.MAX_VALUE

internal fun googleAdRequest(): AdRequest = AdRequest.Builder()
    .setHttpTimeoutMillis(30_000)
    .build()

@Composable
internal fun rememberAdConsent(activity: ComponentActivity): AdConsent {
    val consent = viewModel<AdConsent>(viewModelStoreOwner = activity)
    LaunchedEffect(activity, consent) {
        activity.lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            consent.refresh(activity)
            launch {
                snapshotFlow { consent.shouldShowRequiredForm }.collect { required ->
                    if (required) consent.showRequiredForm(activity) { isActive }
                }
            }
            while (true) {
                snapshotFlow { consent.shouldRetryRefresh }.first { it }
                delay(ADS_RETRY_DELAY)
                if (consent.shouldRetryRefresh) consent.refresh(activity)
            }
        }
    }
    return consent
}

@Composable
internal fun AdPrivacyChoices(activity: Activity, consent: AdConsent) {
    if (!consent.showPrivacyChoices) return
    TextButton(
        onClick = { consent.openPrivacyChoices(activity) },
        enabled = !consent.refreshing && !consent.showingForm,
    ) {
        Text(stringResource(R.string.store_ad_privacy_choices))
    }
}

internal class AdConsent : ViewModel() {
    private var refreshSucceeded by mutableStateOf(false)
    private var refreshFailed by mutableStateOf(false)
    private var needsConsent by mutableStateOf(false)
    private var requiredFormAttempted by mutableStateOf(false)
    var refreshing by mutableStateOf(false)
        private set
    var canRequestAds by mutableStateOf(false)
        private set
    var showPrivacyChoices by mutableStateOf(false)
        private set
    var showingForm by mutableStateOf(false)
        private set

    val shouldRetryRefresh: Boolean get() = refreshFailed && !refreshing && !canRequestAds
    val shouldShowRequiredForm: Boolean get() =
        refreshSucceeded && needsConsent && !requiredFormAttempted && !showingForm

    fun refresh(activity: Activity) {
        if (refreshing || refreshSucceeded || activity.isFinishing || activity.isDestroyed) return
        refreshing = true
        try {
            val consent = UserMessagingPlatform.getConsentInformation(activity)
            consent.requestConsentInfoUpdate(
                activity,
                ConsentRequestParameters.Builder().build(),
                {
                    refreshSucceeded = true
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
            update(consent)
        } catch (error: RuntimeException) {
            Log.w(ADS_TAG, "Consent update could not start", error)
            refreshFailed = true
            refreshing = false
        }
    }

    fun showRequiredForm(activity: ComponentActivity, isCurrentDestination: () -> Boolean) {
        if (shouldShowRequiredForm) {
            showForm(activity, automatic = true) {
                isCurrentDestination() && activity.lifecycle.currentState == Lifecycle.State.RESUMED
            }
        }
    }

    fun openPrivacyChoices(activity: Activity) = showForm(activity, automatic = false)

    private fun showForm(
        activity: Activity,
        automatic: Boolean,
        canPresent: () -> Boolean = { true },
    ) {
        if (refreshing || showingForm || activity.isFinishing || activity.isDestroyed) return
        val consent = UserMessagingPlatform.getConsentInformation(activity)
        if (needsConsent) requiredFormAttempted = true
        showingForm = true
        val onDismissed: (FormError?) -> Unit = { error ->
            if (error != null) {
                Log.d(ADS_TAG, "Consent form failed (${error.errorCode})")
                if (!automatic && !activity.isDestroyed) {
                    Toast.makeText(activity, R.string.store_ad_privacy_unavailable, Toast.LENGTH_SHORT).show()
                }
            }
            update(consent)
            showingForm = false
        }
        try {
            if (needsConsent) {
                UserMessagingPlatform.loadConsentForm(activity, { form ->
                    if (canPresent() && !activity.isFinishing && !activity.isDestroyed) {
                        try {
                            form.show(activity, onDismissed)
                        } catch (error: RuntimeException) {
                            Log.w(ADS_TAG, "Consent form could not be shown", error)
                            onDismissed(null)
                        }
                    } else {
                        requiredFormAttempted = false
                        onDismissed(null)
                    }
                }, onDismissed)
            } else {
                UserMessagingPlatform.showPrivacyOptionsForm(activity, onDismissed)
            }
        } catch (error: RuntimeException) {
            Log.w(ADS_TAG, "Consent form could not be loaded", error)
            onDismissed(null)
        }
    }

    private fun update(consent: ConsentInformation) {
        needsConsent = consent.consentStatus == ConsentInformation.ConsentStatus.REQUIRED
        canRequestAds = consent.canRequestAds()
        showPrivacyChoices = needsConsent ||
            consent.privacyOptionsRequirementStatus == ConsentInformation.PrivacyOptionsRequirementStatus.REQUIRED
    }
}

internal suspend fun initializeGoogleAds(app: Context): Boolean {
    val initialization = synchronized(adsInitializationLock) {
        val current = adsInitialization
        if (current != null && SystemClock.elapsedRealtime() < adsInitializationRetryAtMs) {
            current
        } else {
            val attempt = CompletableDeferred<Boolean>()
            adsInitialization = attempt
            adsInitializationRetryAtMs = Long.MAX_VALUE
            adsInitializationScope.launch {
                val ready = withTimeoutOrNull(ADS_INIT_TIMEOUT) {
                    suspendCancellableCoroutine { result ->
                        try {
                            MobileAds.initialize(app) { result.resume(true) }
                        } catch (error: RuntimeException) {
                            Log.w(ADS_TAG, "Mobile Ads initialization failed", error)
                            result.resume(false)
                        }
                    }
                } ?: run {
                    Log.w(ADS_TAG, "Mobile Ads initialization timed out")
                    false
                }
                if (!ready) synchronized(adsInitializationLock) {
                    adsInitializationRetryAtMs = SystemClock.elapsedRealtime() + ADS_RETRY_DELAY.inWholeMilliseconds
                }
                attempt.complete(ready)
            }
            attempt
        }
    }
    return initialization.await()
}
