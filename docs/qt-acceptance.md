# OpenNOW Qt acceptance runbook

This runbook turns the remaining migration gates into reproducible evidence. A development-host
smoke test is not release acceptance. Keep Electron until every required row has been executed on
the named hardware, the artifacts have been reviewed, and the staged rollout has completed.

## Required live matrix

| Platform | Architecture | Window system | Required package |
| --- | --- | --- | --- |
| Windows 11 | x64 | Win32/DWM | Signed installer and signed portable ZIP |
| Windows 11 | ARM64 | Win32/DWM | Signed installer and signed portable ZIP |
| macOS | Apple Silicon | AppKit/Metal | Developer ID signed and notarized DMG/ZIP |
| macOS | Intel | AppKit/Metal | Developer ID signed and notarized DMG/ZIP |
| Linux | x64 | X11 | AppImage and DEB |
| Linux | x64 | Wayland | AppImage and DEB |
| Linux | ARM64 | Native desktop session | AppImage and DEB |

Use an authorized test account with no production secrets in reports. Sign in interactively; do
not put NVIDIA credentials, refresh tokens, signing keys, or notarization passwords in command
arguments, logs, issue trackers, or acceptance artifacts.

The streamer is loaded by the Qt executable as an in-process Rust library. Platform decoders publish
native GPU frames through a bounded FFI mailbox; `StreamVideoItem` records conversion and
synchronization into the active QRhi command buffer and samples the imported texture in the Qt scene
graph. There is no child streamer process, child HWND or paired native video window. Acceptance must
still prove each platform's native texture import and synchronization on real hardware; the design
alone is not performance or zero-copy evidence.

## Performance evidence

Run the release package on the agreed baseline iGPU with its native display backend. Close frame
capture tools, screen recorders, and unrelated GPU workloads. Run both physical-pixel workloads:

```sh
opennow-qt --allow-multiple-instances \
  --performance-report /absolute/path/opennow-1080p.json \
  --performance-width 1920 --performance-height 1080 \
  --performance-cycles 3 --performance-label <machine-id> \
  --performance-require-hardware

opennow-qt --allow-multiple-instances \
  --performance-report /absolute/path/opennow-4k.json \
  --performance-width 3840 --performance-height 2160 \
  --performance-cycles 3 --performance-label <machine-id> \
  --performance-require-hardware
```

The JSON report records OS, CPU architecture, Qt platform, graphics API, screen, device-pixel
ratio, refresh rate, exact physical dimensions, every transition, focus validity, first-frame
latency, frame intervals, missed-frame ratio, budgets, and the final pass/fail result. The gate
requires `pass: true` for both reports. The hardware flag rejects offscreen/minimal platforms,
software/null renderers, missing screens, and workloads that do not receive the requested physical
dimensions. It also rejects the test-only refresh-rate override, so release evidence always uses
the display-reported rate. This measures the Qt shell workload; it does not prove stream-window
native decoder throughput or GPU texture-import behavior.

## Authorized stream evidence

For every matrix row, launch a real account-owned title and keep the session active for at least
ten minutes. Exercise the following without restarting the app:

1. Complete device login, account switching, subscription and region refresh.
2. Create a session, pass queue/ads if present, reach native NVST first-frame playback, and confirm
   the live evidence reports `stream.transport: "nvst"`.
3. Open and close every guide and stats page while video is live. Confirm QML composes above the
   scene-graph video item without suspending playback, stale frame tokens or input leakage.
4. Exercise keyboard, relative mouse, and every connected controller. Validate neutral controller
   state after overlay entry, reconnect, pause, and resume.
5. Test window resize, fullscreen, display migration, the display's highest supported refresh
   rate, and VRR/HDR only where the machine advertises them.
6. Load a profile that previously selected WebRTC or another legacy transport and confirm settings,
   session creation, streamer status and exported evidence all resolve it to NVST. If a persisted
   microphone mode is armed, confirm settings migration disables it without changing transport;
   microphone capture is not part of the native runtime.
7. Capture a screenshot, start and stop a source-stream Matroska recording, play the resulting
   media, verify the generated thumbnail, and reveal both files through the Media screen.
8. Rebind and exercise all seven active stream shortcuts. Confirm stats and fullscreen reach Qt
   exactly once, pointer lock remains native, screenshot, recording and stop reach the shell exactly
   once, and anti-AFK produces an F13 pulse after four minutes without leaking the key into the game.
9. Enable the anti-AFK indicator/reminder and session clock, then confirm the post-session report
   reflects NVST transport, elapsed time, backend, first-frame latency, recovery/error counters and
   diagnostics navigation.
10. Exercise favorites, entitlement-filtered aspect ratio/resolution/FPS choices, keyboard layout,
    game language, console-friendly launch and in-game-settings persistence on a title that advertises
    the corresponding NVIDIA feature.
11. Force one recoverable network interruption and one graphics-device or native-runtime failure.
   Confirm bounded reconnect/reinitialization behavior, no stuck input, and a usable error if
   recovery is exhausted.
12. Export both the redacted diagnostic report and **live evidence** from the Diagnostics screen
   after the run. The live export is direct machine-readable JSON and must report
   `observedPass: true`; it includes hashed screenshot/recording/thumbnail metadata and bounded
   NVST transport, first-frame, input ownership, guide, recovery and error checks without
   exposing a local path, account, token, session identifier or endpoint.

Retain the two performance JSON files, redacted diagnostic export, screenshot, recording, package
hash, and a short capture showing guide/controller ownership. Hash artifacts before upload and
store them under the release candidate and matrix-row identifier. A checklist without these
artifacts is not proof of the gate.

Copy [`qt-acceptance-attestations.example.json`](qt-acceptance-attestations.example.json) for the
matrix row. Use exactly one of `windows-x64`, `windows-arm64`, `macos-apple-silicon`, `macos-intel`,
`linux-x64-x11`, `linux-x64-wayland` or `linux-arm64-native`. Leave a check `false` until it was
actually exercised. `hdr` and `vrr` accept `passed` or an evidence-backed `not-supported`.
`microphoneUpstream`, if present in an older attestation template, is compatibility metadata and is
not a required NVST gate. Declare every required release artifact,
its byte size and SHA-256, and set the signing/update booleans only after the platform commands below
have succeeded.

Run the verifier shipped beside the Qt executable (repeat `--package` for every required artifact):

```sh
opennow-acceptance-verify \
  --live /absolute/path/opennow-live-acceptance.json \
  --performance-1080p /absolute/path/opennow-1080p.json \
  --performance-4k /absolute/path/opennow-4k.json \
  --attestations /absolute/path/opennow-attestations.json \
  --package /absolute/path/OpenNOW.AppImage \
  --package /absolute/path/opennow.deb \
  --output /absolute/path/opennow-verification.json
```

Exit status 0 is the only pass. Status 1 writes a fail-closed report listing unmet gates; status 2
means the inputs could not be safely read. The verifier rejects symlinks, malformed/oversized JSON,
headless/software performance reports, fewer than three workload cycles, mismatched versions,
machine labels, architectures or window systems, missing platform package types, false manual
attestations, package hash/size mismatches, and unverified signing/update metadata. Its output keeps
only input basenames, sizes and hashes rather than local paths.

## Signing and package verification

- Windows: verify Authenticode on every executable and installer with `signtool verify /pa /all`;
  install, upgrade, uninstall, and launch the portable build on both architectures.
- macOS: verify the hardened-runtime signature with `codesign --verify --deep --strict`, Gatekeeper
  with `spctl --assess`, and notarization attachment with `stapler validate`; test the native
  package on each architecture rather than treating Rosetta as Apple-Silicon proof.
- Linux: launch each AppImage on its native architecture, install/uninstall the DEB, verify desktop
  and `opennow://` associations, and confirm the package uses its bundled core and streamer.
- Verify the published SHA-256 hashes and production Ed25519 update manifest before exposing a
  release to any update channel. Keep the signing key outside CI build workers.

## Staged rollout and Electron removal

Roll out the Qt build in explicit cohorts. Monitor crash-free launches, core/streamer restart rate,
session-start success, first-frame failures, decoder fallback/error rates, queue drops, update
rollback, and opt-in feedback. Define the observation window and rollback threshold before the
first cohort.

Only after every matrix row, signed artifact, update-manifest check, performance report, and rollout
criterion passes may the Electron main/preload/renderer, dependencies, builder jobs, and root entry
points be deleted. Run the full Qt/Rust/package suite again after that deletion before declaring
the migration complete.
