package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import java.io.ByteArrayOutputStream
import java.io.InputStream

private const val SETTINGS_BACKUP_FORMAT = "opennow-settings"
private const val SETTINGS_BACKUP_VERSION = 1
internal const val MAX_SETTINGS_BACKUP_BYTES = 2 * 1024 * 1024

@Serializable
private data class SettingsBackup(
    val format: String,
    val version: Int,
    val settings: AppSettings,
)

internal fun encodeSettingsBackup(settings: AppSettings): ByteArray {
    val backup = SettingsBackup(
        format = SETTINGS_BACKUP_FORMAT,
        version = SETTINGS_BACKUP_VERSION,
        settings = settings,
    )
    return OpenNowJson.encodeToString(backup).toByteArray(Charsets.UTF_8).also {
        require(it.size <= MAX_SETTINGS_BACKUP_BYTES) { "Settings backup is too large" }
    }
}

internal fun decodeSettingsBackup(input: InputStream): AppSettings {
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val count = input.read(buffer)
        if (count < 0) break
        require(output.size() + count <= MAX_SETTINGS_BACKUP_BYTES) { "Settings backup is too large" }
        output.write(buffer, 0, count)
    }
    val bytes = output.toByteArray()
    val backup = OpenNowJson.decodeFromString<SettingsBackup>(bytes.toString(Charsets.UTF_8))
    require(backup.format == SETTINGS_BACKUP_FORMAT) { "This is not an OpenNOW settings backup" }
    require(backup.version == SETTINGS_BACKUP_VERSION) { "Unsupported settings backup version" }
    return backup.settings.withCurrentStreamPresentationDefaults().normalizedForAndroid()
}
