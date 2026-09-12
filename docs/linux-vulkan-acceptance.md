# Linux embedded Vulkan Video acceptance

This checklist validates the Linux Qt implementation on a real GPU. A successful build, a
software Vulkan renderer, or a standalone streamer test does not prove embedded playback.
Record the commit, package, distribution, GPU, driver version, and whether the run uses native
X11 or native Wayland. Repeat on each window system the package claims to support.

## Before playback

Use a driver exposing Vulkan Video decoding for the selected codec. Vulkan rendering support
alone is not sufficient. Run `vulkaninfo --summary` to identify the adapter and driver; a
software adapter such as llvmpipe cannot validate hardware video decoding.

Start the Qt executable from its build or extracted package directory so it loads the matching
native library and core. Do not substitute a separately installed streamer executable. Sign in
interactively with an authorized account; never put credentials in launch arguments or logs.

In Stream settings, explicitly select the Vulkan backend and H.265 with 8-bit 4:2:0 first. Auto
may select another backend and is not evidence that Vulkan Video works. An unavailable Vulkan
option is a failed or unsupported capability probe, not a reason to force-enable the option.

For a development build, run the Qt device-adoption test on the actual desktop before gameplay:

```sh
OPENNOW_TEST_VULKAN_VIDEO=1 QT_QPA_PLATFORM=xcb \
  ./build/opennow-qt/opennow-linuxvulkangraphics-tests \
  adoptsBeforeExposureAndSurvivesSceneGraphRecreation
```

Use `QT_QPA_PLATFORM=wayland` for native Wayland. The explicit hardware flag turns an unavailable
device or window system into a failure rather than a skipped test. This checks Qt adoption,
scene-graph recreation, fullscreen, and resize on the same device; it does not decode a stream.

The separate native HEVC tests require the FFmpeg command-line encoder and a Vulkan Video GPU.
Run from the repository root; these opt-in tests fail on unsupported hardware rather than skip:

```sh
cargo test --manifest-path native/opennow-streamer/Cargo.toml \
  -p opennow-streamer-platform-linux --features ffmpeg-bundled \
  shared_vulkan_hevc_nv12_gpu_only -- --ignored --nocapture
cargo test --manifest-path native/opennow-streamer/Cargo.toml \
  -p opennow-streamer-platform-linux --features ffmpeg-bundled \
  shared_vulkan_hevc_p010_gpu_only -- --ignored --nocapture
```

The tests check actual HEVC decode into GPU-only presentation snapshots, including frame lifetime
after decoder destruction. They complement, rather than replace, the live Qt playback checks below.

## Playback and composition

1. Launch a title and play for at least ten minutes. Video must appear inside the existing Qt
   window. There must be no second presenter window, child streamer process, or standalone SDL
   video surface.
2. Open and close F3 statistics, the Ctrl+G menu, and exit confirmation while the scene is moving.
   Check that video and audio continue behind the overlays and that dismissing them does not
   restart the session. Check both desktop and console shell modes.
3. Repeat with overlays open and closed in windowed and fullscreen modes. Resize the window,
   switch displays where available, and test the desktop's scaling settings. Video placement,
   clipping, overlay alignment, and pointer mapping must stay correct.
4. Confirm a consumed local shortcut does not reach the game. Confirm ordinary gameplay keys
   still do. Enter and dismiss an overlay while holding movement keys, mouse buttons, and a
   controller button; no remote input may remain stuck.
5. Inspect diagnostics for the actual codec and backend. Video must advance through receive,
   assembly, decode, and presentation; continuing audio alone is not a pass. The embedded Vulkan
   path must not report a CPU frame download, software decoder fallback, or cross-device import
   fallback.

## Color and recovery

Repeat playback with H.265 10-bit 4:2:0 when it is offered by the device and service. Confirm the
decoder preserves P010 and the native conversion publishes RGB10A2 rather than an 8-bit
intermediate. Qt's SDR swapchain is 8-bit: the final video draw applies centered spatial
dithering after scaling, including to generated frames, without changing the transfer function.
Check `qt-native.log` for `SDR video composition: textureBits=10 outputBits=8 dither=ordered-8x8`.
Check dark gradients and limited/full-range black and white levels. This is an SDR
precision check, not a claim of 10-bit scan-out or HDR certification. Unsupported chroma/bit-depth combinations must fail
before session creation instead of silently changing the selected quality.

Interrupt the network briefly, then restore it. Recovery must discard unusable reference frames
and resume from a valid frame without unbounded queue growth. Exit the session and start another
one several times; frames and pressed input from the previous session must not appear in the
new session.

Exercise scene-graph/surface recreation with the platform's available tooling. Ordinary overlay
changes and fullscreen transitions must not tear down the media session. A genuine graphics
device loss must not reuse stale native handles, hang shutdown, or display corrupt frames; record
whether the application recovers or reports a usable failure. Do not equate minimizing a window
with testing device loss.

The shared device is application-owned. A decoder GPU timeout/device-loss failure disables that
device rather than creating a replacement behind Qt's back; restart OpenNOW after the driver has
recovered. Ordinary scene-graph recreation reuses the live device and is a separate test.

## Evidence

Export diagnostics from Settings → About → Copy diagnostics after the run. Include the exact
selected backend, codec, color quality, resolution and frame rate with the reproduction steps.
Attach a short capture showing live video behind the menu and a fullscreen transition. Review
the export and capture for account details before sharing them.

Report each unsupported or untested case explicitly. A successful H.265 8-bit run does not prove
Main10, AV1, HDR, every GPU vendor, or both Linux window systems. See [the full Qt acceptance
runbook](qt-acceptance.md) for release-level requirements.
