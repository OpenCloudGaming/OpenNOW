package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AppUpdateTest {
    @Test
    fun parsesManifestWithRelativeApkUrl() {
        val candidate = parseAndroidUpdateCandidate(
            "https://updates.example.com/android/opennow.json",
            """
                {
                  "versionCode": 7,
                  "versionName": "0.5.2",
                  "apkUrl": "OpenNOW-0.5.2.apk",
                  "sha256": "SHA256: AA BB CC",
                  "releaseNotes": "Native Android update"
                }
            """.trimIndent(),
        )

        assertEquals("https://updates.example.com/android/opennow.json", candidate?.sourceUrl)
        assertEquals("https://updates.example.com/android/OpenNOW-0.5.2.apk", candidate?.apkUrl)
        assertEquals("0.5.2", candidate?.versionName)
        assertEquals(7L, candidate?.versionCode)
        assertEquals("aabbcc", candidate?.sha256)
        assertEquals("Native Android update", candidate?.releaseNotes)
    }

    @Test
    fun parsesNestedAndroidManifest() {
        val candidate = parseAndroidUpdateCandidate(
            "https://updates.example.com/releases/latest.json",
            """
                {
                  "android": {
                    "version_code": 8,
                    "version_name": "0.5.3",
                    "download_url": "https://cdn.example.com/OpenNOW-0.5.3.apk",
                    "release_notes": "Fix haptics\\nImprove updater\\r\\nClean up stream UI"
                  }
                }
            """.trimIndent(),
        )

        assertEquals("https://cdn.example.com/OpenNOW-0.5.3.apk", candidate?.apkUrl)
        assertEquals("0.5.3", candidate?.versionName)
        assertEquals(8L, candidate?.versionCode)
        assertEquals("Fix haptics\nImprove updater\nClean up stream UI", candidate?.releaseNotes)
    }

    @Test
    fun parsesGithubReleaseApkAsset() {
        val candidate = parseAndroidUpdateCandidate(
            "https://api.github.com/repos/OpenCloudGaming/OpenNOW/releases/latest",
            """
                {
                  "tag_name": "v0.5.4",
                  "body": "Release notes",
                  "assets": [
                    {
                      "name": "OpenNOW-desktop.zip",
                      "browser_download_url": "https://github.com/example/desktop.zip"
                    },
                    {
                      "name": "OpenNOW-android.apk",
                      "browser_download_url": "https://github.com/example/OpenNOW-android.apk",
                      "digest": "sha256:012345"
                    }
                  ]
                }
            """.trimIndent(),
        )

        assertEquals("https://github.com/example/OpenNOW-android.apk", candidate?.apkUrl)
        assertEquals("0.5.4", candidate?.versionName)
        assertEquals("012345", candidate?.sha256)
        assertEquals("Release notes", candidate?.releaseNotes)
    }

    @Test
    fun parsesPrintedWasteManifestShape() {
        val candidate = parseAndroidUpdateCandidate(
            ANDROID_UPDATE_SOURCE_URL,
            """
                {
                  "id": "5b000fc7-4f4c-464d-862a-ce9409c61081",
                  "appSlug": "opennow",
                  "appName": "OpenNOW",
                  "platform": "android",
                  "channel": "stable",
                  "versionCode": 6,
                  "versionName": "0.5.1",
                  "artifactUrl": "https://api.printedwaste.com/release-files/opennow/app-release.apk",
                  "url": "https://api.printedwaste.com/release-files/opennow/app-release.apk",
                  "sha256": "8bfd318dad6b2590e23d39237d02fb7bfec9fde1d09b6f08aff368ba5e9b073c",
                  "releaseNotes": "- Autoupdating\n- Filter fixies\n- Optimizations",
                  "mandatory": false,
                  "draft": false,
                  "apkUrl": "https://api.printedwaste.com/release-files/opennow/app-release.apk"
                }
            """.trimIndent(),
        )

        assertEquals(ANDROID_UPDATE_SOURCE_URL, candidate?.sourceUrl)
        assertEquals("https://api.printedwaste.com/release-files/opennow/app-release.apk", candidate?.apkUrl)
        assertEquals("0.5.1", candidate?.versionName)
        assertEquals(6L, candidate?.versionCode)
        assertEquals("8bfd318dad6b2590e23d39237d02fb7bfec9fde1d09b6f08aff368ba5e9b073c", candidate?.sha256)
        assertEquals("- Autoupdating\n- Filter fixies\n- Optimizations", candidate?.releaseNotes)
    }

    @Test
    fun rejectsManifestWithoutApkUrl() {
        val candidate = parseAndroidUpdateCandidate(
            "https://updates.example.com/opennow.json",
            """{"versionCode": 7, "versionName": "0.5.2"}""",
        )

        assertNull(candidate)
    }

    @Test
    fun normalizesHttpsSourceWithoutScheme() {
        assertEquals("https://updates.example.com/opennow.json", normalizeAndroidUpdateSourceUrl("updates.example.com/opennow.json"))
    }

    @Test(expected = IllegalStateException::class)
    fun rejectsNonLoopbackHttpSource() {
        normalizeAndroidUpdateSourceUrl("http://updates.example.com/opennow.json")
    }

    @Test
    fun updateChecksAreBlockedAcrossLocalStreamLifecycle() {
        assertFalse(OpenNowUiState().isAndroidUpdateCheckBlockedByStream())
        assertTrue(OpenNowUiState(streamStatus = "queue").isAndroidUpdateCheckBlockedByStream())
        assertTrue(OpenNowUiState(streamStatus = "connecting").isAndroidUpdateCheckBlockedByStream())
        assertTrue(OpenNowUiState(streamStatus = "streaming").isAndroidUpdateCheckBlockedByStream())
        assertTrue(
            OpenNowUiState(
                streamStatus = "idle",
                activeStreamSettings = StreamSettings(),
            ).isAndroidUpdateCheckBlockedByStream(),
        )
    }

    @Test
    fun updateNoticeKeyIsStableAcrossAvailableAndDownloadedStates() {
        val available = AndroidUpdateState(
            status = AndroidUpdateStatus.Available,
            availableVersionName = "0.5.4",
            availableVersionCode = 9,
        )
        val downloaded = available.copy(status = AndroidUpdateStatus.Downloaded)

        assertEquals(androidUpdateNoticeKey(available), androidUpdateNoticeKey(downloaded))
        assertEquals(null, available.visibleNoticeKey(androidUpdateNoticeKey(downloaded)))
        assertEquals(androidUpdateNoticeKey(available), available.visibleNoticeKey(null))
    }
}
