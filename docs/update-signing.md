# Qt update signing contract

Qt updates fail closed unless the application core was compiled with a pinned
Ed25519 public key. GitHub ownership, HTTPS and an asset checksum alone are not
treated as an update signature.

Public nightly publication requires the protected signing environment. See
[Activate nightly update signing](update-signing-setup.md) for the current activation
blocker and the first manual upgrade from a nightly without a pinned key.

## Release key boundary

- Keep the 32-byte Ed25519 private seed only in the protected `qt-update-signing` environment secret
  `OPENNOW_UPDATE_ED25519_PRIVATE_KEY`, encoded as base64. Only the isolated
  `opennow-release-signer` runner may receive it; platform build workers must not.
- Configure the matching 32-byte public key as base64 through CMake's
  `OPENNOW_UPDATE_ED25519_PUBLIC_KEY` cache variable. It is compiled into the
  Rust core and is safe to publish.
- A release build without that public key may discover releases, but reports
  `signaturePolicy: unconfigured-fail-closed` and cannot download an update.
- Rotating the key requires a normally signed application release containing
  the next public key. Do not fetch replacement trust keys from release assets.

## Manifest format

Each installable asset must have a sibling named
`<exact-asset-name>.manifest.json`:

```json
{
  "schemaVersion": 1,
  "version": "0.6.0",
  "asset": "OpenNOW-Qt-linux-x64.AppImage",
  "size": 12345678,
  "sha256": "64 lowercase hexadecimal characters",
  "signature": "base64 Ed25519 signature"
}
```

The signature covers this exact UTF-8 payload, including the final newline:

```text
OpenNOW update manifest v1
version=<version without leading v>
asset=<exact asset file name>
size=<decimal byte count>
sha256=<lowercase digest>
```

`qt-ci.yml` generates nightly manifests only after the shared checks, platform checks,
and complete build succeed. The signing job uses `qt-update-signing` on the isolated
`[self-hosted, opennow-release-signer]` runner. Its reviewed Python signing script and
the runner's OpenSSL tools read packages as data. The signer never compiles source,
extracts packages, or executes candidate binaries, including `opennow-update-manifest`.
The checkout is pinned to the workflow's immutable `github.sha`, with Git credentials
disabled. Environment approval covers that revision, including its signing scripts.

The public `qt-ci` input `public_key` must be canonical base64 encoding of exactly
32 bytes when `publish_nightly` is enabled. `qt-build` passes that same value through
its optional `update_public_key` input to both CMake configurations. Artifact-only
builds may omit the key and retain the core's unconfigured, fail-closed update policy.

`opennow-qt/packaging/sign_nightly_release.py` checks the unsigned inventory's source
commit, version, complete package names, sizes, and checksums before signing. It derives
the public key from the private seed and requires an exact match with the build's key.
Every generated signature is verified against its package before the final directory
becomes available. A separate publisher job verifies the complete signed inventory
again, without receiving the seed, before creating a draft prerelease. Only a complete
upload is made public.

For version `<version>`, the public nightly inventory contains exactly these packages:

- `OpenNOW-Qt-<version>-Windows-x64.msi`
- `OpenNOW-Qt-<version>-Windows-x64.zip`
- `OpenNOW-Qt-<version>-Windows-arm64.msi`
- `OpenNOW-Qt-<version>-Windows-arm64.zip`
- `OpenNOW-Qt-<version>-Linux-x64.AppImage`
- `OpenNOW-Qt-<version>-Linux-x64.deb`
- `OpenNOW-Qt-<version>-Linux-arm64.AppImage`
- `OpenNOW-Qt-<version>-Linux-arm64.deb`
- `OpenNOW-Qt-<version>-Darwin-arm64.dmg`

Each package has its exact sibling manifest. `RELEASE-INFO.json` retains the immutable
package inventory and changes `updates` from `manual-download` to `signed-manifest`.
`platformSigning` remains `unsigned`: update signatures do not provide Authenticode
or macOS notarization. Final `SHA256SUMS` covers all nine packages, all nine manifests,
and the rewritten release metadata. The validation-only macOS ZIP is never published.

The production `qt-release-candidate.yml` contract remains separate: ten Linux, Windows,
and macOS packages, platform signing on Windows and macOS, isolated update signing,
and a candidate artifact. See [Set up signed Qt releases](qt-release-signing-setup.md)
for production credentials and first-release instructions.
Nightly release notes use GitHub-generated changelogs. Installation guidance and known
limitations are documented in [`qt-nightly-release.md`](qt-nightly-release.md).

Nightly macOS packaging enables `OPENNOW_MACOS_ADHOC_SIGN`. Qt deployment signs nested
code with `macdeployqt -codesign=-`, then the final install script seals the complete
bundle with the target's stable `io.github.opencloudgaming.OpenNOW` identifier. This
explicit final seal avoids inheriting a linker's temporary Mach-O identifier. Packaging
fails if `codesign --verify --deep --strict` fails after deployment. Both relocated
DMG and validation-ZIP bundles must also pass strict verification and report the same
bundle identifier, `Signature=adhoc`, and `TeamIdentifier=not set` before any smoke test.
Ad-hoc sealing provides no publisher identity or notarization; release warnings remain
in place. This option is off by default and does not change production candidate signing.

The updater pins the repository, release-download URL, platform/architecture, asset
name, manifest signature, declared size, and SHA-256 digest before atomically staging
anything executable. It re-hashes the staged file immediately before install.
AppImage replacement keeps a `.previous` rollback copy and restores it if the updated
image cannot restart; native installers remain responsible for their platform rollback.

## Windows installation identity

`opennow-qt/cmake/WindowsInstaller.cmake` preserves the existing MSI UpgradeCodes:

- Stable: `6E81F7AE-B19D-4E87-A94A-2B2F01EBF762`
- Nightly: `9661F4F8-656C-4B64-9035-01B04F4822B1`
- Supporter: `B3AF8A40-5F44-445A-AD99-CECE00593601`

Each channel's code is shared by x64 and ARM64 for compatibility with installed
packages. It identifies an upgrade family, not an architecture. The helper must also
validate the incoming MSI's SummaryInformation architecture and match an installed
related product's registered root to the running installation. Portable ZIPs do not
create an MSI product registration. A product registered elsewhere does not make a
portable copy an MSI installation.

CPack's WiX generator sets `ARPINSTALLLOCATION` to the resolved `[INSTALL_ROOT]`
after `CostFinalize`. The installer policy leaves `CPACK_WIX_PROPERTY_ARPINSTALLLOCATION`
unset so CPack also restores the previous product's registered root through its secure
`INSTALL_ROOT` property. Do not add a second `SetARPINSTALLLOCATION` action in a product
patch. Windows Installer stores the resolved value as `InstallLocation`, including
custom installation paths.
CPack still generates a new ProductCode for each MSI package and uses its existing
`MajorUpgrade` behavior. The Windows installer integration test checks the registered
root and requires exactly one related product after each successful channel upgrade.

The Windows update helper builds separately in `update-helper-rust-target` with
`RUSTFLAGS=-C target-feature=+crt-static` and an explicit Rust target. That invocation
clears inherited encoded Rust flags without changing core or streamer builds. The
helper is deployed as the usual sibling `opennow-update-helper.exe`. Final Windows
MSI and ZIP validation rejects helper imports outside an explicit Windows system-DLL
list, so Qt libraries and a separately installed Visual C++ runtime cannot mask a
dependency that would break the helper when it runs outside the installation.
