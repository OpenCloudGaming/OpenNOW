# Windows video decode, codec selection, presentation and pacing

Scope: GeForce NOW Windows 2.0.89.141, `Geronimo.dll` ("gs_04_92"), image base `0x180000000`, plus `NvPixels.dll`. Decompilation used a fully auto-analyzed Ghidra 12.1.4 project, which carries exported C++ names and types (`NVbVideoSettings_t`, `GraphicsContextType`, `DecoderType`, `YUVFormat`). Logs are `geronimo.log` and `geronimo.log.bak` from 2026-10-12; times below are local wall-clock (UTC+2), the second timestamp of each `geronimo.log` line.

Evidence tags: **S** static, **D** decompiled, **L** log, **I** inferred, **U** unknown. Facts marked "macOS Bifrost (PR #1161)" or "macOS Geronimo" come from the macOS research branch, not from Windows.

`Bifrost2.dll` was supplied separately and its network side is covered in [handshake.md](handshake.md), [ANNOUNCE builder](announce-builder.md) and [packet layouts](packet-layouts.md). This file keeps the Geronimo decode and presentation boundary separate.

## Summary

- The Windows selector is the same shared Geronimo code as macOS `chooseMaxSupportedVideoSettings`. On Windows it is the unnamed `FUN_180157f20`. Its signature and branch structure match the macOS function, and its only caller is the exported `platformChooseStreamingSettings` @ `0x18015b210` (**D**).
- Hardware decode is D3D11VA. Geronimo creates `ID3D11VideoDecoder` itself, parses the SPS/PPS with its own `VideoSequenceInfo_*` classes, and picks the DXVA profile GUID. Media Foundation MFTs, DX9Ex/DXVA2 and FFmpeg (`avcodec-59`, loaded at run time) are alternate paths (**D/S**).
- Auto codec preference is AV1, then HEVC, then H.264, for both SDR and HDR. A requested 4:4:4 stream forces HEVC even over an explicit codec. H.264 is capped at 120 fps and at fewer than 4,953,600 pixels whenever H.264 is the requested codec or the only decodable one (**D**, confirmed by **L**).
- The swap chain is flip-sequential with 2 buffers. It uses `R8G8B8A8_UNORM` for 8-bit and `R10G10B10A2_UNORM` for 10-bit, with `ALLOW_TEARING`, a frame-latency waitable object and `SetMaximumFrameLatency(1)`. HDR uses `SetColorSpace1(RGB_FULL_G2084_NONE_P2020)`. No code references `IDXGISwapChain4`, so `SetHDRMetaData` is never called (**D/S**).
- The user's real session at 2026-10-12 19:45 local (S6) ran HEVC 4:4:4 10-bit SDR at 2560x1440@120. Decode output was Y410 (`0x65`), conversion ran in shaders, and the 10-bit swap chain (`0x18`) had vsync off and tearing on (**L**).

## 1. Decoder families and how one is chosen

`platformCreateVideoDecoder` @ `0x18015bc10` (**D**) switches on three inputs.

| Input | Value | Decoder built |
| --- | --- | --- |
| decoder mode 0 or 2 (hardware), graphics context 2 or 4 (DX11), DecoderType 1 (default; 0 is normalized to 1) | | `DXVADecoder(ID3D11Device*)`: D3D11VA |
| same, DecoderType 2 | | `MFTDecoderDX11` |
| hardware mode, other context (DX9Ex) | DecoderType 1 / 2 | `DXVADecoder(IDirect3DDevice9Ex*)` (DXVA2) / `MFTDecoderDX9` |
| decoder mode 1 | | `LAVCDecoder`: software, FFmpeg loaded at run time |
| other | | log "Unknown decoder mode option", returns null |

- RGBA decode is accepted only for hardware mode, DX9 context and DXVA type. Anything else logs "RGBA decode not supported for { mode %u, ctx %u, type %d }" (**D** `0x18015bc10`).
- The user's sessions log `contextType = 4, decoderType = 0` (geronimo.log.bak 19:45:11 "Choose streaming setting i/p params"), so they ran the D3D11 DXVA path (**L**).
- Software decode: `LAVCDecoder` resolves `avcodec_find_decoder`, `avcodec_open2`, `avcodec_send_packet` and `avcodec_receive_frame` by name from libraries named `avcodec-59` and `avutil-57` (**S** `0x1814fd780`, `0x1814fd760`, resolver `0x1803971d0`). Those DLLs are not in the upload. No dav1d or openh264 strings were found (**S**). The selector probes software capability only with `LAVCDecoder::getMaxDecoderCapability(..., codec=1)` (H.264) (**D** `FUN_180157f20`).
- MFT: `MFTEnumEx`, `MFCreateDXGIDeviceManager` and `MFCreateDXGISurfaceBuffer` are resolved from `MFPlat.dll` (**S** `0x180214fb0`). The same module also holds an MFT **encoder** (`MFTVideoEncode`, `CODECAPI_AVLowLatencyMode`, **S** `0x18022e810`). That encoder is the client-side "re-encode to H264" capability behind the GRC gate in section 2 (**I**).
- NVDEC/CUDA is not used for decode. `NvPixels.dll` imports `nvcuda.dll` only for the DeepISP upscaler (section 5) (**S**).

`VideoDecoderSet::CreateAndInitializeDecoder` @ `0x18019bfe0` logs "Failed to create platform decoder[%u]" (**D**). No automatic fallback from DXVA to MFT or LAVC was found at run time. The decoder type comes from settings before creation (**I**, **U** whether a JS-side retry exists).

## 2. Capability probing

`DXVADecoder::getMaxDecoderCapability` @ `0x1801f9da0` (**D**):

- In a DX11 context (2 or 4) it calls `getMaxDecoderCapability_DX11` @ `0x1801f9e90`.
- Otherwise it rejects HDR ("HDR is unavailable with DX9") and 4:2:2/4:4:4 ("YUV422/444 is unavailable with DX9"), then calls `getMaxDecoderCapability_DX9` @ `0x1801fa710`.

`getMaxDecoderCapability_DX11` (**D**):

- `GPUCapabilities::isSupportedDecoderConfig(codec, hdr, yuvFormat)` acts as a remote/config gate first.
- It creates (and caches) a D3D11 device plus `ID3D11VideoDevice`, `ID3D11VideoContext` and `ID3D10Multithread`.
- It picks the output format as follows:
  - 4:2:0 uses `0x67 + hdr` (NV12 or P010).
  - 4:2:2 uses `0x6b + hdr`, logged "Using YUV422 format".
  - 4:4:4 uses `0x64 + hdr` (AYUV or Y410), logged "Using YUV444 format".
- For each candidate resolution, from the top down, it calls `GetVideoDecoderConfigCount` and then `GetVideoDecoderConfig`. `ConfigBitstreamRaw==2` logs "DX11 decoder supports short slice" and `==1` logs "long slice". It then really creates a decoder with `CreateVideoDecoder`. The first success sets `maxDecoderResolution`.
- `GPUCapabilities::isConfigDisabledForGpu` can veto a resolution/fps/codec combination, which logs "Skipping resolution %ux%u@%u with %s for this GPU" (**S** `0x1814c3930`).
- `ignoreMinSpecCheck` gates the AMD path. For H.264 above 0x2a2fff pixels on an AMD GPU that is not on `isGpuInAMD1440pAllowList` and fails an extra test, it logs "Skipping 1440p for this GPU as its not capable of 1440p".

Log for the 4:4:4 probe (geronimo.log.bak 19:45:11) (**L**):

> getMaxDecoderCapability_DX11 for codec HEVC, hdr 1, yuvFormat 2, ignoreMinSpecCheck 0
> Using YUV444 format for codec 2 / HEVC format supported for chroma = 2 / maxDecoderResolution.width = 7680, maxDecoderResolution.height = 4320

The plain HEVC SDR 4:2:0 probe in the same second reported 2560x1440 against a 2560x1440 candidate list. The 10-bit probe (`hdr 1, yuvFormat 0`) reported 3840x2160 (**L**). The earlier session also logged "Skipping resolution 5120x2880/5120x2160@... with H264 for this GPU" and "Platform Supports 240" (**L**).

Gates around the probe:

- `platformSupports444Decode` @ `0x18015cd80` checks remote config `isConfigurationEnabled(vendor, "HEVC", "SDR", ..., 0x1bc)`. It then runs a DXVA probe over `VideoDecoder::getPossibleStreamingResolutions(true)` with `yuvFormat=2` and `hdr = bitDepth > 8`. If the config check fails it logs "YUV444 is disabled for HEVC" (**D**).
- `platformSupportsHDR10Decode` @ `0x18015d970` checks remote config `(codec, "HDR")`. It then runs a DXVA probe at 3840x2160 with `hdr=true`. Auto codec (0 or 7) is tried as HEVC first. If that fails and the AV1-HDR gate `FUN_18015afa0` passes, it tries AV1. If the config check fails it logs "HDR10Decode is disabled for %s" (**D**).
- `platformSupportsVideoTranscode` @ `0x18015dcc0` calls `GPUCapabilities::isTranscodeSettingsSupported(w,h)` and logs "Transcoding %s supported for %dx%d" (**D**). The user's log prints "Transcoding  supported for 2560x1440". The double space is the empty "not" slot (**L**).
- `platformChooseStreamingSettings` snaps fps to {30, 60, 90, 120, 144, 165, 240, 360}: 1–58 → 30, 59–89 → 60, ..., ≥360 → 360. On first use per process it initializes the AMD shader path for codecs 1, 2 and 4 (**D** `0x18015b210`).

## 3. Codec, chroma, bit depth and HDR selection (`FUN_180157f20`)

Signature (**D**): `(NVbVideoSettings_t&, DecoderMode, vector<NVbResolution_t>&, GraphicsContextType, DecoderType, bool ignoreTranscodeSupport-like, YUVFormat, bool, bool)`. It is identical to macOS `chooseMaxSupportedVideoSettings` @ `0xb1de0` (macOS Geronimo). Relevant `NVbVideoSettings_t` fields (**D**, names **I**):

| Offset | Field |
| --- | --- |
| +0 / +2 / +8 | width / height / fps |
| +0x1c | codec: 1 H.264, 2 HEVC, 4 AV1, 0 or 7 Auto |
| +0x20 | SDR colorSpace |
| +0x24 | HDR colorSpace |
| +0x28 | bitDepth |
| +0x2c | chroma: 0 = 4:2:0, 3 = 4:4:4 |
| +0x31 | cloudGsync |
| +0x3c | HDR flag, followed by HDR metadata |
| +0x13c | TrueHDR flag |

Codec values decode as 1 = H.264, 2 = HEVC, 4 = AV1 and 7 = Auto (the earlier "4 and 7 unknown" note is resolved).

Decision table, in execution order:

| # | Condition | Action | Evidence |
| --- | --- | --- | --- |
| 1 | width or height is 0 | 1280x720@60; "Zero resolution provided" | D |
| 2 | not `param_6` | Apply debug overrides: `getCodecOverride`, `userProfile`, width/height/fps overrides. `dynamicMaxBitrate` sets +0x10 per GPU vendor | D |
| 3 | always | Log "Requested streaming settings: ..."; debug `forceVRR` → cloudGsync=1; debug `force444` → chroma=3 | D |
| 4 | width < 1024 or height < 640 (minima of the static table {1280x1024, 1280x800, 1280x720, 1024x768, 1376x640}) | Replace with the table entry whose aspect ratio is closest; "Request resolution less than minimum supported resolution. Clamp resolution to %dx%d" | D (table built from immediates in `FUN_180157f20`) |
| 5 | mode list | Drop modes larger than the request, outside the global limits, or rejected by `FUN_180156420`; sort | D |
| 6 | codec == 0 | codec = 7 (Auto) | D |
| 7 | chroma == 3 | If `platformSupports444Decode(ctx, HEVC, bitDepth)`: **codec = HEVC, replacing any explicit codec**. Otherwise chroma = 0 | D, L (19:45:11 request "codec = H264 ... chroma = 3" → selected "codec: HEVC ... chroma = 3") |
| 8 | decoder mode 1 (software) | Probe `LAVCDecoder` for H.264 only | D |
| 9 | Auto, hardware | Probe H.264 (no HDR), then AV1 and HEVC with the HDR flag. Build a mask (1=H264, 2=HEVC, 4=AV1) and take the largest max resolution | D |
| 10 | explicit codec, hardware | Probe only that codec. H.264 is probed without HDR. Explicit H.264 + HDR logs "Hardware decoding for codec H264 is not supported for HDR" and fails | D |
| 11 | HEVC/AV1 in mask, GRC wanted | If `platformSupportsVideoTranscode` fails, Qualcomm is handled separately. Otherwise, a request under 4,953,600 px, under 240 fps, SDR, under 10-bit and SDR colorSpace not 4 or 5 drops the mask to H.264 only ("HEVC/AV1 supported but transcoding not supported. Disable HEVC/AV1"). Otherwise it logs "Allowing HEVC/AV1 without transcoding" | D |
| 12 | probe failed | Explicit H.264+HDR → `0xc0f11042` "H264 HDR decoding not supported on this setup. No fallback path used."; explicit HEVC → `0xc0f11043`; explicit AV1 → `0xc0f11050`; anything else → 1280x720@60 H.264, "Failed to get decoder capability. Fall back to default resolution" | D |
| 13 | H.264 only in mask **or** codec == H.264 | Cap the pixel budget below 0x4b9600 (4,953,600) and **fps at 120** | D, L (bak 19:35:30: "1920x1080@240, codec = H264" → "1920x1080@120"; "3440x1440@120, codec = H264" (exactly 4,953,600 px) → "No matching aspect ratio found. Select closest" → "2560x1080@120") |
| 14 | mode selection | Pick the largest decodable mode with the same aspect ratio; otherwise "No matching aspect ratio found. Select closest" | D, L |
| 15 | Auto | AV1 if the mask has 4 and the AV1 gate passes; else HEVC if the mask has 2 and the HEVC gate passes; else H.264. In the HDR or ≥240 fps branch, H.264 also clears HDR and caps at 120 ("Select H264 codec and ignore %s %s request"). Gates: with `isConfigValidatorEnabled`, `isConfigurationEnabled(vendor, codec, "SDR"/"HDR", w, h, fps)`; otherwise `FUN_18015ad40(w,h,fps)` (AV1 resolution gate) and `FUN_18015afa0` (AV1 HDR) | D |
| 16 | explicit codec, validator on | If the codec is disabled for w/h/fps/SDR or HDR: "Requested codec %s is disabled for the resolution ..." → cap at 120 fps and retry once; a second rejection returns the codec's error | D |
| 17 | explicit AV1, validator off, fps > 120 | Cap at 120; `FUN_18015ad40` must pass, else `0xc0f11050` | D |
| 18 | GRC | DXVA context with GRC requested: if transcode to 2560x1600 fails (AMD/Intel/Qualcomm, area ≥ 4,953,600) or 3840x2160 fails (area > 0xa8bfff), and the stream is SDR/8-bit/<240 fps/<4.95 MP, log "Disabling GRC because this platform doesn't support re-encoding to H264"; otherwise "Allowing GRC without transcoding" | D |
| 19 | TrueHDR without HDR | Clear TrueHDR. Debug `isTrueHDREnabled` forces HDR and TrueHDR on, with saturation/contrast/middle-grey defaults 200/200/100 | D |
| 20 | color | Covered in color.md §2. HDR colorSpace := 4. HDR needs `platformSupportsHDR10Decode(codec)`; 10-bit SDR also needs remote `enableSDR10` | D |
| 21 | finish | Write three per-codec entries at +0x6c / +0xb0 / +0xf4 (stride 0x44): [selected; H.264 SDR 8-bit (replaced by HEVC when HEVC is decodable and the stream is HDR, or SDR AV1); H.264 SDR 8-bit (HEVC for HDR)]. Set a per-entry GRC byte for codec 1/2/4 | D (meaning of the list **U**) |
| 22 | cloudGsync and fps < `minimumStreamingFpsForCloudGsync` | Clear cloudGsync, "Cloud G-Sync not allowed at %d fps" | D |
| 23 | log | "Resolution selected for streaming: ..." | D, L |

Agreement with macOS: rows 7, 10, 12, 15, 16, 17 and 20 match the macOS decision list point for point (macOS Geronimo `0xb1de0`). Differences come from the probe back end: DXVA/MFT/LAVC on Windows, VideoToolbox masks on macOS. macOS also has its own `hevc120` and `IsAV1Broken` gates (**I** from code identity).

What goes upstream: Geronimo reports depth 10 and chroma 3 for S6, while Bifrost's config has depth 10 and chroma 1. ANNOUNCE field sites `0x1804ef015` and `0x1804ef513` serialize those bytes unchanged when they differ from DESCRIBE (**D/L**). OpenNOW's depth 10 agrees, but its chroma 3 differs. CloudMatch JSON's flags 1/1 are another domain. See [announce-builder.md](announce-builder.md).

## 4. D3D11 decode instance

`DXVADecoderInstance` create @ `0x180201e50` (**D**):

- Profile GUIDs:
  - H.264 uses `H264_VLD_NOFGT` (`0x181419a58`).
  - HEVC 4:2:0 uses `HEVC_VLD_MAIN` (`0x181419d28`) or `HEVC_VLD_MAIN10` (`0x181419d58`), chosen by bit depth.
  - HEVC 4:4:4 uses `HEVC_VLD_Main_444` (4008018f…, `0x1814c31e8`) for 8-bit and `HEVC_VLD_Main10_444` (0dabeffa…, `0x1814c31f8`) for 10-bit.
  - AV1 uses `AV1_VLD_PROFILE0` (`0x181427100`).
  - `HEVC_VLD_Main10_422` (0bac4fe5…, `0x1814c3208`) is present but not selected here.
  - Anything else logs "Unsupported codec %d".
  - GUID names come from the Windows 11 24H2 SDK / FFmpeg (web).
- Decode surface format: 4:2:0 gives NV12 `0x67` or P010 `0x68`; 4:4:4 gives AYUV `0x64` or Y410 `0x65`.
- 4:4:4 on NVIDIA uses a second D3D11 device only when `useMutliDeviceFor444Nvidia` is set and the driver version is above 0xe333. On Intel the gate is `useMutliDeviceFor444Intel`. Otherwise it logs "Using same device for 444".
- `CheckVideoDecoderFormat`, then `GetVideoDecoderConfigCount` / `GetVideoDecoderConfig`, then `CreateVideoDecoder`. It allocates texture-array render targets ("allocating RTArrays"); the user's log shows "pdci->ulNumDecodeSurfaces = 25" (**L**).
- Conversion uses "Using Shaders" (precompiled DXBC, `CS_YUVxA2RGBA` compute shader) when the device supports it. Otherwise it uses the D3D11 video processor ("Shaders not supported, using VPBlit"), with brightness/contrast/hue/saturation filter setup. The user's log shows "Using Shaders" (**L**).
- `DX11FrameFilterHandler` is created for format/primaries/range/10-bit: "DX11FrameFilterHandler initialized for format 0x65, colorPrimaries=1, fullRange=0, is10Bit=1, yuvxa=0" (**L** 19:45:31).

Error and recovery strings:

- `GERONIMO_DX11_DEVICE_REMOVED` / "DXGI_ERROR_DEVICE_REMOVED reason : " and `GERONIMO_DX11_DEVICE_HUNG` @ `0x180281b8d` (**S**).
- `GERONIMO_VIDEODECODER_UNRECOVERABLE_BITSTREAM` @ `0x1800e92a0` (**S**).
- "Invalid SPS change at non-IDR" (**S**).
- "Stream[%u]: Flushing decode queue at frame number: %u" @ `0x1800e1260` (**S**).
- "Error :: Could not recreate VideoBitStreamParser!" (**S**).

Format changes in the log appear as "Stream bit depth changed to: 10", "Stream chroma format changed to: 3" and "Stream colorspace changed to: 2 hdr: 0 range: limited" (**L**). No EOS-VDU handling string was found (**U**).

## 5. Frame filters, upscaling and NvPixels

`FrameFilter` enum (**D** `0x1801644a0` / `0x180386d70`):

| Value | Filter | Label |
| --- | --- | --- |
| 0 | Linear | |
| 1 | Lanczos | `hq` |
| 2 | DeepISP | `ai` |
| 3 | NvScaler | `hq++` |
| 4 | NvSharp | `hq+` |
| 5 | Shepherd3 | `s3` |
| 6 | Shepherd6 | `s6` |

`platformSupportsFrameFilter` @ `0x18015d170` (**D**):

- Linear is always available.
- Lanczos and Shepherd3/6 need a DX11 context and a shader/format check `FUN_180282180(0, NV12)`.
- NvScaler and NvSharp additionally need an NVIDIA GPU (error 1 otherwise).
- DeepISP needs a DX11 context and the "DISP capability check" in `0x180210480`. That check loads `nvcuda.dll` and requires CUDA ≥ 11.2 ("DISP Min CUDA version not met [11020]"), a minimum driver, shaders ("DISP only supported with shaders") and `NvPixels.dll` exporting `GetObj`/`DeleteObj`.
- At run time DeepISP needs NV12 8-bit input ("DISP not supported: requires NV12 (8-bit), got format 0x%x").

`NvPixels.dll` holds `DeepIspUpscaler` and CUDA 12.8 fatbin kernels `nv::UpscaleLumaFull<...>` (luma-only AI upscale, **S**). It is not RTX Video Super Resolution: no VSR or NGX strings exist (**S**). If NvScaler, NvSharp or DISP fails, `DX11FrameFilterHandler` falls back to Lanczos ("NvScaler failed, falling back to Lanczos", "DISP failed, falling back to Lanczos") (**S** `0x180226a3f`).

TrueHDR is a server-side SDR→HDR filter, set through `GERONIMO_SET_TRUEHDR_PARAMS` (**S**). The server prefilter is negotiated separately: "Prefilter:: Client request: Mode: 0 ... Server response: Mode: 2 ... Finalized Mode: 0" (**L** 19:45:29).

## 6. Presentation (DXRenderer)

`createDx11SwapChain` @ `0x18026cc80` (**D**, vtable offsets checked against the dxgi1_2/1_3/1_4 layouts):

| Setting | Value |
| --- | --- |
| Factory call | `IDXGIFactory2::CreateSwapChainForHwnd` (vtable +0x78) |
| Format | `0x1c` `R8G8B8A8_UNORM`, or `0x18` `R10G10B10A2_UNORM` when the renderer's 10-bit flag (+0xdd) is set |
| BufferCount | 2 (3 on Qualcomm with vsync off and tearing available) |
| Scaling | `DXGI_SCALING_NONE` (1) on the flip path |
| SwapEffect | `FLIP_SEQUENTIAL` (3) on Windows build ≥ 9200; `SEQUENTIAL` (1) otherwise, and also when vsync is off and tearing is unsupported |
| Flags | `ALLOW_TEARING` (0x800) when `allowTearingSwapChainFlag` and tearing are supported, or when vsync is off and tearing is supported; `FRAME_LATENCY_WAITABLE_OBJECT` (0x40) when not fullscreen-exclusive and build ≥ 9600; `ALLOW_MODE_SWITCH` (0x2) in FSE |
| After create | `IDXGISwapChain2::SetMaximumFrameLatency(1)` (+0xf8), `GetFrameLatencyWaitableObject` (+0x108), "FrameLatencyWaitableObject OK." |
| Color space | `IDXGISwapChain3::SetColorSpace1(pq ? 12 : 0)` (+0x130), meaning `RGB_FULL_G2084_NONE_P2020` or `RGB_FULL_G22_NONE_P709`, only on build > 18361 and in the resize path that runs after the first create (**D**; the IID xref is unique, `0x18026e2a9`) |
| Window | `MakeWindowAssociation(DXGI_MWA_NO_ALT_ENTER)`; optional `SetFullscreenState` with "Disabling FSE!!" on failure |
| Refresh | Computes the display vsync interval from `IDXGIOutput::GetDesc` and the display config ("Swap chain's display has vsync interval %ldus") |
| VRR | On build ≥ 10240 with VRR on, `FUN_18026c4d0` calls `NvAPI_D3D_SetVRRState` |

Present parameters, `FUN_180273f00` (**D**):

- vsync on: SyncInterval 1, flags 0.
- vsync off: SyncInterval 0, plus `DXGI_PRESENT_ALLOW_TEARING` (0x200) when tearing is supported and the window is not FSE. Otherwise it emits a debug analytics event "AllowTearing/DirectX11".

Log (**L**), geronimo.log.bak 19:45:31, the session start:

> createDx11SwapChain with ... width = 2560, height = 1440, vsync = 0, pq = 0, bit depth = 10
> Old Width = 2560, Height = 1440, Format = 0x1c, BufferCount = 2 / New ... Format = 0x18, BufferCount = 2
> Swap chain's display has vsync interval 6057us.

That is a 165 Hz display.

The swap chain starts 8-bit and switches to 10-bit after the first decoded 10-bit frame. Every session in both logs logged `pq = 0`. No HDR session is in the logs.

Other presenters:

- `createDirectCompositionSwapchain` @ `0x1802bf1a0` uses the Windows composition swap-chain API (`CreatePresentationFactory`, `DCompositionCreateDevice2`, `DCompositionCreateSurfaceHandle`). It creates its own buffers as `B8G8R8A8_UNORM` (`0x57`) or `R10G10B10A2` (`0x18`) textures with misc flags 0x300802 (shared NT handle, displayable, exclusive writer) (**D/S**). It is not used in either log (**L**: 0 hits).
- D3D9Ex (`Direct3DCreate9Ex`, `CreateAdditionalSwapChain`, `ResetEx`) and an OpenGL path (`wglGetProcAddress`, a GLSL YUV shader with `yuv_matrix`) are present as legacy/alternate presenters (**S**).
- `d3dcompiler_47.dll` is not referenced by Geronimo. Its shaders are embedded DXBC, so the DLL ships for CEF (**S**, **I**).

## 7. Frame pacing and latency

- `DX11AsyncVideoFrameRenderer::renderLoop` @ `0x18027b4b0` (**D**):
  - It raises its thread to priority 15 (`THREAD_PRIORITY_TIME_CRITICAL`).
  - Per iteration it asks `AsyncFrameQueue::shouldBlockOnFrames(vsyncInterval)`, then either blocks in `waitForNextFrame(100 ms)` or takes `nextFrame`.
  - It computes `renderScaleFactor` and `frameFilterForRender`.
  - In cinematic mode it presents each frame for `cinematicPresentIntervalsForFrame` vsyncs, split into chunks of at most 4 (the DXGI SyncInterval limit). With VRR it uses `vrrPresentDurationForFrame`.
  - It feeds back `setGPUTime` and `setQueueDepth`.
- `AsyncFrameQueue::push` @ `0x1801c3d10` returns an `NvstClientDJBMode_t`, so the queue drives the NVST de-jitter-buffer mode (**S/D**). Logs show "Sending DJB Config at frame 1. Mode: FIXED" and "Received ADJB response from geronimo; mode 3" (**L**).
- Server-side pacing: "Set vsync interval to: %u" via `BifrostSDKExecutor::setVsyncInterval`. Settings include `VVSync` ("Pace server frames to match client vsync"), `cloudGsync`, `enableVrrDetectionV2`, `minimumStreamingFpsForReflexWithoutVrr` and `vrrDisplayWar` (**S**). The log has "Set vsync interval to: 0" and the "GsyncInfo" metric (**L**).
- A render benchmark gates fps: "Render benchmark completed. p95 GPU time 3556us. MaxFPS: 120. Threshold 6666us. Affordable: 1" (**L** 19:45:35).
- `timeBeginPeriod` is imported (**S**). `GetFrameStatistics` is not called through a recognizable path (**U**).
- Drops: "Total video frames dropped : 99 / Total missed Vsyncs : 135797" (bak 19:33:41) and "111 / 175723" (geronimo.log 20:15:24). Both sessions ran with vsync off and tearing on. The counter's exact definition was not traced, so a high missed-vsync count is not evidence of visible stutter (**L**; definition **U**).

## 8. What the user's real sessions show (**L**)

| Item | S6 (2026-10-12 19:45 local) session (geronimo.log.bak) |
| --- | --- |
| Request | 2560x1440@120, codec H.264 (UI default), bitDepth 10, chroma 3, SDR colorSpace 2, HDR off |
| Selected | 2560x1440@120 **HEVC**, 10-bit SDR, chroma 3 (4:4:4), "Will request 10 bit SDR video for codec HEVC." |
| Decode | DX11 DXVA, `HEVC_VLD_Main10_444`, Y410 (`0x65`), shaders, 25 surfaces |
| Present | R10G10B10A2, flip, 2 buffers, vsync 0 + tearing, pq 0 |
| Filters | Linear (FrameFilter 0); server prefilter mode 2 offered and finalized to 0 |
| Pacing | DJB FIXED, client vsync interval 0, render benchmark affordable at 120 |

The capability sweep at 19:35:30 (bak) shows H.264 requests at 240/360 fps resolving to @120 and HEVC HDR requests resolving to 10-bit HDR with no cap (for example "1920x1200@360, codec: HEVC ... HDR: 1 ... bitDepth = 10").

## 9. Windows vs macOS (video)

| Topic | Windows (this analysis) | macOS (macOS Geronimo / video.md) |
| --- | --- | --- |
| Selector | `FUN_180157f20`, same branches | `chooseMaxSupportedVideoSettings` `0xb1de0` |
| HW decode | D3D11VA with Geronimo's own SPS/PPS parser and DXVA picture params; DX9 DXVA2 legacy | VideoToolbox decompression session |
| Alternate decoders | MFT (DX11/DX9), FFmpeg `avcodec-59` (software, H.264 probe only) | VideoToolbox software mask |
| Capability probe | Creates real `ID3D11VideoDecoder`s per resolution; config-disabled list per GPU; long/short slice | `VTIsHardwareDecodeSupported` masks; `hevc120`; AV1-broken OS check |
| 4:4:4 | DXVA `HEVC_VLD_Main(10)_444` → AYUV/Y410; optional second device on NVIDIA/Intel | `444v`/`x444` pixel buffers after a probe |
| Output conversion | DXBC shaders or `VideoProcessorBlt` | Metal shaders with `GetColorTransferMatrix` |
| Swap chain | DXGI flip-sequential, 2 buffers, RGBA8/RGB10A2, tearing, waitable, max latency 1 | CAMetalLayer, pixel format 0x50/0x5e |
| HDR output | `SetColorSpace1(G2084_P2020)`, no `SetHDRMetaData` | EDR layer + `CAEDRMetadata` from stream SEI |
| Upscale | Lanczos/NvScaler/NvSharp/Shepherd/DeepISP (CUDA, NV12 only) | not compared here (**U**) |
| VRR | `NvAPI_D3D_SetVRRState`, cloud G-Sync, VVSync | display-sync flag in Metal path |

## 10. Agreement with OpenNOW PR #1165 (merged to dev as 34f99765)

Agrees:

- 4:4:4 forces HEVC over an explicit codec, otherwise it drops to 4:2:0 and keeps the bit depth (row 7).
- An explicit codec that cannot decode at all fails (row 12).
- 10-bit SDR falls back to 8-bit BT.709 when 10-bit decode is missing (color.md §2).

Disagrees:

1. **H.264 cap.** Windows caps H.264 at 120 fps and below 4,953,600 px whenever the codec is explicit H.264 or H.264 is the only decodable codec (row 13). PR #1165 caps only Auto-HDR-to-H.264 above 120 fps. Its note "GFN also caps non-HDR H.264 at 240+ fps" understates the rule.
2. **Auto order under HDR.** Windows Auto picks AV1 before HEVC in both the SDR and the HDR branch (row 15), subject to the AV1 config/HDR gates. PR #1165 tries `["h265","av1"]` for HDR. HEVC-before-AV1 is only the internal order of `platformSupportsHDR10Decode` for an Auto codec, and the selector never calls it with Auto because the codec is concrete by then.
3. **Explicit H.264 + HDR.** Windows returns `0xc0f11042` (row 12). PR #1165 deliberately downgrades to SDR. This is documented in the PR.
4. **Display gate.** `FUN_180157f20` never consults display HDR. PR #1165 step 1 clears HDR when the window has no HDR output. Where Windows gates HDR on the display is outside this function (DisplayUtil, see color.md §3) (**U** where the gate is enforced).
5. **Remote-config gates.** Windows also requires remote config (`isConfigurationEnabled`, `enableSDR10`, `isSupportedDecoderConfig`, per-GPU disabled configs). OpenNOW has no equivalent (**I**: acceptable, since OpenNOW cannot fetch NVIDIA's remote config).
6. **Fallback list.** Windows writes three per-codec setting entries into `NVbVideoSettings_t` (row 21). OpenNOW sends one profile (**U** whether the server uses them).
