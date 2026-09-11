# Qt/native CI caching

The `qt-ci` and `qt-release-candidate` workflows use upstream cache actions.
[Blacksmith automatically routes these actions to its colocated cache](https://docs.blacksmith.sh/blacksmith-caching/dependencies-actions);
its old `useblacksmith/cache` and language-specific cache forks are archived.

Actions currently builds Windows and Linux only, each on x64 and ARM64. The macOS
CI matrix entries and release-candidate job are removed for now.

## Cached build inputs

- **Qt SDK:** `install-qt-action` already caches the downloaded SDK and Windows
  ARM64 host tools. That configuration is unchanged.
- **Rust:** `Swatinem/rust-cache` caches Cargo downloads and compiled dependencies
  for both native workspaces. CI includes their normal `target` directories for
  Clippy/tests and the separate `rust-target` and `streamer-rust-target` directories
  that CMake actually builds. Release candidates cache only the CMake Cargo
  directories. The action keys dependencies by Rust toolchain, Cargo manifests,
  lockfiles, and compiler environment, and removes workspace and incremental
  artifacts before saving. Application code is rebuilt, including release-version
  and update-public-key changes; final executables and signing material are not
  cache inputs.
- **Qt C++:** `ccache-action` persists a compressed compiler cache capped at 500 MB
  per Linux matrix entry. CMake explicitly uses `ccache` as its C/C++ compiler
  launcher. Windows keeps its existing Visual Studio generator: CMake compiler
  launchers do not support that generator, so enabling one would not cache MSVC
  compilation. Windows still benefits from Rust and SDL caching.
- **SDL3:** `actions/cache` persists only the Release installation. An exact hit
  skips cloning, configuring, compiling, and installing SDL. The key includes the
  target architecture, runner image family, SDL version, and workflow hash; changes
  to the build recipe invalidate it. There is deliberately no partial-key restore
  for this prebuilt dependency.

CI and release-candidate caches have separate key namespaces. All compiler and
dependency caches distinguish the target architecture, including cross-builds on
the same host. Keep Blacksmith's **Branch Protected Caches** setting enabled to
preserve GitHub-style branch isolation; workflow keys are not a security boundary.
No cache is required for correctness: misses rebuild from source, and the test,
packaging, smoke, and signing steps remain unchanged.

## Verify the improvement

The successful [baseline run 34043105324](https://github.com/opencloudgaming/opennow/actions/runs/34043105324)
spent 452–453 seconds in `Build Qt release` on Windows and 422 seconds on Linux
ARM64. Linux ARM64 also spent 88 seconds in Rust formatting/Clippy and 140 seconds
in Rust tests. SDL compilation took 19–43 seconds across the six targets. These
are baseline measurements, not promised savings.

Compare a successful cold run with a rerun of the same commit and cache scope.
Check Rust cache restore/save logs, the skipped SDL build on an exact hit, and
the ccache post-step hit statistics. Compare total job time as well as build time:
cache transfer and cleanup are part of the cost. A new stable Rust toolchain or
an evicted cache will require another cold build. Signed release-candidate
validation still requires the protected release environment and its credentials.

[Sticky Disks](https://docs.blacksmith.sh/blacksmith-caching/dependencies-sticky-disks)
are intentionally not enabled. They add storage charges and need separately
configured branch protection before trusted builds can safely reuse them. Consider
them only if measured cache transfer time becomes a bottleneck. Rust sccache's
GitHub Actions backend is also not used: Blacksmith currently documents it as an
exception that still goes to GitHub rather than the colocated cache.
