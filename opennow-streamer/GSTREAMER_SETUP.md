# GStreamer Setup for OpenNow Streamer

OpenNow uses GStreamer for H.264 video decoding on Windows with D3D11 hardware acceleration.

## Installation

### Step 1: Download GStreamer Runtime

1. Go to https://gstreamer.freedesktop.org/download/
2. Under **Windows**, download the **MSVC 64-bit (VS 2019)** runtime installer
   - File: `gstreamer-1.0-msvc-x86_64-X.XX.X.msi`
   - Choose the **runtime** package (not development)

### Step 2: Install GStreamer

1. Run the downloaded MSI installer
2. Choose **Complete** installation (or Custom and select all components)
3. Install to default location: `C:\gstreamer\1.0\msvc_x86_64`

### Step 3: Set Environment Variable (for building)

The installer should set this automatically, but if not:

```powershell
# PowerShell (Admin)
[Environment]::SetEnvironmentVariable("GSTREAMER_1_0_ROOT_MSVC_X86_64", "C:\gstreamer\1.0\msvc_x86_64", "Machine")
[Environment]::SetEnvironmentVariable("PATH", "$env:PATH;C:\gstreamer\1.0\msvc_x86_64\bin", "Machine")
```

### Step 4: Restart your terminal/IDE

Close and reopen your terminal or IDE to pick up the new environment variables.

## Building

After installing GStreamer, build normally:

```bash
cargo build --release
```

## Bundling for Distribution

To bundle GStreamer with your app for distribution:

```powershell
# From the opennow-streamer directory
.\scripts\bundle-gstreamer.ps1 -OutputDir "target\release"

# For minimal bundle (only H.264 decoding):
.\scripts\bundle-gstreamer.ps1 -OutputDir "target\release" -Minimal
```

This creates a `gstreamer/` folder next to your executable with all required DLLs.

## How It Works

- **H.264 streams**: Decoded using GStreamer with D3D11 hardware acceleration (`d3d11h264dec`)
- **HEVC streams**: Decoded using native DXVA decoder (better performance for HEVC)

The app automatically detects bundled GStreamer in the `gstreamer/` subfolder, or falls back to system-installed GStreamer.

## Troubleshooting

### Build Error: "pkg-config not found"

Install pkg-config:
```powershell
# Using Chocolatey
choco install pkgconfiglite

# Or using Scoop
scoop install pkg-config
```

### Build Error: "gstreamer-1.0 not found"

Make sure the environment variable is set correctly:
```powershell
echo $env:GSTREAMER_1_0_ROOT_MSVC_X86_64
# Should output: C:\gstreamer\1.0\msvc_x86_64
```

### Runtime Error: "DLL not found"

1. If using system GStreamer: Add `C:\gstreamer\1.0\msvc_x86_64\bin` to PATH
2. If using bundled GStreamer: Run the bundle script and ensure `gstreamer/` folder is next to the executable

## Required GStreamer Plugins

For H.264 decoding, these plugins are required:
- `gstd3d11.dll` - D3D11 hardware decoder (d3d11h264dec)
- `gstvideoparsersbad.dll` - H.264 parser (h264parse)
- `gstvideoconvertscale.dll` - Video format conversion
- `gstapp.dll` - appsrc/appsink elements
- `gstlibav.dll` - Software decoder fallback (avdec_h264)
