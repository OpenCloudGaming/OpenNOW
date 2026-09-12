# Qt production release candidates

For certificate-free Windows/Linux nightlies, use the manual `qt-ci` publishing option described
in [`qt-nightly-release.md`](qt-nightly-release.md). This signed candidate workflow is separate.

The `qt-release-candidate` workflow builds one immutable Qt/Rust source commit for Windows and
Linux, each on x64 and ARM64, and macOS on ARM64. The workflow does not publish a GitHub release;
it produces protected candidate artifacts that must still pass the live matrix and staged rollout.

For credential provisioning and the first release, follow
[Set up signed Qt releases](qt-release-signing-setup.md).

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

The Windows workflow currently uses exportable PFX credentials. New publicly trusted
certificates normally use non-exportable hardware-backed keys. If your provider uses a
token, HSM, or cloud signing service, integrate its signing client before dispatching.
The PFX inputs are not a way to export a hardware-protected private key.

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

- A numeric version such as `1.0.0` is embedded consistently in Qt, Rust, package metadata,
  diagnostics, telemetry and updater selection.
- Windows x64/ARM64 binaries listed in
  `opennow-qt/packaging/windows-release-binaries.txt` and MSI installers are timestamped with
  Authenticode. The list includes the core's required standalone capability probe and the embedded
  streamer DLL. CPack installs the signed deployment copies; extracted MSI and ZIP payloads must
  pass signature verification and match those copies byte-for-byte. Every nonzero `signtool` exit
  fails the workflow.
- macOS ARM64 candidates use Developer ID signing, notarization, and stapled tickets.
  The final DMG and ZIP contain the signed, stapled application. Intel Mac candidates
  are not included. The application retains only the hardened-runtime exceptions
  needed for QML JIT compilation and microphone input. Library validation stays enabled.
- Linux x64/ARM64 DEB and checksum-pinned AppImage builds use native runners. Both the probe and
  embedded streamer enable `linux-vaapi` and `linux-ffmpeg-bundled`. Native VAAPI supports H.264
  only; HEVC/AV1 remain available through other backends, including bundled FFmpeg software decode.
  Builds require libva/libva-drm development headers and libclang for generated bindings. Release
  DEBs reuse the deployed AppImage runtime, including Qt, SDL3, libva, and libva-drm, under
  `/opt/opennow`; usable GPU hardware and a host VAAPI driver are still required for hardware decode.
  Clean Ubuntu 24.04 container checks install the DEB without distribution Qt/SDL3, verify
  offscreen/X11 startup and capabilities, and exercise reinstall/removal without a GPU.
- Every installable artifact receives a sibling Ed25519 update manifest after platform signing.
- The inventory job fails unless it finds both Windows MSI/ZIP pairs, both Linux AppImage/DEB pairs,
  the macOS ARM64 DMG/ZIP pair, and exactly one manifest per artifact (ten artifacts total). It records the immutable commit and
  SHA-256 of every candidate file.

Run the workflow manually with an exact reviewed 40-character source commit, version, and public
key. The workflow rejects mutable branch names and confirms checkout identity before any build.
Download the complete
candidate artifact, retain it under the release-candidate identifier, and execute
[`qt-acceptance.md`](qt-acceptance.md). Only a verifier pass for every required hardware row plus the
defined staged-rollout observation window authorizes promotion. The Electron source and legacy
release workflows have already been removed; this does not waive candidate acceptance.
