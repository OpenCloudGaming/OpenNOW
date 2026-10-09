# Example catalog plugin

This self-contained Rust executable demonstrates OpenNOW's catalog-only plugin protocol 1. It returns 24 clearly labeled illustrative titles with search and paging. It does not authenticate, connect to a cloud service, or make any entry playable.

Build for the same OS and architecture as OpenNOW:

```sh
cargo build --locked --release --manifest-path examples/catalog-plugin/Cargo.toml
examples/catalog-plugin/target/release/opennow-example-catalog --package /absolute/path/example.opennow-plugin
```

On Windows use `opennow-example-catalog.exe`. If you set `CARGO_TARGET_DIR`, use that target directory instead. `--package` creates a ZIP containing the running native executable, a versioned manifest and SHA-256 file inventory. No Python, shell script or installed interpreter is needed by the plugin.

In OpenNOW, open **Settings → Plugins → Install from file**, choose the package, review the identity and digest, and accept the native-code warning only if you trust these bytes. Installation leaves the plugin disabled. Enable it, then preview its catalog. The publisher name is self-declared and the package is unsigned; hashes do not establish authenticity.

The executable runs as your user, without a security sandbox. It can access files, credentials available to your user and the network. The host provides only a scrubbed environment and an `OPENNOW_PLUGIN_DATA_DIR` directory for its own state; this example writes its last process ID there. Disabling stops its process. Uninstall removes both its package and private data. Updates/replacement are not supported in v1: uninstall first.

The public Rust types come from `native/opennow-plugin-api`, not from the application core. The wire is private UTF-8 newline-delimited JSON. Stdout is exclusively protocol output; hello binds the plugin ID, version and `catalog.v1` capability. Catalog replies contain local IDs and plain titles only; the host supplies source identity. No child-to-host RPC, session descriptors, scripts, QML, HTML, URLs or native media commands are accepted.
