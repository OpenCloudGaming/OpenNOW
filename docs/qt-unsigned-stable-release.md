# Publish an update-signed stable release

The manual `Qt update-signed stable release` workflow promotes a successful production
candidate for the numeric version in `opennow-qt/CMakeLists.txt` from an exact commit on
`main`. It never rebuilds or re-signs packages. Build the candidate first with
`qt-release-candidate`, the existing production update key, and the production Apple
Developer ID identity. Windows Authenticode remains optional. Keep both protected
signing environments configured as described in [release signing setup](qt-release-signing-setup.md).

Dispatch `.github/workflows/qt-stable-release.yml` on `main` with `source_commit`
set to its reviewed 40-character commit SHA and `candidate_run_id` set to the successful
`qt-release-candidate` run ID. The workflow rejects other refs and commit mismatches.
It retains shared contracts and Linux, Windows, and macOS tests. Promotion requires
approval through `qt-production-release`, validates the candidate run's repository,
workflow path, event, completed-success status and exact source SHA, then downloads
only that run's named complete-candidate artifact.

The release contains ten packages: Windows x64/ARM64 MSI and portable ZIP,
Linux x64/ARM64 AppImage and DEB, and macOS Apple Silicon DMG and ZIP. It also includes two
AppImage `.zsync` sidecars, twelve exact sibling Ed25519 manifests, `RELEASE-INFO.json`,
and `SHA256SUMS` (26 files). The candidate's isolated signer signs the complete inventory.
The publisher independently verifies every signature, candidate checksum, filename,
version and source identity, then creates flattened checksums and metadata without
changing any package or manifest bytes. It uploads a draft
release, and only then marks it public and latest.
An existing tag/release is not overwritten; a failed upload can leave a draft
that must be inspected before retrying.

Every candidate build embeds the existing production key from
`opennow-qt/packaging/update-public-key.base64`, also used by signed v1.0.1.
Missing or mismatched signing credentials fail the candidate rather than falling back
to unsigned updates. v1.0.0 and other builds without a pinned key require one manual
installation of an update-enabled release; trust cannot be bootstrapped from downloaded
metadata. Later updates remain signature-verified.

Update signatures are separate from platform signatures. macOS candidates preserve
Developer ID signing and notarization, allowing stable-to-stable updates to pass the
helper's unchanged signing-identity check. `RELEASE-INFO.json` records
`platformSigning=macos-developer-id` and the candidate's `windowsSigningMode`; unsigned
Windows packages can trigger SmartScreen. Checksums alone only detect corruption.
AppImages embed the stable `latest` GitHub update selector; see
[AppImage delta updates](qt-nightly-release.md#appimage-delta-updates) for external-tool
behavior and its separate trust boundary. CI does not replace the live hardware checks
in [Qt acceptance](qt-acceptance.md).

For Debian bootstrap installation, use `sudo apt install ./OpenNOW-Qt-<version>-Linux-<arch>.deb`
to resolve dependencies including `pkexec` for graphical update authorization. Users of
older packages without that dependency can install it with `sudo apt install pkexec`.
Do not bypass authorization or signature verification.
