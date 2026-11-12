# Artifact inventory

This study examined one Windows GeForce NOW install subset and its logs, uploaded as `GFN.zip` on 2026-10-13. The archive SHA-256 is `73d96b78930a8b88eb18580c11bd1fbcbd5ab323b0ec56b35002ad861a10dc95` (33,644,730 bytes). Product metadata reports **2.0.89.141**. Geronimo's product-version resource reports `gs_04_92 38967594`. The metadata does not authenticate an NVIDIA release; the Authenticode directory is present on every PE below, but signatures were not validated on this Linux host.

The user supplied `Bifrost2.dll` separately on the same day. Its product version, `gs_04_92 38967594`, matches Geronimo. None of these files are committed. Hashes let a reader confirm they are looking at the same build.

## Portable executables

All files are PE32+ x86-64. Image bases are `0x140000000` for the EXEs and `0x180000000` for the DLLs. Every virtual address (VA) in these documents uses those bases.

| Path in upload | Bytes | SHA-256 | FileVersion / ProductVersion | Link date (UTC) | PDB name | Cert directory |
| --- | ---: | --- | --- | --- | --- | --- |
| `GeForceNOW.exe` | 3885680 | `b14df3712d08837864ea055e9af9e0b6ef5cf4fad2bbb5e4fb0f8121977de1e2` | 128.6613.138.34 / 2.0.89.141 | 2026-09-21 | GeForceNOW.pdb | yes |
| `GeForceNOWStreamer.exe` | 508528 | `5e8429c633c0bf1157a04cc916af5b2a5c0707450547993ca97a4057d0cb5415` | 1.0.0.0 / 2.0.89.141 | 2026-09-21 | (none) | yes |
| `GeForceNOWContainer.exe` | 1691248 | `26d885ee132bc2e32e244254949b88ce8c83646fbf09419f35a3b92a4e7ef623` | 1.51.3806.6365 / 2.0.89.141 | 2026-05-27 | NvContainer.pdb | yes |
| `Geronimo.dll` | 29333616 | `c2e03f512f16db37463f846c79787ebd07c8e6fc662a781fa12429136751d4c3` | - / gs_04_92 38967594 | 2026-09-16 | Geronimo.pdb | yes |
| `Bifrost2.dll` (separate upload) | 22661232 | `31717a7d2463f61e89beed80b086c337e2324481888c3b7fe9d3268e2a841e46` | - / gs_04_92 38967594 | 2026-09-16 | Bifrost2.pdb | yes |
| `GsAudioWebRTC.dll` | 1657968 | `d65a3d1386cf415c8d4278bc3091a680a5002a4563bc4b4fe8120e39f610a936` | - / - | 2021-05-12 | GsAudioWebRTC.pdb | yes |
| `NvPixels.dll` | 3222128 | `fb28fe2355b89c05510b9a1ed9b74ca9c3e13548b3b5d377129a925eeecfc0d3` | - / - | 2026-02-17 | NvPixels.pdb | yes |
| `NvTelemetry64.dll` | 8169072 | `6d48eee39a84411faa4d6ea6435513779c55e577ccf3828b7abeafa9d56a7b02` | 19.8.6.0 / 19.8.6.0 | 2026-08-10 | NvTelemetry64.pdb | yes |
| `d3dcompiler_47.dll` | 4917360 | `e402b3273543e7cefbd2869ca94c08c3af98ae2388c7eb69251526ea6d30efa5` | 10.0.22621.2428 | 2023-11-11 | D3DCompiler_47.pdb | yes |
| `nvc/localuser/GFN/GameStreamClientAgent.dll` | 2475632 | `eead8d124907bb3f8d92bfe6869468019b4bbec092cb049d1d37658649f2fe04` | 2.0.89.141 | 2026-09-16 | GameStreamClientAgent.pdb | yes |
| `nvc/localuser/GFN/GeForceNOWReliabilityMonitor.dll` | 12559472 | `f05e35a7ab329d36f098c67cd8552e9045c9d0a209225f63417c666b91e9fa7c` | 2.0.89.141 | 2026-09-21 | backgroundagent.pdb | yes |
| `nvc/localuser/NvMessageBusBroadcast.dll` | 3520112 | `560ac26d904a4581f04ad90505222c04792669dba8531aca6f328c55ab1778a3` | - / gcomp_dev 36798940 | 2025-11-03 | NvMessageBusBroadcast.pdb | yes |
| `nvc/localuser/NvTelemetry64.dll` | 8169072 | `5282cffb56624b0764fe140aad348cf323fd92d035ff0c27ccfec3ef2a1e5c62` | 19.8.6.0 | 2026-08-10 | NvTelemetry64.pdb | yes |

Geronimo's PDB path names the branch `gs_04_92` under `GridClients\geronimo`. GeForceNOW.exe's PDB path names the CEF client build `GFN-crimson-official-winauto`.

## Import relationships that shape the study

- **S.** `Geronimo.dll` imports 31 functions from `Bifrost2.dll` (for example `nvbStartSession`, `nvbSendInputEvent`, `nvbSendMicAudioFrame`, `nvbUpdateVideoFrameState`, `nvbUpdateDJBState`, `nvbFeatureControl`), 124 from `SDL2.dll`, and 10 from `GsAudioWebRTC.dll`. It also imports `d3d9`, `d3d11`, `dxgi`, `dxva2`, `OPENGL32`, `HID`, `WINHTTP`, `bcrypt`, and `CRYPT32`.
- **S.** `NvPixels.dll` imports `nvcuda.dll`.
- **S.** `GsAudioWebRTC.dll` imports only `WINMM`, `msdmo`, `KERNEL32`, `USER32`, and `ole32`.
- **S.** `GeForceNOWStreamer.exe` imports only `KERNEL32` and `SHELL32`.

## Files the upload does not contain

The following files are referenced by imports, logs, or `GeForceNOW.json`, but are not in the upload:

| Missing file | Why it matters |
| --- | --- |
| `SDL2.dll` | Window, event, controller, and audio-device layer used by Geronimo. |
| `libcef.dll` and the `cef/` plugin folders (`BackgroundProcess`, `GeForceNOW`, `Base`) | The CEF runtime and the native plugins that host the JavaScript application. |
| `mall/` web application | The JavaScript UI, `shared/assets/config/config.json`, login and catalog code. |
| `CrimsonUtil.dll` | Referenced by `GeForceNOWReliabilityMonitor.json`. |

The separate Bifrost upload supplies the Windows network and protocol implementation. The protocol documents distinguish its static evidence from the supplied runtime logs and compare it with macOS Bifrost from PR #1161.

## Logs and configuration

| File | Bytes | Content |
| --- | ---: | --- |
| `geronimo.log`, `geronimo.log.bak` | 1254920, 2108400 | Geronimo and Bifrost/NVST log lines (`[NVST:*]`, `[BifrostClient: *]`), 2026-10-12. |
| `CxNative_GeForceNOW.log`, `.1.log` | 15840901, 52464263 | CEF native (Crimson) layer, 2026-10-11 to 2026-10-12. |
| `console.log`, `.bak` | 1896230, 1575580 | JavaScript application log. |
| `debug.log`, `.bak` | 836180, 760945 | Chromium/CEF browser process log. |
| `logs/gameStreamClientAgent.log`, `.bak` | 51892, 21645 | NvContainer stream agent; message-bus IPC events. |
| `logs/gfn_reliability_monitor.log`, `.bak` | 23892, 17743 | Background agent and reliability monitor. |
| `logs/NvContainer/*` | small | NvContainer host and message-bus broker logs. |
| `GeForceNOW.json` | 1281 | CEF client switches (`nv-*`). |
| `AppConfigs/v2/*.json` | 7449 to 27863 | GEAR detector config and event schemas, EDGE dictionary, Highlights config. |
| `nvc/localuser/messagebus.conf`, `NvTelemetry.json` | small | Message-bus and telemetry configuration. |
| `storage.json`, `updatestatus.json`, `selfupdate.log` | small | Window placement, update status. |

The logs contain account, session, network, and device identifiers. Every quoted excerpt in these documents replaces those values with `<redacted>`. Raw logs are not committed.
