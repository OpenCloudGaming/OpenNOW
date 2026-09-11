# Publish an unsigned stable release

The manual `Qt unsigned stable release` workflow builds the numeric version in
`opennow-qt/CMakeLists.txt` from an exact commit on `main`. This is separate from
signed production candidates and update-signed nightlies. It requires no signing
keys, certificates, or protected signing runner.

Dispatch `.github/workflows/qt-stable-release.yml` on `main` with `source_commit`
set to its reviewed 40-character commit SHA. The workflow rejects other refs and
commit mismatches. It runs shared contracts and Linux, Windows, and macOS tests,
then publishes only after every check and all five package builds pass.

The release contains nine packages: Windows x64/ARM64 MSI and portable ZIP,
Linux x64/ARM64 AppImage and DEB, and a macOS Apple Silicon DMG. It also includes
`RELEASE-INFO.json` and `SHA256SUMS`. Publication verifies the full inventory and
checksums, uploads a draft release, and only then marks it public and latest.
An existing tag/release is not overwritten; a failed upload can leave a draft
that must be inspected before retrying.

Packages have no publisher signatures or pinned update key. Windows may show
SmartScreen warnings, and the macOS app is not notarized. Updates require manual
download and installation; runtime signature verification remains fail-closed.
Checksums detect corruption, not publisher identity. CI does not replace the
live hardware checks in [Qt acceptance](qt-acceptance.md).
