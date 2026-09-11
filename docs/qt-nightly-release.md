# Qt nightly releases

Use `qt-ci` on `dev` for the current Qt/native application. Nightly applications have no Windows
Authenticode certificate or Apple Developer ID signature and are not notarized. This workflow
requires no signing environment or key and produces manual-update builds.

## Build and publish

After the release changes are merged into `dev`, run:

```sh
gh workflow run qt-ci --repo OpenCloudGaming/OpenNOW --ref dev -f publish_nightly=false
```

This builds all five targets without publishing anything. Download the five unsigned
artifact groups from that workflow run and perform the relevant checks in
[`qt-acceptance.md`](qt-acceptance.md), especially Windows ARM64, which is cross-compiled and
cannot run its application tests on the x64 CI worker.

To publish a nightly from the selected branch after all CI jobs pass, run:

```sh
gh workflow run qt-ci --repo OpenCloudGaming/OpenNOW --ref dev -f publish_nightly=true
```

Use the registered `qt-ci` workflow name and an explicit `--ref dev`. The Actions web UI may not
show the dispatch control until this workflow is also present on the default branch. Do not use
the legacy `release` workflow instead. Publishing is opt-in: pushes, pull requests, and a manual
run with the default input only upload validation artifacts.
Manual runs have their own concurrency group, so a later branch push cannot cancel a publication.

The version comes from `project(OpenNOWQt VERSION ...)` and the workflow run identity:
`1.0.0-nightly.<run-number>.<run-attempt>`. The exact checked-out SHA is shared by every build and
recorded in `RELEASE-INFO.json`. Rerunning **all jobs** creates a distinct nightly version; rerunning
only failed jobs retains the identity from the original metadata job. A successful
publisher first uploads a draft, then makes it a prerelease without changing the latest stable
release. A failed upload can leave a draft; it does not expose an incomplete public release.
After a failed publication, rerun all jobs to produce a fresh candidate rather than overwriting
an existing tag or asset. Remove abandoned drafts separately if needed.

## Download formats

Each release contains nine packages with distinct version/platform/architecture filenames:

- `OpenNOW-Qt-<version>-Windows-x64.zip` and `...-Windows-arm64.zip` are unsigned portable builds.
  Extract the entire archive, then launch `bin/OpenNOW.exe`. Windows may display SmartScreen or
  unknown-publisher warnings.
- `OpenNOW-Qt-<version>-Windows-x64.msi` and `...-Windows-arm64.msi` install **OpenNOW Nightly**
  into a separate **OpenNOW Nightly** directory. They do not replace a stable OpenNOW installation.
  Newer runs and retries upgrade the nightly installation; older nightlies are rejected.
- `OpenNOW-Qt-<version>-Linux-x64.AppImage` and `...-Linux-arm64.AppImage` are the recommended
  portable Linux downloads. Make the downloaded file executable before starting it.
- `OpenNOW-Qt-<version>-Linux-x64.deb` and `...-Linux-arm64.deb` require distribution-provided
  Qt 6.8+ and SDL3. Stock Ubuntu 24.04 does not provide those versions; use the AppImage there.
  The internal Debian version uses `1.0.0~nightly.<run>.<attempt>` so a later stable `1.0.0`
  correctly supersedes it.
- `OpenNOW-Qt-<version>-Darwin-arm64.dmg` contains the Apple Silicon application for macOS 13+.
  Open the disk image and drag OpenNOW into Applications. There is no Intel or universal download.
  Gatekeeper can block this non-notarized application. After checking the release and download,
  use the explicit **Open Anyway** confirmation in **System Settings → Privacy & Security** if
  macOS offers it. Do not disable Gatekeeper globally. CI also retains a separate macOS ZIP for
  validation; that ZIP is not a public release asset.

`SHA256SUMS` covers the nine packages and `RELEASE-INFO.json`.
Checksums detect corruption; they do not replace a publisher signature. The inventory rejects
missing platforms, duplicate basenames, wrong versions, empty files, and unexpected assets
before any release upload.
AppImage smoke tests use the packaged offscreen plugin with host Qt plugin, QML, and library
search paths removed, so the installed CI toolkit cannot hide missing bundled dependencies.
macOS checks mount the actual DMG, copy the app out, detach the image, and smoke both that app
and the validation ZIP with development Qt, SDL3, and build directories hidden. Windows checks
extract MSI and ZIP payloads and compare all five first-party binaries against the deployment
copies. Native Windows x64 installer fixtures exercise run upgrades, retry upgrades, downgrade
rejection, and stable/nightly isolation. Windows ARM64 still requires runtime testing on hardware.

The MSI version is independent of the full application SemVer. For nightly run `R` and attempt
`A`, Windows Installer receives `floor(R / 256).(R % 256).A`. Both values must be in `1..65535`;
configuration fails instead of wrapping or truncating an exhausted counter. Retries order within
a run, and every new run orders after every attempt of the preceding run. A changed workflow that
resets its run numbers needs an explicit installer migration. Stable packages keep their existing
upgrade family and numeric version; supporter packages use a third family and directory.

## Updates and signed candidates

These nightlies deliberately have no pinned Ed25519 update-signing key and require manual
downloads. The reusable package workflow accepts an optional public key for a separately
configured signed-update release path, but `qt-ci` does not pass one or produce updater manifests.
The client never bypasses signature verification. The updater compares complete semantic
versions, so nightly runs order numerically and stable `1.0.0` sorts after its nightlies when
signed updates are configured. A stable MSI installs separately from the nightly MSI family
rather than replacing it.

Authenticode and Apple Developer ID signatures authenticate platform applications. Ed25519
manifests authenticate the exact updater payload bytes; they do not remove SmartScreen or
Gatekeeper warnings. Apple Silicon ad-hoc executable signatures are not Developer ID signatures
or notarization. Keep the Ed25519 private key on the isolated signer, never on platform build workers.

The separate [`qt-release-candidate`](qt-release-candidate.md) workflow remains a signed,
numeric-version production-candidate path. It still requires its documented certificates,
environments, and isolated signer. It is not the unsigned nightly publishing workflow.
