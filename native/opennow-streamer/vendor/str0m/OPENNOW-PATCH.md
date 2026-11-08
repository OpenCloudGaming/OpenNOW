# OpenNOW str0m patch

This directory is the published `str0m` 0.23.0 crate with one upstream fix backported.
It replaces the crates.io package through `[patch.crates-io]` in
`native/opennow-streamer/Cargo.toml`.

## Provenance

- Source: `str0m-0.23.0.crate` from crates.io, SHA-256
  `840d11fdd44459d71bc7b3dd6796f55e9ac86c8e3fa8678098a8642f5701c5ff` (the checksum the
  workspace lockfile recorded before this patch). Upstream commit for that release:
  `67820c52ff58df5350659063129e94d7d87225e4` (see `.cargo_vcs_info.json`).
- License: `MIT OR Apache-2.0`, unchanged. `LICENSE-MIT.txt` is the crate's own file.
- Removed from the published crate because they are not built as a dependency: `tests/`,
  `examples/`, `docs/`, `logo/`, editor/CI configuration, the crate's own `Cargo.lock`, and the
  `[[test]]`/`[[example]]` targets that referenced them in `Cargo.toml`. `Cargo.toml.orig` is
  the unmodified upstream manifest.

## Change

`src/packet/buffer_rx.rs` backports the depacketizer cache invalidation and its
`late_packet_in_front_of_waiting_frame` unit test from upstream commit
`67c6b4e7ccb25a989b65c835b575d6ecf881f84a` (first released in str0m 0.24.0).

In 0.23.0, a frame waiting behind a sequence gap is depacketized ahead and cached under its
queue-index range. A late or retransmitted packet inserted in front of it shifts the queue
without invalidating that cache, so the repaired frame is emitted with the following frame's
payload and timestamp and is reported as contiguous. No other upstream change is included.

## Verification

- `late_packet_in_front_of_waiting_frame` fails on unpatched 0.23.0
  (`Time not matching 4 != 3`) and passes with the patch.
- `opennow-streamer-transport`'s
  `repairable_delta_loss_waits_for_retransmission_and_stays_contiguous` holds back one
  original SRTP datagram, re-injects it, and requires all 20 frames with their sender
  timestamps and payloads.

## Removal

Delete this directory and its `[patch.crates-io]` entry when the workspace moves to
str0m 0.24.0 or later, which contains this fix. Keep the transport regression test.
