# OpenNOW H.264 DPB patch

This directory vendors `cros-codecs-extended` 0.0.5-extended.2 from crates.io,
with its BSD-3-Clause license and copyright headers retained. The release archive
checksum is `a5130a93260541c9418be3a57af40916c662e16d02f25727d8f7ad0738ebde1f`.
Its source revision is `ca09c57e3c909f4c9a7b5bbd4879452a45053050` in
<https://github.com/chojs23/cros-codecs-extended>.

The copied `Cargo.toml`, `LICENSE`, `README.md`, `rustfmt.toml`, `examples/`, and
`src/` match the release, except for `src/codec/h264/dpb.rs` and
`src/decoder/stateless/h264.rs`. Cargo registry bookkeeping and unrelated build
metadata are not vendored. OpenNOW adds this note and a build-artifact ignore file.

## Local changes

The progressive-frame bumping change follows upstream
[PR #2](https://github.com/chojs23/cros-codecs-extended/pull/2), revision
`ade18c441512f79974f4c6918a0c349ca11102a0`. That PR was still open, and
0.0.5-extended.2 was the latest published version, when checked on 2026-10-12.
Replace this patch with a released dependency once it covers the regression tests.

After storing a submitted picture, the decoder outputs the lowest-POC pictures
while the number still needed for output exceeds the SPS reorder bound. Output
pictures remain in the DPB while used as short-term or long-term references.
Non-existing gap pictures and already-output references do not count toward the
reorder delay. Interlaced streams keep the existing field-pair bumping path.

Three safety changes accompany the upstream logic:

- DPB clear and drain preserve all stream limits. The original clear operation
  reset the reorder bound to zero, so the upstream patch alone outputs reordered
  streams too early after IDR, MMCO5, or flush.
- A declared reorder bound larger than DPB capacity falls back to that capacity,
  not zero. An invalid restriction must not enable immediate output.
- Capacity bumping first reclaims pictures already output and no longer used as
  references. Adaptive reference marking can otherwise leave a full DPB with no
  bumpable picture after early output, causing the next storage operation to fail.

The existing one-picture lookahead for identifying slice boundaries is unchanged.
No OpenNOW VA-API submission, Annex-B parsing, surface ownership, or device code
is changed.

## Verification

Run the dependency's parser and DPB unit tests without backend features. Its
upstream backend unit-test helpers do not implement the current backend traits;
OpenNOW does not repair those unrelated helpers in this patch.

```sh
CARGO_TARGET_DIR="$HOME/.capy/work/opennow-cros-tests" \
  cargo test --manifest-path native/opennow-streamer/vendor/cros-codecs-extended/Cargo.toml --lib
cargo test --manifest-path native/opennow-streamer/Cargo.toml \
  -p opennow-streamer-platform-linux --no-default-features --features vaapi \
  --test h264_dpb_latency
cargo test --manifest-path native/opennow-streamer/Cargo.toml \
  -p opennow-streamer-platform-linux --no-default-features --features vaapi
cargo test --manifest-path native/opennow-streamer/Cargo.toml --workspace
```

The Linux integration tests exercise the actual H.264 parser, decoder completion,
DPB, and event queue with a deterministic backend. They verify immediate output
for zero-reorder progressive pictures, retained reference handles, reordered
B pictures, sustained sliding-window and adaptive reference marking across frame
number and POC wraps, IDR discard, MMCO5, flush/restart, format renegotiation,
inferred and invalid reorder restrictions, and an upstream interlaced fixture. They do not
decode pixels or measure live GPU latency. The unit tests also cover long-term
references, gap pictures, non-reference cleanup, and incomplete field pairs.

## Initial VAContext size

`VaapiBackend::new` created its placeholder VAContext at 16x16. Mesa radeonsi
(AMD VCN, e.g. Renoir/Cezanne) rejects H.264 decode contexts below 64x64 with
`VA_STATUS_ERROR_RESOLUTION_NOT_SUPPORTED`, so the constructor panicked and
OpenNOW's capability probe reported VA-API as unavailable on those GPUs. The
placeholder is replaced by `new_sequence` once the SPS arrives, so its size is
arbitrary; it is now `INITIAL_CONTEXT_SIZE` (64), which OpenNOW's probe shares.
