package com.opencloudgaming.opennow

import android.app.Application

class OpenNowApplication : Application() {
    override fun onCreate() {
        super.onCreate()

        OpenNowAnalytics.setup(this, SettingsStore(this).settings.value)
    }
}
