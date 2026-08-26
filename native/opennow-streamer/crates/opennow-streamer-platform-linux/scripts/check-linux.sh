#!/usr/bin/env sh
set -eu

cargo check -p opennow-streamer-platform-linux --all-targets
cargo check -p opennow-streamer-platform-linux --all-targets --all-features
cargo test -p opennow-streamer-platform-linux --all-features
cargo clippy -p opennow-streamer-platform-linux --all-targets --all-features -- -D warnings

if rustup target list --installed | grep -qx aarch64-unknown-linux-gnu; then
  cargo check -p opennow-streamer-platform-linux --target aarch64-unknown-linux-gnu --no-default-features
  cargo check -p opennow-streamer-platform-linux --target aarch64-unknown-linux-gnu --features vulkan
fi
