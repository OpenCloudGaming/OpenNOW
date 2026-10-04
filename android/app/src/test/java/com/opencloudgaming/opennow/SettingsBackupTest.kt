package com.opencloudgaming.opennow

import java.io.ByteArrayInputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class SettingsBackupTest {
    @Test
    fun restoresControllerLayoutsPresetsAndOtherSettings() {
        val touch = AndroidTouchSettings(
            buttonScale = 1.3f,
            offsets = mapOf("face_landscape" to TouchOffset(12f, -8f)),
        )
        val settings = AppSettings(
            androidTouch = touch,
            touchControlPresets = listOf(TouchControlPreset("custom", "My layout", touch)),
            favoriteGameIds = listOf("game-1"),
            controllerMouseEmulation = true,
        ).normalizedForAndroid()

        val restored = decodeSettingsBackup(ByteArrayInputStream(encodeSettingsBackup(settings)))

        assertEquals(settings, restored)
    }

    @Test
    fun rejectsWrongFormatAndUnsupportedVersion() {
        val valid = encodeSettingsBackup(AppSettings()).toString(Charsets.UTF_8)
        assertThrows(IllegalArgumentException::class.java) {
            decodeSettingsBackup(ByteArrayInputStream(valid.replace("opennow-settings", "another-app").toByteArray()))
        }
        assertThrows(IllegalArgumentException::class.java) {
            decodeSettingsBackup(ByteArrayInputStream(valid.replace("\"version\":1", "\"version\":2").toByteArray()))
        }
    }

    @Test
    fun rejectsOversizedImportBeforeParsing() {
        val oversized = ByteArray(MAX_SETTINGS_BACKUP_BYTES + 1) { 'x'.code.toByte() }
        val error = assertThrows(IllegalArgumentException::class.java) {
            decodeSettingsBackup(ByteArrayInputStream(oversized))
        }
        assertTrue(error.message.orEmpty().contains("too large"))
    }
}
