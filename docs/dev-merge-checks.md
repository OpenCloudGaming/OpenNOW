# PR 776: merge-check compatibility fixes

This is a historical record. The referenced Electron source has since been removed;
the Rust transport and Qt build/test guidance below remain relevant.

## Security alert 55: no crypto change

The reported `js/weak-cryptographic-algorithm` sink is
`opennow-stable/src/main/platforms/gfn/nvstRtsp/probe.ts:226`.
It creates STUN MESSAGE-INTEGRITY attribute `0x0008`, a 20-byte HMAC-SHA1
keyed by the negotiated ICE password. Username fragments are authenticated
message bytes, not the secret key. The callers at lines 805 and 814 supply
the negotiated password separately. This is not unkeyed hashing or password
storage, and the report does not establish a MAC-forgery path.

[RFC 8489 section 14.5](https://www.rfc-editor.org/rfc/rfc8489.html#section-14.5)
specifies HMAC-SHA1 for this field. SHA256 has a different STUN attribute and
compatibility requirements; substituting it here would change the wire format.
The supported Rust transport has equivalent keyed construction and constant-time
MAC verification in `opennow-streamer-transport/src/nvst.rs`, with known-answer,
altered-packet, wrong-credential and negotiated-identity tests.

Two independent read-only reviews agreed that this exact algorithm alert is a
false positive. It is dismissed with a scoped explanation, not by disabling
CodeQL or changing cryptographic validation. Legacy Electron code is neither
modified nor built/tested. This disposition is not an audit of other legacy
transport behavior or a claim that every use of SHA-1 is safe.

## Build and test boundaries

- Qt 6.8 exports `Qt6::GuiPrivate` through `Gui`; newer Qt packages may export it
  separately. CMake accepts both layouts and still requires the private target.
- Windows-only atomic state and packet-generation fields are compiled only for
  their consumers. Queue generation remains available to unit tests on all OSes.
  No frame ordering, queue recovery, or runtime crypto logic changes.
- Windows GPU tests check the actual D3D11 video interface. Only `E_NOINTERFACE`
  at that probe permits an explicit hardware-unavailable diagnostic and return.
  Device creation, swap-chain initialization, texture processing and blits still
  must succeed on capable hardware. No CI flag unconditionally disables tests.

## Verification

Use the existing Qt build and CTest suite, native workspace tests, and
`cargo clippy --workspace --all-targets -- -D warnings` from the streamer
workspace. Required GitHub Linux and Windows checks must pass before merging;
these changes do not bypass branch rules. Headless-runner capability skips are
not substitutes for real-GPU acceptance.
