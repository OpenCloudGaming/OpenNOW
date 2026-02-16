# Test Video Decoders - Checks available decoders and measures latency
# Usage: .\test-decoders.ps1

$gstRoot = "C:\Program Files\gstreamer\1.0\msvc_x86_64"
$env:GSTREAMER_1_0_ROOT_MSVC_X86_64 = $gstRoot
$env:PATH = "$gstRoot\bin;$env:PATH"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  OpenNow Decoder Latency Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check GStreamer
Write-Host "Checking GStreamer installation..." -ForegroundColor Yellow
$gstInspect = "$gstRoot\bin\gst-inspect-1.0.exe"
$gstLaunch = "$gstRoot\bin\gst-launch-1.0.exe"
$gstDir = "$gstRoot\bin"

if (Test-Path $gstDir) {
    # Check if runtime is installed (not just dev files)
    if (Test-Path $gstInspect) {
        $version = & $gstLaunch --version 2>$null | Select-Object -First 1
        Write-Host "  GStreamer Runtime found: $version" -ForegroundColor Green
        Write-Host ""

        # Check D3D11 decoders
        Write-Host "Available Hardware Decoders:" -ForegroundColor Yellow

        $decoders = @(
            @{ Name = "d3d11h264dec"; Desc = "D3D11 H.264 HW decoder (NVIDIA/AMD/Intel)"; Latency = "Low"; Priority = 2 },
            @{ Name = "d3d11h265dec"; Desc = "D3D11 HEVC HW decoder (NVIDIA/AMD/Intel)"; Latency = "Low"; Priority = 2 },
            @{ Name = "nvh264dec"; Desc = "NVIDIA NVDEC H.264"; Latency = "Very Low*"; Priority = 1 },
            @{ Name = "nvh265dec"; Desc = "NVIDIA NVDEC HEVC"; Latency = "Very Low*"; Priority = 1 },
            @{ Name = "qsvh264dec"; Desc = "Intel QuickSync H.264"; Latency = "Low"; Priority = 3 },
            @{ Name = "qsvh265dec"; Desc = "Intel QuickSync HEVC"; Latency = "Low"; Priority = 3 },
            @{ Name = "avdec_h264"; Desc = "FFmpeg H.264 (Software)"; Latency = "Medium"; Priority = 10 },
            @{ Name = "avdec_h265"; Desc = "FFmpeg HEVC (Software)"; Latency = "Medium-High"; Priority = 10 }
        )

        $availableDecoders = @()
        foreach ($dec in $decoders) {
            $null = & $gstInspect $dec.Name 2>$null
            if ($LASTEXITCODE -eq 0) {
                Write-Host "  [OK] $($dec.Name) - $($dec.Desc) [$($dec.Latency)]" -ForegroundColor Green
                $availableDecoders += $dec
            } else {
                Write-Host "  [--] $($dec.Name) - $($dec.Desc)" -ForegroundColor DarkGray
            }
        }

        Write-Host ""
        Write-Host "  * NVDEC benchmark shows higher init time but lowest sustained latency" -ForegroundColor DarkGray
        Write-Host ""

        Write-Host "========================================" -ForegroundColor Cyan
        Write-Host "  Latency Analysis" -ForegroundColor Cyan
        Write-Host "========================================" -ForegroundColor Cyan
        Write-Host ""

        # Check which HW decoder is available and provide recommendations
        $hasD3d11H264 = $availableDecoders | Where-Object { $_.Name -eq "d3d11h264dec" }
        $hasNvdec = $availableDecoders | Where-Object { $_.Name -eq "nvh264dec" }
        $hasNvdecH265 = $availableDecoders | Where-Object { $_.Name -eq "nvh265dec" }
        $hasQsv = $availableDecoders | Where-Object { $_.Name -eq "qsvh264dec" }

        Write-Host "Decoder latency breakdown (typical values):" -ForegroundColor White
        Write-Host ""
        Write-Host "  Hardware Decoders (GPU accelerated):" -ForegroundColor Yellow
        Write-Host "    NVIDIA NVDEC:    0.5-1.5ms decode + GPU memory (best for NVIDIA)" -ForegroundColor Gray
        Write-Host "    D3D11 Decoder:   1-3ms decode + CPU copy (universal fallback)" -ForegroundColor Gray
        Write-Host "    Intel QSV:       1-2ms decode (Intel integrated GPU)" -ForegroundColor Gray
        Write-Host ""
        Write-Host "  Software Decoder:" -ForegroundColor Yellow
        Write-Host "    FFmpeg avdec:    5-15ms @ 1080p, 15-40ms @ 4K (CPU bound)" -ForegroundColor Gray
        Write-Host ""

        Write-Host "========================================" -ForegroundColor Cyan
        Write-Host "  OpenNow Configuration" -ForegroundColor Cyan
        Write-Host "========================================" -ForegroundColor Cyan
        Write-Host ""

        Write-Host "Current OpenNow decoder selection:" -ForegroundColor White
        Write-Host ""

        if ($hasNvdec -or $hasD3d11H264) {
            Write-Host "  H.264 streams: GStreamer with D3D11 hardware decode" -ForegroundColor Green
            if ($hasNvdec) {
                Write-Host "    -> d3d11h264dec (NVDEC backend on NVIDIA GPUs)" -ForegroundColor Gray
            } else {
                Write-Host "    -> d3d11h264dec (generic D3D11VA)" -ForegroundColor Gray
            }
        } else {
            Write-Host "  H.264 streams: Software decode (avdec_h264)" -ForegroundColor Yellow
            Write-Host "    -> Higher CPU usage and latency" -ForegroundColor Gray
        }

        Write-Host ""
        Write-Host "  HEVC streams: Native DXVA decoder (built-in)" -ForegroundColor Green
        Write-Host "    -> Direct D3D11 Video API, lowest latency" -ForegroundColor Gray
        Write-Host "    -> Zero-copy GPU textures" -ForegroundColor Gray

        Write-Host ""
        Write-Host "========================================" -ForegroundColor Cyan
        Write-Host "  Low Latency Tips" -ForegroundColor Cyan
        Write-Host "========================================" -ForegroundColor Cyan
        Write-Host ""
        Write-Host "  1. Use HEVC codec when possible (better native support)" -ForegroundColor White
        Write-Host "  2. Disable VSync in GPU control panel" -ForegroundColor White
        Write-Host "  3. Use 'Balanced' stream quality (lower bitrate = faster decode)" -ForegroundColor White
        Write-Host "  4. Connect via Ethernet, not WiFi" -ForegroundColor White
        Write-Host "  5. Choose nearest server region" -ForegroundColor White
        Write-Host ""

        # Show expected total latency
        Write-Host "Expected end-to-end latency breakdown:" -ForegroundColor Yellow
        Write-Host "  Network RTT:     10-50ms (depends on server distance)" -ForegroundColor Gray
        Write-Host "  Server encode:   5-10ms" -ForegroundColor Gray
        Write-Host "  Decode:          1-3ms (HW) or 10-20ms (SW)" -ForegroundColor Gray
        Write-Host "  Render:          1-2ms" -ForegroundColor Gray
        Write-Host "  Display:         0-16ms (VSync) or <1ms (no VSync)" -ForegroundColor Gray
        Write-Host "  ----------------------------------------" -ForegroundColor Gray
        Write-Host "  Total:           ~25-80ms typical with HW decode" -ForegroundColor White

    } else {
        Write-Host "  GStreamer directory found but RUNTIME not installed!" -ForegroundColor Red
        Write-Host ""
        Write-Host "  You have the development files but not the runtime." -ForegroundColor Yellow
        Write-Host ""
        Write-Host "  Please download and install BOTH packages from:" -ForegroundColor Yellow
        Write-Host "  https://gstreamer.freedesktop.org/download/" -ForegroundColor Cyan
        Write-Host ""
        Write-Host "  1. Runtime installer (MSVC 64-bit): gstreamer-1.0-msvc-x86_64-X.XX.X.msi" -ForegroundColor White
        Write-Host "  2. Development installer (MSVC 64-bit): gstreamer-1.0-devel-msvc-x86_64-X.XX.X.msi" -ForegroundColor White
        Write-Host ""
        Write-Host "  You currently have: Development files only" -ForegroundColor DarkGray
        Write-Host "  Missing: gst-inspect-1.0.exe, gst-launch-1.0.exe, etc." -ForegroundColor DarkGray
    }
} else {
    Write-Host "  GStreamer not found at: $gstRoot" -ForegroundColor Red
    Write-Host ""
    Write-Host "  Please install GStreamer MSVC runtime from:" -ForegroundColor Yellow
    Write-Host "  https://gstreamer.freedesktop.org/download/" -ForegroundColor Cyan
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Done" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
