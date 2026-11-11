# VN fork — CI (what runs, when, and how to get a new file built)

Verified against the workflow files at upstream tag v1.0.4 / commit `bec4a0b`.
No workflow file has been changed by this doc.

## 1. Will pushing `vn-translation` trigger CI automatically? NO

`.github/workflows/qt-ci.yml` `on.push.branches` is exactly:

```yaml
push:
  branches:
    - dev
    - main
```

`vn-translation` is **not** in that list, and neither is any other feature
branch. A plain `git push origin vn-translation` runs **zero** CI jobs —
no checks, no builds. The same applies to `codeql.yml`
(push/PR on `dev`/`main` only).

CI runs on `vn-translation` work only through one of these paths:

| Path | What runs | Notes |
| --- | --- | --- |
| Open a PR `vn-translation` → `dev` (or `main`) | Full `qt-ci`: contracts + 3 platform check jobs (see §2). `qt-ci.yml` `pull_request.branches` includes `dev`, `main`, `capy/explain-cloud-session-setup`, `capy/gfn-correctness/**`. | **This is the intended trigger.** The PR also gets the `paths:` filter — see below. |
| `workflow_dispatch` on `qt-ci.yml` | Preflight + contracts + checks + **manual package builds** + optionally signed nightly publish. | Manual only. **Never dispatch with `publish_nightly: true` on this fork** — it signs with the upstream production key by default and publishes to *this* repo's releases (see `UPDATER.md`). |
| `workflow_dispatch` on `qt-release-candidate.yml` / `qt-stable-release.yml` | Full signed release-candidate / promotion flow. | Manual only, requires exact reviewed SHAs and the production signing environments. Not for VN builds. |

### Paths filter (PRs and pushes)

Even on `dev`/`main`, `qt-ci` only runs when the change touches a listed
path. The translation work **is** covered — the filter includes:

- `opennow-qt/**` — all C++, QML, CMake, packaging, tests (translation
  sources, overlay QML, settings pages, `cmake/*.cmake` all match).
- `native/opennow-core/**`, `native/opennow-streamer/**`
- `locales/**`, `scripts/**`, `package.json`, `decky/**`
- `.github/workflows/qt-*.yml`, `.github/actions/**`, `.github/cmake/**`,
  `.github/scripts/**`, `.github/actionlint.yaml`

Docs-only changes under `docs/vn-translation/` do **not** trigger `qt-ci`
(the filter has no `docs/**` entry). That is fine — docs don't need a build.

### Optional future change (needs approval, not done here)

Add `vn-translation` to `qt-ci.yml` `on.push.branches` so every push gets
checks without opening a PR. Cost: CI minutes on every push; benefit: fast
feedback while the branch is active. Do not add it silently — it edits a
shared workflow file and affects the upstream merge surface (see
`UPSTREAM.md`).

## 2. What jobs run (on a PR to `dev`/`main`)

`qt-ci.yml` job graph: `preflight` (dispatch only) → `contracts` →
`checks` (matrix) → `build` (dispatch only) → `sign-nightly` /
`publish-nightly` (dispatch + `publish_nightly` only).

### `contracts` — shared checks (`.github/workflows/qt-checks.yml`, `workflow_call` only)

Single job, `blacksmith-2vcpu-ubuntu-2404`, ~10 min timeout:

1. **Lint GitHub workflows** — `actionlint` v1.7.12 (SHA-pinned tarball)
   over `qt-ci.yml`, `qt-checks.yml`, `qt-build.yml`,
   `qt-release-candidate.yml`, `qt-flatpak.yml`, `qt-stable-release.yml`,
   `qt-linux-arm64-cross.yml`.
2. **Test build and packaging contracts** — `python3 -m unittest discover
   -s opennow-qt/tests -p 'test_*.py'` (packaging, manifest, nightly-release
   and candidate-promotion contract tests).
3. **Validate localization** — `npm run locales:check` plus
   `node --test scripts/check-translations.test.mjs
   scripts/check-keyboard-layouts.test.mjs`. **Any new UI string must have
   its key in `locales/en.json`** or this job fails. Only edit `en.json`;
   other `locales/*.json` are Crowdin-generated (see repo `AGENTS.md` →
   Localization).

### `checks` — platform unit tests (matrix, needs `contracts`)

| Job | Runner | Qt host / arch | Target |
| --- | --- | --- | --- |
| `linux-x64` | `blacksmith-16vcpu-ubuntu-2404` | linux / `linux_gcc_64` | x64, 8 parallel |
| `windows-x64` | `blacksmith-16vcpu-windows-2025` | windows / `win64_msvc2022_64` | x64, 8 parallel |
| `macos-arm64` | `blacksmith-6vcpu-macos-15` | mac / `clang_64` | arm64, 3 parallel |

Each runs the composite action `.github/actions/qt-unit-tests`:
install Qt, install Linux media deps, limit Rust debug info, run the Qt
unit tests (`ctest`). 30 min timeout, `fail-fast: false`.

Note: these jobs run the **unit tests only** — they do not produce
installable packages. A green `checks` run means the code compiles and unit
tests pass on all three platforms; it does not mean a packaged app exists.

### `build` — manual packages (dispatch only)

`.github/workflows/qt-build.yml` (`workflow_call` only) builds full packages
on 6 targets:

| Label | Runner | Qt arch |
| --- | --- | --- |
| `linux-x64` | `blacksmith-8vcpu-ubuntu-2404` | `linux_gcc_64` |
| `linux-arm64` | `blacksmith-8vcpu-ubuntu-2404-arm` | `linux_gcc_arm64` |
| `windows-x64` | `blacksmith-8vcpu-windows-2025` | `win64_msvc2022_64` |
| `windows-arm64` | `blacksmith-8vcpu-windows-2025` | `win64_msvc2022_arm64_cross_compiled` (+ host Qt tools) |
| `macos-arm64` | `blacksmith-6vcpu-macos-15` | `clang_64` |
| `macos-x64` | `blacksmith-6vcpu-macos-15` | `clang_64` |

**Qt version: 6.8.3** (`jurplel/install-qt-action@v4`, `qt-build.yml:246-275`
and `.github/actions/qt-unit-tests/action.yml:94-102`), modules
`qtmultimedia qtshadertools` plus `qtwaylandcompositor` on Linux. Repo
minimum is Qt 6.8 (`AGENTS.md`, `CMakeLists.txt: qt_standard_project_setup(REQUIRES 6.8)`).
`OPENNOW_BUILD_VERSION` comes from the project version
(`opennow-qt/CMakeLists.txt:10` — `project(OpenNOWQt VERSION 1.0.4 ...)`).

Package builds are **not** produced by PR/push CI. To get a Windows
`.exe`/`.zip` Ethan can run, someone must dispatch `qt-ci` (or
`qt-build` via dispatch) manually — or build locally with
`cmake -S opennow-qt -B build/opennow-qt` + `cmake --build`.

## 3. What a new C++/QML file must do to be included in the build

There is **no glob** — every file is listed explicitly. A new file that is
only created on disk is silently **not built**. Checklist per file type:

### New C++ (`.cpp`/`.h`) under `opennow-qt/src/`

Add it to **`opennow-qt/cmake/Sources.cmake`**:

- App sources: the `qt_add_executable(opennow-qt ...)` list (starts at
  `Sources.cmake:59`, includes `src/main.cpp` and the `app/` sources).
- Or the matching source-set variable if it belongs to a subsystem, e.g.
  `OPENNOW_STREAM_RUNTIME_SOURCES` (`Sources.cmake:18`) or
  `OPENNOW_STREAM_PRESENTATION_SOURCES` (`Sources.cmake:23`).
- Headers go in the same list as their `.cpp` (the repo lists both).
- `src/` is the include root — include cross-module headers qualified,
  e.g. `"streaming/NativeStreamRuntime.h"`.

For the translation feature: new files under
`opennow-qt/src/translation/` need an entry here (a new
`OPENNOW_TRANSLATION_SOURCES` set, or direct listing in
`qt_add_executable`). **As of this writing no `src/translation/` files
exist yet** — the C++ owner must add both the files and the CMake entries
together, or the build will silently omit the feature.

### New QML under `opennow-qt/qml/`

Add it to **`opennow-qt/cmake/QmlModule.cmake`**, in the
`qt_add_qml_module(opennow-qt ... QML_FILES ...)` list (starts at
`QmlModule.cmake:223`). Examples of existing entries:
`QmlModule.cmake:307` (`qml/desktop/settings/DesktopSettingsScreen.qml`),
`:353` (`DesktopSettingsUpdatesPage.qml`), `:371`
(`DesktopStreamScreen.qml`).

- Keep the resource URL stable when moving a file; new QML types must be
  explicitly registered here.
- Singleton declarations must move with their source paths.
- Keep `qsTr` bindings and localized copy intact when extracting components;
  the translator resolves source text independently of filename.

### New localized strings

Add the key to **`locales/en.json` only** (source language). Never hand-edit
other `locales/*.json` files. `npm run locales:check` in the `contracts`
job enforces this.

### New tests

Qt tests are registered in `opennow-qt/cmake/Tests.cmake`; QML acceptance
workloads live there too. New Python packaging/contract tests go in
`opennow-qt/tests/test_*.py` (auto-discovered by the `contracts` job).

## 4. Local verification before pushing

From repo `AGENTS.md` (Build and Checks):

```sh
cmake -S opennow-qt -B build/opennow-qt -DCMAKE_BUILD_TYPE=Debug
cmake --build build/opennow-qt
ctest --test-dir build/opennow-qt --output-on-failure
npm run locales:check   # if any UI copy changed
```

Run the narrowest relevant Qt test first, then the full suite when
practical. Do not claim completion if a relevant check fails — report the
failing command and failure point.

## 5. CI status for this fork

**Unknown until pushed.** No `vn-translation` push triggers CI (see §1), and
no PR has been opened yet, so there is no CI result for the VN work at all —
not green, not red, unknown. The first signal will be the PR
`vn-translation` → `dev` (contracts + 3 platform check jobs). Package
builds need a manual dispatch after that.
