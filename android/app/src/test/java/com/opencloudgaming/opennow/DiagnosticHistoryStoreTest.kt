package com.opencloudgaming.opennow

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DiagnosticHistoryStoreTest {
    @Test
    fun currentProcessSnapshotBecomesPreviousOnNextAppRun() {
        val directory = Files.createTempDirectory("diagnostic-history").toFile()
        val store = DiagnosticHistoryStore(directory) { 1234L }

        store.beginAppRun()
        assertNull(store.previousSnapshot())
        store.saveCurrent("OpenNOW Android diagnostics\nevent.1 stream failed")
        store.beginAppRun()

        assertEquals(
            PreviousDiagnosticSnapshot(
                capturedAtEpochMs = 1234L,
                text = "OpenNOW Android diagnostics\nevent.1 stream failed",
            ),
            store.previousSnapshot(),
        )
    }

    @Test
    fun launchWithoutANewCurrentSnapshotPreservesPreviousEvidence() {
        val directory = Files.createTempDirectory("diagnostic-history").toFile()
        var now = 1L
        val store = DiagnosticHistoryStore(directory) { now }
        store.saveCurrent("first run")
        store.beginAppRun()
        assertEquals("first run", store.previousSnapshot()?.text)

        now = 2L
        store.beginAppRun()

        assertEquals(1L, store.previousSnapshot()?.capturedAtEpochMs)
        assertEquals("first run", store.previousSnapshot()?.text)
    }

    @Test
    fun latestSnapshotFromTheRunReplacesEarlierSnapshots() {
        val directory = Files.createTempDirectory("diagnostic-history").toFile()
        var now = 10L
        val store = DiagnosticHistoryStore(directory) { now }
        store.saveCurrent("early")
        now = 20L
        store.saveCurrent("latest")

        store.beginAppRun()

        assertEquals(20L, store.previousSnapshot()?.capturedAtEpochMs)
        assertEquals("latest", store.previousSnapshot()?.text)
    }

    @Test
    fun corruptCurrentSnapshotDoesNotEraseTheLastReadableRun() {
        val directory = Files.createTempDirectory("diagnostic-history").toFile()
        val store = DiagnosticHistoryStore(directory) { 10L }
        store.saveCurrent("readable previous run")
        store.beginAppRun()
        File(directory, "diagnostic-history/current.txt.gz").writeText("not gzip")

        store.beginAppRun()

        assertEquals("readable previous run", store.previousSnapshot()?.text)
    }

    @Test
    fun boundedSnapshotKeepsBothTheHeaderAndLatestEvidence() {
        val original = "header-" + "x".repeat(500) + "-latest"

        val bounded = boundDiagnosticSnapshot(original, maxCharacters = 256)

        assertTrue(bounded.startsWith("header-"))
        assertTrue(bounded.endsWith("-latest"))
        assertTrue(bounded.contains("persisted diagnostic snapshot truncated"))
        assertTrue(bounded.length <= 256)
    }

    @Test
    fun persistenceLimitNeverCutsThroughParserJsonOrChangesApiIndexes() {
        val original = "Header\n<parser>\n" +
            """{"schemaVersion":2,"api":[{"statusCode":500,"response":{"detail":"${"x".repeat(10_000)}"}}]}""" + "\n</parser>"
        val bounded = boundDiagnosticSnapshot(original, 2048)
        val parser = OpenNowJson.parseToJsonElement(bounded.substringAfter("<parser>\n").substringBefore("\n</parser>"))
        val data = parser as kotlinx.serialization.json.JsonObject
        val api = data.getValue("api") as kotlinx.serialization.json.JsonArray
        assertTrue(api[0].toString().contains("500"))
        assertTrue(api[0].toString().contains("persisted_snapshot_limit"))
        assertTrue(bounded.length <= 2048)
    }

    @Test
    fun exportedLogLabelsPreviousRunWithoutChangingCurrentSection() {
        val current = "OpenNOW Android diagnostics\nstreamStatus=idle"
        val merged = appendPreviousDiagnosticSnapshot(
            current = current,
            previous = PreviousDiagnosticSnapshot(42L, "OpenNOW Android diagnostics\nstreamStatus=streaming"),
        )

        assertTrue(merged.startsWith(current))
        assertTrue(merged.contains("previousAppRun.capturedAtEpochMs=42"))
        assertTrue(merged.contains("----- BEGIN PREVIOUS APP RUN -----"))
        assertTrue(merged.contains("streamStatus=streaming"))
        assertFalse(appendPreviousDiagnosticSnapshot(current, null).contains("previousAppRun"))
    }
}
