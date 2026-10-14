# Qt CI performance and runner budget

`qt-ci` runs on pull requests targeting `dev` or `main`, pushes to those two
branches, and manual dispatches. Feature-branch pushes do not start a second
copy of the pull-request matrix. Open a PR or dispatch the workflow to validate
a feature branch. Superseded push/PR runs are cancelled; manual runs remain
independent.

## Build runners

`qt-ci` validation uses `blacksmith-16vcpu-ubuntu-2404` for Linux x64 and
`blacksmith-16vcpu-windows-2025` for Windows x64. CMake and Cargo on those jobs
are limited to eight jobs. macOS ARM64 validation uses `blacksmith-6vcpu-macos-15`
and three jobs. Linux ARM64 and Windows ARM64 are not in that matrix.

Package builds and release candidates use 8-vCPU runners for Linux x64, Linux
ARM64 (`blacksmith-8vcpu-ubuntu-2404-arm`), and both Windows architectures.
Windows package and candidate jobs use Ninja Multi-Config and limit each Cargo
invocation to four jobs. macOS package and candidate builds use
`blacksmith-6vcpu-macos-15`.

Windows desktop-application tests are not run by GitHub Actions. Windows x64
checks run Cargo tests, `ctest -L ci-unit`, and the signed update-helper
integration. Windows ARM64 is cross-built and packaged, not runtime-tested.
The desktop-test workflow and its test-bundle uploads have been removed.

The x64 build is a separate job using the same anchored build steps as the
matrix. Unsigned nightly inventory and signed release inventory require the
platform build jobs, with no Windows desktop-test dependency.
Release dispatches must select a branch or tag pointing to `source_commit`;
preflight rejects a different revision. Build jobs check out
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
- Rust dependency/build caching and Qt SDK caching remain enabled.

[Blacksmith's standard cache](https://docs.blacksmith.sh/blacksmith-caching/dependencies-actions)
automatically accelerates upstream cache actions. There are no paid sticky
disks or archived Blacksmith-specific action forks in this setup. Keep
Blacksmith's branch-protected cache setting enabled, especially for release
builds; this workflow does not change account settings.

## Measuring the change

The first run after a compiler, cache-key, or recipe change can be cold. Compare
successful runs after the caches have been populated, checking the CCache
Statistics job summaries, Rust build times, and packaging duration. Do not infer
end-to-end savings from compilation alone.

Doubling runner cores doubles the per-minute usage rate, so it is only
usage-neutral when the job time halves. Keep Windows at eight cores unless
measured end-to-end savings justify another change under the sponsorship.

Local checks for the cache and release trust contracts:

```sh
python3 -m unittest discover -s opennow-qt/tests -p test_ci_build_cache.py
python3 -m unittest discover -s opennow-qt/tests -p test_ci_release_trust.py
actionlint .github/workflows/qt-ci.yml .github/workflows/qt-release-candidate.yml
```

The native Windows build and packaging still need Windows CI. Desktop runtime
validation must be run separately on Windows. Production signing is not
exercised by ordinary PR validation.
