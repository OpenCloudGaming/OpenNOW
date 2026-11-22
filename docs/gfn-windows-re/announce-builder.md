# Windows ANNOUNCE builder and color encoding

This is static evidence from the separately supplied `Bifrost2.dll`, `gs_04_92 38967594`, image base `0x180000000`. It describes the serializer's decisions, not a captured ANNOUNCE body. The S6 log reports 163 appended attributes but does not print their names or values.

## The body is a difference against the server offer

**D.** `SignalingHandler` setup at `0x1803babe0` builds a comparison configuration from the DESCRIBE SDP. `ReadNvscConfigFromSdp` at `0x1803f8670` returns whether it read any attributes. That result enables comparison mode for `AppendNvscConfigToSdp` at `0x1803f8140`, which calls the generated field walk at `0x1804d5e00` and counts appended attributes.

The typed writer at `0x1803f7970` emits a byte-valued attribute only when comparison mode is off or its current value differs from the comparison value. It writes the byte unchanged as a decimal value. Other typed writers implement the corresponding comparison for their types. Field wrappers such as `0x1805b1b90` add the `x-nv-` prefix. They can force emission for a name in an explicit inclusion list. The traced setup call supplies an empty list.

So a key in the final effective configuration is not proof that it appeared in ANNOUNCE. A key absent from the body can still be negotiated through DESCRIBE. This also explains why the 975-key log dump, the 4,145-attribute server SDP, and the 163 appended client attributes have different sizes.

## ICE and recovery fields

These are field sites inside the generated walk, not separate function entries. The offsets are relative to the current `NvscClientConfig` object for stream zero.

| Key before `x-nv-` | Field site in `0x1804d5e00` | Config offset | Serializer behavior |
| --- | --- | --- | --- |
| `general.iceUsernameFragment` | `0x1804d9a91` | `+0x2d30` | Nullable legacy value |
| `general.iceUsernamePwd` | `0x1804d9b42` | `+0x2d40` | Nullable legacy value |
| `general.dtlsFingerprint` | `0x1804d9bf3` | `+0x2d50` | Nullable legacy value |
| `general.iceUserNameFragmentV2` | `0x1804d9e06` | `+0x2f64` | String value, compared with the offer |
| `general.enableRecoveryMode` | `0x1804da2dd` | `+0x3d38` | Compared typed value |
| `general.recoveryModeTriggerThresholdMs` | `0x1804da38e` | `+0x3d3c` | Compared typed value |
| `general.enableIntraRefreshInRecoveryMode` | `0x1804da43f` | `+0x3d3e` | Compared byte |
| `video[0].streamRecoveryType` | `0x1804dfd27` | `+0x3e54` | Compared typed value |
| `video[0].refPicInvalidation` | `0x1804ea533` | `+0x3ec9` | Compared byte |
| `video[0].framesWithInvalidRefThreshold` | `0x1804ec825` | `+0x3ee0` | Compared typed value |

**D/L.** The V1 wrapper at `0x1805b36d0` calls `addSdpAttribute` at `0x1802db3a0`. A null legacy value takes the `Skipping field` branch. That is why the three V1 keys are skipped in S6. It is not a global prohibition on serializing their names. The V2 fields use a different string writer, `0x1803f7600`.

**D.** `refPicInvalidation` and the recovery keys are supported by the Windows ANNOUNCE field walk. They are subject to the same comparison rule. The missing body prevents an exact statement that S6 sent each one. OpenNOW already implements reference invalidation and dependent-frame withholding, so a missing client-side attribute alone does not establish missing recovery.

## Color fields are emitted unchanged; chroma differs

**D.** The bit-depth field site at `0x1804ef015` reads byte `+0x3efc`; chroma at `0x1804ef513` reads byte `+0x3efd`. Both call `0x1805b1b90` and then `0x1803f7970`. There is no depth or chroma conversion between the configuration byte and the serialized decimal value.

**L.** S6's configuration contains `video[0].bitDepth 10` and `video[0].chromaFormat 1` (`geronimo.log.bak`, local 19:45:31; `nvsc-client-config.tsv`). Geronimo and the Y410 decoder independently confirm HEVC 4:4:4 10-bit. When emitted, these fields therefore carry `10` and `1`. Equal-to-offer values can be omitted. The unchanged-byte writer does not prove a bit-depth enum value of 1.

OpenNOW at `7ce3cd54` always emits depth `8` or `10` and chroma `1` or `3` in `native/opennow-streamer/crates/opennow-streamer-core/src/nvst_rtsp_color.rs:15-29`. Depth 10 agrees with S6. Chroma 3 differs from Windows' configured and serialized 4:4:4 value 1. Its claim that codec chroma IDs are the official wire convention is contradicted by this Windows path. The equivalent macOS writer also serializes its configuration value unchanged; the earlier macOS study did not establish the complete boundary conversion.

This establishes a client-encoding difference. It does not prove that a server rejects OpenNOW's values or that copying GFN's values into OpenNOW will work for every seat. That needs an authorized, redacted capture or controlled session comparison. CloudMatch JSON, Geronimo's presentation settings, and ANNOUNCE are separate boundaries even when some numeric values coincide.

## What the DLL does not recover by itself

The exact 163-key S6 body, the complete baseline DESCRIBE offer, and server acceptance of alternate encodings remain unknown. Likewise, a final-config difference in FEC, pacing or mic SSRC is not an actual-wire discrepancy until the comparison mode and the selected field values are accounted for. OpenNOW also filters server-owned FEC and pacing defaults against DESCRIBE in `nvst_rtsp.rs:1378-1459`.
