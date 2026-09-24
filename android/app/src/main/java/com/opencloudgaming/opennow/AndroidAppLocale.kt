package com.opencloudgaming.opennow

import android.app.Activity
import android.app.LocaleManager
import android.content.Context
import android.content.ContextWrapper
import android.content.res.Configuration
import android.content.res.Resources
import android.os.Build
import android.os.LocaleList
import java.util.Locale

internal const val ANDROID_APP_LANGUAGE_SYSTEM = ""
internal const val ANDROID_APP_LANGUAGE_ENGLISH = "en"
internal const val ANDROID_APP_LANGUAGE_ARABIC = "ar"
internal const val ANDROID_APP_LANGUAGE_GERMAN = "de"
internal const val ANDROID_APP_LANGUAGE_SPANISH = "es"
internal const val ANDROID_APP_LANGUAGE_FRENCH = "fr"
internal const val ANDROID_APP_LANGUAGE_INDONESIAN = "id"
internal const val ANDROID_APP_LANGUAGE_JAPANESE = "ja"
internal const val ANDROID_APP_LANGUAGE_KOREAN = "ko"
internal const val ANDROID_APP_LANGUAGE_DUTCH = "nl"
internal const val ANDROID_APP_LANGUAGE_POLISH = "pl"
internal const val ANDROID_APP_LANGUAGE_PORTUGUESE = "pt"
internal const val ANDROID_APP_LANGUAGE_ROMANIAN = "ro"
internal const val ANDROID_APP_LANGUAGE_RUSSIAN = "ru"
internal const val ANDROID_APP_LANGUAGE_TURKISH = "tr"
internal const val ANDROID_APP_LANGUAGE_SIMPLIFIED_CHINESE = "zh-Hans"

internal val ANDROID_APP_LANGUAGE_TAGS = setOf(
    ANDROID_APP_LANGUAGE_ENGLISH,
    ANDROID_APP_LANGUAGE_ARABIC,
    ANDROID_APP_LANGUAGE_GERMAN,
    ANDROID_APP_LANGUAGE_SPANISH,
    ANDROID_APP_LANGUAGE_FRENCH,
    ANDROID_APP_LANGUAGE_INDONESIAN,
    ANDROID_APP_LANGUAGE_JAPANESE,
    ANDROID_APP_LANGUAGE_KOREAN,
    ANDROID_APP_LANGUAGE_DUTCH,
    ANDROID_APP_LANGUAGE_POLISH,
    ANDROID_APP_LANGUAGE_PORTUGUESE,
    ANDROID_APP_LANGUAGE_ROMANIAN,
    ANDROID_APP_LANGUAGE_RUSSIAN,
    ANDROID_APP_LANGUAGE_TURKISH,
    ANDROID_APP_LANGUAGE_SIMPLIFIED_CHINESE,
)

internal data class AndroidAppLocaleState(
    val selectedLanguageTag: String,
    val effectiveLanguageTag: String,
    val deviceLanguageTag: String = effectiveLanguageTag,
) {
    val bugReportsAllowed: Boolean
        get() = androidAppLocaleIsEnglish(selectedLanguageTag) || androidAppLocaleIsEnglish(deviceLanguageTag)

    val bugReportLanguageTag: String?
        get() = when {
            androidAppLocaleIsEnglish(selectedLanguageTag) -> selectedLanguageTag
            androidAppLocaleIsEnglish(deviceLanguageTag) -> deviceLanguageTag
            else -> null
        }
}

/**
 * [Configuration.getLocales] landed in API 24, but this module ships to API 23. Reading the
 * deprecated single-locale field below that level keeps locale detection — which runs on the
 * startup path — from throwing NoSuchMethodError on the oldest supported devices.
 */
private val Configuration.primaryLocale: Locale?
    get() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
        locales.get(0)
    } else {
        @Suppress("DEPRECATION")
        locale
    }

internal fun currentAndroidAppLocale(context: Context): AndroidAppLocaleState {
    val selected = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        context.getSystemService(LocaleManager::class.java)
            ?.applicationLocales
            ?.get(0)
            ?.toLanguageTag()
            .orEmpty()
    } else {
        context.getSharedPreferences(APP_LOCALE_PREFERENCES, Context.MODE_PRIVATE)
            .getString(APP_LOCALE_LANGUAGE_TAG, null)
            .orEmpty()
    }
    val effective = selected.ifBlank {
        context.resources.configuration.primaryLocale?.toLanguageTag().orEmpty()
    }
    val device = (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        context.getSystemService(LocaleManager::class.java)
            ?.systemLocales
            ?.get(0)
            ?.toLanguageTag()
            .orEmpty()
    } else {
        Resources.getSystem().configuration.primaryLocale?.toLanguageTag().orEmpty()
    }).ifBlank {
        Resources.getSystem().configuration.primaryLocale?.toLanguageTag().orEmpty()
    }
    return AndroidAppLocaleState(
        selectedLanguageTag = selected,
        effectiveLanguageTag = effective,
        deviceLanguageTag = device,
    )
}

internal fun androidAppLocaleIsEnglish(languageTag: String): Boolean =
    Locale.forLanguageTag(languageTag.replace('_', '-')).language.equals("en", ignoreCase = true)

internal fun androidAppLanguageSelectionIsSupported(languageTag: String): Boolean {
    val normalized = languageTag.trim().replace('_', '-')
    return normalized.isBlank() || normalized in ANDROID_APP_LANGUAGE_TAGS
}

internal fun setAndroidAppLanguage(context: Context, languageTag: String) {
    val normalized = languageTag.trim().replace('_', '-')
    require(androidAppLanguageSelectionIsSupported(normalized)) {
        "Unsupported OpenNOW Android app language: $normalized"
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        context.getSystemService(LocaleManager::class.java)?.applicationLocales =
            LocaleList.forLanguageTags(normalized)
        return
    }
    context.getSharedPreferences(APP_LOCALE_PREFERENCES, Context.MODE_PRIVATE)
        .edit()
        .putString(APP_LOCALE_LANGUAGE_TAG, normalized)
        .apply()
    context.findActivity()?.recreate()
}

/** Applies the in-app language choice on Android 12 and older. Android 13+ owns this context. */
internal fun localizedAndroidContext(base: Context): Context {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) return base
    val languageTag = base.getSharedPreferences(APP_LOCALE_PREFERENCES, Context.MODE_PRIVATE)
        .getString(APP_LOCALE_LANGUAGE_TAG, null)
        .orEmpty()
    if (languageTag.isBlank()) return base
    val configuration = Configuration(base.resources.configuration).apply {
        setLocale(Locale.forLanguageTag(languageTag))
    }
    return base.createConfigurationContext(configuration)
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}

private const val APP_LOCALE_PREFERENCES = "opennow_app_locale"
private const val APP_LOCALE_LANGUAGE_TAG = "language_tag"
