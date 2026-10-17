# Mac video decode and presentation

Geronimo owns platform decoder selection, VideoToolbox sample submission, decoded-frame objects, and Metal presentation. Codec negotiation policy is distinct from decoder fallback. Explicit unsupported HEVC or AV1 requests can fail without a codec substitution, while automatic selection can choose another codec.

## Scope and evidence

This analysis covers `archive/libGeronimo.dylib`, x86_64, SHA-256 `f564aa1910f1a366973ce3c0f0e6261122833bc41c530a669ec846a51d50d15a`. Every address below is a thin Mach-O virtual address, not a fat-container offset. See [artifacts](artifacts.md), [method](method.md), and [color handling](color.md).

**D** identifies a decompiled branch, call, or decoded data. **S** identifies a symbol or string. **I** identifies an inference. **U** identifies an unresolved question. Runtime behavior is unverified on the Linux analysis host. This is a focused reconstruction, not an exhaustive account of the library.

## The Bifrost boundary supplies decode units, not pixels

**D.** `GeronimoIOInterface::queueDecodeVideoUnit` receives `NvstVideoDecodeUnit_t` pointers and routes each pointer by the unit's supplied stream index. The unit carries a frame number and a release callback. Rejected streams and discarded units report a frame state and invoke that callback when present. The queue retains the unit rather than copying an image. `getVideoDecodeUnit` selects the per-stream queue and waits when requested.[^V1][^V2]

**D.** `VideoDecoderThread::run` waits for a unit for its assigned stream, stores that unit's frame number in the decoder, and calls the decoder through its virtual interface. A returned frame receives timing data and deferred cleanup work before it goes to the renderer queue. Mismatched stream indices take a rejection path. These calls establish a decoder-thread boundary rather than codec work in the Metal render callback.[^V3]

**D.** Geronimo's `BifrostSDKExecutor` forwards decoder state to imported `_nvbUpdateVideoDecoderState` and frame state to `_nvbUpdateVideoFrameState`. The calls preserve the supplied stream index and frame number. The frame-state wrapper also forwards timing and additional numeric arguments. Both wrappers check an executor-active flag and treat a nonzero returned status as failure.[^V4][^V5]

**U.** This analysis does not assign wire meanings to the numeric frame states, prove a packet layout, or resolve every IOInterface virtual call to a concrete Bifrost operation. The imported calls are an API boundary, not proof of a transport implementation.

## Capability probes and default codec choice

**D.** `VTDecoder::codecsAvailable` initializes cached hardware and software masks once. Its initialization block dynamically resolves `VTIsHardwareDecodeSupported` and probes the `avc1`, `hvc1`, and `av01` fourcc values. Successful probes set bits 1, 2, and 4 respectively. The codec-specific transformer construction independently associates numeric codec values 1, 2, and 4 with H.264, HEVC, and AV1.[^V6][^V7]

**D.** The software mask is set to 3 when the hardware-query API is available. That assignment is not a successful software-decoder session test. When the query API is absent, the software mask is 1 and an H.264 format/session probe sets hardware bit 1 only on successful session creation.[^V6]

**D.** The hardware availability accessor removes HEVC bit 2 at frame rates of at least 120 when its cached `hevc120` flag is false. The initializer derives that flag from comparisons with model identifiers `MacPro7,1` and `iMacPro1,1`. The AV1 probe is skipped when `IsAV1BrokenOnThisVersionOfMacOS` returns true. Its initialization block tests whether the first `_getOSVersion` output equals 15 and the second is below 2.[^V6][^V8][^V9]

**D.** `platformCodecGetDefaults` queries both masks at 60 FPS. It prefers hardware HEVC, then hardware H.264. If neither hardware bit is present, it uses the same preference within the software mask. It sets decoder mode 0. This default helper does not select AV1 even if hardware bit 4 exists.[^V10]

**U.** The underlying OS and model helper implementations were not reconstructed. These checks document shipped policy, not verified device performance or a universal statement about macOS codec support.

## Automatic selection is not explicit-request fallback

**D.** `platformChooseStreamingSettings` calls `chooseMaxSupportedVideoSettings` after collecting display modes and normalizing the requested FPS. `Nsk::convertToStreamingParams` calls the former while building `NVbStreamingParams_t`, then adjusts decoder settings. Direct caller enumeration also finds capability and telemetry callers, so the same helper can run outside a live stream-start path.[^V11][^V12][^V13]

The following decisions are branches in `chooseMaxSupportedVideoSettings`, not interpretations of its log messages.[^V14]

1. **D.** A zero requested codec becomes internal value 7, an automatic-selection sentinel. Hardware codec availability, decoder capability, configuration checks, and transcode checks contribute to the selection. Software decoder mode 1 instead queries LAVC capability and uses availability mask 1.
2. **D.** Automatic selection chooses AV1 when hardware bit 4 and the relevant AV1 configuration gate allow it. Otherwise it chooses HEVC when bit 2 and the HEVC gate allow it. Otherwise it chooses H.264. With HDR requested or FPS at least 240, the H.264 branch clears HDR and caps FPS at 120.
3. **D.** Failed decoder-capability lookup for explicit H.264 with HDR returns `0xc0f11042`. Explicit AV1 returns `0xc0f11050`. Explicit HEVC returns `0xc0f11043`. Those branches clear resolution fields and bit depth, then return without selecting another codec.
4. **D.** Other failed capability lookups take the visible default branch of H.264 at 1280×720 and 60 FPS. This is not evidence that every unsupported request falls back to H.264.
5. **D.** When the configuration validator is enabled and rejects an explicit codec at the chosen resolution/FPS/HDR combination, the helper caps FPS at 120 and retries the configuration check with the same codec. A second rejection returns a codec-specific error. When that validator is disabled, an explicit AV1 request above 120 FPS is capped at 120 and checked by `isAv1Enabled`; failure returns the AV1 error.

**D.** A transcode-policy branch can remove HEVC and AV1 from the automatic availability mask when an H.264 encode test fails. Its narrow removal case requires a requested image area below 4,953,600 pixels, FPS below 240, no HDR, bit depth below 10, and neither of the tested SDR color values 4 or 5. Outside that case, it permits HEVC/AV1 without transcoding. Later code can disable GRC when re-encoding is unavailable. These are recording/transcode-related gates in selection, not a decoder crash fallback.[^V14]

**U.** The complete remote-configuration policy, model minimum-spec list, and server acceptance of each selected combination remain unresolved. Debug overrides and telemetry-mode paths also affect this helper. The conditions above must not be generalized to every caller.

## Decoder mode determines fallback permission

**D.** `platformCreateVideoDecoder` constructs a `LAVCDecoder` for mode 1. Modes 0 and 2 construct a `VTDecoder`; other values return null. Its RGBA-request branch imposes additional restrictions before construction.[^V15]

**D.** `VTDecoder::initialize` selects a H.264, HEVC, or AV1 decode-unit transformer. For mode 0, it accepts the requested codec when either the hardware or software availability mask contains its bit. Mode 2 checks the hardware mask. A missing transformer or unsupported codec returns `0xc0f11018`.[^V7]

**D.** `VTDecoder::createDecoderSession` sets `EnableHardwareAcceleratedVideoDecoder` for modes 0 and 2. Only mode 2 also sets `RequireHardwareAcceleratedVideoDecoder`. After successful creation it queries `UsingHardwareAcceleratedVideoDecoder` and logs which implementation VideoToolbox supplied. A session-creation error is logged with `GERONIMO_VTDECODER_DECOMPRESSION_SESSION`; the function contains no second session attempt with altered flags and no call constructing LAVC.[^V16]

**D.** `VideoDecoderSet::CreateAndInitializeDecoder` constructs one platform decoder and calls its initialization method. Initialization failure destroys that decoder and clears the output pointer. `NskStreamClientImpl::createVideoDecoder` similarly destroys a failed decoder set and returns `0xc0f11006`. Neither body contains an explicit retry in another decoder mode.[^V17][^V18]

**I.** Mode 0 permits VideoToolbox to supply software decoding within the same framework. That is different from Geronimo explicitly rebuilding the pipeline with LAVC. The diagnostic text containing "falling back to software" does not establish a separate retry algorithm.[^V16]

## Format changes rebuild only when required

**D.** `VTDecoder::decode` asks the selected transformer for a sample buffer and updates the stored format. It checks `VTDecompressionSessionCanAcceptFormatDescription` against the sample's format. A missing session or a rejected format causes session creation. Creation first invalidates and releases an old session when one exists. AV1 session creation enables `AllowBitstreamToChangeFrameDimensions`.[^V19][^V16]

**D.** `setVideoFormat` retains the changed format description. For H.264 it extracts SPS/PPS and recording information. `onCodecChanged` stores the new codec, destroys the current conversion helper and decode-unit transformer, invalidates the VideoToolbox session, releases a retained decoded image, and reinitializes the decoder.[^V20][^V21]

**D.** The VideoToolbox output callback logs nonzero decode status and returns without a frame. On success it retains the image buffer. `decode` transfers the retained image into a `VideoFrame` owned by a `VTRenderer`, then clears the decoder's retained-image slot. A sample transform failure, missing session, or missing output image produces state reporting and no renderer.[^V22][^V19]

**U.** These functions do not prove the whole session-restart algorithm, complete VideoToolbox drain behavior, or recovery after graphics-device loss. The no-output-image paths also depend on the caller's boolean submission option. Their state integers are not assigned protocol names here.

## Pixel-format probing and Metal presentation

**D.** `defaultPixelFormat` caches a per-codec result. For H.264 and HEVC it constructs a test format, creates a hardware-enabled session, and obtains a black pixel buffer to inspect the native fourcc. Its classifications are 1 for `420v`, 2 for `2vuy`, and 3 for `BGRA`. Other outputs remain unclassified. The AV1 case skips this test. Initialization uses `420v` as the request for an unclassified result.[^V23][^V7]

**D.** Session creation requests an IOSurface-backed pixel buffer. It reads `BitsPerComponent` from the format description and selects a 10-bit output when that value exceeds 9. A HEVC parameter-set result equal to 3 selects the 4:4:4 output variants `444v` or `x444`; otherwise the 10-bit override is `x420`. It then merges attributes supplied through the renderer's virtual interface.[^V16]

**D.** With an asynchronous renderer, initialization does not construct the synchronous GL conversion helper. Without one, it creates the relevant BGRA, packed-YUV, or bi-planar converter. YUVxA requires a compatible default format and an asynchronous renderer, otherwise initialization returns a specific error.[^V7]

**D.** `MetalAsyncVideoFrameRenderer::addVideoFramePass` imports image-buffer planes with `CVMetalTextureCacheCreateTextureFromImage`, obtains Metal textures, binds them to fragment texture slots, and draws a quad. It has explicit bi-planar, `2vuy`, and BGRA cases and reports an unknown fourcc otherwise. The bi-planar path calls `GetColorTransferMatrix`. The render function submits a drawable through `presentDrawable:`, `presentDrawable:atTime:`, or `presentDrawable:afterMinimumDuration:` according to pacing branches.[^V24][^V25]

**I.** This is a GPU texture-import presentation path. The traced normal Metal frame pass has no visible CPU pixel-buffer copy, but that does not prove every filter, recorder, or alternate presenter is zero-copy.[^V24]

**U.** Shader arithmetic, texture-import failures, complete drawable starvation behavior, and Metal-device recreation remain outside the verified coverage. Renderable fourcc support is not the same as advertised decoder capability. In this binary, `VTDecoder::is444Capable` and `is422Capable` both return false even though decode/presentation code contains 4:4:4 format cases.[^V26][^V27][^V16][^V24]

## Queue recovery is visible, but feedback meanings are not

**D.** The receive queue checks its size after insertion. The visible initial threshold is 40 units, with a later threshold drawn from configuration after a counter exceeds 79. Unless an object flag bypasses the check, crossing the threshold invokes the IOInterface recovery virtual method. This is overflow recovery after insertion, not proof of a fixed-capacity allocation.[^V1]

**D.** `flushAndRecoverVideoStream` can discard the complete queue or discard an older prefix while reinserting a retained suffix. Discarded units receive state notifications and release-callback execution. A complete-recovery flag makes later queue insertion reject unit types outside numeric values 1 through 4 until an accepted unit clears the flag.[^V28][^V1]

**U.** This trace does not label those unit types as IDR/keyframes, prove that the resulting feedback requests a reference frame, or establish the wire encoding of recovery states. Those require Bifrost-side traces.

## Compatibility implications

**Recommendation.** Keep automatic codec choice, explicit-request rejection, VideoToolbox hardware permission, and fallback to another decoder implementation as separate decisions. Preserve the codec, stream index, frame number, format description, and image metadata through decode and presentation. Change format without restarting transport when the decoder can accept the new description. See [color handling](color.md) for the metadata distinctions.

**Recommendation.** Treat output-format probing as a capability observation, not a promise that every later session succeeds. Keep decoding off the presentation thread. Make queue-overflow and no-output-image behavior observable, and validate recovery feedback against the Bifrost API rather than copying unexplained state integers.

## Analysis limitations

Ghidra reports unsettled type propagation in `createDecoderSession` and `chooseMaxSupportedVideoSettings`, and removes unreachable blocks in the decoder-set constructor path. Several virtual calls and Objective-C ABI arguments remain imperfectly typed. The evidence supports the named branch decisions above, not every inferred C++ field name or decompiled signature. Empty direct-caller results for virtual methods do not establish that those methods are unused.

## Evidence references

[^V1]: `archive/libGeronimo.dylib`, x86_64, `GeronimoIOInterface::queueDecodeVideoUnit(NvstVideoDecodeUnit_t const*)`, `0x51d50`. REA `ev_b2a89ad951bd2bd05e5938ae05c2c9dd85884b66ca1b04c58563d1eb32bc68c0`, `procedure_pseudo_code`.
[^V2]: `archive/libGeronimo.dylib`, x86_64, `GeronimoIOInterface::getVideoDecodeUnit(unsigned short, unsigned int&, bool, unsigned int)`, `0x52280`. REA `ev_6f3a778dc5b343003b09f90c4afe897284f7d88e3c82fe11d0d007b41b7afe15`, `procedure_pseudo_code`.
[^V3]: `archive/libGeronimo.dylib`, x86_64, `VideoDecoderThread::run()`, `0xd2ad0`. REA `ev_1ceae4ed7c4428d754e61f68d7d86571cddd450a2e1e14cb02bc44fb6d3af360`, `procedure_pseudo_code`.
[^V4]: `archive/libGeronimo.dylib`, x86_64, `BifrostSDKExecutor::updateVideoDecoderState(unsigned short, unsigned int, NvstVideoDecoderState_t)`, `0x4df10`. REA `ev_14f2e9485603d8801d339397a8452bd5508b10e63fe17d684a20d8164a2d96c5`, `procedure_pseudo_code`.
[^V5]: `archive/libGeronimo.dylib`, x86_64, `BifrostSDKExecutor::updateVideoFrameState(unsigned short, unsigned int, NvstVideoFrameState_t const&, unsigned int, unsigned int, std::chrono::time_point<std::chrono::steady_clock, std::chrono::duration<long long, std::ratio<1l, 1000000000l> > >)`, `0x4db60`. REA `ev_23bc33f4068b916764ce71b07afb17a35386e0c4af4c942a62d0d4c6361e3141`, `procedure_pseudo_code`.
[^V6]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::codecsAvailable initialization block`, `0x14f490`. REA `ev_52eea3a897907afa345d70afcca5398773023041ab06a6527e3f941fc2fc43aa`, `procedure_pseudo_code`.
[^V7]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::initialize(IOInterface*, PlatformDecoderSettings const&, std::shared_ptr<AsyncVideoFrameRenderer> const&)`, `0x14e870`. REA `ev_59dc8fc74809945800502720999734dab760a03d7129bc70193e878b60fa6924`, `procedure_pseudo_code`.
[^V8]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::codecsAvailable(bool, unsigned int)`, `0x14f1c0`. REA `ev_2f85149fcccce72a52bbe36a39d1413e5a6b800715f237e7165610f1d2def5c5`, `procedure_pseudo_code`.
[^V9]: `archive/libGeronimo.dylib`, x86_64, `IsAV1BrokenOnThisVersionOfMacOS initialization block`, `0x1502f0`. REA `ev_c29bc3a968a44b5a0518f133f70b6e0700838a6cd0818a3b0e979cb8543021bc`, `procedure_pseudo_code`.
[^V10]: `archive/libGeronimo.dylib`, x86_64, `platformCodecGetDefaults(DecoderMode&, NVbCodec_t&)`, `0xb5060`. REA `ev_eab3d51139d363925c457b826aaad2b23a7d1ba6777fcd160f794a37dc3649df`, `procedure_pseudo_code`.
[^V11]: `archive/libGeronimo.dylib`, x86_64, `platformChooseStreamingSettings(PlatformStreamingSettingsParams const&, NVbVideoSettings_t&, bool, bool)`, `0xb40e0`. REA `ev_21d2fa1cdd3c0449fbb0605519972f20d064491d8f567a119a086738affe4c3e`, `procedure_pseudo_code`.
[^V12]: `archive/libGeronimo.dylib`, x86_64, `Nsk::convertToStreamingParams(Nsk::StreamStartParameters const&, Nsk::VideoDecoderInitParams const&, NVbStreamingParams_t&)`, `0x9d740`. REA `ev_2fe06a847ff0609d5548bf28c74a74a0ef787a292db39518d91c70bc9806c6ff`, `procedure_pseudo_code`.
[^V13]: `archive/libGeronimo.dylib`, x86_64, `procedure_callers at 0xb1de0`, `0xb1de0`. REA `ev_14a809d2bd614f1235e3a4ada36d9f1f44a7c15aa912d1ca22a8a6e25c64aa8a`, `procedure_callers`.
[^V14]: `archive/libGeronimo.dylib`, x86_64, `chooseMaxSupportedVideoSettings(NVbVideoSettings_t&, DecoderMode, std::vector<NVbResolution_t, std::allocator<NVbResolution_t> >&, GraphicsContextType, DecoderType, bool, YUVFormat, bool, bool)`, `0xb1de0`. REA `ev_48b1acb5cb2a78af805b1f0a6c3db0ed1cfb8fa392f8a630e628e6dfcfbb868b`, `procedure_pseudo_code`.
[^V15]: `archive/libGeronimo.dylib`, x86_64, `platformCreateVideoDecoder(PlatformDecoderCreationSettings&, unsigned int)`, `0xb1c00`. REA `ev_3950a6f3aa8deb3a5fa00d1e68cdc9a6a5094e94d7c806aa21176bf844d8ce72`, `procedure_pseudo_code`.
[^V16]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::createDecoderSession(opaqueCMFormatDescription const*)`, `0x14d780`. REA `ev_2eadc2f766ec2c91e7b9f71808f0437e169e838461f7ad9c6f90d28371945570`, `procedure_pseudo_code`.
[^V17]: `archive/libGeronimo.dylib`, x86_64, `VideoDecoderSet::CreateAndInitializeDecoder(IOInterface*, VideoDecoderSet::InitParams const&, unsigned int, VideoDecoder**)`, `0xd7d20`. REA `ev_9610c3852dfe5085c9819c6040499e4b5ea7ee2b997bdfd8ba46bdcfef80aec4`, `procedure_pseudo_code`.
[^V18]: `archive/libGeronimo.dylib`, x86_64, `Nsk::NskStreamClientImpl::createVideoDecoder()`, `0x996b0`. REA `ev_9eed0cd2adc3b2b86778fb2c9f24dffc2d1e060d012de373633d855ce867a97f`, `procedure_pseudo_code`.
[^V19]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::decode(NvstVideoDecodeUnit_t const&, bool)`, `0x14e1d0`. REA `ev_874799e52c14a2b58c65e91e27937d4c7153d47aecbc66506135495829ff156e`, `procedure_pseudo_code`.
[^V20]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::setVideoFormat(opaqueCMFormatDescription const*)`, `0x14e0b0`. REA `ev_e6cb6c382e1832147af32482e34b23c4ceba3bb7943b514377dc08690ccde36b`, `procedure_pseudo_code`.
[^V21]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::onCodecChanged(NVbCodec_t)`, `0x14e780`. REA `ev_8a64b01016570a2f11b333865318c3079fe7d700dd508f36903e4a3a7450b644`, `procedure_pseudo_code`.
[^V22]: `archive/libGeronimo.dylib`, x86_64, `_decodeCallback`, `0x14d2a0`. REA `ev_c196244cfbb11ad787f5d1dfc6c0b5ffdcca56cb5aff4b8f2572a78d68076949`, `procedure_pseudo_code`.
[^V23]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::defaultPixelFormat(NVbCodec_t)`, `0x14ec70`. REA `ev_b5e55f151e67136b2c66a6d8c50d70ee92c29622f47bc3525a14b09daad3e2cd`, `procedure_pseudo_code`.
[^V24]: `archive/libGeronimo.dylib`, x86_64, `MetalAsyncVideoFrameRenderer::addVideoFramePass(objc_object objcproto23MTLRenderCommandEncoder* ( block_pointer)(), unsigned int, __CVBuffer*, double, CGRect, objc_object objcproto16MTLCommandBuffer*, objc_object objcproto10MTLTexture*)`, `0x134c60`. REA `ev_0d4aac90d47cb8c114bff884e07d80d1df907b7469ba510274fca772c51a97e3`, `procedure_pseudo_code`.
[^V25]: `archive/libGeronimo.dylib`, x86_64, `MetalAsyncVideoFrameRenderer::render(objc_object objcproto15CAMetalDrawable*, VTRenderer*, AsyncOffscreenBuffer*, AsyncOffscreenBufferPool*, double, double)`, `0x138b20`. REA `ev_ad6fe582702f65a386f2d95756ceb1a8277a5244cd7f955cdae944d50d2008ab`, `procedure_pseudo_code`.
[^V26]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::is444Capable(NVbCodec_t)`, `0x14f910`. REA `ev_a040543a439956df538948a2183dba6db35c92daa793df6f881f49ede4a4c603`, `procedure_pseudo_code`.
[^V27]: `archive/libGeronimo.dylib`, x86_64, `VTDecoder::is422Capable(NVbCodec_t)`, `0x14f920`. REA `ev_c9ffa55232143fd4ef50357878affaf95a677d1f74128c362c8ebb6f42da1c9e`, `procedure_pseudo_code`.
[^V28]: `archive/libGeronimo.dylib`, x86_64, `GeronimoIOInterface::flushAndRecoverVideoStream(unsigned short, bool)`, `0x52530`. REA `ev_1beb83ef32bb31e33765f5b871b97be255b7993382e469cef69471efbd557f65`, `procedure_pseudo_code`.
