# NVB features and control binaries

Byte-verifiable control frames only. Official Geronimo logs name feature types and print a heap pointer for `nvbFeatureControl`, not hex. Type 10 has no captured frame on this machine.

Hex in tests is compact lowercase. The same bytes are also written with spaces so you can count fields.

Source of truth for OpenNOW frames is `control_keepalive_and_activation_are_byte_exact` and the neighboring tests in `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst_input.rs`. QoS and IDR vectors live in `nvst_control.rs`.

## Wire header

Every OpenNOW outbound control frame is

| Offset | Type | Meaning |
|---|---|---|
| 0 | u16 LE | command code |
| 2 | u16 LE | payload length |
| 4 | bytes | payload |

`0x0308` on the wire is `08 03`. `0x030d` is `0d 03`. Do not read those as big-endian command IDs.

## Official NVB feature types

From `geronimo.log` and `geronimo.log.bak` on this machine. `nvbFeatureControl` only prints a heap pointer such as `000001F167CCC990`. That is not a payload.

| NVB type | Log verb | When | Meaning from surrounding lines | OpenNOW command | Payload in this tree |
|---|---|---|---|---|---|
| 0 | Enabled / Disabled | Every cursor-info change. Official then hides the remote cursor. | Server-composited cursor in the video | `0x0308` `COMMAND_MOUSE_CURSOR_CAPTURE` | Known. See below. |
| 6 | Enabled | After `Sending haptics state 1 to server` | Gamepad rumble | `0x0322` `COMMAND_HAPTICS_STATE` | Known. One byte. See test `transport_control_inputs_do_not_disable_native_input`. |
| 8 | Enabled | Once when the session is ready, with type 0 | Track remote cursor image | `0x030d` `COMMAND_TRACK_REMOTE_CURSOR_IMAGE` | Known. See below. |
| 10 | Update | Next to `Sending SDL mouse settings` or `Sending alt mouse settings (accel=0, speed=10)` | Host mouse accel and speed | none captured | **Not dumped. Do not send a guess.** |

No other feature type numbers appear in these logs.

## Feature frames OpenNOW does send

Proven by `assert_eq!(..., hex(...))` in `nvst_input.rs`.

### `0x0308` mouse cursor capture

NVB type 0. One payload byte. `0` off, `1` on.

```
off  08 03 01 00 00
on   08 03 01 00 01
```

Activation sends **on**. After the first local cursor, OpenNOW sends **off** so the host stops compositing its cursor into the video. Notifications keep arriving.

### `0x030d` track remote cursor image

NVB type 8. Same one-byte payload.

```
off  0d 03 01 00 00
on   0d 03 01 00 01
```

Activation sends **on** and leaves it on.

### Type 10 mouse settings — no captured frame

What is actually on disk:

- `geronimo.log` / `.bak`: `Sending SDL mouse settings (accel=0, speed=10)` or `Sending alt mouse settings (accel=0, speed=10)`, then `nvbFeatureControl( <heap pointer> )`, then `Update feature type: 10`. The pointer is the client object, not a payload.
- One early-session failure in `geronimo.log.bak`: `Failed to update mouse sensitivity: 10 NVST_R_SUCCESS (0x0)` then `nvbFeatureControl failed for type: 10`. The `10` in that line is the speed value from the format string `Failed to update mouse sensitivity: %d %s (0x%x)` in `Bifrost2.dll`.
- `debug.log`: `Disabling session mouse acceleration` at `QUERY_GFN_START`. No bytes.
- `Geronimo.dll` names: `sendMouseSettings(bool, unsigned int)`, literals `accel=0, speed=10` / `accel=%d, speed=%d`.
- `Bifrost2.dll` names: `Failed to %s feature mouse acceleration`, `NVST_R_SERVER_CONTROL_MOUSE_SETTING_FAILED`. No `runtime.mouseAccel` key. No hex dump format string.

Searched and not present: `.pcap` / `.pcapng` / `.etl` under GFN AppData, Downloads, Documents. No control-frame hex next to type 10 in any `.log` / `.bak`. `snapshot_blob.bin` is V8, not NVST.

There is no byte-exact type 10 frame to copy. OpenNOW currently emits a reconstructed `0x0323` in `nvst_input.rs`. That is not a capture. Leave this section empty until one exists.

## Activation chain

`activation_chain(20102193)` in the same test. Sent on `control_channel_reliable` in this order.

| Step | Code | Hex |
|---|---|---|
| 1. enable input false | `0x020b` | `0b 02 0c 00 00 00 00 00 01 00 00 00 00 00 00 00` |
| 2. device descriptor index 2 | `0x020d` | `0d 02 30 00 23 00 00 00 00 01 32 bc 31 22 0c 00 00 00 1a 00 00 00 02 00 14 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 55 00 00 00 00 00 00 00 00 00 00 00 00` |
| 3. cursor capture on | `0x0308` | `08 03 01 00 01` |
| 4. track cursor on | `0x030d` | `0d 03 01 00 01` |
| 5. window state 19 | `0x0320` | `20 03 0c 00 00 00 00 00 13 00 00 00 00 00 00 00` |
| 6. system state 0 | `0x0321` | `21 03 0c 00 00 00 00 00 00 00 00 00 00 00 00 00` |
| 7. enable input true | `0x020b` | `0b 02 0c 00 00 00 00 00 01 00 00 00 01 00 00 00` |

`0x020b` payload is three LE u32 fields. stream index `0`, counter `1`, enabled `0` then `1`.

`0x0320` / `0x0321` payload is three LE u32 fields. stream index `0`, state, frame `0`. Window state **19** (`13 00 00 00`) is required. All-zero window state leaves the session looking inactive and the server withholds system-cursor mode updates.

The test function currently also inserts a reconstructed mouse-settings command between steps 4 and 5. That step is not listed here. It is not from a capture.

## Other outbound control frames

### Keepalive `0x0200`

Every 3 seconds. `control_keepalive(0)`:

```
00 02 04 00 00 00 00 00
```

### Input version inbound

Handshake is ready when `control_channel_reliable` delivers version 3.

```
0e 02 02 00 03 00
```

That is `0x020e`, length 2, version `3`.

### Relative mouse `0x0206`

Test vector. dx=24, dy=24, timestamp `0x01861330`. Route is `control_channel_partially_reliable`.

```
06 02 28 00 00 00 00 24 0e 00 00 00 00 00 00 0a 07 00 00 00 00 18 00 18 00 00 00 00 00 00 00 00 00 00 00 00 30 13 86 01 00 00 00 00
```

Inner type is LE u32 `7`. Motion uses one trailing LE timestamp.

### Key down `0x0206`

Test vector. virtual key `0x41`, modifiers `1`, timestamp `0x015b15b1`. Route is reliable.

```
06 02 30 00 00 00 00 2c 0e 00 00 00 00 00 00 12 03 00 00 00 00 41 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 b1 15 5b 01 00 00 00 00 b1 15 5b 01 00 00 00 00
```

Keys and absolute mouse get two trailing timestamps.

### Gamepad `0x020d`

First report also sends a device descriptor on reliable control, index **3**. Updates go `input_channel_partially_reliable`.

First update in the test. sequence `1`, buttons `0x1000`, timestamp `0x015b171a`:

```
0d 02 34 00 23 00 00 00 00 01 5b 17 1a 26 00 00 01 22 0c 00 00 00 1a 00 00 00 03 00 14 00 00 10 00 00 00 00 00 00 00 00 00 00 00 00 55 00 00 00 00 00 00 00 00 00 00 00 00
```

The inner player index is hardcoded `03`. Slot and bitmap from the 38-byte type-12 packet are not copied.

`INPUT_HAPTICS_ENABLED` encodes to `22 03 01 00 01` when the flag is on.

## Media control, not NVB features

These are QoS and recovery. Same header rule. Tests in `nvst_control.rs`.

| Code | Name | Proven frame or payload |
|---|---|---|
| `0x0302` | IDR request | `02 03 02 00 00 00` |
| `0x0203` | frame pacing | payload `05 00 00 00 00 00 00 00 02 00 00 00 01 00 00 00 80 3e 00 00 80 3e 00 00 1a 34 00 00` for frame 1, 16000 µs, 16000 µs error |
| `0x0204` | frame ack | 102-byte payload. Full hex in `frame_ack_places_only_source_pinned_fields` |
| `0x0207` | QoS report | 52-byte payload. Full hex in `qos_report_matches_the_source_test_layout` |

IDR is the recovery command OpenNOW already sends. Official also sends a reference-invalidation request. That invalidation frame is **not** in this tree and is not guessed here.

## Inbound cursor examples

Server `0x010f` system cursor. Tests in `extracts_system_cursor_notifications_from_server_control`.

```
visible id 1     0f 01 09 00 01 00 00 00 0c 80 16 80 01
id 0 lock-style  0f 01 04 00 00 00 00 00
```

Normalized extract for id 1 is `00 01 00 00 00 00 00 0c 80 16 80`. OpenNOW treats type 0 and cursor id 0 as hidden relative.

## What is missing on purpose

- Feature type 10 on-wire bytes. Logs and DLL names prove it is mouse accel/speed. They do not dump the frame.
- Official encoding of type 0 and type 8. OpenNOW’s `0x0308` / `0x030d` match the comments and the session behavior. They are not proven equal to Geronimo’s on-wire feature blob until a capture says so.
- Reference-invalidation bytes.

When you have a capture, put the exact type 10 frame in the section above and pin it with a test. Do not fill it from the log English or from reading the DLL.
