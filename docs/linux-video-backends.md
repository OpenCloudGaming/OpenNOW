# Troubleshoot unavailable Linux video backends

The Qt stream view requires a hardware decoder. On Intel systems, VA-API decoding can
work without Vulkan Video decoding support. The stream view still uses Vulkan for
presentation; OpenGL is not an embedded streaming backend. Enabling software decoding
in a standalone streamer does not enable it in the Qt stream view.

## Check the host driver first

Record the OpenNOW version, package format, distribution, GPU, and whether you use X11
or Wayland. Export OpenNOW diagnostics after a failed launch. The backend and codec
failure reasons distinguish missing drivers from unsupported profiles.

With `vainfo` installed, check the render node belonging to your GPU:

```sh
ls -l /dev/dri/renderD*
vainfo --display drm --device /dev/dri/renderD128
```

Use the actual render-node path if it differs. For H.264, look for
`VAProfileH264Main` with `VAEntrypointVLD`. If this fails, fix the host's VA-API driver
installation or render-node access first. Do not run OpenNOW as root or make device
nodes world-writable. Intel's [media driver](https://github.com/intel/media-driver)
lists Gemini Lake, the platform used by the Celeron N4020 and UHD 600, as supported.

## Check AppImage driver discovery on Arch and CachyOS

The OpenNOW 1.0.1 AppImage bundles Ubuntu's libva, whose default driver directory is
`/usr/lib/x86_64-linux-gnu/dri`. Arch and CachyOS install VA-API drivers in
`/usr/lib/dri`. A successful host `vainfo` check does not prove that this AppImage can
find the same driver.

For that release, test the driver path in one launch from the download directory:

```sh
LIBVA_DRIVERS_PATH=/usr/lib/dri LIBVA_MESSAGING_LEVEL=2 \
  ./OpenNOW-Qt-1.0.1-Linux-x64.AppImage
```

This command temporarily replaces any custom `LIBVA_DRIVERS_PATH` for that process.
It does not install a driver or change your shell configuration. If the log reaches
`iHD_drv_video.so` but reports a missing library or symbol, retain that error in the
bug report; fixing the search path alone does not fix an incompatible driver binary.

The packaging fix installs an AppRun hook that searches `/usr/lib/dri`, `/usr/lib64/dri`,
and the matching x64 or ARM64 Debian multiarch directory. It preserves an explicitly
set `LIBVA_DRIVERS_PATH`, including an empty value, and never forces a driver name.
The bundled DEB inherits this hook. Flatpak and source builds do not use this hook;
Flatpak drivers must come from its matching runtime, not host library paths.

## Validate the packaged fix on Intel hardware

1. Launch the fixed AppImage without the temporary path override. Check that VAAPI is
   available in Stream settings, then select VAAPI, H.264, 8-bit 4:2:0, and SDR.
2. Start a game at 1280×720 and 60 FPS as an initial test on a low-power laptop. Confirm
   moving video appears in the existing Qt window and diagnostics identify VA-API
   decoding. Audio alone is not evidence of working video.
3. Open statistics, stream menus, and exit confirmation in windowed and fullscreen
   modes. Confirm playback continues and closing an overlay does not restart the stream.
4. End the session and start another. Repeat on X11 and Wayland where available.

The Python packaging tests verify path selection and launcher registration without a
GPU. They do not certify decode, DMA-BUF import, or gameplay on the target laptop.
