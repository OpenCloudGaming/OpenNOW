# OpenNOW Linux platform backend

This crate owns the Linux hardware decode, audio, and Vulkan presentation path used by the native-streamer workspace.

The backend accepts H.264, HEVC/H.265, and AV1 video plus Opus audio. With the `ffmpeg` feature, automatic decode prefers CUDA/NVDEC when an NVIDIA driver is present, then Vulkan Video, and finally FFmpeg's software decoder. Native VA-API and stateful V4L2 M2M remain available for H.264. All decoder outputs are normalized to NV12. Presentation prefers a Vulkan swapchain attached to a caller-owned X11 or Wayland surface and automatically falls back to an SDL NV12 texture when Vulkan presentation is unavailable. The integrated streamer decodes Opus from its static library and sends PCM through its bundled SDL audio path; the standalone Linux-session API also exposes PipeWire and ALSA sinks.

Runtime probing opens the real device or service. A compiled feature or a shared library by itself is never reported as an available decoder, presenter, or audio sink.

`LinuxSession` owns the bounded decode and audio workers. Submit complete encoded access units and Opus packets, drain decoded frames and typed events, and call `reconfigure` or `stop` explicitly. Queue overflow drops the oldest media, flushes video reference state, and emits `NeedKeyframe` instead of allowing corrupted prediction chains to continue. If a decoder fails, the session advances through the configured fallback order and reuses the current keyframe when possible.

Presentation stays on the caller's window-system thread. Construct `NativeSurface::borrow_x11` or `NativeSurface::borrow_wayland` from handles that the caller owns, then create `VulkanPresenter` and feed it frames returned by the session. The integrated runtime obtains either pair from SDL's raw window handles: X11 is reparented into the shell-owned surface, while Wayland remains a compositor-managed top-level surface. The presenter owns its Vulkan instance, device, swapchain, and `VkSurfaceKHR`; it never destroys the borrowed X11 window, Xlib display, `wl_surface`, or `wl_display`. Its lifetime is tied to `NativeSurface`, and it is intentionally not `Send`.

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

The V4L2 fallback supports both single-planar and multi-planar stateful decoder nodes used on x86_64, aarch64, and Raspberry Pi. Actual decode tests require `/dev/video*`; VA-API tests require `/dev/dri/renderD*`; Vulkan presentation requires a live X11 or Wayland surface; audio requires an SDL-supported system audio service. The ignored `local_hardware_decodes_all_required_codecs` test performs real Vulkan Video and CUDA/NVDEC decoding when the FFmpeg command-line encoder is installed. Ordinary unit tests do not claim those devices exist.
