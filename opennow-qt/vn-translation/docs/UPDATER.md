# VN fork — updater safety (read before shipping any VN build)

Status: findings only, verified against upstream tag v1.0.4 / commit `bec4a0b`.
**No code in this file has been changed yet.** The change below is a
recommendation for a follow-up task; do not apply it silently.

## Short answer

**The updater lives entirely in the Rust core (`native/opennow-core/`), which
the VN fork must not touch.** Its release URLs are hardcoded to the upstream
repo `OpenCloudGaming/OpenNOW`, and its signing key is compiled in at build
time. There is therefore no QML-only way to repoint the updater at the VN
fork's releases — a repoint needs a (small, deliberate) Rust change plus a
new VN signing keypair. Until that is done, the safe posture is:

1. **Ship VN builds WITHOUT any pinned update key** (fail-closed by
   construction — downloads are refused), **and**
2. **Add a QML-level guard that disables the automatic update check by
   default in VN builds** (see §4), so the app never even phones upstream
   GitHub Releases on its own.

## 1. How the upstream updater works

End-to-end flow (all in Rust, driven from QML through the versioned JSON
protocol — see `docs/core-protocol.md`):

1. QML asks for a check: `ShellStore.checkForUpdates()` in
   `opennow-qt/qml/state/ShellStore.qml:1666` sends
   `CoreClient.request("updater.check", {channel: settings.updateChannel})`.
   A 6-hour timer does the same automatically — see §3.
2. Rust `UpdaterService::check()` in
   `native/opennow-core/src/updater.rs:193` fetches the release list from
   `RELEASES_URL` (GitHub API, §2), picks the newest installable release for
   the channel (`select_installable_release` / `select_latest_release`,
   `updater.rs:779-796` — drafts are always skipped; prereleases are skipped
   on the `stable` channel, allowed on `nightly`), and finds the
   platform-compatible asset plus its sibling `<asset>.manifest.json`.
3. Download is gated on the Ed25519 signature: `verify_manifest()` in
   `native/opennow-core/src/update_apply/verification.rs:92` verifies the
   manifest (version / asset / size / SHA-256, exact payload in
   `docs/update-signing.md`) against the pinned public key. The staged file
   is re-hashed immediately before install (`verify_package`,
   `verification.rs:96`).
4. Install is staged atomically; AppImage keeps a `.previous` rollback copy.
   Install always requires user confirmation in the UI
   (`ShellStore.installUpdate()`, `updater.install`).
5. Release notes come from the same GitHub release body
   (`update_highlights`, `updater.rs`).

Manual fallback: the Updates settings page has an "Open releases" button
(`opennow-qt/qml/desktop/settings/pages/DesktopSettingsUpdatesPage.qml:111`)
that opens `updaterState.releaseUrl` — which, unmodified, is an upstream URL.

## 2. Where the repo and the signing key are configured

### Repo URLs — hardcoded constants, Rust only

`native/opennow-core/src/updater.rs:18-20`:

```rust
const RELEASES_URL: &str = "https://api.github.com/repos/OpenCloudGaming/OpenNOW/releases";
const RELEASES_PAGE: &str = "https://github.com/OpenCloudGaming/OpenNOW/releases";
const RELEASE_ASSET_PREFIX: &str = "https://github.com/OpenCloudGaming/OpenNOW/releases/download/";
```

Two more pins enforce the same repo — `trusted_release_url()`
(`updater.rs:1006-1010`) accepts only `html_url`s under
`https://github.com/OpenCloudGaming/OpenNOW/releases/`, and
`trusted_asset_url()` (`updater.rs:996-999`) accepts only download URLs
under the `RELEASE_ASSET_PREFIX`. **Changing any one of these without the
others leaves the updater half-pointing at upstream.** All five sites must
change together (3 constants + 2 validators).

There is no config file, environment variable, or QML setting that overrides
the repo. QML only chooses the channel (`stable` / `nightly`).

### Signing key — compile-time env var, NOT a runtime setting

- Key source: `embedded_update_key()` in
  `native/opennow-core/src/update_apply/verification.rs:24-30` reads the
  compile-time env var `OPENNOW_UPDATE_ED25519_PUBLIC_KEY` via `option_env!`.
- CMake plumbing: `opennow-qt/CMakeLists.txt:55-56` declares it as a cache
  variable defaulting to `""`; `opennow-qt/cmake/NativeRuntime.cmake:24,183`
  forwards it to the Rust build.
- CI plumbing: `.github/workflows/qt-ci.yml` `workflow_dispatch` input
  `public_key` defaults to the upstream production key; `qt-build.yml:131,346,451,610`
  passes it through as `update_public_key` → `OPENNOW_UPDATE_ED25519_PUBLIC_KEY`.
- Production public key (safe to publish, recorded in
  `opennow-qt/packaging/update-public-key.base64`):
  `HoyKVmfuH+KDioPCJJNxOB2e/bXJoOALSjo1QeMsDlk=`
- The matching **private** key lives only in the upstream
  `qt-update-signing` GitHub environment secret
  `OPENNOW_UPDATE_ED25519_PRIVATE_KEY`. The fork does not have it and must
  never try to obtain or reuse it.

### Fail-closed behaviour (the safety net we rely on)

Per `docs/update-signing.md`: a build compiled **without** a pinned public key
may discover releases but reports `signaturePolicy: unconfigured-fail-closed`
and **cannot download an update** (`updater.rs:1831` test
`builds_without_a_pinned_key_cannot_download_updates`). Ordinary `qt-ci`
check builds (push/PR) never pass a key, so they are fail-closed by default.
Only `workflow_dispatch` with `publish_nightly` (or a manual
`qt-release-candidate` run) embeds a key.

## 3. Auto-check defaults — the trap

- **Rust default is ON.** `native/opennow-core/src/settings.rs:1463`:
  `"autoCheckForUpdates":true, "autoDownloadUpdates":false`. A fresh
  install therefore auto-checks every 6 hours.
- The QML toggle in `DesktopSettingsUpdatesPage.qml:154` reads
  `boolSetting("autoCheckForUpdates", false)` — that `false` is only a
  fallback for a missing setting; it does **not** override the Rust default
  that is already stored. Changing the QML fallback alone changes nothing.
- The timer is `ShellStore.qml:1025-1031` (`autoUpdateCheckTimer`, 15 s first
  run, then 60 s ticks) and `runAutomaticUpdates()` at
  `ShellStore.qml:1749-1765` (check at most every 6 h, `21600000` ms, only
  when no session is active). Auto-download is off by default and retries at
  most 3 times with backoff — but auto-**check** alone is enough to surface
  an upstream release and offer it to the user.
- Channel default: `settings.rs:618-623` + `version.rs:6-14`
  (`update_channel()` — `nightly` only if the build version has a `-nightly.`
  prerelease tag; a `1.0.4` VN build defaults to `stable`).

## 4. Recommendation (minimal safe change for the VN fork)

Do this, in order. Steps 1–2 are the minimum before any VN build is handed
to Ethan; step 3 is the later, proper fix and needs its own approval.

### Step 1 — Never embed a key in VN builds (zero code, just discipline)

- Never run `workflow_dispatch` with `publish_nightly: true` on this fork,
  and never pass `update_public_key` / `OPENNOW_UPDATE_ED25519_PUBLIC_KEY`
  in any VN build. With no key, downloads fail closed even if a check ever
  finds an upstream release.
- Never copy the upstream public key into a VN build "to make updates work":
  that key only verifies manifests signed with the upstream private key,
  i.e. upstream builds. Embedding it is exactly how a VN install would
  auto-update itself into upstream.

### Step 2 — QML-level guard: disable the auto-check in VN builds (the change to make)

Since the Rust default (`settings.rs:1463`) is `true` and QML cannot change
a stored default by editing a fallback, the guard must actively intervene.
Minimal option, all in QML, no Rust touched:

- In `opennow-qt/qml/state/ShellStore.qml`, make `runAutomaticUpdates()`
  (`:1749`) and `checkForUpdates()` (`:1666`) no-op when a VN-build flag is
  set, and force `autoCheckForUpdates` to `false` on first run (e.g. set the
  setting to `false` from `ApplicationStartup` / shell init when the flag is
  present and the user has never explicitly enabled it).
- Exact lines to touch: `ShellStore.qml:1666` (`checkForUpdates`),
  `ShellStore.qml:1749` (`runAutomaticUpdates`), `ShellStore.qml:1025`
  (`autoUpdateCheckTimer` `running:` condition). The flag itself can be a
  QML-visible build constant (e.g. exposed from `ApplicationStartup.cpp` —
  one of the 3 allowed shim files — or a CMake-defined `OPENNOW_VERSION`
  suffix); do not invent a second settings key.
- Manual "Check for updates" must also be disabled or clearly labelled in
  VN builds: it currently calls the same `checkForUpdates()` path, and the
  "Open releases" button would still open an upstream releases page.
- Keep install confirmation as-is: even with the guard, installation always
  requires confirmation; the guard removes the *discovery* step, not the
  confirmation step.

### Step 3 — Proper repoint (later, separate approval, touches `native/`)

Only when the fork wants its own signed updates:

1. Generate a **new VN-only Ed25519 keypair** (offline, stored as a
   `duyzoz/OpenNOW-ORC` repo/environment secret — never in the repo, never the
   upstream key). Publish the new public key in
   `opennow-qt/packaging/update-public-key.base64` (replacing the upstream
   value) and pass it via CI.
2. In `native/opennow-core/src/updater.rs:18-20`, change all three URL
   constants from `OpenCloudGaming/OpenNOW` to `duyzoz/OpenNOW-ORC`, **and**
   update `trusted_release_url()` (`:1006`) and `trusted_asset_url()`
   (`:996`) to the same repo. All five sites, one commit, with the
   `updater.rs` unit tests updated for the new URLs.
3. Publish VN releases with sibling `<asset>.manifest.json` files signed
   with the VN private key (same format as `docs/update-signing.md`).
   Until step 3 exists, step 2 alone is harmless but useless — checks will
   find no releases and report "No published releases were found".

Do **not** do step 3 as a drive-by inside a translation PR. It changes trust
roots and needs Ethan's explicit go-ahead plus a key-generation ceremony.

## 5. What to verify before handing Ethan a build

- [ ] VN build was produced without `OPENNOW_UPDATE_ED25519_PUBLIC_KEY`
      (check the CI run's configure line / `signaturePolicy` reads
      `unconfigured-fail-closed` in the app's updater state).
- [ ] On first launch with no prior settings, no automatic update check
      fires within the first hour (watch `lastAutoUpdateCheckMs` / the
      Updates page status stays "Ready to check GitHub Releases" or the VN
      guard message — it must never show an upstream version as available).
- [ ] The Updates settings page does not offer an upstream download, and
      "Open releases" does not link to `OpenCloudGaming/OpenNOW`.
- [ ] `grep -rn "OpenCloudGaming/OpenNOW" native/opennow-core/src/updater.rs`
      still shows the upstream constants (expected until step 3) — and the
      QML guard is what prevents them from being reached automatically.

## References

- `docs/update-signing.md` — signing contract, fail-closed policy, manifest format.
- `docs/qt-release-candidate.md` — candidate/promotion flow; `qt-update-signing` env.
- `native/opennow-core/src/updater.rs` — `UpdaterService`, constants `:18-20`,
  `check()` `:193`, `trusted_*` `:996-1010`, fail-closed test `:1831`.
- `native/opennow-core/src/update_apply/verification.rs` — `embedded_update_key()` `:24`.
- `native/opennow-core/src/settings.rs:1463` — `autoCheckForUpdates: true` default.
- `opennow-qt/qml/state/ShellStore.qml:1025,1666,1749` — timer, check, auto-run.
- `opennow-qt/qml/desktop/settings/pages/DesktopSettingsUpdatesPage.qml` — UI.
