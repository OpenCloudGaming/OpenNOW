# Session-start progress

The Qt shell presents cloud seat setup separately from local media readiness. The
Rust core already exposes the server's `seatSetupInfo.seatSetupStep` as
`activeSession.seatSetupStep`. `SessionSetupProgress.qml` translates that value
for the desktop loading screen and the console's existing `streamMessage`.

| Cloud seat step | Presentation |
| --- | --- |
| `0` | Connecting to GeForce NOW |
| `1` | Waiting for an available rig, with a queue position when supplied |
| `2`, `3`, `4` | Configuring the cloud gaming rig |
| `5` | Cleaning up the previous session |
| `6` | Waiting for cloud storage |
| Missing, invalid, or unknown | Preparing the game |

A positive queue position retains the queue presentation except when the server
explicitly reports cleanup or storage waiting. The core reads
`seatSetupInfo.queuePosition` before the other containers, and it never reports a
higher position than the lowest one already reported for the same session.
CloudMatch can report a different position on successive polls. The official
client handles this by displaying the lowest position it has seen, and OpenNOW
matches that behavior. Cloud steps do not form a fixed
linear sequence and do not support a calculated percentage. Unknown values do
not imply completion or failure.

These numbers are **not** the native bridge's `NVB_SESSION_SETUP_STATE` enum.
That separate enum uses `3` for starting the streamer and `4` for seat ready.
Do not apply those meanings directly to `seatSetupStep`. The supplied SDK maps
raw steps `0`, `1`, `5`, and `6` explicitly and defaults other steps to configuring.
OpenNOW only names the bounded known range and keeps a generic future-value fallback.

After navigating to the stream, the existing native `starting` status displays
connection initialization. The native `streaming` status before the first-frame
handoff displays a wait for video. It does not claim that gameplay is visible.
Failure, stopping, and reconnecting retain their existing precedence. This
presentation does not change polling, cancellation, session ownership, or the
video item's lifetime.

## Evidence

The comparison used the user-supplied GeForce NOW Linux archive with SHA-256
`47ddbe0425b9ab560f64fa42a0052794c9de335ded0fd59637f082dd7a161ad4`.
Its 325 files contain 32 source maps and 2,717 recovered source entries. Reproduce
the local source index without executing the packaged client:

```sh
python3 scripts/audit-gfn-archive.py /path/to/client.zip /path/to/audit-output
```

The output includes archive file hashes and source-map provenance in
`manifest.json`. Keep the extracted proprietary files outside the repository.

The raw step mapping is in the recovered `ragnarok.js` source, at
`sources/bb8a6388e2b8b096/00004-ragnarok.js`, in the method containing
`switch(t.seatSetupInfo.seatSetupStep)`. The bridge enum is in
`sources/6231326702bae925/00854-Streaming.ts:465-480`, originally
`projects/gfn/streamer/src/app/streaming/streaming/Streaming.ts`.
The adapter's separate progress-state conversion is in
`sources/6231326702bae925/00857-session-manager-adapter.ts:514-550`.
These are bundled source definitions, not an authenticated native-Linux trace.

OpenNOW Mac's corresponding raw mapping is in
`Model/Game/OPNSessionModels.swift`, `progressState(seatSetupStep:queuePosition:)`.
The Qt core normalization is in `native/opennow-core/src/cloudmatch.rs`,
`session_info`.

## Verification

`opennow-qt/tests/theme/tst_sessionprogress.qml` covers every known raw step,
missing/invalid/future values, bounded queue positions, cleanup/storage precedence,
backward transitions, lifecycle overrides, and the native first-frame wait.

```sh
cmake --build build/opennow-qt --target opennow-theme-tests
ctest --test-dir build/opennow-qt --output-on-failure -R '^opennow-theme-tests$'
npm run locales:check
```

The existing `qml-session-launch-*` acceptance checks cover first-frame handoff,
confirmation overlays, reconnects, and the same video surface in windowed and
fullscreen modes. Run those with the complete Qt executable before shipping.
