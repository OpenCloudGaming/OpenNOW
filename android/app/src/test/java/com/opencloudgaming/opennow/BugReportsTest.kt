package com.opencloudgaming.opennow

import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BugReportsTest {
    private val reporterId = androidBugReportReporterId("test-gfn-device-id")

    @Test
    fun nvstWarningIsShownOnlyForTheExperimentalTransport() {
        assertEquals(ANDROID_BUG_REPORT_NVST_WARNING, androidBugReportNvstWarning(true))
        assertEquals(null, androidBugReportNvstWarning(false))
        assertTrue(ANDROID_BUG_REPORT_NVST_WARNING.contains("bugs are expected"))
        assertTrue(ANDROID_BUG_REPORT_NVST_WARNING.contains("rude"))
        assertTrue(ANDROID_BUG_REPORT_NVST_WARNING.contains("will be ignored"))
    }

    @Test
    fun buildsPrintedWasteMultipartReportWithRedactedLogAttachment() {
        val request = buildAndroidBugReportRequest(
            AndroidBugReport(
                title = " Stream froze ",
                description = " Video stopped after reconnecting and remained frozen until I restarted the session. ",
                versionName = "0.9.0",
                versionCode = "45",
                reporterId = reporterId,
                appLanguageSelectionTag = "en-US",
                languageCheck = englishLanguageCheck,
                metadata = """{"device":"Pixel 9","sessionId":"[redacted]"}""",
                files = listOf(
                    AndroidBugReportAttachment(
                        fileName = "opennow.log",
                        contentType = "text/plain; charset=utf-8",
                        bytes = "sessionId=[redacted]".toByteArray(),
                    ),
                ),
            ),
        )

        val buffer = Buffer()
        requireNotNull(request.body).writeTo(buffer)
        val multipart = buffer.readUtf8()

        assertEquals(ANDROID_BUG_REPORT_ENDPOINT, request.url.toString())
        assertEquals("POST", request.method)
        assertTrue(multipart.contains("name=\"title\"\r\n\r\nStream froze"))
        assertTrue(
            multipart.contains(
                "name=\"description\"\r\n\r\nVideo stopped after reconnecting and remained frozen until I restarted the session.",
            ),
        )
        assertTrue(multipart.contains("name=\"versionName\"\r\n\r\n0.9.0"))
        assertTrue(multipart.contains("name=\"versionCode\"\r\n\r\n45"))
        assertTrue(multipart.contains("name=\"platform\"\r\n\r\nandroid"))
        assertTrue(multipart.contains("name=\"reporterId\"\r\n\r\n$reporterId"))
        assertTrue(multipart.contains("name=\"files\"; filename=\"opennow.log\""))
        assertTrue(multipart.contains("sessionId=[redacted]"))
    }

    @Test
    fun metadataIncludesDeviceAndAndroidContextForTriage() {
        val fileName = "opennow-android-logs-20260718-123456.txt"
        val metadata = buildAndroidBugReportMetadata(fileName, device = testDeviceDiagnostics)

        assertTrue(metadata.contains("\"source\":\"settings-advanced-debug-logs\""))
        assertTrue(metadata.contains("\"attachment\":\"$fileName\""))
        assertTrue(metadata.contains("\"manufacturer\":\"Google\""))
        assertTrue(metadata.contains("\"model\":\"Pixel_9_Pro\""))
        assertTrue(metadata.contains("\"sdk\":35"))
        assertTrue(metadata.contains("\"targetSdk\":36"))
        assertTrue(metadata.contains("\"supportedAbis\":[\"arm64-v8a\"]"))
        assertTrue(metadata.contains("\"widthPixels\":1440"))
        assertFalse(metadata.contains("sessionId"))
    }

    @Test
    fun metadataRecordsAUserAcknowledgedKnownIssueOverride() {
        val metadata = buildAndroidBugReportMetadata(
            logFileName = "opennow-android-logs.txt",
            knownIssueOverrideKey = "network-2.4ghz",
        )

        assertTrue(metadata.contains("\"knownIssueOverride\":true"))
        assertTrue(metadata.contains("\"knownIssueKey\":\"network-2.4ghz\""))
    }

    private val testDeviceDiagnostics = AndroidDeviceDiagnosticsSnapshot(
        manufacturer = "Google",
        brand = "google",
        model = "Pixel_9_Pro",
        deviceCodename = "komodo",
        product = "komodo",
        hardware = "komodo",
        board = "komodo",
        androidRelease = "15",
        androidCodename = "REL",
        androidSdk = 35,
        targetSdk = 36,
        securityPatch = "2026-07-05",
        supportedAbis = listOf("arm64-v8a"),
        is64BitRuntime = true,
        processorCount = 8,
        totalMemoryMiB = 12_288,
        lowRamDevice = false,
        displayWidthPixels = 1440,
        displayHeightPixels = 3120,
        densityDpi = 512,
        smallestScreenWidthDp = 411,
        formFactor = "phone",
        emulator = false,
    )

    @Test(expected = IllegalArgumentException::class)
    fun rejectsMoreThanFiveFiles() {
        val files = (1..6).map { index ->
            AndroidBugReportAttachment("$index.log", "text/plain", byteArrayOf())
        }
        buildAndroidBugReportRequest(
            AndroidBugReport(
                "Title",
                "The stream stopped decoding video after a reconnect and did not recover.",
                "0.9.0",
                "45",
                reporterId,
                "en",
                englishLanguageCheck,
                "{}",
                files,
            ),
        )
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejectsFilesLargerThanTenMib() {
        buildAndroidBugReportRequest(
            AndroidBugReport(
                "Title",
                "The stream stopped decoding video after a reconnect and did not recover.",
                "0.9.0",
                "45",
                reporterId,
                "en",
                englishLanguageCheck,
                "{}",
                listOf(
                    AndroidBugReportAttachment(
                        "too-large.log",
                        "text/plain",
                        ByteArray(ANDROID_BUG_REPORT_MAX_FILE_BYTES.toInt() + 1),
                    ),
                ),
            ),
        )
    }

    @Test
    fun parsesServerReferenceWhenPresent() {
        assertEquals("report-123", parseAndroidBugReportReference("""{"id":"report-123"}"""))
        assertEquals("report-456", parseAndroidBugReportReference("""{"reportId":" report-456 "}"""))
        assertEquals("report-789", parseAndroidBugReportReference("""{"bugReportId":"report-789"}"""))
        assertEquals(null, parseAndroidBugReportReference("""{"ok":true}"""))
    }

    @Test
    fun acceptedReceiptReturnsTheReportId() {
        assertEquals(
            "report-456",
            parseAndroidBugReportReceipt("""{"ok":true,"reportId":"report-456"}""").reference,
        )
    }

    @Test(expected = AndroidBugReportUploadException::class)
    fun acceptedReceiptWithoutAReportIdIsRejected() {
        parseAndroidBugReportReceipt("""{"ok":true}""")
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejectsDescriptionsShorterThanFiftyCharacters() {
        buildAndroidBugReportRequest(
            AndroidBugReport(
                title = "Lag",
                description = "It lagged.",
                versionName = "1.0.5",
                versionCode = "60",
                reporterId = reporterId,
                appLanguageSelectionTag = "en",
                languageCheck = englishLanguageCheck,
                metadata = "{}",
                files = emptyList(),
            ),
        )
    }

    @Test
    fun playStoreReportsRequireAFreshCurrentVersionCheck() {
        val playUpdate = AndroidUpdateState(
            installSource = AndroidAppInstallSource(setOf(GOOGLE_PLAY_STORE_PACKAGE)),
            status = AndroidUpdateStatus.NotAvailable,
        )

        assertFalse(
            androidBugReportsAllowed(
                playUpdate,
                AndroidBugReportVersionCheckState(AndroidBugReportVersionCheckStatus.NotChecked),
            ),
        )
        assertTrue(
            androidBugReportsAllowed(
                playUpdate,
                AndroidBugReportVersionCheckState(AndroidBugReportVersionCheckStatus.Current),
            ),
        )
        assertFalse(
            androidBugReportsAllowed(
                playUpdate.copy(status = AndroidUpdateStatus.Available),
                AndroidBugReportVersionCheckState(AndroidBugReportVersionCheckStatus.Current),
            ),
        )
    }

    @Test
    fun sideloadReportsDoNotDependOnGooglePlayVerification() {
        val sideloadUpdate = AndroidUpdateState(
            installSource = AndroidAppInstallSource(emptySet()),
            status = AndroidUpdateStatus.Idle,
        )

        assertTrue(
            androidBugReportsAllowed(
                sideloadUpdate,
                AndroidBugReportVersionCheckState(AndroidBugReportVersionCheckStatus.NotChecked),
            ),
        )
    }

    @Test
    fun reporterIdIsStableButDoesNotExposeTheRawProviderDeviceId() {
        val rawDeviceId = "4fe17fe6-4b40-4897-bc3a-1e61cb4fd3aa"
        val first = androidBugReportReporterId(rawDeviceId)
        val second = androidBugReportReporterId(rawDeviceId)
        val different = androidBugReportReporterId("a-different-installation")

        assertEquals(first, second)
        assertTrue(first.startsWith(ANDROID_BUG_REPORT_REPORTER_ID_PREFIX))
        assertEquals(ANDROID_BUG_REPORT_REPORTER_ID_PREFIX.length + 64, first.length)
        assertFalse(first.contains(rawDeviceId))
        assertFalse(first == different)
    }

    @Test
    fun parsesStructuredBanMessageForDisplay() {
        val error = parseAndroidBugReportServerError(
            body = """
                {
                  "ok": false,
                  "error": {
                    "code": "REPORTER_BANNED",
                    "message": "Bug reporting is disabled for this installation.  Contact support if this is a mistake.",
                    "retryable": false
                  }
                }
            """.trimIndent(),
            statusCode = 403,
        )

        assertEquals("REPORTER_BANNED", error.code)
        assertEquals(
            "Bug reporting is disabled for this installation. Contact support if this is a mistake.",
            error.message,
        )
        assertEquals(false, error.retryable)
    }

    @Test
    fun nonJsonFailureUsesSafeStatusMessageInsteadOfRawResponse() {
        val error = parseAndroidBugReportServerError(
            body = "<html>private reverse proxy failure details</html>",
            statusCode = 502,
        )

        assertEquals("Bug report upload failed (HTTP 502).", error.message)
        assertFalse(error.message.contains("private reverse proxy"))
    }

    @Test
    fun rejectsRandomOrRepeatedPaddingThatOnlyPassesTheRawCharacterLimit() {
        assertTrue(
            androidBugReportDescriptionError("eworuejwgojug ".repeat(8))
                ?.contains("complete English sentences") == true,
        )
        assertTrue(
            androidBugReportDescriptionError(
                "the stream froze while loading the game the stream froze while loading the game",
            )?.contains("repeated or random text") == true,
        )
    }

    @Test
    fun acceptsDetailedDescriptionWithEnoughMeaningfulEnglishWords() {
        assertEquals(
            null,
            androidBugReportDescriptionError(
                "The video froze after I reopened the app, while audio continued until I ended the stream.",
            ),
        )
    }

    @Test
    fun languageCandidatesMustConfidentlyIdentifyEnglish() {
        assertEquals(
            null,
            androidBugReportLanguageError(
                listOf(AndroidBugReportLanguageCandidate("en", 0.91f)),
            ),
        )
        assertTrue(
            androidBugReportLanguageError(
                listOf(
                    AndroidBugReportLanguageCandidate("es", 0.82f),
                    AndroidBugReportLanguageCandidate("en", 0.12f),
                ),
            )?.contains("clear English") == true,
        )
        assertTrue(
            androidBugReportLanguageError(
                listOf(AndroidBugReportLanguageCandidate("und", 1.0f)),
            )?.contains("unrecognizable") == true,
        )
    }

    @Test
    fun mlKitNullFailureCanUseAlreadyValidatedDetailedReport() {
        val check = androidBugReportLanguageCheckAfterMlKitNullFailure(
            title = "Video freezes after reconnect",
            description = "The video stopped after reconnecting, but audio continued until I manually ended the stream.",
        )

        assertEquals("en", check.languageTag)
        assertEquals(ANDROID_BUG_REPORT_MIN_ENGLISH_CONFIDENCE, check.confidence)
    }

    @Test(expected = IllegalArgumentException::class)
    fun mlKitNullFailureDoesNotBypassContentValidation() {
        androidBugReportLanguageCheckAfterMlKitNullFailure(
            title = "Lag",
            description = "It lagged.",
        )
    }

    @Test(expected = IllegalArgumentException::class)
    fun requestBuilderRejectsReportsWhenTheAppLocaleIsNotEnglish() {
        buildAndroidBugReportRequest(
            validReport().copy(appLanguageSelectionTag = "fr-FR"),
        )
    }

    @Test(expected = IllegalArgumentException::class)
    fun requestBuilderRejectsReportsWithoutAConfidentEnglishLanguageCheck() {
        buildAndroidBugReportRequest(
            validReport().copy(
                languageCheck = AndroidBugReportLanguageCheck("es", 0.97f),
            ),
        )
    }

    @Test
    fun appLocaleGateAllowsAnEnglishAppOrDeviceLanguage() {
        assertTrue(androidAppLocaleIsEnglish("en-CA"))
        assertTrue(androidAppLocaleIsEnglish("en_GB"))
        assertFalse(androidAppLocaleIsEnglish("fr-CA"))
        assertFalse(androidAppLocaleIsEnglish(""))
        assertTrue(
            AndroidAppLocaleState(
                selectedLanguageTag = "",
                effectiveLanguageTag = "en-CA",
            ).bugReportsAllowed,
        )
        assertTrue(
            AndroidAppLocaleState(
                selectedLanguageTag = "fr",
                effectiveLanguageTag = "fr-FR",
                deviceLanguageTag = "en-US",
            ).bugReportsAllowed,
        )
        assertTrue(
            AndroidAppLocaleState(
                selectedLanguageTag = "en",
                effectiveLanguageTag = "en-CA",
                deviceLanguageTag = "fr-FR",
            ).bugReportsAllowed,
        )
        assertFalse(
            AndroidAppLocaleState(
                selectedLanguageTag = "fr",
                effectiveLanguageTag = "fr-FR",
                deviceLanguageTag = "de-DE",
            ).bugReportsAllowed,
        )
        assertEquals(
            "en-US",
            AndroidAppLocaleState("fr", "fr-FR", "en-US").bugReportLanguageTag,
        )
    }

    @Test
    fun androidAppLanguageSelectionSupportsEveryBundledLocale() {
        assertTrue(androidAppLanguageSelectionIsSupported(""))
        listOf("en", "ar", "de", "es", "fr", "ja", "ko", "nl", "pl", "pt", "ro", "ru", "tr", "zh-Hans")
            .forEach { languageTag ->
                assertTrue(languageTag, androidAppLanguageSelectionIsSupported(languageTag))
            }
        assertFalse(androidAppLanguageSelectionIsSupported("pt-BR"))
        assertFalse(androidAppLanguageSelectionIsSupported("zh-Hant"))
    }

    private fun validReport() = AndroidBugReport(
        title = "Video freezes after reconnect",
        description = "The video stopped after reconnecting, but audio continued until I manually ended the stream.",
        versionName = "1.2.2",
        versionCode = "78",
        reporterId = reporterId,
        appLanguageSelectionTag = "en-CA",
        languageCheck = englishLanguageCheck,
        metadata = "{}",
        files = emptyList(),
    )

    private val englishLanguageCheck = AndroidBugReportLanguageCheck("en", 0.95f)
}
