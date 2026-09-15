# Android diagnostics format 2

Exports contain a readable report followed by a JSON document between lines containing
`<parser>` and `</parser>`. JSON is data, not executable code. A previous app run may follow
with its own block; the first block is always the current run.

```python
import json
import re
from pathlib import Path

text = Path("opennow-android-logs.txt").read_text(encoding="utf-8")
runs = [json.loads(block) for block in
        re.findall(r"(?ms)^<parser>\r?\n(.*?)^</parser>$", text)]
current = runs[0]
assert current["schemaVersion"] == 2
if current.get("incomplete"):
    raise ValueError("Previous snapshot exceeded its retention limit")
print(current["device"])
print(current["stream"])
for call in current["api"]:
    if call["statusCode"] is None or call["statusCode"] >= 400:
        print(call["timestamp"], call["url"], call.get("response"))
```

## Fields

| Field | Contents |
| --- | --- |
| `schemaVersion` | Integer format version; currently `2` |
| `capturedAt`, `capturedAtEpochMs`, `uptimeMs`, `timezone` | UTC capture time and monotonic clock anchor |
| `app` | App version/build, distribution, provider, membership |
| `device` | Model, manufacturer, Android release/SDK/security patch, hardware, ABIs, cores, RAM, screen pixels/density, form factor |
| `deviceRuntime` | Battery, thermal state, network type, signal, Wi-Fi frequency/band |
| `stream` | Game/state/error, provider and session endpoints, settings, storage region, latest media/input-independent runtime statistics |
| `inputSettings` | Mouse lock and complete touch settings, including control positions |
| `codecs` | Decoder/encoder names, availability, hardware flags, profiles and limits |
| `cpuBuckets` | Ten-second groups retaining sample counts, average and peak CPU usage |
| `input.state`, `input.events` | Retained input counters/status and recent transitions |
| `events` | Timestamped launch, recovery and stream event timeline |
| `api` | Method, URL, status, timing, request/response data and repetition counts |
| `sessionScore` | Active or completed score, metric contributions, downgrades and app recommendations |
| `failureAssessment` | Versioned observations, possible explanations and JSON Pointer evidence links |
| `processExitHistory` | Up to three prior main-process OS exit records, with reason and memory data when available |

Fields may be absent or null when unavailable. CPU process percentage can exceed 100%; device
capacity percentage divides it by the logical core count. Non-finite runtime measurements are
treated as unavailable and omitted, never emitted as invalid JSON numbers.

Times use `YYYY-MM-DDTHH:mm:ss.SSSZ`. Input/CPU wall times are reconstructed from the export's
wall/monotonic clock anchor. Use `uptimeMs` for exact ordering if the device clock changed.
Counted input entries distinguish the detail sample time from `lastSeenUptimeMs`; counts include
events whose detail strings were throttled.

## API detail and retention

- Keep each API call individually, with its timestamp, method, endpoint, status, elapsed time,
  request/response bodies, decoded query parameters and request/response headers. Successful
  polling responses are not collapsed, so status and payload changes remain visible.
- Full JSON arrays, null fields, provider metadata and error details stay intact. The response
  formatter only replaces promotional `description`, `shortDescription`, `longDescription` and
  `synopsis` strings longer than 256 characters inside identifiable game objects with an explicit
  omitted-text marker. Error descriptions and request payloads are preserved.
- Payload parsing, text trimming and redaction run lazily on export and are cached. Compact JSON
  removes formatting whitespace without changing useful field structure.
- Credentials, account/device/session identifiers and IP addresses remain redacted. Query
  parameters are in `requestQuery` (each name maps to an array, preserving duplicates); GraphQL
  variables/extensions are decoded as JSON. Header names map to arrays in `requestHeaders` and
  `responseHeaders`. `url` retains the endpoint without query data or URL user credentials.
- Capture remains bounded: 256 request records and 2,097,152 captured characters across their
  payloads/headers/queries, prioritizing session traffic and errors over routine catalogue traffic.
  Each body has a 262,144-character ceiling; larger bodies carry `truncated`, `originalChars`,
  `head`, and `tail`. `retention.api` reports limits, captured size and evicted record counts.
- Known, replayable request bodies up to 262,144 bytes are captured. Unknown-length, oversized,
  one-shot or duplex bodies have explicit omission markers to avoid consuming the actual request.
- `stream.session` preserves the allocated session, endpoints, routing, negotiated profile,
  monitors and requested/finalized features. `stream.samples` retains the last 600 original runtime
  samples; `retention.stream` exposes sample totals and eviction counts. These survive stopping
  the stream and reset for a new launch. `cpuSamples` keeps the original CPU measurements while
  the readable section uses ten-second averages/peaks.

## Logging cost

Input counters update every event, while repeated detail strings are built at most once per
second per key. Send failure/recovery transitions bypass that interval. Logcat mirroring of
routine input/debug events is disabled in release builds. Repeated haptics advertisements are
logged only when state changes or an explicit advertisement is requested.

CPU sampling and stream health checks retain their existing cadence. Only the readable CPU summary
is aggregated; original samples remain in the parser. Periodic compressed history writes run every 30 seconds during streaming and
every five minutes while idle, with an immediate snapshot on active/idle transitions. A hard
process death can therefore lose the most recent periodic interval.

## JSON Schema and preview

- [`diagnostics-parser.schema.json`](diagnostics-parser.schema.json) is the complete JSON Schema
  (draft 2020-12), including device, network, codec, stream/touch settings and report definitions.
- [`diagnostics-preview.txt`](diagnostics-preview.txt) is a synthetic formatting example generated
  through the production score, assessment and sanitization functions. It is not a device capture.
- Ignore unknown fields for forward compatibility. `schemaVersion` describes the envelope;
  `failureAssessment.rulesVersion` and `sessionScore.algorithmVersion` identify interpretation rules.
- API `request` and `response` can be JSON objects, arrays, strings, or bounded preview objects.
  They deliberately have open schemas because upstream provider responses vary.

`DiagnosticSchemaTest` compares the schema with Kotlin serialization descriptors. To regenerate
it and the preview after an intentional contract change (JVM only):

```sh
OPENNOW_UPDATE_DIAGNOSTIC_SCHEMA=1 ANDROID_HOME="$HOME/Library/Android/sdk" \
  android/gradlew -p android :app:testDebugUnitTest \
  --tests '*DiagnosticSchemaTest' --tests '*DiagnosticAssessmentTest' --rerun-tasks
```

## Failure assessment

`failureAssessment.findings[]` contains:

| Field | Meaning |
| --- | --- |
| `reasonCode` | Stable identifier such as `provider_storage_unavailable`, `dns_resolution_failed`, `api_access_rejected`, `api_timeout`, `provider_http_failure`, `provider_application_failure`, `api_call_failed`, or `stream_error_reported` |
| `category` | `api`, `stream`, `session_quality`, or `previous_process` |
| `confidence` | `observed_failure` for a recorded error, `metric_based_hint` for report advice, `os_recorded_exit` for historical Android exit evidence; these do not establish current-session crash causality |
| `possibleReason` | Human-readable explanation or recommendation title (full advice is at the evidence path) |
| `evidencePaths` | JSON Pointers within this parser block, e.g. `/api/0` or `/sessionScore/report/recommendations/1` |
| `timestamp`, `httpStatus`, `providerStatus`, `error` | Present when supplied by the evidence; the referenced API entry retains the condensed response |

`crashConclusion` is currently `not_established` for the running process. It identifies possible causes of launch/session trouble without
mistaking an HTTP error, DNS failure or bad quality score for an app crash. Findings cover retained
records from the current app run and can include an earlier session; correlate their timestamps.
The bounded buffer is not a complete request history. An empty finding list does not prove health.

## Session score and recommendations

`sessionScore` is captured during streaming (`phase: active`) or retained after completion
(`phase: completed`), including after the dialog is dismissed or disabled. A new launch clears the
previous report. No measured stream samples means `available: false`, `phase: unavailable`, with
an `unavailableReason` instead of a fabricated score (for example, a rejected launch).

The report dialog is enabled by default, including a one-time migration from older defaults.
The existing setting and "Don't show again" can disable it after this migration. Its half-circle
fills from 0 to 180 degrees according to `report.score / 100`; `gaugeFraction` exports that fraction.
This controls local report display, not automatic bug submission.

`sessionScore.report` contains the exact report metrics, rating, `limitedData` flag, downgrades and
recommendations used by the app. Each recommendation/downgrade has `reasonCode`, `title`, `detail`
and `kind`. `limitedData` means fewer than ten samples; a high score is not proof of a long healthy
session. Stable recommendation codes are `prefer_high_band_wifi`, `weak_wifi_signal`,
`check_wifi_band`, `cellular_variability`, `packet_loss`, `latency_jitter`, `link_capacity`,
`decoder_bottleneck`, and `healthy_connection`.

`components` exposes each metric's 0–100 score and weight: latency 35, packet loss 30, jitter 15,
frame rate 15 and decode 5. Missing metrics are excluded and the remaining weights are normalized.
If none of these scoring metrics is available, the existing algorithm uses 50 as its fallback.
Ratings are Excellent (90–100), Good (75–89), Fair (60–74), and Poor (0–59, displayed as "Needs work").
Recommendations are suggestions; exporting them does not change stream settings.

## Previous process exits

On Android 11/API 30 and later, `processExitHistory` reads Android's retained exit history once
per process, off the UI thread during export. It inspects at most eight package records and retains
up to three matching the main app process. No crash trace file is opened. Older Android versions
return `availability: unsupported_android_version`; a missing service or failed query returns
`service_unavailable` or `query_failed`. `available` with an empty list means no matching record
was retained by Android, not proof that no crash occurred.

Each record includes `timestamp`, `timestampEpochMs`, numeric `reason`, stable `reasonCode`,
`status` (exit code/signal when applicable), optional `description`, and optional `pssKiB`/`rssKiB`
(memory samples, not necessarily the values at the instant of failure). Reasons include
`crash_java`, `crash_native`, `anr`, `low_memory`, `initialization_failure`, and
`excessive_resource_usage`; normal/user-requested exits are retained without being labeled crashes.
Unknown future reasons preserve their numeric value with `reasonCode: unknown`.

Abnormal prior exits also appear as `previous_process_*` assessment findings with evidence paths
such as `/processExitHistory/records/0`. They belong to **previous app processes**, so correlate
the exit timestamp with the previous-run parser block. Their presence does not prove the current
session crashed or that a nearby API response caused the exit.

Persisted previous-run snapshots have a 16,777,216-character safety ceiling. If exceeded, API bodies
are replaced with explicit `persisted_snapshot_limit` markers without changing API record indexes,
and `persistence.truncated` is set. Exceptional oversized non-API content yields an explicit
`incomplete: true` parser block instead of broken JSON. Check `incomplete` before inspecting fields.
