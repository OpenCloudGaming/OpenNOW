# Metal 4 streaming renderer

## Rendering

Hardware-decoded frames enter a bounded latest-frame mailbox. A display clock submits at most three GPU frames; stale pending frames are replaced without buffering extra latency. Direct PQ HDR avoids the intermediate linear image when no effects are requested. Native SDR/PQ/HLG conversion, linear-light sharpening and MetalFX spatial upscaling share a Metal 4 command buffer with explicit pass dependencies. Unsupported inputs/devices or GPU failures use the compatible renderer.

Metal 4 renders into private textures. `NativeStreamMetal4Presentation` owns three output slots and the compatible queue's final copy into the MTKView drawable. The copy waits for the producer's shared-event signal and uses command-buffer presentation, matching the clean compatible renderer. Output slots remain occupied until the copy completes, including during resize or format changes. The final copy preserves encoded pixels, HDR metadata and fit/fill geometry; it performs no additional conversion. Metal 4 residency covers decoder planes, uniforms, intermediate/output textures and lookup tables. Playback does not write or present layer IOSurfaces through the Metal 4 queue.

Three slots bound resource reuse. GPU feedback retains decoded IOSurfaces and texture wrappers until work completes. Both native input paths use the same fragment-read alias visibility barrier for recycled decoder surfaces; retention alone does not provide cache coherence. Shared GPU events order switches between native/effects/compatible queues and recover skipped signals after GPU failure. No CPU readback, diagnostic launch override, frame interpolation or per-frame disk tracing is part of playback. The copy adds three private display-sized textures and a GPU blit; physical-device acceptance still requires clean panning at the requested cadence.

PQ conversion preserves BT.2020 HDR. HLG and SDR transfer lookup tables are prepared off the display thread. HDR intermediates retain half-float highlights; both HDR transfers present through a PQ BT.2020 10-bit EDR drawable. Framebuffer geometry preserves fit/fill and chroma detail. MetalFX presets select source resolutions without changing HDR, codec or requested FPS. When MetalFX is enabled, runtime upscaling uses the same supported enlargement range as preset selection (more than 2% on both axes, up to 4×). The 2560×1080 Quality stream therefore upscales to 2868×1210 with Fit, or 2868×1320 with Stretch on the reported phone. With MetalFX off or an ineligible size, PQ HDR stays on the direct single-pass path when sharpening is off.

## User controls and stats

MetalFX and sharpening controls remain in Settings and the stream Picture panel. Live MetalFX choices persist for future sessions. Normal stats show source/decoded/displayed FPS, loss, latency, codec and received color mode. Displayed FPS counts actual drawable callbacks through a small thread-safe tracker. Standard error reporting remains; Apple's developer HUD and the temporary timing/cadence trace system are removed.

## Validation

On Apple Silicon with the current Xcode SDK:

```sh
python3 ios/OpenNOWiOS/BuildScripts/validate-metal4-hdr-macos.py
python3 ios/OpenNOWiOS/BuildScripts/validate-metal4-effects-macos.py
python3 ios/OpenNOWiOS/BuildScripts/validate-metal4-presentation-macos.py
python3 ios/OpenNOWiOS/BuildScripts/validate-video-effects-macos.py
```

The GPU harnesses check actual pixels for chroma/range/transfer, HDR highlights, orientation, sharpening, MetalFX, recycled surfaces and cross-queue switching. Presentation validation reads back actual MTKView drawables across size changes, compares changing frames against compatible rendering, and verifies that producer feedback cannot release textures still owned by the copy consumer. Effects validation also presents HDR/MetalFX frames through the same presentation owner. Simulator tests cover settings migration, displayed FPS, input, navigation and lifecycle behavior. These do not establish sustained 120 FPS or physical iPad support. Historical experiments are archived locally in `Build/cleanup-before-150`; build-150 cleanup details are in [METAL4-CLEANUP-150.md](METAL4-CLEANUP-150.md).

## References

- [Apple's Metal 4 presentation and residency sample](https://developer.apple.com/documentation/metal/drawing-a-triangle-with-metal-4)
- [Metal 4 synchronization](https://developer.apple.com/documentation/metal/resource-synchronization)
- [MetalFX spatial scaler](https://developer.apple.com/documentation/metalfx/mtl4fxspatialscaler)

## Renderer selection

Metal 4 Rendering is a saved opt-in toggle, defaulting off for new and existing installations. It appears in Settings → Stream and the live Picture panel. Pipeline preparation starts only after opt-in; all Metal 4 submission paths are gated by the current choice. In-flight frames complete normally when switched off. HDR and MetalFX settings remain independent, and unsupported devices use compatible rendering.
