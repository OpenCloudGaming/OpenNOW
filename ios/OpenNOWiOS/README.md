# OpenNOW iOS

Native iPhone/iPad GeForce NOW client with hardware decoding, HDR and 4:4:4 streaming, MetalFX spatial upscaling, controller/touch input, pointer capture, picture in picture and per-game stream profiles. Frame generation is removed.

## Metal rendering

Metal 4 handles supported direct HDR rendering, native color conversion, sharpening and MetalFX. The display layer's residency set is registered with each Metal 4 queue, and frames present directly to its drawables. The block repair was verified on an iPhone in build 149 with HEVC 10-bit 4:4:4 HDR and active MetalFX. See [renderer architecture and validation](METAL4.md).

Build 150 removes the diagnostic presentation-copy experiment, launch overrides, developer HUD and disk/per-frame trace machinery. Normal stream stats and error reporting remain. Existing settings load without the removed developer-HUD key; video and input choices are preserved. Historical experiment notes and pre-cleanup sources are archived locally under `Build/cleanup-before-150`.

## Build on an Apple Silicon Mac

Install full Xcode providing the iOS 27 SDK and its iOS platform. Run these commands from the repository root:

```sh
python3 ios/OpenNOWiOS/BuildScripts/prepare-native-libraries.py
xcodebuild build \
  -project ios/OpenNOWiOS/OpenNOWiOS.xcodeproj \
  -scheme OpenNOWiOS \
  -configuration Debug \
  -destination 'generic/platform=iOS' \
  -derivedDataPath DerivedData \
  CODE_SIGNING_ALLOWED=NO
```

The preparation script downloads the matching static OpenSSL/usrsctp XCFrameworks, verifies the pinned SHA-256, and installs only their framework directories. The native C libraries are unchanged from build 128, so the preparation script reuses that checksum-pinned archive. Generated libraries and build outputs are ignored by Git. WebRTC remains the vendored framework already in the upstream repository. For a source build of the native dependencies, install CMake and run:

```sh
python3 ios/OpenNOWiOS/BuildScripts/build-native-protocol-libs.py --cmake /path/to/cmake
```

The unsigned device app is in `DerivedData/Build/Products/Debug-iphoneos/OpenNOWiOS.app`. To run directly from Xcode, open `ios/OpenNOWiOS/OpenNOWiOS.xcodeproj` and select your signing team. The native dependency archives contain arm64 device and arm64 simulator slices.

## Native receiver

The native NVST receiver requires an advertised RTSPS endpoint and compatible hardware decoding. HEVC 10-bit 4:4:4 is validated against the actual bitstream and received surface, preserving chroma/depth or reporting an unsupported format. Ordinary WebRTC and compatible rendering remain available on their supported paths.

Native microphone carriage requires a host bundle offering it and the microphone option enabled. Protocol provenance, pinned dependencies and licenses are in [NVST](OpenNOWiOS/NVST/README.md).

## Checks

Run the simulator test scheme `OpenNOWiOS`, plus the GPU validation commands in [METAL4.md](METAL4.md). Simulator and Mac checks cannot establish iPhone/iPad performance. Build 149 verified the visible iPhone block repair; consistent 120 FPS, the separate decoder recovery event and physical iPad behavior remain separate validation targets.
