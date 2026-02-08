# Cross-Platform Rebuild Roadmap (Windows + macOS + Linux + Raspberry Pi)

This roadmap is for a **full rebuild** focused on one goal:

> Ship one lightweight app architecture that runs everywhere with minimal per-platform patching.

## Reality check (important)

There is no desktop stack with **zero** platform-specific work forever. GPU drivers, input APIs, packaging/signing, and media backends differ by OS.

What we *can* do is design a stack where platform-specific code is tiny, isolated, and rarely changed.

## Recommended full-rewrite stack

If you are willing to fully rewrite and want broad compatibility + Raspberry Pi support, use:

- **Language:** C++20
- **UI + Windowing:** Qt 6 (Qt Quick/QML)
- **Rendering/Input (stream surface):** SDL2 (or native Qt scene integration)
- **Media pipeline:** GStreamer (single pipeline model across Linux/macOS/Windows)
- **Networking/WebRTC:** libdatachannel or Google WebRTC native
- **Build system:** CMake + Conan/vcpkg
- **Packaging:**
  - Windows: MSI (WiX)
  - macOS: signed `.app` + DMG
  - Linux desktop: AppImage + Flatpak
  - Raspberry Pi: `.deb` + AppImage fallback

### Why this stack

- Qt is one of the most proven desktop cross-platform toolkits.
- C++ keeps runtime overhead lower than Electron/Chromium-based stacks.
- GStreamer already has strong Linux/Pi support and solid Windows/macOS paths.
- You can keep one UX and one media architecture instead of shell-swapping.

## Alternatives (and why not first choice here)

| Option | Why teams choose it | Why it is weaker for your stated goal |
|---|---|---|
| Flutter desktop | Great UI velocity, strong cross-platform UI consistency | Heavier runtime than a tuned C++ app; Pi support is workable but less ideal for video-heavy low-latency workloads |
| Electron | Fastest development and plugin ecosystem | Too heavy for Raspberry Pi + cloud-streaming decode scenarios |
| Go desktop rewrite | Simple language, good tooling | Desktop UI ecosystem is less mature for a long-lived media-heavy app |
| Keep Rust + new shell | Reuses existing work | You explicitly said you are okay with full rewrite and want to avoid ongoing platform-fix complexity from current direction |

## Architecture to minimize platform-specific fixes

Use a strict layered design:

1. **Core domain layer (100% portable C++)**
   - Session state machine
   - Auth/session token orchestration
   - Input abstraction model
   - Settings/config model

2. **Media layer (mostly portable)**
   - Unified GStreamer graph builder
   - Codec selection strategy
   - Adaptive bitrate and latency controls

3. **Platform adapters (small boundary only)**
   - GPU decode capability probe
   - Window handle plumbing
   - Gamepad/raw input adapter
   - Installer/update hooks

4. **UI layer (Qt/QML)**
   - No direct platform calls
   - Talks only to core interfaces

Hard rule: platform code must stay in `platform/<os>/` and never leak into domain/UI layers.

## Raspberry Pi requirements (first-class target)

- Target Raspberry Pi 4/5 (64-bit OS only).
- Prefer H.264 decode path first; AV1 optional later.
- Use low-power defaults:
  - 720p / 60 FPS preset
  - conservative bitrate auto mode
  - reduced UI effects
- Add a Pi-specific “Performance Mode” profile.
- Validate on both X11 and Wayland sessions.

## Delivery plan (rewrite)

### Phase 0 — Foundation (2-3 weeks)
- Create new repo structure and CI matrix.
- Bring up Qt app shell and settings persistence.
- Set up package builds for all target OSes.

### Phase 1 — Streaming MVP (4-6 weeks)
- Implement login/session flow.
- Build basic video/audio stream + input forwarding.
- Deliver playable session on Windows/macOS/Linux.

### Phase 2 — Raspberry Pi stabilization (3-4 weeks)
- Tune decode paths and defaults for Pi.
- Add thermal/performance telemetry panel.
- Publish Pi build artifacts and install docs.

### Phase 3 — Parity + hardening (4-8 weeks)
- Add gamepad/racing wheel mapping.
- Add crash reporting + startup diagnostics export.
- Freeze API boundaries and optimize startup/memory.

## CI and quality gates (must-have)

Run for every PR:

- Build: Windows, macOS, Ubuntu, Raspberry Pi (arm64 cross or native runner)
- Smoke test: login screen launch, session start, input event, audio output, clean shutdown
- Packaging check: install/uninstall
- Performance budget checks:
  - startup time
  - memory ceiling
  - dropped-frame threshold

Reject merges that fail any target platform gate.

## Maintenance model

To keep long-term maintenance easy:

- One owner per layer (core/media/platform/ui).
- Monthly dependency update window.
- Platform bug triage labels (`win`, `mac`, `linux`, `pi`).
- Never merge emergency OS hacks without adding a cross-platform abstraction follow-up task.

## Final recommendation

Given your goals (all major desktop OSes + Raspberry Pi + lightweight + fewer platform-specific surprises), a **C++ + Qt + GStreamer** full rewrite is the most practical fit.

It is more upfront work, but it gives the best chance of stable long-term cross-platform behavior with low runtime overhead.
