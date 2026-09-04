# Qt production release candidates

The `qt-release-candidate` workflow builds one immutable Qt/Rust source commit for all seven
release architectures/window families. It does not publish a GitHub release or replace Electron;
it produces protected candidate artifacts that must still pass the live matrix and staged rollout.

## Protected environment

Create a GitHub environment named `qt-production-release`, require reviewer approval, and define
the platform-signing secrets below. Create a second environment named `qt-update-signing` containing
only `OPENNOW_UPDATE_ED25519_PRIVATE_KEY`; restrict it to a dedicated self-hosted runner carrying the
`opennow-release-signer` label.

| Secret | Purpose |
| --- | --- |
| `OPENNOW_UPDATE_ED25519_PRIVATE_KEY` | Base64 32-byte offline update-signing seed (`qt-update-signing` only) |
| `OPENNOW_WINDOWS_SIGNING_PFX_BASE64` | Base64 Authenticode certificate and private key |
| `OPENNOW_WINDOWS_SIGNING_PFX_PASSWORD` | PFX password |
| `OPENNOW_MACOS_DEVELOPER_ID_P12_BASE64` | Base64 Developer ID Application certificate |
| `OPENNOW_MACOS_DEVELOPER_ID_P12_PASSWORD` | P12 password |
| `OPENNOW_MACOS_SIGN_IDENTITY` | Exact Developer ID Application identity |
| `OPENNOW_APPLE_API_KEY_BASE64` | Base64 App Store Connect notarization `.p8` key |
| `OPENNOW_APPLE_API_KEY_ID` | Notarization API key ID |
| `OPENNOW_APPLE_API_ISSUER_ID` | Notarization issuer ID |

The matching Ed25519 public key is a workflow input, not a secret. The workflow embeds that exact
value into every core. Ordinary Linux, Windows and macOS build workers never receive the update
private seed. After their platform-signed artifacts are uploaded, the isolated signer downloads
them without executing any candidate program, derives the Ed25519 public key from the protected
seed, compares it byte-for-byte with the embedded public-key input, signs the canonical payload with
OpenSSL and verifies every signature before producing a manifest.

The signing runner requires Bash, OpenSSL with Ed25519 `pkeyutl` support, `jq`, GNU coreutils,
and the GitHub Actions runner. It should have no general development credentials and should be
ephemeral or reset after each approved release operation.

## Candidate guarantees

- A numeric version such as `0.6.0` is embedded consistently in Qt, Rust, package metadata,
  diagnostics, telemetry and updater selection.
- Windows x64/ARM64 application executables and MSI installers are timestamped with Authenticode;
  the portable ZIPs are unpacked and every executable is verified again.
- macOS Intel/Apple-Silicon apps use hardened-runtime Developer ID signing. The app is notarized and
  stapled before final ZIP/DMG creation; the DMG is then signed, notarized, stapled and assessed.
- Linux x64/ARM64 DEB and checksum-pinned AppImage builds use native runners.
- Every installable artifact receives a sibling Ed25519 update manifest after platform signing.
- The inventory job fails unless it finds both Windows MSI/ZIP pairs, both macOS DMG/ZIP pairs, both
  Linux AppImage/DEB pairs and exactly one manifest per artifact. It records the immutable commit and
  SHA-256 of every candidate file.

Run the workflow manually with an exact reviewed 40-character source commit, version, and public
key. The workflow rejects mutable branch names and confirms checkout identity before any build.
Download the complete
candidate artifact, retain it under the release-candidate identifier, and execute
[`qt-acceptance.md`](qt-acceptance.md). Only a verifier pass for every required hardware row plus the
defined staged-rollout observation window authorizes promotion and subsequent Electron removal.
