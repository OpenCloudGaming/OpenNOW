# OpenNOW Qt acceptance runbook

This runbook turns the remaining migration gates into reproducible evidence. A development-host
smoke test is not release acceptance. The legacy Electron source has been removed, but release
acceptance still requires every row to be executed on the named hardware, the artifacts to be
reviewed, and the staged rollout to complete.

## FSR 1 upscaling on Windows and Linux

Stream settings and onboarding offer Off or FSR 1 on Windows and Linux. macOS retains
MetalFX. FSR 1 runs AMD's EASU spatial upscaler and optional RCAS sharpening on the existing
GPU video textures. Clarity controls sharpening; zero disables RCAS, not EASU. This does not
change the requested stream resolution or frame rate. Noise Reduction remains MetalFX-only.

FSR 1 applies only when enlarging SDR video on D3D11 or Vulkan. HDR, native-size video,
downscaling, and unsupported resources retain normal scaling. It adds GPU work and can
emphasize compression artifacts, so compare it with Off on the same stream before enabling
it permanently.

After building, run the settings and presenter checks:

```sh
ctest --test-dir build/opennow-qt --output-on-failure -R 'fsrupscaler|qml-upscaling|qml-onboarding|opennow-streamvideo-tests'
cargo test --manifest-path native/opennow-core/Cargo.toml upscaling
```

On Windows D3D11 and Linux Vulkan hardware, enlarge an SDR stream and switch between Off
and FSR 1 at Clarity 0 and 15. Check windowed and fullscreen modes, display-scale changes,
F3 statistics, and Ctrl+G overlays. Repeat with frame generation enabled and after a session
restart. Confirm that video and audio continue, pointer mapping follows the viewport, and
HDR and native-size video remain unchanged. Synthetic tests do not establish live GPU cost
or image quality on these devices.

## Stream Stats V2 visual check

After building the Qt application, run:

```sh
ctest --test-dir build/opennow-qt --output-on-failure -R 'stream-stats|streamtoasts|queue-drops|frame-generation-stats|fullscreen.*stats'
bash scripts/capture-qt-stream-stats.sh build/opennow-qt/opennow-qt build/stats-v2-captures
```

The capture script renders compact, expanded, degraded, 1.5× scale, and controller/packet-loss
toast states using synthetic telemetry, without an account or a live session. Compare them with
OpenNOW Socket → Desktop Renew → Renew 08 and Renew 09a–c in Paper. These images verify layout,
not network quality or hardware decode. Unsupported FEC recovery and decoder queue values are
not populated from the design's example numbers. The bitrate bar divides measured Mbps by the
prepared session's allocation and clamps its fill to 0–100%.

## Check existing-session recovery

Run the orchestration and protocol regression tests:

```sh
ctest --test-dir build/opennow-qt --output-on-failure -R 'embedded-orchestration|qml-session-resume'
cargo test --manifest-path native/opennow-core/Cargo.toml cloudmatch::tests
```

After building the app, capture the different-game confirmation without signing in:

```sh
build/opennow-qt/opennow-qt --smoke-test --allow-multiple-instances \
  --desktop --route inserting --reduced-motion --smoke-width 960 --smoke-height 640 \
  --smoke-session-resume conflict --screenshot /absolute/path/session-conflict.png
```

Replace `conflict` with `unavailable` to check the session-limit retry screen, or with
`resuming` to check the reconnect message. Repeat in console mode by replacing
`--desktop` with `--console`. These fixtures use synthetic sessions and do not connect
to NVIDIA or prove live resume behavior.

With an authorized account, disconnect from a game without ending its cloud session,
restart OpenNOW, and select Play for the same game. Check that OpenNOW reconnects
without asking to create another session. Select a different game and verify that
Cancel preserves the running game, Return to game reconnects to it, and End game and
start new closes it only after you choose that action. Repeat with the session in a
different region and while the network is unavailable. A failed lookup must offer a
retry rather than create another session. Verify windowed and fullscreen presentation.

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

## Custom library collections

Run `ctest --test-dir build/opennow-qt -R qml-collections --output-on-failure`
for the isolated 960- and 1440-pixel collection workflows in dark and light themes. These exercise the real
Qt library, collection editor, and state owner with a mock core transport, including
create/rename/delete, multiple membership, search/store/hidden filters, rejected
names, failed saves, and disconnect recovery. Persistence and validation are covered
by `cargo test --manifest-path native/opennow-core/Cargo.toml settings::tests`.

For visual evidence, run the built app with `--smoke-test --allow-multiple-instances
--desktop --route library --smoke-collections --reduced-motion --screenshot
/absolute/path/collections.png`. The collections in this screenshot are test-created;
new installations start with no collections. For a manual restart check, create a
collection using **New collection** or the sidebar **+**, add/remove games through
**Add to collection** in a game's context menu, restart, then rename and delete the
folder. Membership and names must survive restart, and deleting a folder must not
remove games from the library.

## Queue-drop reporting

Run `ctest --test-dir build/opennow-qt -R qml-queue-drops --output-on-failure`
for the 960- and 1440-pixel stats and session-report workloads. They exercise typed
video/audio/callback counters, sample-to-duration conversion, invalid inputs,
same-session reconnects, shutdown deltas, new-session reset, retained exports,
and the existing drop-metric visibility preference using a mock core transport.

For screenshots, run the built app with `--smoke-test --allow-multiple-instances
--desktop --route settings-streaming --smoke-queue-drops --reduced-motion
--screenshot /absolute/path/drops.png`. Add `--smoke-queue-report` to capture the
completed-session report instead. These values are synthetic test inputs, not a
live gameplay measurement. Audio packet/block drops remain separate because
their discarded duration is unknown; the legacy mixed-unit total is not shown.

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
12. For VPN compatibility, start fresh sessions with Cloudflare WARP off and on, keeping the
    same game and stream settings. Record the OS, WARP version, and tunnel mode (WireGuard or
    MASQUE). Confirm first-frame playback, audio, and input with WARP still enabled. The native
    `nvst-udp` log reports the video peer's `interface_mtu`, `vpn_detected`, and `packet_size`;
    a detected VPN's 1,280-byte IPv4 interface should select 1,216, while a 1,500-byte interface
    retains 1,280. Non-VPN and unrecognized routes always retain the original 1,280-byte packet
    size, even with a smaller MTU. Verify a split-tunnel route that bypasses the VPN also retains it.
    These are NVST payload sizes, not complete IP packet sizes: sizing reserves IP, UDP, RTP/FEC,
    and the largest supported SRTP authentication tag, then rounds down to 16 bytes. Detection
    uses Linux TUN/TAP metadata and known VPN interface names, macOS tunnel interface names,
    and known VPN driver descriptions on Windows. It is conservative, not an exhaustive VPN
    inventory; if route discovery or VPN identification fails, packet sizing stays unchanged.
    This queries the local outgoing interface,
    not the full Internet path, and does not bypass a VPN or a firewall blocking UDP. Keep paired
    diagnostic exports if video still fails; changing WARP during a live session is a separate
    route-change/recovery test, not evidence that fresh-session negotiation passed.
13. Export both the redacted diagnostic report and **live evidence** from the Diagnostics screen
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

## Local Store paging checks

Run `ctest --test-dir build/opennow-qt --output-on-failure -R "qml-store-(paging|navigation)"`.
The fixture covers demand-only continuation, global facets, local ranking passthrough,
six-result command-palette queries, cancellation, offscreen shelf requests, partial-row
poster sizing, and keyboard/manual scrolling at both motion settings.

With a signed-in account and a complete saved Store catalog, run the optional live check:

```powershell
./opennow-qt/tests/verify_store_local.ps1 -CorePath ./build/opennow-qt/opennow-core.exe
```

It opens a separate core process, checks bounded local pages and all saved categories,
verifies metadata-only shelf responses and ranked searches, then closes its own process.
No cache invalidation is requested. Also inspect the native Store: the loaded count must
stay at 40 while idle; Load more adds one page; route re-entry retains it; category selection
and See all remain in Store; Ctrl+K finds games outside the loaded page. Scroll through a
short final row and confirm its posters remain the same size as those in a full row.

## Desktop settings motion

`qml-idle-mode` verifies that expiry of the mouse grace period cannot change the
selected shell, general input-mode changes cannot trigger console mode, an explicit
desktop choice wins over automatic controller switching, and fresh controller actions
still honor the automatic-switch preference. `opennow-controllerinput-tests` checks
that hotplug, stick drift and held navigation repeats do not emit fresh activity.

Run `ctest --test-dir build/opennow-qt -R qml-settings-motion --output-on-failure`
for windowed/fullscreen and normal/reduced-motion coverage. These cases verify that
Account activity-sharing and crash-report controls are visible with Advanced closed
(without changing either preference), inline pickers and Advanced sections have
intermediate frames, rapid reversals settle at the correct height, and section/page
changes finish fully opaque without fading the shell. Shortcuts must expand inline.
Reduced motion must settle immediately. In the native app, also check resolution,
theme, region and language pickers, Escape-to-close/focus return, and quick switching
between settings sections and desktop pages.

## Native session resume and frame lifetime

- Pointer-lock HUD regression: in desktop and console shells, compact/expanded
  statistics must be pointer-transparent while the stream's relative mouse mode is
  enabled. F3/configured stats and copy shortcuts must still work. Unlocking restores
  panel taps/scrolling. `qml-stream-recovery` covers both panel enablement states;
  `opennow-streamvideo-tests` verifies native Windows raw-input confinement remains
  one pixel in windowed/fullscreen modes and is released when overlays disable input.

- Run `qml-stream-recovery`: claim acknowledgement and transient status `6` must
  not prepare a streamer; a fresh ready poll must. Recovery must discover and claim
  the exact previous session, ignore other games, and respect the retry budget.
- Run `opennow-nativestreamruntime-tests`: presentation is invalidated immediately
  on stop, failure, shutdown and a new start; only the matching successful start
  response can enable it again.
- Run `opennow-streamvideo-tests` on the native Windows platform (not just offscreen):
  clearing imported video must reveal the background while preserving overlay pixels,
  and new frames must import normally in windowed/fullscreen views.
- With a real account, resume an existing seat and verify RESUME acceptance followed
  by ready polling before native setup. Interrupt and restore the stream connection:
  the same cloud game must reconnect with fresh context, without creating a new seat.
  End the session during recovery and verify no late response restarts it. Stop one
  game and start another; no frame from the previous game may flash during startup.

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

## Staged rollout after Electron removal

Roll out the Qt build in explicit cohorts. Monitor crash-free launches, core/streamer restart rate,
session-start success, first-frame failures, decoder fallback/error rates, queue drops, update
rollback, and opt-in feedback. Define the observation window and rollback threshold before the
first cohort.

The Electron main/preload/renderer, dependencies, builder jobs and root entry points have been
removed. Every matrix row, signed artifact, update-manifest check, performance report and rollout
criterion must still pass before declaring the migration release-ready. Run the full
Qt/Rust/package suite against the Qt-only source tree; source deletion is not acceptance evidence.
