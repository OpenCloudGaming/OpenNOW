package com.opencloudgaming.opennow

import android.app.Application
import com.opencloudgaming.opennow.boosteroid.BoosteroidController
import kotlinx.coroutines.CoroutineScope

internal const val BOOSTEROID_AVAILABLE = true

internal fun createBoosteroidIntegration(application: Application, scope: CoroutineScope): BoosteroidIntegration =
    BoosteroidController(application, scope)
