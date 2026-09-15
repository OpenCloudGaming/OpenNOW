package com.opencloudgaming.opennow

import java.io.File
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.descriptors.*
import kotlinx.serialization.json.*
import org.junit.Assert.assertEquals
import org.junit.Test

/** Keep the public schema in sync with the serialized device/settings/report DTOs. */
@OptIn(ExperimentalSerializationApi::class)
class DiagnosticSchemaTest {
    private val definitions = linkedMapOf<String, JsonElement>()
    private fun type(name: String) = buildJsonObject { put("type", name) }
    private val str = type("string")
    private val int = type("integer")
    private val num = type("number")
    private val bool = type("boolean")
    private fun array(item: JsonElement) = buildJsonObject { put("type", "array"); put("items", item) }
    private fun nullable(value: JsonElement) = buildJsonObject { put("anyOf", JsonArray(listOf(value, type("null")))) }
    private fun obj(properties: Map<String, JsonElement>, required: List<String> = properties.keys.toList()) = buildJsonObject {
        put("type", "object")
        put("properties", JsonObject(properties))
        put("required", JsonArray(required.map(::JsonPrimitive)))
        // Allow additions within a format version. Consumers must ignore unknown fields.
        put("additionalProperties", true)
    }
    private fun constant(value: String) = buildJsonObject { put("const", value) }
    private fun descriptor(d: SerialDescriptor, allowNull: Boolean = true): JsonElement {
        if (d.isNullable && allowNull) return nullable(descriptor(d, false))
        return when (d.kind) {
            PrimitiveKind.BOOLEAN -> bool
            PrimitiveKind.BYTE, PrimitiveKind.SHORT, PrimitiveKind.INT, PrimitiveKind.LONG -> int
            PrimitiveKind.FLOAT, PrimitiveKind.DOUBLE -> num
            PrimitiveKind.CHAR, PrimitiveKind.STRING -> str
            StructureKind.LIST -> array(descriptor(d.getElementDescriptor(0)))
            StructureKind.MAP -> buildJsonObject {
                put("type", "object"); put("additionalProperties", descriptor(d.getElementDescriptor(1)))
            }
            SerialKind.ENUM -> buildJsonObject {
                put("type", "string")
                put("enum", JsonArray((0 until d.elementsCount).map { JsonPrimitive(d.getElementName(it)) }))
            }
            StructureKind.CLASS, StructureKind.OBJECT -> {
                val name = d.serialName.substringAfterLast('.').removeSuffix("?")
                if (name !in definitions) {
                    definitions[name] = JsonObject(emptyMap())
                    definitions[name] = obj(
                        (0 until d.elementsCount).associate { d.getElementName(it) to descriptor(d.getElementDescriptor(it)) },
                        (0 until d.elementsCount).filter { !d.isElementOptional(it) && !d.getElementDescriptor(it).isNullable }.map(d::getElementName),
                    )
                }
                buildJsonObject { put("\$ref", "#/\$defs/$name") }
            }
            else -> error("Unsupported diagnostic schema type: ${d.serialName}")
        }
    }

    @Test
    fun publishedSchemaMatchesParserContracts() {
        val inputEntry = obj(mapOf("timestamp" to str, "uptimeMs" to int, "lastSeenUptimeMs" to int,
            "message" to str, "count" to int), listOf("timestamp", "uptimeMs", "lastSeenUptimeMs", "message"))
        val api = obj(mapOf("timestamp" to str, "firstTimestamp" to str, "count" to int,
            "method" to str, "url" to str, "statusCode" to nullable(int), "elapsedMs" to int,
            "requestBytes" to int, "responseChars" to int, "request" to JsonObject(emptyMap()),
            "response" to JsonObject(emptyMap()), "error" to str, "requestQuery" to type("object"),
            "requestHeaders" to type("object"), "responseHeaders" to type("object"), "truncatedBodies" to int),
            listOf("timestamp", "firstTimestamp", "count", "method", "url", "statusCode", "elapsedMs", "responseChars"))
        val component = obj(mapOf("available" to bool, "score" to nullable(int), "weight" to int))
        val score = obj(mapOf("available" to bool, "phase" to buildJsonObject {
            put("enum", JsonArray(listOf("active", "completed", "unavailable").map(::JsonPrimitive)))
        }, "showAfterStream" to bool, "scale" to buildJsonObject { put("const", 100) },
            "algorithmVersion" to int, "gaugeFraction" to buildJsonObject {
                put("type", "number"); put("minimum", 0); put("maximum", 1)
            }, "unavailableReason" to str, "report" to descriptor(SessionReport.serializer().descriptor),
            "components" to obj(listOf("latency", "packetLoss", "jitter", "frameRate", "decode").associateWith { component })),
            listOf("available", "phase", "showAfterStream", "scale", "algorithmVersion"))
        val finding = obj(mapOf("reasonCode" to str, "category" to str, "confidence" to str,
            "possibleReason" to str, "evidencePaths" to array(str), "timestamp" to str,
            "httpStatus" to nullable(int), "providerStatus" to type("object"), "error" to str),
            listOf("reasonCode", "category", "confidence", "possibleReason", "evidencePaths"))
        val properties = linkedMapOf(
            "schemaVersion" to buildJsonObject { put("const", 2) },
            "capturedAt" to str, "capturedAtEpochMs" to int, "uptimeMs" to int, "timezone" to constant("UTC"),
            "app" to obj(mapOf("version" to str, "build" to int, "debug" to bool, "distribution" to str,
                "provider" to nullable(str), "membershipTier" to nullable(str))),
            "device" to descriptor(AndroidDeviceDiagnosticsSnapshot.serializer().descriptor),
            "deviceRuntime" to descriptor(AndroidRuntimeDiagnosticsSnapshot.serializer().descriptor),
            "stream" to obj(mapOf("state" to str, "game" to nullable(str), "launchPhase" to str,
                "queuePosition" to nullable(int), "error" to nullable(str), "providerDefaultUrl" to nullable(str),
                "sessionBaseUrl" to nullable(str), "settings" to descriptor(StreamSettings.serializer().descriptor),
                "activeSettings" to descriptor(StreamSettings.serializer().descriptor),
                "storage" to descriptor(StorageAddon.serializer().descriptor),
                "latestStatsCapturedAt" to str, "latestStats" to descriptor(StreamRuntimeStats.serializer().descriptor),
                "session" to descriptor(SessionInfo.serializer().descriptor),
                "samples" to array(descriptor(DiagnosticStreamSample.serializer().descriptor))),
                listOf("state", "game", "launchPhase", "queuePosition", "error", "providerDefaultUrl", "sessionBaseUrl", "settings")),
            "inputSettings" to obj(mapOf("mouseLock" to bool, "touch" to descriptor(AndroidTouchSettings.serializer().descriptor))),
            "codecs" to descriptor(RuntimeCodecReport.serializer().descriptor),
            "cpuBuckets" to array(descriptor(CpuDiagnosticBucket.serializer().descriptor)),
            "cpuSamples" to array(descriptor(ProcessCpuUsageSample.serializer().descriptor)),
            "retention" to obj(mapOf("api" to obj(mapOf("mode" to constant("individual_requests"),
                "totalRecorded" to int, "retainedRecords" to int, "evictedRecords" to int, "recordLimit" to int,
                "capturedChars" to int, "characterLimit" to int, "bodyCharacterLimit" to int, "truncatedBodies" to int)),
                "stream" to obj(mapOf("sampleLimit" to int, "totalSamples" to int, "retainedSamples" to int, "evictedSamples" to int)))),
            "input" to obj(mapOf("state" to buildJsonObject {
                put("type", "object"); put("additionalProperties", inputEntry)
            }, "events" to array(inputEntry))),
            "events" to array(obj(mapOf("timestamp" to str, "category" to str, "message" to str))),
            "api" to array(api), "sessionScore" to score,
            "processExitHistory" to descriptor(DiagnosticProcessExitHistory.serializer().descriptor),
            "failureAssessment" to obj(mapOf("rulesVersion" to int, "crashConclusion" to constant("not_established"),
                "crashEvidence" to str, "scope" to constant("retained_run_records"), "limitations" to str, "findings" to array(finding))),
        )
        val schema = buildJsonObject {
            put("\$schema", "https://json-schema.org/draft/2020-12/schema")
            put("title", "OpenNOW Android diagnostic parser format 2")
            put("anyOf", JsonArray(listOf(
                obj(properties, properties.keys.filter { it != "codecs" }),
                obj(mapOf("schemaVersion" to buildJsonObject { put("const", 2) },
                    "incomplete" to buildJsonObject { put("const", true) },
                    "persistence" to obj(mapOf("truncated" to buildJsonObject { put("const", true) },
                        "originalCharacters" to int, "characterLimit" to int)))),
            )))
            put("\$defs", JsonObject(definitions))
        }
        val file = File("../docs/diagnostics-parser.schema.json")
        if (System.getenv("OPENNOW_UPDATE_DIAGNOSTIC_SCHEMA") == "1") {
            file.writeText(Json { prettyPrint = true }.encodeToString(JsonObject.serializer(), schema) + "\n")
        }
        assertEquals("Update schema with OPENNOW_UPDATE_DIAGNOSTIC_SCHEMA=1 when intentionally changing the format",
            schema, OpenNowJson.parseToJsonElement(file.readText()))
    }
}
