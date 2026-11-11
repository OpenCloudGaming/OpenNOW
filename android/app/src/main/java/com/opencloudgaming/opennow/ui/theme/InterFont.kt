package com.opencloudgaming.opennow.ui.theme

import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import android.os.Build
import android.os.Looper
import androidx.compose.ui.text.font.AndroidFont
import androidx.compose.ui.text.font.FontLoadingStrategy
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.core.content.res.ResourcesCompat
import com.opencloudgaming.opennow.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Compose caches equal descriptors; every weight shares the same bounded loader. */
internal data class InterFont(override val weight: FontWeight) : AndroidFont(
    loadingStrategy = FontLoadingStrategy.Async,
    typefaceLoader = InterTypefaceLoader,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
) {
    override val style = FontStyle.Normal
}

private object InterTypefaceLoader : AndroidFont.TypefaceLoader {
    private val dispatcher = Dispatchers.IO.limitedParallelism(1)

    override fun loadBlocking(context: Context, font: AndroidFont): Typeface? =
        error("Inter must load asynchronously")

    override suspend fun awaitLoad(context: Context, font: AndroidFont): Typeface = withContext(dispatcher) {
        // Compose starts async font loading UNDISPATCHED. A resource Font marked Async alone can
        // still read the local TTF inline; dispatch before both loading and variation creation.
        check(Looper.myLooper() != Looper.getMainLooper())
        val base = checkNotNull(ResourcesCompat.getFont(context, R.font.inter_variable))
        if (Build.VERSION.SDK_INT >= 26) {
            Paint().apply {
                typeface = base
                fontVariationSettings = "'wght' ${font.weight.weight}"
            }.typeface
        } else {
            base
        }
    }
}
