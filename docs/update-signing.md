# Qt update signing contract

Qt updates fail closed unless the application core was compiled with a pinned
Ed25519 public key. GitHub ownership, HTTPS and an asset checksum alone are not
treated as an update signature.

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

Generate a manifest on the isolated release runner after packaging and platform
code signing:

```sh
OPENNOW_UPDATE_ED25519_PRIVATE_KEY="$RELEASE_SECRET" \
OPENNOW_UPDATE_ED25519_PUBLIC_KEY="$PINNED_PUBLIC_KEY" \
  cargo run --manifest-path native/opennow-core/Cargo.toml \
  --bin opennow-update-manifest -- \
  build/OpenNOW-Qt-linux-x64.AppImage 0.6.0
```

The generator refuses to write a manifest unless the supplied public key matches the private seed
and the freshly generated signature verifies with that public key. Use the same public-key value
for the CMake `OPENNOW_UPDATE_ED25519_PUBLIC_KEY` option and manifest generation; this prevents a
signed release whose client cannot verify its own update feed.

Publish the package and its manifest together. The updater pins the repository,
release-download URL, platform/architecture, asset name, manifest signature,
declared size and SHA-256 digest before atomically staging anything executable.
It re-hashes the staged file immediately before install. AppImage replacement
keeps a `.previous` rollback copy and restores it if the updated image cannot
restart; native installers remain responsible for their platform rollback.
