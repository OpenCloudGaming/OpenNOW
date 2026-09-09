# Qt CI performance and runner budget

`qt-ci` runs on pull requests targeting `dev` or `main`, pushes to those two
branches, and manual dispatches. Feature-branch pushes do not start a second
copy of the pull-request matrix. Open a PR or dispatch the workflow to validate
a feature branch. Superseded push/PR runs are cancelled; manual runs remain
independent.

## Windows builds and desktop tests

Windows x64 and ARM64 compilation uses 8-vCPU Blacksmith Windows 2025 runners,
MSVC, and Ninja Multi-Config. CMake is limited to eight build jobs and each Cargo
invocation to four jobs, since the native core and streamer can build together.
The existing Linux and macOS runner sizes are unchanged.

Windows x64 tests run on an interactive GitHub-hosted `windows-2025` desktop.
The build job uploads only the Release runtime and generated CTest files, not
the Cargo targets or compiler objects. The desktop job does not rebuild. Both
jobs map their checkout to `O:` so compiled-in fixture paths and generated test
commands stay valid. The bundle records the source revision and complete test
inventory; a different revision, empty inventory, or changed inventory fails
the handoff. All CTest tests run, including native-window/HDR tests, and failures
preserve diagnostics. Windows ARM64 remains cross-built, not runtime-tested.

The x64 build is a separate job using the same anchored build steps as the
matrix, so its desktop tests can start without waiting for Linux or ARM64.
Unsigned nightly inventory and signed release inventory both require the
desktop tests to pass. Signed release builds use the same transfer and test
workflow, with the immutable release source revision.
Release dispatches must select a branch or tag pointing to `source_commit`;
preflight rejects a different revision. Build and desktop-test jobs check out
the workflow's immutable `github.sha`, not an independently supplied ref, so a
dispatch input cannot select code that writes into the caller's cache scope.

## Caches

- C++ compiler caches are bounded to 1 GB per CI platform/architecture and per
  Windows release architecture. The existing Linux release cache stays at
  500 MB. Windows uses Ninja because CMake's compiler-launcher integration does
  not apply to the Visual Studio generator. macOS now uses a compiler cache too.
- Nightly version definitions apply only to the two C++ sources that consume
  them. A new nightly identity no longer invalidates every application object.
- SDL installations use one shared build recipe. Keys include the SDL release,
  host and target architecture, compiler/CMake identity, macOS deployment
  target, and recipe hash. Unrelated workflow edits no longer evict SDL, while
  toolchain or recipe changes still invalidate it.
- Rust dependency/build caching and Qt SDK caching remain enabled. The Windows
  test-transfer artifact expires after one day; compiler caches do not retain
  the transferred test archive.

[Blacksmith's standard cache](https://docs.blacksmith.sh/blacksmith-caching/dependencies-actions)
automatically accelerates upstream cache actions. There are no paid sticky
disks or archived Blacksmith-specific action forks in this setup. Keep
Blacksmith's branch-protected cache setting enabled, especially for release
builds; this workflow does not change account settings.

## Measuring the change

The first run after a compiler, cache-key, or recipe change can be cold. Compare
successful runs after the caches have been populated, checking the CCache
Statistics job summaries, Rust build times, bundle transfer, and desktop-test
duration. Do not infer end-to-end savings from compilation alone.

Doubling runner cores doubles the per-minute usage rate, so it is only
usage-neutral when the job time halves. Keep Windows at eight cores unless
measured end-to-end savings justify another change under the sponsorship.

Local checks for the transfer and cache contracts:

```sh
python3 -m unittest discover -s opennow-qt/tests -p test_windows_test_bundle.py
python3 -m unittest discover -s opennow-qt/tests -p test_ci_build_cache.py
actionlint .github/workflows/qt-ci.yml .github/workflows/qt-release-candidate.yml \
  .github/workflows/qt-windows-desktop-tests.yml
```

The native Windows build, desktop tests, and packaging still need Windows CI.
Production signing is not exercised by ordinary PR validation.
