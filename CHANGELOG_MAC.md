# Changelog — Mac Mini Optimization

## [mac-mini-optimization-v1] — 2026-06-30

---

## Summary

This release targets **lag reduction and stream quality improvement** on Mac mini hardware running OpenNOW. Prior to these changes, the Electron/Chromium layer received zero macOS-specific GPU acceleration flags, meaning VideoToolbox hardware decoders sat idle while the CPU decoded every video frame in software. WebRTC codec negotiation was also platform-agnostic, causing Apple Silicon machines to miss native AV1 decode (available on M3+) and Intel Macs to potentially attempt software AV1 decode.

The optimization approach covers four layers:

1. **GPU/decoder activation** — pass Chromium feature flags and switches that unlock VideoToolbox, Metal, and IOSurface paths for hardware-accelerated video decode on both Apple Silicon and Intel Mac.
2. **Codec negotiation** — reorder SDP codec candidates so WebRTC selects the codec with the best hardware decode path for the running chip, without removing fallback codecs.
3. **Stream diagnostics** — surface latency grades and jitter severity in the HUD so users can see stream health at a glance.
4. **Quality presets** — expose Mac-specific stream quality presets that encode codec hints and latency mode preferences tuned for Mac mini thermal and bandwidth profiles.

Together these changes eliminate the primary source of unnecessary lag: software video decode on hardware that is fully capable of decoding in dedicated silicon.

---

## Changed Files

### 1. `src/main/videoAcceleration.ts` — Full macOS Hardware Acceleration

**Status:** New macOS branch added (was entirely absent before this change)

The file previously contained Windows and Linux acceleration branches only. A complete macOS branch was added that forks on CPU architecture to apply the correct Chromium feature flags and GPU switches.

#### Apple Silicon (`arm64`)

Chromium feature flags enabled:

| Flag | Purpose |
|---|---|
| `VideoToolboxVideoDecoder` | Route all supported codecs through macOS VideoToolbox hardware decoder |
| `VideoToolboxVp9Decoding` | Enable VP9 decode via VideoToolbox |
| `VideoToolboxHEVCDecoding` | Enable HEVC/H.265 decode via VideoToolbox |
| `VideoToolboxVp9DecodingOnArm` | Arm-specific VP9 VideoToolbox path |
| `UseMetalVideoDecoder` | Use Metal-backed video decoder pipeline |
| `MetalANGLE` | Use Metal as the ANGLE graphics backend |
| `UseEGLImageForMacVideoToolbox` | Bridge VideoToolbox frames to EGL without CPU copy |
| `Metal` | Enable Metal rendering backend in Chromium |
| `IOSurfaceMemory` | Use IOSurface for zero-copy GPU memory sharing |
| `CanvasOopRasterization` | Offload canvas rasterization to GPU process |

Chromium switch added:
- `--use-gl=metal` — forces the Metal GPU backend instead of the default ANGLE/OpenGL path

#### Intel Mac (`x64`)

Chromium feature flags enabled:

| Flag | Purpose |
|---|---|
| `VideoToolboxVideoDecoder` | Route supported codecs through VideoToolbox hardware decoder |
| `VideoToolboxVp9Decoding` | Enable VP9 decode via VideoToolbox |

Chromium switch added:
- `--use-gl=angle` — uses ANGLE (OpenGL ES over OpenGL) which is stable on Intel Mac

#### Both Architectures

- `AcceleratedMJpegDecode` feature flag enabled — hardware MJPEG decode for webcam/media sources
- `GpuRasterization` feature flag enabled — offloads 2D rasterization to GPU

#### New Helper

```typescript
function getMacPlatformLabel(): string
```

Returns a human-readable label (`'Apple Silicon'` or `'Intel Mac'`) derived from `process.arch`, used for logging and diagnostic labeling.

---

### 2. `src/main/signaling/sdp.ts` — Smart WebRTC Codec Reordering

**Status:** New export function added

WebRTC codec negotiation proceeds by offering an ordered list of codecs in the SDP. The first mutually supported codec is selected. Previously, codec order was left at the browser default with no awareness of what the local hardware can decode in silicon.

#### New Function

```typescript
function reorderCodecsForPlatform(
  sdp: string,
  platform: NodeJS.Platform,
  arch: string
): string
```

Parses the SDP offer/answer, identifies video codec `m=` sections, and reorders the payload type lines to put the preferred codec family first. Codecs are **never removed** — only reordered — so fallback paths remain intact.

#### Codec Priority Tables

**Apple Silicon (`darwin` + `arm64`)**

| Priority | Codec | Reason |
|---|---|---|
| 1 | AV1 | M3 and later have native AV1 VideoToolbox decode; best compression |
| 2 | H264 | Universal VideoToolbox support across all M-series |
| 3 | H265 | VideoToolbox HEVC, slightly higher CPU overhead than H264 |

**Intel Mac (`darwin` + `x64`)**

| Priority | Codec | Reason |
|---|---|---|
| 1 | H264 | Broadwell+ Intel Quick Sync handles H264 in hardware |
| 2 | H265 | Available on newer Intel via VideoToolbox, lower priority |
| 3 | AV1 | Software-only decode on Intel; avoid for latency-sensitive streaming |

#### New Constant

```typescript
const CODEC_FAMILY_MAP: Record<string, string>
```

Maps RTP codec names from SDP `rtpmap` lines (e.g., `"av01"`, `"avc1"`, `"hev1"`) to canonical family names (`"AV1"`, `"H264"`, `"H265"`), handling the variety of codec name strings that appear in real SDP payloads.

---

### 3. `src/renderer/src/components/StatsOverlay.tsx` — Enhanced Stream Diagnostics HUD

**Status:** Existing component updated

The stats overlay previously displayed raw RTT milliseconds with no context for whether the value was good or bad. Two improvements were made.

#### RTT / Latency Grade Label

The RTT display now appends a grade label derived from `formatLatencyGrade()`:

```
Before: 18ms
After:  18ms · Excellent
```

Grade is color-coded and updates live as RTT fluctuates during a session.

#### Jitter Pill

A new jitter indicator was added as a pill badge beside the packet-loss section. It displays the current jitter value in milliseconds and applies a background color based on severity:

| Range | Color | Meaning |
|---|---|---|
| < 5 ms | Green (`--color-success`) | Stable connection, no perceptible jitter |
| 5 – 15 ms | Yellow (`--color-warning`) | Mild jitter, may cause occasional micro-stutters |
| > 15 ms | Red (`--color-error`) | High jitter, expect visible frame irregularity |

---

### 4. `src/renderer/src/utils/streamDiagnosticsFormat.ts` — New Formatting Utilities

**Status:** New file

Consolidates display-layer formatting logic for stream diagnostic values, keeping formatting concerns out of components.

#### `formatLatencyGrade(rttMs: number): string`

Formats an RTT value into a combined `"<value>ms · <grade>"` string for display in the HUD.

```typescript
formatLatencyGrade(18)   // → "18ms · Excellent"
formatLatencyGrade(45)   // → "45ms · Good"
formatLatencyGrade(65)   // → "65ms · Fair"
formatLatencyGrade(120)  // → "120ms · Poor"
```

Grade thresholds:

| Grade | RTT Range |
|---|---|
| Excellent | < 20 ms |
| Good | 20 – 49 ms |
| Fair | 50 – 79 ms |
| Poor | ≥ 80 ms |

#### `getJitterColor(jitterMs: number): string`

Returns a CSS custom property reference string for use in inline styles or className logic.

```typescript
getJitterColor(3)   // → "var(--color-success)"
getJitterColor(10)  // → "var(--color-warning)"
getJitterColor(20)  // → "var(--color-error)"
```

---

### 5. `src/renderer/src/utils/streamHealthSummary.ts` — Latency Grade Types

**Status:** Existing file extended

#### New Type

```typescript
type LatencyGrade = 'Excellent' | 'Good' | 'Fair' | 'Poor';
```

Provides a strict union type for latency grade values, preventing magic strings from spreading through the codebase.

#### New Function

```typescript
function getLatencyGrade(rttMs: number): LatencyGrade
```

Returns the appropriate `LatencyGrade` for a given RTT in milliseconds using the same threshold table as `formatLatencyGrade()`. Separating the grade logic from the formatting logic allows consumers to branch on grade without parsing a display string.

---

### 6. `src/renderer/src/utils/streamQualityPresets.ts` — Mac-Optimized Stream Presets

**Status:** New file

Exposes four stream quality presets tuned specifically for Mac mini hardware profiles. Presets encode both a codec preference hint and a latency mode so that the streaming stack can apply coordinated settings rather than independent knobs.

#### New Type

```typescript
type MacStreamQualityPresetId =
  | 'mac-performance'
  | 'mac-balanced'
  | 'mac-quality'
  | 'mac-battery';
```

#### New Interface

```typescript
interface MacStreamPreset {
  id: MacStreamQualityPresetId;
  label: string;
  description: string;
  codecHint: 'AV1' | 'H264' | 'H265' | 'auto';
  latencyMode: 'ultra-low' | 'low' | 'balanced' | 'quality';
  bitrateKbps: number;
}
```

`codecHint` signals to the SDP reordering layer which codec should be prioritized for this preset. `latencyMode` maps to upstream streaming session parameters.

#### `getMacStreamPresets(): MacStreamPreset[]`

Returns the full array of four presets:

| Preset ID | Label | Codec Hint | Latency Mode | Bitrate |
|---|---|---|---|---|
| `mac-performance` | Performance | H264 | ultra-low | 10 000 kbps |
| `mac-balanced` | Balanced | auto | low | 20 000 kbps |
| `mac-quality` | Quality | H265 | balanced | 35 000 kbps |
| `mac-battery` | Battery Saver | H264 | low | 8 000 kbps |

#### `pickMacStreamPreset(presetId, arch): MacStreamPreset`

Resolves the final preset with Apple Silicon AV1 override logic applied:

- If `arch === 'arm64'` and the selected preset's `codecHint` is `'auto'`, the codec hint is overridden to `'AV1'` to leverage native AV1 VideoToolbox decode on M3+ chips.
- On all other architectures the preset is returned unmodified.

This ensures the Balanced preset (which ships with `auto` codec hint) automatically benefits from AV1 on capable Apple Silicon without requiring the user to manually select a codec.

---

## Impact Summary

| Area | Before | After |
|---|---|---|
| macOS GPU decode | Software CPU decode only | VideoToolbox hardware decode on all supported codecs |
| Metal rendering (Apple Silicon) | ANGLE/OpenGL | Native Metal backend |
| WebRTC codec selection | Browser default order | Hardware-aware order per chip |
| AV1 on M3+ | Never selected first | Prioritized as first-choice codec |
| AV1 on Intel Mac | May be selected (software decode) | Deprioritized to position 3 |
| Latency visibility | Raw RTT ms | RTT + grade label (Excellent/Good/Fair/Poor) |
| Jitter visibility | Not shown | Color-coded jitter pill |
| Stream presets | Generic presets only | 4 Mac-specific presets with codec + latency tuning |

---

## Notes

- All changes are additive or branch on `process.platform === 'darwin'`; Windows and Linux behavior is unchanged.
- Codec reordering is non-destructive: no codecs are removed from the SDP, ensuring compatibility with GFN servers that may not support preferred codecs.
- The `getMacPlatformLabel()` helper is exported for use in settings UI and telemetry.
- Quality presets are data only in this release; wiring to the settings UI is a follow-up task.
- Latency grade thresholds (`<20 / <50 / <80 / ≥80 ms`) reflect typical GeForce NOW datacenter RTTs for North America and Europe; they may be tuned in a future release based on observed session data.
