package com.opencloudgaming.opennow

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal sealed interface CatalogWallpaperSelection {
    data class BuiltIn(val preset: CatalogBackgroundPreset) : CatalogWallpaperSelection
    data class Custom(val source: String) : CatalogWallpaperSelection
}

internal fun catalogWallpaperSelection(
    preset: CatalogBackgroundPreset,
    customSource: String?,
): CatalogWallpaperSelection =
    customSource
        ?.trim()
        ?.takeIf { it.isNotBlank() }
        ?.let(CatalogWallpaperSelection::Custom)
        ?: CatalogWallpaperSelection.BuiltIn(preset)

internal fun shouldShowCatalogWallpaper(settings: AppSettings): Boolean =
    settings.nerdCatalogBackground

@Composable
internal fun CatalogWallpaperBackdrop(
    settings: AppSettings,
    tvProfile: Boolean,
    width: Dp,
    height: Dp,
) {
    val showBackdrop = shouldShowCatalogWallpaper(settings)
    if (!showBackdrop) {
        return
    }
    val wallpaper = catalogWallpaperSelection(
        preset = settings.catalogBackgroundPreset,
        customSource = settings.nerdCatalogBackgroundUri,
    )
    val scrimAlpha = when {
        tvProfile -> 0.48f
        width > height -> 0.28f
        else -> 0.36f
    }
    Box(Modifier.fillMaxSize().clipToBounds()) {
        when (wallpaper) {
            is CatalogWallpaperSelection.BuiltIn -> {
                CatalogBuiltInWallpaperBackdrop(wallpaper.preset, Modifier.matchParentSize())
            }
            is CatalogWallpaperSelection.Custom -> {
                val fallbackPainter = painterResource(settings.catalogBackgroundPreset.drawableRes)
                AsyncImage(
                    model = imageDataForSource(wallpaper.source),
                    contentDescription = null,
                    modifier = Modifier.matchParentSize(),
                    contentScale = ContentScale.Crop,
                    placeholder = fallbackPainter,
                    error = fallbackPainter,
                    fallback = fallbackPainter,
                )
            }
        }
        Box(
            Modifier
                .matchParentSize()
                .background(Color.Black.copy(alpha = scrimAlpha)),
        )
    }
}

internal val CatalogBackgroundPreset.drawableRes: Int
    get() = when (this) {
        CatalogBackgroundPreset.ColorfulAbstract -> R.drawable.catalog_colorful_abstract_background
        CatalogBackgroundPreset.Original -> R.drawable.catalog_default_background
        CatalogBackgroundPreset.AbsoluteCinema -> R.drawable.catalog_absolute_cinema_background
    }

@Composable
private fun CatalogBuiltInWallpaperBackdrop(
    preset: CatalogBackgroundPreset,
    modifier: Modifier = Modifier,
) {
    Image(
        painter = painterResource(preset.drawableRes),
        contentDescription = null,
        modifier = modifier.background(OpenNowPalette.WallpaperBackdrop),
        contentScale = ContentScale.Crop,
    )
}

internal const val CATALOG_BACKGROUND_IMAGE_FILE_PREFIX = "catalog_background_image"

@Composable
internal fun catalogBackgroundPresetLabel(preset: CatalogBackgroundPreset): String = when (preset) {
    CatalogBackgroundPreset.ColorfulAbstract -> stringResource(R.string.catalog_background_colorful_abstract)
    CatalogBackgroundPreset.Original -> stringResource(R.string.catalog_background_original)
    CatalogBackgroundPreset.AbsoluteCinema -> stringResource(R.string.catalog_background_absolute_cinema)
}

/**
 * Imports a picture from the device and makes it the catalog backdrop.
 *
 * Returns the launcher so a caller can hang it off whatever control it likes — a settings row, a
 * tile in first-run setup — without any of them re-deriving the copy/prune/permission dance.
 */
@Composable
internal fun rememberCatalogBackgroundImagePicker(
    settings: AppSettings,
    onSettingsChange: (AppSettings) -> Unit,
): () -> Unit {
    val context = LocalContext.current
    val appContext = context.applicationContext
    val currentSettings by rememberUpdatedState(settings)
    val currentOnSettingsChange by rememberUpdatedState(onSettingsChange)
    val scope = rememberCoroutineScope()
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        takePersistableImageReadPermission(context, uri)
        scope.launch {
            val newUri = withContext(Dispatchers.IO) {
                persistCatalogBackgroundImage(appContext, uri)
            }
            val previousUri = currentSettings.nerdCatalogBackgroundUri
            if (newUri != uri.toString()) {
                releasePersistableImageReadPermission(context, uri.toString())
            }
            currentOnSettingsChange(
                currentSettings.copy(
                    nerdCatalogBackground = true,
                    systemWallpaperBackground = false,
                    nerdCatalogBackgroundUri = newUri,
                ),
            )
            if (!previousUri.isNullOrBlank() && previousUri != newUri) {
                releasePersistableImageReadPermission(context, previousUri)
            }
            pruneStoredCatalogBackgroundImages(appContext, keepUri = newUri)
        }
    }
    return remember(picker) { { picker.launch(arrayOf("image/*")) } }
}

/**
 * Switches the backdrop to a bundled preset, dropping any imported picture.
 *
 * Choosing a backdrop turns the backdrop on. Leaving these choices reachable while it is off is
 * deliberate — a user browsing them should not have to find a separate switch first.
 */
internal fun applyCatalogBackgroundPreset(
    context: Context,
    settings: AppSettings,
    preset: CatalogBackgroundPreset,
    onSettingsChange: (AppSettings) -> Unit,
) {
    val previousUri = settings.nerdCatalogBackgroundUri?.takeIf { it.isNotBlank() }
    onSettingsChange(
        settings.copy(
            nerdCatalogBackground = true,
            systemWallpaperBackground = false,
            catalogBackgroundPreset = preset,
            nerdCatalogBackgroundUri = null,
        ),
    )
    previousUri?.let { releasePersistableImageReadPermission(context, it) }
    pruneStoredCatalogBackgroundImages(context.applicationContext)
}

/** Drops the imported picture and falls back to the selected preset. */
internal fun clearCatalogBackgroundImage(
    context: Context,
    settings: AppSettings,
    onSettingsChange: (AppSettings) -> Unit,
) {
    val previousUri = settings.nerdCatalogBackgroundUri?.takeIf { it.isNotBlank() }
    onSettingsChange(settings.copy(nerdCatalogBackgroundUri = null))
    previousUri?.let { releasePersistableImageReadPermission(context, it) }
    pruneStoredCatalogBackgroundImages(context.applicationContext)
}

/** Settings > Interface: the backdrop preset row and custom-image buttons. */
@Composable
internal fun CatalogBackgroundPicker(
    settings: AppSettings,
    onSettingsChange: (AppSettings) -> Unit,
) {
    val context = LocalContext.current
    val launchImagePicker = rememberCatalogBackgroundImagePicker(settings, onSettingsChange)
    val hasCustomBackground = !settings.nerdCatalogBackgroundUri.isNullOrBlank()
    val presetOptions = CatalogBackgroundPreset.entries.map { it to catalogBackgroundPresetLabel(it) }
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f),
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        stringResource(R.string.settings_catalog_background_image),
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        if (hasCustomBackground) {
                            stringResource(R.string.settings_catalog_background_image_custom)
                        } else {
                            presetOptions.first { it.first == settings.catalogBackgroundPreset }.second
                        },
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            ChoiceRow(
                label = stringResource(R.string.settings_catalog_background_built_in),
                options = presetOptions.map { it.second },
                selected = presetOptions.first { it.first == settings.catalogBackgroundPreset }.second,
            ) { selectedLabel ->
                val selectedPreset = presetOptions.firstOrNull { it.second == selectedLabel }
                    ?.first
                    ?: return@ChoiceRow
                applyCatalogBackgroundPreset(context, settings, selectedPreset, onSettingsChange)
            }
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedButton(
                    onClick = launchImagePicker,
                    modifier = Modifier.weight(1f),
                ) {
                    Text(stringResource(R.string.action_choose_image), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (hasCustomBackground) {
                    OutlinedButton(
                        onClick = { clearCatalogBackgroundImage(context, settings, onSettingsChange) },
                        modifier = Modifier.weight(1f),
                    ) {
                        Text(stringResource(R.string.action_use_default), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

private fun takePersistableImageReadPermission(context: Context, uri: Uri) {
    runCatching {
        context.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
}

private fun persistCatalogBackgroundImage(context: Context, uri: Uri): String {
    val uniqueId = UUID.randomUUID().toString()
    val target = File(context.filesDir, "$CATALOG_BACKGROUND_IMAGE_FILE_PREFIX-$uniqueId")
    val temp = File(context.cacheDir, "$CATALOG_BACKGROUND_IMAGE_FILE_PREFIX-$uniqueId.tmp")
    return try {
        val input = context.contentResolver.openInputStream(uri) ?: return uri.toString()
        input.use {
            temp.outputStream().use { output ->
                it.copyTo(output)
            }
        }
        if (!temp.renameTo(target)) {
            temp.copyTo(target, overwrite = true)
        }
        Uri.fromFile(target).toString()
    } catch (_: Exception) {
        target.delete()
        uri.toString()
    } finally {
        temp.delete()
    }
}

internal fun isManagedCatalogBackgroundImageFile(filesDir: File, candidate: File): Boolean {
    val normalizedFilesDir = runCatching { filesDir.canonicalFile }.getOrElse { filesDir.absoluteFile }
    val normalizedCandidate = runCatching { candidate.canonicalFile }.getOrElse { candidate.absoluteFile }
    val managedName = normalizedCandidate.name == CATALOG_BACKGROUND_IMAGE_FILE_PREFIX ||
        normalizedCandidate.name.startsWith("$CATALOG_BACKGROUND_IMAGE_FILE_PREFIX-")
    return normalizedCandidate.parentFile == normalizedFilesDir && managedName
}

private fun pruneStoredCatalogBackgroundImages(context: Context, keepUri: String? = null) {
    val keepFile = keepUri
        ?.let { runCatching { Uri.parse(it) }.getOrNull() }
        ?.takeIf { it.scheme.equals("file", ignoreCase = true) }
        ?.path
        ?.let(::File)
        ?.let { file -> runCatching { file.canonicalFile }.getOrElse { file.absoluteFile } }
    runCatching {
        context.filesDir.listFiles()
            .orEmpty()
            .asSequence()
            .filter { isManagedCatalogBackgroundImageFile(context.filesDir, it) }
            .filterNot { candidate ->
                val normalized = runCatching { candidate.canonicalFile }.getOrElse { candidate.absoluteFile }
                normalized == keepFile
            }
            .forEach(File::delete)
        context.cacheDir.listFiles()
            .orEmpty()
            .asSequence()
            .filter {
                it.name == "$CATALOG_BACKGROUND_IMAGE_FILE_PREFIX.tmp" ||
                    (it.name.startsWith("$CATALOG_BACKGROUND_IMAGE_FILE_PREFIX-") && it.name.endsWith(".tmp"))
            }
            .forEach(File::delete)
    }
}

private fun releasePersistableImageReadPermission(context: Context, uriString: String) {
    val uri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return
    runCatching {
        context.contentResolver.releasePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
}
