# Official GeForce NOW binary audit (Linux x86_64 Flatpak)

Extended static reverse-engineering audit of the official GeForce NOW client shipped as the Linux Flatpak payload, for **OpenNOW feature-parity** work. Evidence is from symbols, rodata strings, radare2 disassembly, and shipped JSON—not from a live session.

## Artifact under test

| Field | Value |
| --- | --- |
| Source | User-hosted zip: `https://files.zortos.me/x86_64.zip` |
| Layout | Flatpak commit tree `235a800084abb2246e4578e5b60c33397fac73152c02a5751e663ddafd00e260` |
| Mall config build | `2.0.84.127` (`files/mall/shared/assets/config/config.json`) |
| Geronimo branch (embedded) | `gs_04_87` (Perforce paths in `.so` rodata) |
| Runtime | Freedesktop Platform `24.08`, command `GeForceNOW` |
| Primary natives | `libGeronimo.so` (34,347,824 bytes), `libBifrost2.so` (19,052,024 bytes), `libGsAudioWebRTC.so`, CEF shell `GeForceNOW` |

Reproduce locally: download the zip, extract under `audit/gfn-official/` (gitignored), run `strings` / `nm -D` / `r2` as documented in each section.

## Relationship to existing notes

- [`docs/streamer-comparison/`](../streamer-comparison/README.md) — behavioral comparison from Windows logs (build **2.0.87.131**, branch `gs_04_90`). This audit **disassembles** the Linux **`gs_04_87`** binaries and fills gaps the log-only notes left open (NACK-v2 layout, `nvbSendInputEvent`, DJB, empty `IOInterface` stubs, and more).
- OpenNOW implementations: `native/opennow-core/`, `native/opennow-streamer/`, `opennow-qt/`.

## Sections

| Document | Topic |
| --- | --- |
| [manifest-and-binaries.md](manifest-and-binaries.md) | Layout, sizes, NEEDED libs, CEF switches, config entry points |
| [session-creation-cloudmatch.md](session-creation-cloudmatch.md) | NVB API, GridServer HTTP, CEF `QUERY_GFN_*`, CloudMatch JSON, OpenNOW mapping |
| [transport-rtsp-mjolnir.md](transport-rtsp-mjolnir.md) | RTSP/RTSPS, ports, Mjolnir, ICE/DTLS/SCTP, NACK-v2, command IDs |
| [video-streaming-decode-recovery.md](video-streaming-decode-recovery.md) | Decode, present queues, DJB, dynamic streaming, recovery vs OpenNOW |
| [audio-opus-red-jitter.md](audio-opus-red-jitter.md) | Opus, RED, TimestampAudioBuffer, GsAudioWebRTC, SDL sink |
| [input-mouse-gamepad-features.md](input-mouse-gamepad-features.md) | XInput2/SDL, type 7/12, NVB features 0/6/8/10, activation chain |
| [opennow-parity-gaps.md](opennow-parity-gaps.md) | Consolidated gap list and recommended closure order |
| [index.html](index.html) | Same corpus as a navigable HTML report |

## Methods (nothing skipped on purpose)

- **Dynamic symbols**: `nm -D`, `c++filt` → `audit/gfn-official/extracts/*.demangled.symbols.txt` (local only).
- **Strings**: full `strings -a` on `libGeronimo.so`, `libBifrost2.so`, shell binaries.
- **Disassembly**: radare2 5.5.0 `pdf` on `nvbSendInputEvent`, NACK-v2 serializer (`0x0317`), `VulkanDecoder::decode`, `AsyncFrameQueue::flush`, `BifrostSDKExecutor` feature paths.
- **Config**: shipped `config.json`, `GeForceNOW.json`, Flatpak `manifest.json`.

Limits: no live seat capture in this run; mouse-settings **feature type 10** payload still lacks a byte-exact capture (log + API only). Enum integers for every `NVB_R_*` code are not dense-indexed from rodata alone.

## HTML report

Open [`index.html`](index.html) in a browser for section navigation and anchor links. It embeds the same Markdown bodies via pre-rendered HTML sections maintained alongside the `.md` files.
