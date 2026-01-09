# Bundle GStreamer for OpenNow Streamer
# This script copies the required GStreamer DLLs to the app's directory
#
# Prerequisites:
# 1. Install GStreamer MSVC runtime from https://gstreamer.freedesktop.org/download/
#    - Choose the MSVC 64-bit runtime installer
#    - Install with default options
#
# Usage:
#   .\bundle-gstreamer.ps1 -OutputDir "target\release"
#   .\bundle-gstreamer.ps1 -OutputDir "target\debug" -Minimal

param(
    [Parameter(Mandatory=$true)]
    [string]$OutputDir,

    [switch]$Minimal,  # Only copy essential plugins for H.264 decoding

    [string]$GStreamerRoot = $env:GSTREAMER_1_0_ROOT_MSVC_X86_64
)

$ErrorActionPreference = "Stop"

# Find GStreamer installation
if (-not $GStreamerRoot) {
    # Try common installation paths
    $possiblePaths = @(
        "C:\gstreamer\1.0\msvc_x86_64",
        "C:\Program Files\gstreamer\1.0\msvc_x86_64",
        "C:\Program Files (x86)\gstreamer\1.0\msvc_x86_64"
    )

    foreach ($path in $possiblePaths) {
        if (Test-Path $path) {
            $GStreamerRoot = $path
            break
        }
    }
}

if (-not $GStreamerRoot -or -not (Test-Path $GStreamerRoot)) {
    Write-Error @"
GStreamer not found! Please install GStreamer MSVC runtime:
1. Download from: https://gstreamer.freedesktop.org/download/
2. Choose 'MSVC 64-bit (VS 2019, Release CRT)' runtime installer
3. Run installer with default options
4. Re-run this script

Or set GSTREAMER_1_0_ROOT_MSVC_X86_64 environment variable to your GStreamer installation path.
"@
    exit 1
}

Write-Host "Using GStreamer from: $GStreamerRoot" -ForegroundColor Cyan

# Create output directories
$gstOutputDir = Join-Path $OutputDir "gstreamer"
$gstBinDir = Join-Path $gstOutputDir "bin"
$gstPluginDir = Join-Path $gstOutputDir "lib\gstreamer-1.0"

New-Item -ItemType Directory -Force -Path $gstBinDir | Out-Null
New-Item -ItemType Directory -Force -Path $gstPluginDir | Out-Null

# Core GStreamer DLLs (always required)
$coreDlls = @(
    "gstreamer-1.0-0.dll",
    "gstbase-1.0-0.dll",
    "gstvideo-1.0-0.dll",
    "gstapp-1.0-0.dll",
    "gstpbutils-1.0-0.dll",
    "gsttag-1.0-0.dll",
    "gstaudio-1.0-0.dll",
    "gstrtp-1.0-0.dll",
    "gstcodecparsers-1.0-0.dll",
    # GLib dependencies
    "glib-2.0-0.dll",
    "gobject-2.0-0.dll",
    "gmodule-2.0-0.dll",
    "gio-2.0-0.dll",
    "intl-8.dll",
    "ffi-8.dll",
    "pcre2-8-0.dll",
    "z-1.dll",
    # Other dependencies
    "orc-0.4-0.dll"
)

# Essential plugins for H.264/HEVC decoding
$essentialPlugins = @(
    # Core plugins
    "gstcoreelements.dll",
    "gstcoretracers.dll",
    "gstvideoparsersbad.dll",     # h264parse, h265parse
    "gstvideoconvertscale.dll",   # videoconvert, videoscale
    # D3D11 hardware decoding (Windows)
    "gstd3d11.dll",               # d3d11h264dec, d3d11h265dec, d3d11download
    # Software fallback
    "gstlibav.dll",               # avdec_h264, avdec_h265 (FFmpeg-based)
    # App source/sink
    "gstapp.dll",                 # appsrc, appsink
    # Video format handling
    "gstvideorate.dll",
    "gstautodetect.dll",
    "gsttypefindfunctions.dll"
)

# Additional plugins for full functionality
$additionalPlugins = @(
    "gstplayback.dll",            # playbin, decodebin
    "gstaudioparsers.dll",
    "gstaudioconvert.dll",
    "gstaudioresample.dll",
    "gstvolume.dll",
    "gstopus.dll",                # Opus audio codec
    "gstrtp.dll",                 # RTP support
    "gstrtpmanager.dll",
    "gstrtsp.dll",
    "gstwasapi.dll",              # Windows audio
    "gstwasapi2.dll"
)

# Copy core DLLs
Write-Host "`nCopying core DLLs..." -ForegroundColor Yellow
$srcBinDir = Join-Path $GStreamerRoot "bin"

foreach ($dll in $coreDlls) {
    $src = Join-Path $srcBinDir $dll
    if (Test-Path $src) {
        Copy-Item $src $gstBinDir -Force
        Write-Host "  Copied: $dll" -ForegroundColor Green
    } else {
        Write-Warning "  Missing: $dll"
    }
}

# Copy plugins
Write-Host "`nCopying plugins..." -ForegroundColor Yellow
$srcPluginDir = Join-Path $GStreamerRoot "lib\gstreamer-1.0"

$pluginsToCopy = if ($Minimal) { $essentialPlugins } else { $essentialPlugins + $additionalPlugins }

foreach ($plugin in $pluginsToCopy) {
    $src = Join-Path $srcPluginDir $plugin
    if (Test-Path $src) {
        Copy-Item $src $gstPluginDir -Force
        Write-Host "  Copied: $plugin" -ForegroundColor Green
    } else {
        Write-Warning "  Missing: $plugin"
    }
}

# Calculate total size
$totalSize = 0
Get-ChildItem -Recurse $gstOutputDir | ForEach-Object { $totalSize += $_.Length }
$sizeMB = [math]::Round($totalSize / 1MB, 2)

Write-Host "`nGStreamer bundle created successfully!" -ForegroundColor Cyan
Write-Host "Location: $gstOutputDir" -ForegroundColor Cyan
Write-Host "Total size: $sizeMB MB" -ForegroundColor Cyan

# Verify essential components
Write-Host "`nVerifying essential components..." -ForegroundColor Yellow
$d3d11Plugin = Join-Path $gstPluginDir "gstd3d11.dll"
if (Test-Path $d3d11Plugin) {
    Write-Host "  D3D11 hardware decoder: OK" -ForegroundColor Green
} else {
    Write-Warning "  D3D11 hardware decoder: MISSING - H.264 will use software decoding"
}

$libavPlugin = Join-Path $gstPluginDir "gstlibav.dll"
if (Test-Path $libavPlugin) {
    Write-Host "  Software decoder (libav): OK" -ForegroundColor Green
} else {
    Write-Warning "  Software decoder (libav): MISSING"
}

Write-Host "`nDone! The app will automatically detect the bundled GStreamer." -ForegroundColor Green
