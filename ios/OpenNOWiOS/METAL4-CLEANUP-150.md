# OpenNOW iOS 150 — Metal 4 cleanup

## Working baseline
Build 149 was verified on the physical iPhone without blocks, using direct Metal 4 native PQ presentation with CAMetalLayer residency, active MetalFX and HEVC 10-bit 4:4:4 HDR. That rendering behavior is preserved. Full 120-FPS pacing, a separate decoder recovery event and physical iPad validation remain separate work.

## Removed
- Retired presentation-copy bridge, its texture pool, completion join and bridge-only validation.
- Compatible-renderer and presentation-bridge diagnostic launch overrides.
- Apple developer HUD controls, view plumbing and persisted setting. Old saved keys are ignored and omitted on save.
- Rotating video-performance disk logger and transport/input/display trace callers.
- Per-frame arrival/RTP cadence, decode-call/delivery, renderer-age, GPU/CPU/clock timing traces and diagnostic receiver-delay collection.
- Startup CPU luma sampling and first-frame/format debug dumps.
- Trace-only tests. Old experiment documents/build histories are archived under Build/cleanup-before-150; current README/METAL4 documentation describes the actual renderer.

Approximately 658 net Swift source lines were removed relative to the verified build-149 snapshot. Shader strings and native libraries are unchanged.

## Retained and consolidated
The layer's residency set remains registered on both Metal 4 queues through one shared NativeStreamMetalDrawableResidency helper. Direct HDR, conversion, sharpening, MetalFX, bounded latest-frame mailbox, two-slot GPU admission, decoder/surface retention, GPU events and failure recovery remain. Unsupported formats/devices and GPU failure still have a normal compatible renderer. This is operational fallback, not the retired experimental display-copy bridge.

User-facing source/decoded/displayed FPS, loss, latency, codec/color status and error/reporting features remain. A small lock-protected presentation tracker retains only the actual displayed-FPS counter, with no disk tracing. Existing settings/video/input choices and live MetalFX persistence remain compatible. Controller tap/navigation fixes and frame-generation removal remain.

## Validation
Optimized unsigned device Release build passed. Final simulator suite: 331 passed, zero failures, one hardware-only skip. Three removed tests covered retired trace helpers. The retained concurrency test checks displayed-FPS snapshots and reset; legacy settings migration now covers the removed developer-HUD key alongside frame-generation keys. GPU/shader validation: 165 checks passed, including direct layer-resident HDR/MetalFX drawable presentation, format/chroma/HDR/orientation/sharpen/upscale comparisons and recycled source surfaces. Both shader strings match the verified build-149 snapshot.

Initial compile cleanup corrected an accidental texture-usage enum rename and an empty statement left after removing a logger call. Final builds/checks passed. The final device artifact contains none of the retired bridge, performance-file logger, trace helper, developer-HUD control or interpolation symbols.

Physical build-150 regression remains pending. End the current stream before installing. With the same HEVC 10-bit 4:4:4 HDR and MetalFX settings, repeat the pan and verify blocks remain absent, input and ordinary stats work. Detailed removed traces are no longer available; normal stats/error reports remain. No network/account settings changed, and no phone stream was interrupted by this cleanup.

## Distribution
Unsigned 1.1.150/build 150 IPA for KravaSigner signing. Both feeds updated; build 149 remains available as the verified fallback. Cumulative source patch applies against upstream 95c0f58d42eeed176edd677f604b193c85169d9e. Native archive is unchanged; private traces/preferences and the local backup archive are excluded.
