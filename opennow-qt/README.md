# OpenNOW Qt shell

The Qt Quick/QML replacement for the Electron desktop shell. It targets Qt 6.8
or newer and uses SDL3 for controller input. A bundled Rust process owns settings
and is the start of the shell-neutral application core. See
`docs/qt-migration.md` for the full parity and removal checklist.

## Build

```sh
cmake -S opennow-qt -B build/opennow-qt -DCMAKE_BUILD_TYPE=Debug
cmake --build build/opennow-qt
ctest --test-dir build/opennow-qt --output-on-failure
```

Run with the offscreen Qt platform plugin for a startup smoke test:

```sh
QT_QPA_PLATFORM=offscreen ./build/opennow-qt/opennow-qt \
  --smoke-test --allow-multiple-instances --route home
```

Useful development switches are `--route <name>`, `--overlay <name>`,
`--reduced-motion`, `--core <path>` and `--screenshot <png-path>`. The test suite
opens every route and overlay with QML warnings treated as failures.

The representative-hardware performance workload drives production route and popup motion,
checks focus after every transition, and records refresh-relative frame budgets atomically:

```sh
./build/opennow-qt/opennow-qt --allow-multiple-instances \
  --performance-report "$PWD/opennow-1080p.json" \
  --performance-width 1920 --performance-height 1080 \
  --performance-cycles 3 --performance-label linux-intel-uhd \
  --performance-require-hardware
```

Requested dimensions are physical pixels, so the workload remains comparable on HiDPI screens.
Offscreen/software runs validate the harness only and are rejected when
`--performance-require-hardware` is present. Hardware acceptance also forbids the test-only
`--performance-refresh-hz` override and records both the effective and display-reported rates.
See `docs/qt-acceptance.md` for the release matrix.

The `theme-store` route implements the current Paper V3 collection with
controller filters, temporary preview, persistent install/apply and access to
the local plain-file theme directory.

The versioned Rust core owns settings, NVIDIA device login and token refresh,
OS-protected accounts, PINs, catalogs, subscriptions, regions and latency tests,
account connections, persistent storage, CloudMatch lifecycle/recovery/ads,
NVST session orchestration, diagnostics, media listing, Discord, telemetry,
feedback and update discovery. The protocol-v5 native streamer is linked into
the Qt executable as an in-process Rust library. It owns NVST RTSPS negotiation,
Mjolnir video, the ICE/DTLS/SCTP control bundle, decode, audio and native input.
Qt/QML owns stream status, stats, menus, recovery, failure and fullscreen
chrome. The shell sends one complete CloudMatch session context and never
proxies RTSPS, ICE, SRTP or encoded media. Decoders publish native GPU frames
through the C FFI; `StreamVideoItem` imports and samples them on Qt's QRhi
render command stream without a child streamer process or native presenter
window. CPack installs the Qt executable, `opennow-core` and the runtime library;
there is no separate streamer application. CI produces the platform packages
from that layout.
The screenshot shortcut captures the exact stream region, and F12 records the
negotiated H.264/H.265/AV1 source stream plus Opus audio atomically into Matroska
before generating a media thumbnail. Microphone capture is not part of the
native NVST runtime. Live multi-OS streaming, GPU interop and
hardware validation plus production signing/notarization remain removal gates,
so Electron is still retained as the shipping fallback.

The current presenter is a Qt scene-graph item. Platform decoders retain native
textures, record any required conversion and synchronization into the active
QRhi command buffer, and expose only opaque frame tokens across the FFI. QML
overlays therefore compose normally above the stream. No CPU frame callback,
standalone SDL presenter, child HWND or paired top-level video window participates
in the Qt path.
