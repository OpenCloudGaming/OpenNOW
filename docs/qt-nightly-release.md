# Qt nightly releases

Use `qt-ci` on `dev` for the current Qt/native application. Nightly applications have no Windows
Authenticode certificate or Apple Developer ID signature and are not notarized. This workflow
supports no-key artifact-only builds, but public nightlies require a pinned Ed25519 public key
and manifests signed by the protected isolated signer. Configure the environment, runner, and
key described in [`update-signing-setup.md`](update-signing-setup.md) before publishing.

**Known issue: Alliance Partners are not working correctly in this build.**

## Build and publish

After the release changes are merged into `dev`, run:

```sh
gh workflow run qt-ci --repo OpenCloudGaming/OpenNOW --ref dev -f publish_nightly=false
```

Leave `public_key` empty for this no-key artifact-only path. It requires no signing environment,
does not create updater manifests, and cannot download updates in the application.
This builds all five targets without publishing anything. Download the five unsigned
artifact groups from that workflow run and perform the relevant checks in
[`qt-acceptance.md`](qt-acceptance.md), especially Windows ARM64, which is cross-compiled and
cannot run its application tests on the x64 CI worker.

To publish, set `OPENNOW_UPDATE_PUBLIC_KEY` to the matching canonical base64-encoded 32-byte
public key from the signing setup. This is the public key, never the private seed. Then run:

```sh
gh workflow run qt-ci --repo OpenCloudGaming/OpenNOW --ref dev \
  -f publish_nightly=true -f public_key="$OPENNOW_UPDATE_PUBLIC_KEY"
```

An empty or malformed key fails preflight. After checks and package builds pass, approve the
exact source/workflow revision in the protected `qt-update-signing` environment. Its isolated
signer verifies the complete nine-package inventory, signs each package's sibling manifest,
and uploads `opennow-qt-<version>-complete-update-signed`. The publisher independently verifies
that finalized set before uploading it. Missing signing configuration blocks publication; it
does not fall back to an unsigned public update.

Use the registered `qt-ci` workflow name and an explicit `--ref dev`. The Actions web UI may not
show the dispatch control until this workflow is also present on the default branch. Do not use
the legacy `release` workflow instead. Publishing is opt-in: pushes and pull requests run checks,
and a manual run with the default inputs uploads validation artifacts without publishing.
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
- `OpenNOW-Qt-<version>-Linux-x64.deb` and `...-Linux-arm64.deb` bundle the AppImage's deployed
  Qt, QML plugins, SDL3, and media runtime privately under `/opt/opennow`. Ubuntu 24.04 and
  Linux Mint 22.x do not need a Qt upgrade or third-party repositories. Install the downloaded
  package with `sudo apt install ./OpenNOW-Qt-<version>-Linux-x64.deb` so APT installs the
  remaining system dependencies. The desktop entry and `opennow-qt` command use that private runtime.
  The internal Debian version uses `1.0.0~nightly.<run>.<attempt>` so a later stable `1.0.0`
  correctly supersedes it.
- `OpenNOW-Qt-<version>-Darwin-arm64.dmg` contains the Apple Silicon application for macOS 13+.
  Open the disk image and drag OpenNOW into Applications. There is no Intel or universal download.
  Gatekeeper can block this non-notarized application. After checking the release and download,
  use the explicit **Open Anyway** confirmation in **System Settings → Privacy & Security** if
  macOS offers it. Do not disable Gatekeeper globally. CI also retains a separate macOS ZIP for
  validation; that ZIP is not a public release asset.

The public signed set contains 20 files: nine packages, nine `<package>.manifest.json` siblings,
`RELEASE-INFO.json`, and `SHA256SUMS`. Its checksums cover all 19 other files. The no-key
artifact-only inventory has no manifests; its checksums cover the packages and release metadata.
Checksums detect corruption; they do not replace a publisher signature. The inventory rejects
missing platforms, duplicate basenames, wrong versions, empty files, and unexpected assets
before any release upload.
AppImage smoke tests use the packaged offscreen plugin with host Qt plugin, QML, and library
search paths removed, so the installed CI toolkit cannot hide missing bundled dependencies.
Release DEBs are assembled from that same deployed AppDir, then installed in a clean Ubuntu 24.04
container on each native architecture. Checks reject distribution Qt/SDL3 dependencies, check
library resolution before adding test tools, run offscreen and X11 smoke tests and native capability
probes, and exercise reinstall/removal. Run `bash opennow-qt/packaging/verify_bundled_deb.sh <package.deb>`
to repeat these checks locally with Docker. Direct developer CPack builds without the
`LinuxBundledDeb.cmake` project configuration still require distribution Qt 6.8+ and SDL3.
macOS checks mount the actual DMG, copy the app out, detach the image, and smoke both that app
and the validation ZIP with development Qt, SDL3, and build directories hidden. Windows checks
extract MSI and ZIP payloads and compare every binary in the
[Windows release list](../opennow-qt/packaging/windows-release-binaries.txt) against the deployment
copies, including the update helper. Native Windows x64 installer fixtures exercise run upgrades,
retry upgrades, downgrade rejection, and stable/nightly isolation. Windows ARM64 still requires
runtime testing on hardware.

The MSI version is independent of the full application SemVer. For nightly run `R` and attempt
`A`, Windows Installer receives `floor(R / 256).(R % 256).A`. Both values must be in `1..65535`;
configuration fails instead of wrapping or truncating an exhausted counter. Retries order within
a run, and every new run orders after every attempt of the preceding run. A changed workflow that
resets its run numbers needs an explicit installer migration. Stable packages keep their existing
upgrade family and numeric version; supporter packages use a third family and directory.

## Updates and signed candidates

Public nightlies embed the supplied Ed25519 public key in both the core and apply helper.
Their manifests authenticate each exact package before download completion and again before
installation. Installation requires confirmation and no active or recovering streaming session;
the helper applies the replacement and waits for the restarted Qt application and core to
acknowledge startup. Automatic downloading is a separate opt-in and never authorizes shutdown.

An earlier nightly without a pinned key requires **one manual upgrade** to an update-enabled
build. It cannot securely learn a trust key from release metadata. Artifact-only builds still
default to that no-key, manual-update behavior. The client never bypasses signature verification.
See the [signing setup and bootstrap instructions](update-signing-setup.md) before the first
public update-enabled release.

The updater compares complete semantic versions, so nightly runs order numerically and stable
`1.0.0` sorts after its nightlies. A stable MSI installs separately from the nightly MSI family
rather than replacing it. Portable Windows replacement requires persistent ACL support;
FAT/exFAT installations are refused before shutdown and require manual updating.

Authenticode and Apple Developer ID signatures authenticate platform applications. Ed25519
manifests authenticate the exact updater payload bytes; they do not remove SmartScreen or
Gatekeeper warnings. Apple Silicon ad-hoc executable signatures are not Developer ID signatures
or notarization. Keep the Ed25519 private key on the isolated signer, never on platform build workers.

The separate [`qt-release-candidate`](qt-release-candidate.md) workflow remains a signed,
numeric-version production-candidate path. It still requires its documented certificates,
environments, and isolated signer. It remains separate from nightly update-manifest signing.
