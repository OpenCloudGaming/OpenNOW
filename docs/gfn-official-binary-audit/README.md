# GeForce NOW Linux payload audit (x86_64 Flatpak)

Static reverse-engineering audit of a user-supplied GeForce NOW Linux Flatpak payload, for **OpenNOW feature-parity** work. Evidence is from symbols, rodata strings, disassembly, and shipped JSON, not a live session. References to the "official" client describe the payload's product identity and API names; a signed NVIDIA distribution source was not verified.

## Artifact under test

| Field | Value |
| --- | --- |
| Source | User-hosted zip: `https://files.zortos.me/x86_64.zip` |
| Archive SHA-256 | `47ddbe0425b9ab560f64fa42a0052794c9de335ded0fd59637f082dd7a161ad4` |
| OpenNOW source comparison | `d33d42d63bd96247330a111b8bd9767df6822fb2` |
| Layout | Flatpak commit tree `235a800084abb2246e4578e5b60c33397fac73152c02a5751e663ddafd00e260` |
| Mall config build | `2.0.84.127` (`files/mall/shared/assets/config/config.json`) |
| Geronimo branch (embedded) | `gs_04_87` (Perforce paths in `.so` rodata) |
| Runtime | Freedesktop Platform `24.08`, command `GeForceNOW` |
| Primary natives | `libGeronimo.so` (34,347,824 bytes), `libBifrost2.so` (19,052,024 bytes), `libGsAudioWebRTC.so`, CEF shell `GeForceNOW` |

To reproduce the static evidence, verify the archive hash before extracting the named libraries under `audit/gfn-official/`, which is gitignored. Use `strings`, `nm -D`, `readelf`, and bounded `objdump` or radare2 listings. Do not execute, load, or install the payload. A matching hash identifies this supplied artifact, not its publisher. Per-library hashes are in [manifest-and-binaries.md](manifest-and-binaries.md).

Current-product comparisons refer to the exact OpenNOW commit above. The supported desktop path is Qt/QML plus the in-process native core and streamer. Standalone presenter code and the vendor's CEF shell are comparison material, not alternative OpenNOW runtime designs. Static calls and adjacent log strings do not establish live wire behavior or measured quality differences.

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
| [opennow-parity-gaps.md](opennow-parity-gaps.md) | Candidate gaps, ownership, and evidence gates |
| [index.html](index.html) | Same corpus as a navigable HTML report |

## Methods and limits

- **Dynamic symbols**: `nm -D`, `c++filt` → `audit/gfn-official/extracts/*.demangled.symbols.txt` (local only).
- **Strings**: full `strings -a` on `libGeronimo.so`, `libBifrost2.so`, shell binaries.
- **Disassembly**: radare2 5.5.0 `pdf` on `nvbSendInputEvent`, NACK-v2 serializer (`0x0317`), `VulkanDecoder::decode`, `AsyncFrameQueue::flush`, `BifrostSDKExecutor` feature paths.
- **Config**: shipped `config.json`, `GeForceNOW.json`, Flatpak `manifest.json`.

Limits: no live seat capture in this run; mouse-settings **feature type 10** payload still lacks a byte-exact capture (log + API only). Enum integers for every `NVB_R_*` code are not dense-indexed from rodata alone.

## HTML report

Open [`index.html`](index.html) in a browser for section navigation and anchor links. It embeds these Markdown sections with the checked-in template and a pinned renderer. From the repository root:

```bash
python3 -m venv ~/.capy/work/gfn-audit-venv
~/.capy/work/gfn-audit-venv/bin/pip install -r docs/gfn-official-binary-audit/requirements.txt
~/.capy/work/gfn-audit-venv/bin/python scripts/generate-gfn-binary-audit.py
~/.capy/work/gfn-audit-venv/bin/python scripts/generate-gfn-binary-audit.py --check
~/.capy/work/gfn-audit-venv/bin/python -m unittest discover -s scripts -p test_generate_gfn_binary_audit.py
```

The check requires matching HTML, existing relative links and anchors, passive HTML, and selected source-contract markers at the recorded comparison commit. In a shallow checkout, fetch that comparison commit before running it. Its credential-pattern checks do not replace manual public-safety review. Source markers detect drift; they do not prove runtime acceptance, every forensic inference, or the absence of all possible secrets.
