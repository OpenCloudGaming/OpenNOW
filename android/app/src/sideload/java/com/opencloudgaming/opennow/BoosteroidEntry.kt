package com.opencloudgaming.opennow

import android.content.Context
import android.content.Intent
import com.opencloudgaming.opennow.boosteroid.BoosteroidActivity
import com.opencloudgaming.opennow.boosteroid.EXTRA_CHOOSE_CLOUD_SERVICE

internal const val BOOSTEROID_AVAILABLE = true

internal fun openBoosteroid(context: Context, chooseService: Boolean = false) {
    context.startActivity(
        Intent(context, BoosteroidActivity::class.java).putExtra(EXTRA_CHOOSE_CLOUD_SERVICE, chooseService),
    )
}
