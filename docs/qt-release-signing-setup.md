# Set up signed Qt releases

Use this guide for `OpenCloudGaming/OpenNOW`, the Qt desktop app. It does not configure
the separate `OpenNOW-Mac` Swift app. Configure the release credentials before running
`qt-release-candidate`. The workflow produces candidates, not a public GitHub release.

## Choose the signing identities

1. Keep one Ed25519 update-signing key for all supported platforms. OpenNOW pins its
   public key in each build and rejects packages without a valid sibling manifest.
2. For macOS, use an Apple Developer Program membership and a Developer ID Application
   certificate. Use the same Apple team for later releases. Do not change the Qt bundle
   identifier, `io.github.opencloudgaming.OpenNOW`.
3. For Windows, choose a publicly trusted Authenticode signing provider before buying
   a certificate. The current candidate workflow accepts a PFX with its private key.
   Newly issued public code-signing certificates normally require non-exportable keys
   in a hardware token, HSM, or cloud signing service. Those services need a provider
   integration in the workflow; do not try to export a protected private key into a PFX.
4. For Linux, use the Ed25519 manifests for OpenNOW's built-in updater. No paid platform
   certificate is required. This does not create an APT repository or sign its metadata.

For a new Windows setup, consider [Azure Artifact Signing][windows-signing] if your
publisher is eligible. Microsoft's current eligibility includes organizations in the
US, Canada, the EU, and the UK, and individuals in the US and Canada. Otherwise, choose
a CA with a CI-compatible hardware-backed signing service. Confirm eligibility and
integration requirements with the provider before purchase. Neither option guarantees
that a new release immediately avoids SmartScreen reputation warnings.

## Create the protected GitHub environments

1. In this repository, open **Settings → Environments**.
2. Create `qt-production-release` for the platform certificates and notarization key.
3. Create `qt-update-signing` for the update-signing seed only.
4. Require reviewers, prevent self-review, and restrict both environments to approved
   protected release refs. Disable administrator bypass where the repository plan permits it.
5. Configure the isolated `opennow-release-signer` runner as described in
   [Activate nightly update signing](update-signing-setup.md#configure-the-protected-signer).
   It must never execute downloaded packages or run pull-request builds.

Refer to [the candidate secret table](qt-release-candidate.md#protected-environment)
for the exact secret names. Store private values in environment secrets, not workflow
inputs, repository files, build logs, or chat. Base64 encoding does not encrypt a key.
The Apple secrets used by `OpenNOW-Mac` have different names and are not automatically
available to this repository.

## Generate the update key once

Use OpenSSL 3 on a trusted machine outside CI. Do not run this inside the checkout.
Keep an encrypted offline backup before adding the seed to GitHub.

```sh
set -euo pipefail
umask 077
mkdir "$HOME/opennow-update-key"
cd "$HOME/opennow-update-key"
openssl genpkey -algorithm ED25519 -out private.pem
openssl pkey -in private.pem -outform DER -out private.der
openssl pkey -in private.pem -pubout -outform DER -out public.der
test "$(wc -c < private.der)" -eq 48
test "$(wc -c < public.der)" -eq 44
tail -c 32 private.der | openssl base64 -A > private-seed.base64
tail -c 32 public.der | openssl base64 -A > public-key.base64
```

Add the contents of `private-seed.base64` as `OPENNOW_UPDATE_ED25519_PRIVATE_KEY` in
`qt-update-signing`. Use the contents of `public-key.base64` as the candidate workflow's
`update_public_key` input. The public key is safe to share. The private seed is not.
Do not generate a new key for each release. Follow the rotation rules in
[the update-signing contract](update-signing.md#release-key-boundary).

## Configure Apple signing and notarization

1. On your Mac, open **Keychain Access → Certificate Assistant → Request a Certificate
   From a Certificate Authority** and save a certificate signing request.
2. As your Apple team's Account Holder, open **Apple Developer → Certificates,
   Identifiers & Profiles → Certificates**. Create a **Developer ID Application**
   certificate using that request, download it, and install it on the same Mac.
3. In **Keychain Access → My Certificates**, export the certificate and its private key
   as a password-protected `.p12`. A `.cer` without the private key cannot sign builds.
4. Add its base64 encoding as `OPENNOW_MACOS_DEVELOPER_ID_P12_BASE64` and the export
   password as `OPENNOW_MACOS_DEVELOPER_ID_P12_PASSWORD` in `qt-production-release`.
5. Run `security find-identity -v -p codesigning` on that Mac. Add the exact
   `Developer ID Application: …` identity as `OPENNOW_MACOS_SIGN_IDENTITY`.
6. In **App Store Connect → Users and Access → Integrations → App Store Connect API →
   Team Keys**, create a key authorized for notarization. Download its `.p8` private key
   and retain an encrypted backup. Apple allows only one download of the private key.
7. Add the base64-encoded `.p8` as `OPENNOW_APPLE_API_KEY_BASE64`. Add its Key ID as
   `OPENNOW_APPLE_API_KEY_ID` and Issuer ID as `OPENNOW_APPLE_API_ISSUER_ID`.

On macOS, these commands create base64 files without printing their contents:

```sh
umask 077
openssl base64 -A -in DeveloperID.p12 -out DeveloperID.p12.base64
openssl base64 -A -in AuthKey.p8 -out AuthKey.p8.base64
```

Use a Developer ID Application certificate, not an Apple Development or Mac App Store
distribution certificate. The current DMG packaging does not need a Developer ID Installer
certificate. See Apple's [certificate instructions][apple-certificates] and
[notarization workflow][apple-notarization].

## Configure Windows signing

If you already have an appropriate exportable signing identity supported by the current
workflow, add its base64 PFX as `OPENNOW_WINDOWS_SIGNING_PFX_BASE64` and its password as
`OPENNOW_WINDOWS_SIGNING_PFX_PASSWORD` in `qt-production-release`.

If your provider uses a hardware token, HSM, or cloud signing account, stop before dispatching
the production workflow. Integrate that provider's supported signing command and authentication
first. Preserve the existing binary allowlist, timestamping, signature verification, and
extracted MSI and ZIP checks. A self-signed certificate can test the mechanics but does not
provide a publicly trusted Windows publisher identity.

## Build and promote the first signed release

1. Select a reviewed protected commit containing the release and icon changes. Choose a
   numeric version newer than the installed stable version, for example `1.0.1` after `1.0.0`.
2. Open **Actions → qt-release-candidate → Run workflow**. Select a ref pointing to that
   exact commit. Enter the version, full 40-character `source_commit`, and public update key.
3. Approve the platform signing environment only after reviewing the dispatched revision.
4. After the platform jobs pass, inspect the full package set before approving
   `qt-update-signing`. The signer checks that the private seed matches the embedded public key.
5. Download the complete candidate artifact. Run the hardware acceptance and staged-rollout
   checks in [Qt production release candidates](qt-release-candidate.md).
6. Only after acceptance and release approval, create a draft release in this repository
   with tag `v<version>` at the reviewed commit. Upload every candidate package and its exact
   `.manifest.json` sibling. Keep their filenames and bytes unchanged.
7. Verify the complete upload before making the stable release public. Do not publish a
   partial set or replace an existing version's packages. Test updating between two signed
   versions on each supported platform before declaring automatic updates ready.

The updater reads GitHub Releases from `OpenCloudGaming/OpenNOW`. Stable users receive
stable releases. Linux DEB and Windows MSI installation can require administrator approval.
OpenNOW checks automatically when enabled, can download automatically when opted in, and
requires confirmation before applying an update and restarting. It does not silently quit
an active stream to install an update.

The published Qt `1.0.0` release is unsigned and manual-update-only. Users of a build without
the pinned public key must manually install the first update-enabled release once. Later
releases can use the built-in updater. Never bypass signature verification to bootstrap trust.

[windows-signing]: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options
[apple-certificates]: https://developer.apple.com/help/account/certificates/create-developer-id-certificates
[apple-notarization]: https://developer.apple.com/documentation/security/customizing-the-notarization-workflow
