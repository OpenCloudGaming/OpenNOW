# OpenNOW Linux platform backend

This crate owns the Linux hardware decode, audio, and Vulkan presentation path used by the native-streamer workspace.

The backend accepts H.264, HEVC/H.265, and AV1 video plus Opus audio. The supported Qt application uses the embedded GPU frame path described below, without an SDL video window or a second presenter. It decodes Opus from its static library and sends PCM through its bundled SDL audio path; the standalone Linux-session API also exposes PipeWire and ALSA sinks.

The standalone development host retains its existing decoder order: with the `ffmpeg` feature, automatic decode prefers CUDA/NVDEC when an NVIDIA driver is present, then Vulkan Video, and finally FFmpeg's software decoder. Native VA-API and stateful V4L2 M2M remain available for H.264. That standalone presentation path normalizes output to NV12, prefers a Vulkan swapchain attached to a caller-owned X11 or Wayland surface, and falls back to an SDL NV12 texture when Vulkan presentation is unavailable.

Runtime probing opens the real device or service. A compiled feature or a shared library by itself is never reported as an available decoder, presenter, or audio sink.

`LinuxSession` owns the bounded decode and audio workers. Submit complete encoded access units and Opus packets, drain decoded frames and typed events, and call `reconfigure` or `stop` explicitly. Queue overflow drops the oldest media, flushes video reference state, and emits `NeedKeyframe` instead of allowing corrupted prediction chains to continue. If a decoder fails, the session advances through the configured fallback order and reuses the current keyframe when possible.

Standalone presentation stays on the caller's window-system thread. Construct `NativeSurface::borrow_x11` or `NativeSurface::borrow_wayland` from handles that the caller owns, then create `VulkanPresenter` and feed it frames returned by the session. The standalone development runtime obtains either pair from SDL's raw window handles: X11 is reparented into the host-owned surface, while Wayland remains a compositor-managed top-level surface. The presenter owns its Vulkan instance, device, swapchain, and `VkSurfaceKHR`; it never destroys the borrowed X11 window, Xlib display, `wl_surface`, or `wl_display`. Its lifetime is tied to `NativeSurface`, and it is intentionally not `Send`.

## Embedded shared Vulkan decode

`SharedVulkanDevice::create` creates an owned Vulkan Video device when both `ffmpeg` and `vulkan` are enabled. The Qt shell obtains it through streamer FFI ABI 5, adopts the returned instance, device, and graphics queue, and passes the opaque owner into engine creation. The runtime clones the same `Arc<SharedVulkanDevice>` into `SessionConfig::vulkan_device`; it does not open an unrelated decode device for the embedded session.

Embedded capability reporting uses the attached device's codec profiles. Session creation also checks the negotiated codec, dimensions, bit depth, and chroma subsampling. Supported 4:2:0 8-bit streams use NV12; supported 4:2:0 10-bit streams use P010. Shared Vulkan supports 4:4:4 through NV24 (8-bit) and P410 (10-bit) only when the exact codec profile, two-plane decode output format, and sampled snapshot formats are available on the attached device. Chroma remains full resolution through the GPU snapshot and conversion; unsupported formats are rejected rather than converted to 4:2:0. Without a shared owner, embedded Vulkan decode remains unavailable and Auto retains the existing non-Vulkan fallback path. Standalone probe results cannot enable embedded Vulkan.

The 4:4:4 format mappings and codec profiles follow bundled FFmpeg 9.0's `libavcodec/vulkan_video.c`. This is an implementation capability, not a guarantee that a GPU or provider supports each profile. CUDA/NVDEC does not have an embedded GPU import path here; its CPU transfer cannot enable HDR or 4:4:4. VA-API's supported DMA-BUF path remains NV12/P010 only. Neither decoder availability nor an HDR-capable source establishes HDR display support: Qt must separately enable a suitable output surface. Conversion preserves PQ/HLG with BT.2020 metadata in float textures, while 10-bit SDR uses a 10-bit RGB texture.

The shared-device probe matches FFmpeg's distinct/coincident decoded-picture-buffer usage and conservatively rejects profiles exposing competing output layouts. FFmpeg may prefer three-plane storage over an available two-plane format, so merely finding NV24/P410 in the driver's format list is not enough to advertise support. Such mixed-format devices remain unavailable until their selected storage has an implemented GPU snapshot path.

The hello report carries per-codec `colorQualities` from those same profiles. Qt forwards it to the application core, which checks both Auto and manual codec choices against the user's color quality before CloudMatch allocates a seat. Main10 and 4:4:4 therefore fail at preflight when unsupported, rather than only after transport starts.

Decode workers copy decoded images into bounded GPU snapshots isolated from FFmpeg's decoded-picture buffer. Only completed, immutable snapshots are published to Qt; retaining or sampling a snapshot cannot race with decoder reuse of a reference image. Qt records conversion and synchronization into its own command stream and keeps the snapshot alive until its GPU use completes. This is GPU-only presentation without CPU readback, not zero-copy. Device owners must outlive all adopted Qt resources, sessions, and outstanding frame resources; scene-graph retirement still occurs before Qt graphics teardown.

## Build requirements

The default developer build needs a Linux C toolchain, Linux UAPI headers, libclang (for `v4l2-sys`), and a Vulkan loader at runtime. VA-API is optional because its maintained Rust bindings generate against the host libva headers. The `ffmpeg` feature uses installed FFmpeg development libraries. Production uses `ffmpeg-bundled`, which builds FFmpeg statically and needs CMake, Make, NASM, pkg-config, and Git but no FFmpeg development package:

```sh
cargo test -p opennow-streamer-platform-linux
cargo test -p opennow-streamer-platform-linux --features ffmpeg-bundled
cargo check -p opennow-streamer-platform-linux --all-targets
cargo check -p opennow-streamer-platform-linux --all-targets --all-features
./scripts/check-linux.sh
```

The last command additionally needs the libva and DRM development packages. Cross-check the architecture-independent path with:

```sh
rustup target add aarch64-unknown-linux-gnu
cargo check -p opennow-streamer-platform-linux --target aarch64-unknown-linux-gnu --no-default-features
```

The H.264 V4L2 fallback supports both single-planar and multi-planar stateful decoder nodes used on x86_64, aarch64, and Raspberry Pi 4. Bundled Linux aarch64 builds also include pinned Raspberry Pi FFmpeg support for HEVC V4L2 Request decoding on Pi 4 and Pi 5. That HEVC path is embedded-only, supports 8-bit 4:2:0 SDR, and requires Qt's explicit DMA-BUF buffer-import capability. SAND128 single-object NC12 and split-object Nc12 frames are copied into planar textures entirely on the GPU; packed P030/10-bit is rejected. See [Raspberry Pi requirements and acceptance](../../../../docs/raspberry-pi.md).

Actual decode tests require `/dev/video*` and, for HEVC Request, `/dev/media*`; VA-API tests require `/dev/dri/renderD*`; Vulkan presentation requires a live X11 or Wayland surface; audio requires an SDL-supported system audio service. The ignored `raspberry_pi_hevc_request_drm_only` test exercises Pi HEVC Request output, and `local_hardware_decodes_all_required_codecs` exercises Vulkan Video and CUDA/NVDEC, when the corresponding hardware and FFmpeg command-line encoder are installed. Ordinary unit tests do not claim those devices exist.

The ignored shared-device HEVC tests exercise the embedded GPU-only snapshot path separately for NV12, P010, NV24, and P410. Run them on a Vulkan Video device supporting the corresponding HEVC profiles; 4:4:4 requires Range Extensions support. Compilation, ordinary unit tests, and synthetic conversion tests on Mesa software Vulkan do not establish hardware decode support:

```sh
cargo test -p opennow-streamer-platform-linux --features ffmpeg-bundled shared_vulkan_hevc_nv12_gpu_only -- --ignored --nocapture
cargo test -p opennow-streamer-platform-linux --features ffmpeg-bundled shared_vulkan_hevc_p010_gpu_only -- --ignored --nocapture
cargo test -p opennow-streamer-platform-linux --features ffmpeg-bundled shared_vulkan_hevc_nv24_gpu_only -- --ignored --nocapture
cargo test -p opennow-streamer-platform-linux --features ffmpeg-bundled shared_vulkan_hevc_p410_gpu_only -- --ignored --nocapture
```
