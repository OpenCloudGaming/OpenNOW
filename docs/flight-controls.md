# Flight controls and wheels

OpenNOW's Qt application forwards allowlisted flight controls and wheels as raw HID devices through the native NVST input transport. It does not convert them to Xbox controllers. Physical I/O belongs to `ControllerInput`'s HID worker. The Rust HID crate owns registration, bounded queues, report release, and session lifetime.

## Device allowlist

The Windows and macOS reference clients have the same 18 public VID:PID defaults:

| Vendor | Product IDs, hexadecimal |
| --- | --- |
| Thrustmaster `044F` | `0402`, `0404`, `B67C`, `B687`, `B68D`, `B68F`, `B697` |
| Logitech `046D` | `C24F`, `C262`, `C266`, `C268`, `C26E`, `C272`, `C276` |
| Saitek `06A3` | `075C`, `0762` |
| Mad Catz `0738` | `2221`, `A221` |

The list covers Warthog, T.Flight, TWCS, rudder-pedal, X52, X56, and Logitech wheel families. A matching ID is an admission rule, not a claim that every firmware version has passed physical testing. A stick and throttle with different PIDs remain separate logical devices, even when the operating system gives them the same container identity.

These devices are excluded from SDL's gamepad path while the generic-HID owner is enabled. This prevents duplicate HID and Xbox input. Rejected HID registration does not silently become gamepad emulation. Other gamepads retain their existing routing.

Known keyboard, keypad, pointer, mouse, and digitizer collections remain outside this passthrough path, using the reference Windows collection filter. Qt retains keyboard and pointer capture ownership.

## Registration and reports

Generic request IDs use 10 through 254. Synthesized Sony IDs use 6 through 9. OpenNOW waits for the server's attach response before forwarding reports with the assigned ID. Request IDs are not reused within a session, so late responses cannot attach a replacement device accidentally. Exhausting that range refuses further registrations and logs that a new session is required. An assigned server ID can be reused only after its detach was queued and a later attach received fresh approval on the ordered reliable channel.

The attach event uses control 1, flags `0xF`, and big-endian VID, PID, revision, and `customId` words. `customId` is zero for this path. Removal uses control 3. These words are not HID usage page and usage, and `customId` is not the report's interface number. The report descriptor is read locally for release and output routing; it is not appended to this attach event.

HID input uses the existing reliable NVST control channel, command `0x020D`. A single record is `22 | 11 00 00 00 | deviceId | interface | operation | raw report`. Operation 0 carries input bytes unchanged, including a report ID when the device supplies one. The outer command supplies the length. Output arrives on command `0x0206` with RI type `0x11`:

| Operation | Local HID action |
| --- | --- |
| 0 | Write an ordinary output report |
| 1 | Send an output report through the control-report API |
| 2 | Read an input report |
| 3 | Read a feature report |
| 4 | Send a feature report |

Operations 0 and 1 remain distinct. Windows uses `WriteFile` for the former and `HidD_SetOutputReport` for the latter. Feature-read requests contain a report ID and a little-endian 16-bit requested length. Successful GET replies use the operation's response form, whose second header byte is zero rather than the physical interface.

`ri.hidDeviceMask` bit `0x4` means DS4 support and bit `0x2` means DS5 support. Neither is used as an invented generic-HID capability bit. Generic registration has its own server admission response. The response's extra 32-bit word is parsed but does not gate admission, matching the reference client.

## Capture and failure behavior

HID polling continues when gameplay capture is disabled. Forwarding stops on focus loss, a local overlay, capture disable, and shutdown. Before forwarding stops, the native session derives one release report from each last-forwarded report ID:

- Button-page data fields become zero.
- Nullable hat fields receive a representable value outside their logical range.
- Axes, throttles, non-nullable hats, constants, and other fields retain their last forwarded values.

A missing, malformed, or unsupported descriptor causes detach on capture loss and a fresh attach on resume instead of an invented neutral report. The next polled report can be forwarded after admission. Device removal and session end detach the device. Incarnations and platform arrival identities distinguish rapid reconnects from the old device, even when its path and physical port are reused. Ambiguous composite routing, oversized descriptors, and missing physical or arrival identity fail closed.

On Linux, a nonempty SDL serial must exactly match either the USB ancestor's `serial` or the HID ancestor's `HID_UNIQ` on the same sysfs path. [SDL HIDAPI reads `HID_UNIQ`](https://github.com/libsdl-org/SDL/blob/release-3.2.20/src/hidapi/linux/hid.c), while the [Logitech HID++ driver can replace it with a device-information serial](https://github.com/torvalds/linux/blob/v6.17/drivers/hid/hid-logitech-hidpp.c). That serial can differ from the USB descriptor serial, as reported for the G920 in [#1258](https://github.com/OpenCloudGaming/OpenNOW/issues/1258). OpenNOW validates the serial against its source rather than normalizing unrelated identifiers or ignoring mismatches for a wheel model. USB VID/PID, the physical sysfs parent, and the per-arrival path and inode checks still apply, including when the device is opened.

There are at most eight logical sources and 16 interfaces per source. Descriptors are limited to 4,096 bytes and reports to 256 bytes. Each native source queue holds at most 64 reports and 16 KiB. The output queue holds at most 32 commands and 8 KiB. Overflow retires the affected source instead of discarding a release. Physical-device discovery and blocking output operations stay off the GUI and render threads.

## Protocol evidence

The implementation uses static analysis of the supplied clients. No GFN executable was run and no NVIDIA service was contacted.

| Fact | Windows linked address | macOS x86_64 unslid address |
| --- | --- | --- |
| Generic attach and request allocation | Geronimo `0x1801AF130` | Geronimo `0xE09D0` |
| Device-change wire encoder | Bifrost `0x1803F4270` | Geronimo `0x56200` constructs the matching event |
| Raw single and batched reports | Bifrost `0x1803F27C0`, `0x1803F29E0` | Bifrost `0x225C70`, `0x226240` |
| Generic output operations | Geronimo `0x1801B8160`, `0x1804DF310` | Geronimo `0xE6B10` |
| Gamepad exclusion | Geronimo `0x1800E34A0` | Geronimo `0x56170` |
| Polling and bounded report batches | Geronimo `0x1801B5E60` | Geronimo `0xE6320` |
| Public allowlist | Geronimo settings and controller initialization | Geronimo default constructor `0x72320` |
| DS4 and DS5 mask meanings | Geronimo `0x1800E0660` | Separate generic controller creation |

The reference polling period is 4 ms. Its coalescing can discard intermediate joystick states; OpenNOW uses bounded ordered report queues instead. Descriptor-derived capture release is an OpenNOW safety policy. The reference capture gate alone does not establish remote neutralization.

`expandSessionParamsMetaDataWithGenericHid` at Windows `0x1800C76D0` adds `TrueForceEnabled` and an entire local `RIDevices.json` object only for internal users. It does not build that object from attached public devices. OpenNOW does not add this metadata. The internal JSON descriptor schema, a separate generic-HID DESCRIBE capability, and the initial CloudMatch HID-inventory population were not established and are not guessed.

## Physical validation still required

Unit tests cover wire bytes, allowlisting, descriptor release, queue bounds, stale device identities, and lifecycle transitions. Physical validation still needs an allowlisted stick or throttle and a wheel: server registration, all axes and buttons, multiple interfaces, focus and overlay release, unplug and reconnect, and output or force-feedback behavior. Windows and macOS runtime behavior, firmware-specific reports, and a real account's effective allowlist require those checks. OS control-report calls can block inside a driver; shutdown joins the worker that owns those calls rather than destroying a live handle. Linux libusb-only paths without a verifiable hidraw identity are not admitted.

## Flight-controls UI

Settings → Controls persists native forwarding in the Rust core, enabled by
default. Qt changes routing only after the setting write is confirmed. When it
is off, mapped SDL devices become controllers and unmapped devices are ignored.
The connected list distinguishes detection (`ready`) from a verified server
attach (`active`) and refusal (`declined`). On Linux, a device whose hidraw node
OpenNOW cannot open for reading and writing shows `no-access`; the HID worker
checks access on every discovery scan and retries once a udev rule grants it.
A device SDL reports with an allowlisted VID:PID but no matching hidraw source
shows `no-hid-path`: it cannot be forwarded and stays out of the gamepad path
until forwarding is turned off. This status waits until two HID discovery
scans complete after SDL first reports the device, so startup and hotplug
ordering do not report it falsely.
Session teardown and capture detach reset admission; stale incarnations cannot
update a replacement device.

HID worker refusals, opens, failures, and server admission results are written
to the diagnostics log as `input generic-hid …` lines, so an exported report
shows why a device never reached the game.

### T.Flight Hotas 4 guidance

Unplug the device, hold **SHARE + OPTIONS**, connect USB, wait a few seconds, then
release both buttons. Repeat this each time you start a GeForce NOW session.
This is static guidance, not mode detection: no independently confirmed PS4-mode
product ID is available, so there is no `needsHidMode` state.

Sources: [Thrustmaster KB 1881](https://support.thrustmaster.com/en/kb/1881-en/)
and [NVIDIA answer 5787](https://nvidia.custhelp.com/app/answers/detail/a_id/5787).

### Display-name evidence

| VID:PID | Device | Evidence |
| --- | --- | --- |
| 046D:C262 | Logitech G920 | usb.ids |
| 046D:C24F | Logitech G29, PS3 mode | usb.ids |
| 046D:C26E | Logitech G923, Xbox/PC | Oversteer |
| 046D:C266 | Logitech G923, PlayStation/PC | Oversteer |
| 046D:C272 | Logitech G PRO, Xbox/PC / RS50 compatibility mode | Oversteer; TrueForce |
| 046D:C268 | Logitech G PRO, PlayStation/PC | Oversteer |
| 046D:C276 | Logitech RS50 | USB product-string evidence |
| 044F:B67C | Thrustmaster T.Flight Hotas 4 | linux-hardware |
| 044F:B68D | Thrustmaster T.Flight Hotas One, PC mode | Flight Simulator forum; HID product log |
| 044F:B68F | Thrustmaster TPR Pendular Rudder | linux-hardware |
| 044F:0402 | Thrustmaster HOTAS Warthog Joystick | usb.ids |
| 044F:0404 | Thrustmaster HOTAS Warthog Throttle | usb.ids |
| 044F:B687 | Thrustmaster TWCS Throttle | usb.ids |
| 044F:B697 | Thrustmaster T80 / T98 Ferrari 296 GTB | Driver database revision evidence |
| 06A3:075C | Saitek X52 | usb.ids |
| 06A3:0762 | Saitek X52 Pro | usb.ids |
| 0738:2221 | Saitek X56 Stick | x56linux |
| 0738:A221 | Saitek X56 Throttle | x56linux |

Sources: [usb.ids](https://github.com/usbids/usbids/blob/master/usb.ids),
[Oversteer USB rules](https://github.com/berarma/oversteer/blob/master/data/udev/99-logitech-wheel-perms.rules),
[TrueForce driver](https://github.com/mescon/logitech-trueforce-linux-driver),
[RS50 product-string evidence](https://github.com/mescon/logitech-trueforce-linux-driver/issues/21),
[Hotas 4](https://linux-hardware.org/?id=usb:044f-b67c),
[Hotas One](https://forums.flightsimulator.com/t/thrustmaster-hotas-one-not-working-in-geforce-now/757328),
[HID product log](https://github.com/mixxxdj/mixxx/issues/14632),
[TPR](https://linux-hardware.org/?id=usb:044f-b68f),
[T80](https://treexy.com/products/driver-fusion/database/human-interface-devices/thrustmaster/t80/),
[T98 revision evidence](https://treexy.com/products/driver-fusion/database/id/usb/vid_044f/pid_b697/),
and [X56](https://github.com/Chryseus/x56linux).

`C272` is shared by two models, and `B697` varies by revision and does not establish
a firmware mode. The list retains those ambiguities. Paper's T.Flight Rudder
Pedals row is not in this allowlist; `B68F` identifies TPR instead. The seven Logitech wheel
IDs include G PRO and RS50, so the UI does not collapse them into G29/G920/G923.

### Visual acceptance

The reference is [Paper → OpenNOW → Settings](https://app.paper.design/file/01M11SPTRPMYQB9S9AX948A6WH/p-H-1),
the Flight controls row of artboards. Run
`ctest --test-dir build/opennow-qt -R '^qml-flight-controls-' --output-on-failure`.
Screenshot fixtures use `--smoke-test --allow-multiple-instances --desktop
--smoke-flight-controls --flight-state STATE --reduced-motion --screenshot PATH`.
Settings states use `--route settings-input`; stream states use `--route stream`.
Add `--flight-fullscreen` for fullscreen. Stream fixtures check that closing and
reopening overlays preserves the same video item and session.
The sunrise backdrop and telemetry values in these captures are acceptance
fixtures, not a live gameplay recording. They do not replace the production video
item or change production admission states.
