# Windows color, HDR and display capability handling

Same scope, tags and image base as video.md. Function VAs are in `Geronimo.dll`.

## Summary

- Geronimo's color-space numbers can be named from Windows code. `QueryMonitorColorSpace` @ `0x1802650d0` maps every `DXGI_COLOR_SPACE_TYPE` to an internal value from 0 to 9 (YCbCr 601/709/2020 and RGB 709/2020, each limited or full). With that table, the stream values read as SDR 2 = YCbCr BT.709 limited and HDR 4 = YCbCr BT.2020 limited. This is consistent with macOS (**D** mapping, names **I**).
- The bit-depth, 10-bit SDR and HDR rules in the Windows selector match macOS line for line. HDR needs `platformSupportsHDR10Decode`. 10-bit SDR additionally needs remote `enableSDR10`. Failure gives 8-bit BT.709 (**D**).
- A display counts as HDR-capable only with all of these: OS advanced-color support (`DisplayConfigGetDeviceInfo` type 9 bit 0), plus either a BT.2020 output color space or `MaxLuminance` > 400 nits (**D**). "HDR enabled" needs advanced-color bit 1 and a BT.2020 output color space. The SDR white level comes from `DISPLAYCONFIG_SDR_WHITE_LEVEL`, default 200 nits (**D**).
- HDR output means PQ BT.2020 in an `R10G10B10A2` swap chain via `SetColorSpace1(12)`, with no DXGI HDR metadata. HDR10 metadata goes to the server, built from the monitor's `DXGI_OUTPUT_DESC1` (`SDLWindow::getHDRMetadata`). SDR UI layers (SpriteBatch, ImGui) are PQ-encoded at `sdrWhitePoint` (**D/S**).
- The user's logs show only SDR sessions. They were 8-bit, or 10-bit SDR 4:4:4 with stream color space 2 and limited range (**L**).

## 1. Color-space numbering

`QueryMonitorColorSpace` @ `0x1802650d0` calls `IDXGIOutput6::GetDesc1` (vtable +0xd8) on the window's output. It stores the primaries, white point, min/max/max-full-frame luminance and `BitsPerColor` in the monitor record at +0x90..+0xbc. It then sets the record's color value at +0xc0 from `DXGI_OUTPUT_DESC1.ColorSpace` (**D**):

| Internal value | DXGI color spaces mapped to it | Reading (**I**) |
| --- | --- | --- |
| 0 | 6 YCBCR_STUDIO_G22_LEFT_P601 | YCbCr BT.601 limited |
| 1 | 7 YCBCR_FULL_G22_LEFT_P601 | YCbCr BT.601 full |
| 2 | 8, 22 YCBCR_STUDIO_*_P709 | YCbCr BT.709 limited |
| 3 | 5, 9 YCBCR_FULL_*_P709 | YCbCr BT.709 full |
| 4 | 10, 13, 15, 16, 18, 23, 24 YCBCR_STUDIO_*_P2020 | YCbCr BT.2020 limited |
| 5 | 11, 19 YCBCR_FULL_*_P2020 | YCbCr BT.2020 full |
| 6 | 2, 20 RGB_STUDIO_*_P709 | RGB BT.709 limited |
| 7 | 0, 1 RGB_FULL_*_P709 | RGB BT.709 full |
| 8 | 3, 14, 21 RGB_STUDIO_*_P2020 | RGB BT.2020 limited |
| 9 | 12, 17 RGB_FULL_*_P2020 | RGB BT.2020 full |
| -1 | other | unknown |

Cross-checks:

- The macOS monitor helper writes display color 7 or 9, depending on `canRepresentDisplayGamut` (macOS Geronimo, color.md). That is RGB 709 full or RGB 2020 full in this table.
- macOS `Nsk::cast(NVbColorSpace_t)` bounds stream values to 0–5, the YCbCr half of the table.
- The Windows selector normalizes SDR to 2 and HDR to 4 (§2). The user's decoder log reports "Stream colorspace changed to: 2 hdr: 0 range: limited" for a 10-bit SDR stream requested with SDR colorSpace 2 (**L** geronimo.log.bak 19:45:31).

So 2 = BT.709 limited and 4 = BT.2020 limited is well supported. It is still inferred, because no symbol names the enum.

Decoder-side matrix field: `FUN_180206460` (a `DXVADecoderInstance` virtual, called from create @ `0x180201e50`) reads a matrix code from the decode parameters at +0x24 and writes +0x324 (**D**):

- 9 → 0 (BT.2020)
- 1 → 1 (BT.709)
- 6 → 2 (BT.601)
- 2 → 1, with "Warning :: colorspace not specified"
- anything else → `GERONIMO_DXVADECODER_INVALID_PARAMS` (`0xc0f1103d`)

The inputs are ITU-T H.273 `matrix_coefficients` codes (**I**: 1 = BT.709, 2 = unspecified, 6 = SMPTE 170M, 9 = BT.2020 NCL). Unspecified is treated as BT.709.

## 2. Bit depth, 10-bit SDR and HDR in the selector

Tail of `FUN_180157f20` (**D**), after the codec is fixed:

1. TrueHDR without HDR is cleared ("TrueHDR requested without HDR, disabling TrueHDR"). Debug `isTrueHDREnabled` forces HDR and TrueHDR on.
2. HDR colorSpace (+0x24) is set to 4 unconditionally.
3. If HDR is requested, `platformSupportsHDR10Decode(ctx, codec)` must pass. If it does, bitDepth becomes max(bitDepth, 10), the HDR metadata block at +0x3c..+0x6b is zeroed with the HDR flag kept, and SDR colorSpace is set to 2. If it fails, HDR is cleared and the SDR path runs.
4. SDR path: the 10-bit branch runs when SDR colorSpace is 4 or 5, or bitDepth > 9. Debug `forceSDR10` sets 4/10. The branch needs `platformSupportsHDR10Decode(ctx, codec)` (a 10-bit decode probe at 3840x2160) **and** `GeronimoSettingsImpl::enableSDR10()`.
   - Success: "Will request 10 bit SDR video for codec %s." SDR colorSpace keeps 2, 3 or 5; any other value becomes 4. bitDepth = max(bitDepth, 10).
   - Failure: "10 bit video requested but 10 bit SDR is disabled via RemoteConfig..." or "10 bit video requested, this system does not support 10 bit decode for codec %s. Falling back to 8 bit BT.709." bitDepth = 8, SDR colorSpace = 2.
5. Without a 10-bit request: bitDepth = 8, SDR colorSpace = 2.

This matches macOS color.md §"Requested HDR and 10-bit SDR are separately gated" exactly. On Windows, `platformSupportsHDR10Decode` @ `0x18015d970` probes DXVA with `hdr=true` (P010/Y410 output). The macOS version calls VideoToolbox `isHDR10Capable` (**D** both).

Log (**L**, bak 19:45:11): "Requested streaming settings: 2560x1440@120, codec = H264, SDR colorSpace = 2, HDR colorSpace = 4, bitDepth = 10, chroma = 3, HDR: FALSE" → "Resolution selected for streaming: 2560x1440@120, codec: HEVC, ... HDR: 0, SDR colorSpace: 2, HDR colorSpace: 4, bitDepth = 10, chroma = 3". Bifrost then logs "HDR:: enableHDR: 0, sdrColorSpace: 2, hdrColorSpace: 4, bitDepth: 10" (19:45:29).

## 3. Display HDR detection (DisplayUtil)

Monitor population is `FUN_180264000(HMONITOR, Monitor*)` (**D**). It calls `GetMonitorInfoA` and `EnumDisplaySettingsA` (mode and refresh), then:

- `FUN_180265520(hmon, &hdrSupported, &hdrEnabled)`:
  - `DisplayConfigGetDeviceInfo` with type 9 (`GET_ADVANCED_COLOR_INFO`, size 0x20) for the monitor's target path ("Failed to query monitor target path for HDR detection").
  - `CreateDXGIFactory1` (IDXGIFactory6 IID) and enumeration to the output whose `DXGI_OUTPUT_DESC.Monitor` matches, then `IDXGIOutput6::GetDesc1`.
  - `wideGamut` = output ColorSpace < 24 and in mask `0xa27408`. The mask holds DXGI 3, 10, 12, 13, 14, 17, 21, 23: every BT.2020 space except full-range YCbCr 2020 and the TOPLEFT/HLG variants.
  - `hdrSupported` = advancedColorSupported (bit 0) and (`MaxLuminance` > 400.0 (`0x1814e9644`) or `wideGamut`).
  - `hdrEnabled` = advancedColorEnabled (bit 1) and `wideGamut`.
  - Failure logs "Failed to get HDR info for monitor: %s" and clears both flags.
- If HDR is enabled, `DisplayConfigGetDeviceInfo` type 11 (`GET_SDR_WHITE_LEVEL`, size 0x18) → `SDRWhiteLevel * 80 / 1000` nits. Failure logs "Failed to get SDR white point for HDR monitor" and uses 200.0 nits.
- DPI/scale via `FUN_180175480`, the device string from `EnumDisplayDevicesA`, then `QueryMonitorColorSpace` (§1) for luminance, primaries and the color value.

`SDLWindow::getHDRMetadata(NVbHDR10Settings&)` @ `0x180168190` finds the window's monitor via `DisplayUtil::GetMonitorFromWindow`. It copies that monitor's DXGI primaries, white point and luminance into `NVbHDR10Settings`, or zeros on failure (**D**). The caller is `0x18013f8f7`. The data feeds the server request (HDR metadata keys `displayPrimaryX0..Y2` and `displayWhitePointX/Y` @ `f1801281d0`, **S**).

Geronimo therefore sends the **display's** measured volume upstream. It does not pass stream SEI to DXGI (**I**).

Logs show 19 × "ERROR [DisplayUtil] Failed to get monitor handle for window handle" (**L**). The HDR display query can fail transiently, and the code then falls back to "not HDR" (**I**).

## 4. Output color space and swap-chain format

- `createDx11SwapChain` @ `0x18026cc80` (**D**):
  - Format is `R10G10B10A2_UNORM` when the renderer's 10-bit flag (+0xdd) is set, else `R8G8B8A8_UNORM`.
  - On Windows build > 18361 it QIs `IDXGISwapChain3` and calls `SetColorSpace1(pq ? DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 (12) : DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 (0))`. pq is renderer byte +0xdc.
  - No scRGB / `R16G16B16A16_FLOAT` path was found in the DXGI or composition swap chains. The composition path uses `B8G8R8A8` or `R10G10B10A2` (**D** `0x1802bf1a0`).
- `IDXGISwapChain4` (`SetHDRMetaData`) has no code reference. Its IID appears only in a static GUID table at `0x181426fa0` (**S**). DXGI HDR metadata is not set.
- 10-bit SDR is presented as `R10G10B10A2` with `G22_P709`. The log shows the switch from `Format = 0x1c` to `0x18` with `pq = 0, bit depth = 10` (**L** bak 19:45:31). No dithering shader was identified for the 8-bit swap chain (**U**).
- UI overlays in HDR: `DXRenderer` creates "SpriteBatch HDR shader" from DXBC @ `0x1814ea300`. Its `HDRParams` cbuffer holds `sdrWhitePoint`, and the blob carries ST 2084 constants (m2 = 78.84375, c1 = 0.8359375, ...) (**S/D** `0x180270460`). `ImGui_ImplDX11_CreateDeviceObjects` @ `0x180288320` uses a pixel shader with the same `HDRParams.sdrWhitePoint` and PQ constants (`0x1814eeb40`). SDR UI is PQ-encoded at the SDR white level, the Windows counterpart of macOS assigning a PQ color space to the ImGui renderer (**I**).
- The MFT encoder path holds a "luma/chroma" DXBC with PQ constants (`0x1814e1b40`, used from `0x18022e810`). This looks like tone mapping or conversion for client-side H.264 re-encode (**I**).

## 5. YUV → RGB conversion

- The DX11 decode path converts in precompiled DXBC shaders (`CS_YUVxA2RGBA` compute shader and pixel shaders), or with `VideoProcessorBlt` when shaders are unavailable (**D** `0x180201e50`, **L** "Using Shaders").
- `DX11FrameFilterHandler` is configured per frame format with `format`, `colorPrimaries`, `fullRange` and `is10Bit`, and rebuilds shaders on change ("reconfigure - Failed to recreate shaders for new format") (**S**). The user's 10-bit SDR stream: "format=3, colorPrimaries=1, fullRange=0, is10Bit=1" and "format 0x65" (Y410) (**L**).
- Coefficient tables (**S**):
  - `0x181426c30`–`0x181426e50` hold BT.601, BT.709 and BT.2020 sets, each in limited/full and 8/10-bit variants. Examples: 1.164384 / 1.596027 (601 limited 8-bit), 1.167808 (= 1023/876, 10-bit limited luma scale), 1.792741 / 2.112402 (709 limited), 1.5748 / 1.8556 (709 full), 1.678674 / 2.141772 (2020 limited), 1.4746 / 1.8814 (2020 full).
  - Referenced from `0x18015c890` and `0x18015bc10`.
  - A second copy at `0x1814bfc38`–`0x1814bfe08` is used by `0x1801dc880`, `0x1801dcbd0`, `0x1801da440` and `0x1801da960`.
  - Further copies at `0x1814c2870`–`0x1814c2a20`, `0x1814c5358`–`0x1814c54d8` and `0x1814c7ad0`–`0x1814c7c50` lie outside the DXBC blobs. Their users were not traced (**U**).
- Which table the D3D11 shader path picks for a frame was not traced to the instruction (**U**). The selection inputs are the H.273 matrix (+0x324), range and bit depth (**D** for inputs).
- Chroma siting handling was not found in the shaders' reflection names (**U**).

## 6. HDR end to end (Windows, from static evidence)

1. UI requests HDR. The selector keeps HDR only if `platformSupportsHDR10Decode` passes, and then forces at least 10-bit (§2). Display capability is detected by DisplayUtil (§3). Where the UI or JS uses it to gate the request is **U**.
2. Display HDR10 metadata from `DXGI_OUTPUT_DESC1` goes upstream in `NVbHDR10Settings`. TrueHDR (server-side SDR→HDR) has its own parameters (`GERONIMO_SET_TRUEHDR_PARAMS`).
3. Decode to P010 (4:2:0) or Y410 (4:4:4). The decoder event "Stream colorspace changed to: %d hdr: %d range: %s" carries the stream state (**S/L**).
4. Present through a 10-bit swap chain with `SetColorSpace1(G2084_P2020)`, PQ-encoded UI at `sdrWhitePoint`, and no `SetHDRMetaData`.
5. No HDR session exists in the supplied logs, so steps 3 and 4 are not log-confirmed for PQ (**U**).

## 7. Windows vs macOS (color)

| Topic | Windows | macOS (macOS Geronimo, color.md) |
| --- | --- | --- |
| Stream color enum | 2 / 4 normalization; full 0–9 table recoverable from `QueryMonitorColorSpace` | 0–5 bounds via `Nsk::cast`; 2 / 4 normalization |
| 10-bit SDR gate | `platformSupportsHDR10Decode` + `enableSDR10` | same |
| HDR decode gate | DXVA probe at 3840x2160 with `hdr=true`, plus remote config "HDR" | VideoToolbox `isHDR10Capable`, effectively HEVC only |
| Display HDR test | advanced-color bits + BT.2020 output color space or > 400 nits | EDR headroom > 1.0 |
| SDR white | `DISPLAYCONFIG_SDR_WHITE_LEVEL` (default 200 nits) | EDR headroom-derived |
| Output | RGB10A2 + `SetColorSpace1(G2084_P2020)` | CAMetalLayer 0x5e + ITU-R 2100 PQ color space + EDR metadata |
| Output HDR metadata | none at DXGI | `CAEDRMetadata` from stream content-light/mastering attachments |
| Metadata upstream | monitor `DXGI_OUTPUT_DESC1` → `NVbHDR10Settings` | monitor fields from EDR headroom ×100/×50 |
| Overlay in HDR | SpriteBatch/ImGui PQ shaders with `sdrWhitePoint` | ImGui renderer assigned PQ color space |
| Range source | decoder matrix code + `fullRange` flag in FrameFilterHandler | CVPixelBuffer fourcc; `VideoFrame[0x44]` byte never written |

## 8. Comparison with OpenNOW

- **Display detection.** OpenNOW `opennow-qt/src/streaming/rendering/WindowsHdrDisplay.cpp` uses only `IDXGIOutput6::GetDesc1` (color space, luminance, primaries). GFN also requires the advanced-color bits from `DisplayConfigGetDeviceInfo` and reads the SDR white level (§3).
- **Output.** OpenNOW prefers scRGB and has HDR10 as a fallback (docs/hdr.md). GFN uses only HDR10/PQ in RGB10A2 and never sets DXGI HDR metadata. Both are valid. GFN's choice means HDR10 output is the configuration NVIDIA ships.
- **10-bit SDR.** Both present 10-bit SDR as RGB10A2 BT.709 (agrees).
- **Upstream metadata.** OpenNOW sends measured display luminance (and primaries when validated) or 1000/400/0 defaults (docs/hdr.md). GFN sends the monitor's `DXGI_OUTPUT_DESC1` values, and the zeroed-on-failure branch suggests it sends zeros rather than defaults when the monitor lookup fails (**D** `0x180168190`; server handling **U**).
