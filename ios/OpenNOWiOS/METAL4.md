# Metal 4 streaming renderer

## Rendering

Hardware-decoded frames enter a bounded latest-frame mailbox. A display clock submits at most two GPU frames; stale pending frames are replaced without buffering extra latency. Direct PQ HDR avoids the intermediate linear image when no effects are requested. Native SDR/PQ/HLG conversion, linear-light sharpening and MetalFX spatial upscaling share a Metal 4 command buffer with explicit pass dependencies. Unsupported inputs/devices or GPU failures use the compatible renderer.

Metal 4 queues register the display layer's unmodified residency set through `NativeStreamMetalDrawableResidency`. Per-frame residency still covers decoder planes, uniforms, intermediate textures and lookup tables. The layer owns presentation-resource tracking across drawable reuse and format/size changes. Drawables use waitForDrawable → commit → signalDrawable → present. The build-149 iPhone test verified this direct path without motion blocks while MetalFX and 10-bit 4:4:4 HDR were active.

Two slots bound resource reuse. GPU feedback retains decoded IOSurfaces and texture wrappers until work completes. Shared GPU events order switches between native/effects/compatible queues and recover skipped signals after GPU failure. No CPU readback, presentation-copy bridge, diagnostic launch override, frame interpolation or per-frame disk tracing is part of playback.

PQ conversion preserves BT.2020 HDR. HLG and SDR transfer lookup tables are prepared off the display thread. HDR intermediates retain half-float highlights; both HDR transfers present through a PQ BT.2020 10-bit EDR drawable. Framebuffer geometry preserves fit/fill and chroma detail. MetalFX presets select eligible source resolutions without changing HDR, codec or requested FPS.

## User controls and stats

MetalFX and sharpening controls remain in Settings and the stream Picture panel. Live MetalFX choices persist for future sessions. Normal stats show source/decoded/displayed FPS, loss, latency, codec and received color mode. Displayed FPS counts actual drawable callbacks through a small thread-safe tracker. Standard error reporting remains; Apple's developer HUD and the temporary timing/cadence trace system are removed.

## Validation

On Apple Silicon with the current Xcode SDK:

```sh
python3 ios/OpenNOWiOS/BuildScripts/validate-metal4-hdr-macos.py
python3 ios/OpenNOWiOS/BuildScripts/validate-metal4-effects-macos.py
python3 ios/OpenNOWiOS/BuildScripts/validate-video-effects-macos.py
```

The GPU harnesses check actual pixels for chroma/range/transfer, HDR highlights, orientation, sharpening, MetalFX, recycled surfaces and cross-queue switching. Effects validation also presents actual layer-resident HDR/MetalFX drawables. Simulator tests cover settings migration, displayed FPS, input, navigation and lifecycle behavior. These do not establish sustained 120 FPS or physical iPad support. Historical experiments are archived locally in `Build/cleanup-before-150`; build-150 cleanup details are in [METAL4-CLEANUP-150.md](METAL4-CLEANUP-150.md).

## References

- [Apple's Metal 4 presentation and residency sample](https://developer.apple.com/documentation/metal/drawing-a-triangle-with-metal-4)
- [Metal 4 synchronization](https://developer.apple.com/documentation/metal/resource-synchronization)
- [MetalFX spatial scaler](https://developer.apple.com/documentation/metalfx/mtl4fxspatialscaler)
