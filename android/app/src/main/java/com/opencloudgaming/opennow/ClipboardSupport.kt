package com.opencloudgaming.opennow

import android.content.ClipData
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.Clipboard

internal suspend fun Clipboard.copyPlainText(value: String) {
    setClipEntry(ClipEntry(ClipData.newPlainText("OpenNOW", value)))
}
