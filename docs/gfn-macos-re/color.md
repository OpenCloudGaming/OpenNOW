# Mac color, HDR, and display capability handling

Geronimo distinguishes requested color settings, decoded-image metadata, renderer format classification, and display capability. These values are not interchangeable. Metal chooses its matrix and range from the image buffer, while stream-property reporting uses a separate frame-object representation.

## Scope and evidence

This analysis covers `archive/libGeronimo.dylib`, x86_64, SHA-256 `f564aa1910f1a366973ce3c0f0e6261122833bc41c530a669ec846a51d50d15a`. Addresses are thin Mach-O virtual addresses. See [artifacts](artifacts.md), [method](method.md), and [video handling](video.md).

**D** identifies decompiled control flow or decoded data. **S** identifies a symbol or string. **I** identifies an inference. **U** identifies an unresolved question. Runtime behavior is unverified on the Linux analysis host.

## Proven mappings have a named domain

**D.** `_decodeCallback` reads `kCVImageBufferYCbCrMatrixKey` from a successful decoded image and stores a `PlatformColorSpace` classification. `VTDecoder::decode` copies that classification into its `VideoFrame`, and `FrameRenderer::colorSpace` reads the frame's field at offset `0x40`.[^C1][^C2][^C3]

| PlatformColorSpace result | Proven callback condition |
| --- | --- |
| 1 | Attachment compares equal to `kCVImageBufferYCbCrMatrix_ITU_R_601_4`. |
| 2 | Attachment compares equal to `kCVImageBufferYCbCrMatrix_ITU_R_709_2`. |
| 4 | Attachment compares equal to the `ITU_R_2020` literal. |
| 5 | The matrix attachment is absent. |
| 0 | The attachment exists but matches none of those comparisons; the callback logs "Unknown color space!". |

This table reconstructs callback output values, not a complete declaration of `PlatformColorSpace`. No name for value 3 was proven.[^C1]

**D.** `VideoDecoderThread::run` obtains matrix, PQ, and full-range properties from a returned renderer. It constructs the `NVbColorSpace_t` argument for its IOInterface property notification by OR-ing the full-range boolean with base 2 for PlatformColorSpace 2 or base 4 for PlatformColorSpace 4. Other classifications use base 0.[^C4]

| Constructed notification value | Proven meaning in that branch |
| --- | --- |
| 2 | BT.709 classification and false full-range flag. |
| 3 | BT.709 classification and true full-range flag. |
| 4 | BT.2020 classification and false full-range flag. |
| 5 | BT.2020 classification and true full-range flag. |
| 0 or 1 | Any other or unclassified matrix, with false or true full-range flag respectively. |

**D.** Exact vtable bytes at `__ZTV10VTRenderer`, `0xf25858`, resolve the thread's matrix, PQ, depth, chroma, and range dispatches to `FrameRenderer::colorSpace`, `VTRenderer::isPQ`, `is10Bit`, `chromaFormat`, and `FrameRenderer::isFullRange`. The last method reads the frame's byte at `0x44`.[^C5][^C6]

**U.** Values 0 and 1 are not assigned canonical BT.601 enum names here. The branch collapses absent and unknown matrices into those same values. The table describes construction of an API notification, not the wire enum encoding.

**D.** `Nsk::cast(NVbColorSpace_t)` preserves input values 0 through 5 and replaces higher unsigned values with 0 after logging an undefined-value error. `convertToStreamingParams` independently bounds both requested color fields to 0 through 5. Neither fact supplies names for all six values.[^C7][^C8]

## Requested HDR and 10-bit SDR are separately gated

**D.** `chooseMaxSupportedVideoSettings` logs and processes separate SDR color, HDR color, bit-depth, chroma, and HDR fields. Its normalization code sets the HDR color field to numeric value 4. The ordinary SDR path ends with value 2 and 8-bit depth unless the 10-bit request survives the capability and configuration gates.[^C9]

**D.** A requested SDR depth above 9 or SDR color value 4 or 5 enters the 10-bit path. A debug force-SDR10 option can also set value 4 and depth 10. The path requires `platformSupportsHDR10Decode` and `GeronimoSettingsImpl::enableSDR10`. Failure returns to numeric color 2 and depth 8. The branch's own diagnostic calls this fallback "8 bit BT.709"; the actual writes, not the diagnostic alone, establish the policy.[^C9]

**D.** An allowed 10-bit SDR request preserves SDR values 2, 3, or 5; other values normalize to 4. It sets depth to at least 10. An HDR request also needs the decode-capability check. Failure clears HDR and enters SDR normalization. Success retains HDR, sets depth to at least 10, and resets the HDR metadata fields in this helper.[^C9]

**D.** `platformSupportsHDR10Decode` checks remote configuration for the codec and the HDR configuration. Its VideoToolbox capability call requires graphics-context numeric value 3. Automatic codec values 0 or 7 are first tried as HEVC, with a separately gated AV1 check afterward. In this binary, `VTDecoder::isHDR10Capable` returns false for every codec except HEVC value 2; HEVC returns an `IsMacModelAtLeast` result using an embedded model list.[^C10][^C11]

**U.** These are shipped capability-policy decisions, not proof that VideoToolbox cannot decode AV1 HDR or that the display cannot show a particular format. The model-list enum meanings and remote configuration values were not reconstructed. The meaning of requested bit depths above 10 is also unresolved.

## Pixel format carries bit depth, chroma, and range

**D.** `VTRenderer::is10Bit` returns true for `x420`, `xf20`, `x444`, and `xf44`. Its chroma predicate returns numeric value 3 for `444v`, `444f`, `x444`, and `xf44`, and 0 for other image formats or no image. The independent `Nsk::cast(NVbChromaFormat_t)` returns whether its input equals 3.[^C12][^C13][^C14]

**D.** `GetColorTransferMatrix` uses the actual CVPixelBuffer fourcc to select 8-bit versus 10-bit handling and full-range versus video-range tables. The recognized full-range formats are `420f`, `444f`, `xf20`, and `xf44`; `x420` and `x444` select 10-bit handling without the full-range branch. Other formats take the default 8-bit video-range handling in this helper.[^C15]

**D.** The helper compares the buffer's matrix attachment against BT.601 and BT.2020 constants. Matching values choose the corresponding named YUV coefficient tables. The remaining branch chooses BT.709 tables. Full-range selection chooses the corresponding `Full10` table symbol, including when the input is 8-bit. This describes the compiled table selection and does not assume different coefficients merely because a symbol contains "10".[^C15]

**U.** Exact matrix coefficients and SIMD adjustment-vector lanes were not reconstructed. The decompiler's SIMD temporaries are insufficient evidence for an exact shader conversion formula. Missing-attachment safety is also unresolved because this helper directly calls `CFStringCompare` on the attachment result.

### Rendering range can differ from reported range

**D.** Metal's matrix helper classifies range from the CVPixelBuffer fourcc. By contrast, `FrameRenderer::isFullRange` reads `VideoFrame[0x44]`. In the traced `VTDecoder::decode` body, the allocated `VideoFrame` is zero-initialized, its color classification is written at `0x40`, and no visible assignment updates the full-range byte at `0x44` before returning the renderer.[^C15][^C6][^C2]

**I.** The traced VT path can therefore report a limited-range flag even when Metal selects a full-range conversion from the pixel format. This is a static inconsistency candidate, not an observed runtime bug. It is not evidence that the image is necessarily rendered with the wrong range.

**Recommendation.** Derive and preserve range explicitly at the decode/presentation boundary. Verify that diagnostics report the same range the shader consumes. Do not use a requested color enum as a replacement for the decoded buffer's range metadata.

## PQ controls the Metal HDR state

**D.** `VTRenderer::isPQ` compares `kCVImageBufferTransferFunctionKey` against `kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ`; an absent image or absent attachment returns false. This is a transfer-function test, not a test of codec or resolution.[^C16]

**D.** The Metal render callback calls `setHDR` before rendering a selected frame. `setHDR` derives a PQ flag and a separate high-precision flag. PQ sets both true. Non-PQ `x420` and `xf20` set the precision flag true; the equivalent 4:4:4 ten-bit formats are not included in that non-PQ test. If both cached flags are unchanged, `setHDR` returns before fetching HDR metadata.[^C17][^C18]

**D.** On a flag change, `setHDR` obtains `kCVImageBufferContentLightLevelInfoKey` and `kCVImageBufferMasteringDisplayColorVolumeKey`. When both exist, it creates `CAEDRMetadata` through `HDR10MetadataWithDisplayInfo:contentInfo:opticalOutputScale:` and passes that object to the layer update.[^C18]

**D.** `updateLayerIfNeeded` selects layer pixel format numeric value `0x5e` when PQ is true or when the precision flag and display-sync flag are both true. Otherwise it selects `0x50`. These are Metal API values, not NVb color enums. Their symbolic API enum names are not claimed from this trace alone.[^C19]

**D.** Entering HDR enables `wantsExtendedDynamicRangeContent`, sets EDR metadata, and assigns a named PQ color space to both the CAMetalLayer and ImGui renderer. The selected name is `kCGColorSpaceITUR_2100_PQ` on the macOS-11-or-later branch and `kCGColorSpaceITUR_2020_PQ_EOTF` otherwise. A guarded path synthesizes default HDR10 metadata when explicit metadata is absent. Leaving HDR disables EDR, clears metadata, and clears both color-space assignments.[^C19]

**U.** Changing only mastering/content-light metadata does not trigger the visible `setHDR` refresh while the two cached flags remain unchanged. Runtime implications and any alternate metadata-refresh path remain unverified. HLG, SDR tone mapping, gamut clipping, and exact PQ shader arithmetic were not traced.

## Display capability uses potential EDR headroom

**D.** `MetalAsyncVideoFrameRenderer::isHDR10RenderingCapable` gets the current NSWindow's NSScreen and returns whether `maximumPotentialExtendedDynamicRangeColorComponentValue` exceeds 1.0. It does not use image resolution or the requested codec for that decision.[^C20]

**D.** `PopulateMonitorDetails` uses the same greater-than-1.0 test for its monitor HDR flag. For an HDR-capable screen it writes a 10-bit capability field and chooses one of numeric display color values 7 or 9 according to `canRepresentDisplayGamut:2`. It writes luminance-like fields using EDR headroom multiplied by 100 and 50, and writes the float constant represented by `0x38d1b717` to the minimum field.[^C21]

**I.** Those arithmetic writes are derived display-capability estimates, not readings of physical peak luminance from EDID. **U.** The canonical names for DisplayColorSpace values 7 and 9 and the exact interpretation of all monitor fields remain unproven. They must not be equated with NVbColorSpace_t values of the same number.

**D.** The monitor helper also records backing scale, desktop-usable display modes, refresh capability, and the current display's Metal-device name. Its newer-screen branch uses `maximumFramesPerSecond`; its fallback uses CVDisplayLink timing. The HDR-rendering predicate queries the current screen at call time, while this analysis does not prove when the monitor cache is rebuilt after a display move.[^C21][^C20]

## Property changes cross a separate application callback

**D.** `VideoDecoderThread::run` compares the returned renderer's resolution, depth, chroma, matrix, PQ, and range values against previous values, then calls the IOInterface property-change virtual method when needed. `GeronimoIOInterface::streamingVideoPropertiesChanged` updates stats and a per-stream record under a mutex. It constructs separate `BifrostEvent` objects for changed resolution, depth, chroma, and color/HDR categories, then dispatches them through a stored callback.[^C4][^C22]

**U.** Despite the class name, that event construction is not proof of a server renegotiation message. This trace does not establish the event's serialized shape or the callback consumer's behavior.

## Compatibility implications

**Recommendation.** Keep matrix coefficients, transfer function, primaries, range, chroma, bit depth, HDR metadata, and display capability as separate typed values. Preserve decoded CVPixelBuffer attachments through texture import. Recompute HDR presentation when output format or display changes, and make the metadata-refresh rule explicit.

**Recommendation.** Do not copy unexplained enum numbers into an implementation. The proven notification construction above is narrower than a complete wire mapping. Validate the shader's matrix/range choice, the layer's HDR state, and the displayed capability independently, especially for 10-bit SDR and full-range images.

## Analysis limitations

The reconstruction resolves one concrete renderer vtable, not every indirect dispatch. Ghidra's Objective-C ABI reconstruction and SIMD temporaries are imperfect. The source of all primaries and HDR extension fields was not recovered, and no target binary was executed. Assertions about actual image fidelity, display output, or negotiated server behavior require runtime evidence.

## Evidence references

[^C1]: `archive/libGeronimo.dylib`, x86_64, `_decodeCallback`, `0x14d2a0`. REA `ev_c196244cfbb11ad787f5d1dfc6c0b5ffdcca56cb5aff4b8f2572a78d68076949`, `procedure_pseudo_code`.
[^C2]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::decode(NvstVideoDecodeUnit_t const&, bool)`, `0x14e1d0`. REA `ev_874799e52c14a2b58c65e91e27937d4c7153d47aecbc66506135495829ff156e`, `procedure_pseudo_code`.
[^C3]: `archive/libGeronimo.dylib`, x86_64, `FrameRenderer::colorSpace() const`, `0xea6b0`. REA `ev_35adbfece0d8097956bab30f2c8d802bbbae911b9b0376eae14d90d65255e58b`, `procedure_pseudo_code`.
[^C4]: `archive/libGeronimo.dylib`, x86_64, `VideoDecoderThread::run()`, `0xd2ad0`. REA `ev_1ceae4ed7c4428d754e61f68d7d86571cddd450a2e1e14cb02bc44fb6d3af360`, `procedure_pseudo_code`.
[^C5]: `archive/libGeronimo.dylib`, x86_64, `__ZTV10VTRenderer bytes`, `0xf25858`. REA `ev_ab4f44130230d5b514011b1aee6c65c3e582ff75ed58e7c7d5532876367adda3`, `read_bytes`.
[^C6]: `archive/libGeronimo.dylib`, x86_64, `FrameRenderer::isFullRange() const`, `0xea6e0`. REA `ev_48664257acdcdaf86729e15575a08f4b6ace184215c1f3013cf40122d491827c`, `procedure_pseudo_code`.
[^C7]: `archive/libGeronimo.dylib`, x86_64, `Nsk::cast(NVbColorSpace_t)`, `0x9d160`. REA `ev_6ed9cca2aa22eb25cca19428c48c66256ab28261f09705b2a3be2352b9b2de57`, `analyze_function`.
[^C8]: `archive/libGeronimo.dylib`, x86_64, `Nsk::convertToStreamingParams(Nsk::StreamStartParameters const&, Nsk::VideoDecoderInitParams const&, NVbStreamingParams_t&)`, `0x9d740`. REA `ev_2fe06a847ff0609d5548bf28c74a74a0ef787a292db39518d91c70bc9806c6ff`, `procedure_pseudo_code`.
[^C9]: `archive/libGeronimo.dylib`, x86_64, `chooseMaxSupportedVideoSettings(NVbVideoSettings_t&, DecoderMode, std::vector<NVbResolution_t, std::allocator<NVbResolution_t> >&, GraphicsContextType, DecoderType, bool, YUVFormat, bool, bool)`, `0xb1de0`. REA `ev_48b1acb5cb2a78af805b1f0a6c3db0ed1cfb8fa392f8a630e628e6dfcfbb868b`, `procedure_pseudo_code`.
[^C10]: `archive/libGeronimo.dylib`, x86_64, `platformSupportsHDR10Decode(GraphicsContextType, NVbCodec_t, DecoderType)`, `0xb3ce0`. REA `ev_96dc6c40ea33af38635b62a58fbbd6265a5b897691cdedd1d89d11caf3560647`, `procedure_pseudo_code`.
[^C11]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::isHDR10Capable(NVbCodec_t)`, `0x14f8c0`. REA `ev_5ebe35a1da5a4de4c5ed1b0b3d0352dee137a42903ee819d9f19f19f4c993fa8`, `procedure_pseudo_code`.
[^C12]: `archive/libGeronimo.dylib`, x86_64, `VTRenderer::is10Bit() const`, `0x150180`. REA `ev_3b1705e2d91d6ba0b7605ac2b06d1ff56ec3f3c8640a720758c218540a1d03f7`, `procedure_pseudo_code`.
[^C13]: `archive/libGeronimo.dylib`, x86_64, `VTRenderer::chromaFormat() const`, `0x1501d0`. REA `ev_b3392de7122eb907e920de44190b8e0dbf88b6d5576e96cb51c0d447df0188b4`, `procedure_pseudo_code`.
[^C14]: `archive/libGeronimo.dylib`, x86_64, `Nsk::cast(NVbChromaFormat_t)`, `0x9d290`. REA `ev_94389419f2623bef5380fe684775d0bffe38c7697001293b505920e51ddd1acc`, `procedure_pseudo_code`.
[^C15]: `archive/libGeronimo.dylib`, x86_64, `GetColorTransferMatrix(simd_float3x3*, float __vector(3)*, __CVBuffer*)`, `0x1368d0`. REA `ev_cd8a71633ff38ad870623a8d653adc89f8a12f62b0919312a60084904ad372d9`, `procedure_pseudo_code`.
[^C16]: `archive/libGeronimo.dylib`, x86_64, `VTRenderer::isPQ() const`, `0x150120`. REA `ev_0b136c038b0c35f3fee71c0f129498aa7d6bb2e72fec6b9633fa4d543c64d0dd`, `procedure_pseudo_code`.
[^C17]: `archive/libGeronimo.dylib`, x86_64, `MetalAsyncVideoFrameRenderer::renderCallback(std::chrono::time_point<std::chrono::steady_clock, std::chrono::duration<long long, std::ratio<1l, 1000000000l> > >, std::chrono::time_point<std::chrono::steady_clock, std::chrono::duration<long long, std::ratio<1l, 1000000000l> > >, std::chrono::time_point<std::chrono::steady_clock, std::chrono::duration<long long, std::ratio<1l, 1000000000l> > >)`, `0x139fb0`. REA `ev_5ea2d3734564c2d3cbfea3ba087c1b0c435b2e5c24862525c5baaffe714859ca`, `procedure_pseudo_code`.
[^C18]: `archive/libGeronimo.dylib`, x86_64, `MetalAsyncVideoFrameRenderer::setHDR(VTRenderer*)`, `0x134730`. REA `ev_01a69d1b0c7d64b232150462f514b9d3daa2274e18c118f11bdfce5ab2bf99fe`, `procedure_pseudo_code`.
[^C19]: `archive/libGeronimo.dylib`, x86_64, `MetalAsyncVideoFrameRenderer::updateLayerIfNeeded(bool, bool, bool, objc_object*)`, `0x134930`. REA `ev_0cc2bbab8964f1725a788192164591d7d1f74b8e96138ae3566c92aeb453068c`, `procedure_pseudo_code`.
[^C20]: `archive/libGeronimo.dylib`, x86_64, `MetalAsyncVideoFrameRenderer::isHDR10RenderingCapable() const`, `0x13d1d0`. REA `ev_90ca40f3c1e8bab4ac3e7e76d2720d0a4f227f9f1be5a97c8e5aba0a473de458`, `procedure_pseudo_code`.
[^C21]: `archive/libGeronimo.dylib`, x86_64, `PopulateMonitorDetails(NSScreen*, DisplayUtil::Monitor&, unsigned int)`, `0x11df90`. REA `ev_325aeda9ae93a1d012bc791a72530243f978998d5d76d3dda0afd56241551aba`, `procedure_pseudo_code`.
[^C22]: `archive/libGeronimo.dylib`, x86_64, `GeronimoIOInterface::streamingVideoPropertiesChanged(unsigned short, unsigned int, unsigned int, unsigned char, NVbChromaFormat_t, NVbColorSpace_t, bool)`, `0x53ac0`. REA `ev_fab31ef5e8c897d520c9d7b476f89069968f3294e724f5998eb9ed8bf98175e5`, `procedure_pseudo_code`.
