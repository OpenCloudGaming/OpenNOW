package com.opencloudgaming.opennow

import kotlinx.serialization.json.*
import java.io.BufferedReader
import java.io.File
import java.io.InputStreamReader
import java.io.OutputStream
import java.io.OutputStreamWriter
import java.util.zip.Deflater
import java.util.zip.GZIPInputStream
import java.util.zip.GZIPOutputStream

internal data class PreviousDiagnosticSnapshot(
    val capturedAtEpochMs: Long,
    val text: String,
)

/**
 * Keeps the latest bounded diagnostic export from the current process so the next app run can
 * attach it. Snapshots are compressed because HTTP response diagnostics are intentionally rich,
 * and writing an uncompressed copy every few seconds would create unnecessary storage traffic on
 * lower-end Android TV hardware.
 */
internal class DiagnosticHistoryStore(
    directory: File,
    private val nowEpochMs: () -> Long = System::currentTimeMillis,
) {
    private val historyDirectory = File(directory, DIRECTORY_NAME)
    private val currentFile = File(historyDirectory, CURRENT_FILE_NAME)
    private val previousFile = File(historyDirectory, PREVIOUS_FILE_NAME)

    /**
     * Promotes the last process snapshot exactly once during Application startup. If the previous
     * process died before creating a usable snapshot, the older previous snapshot is preserved.
     */
    @Synchronized
    fun beginAppRun() {
        historyDirectory.mkdirs()
        recoverInterruptedReplacement(currentFile)
        recoverInterruptedReplacement(previousFile)
        val current = readSnapshot(currentFile)
        if (current == null) {
            currentFile.delete()
            return
        }

        val stagedPrevious = File(historyDirectory, "$PREVIOUS_FILE_NAME.stage")
        stagedPrevious.delete()
        currentFile.copyTo(stagedPrevious, overwrite = true)
        replaceFile(stagedPrevious, previousFile)
        currentFile.delete()
    }

    @Synchronized
    fun saveCurrent(text: String) {
        historyDirectory.mkdirs()
        recoverInterruptedReplacement(currentFile)
        val stagedCurrent = File(historyDirectory, "$CURRENT_FILE_NAME.stage")
        stagedCurrent.delete()
        val bounded = boundDiagnosticSnapshot(text)
        // This runs periodically during a stream. BEST_SPEED keeps the crash-history feature while
        // minimizing CPU contention with WebRTC/decoder threads; snapshots are small and rotated.
        FastGzipOutputStream(stagedCurrent.outputStream().buffered()).use { compressed ->
            OutputStreamWriter(compressed, Charsets.UTF_8).use { writer ->
                writer.append(nowEpochMs().toString())
                writer.append('\n')
                writer.append(bounded)
            }
        }
        replaceFile(stagedCurrent, currentFile)
    }

    @Synchronized
    fun previousSnapshot(): PreviousDiagnosticSnapshot? {
        recoverInterruptedReplacement(previousFile)
        return readSnapshot(previousFile)
    }

    private fun readSnapshot(file: File): PreviousDiagnosticSnapshot? {
        if (!file.isFile || file.length() <= 0L) return null
        return runCatching {
            GZIPInputStream(file.inputStream().buffered()).use { compressed ->
                BufferedReader(InputStreamReader(compressed, Charsets.UTF_8)).use { reader ->
                    val capturedAt = reader.readLine()?.toLongOrNull() ?: return null
                    val text = reader.readText().trimEnd()
                    if (text.isBlank()) return null
                    PreviousDiagnosticSnapshot(capturedAtEpochMs = capturedAt, text = text)
                }
            }
        }.getOrNull()
    }

    private fun recoverInterruptedReplacement(target: File) {
        val backup = File(historyDirectory, "${target.name}.backup")
        if (!target.exists() && backup.isFile) {
            backup.renameTo(target)
        } else if (target.exists()) {
            backup.delete()
        }
    }

    private fun replaceFile(staged: File, target: File) {
        val backup = File(historyDirectory, "${target.name}.backup")
        backup.delete()
        val hadTarget = target.isFile
        if (hadTarget && !target.renameTo(backup)) {
            staged.delete()
            error("Could not stage existing diagnostic history")
        }
        if (!staged.renameTo(target)) {
            if (hadTarget) backup.renameTo(target)
            staged.delete()
            error("Could not save diagnostic history")
        }
        backup.delete()
    }

    private companion object {
        const val DIRECTORY_NAME = "diagnostic-history"
        const val CURRENT_FILE_NAME = "current.txt.gz"
        const val PREVIOUS_FILE_NAME = "previous.txt.gz"
    }
}

private class FastGzipOutputStream(output: OutputStream) : GZIPOutputStream(output) {
    init {
        def.setLevel(Deflater.BEST_SPEED)
    }
}

internal fun boundDiagnosticSnapshot(
    text: String,
    maxCharacters: Int = 16_777_216,
): String {
    require(maxCharacters >= 256)
    if (text.length <= maxCharacters) return text
    // Never splice through JSON: that would destroy all machine-readable evidence in the run.
    val parserStart = text.indexOf("<parser>\n")
    val parserEnd = text.lastIndexOf("\n</parser>")
    if (parserStart >= 0 && parserEnd > parserStart) {
        val parsed = runCatching { OpenNowJson.parseToJsonElement(text.substring(parserStart + 9, parserEnd)).jsonObject }.getOrNull()
        if (parsed != null) {
            val prefix = text.take(minOf(parserStart, maxCharacters / 4, 32_000)) + "\n[persisted snapshot reduced to fit retention limit]\n"
            val data = parsed.toMutableMap()
            data["persistence"] = buildJsonObject {
                put("truncated", true); put("originalCharacters", text.length); put("characterLimit", maxCharacters)
            }
            // Keep request records/statuses and evidence pointers; only replace oversized bodies.
            val api = (data["api"] as? JsonArray)?.toMutableList()
            if (api != null) {
                var estimatedSize = JsonObject(data).toString().length + prefix.length + 32
                val payloads = api.flatMapIndexed { index, entry ->
                    (entry as? JsonObject)?.filterKeys { it in setOf("request", "response", "requestQuery", "requestHeaders", "responseHeaders") }
                        ?.map { (key, value) -> Triple(index, key, value.toString().length) }.orEmpty()
                }.sortedByDescending { it.third }
                for ((index, key, length) in payloads) {
                    if (estimatedSize <= maxCharacters) break
                    if (length <= 160) continue
                    val marker = buildJsonObject { put("omitted", true); put("reason", "persisted_snapshot_limit"); put("originalCharacters", length) }
                    api[index] = JsonObject(api[index].jsonObject + (key to marker))
                    estimatedSize -= length - marker.toString().length
                }
                data["api"] = JsonArray(api)
            }
            val reduced = prefix + diagnosticParserBlock(JsonObject(data))
            if (reduced.length <= maxCharacters) return reduced
        }
        // An exceptional non-API oversized snapshot must still be parseable and explicit about loss.
        return diagnosticParserBlock(buildJsonObject {
            put("schemaVersion", 2); put("incomplete", true)
            put("persistence", buildJsonObject {
                put("truncated", true); put("originalCharacters", text.length); put("characterLimit", maxCharacters)
            })
        })
    }
    val marker = "\n... persisted diagnostic snapshot truncated ${text.length - maxCharacters} characters ...\n"
    val available = (maxCharacters - marker.length).coerceAtLeast(2)
    val headLength = available / 2
    val tailLength = available - headLength
    return text.take(headLength) + marker + text.takeLast(tailLength)
}

internal fun appendPreviousDiagnosticSnapshot(
    current: String,
    previous: PreviousDiagnosticSnapshot?,
): String {
    if (previous == null) return current
    return buildString(current.length + previous.text.length + 160) {
        append(current.trimEnd())
        appendLine()
        appendLine()
        appendLine("previousAppRun.diagnostics:")
        appendLine("previousAppRun.capturedAtEpochMs=${previous.capturedAtEpochMs}")
        appendLine("previousAppRun.capturedAt=${diagnosticTimestamp(previous.capturedAtEpochMs)}")
        appendLine("----- BEGIN PREVIOUS APP RUN -----")
        appendLine(previous.text.trimEnd())
        append("----- END PREVIOUS APP RUN -----")
    }
}
