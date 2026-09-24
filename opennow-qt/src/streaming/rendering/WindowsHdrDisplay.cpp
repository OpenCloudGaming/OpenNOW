#include "streaming/rendering/WindowsHdrDisplay.h"

#include <QWindow>
#include <cmath>

#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <dxgi1_6.h>
#include <wrl/client.h>

static_assert(DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 == 12);
#endif

std::optional<WindowsHdrDisplay> validatedWindowsHdrDisplay(
    int colorSpace, double minimumNits, double maximumNits, double maximumFullFrameNits,
    const HdrChromaticity &chromaticity)
{
    if (colorSpace != 12 || !std::isfinite(minimumNits) || minimumNits < 0.0
        || !std::isfinite(maximumNits) || maximumNits <= minimumNits
        || maximumNits > 10'000.0) return std::nullopt;

    WindowsHdrDisplay display{minimumNits, maximumNits};
    if (!std::isfinite(maximumFullFrameNits) || maximumFullFrameNits <= minimumNits
        || maximumFullFrameNits > maximumNits) return display;
    const double coordinates[] = {chromaticity.redX, chromaticity.redY,
        chromaticity.greenX, chromaticity.greenY, chromaticity.blueX, chromaticity.blueY,
        chromaticity.whiteX, chromaticity.whiteY};
    bool valid = true;
    for (int i = 0; i < 8; i += 2)
        valid = valid && std::isfinite(coordinates[i]) && std::isfinite(coordinates[i + 1])
            && coordinates[i] >= 0.0 && coordinates[i] <= 1.0
            && coordinates[i + 1] > 0.0 && coordinates[i + 1] <= 1.0
            && coordinates[i] + coordinates[i + 1] <= 1.0;
    const double area = (chromaticity.greenX - chromaticity.redX)
            * (chromaticity.blueY - chromaticity.redY)
        - (chromaticity.greenY - chromaticity.redY)
            * (chromaticity.blueX - chromaticity.redX);
    if (valid && std::isfinite(area) && std::abs(area) > 1e-6) {
        display.maximumFullFrameNits = maximumFullFrameNits;
        display.chromaticity = chromaticity;
    }
    return display;
}

std::optional<WindowsHdrDisplay> activeWindowsHdrDisplay(QWindow *window)
{
#ifdef Q_OS_WIN
    if (!window || !window->isVisible()) return std::nullopt;
    const auto hwnd = reinterpret_cast<HWND>(window->winId());
    const HMONITOR monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
    if (!monitor) return std::nullopt;

    using Microsoft::WRL::ComPtr;
    ComPtr<IDXGIFactory1> factory;
    if (FAILED(CreateDXGIFactory1(IID_PPV_ARGS(&factory)))) return std::nullopt;
    std::optional<WindowsHdrDisplay> fallback;
    for (UINT adapterIndex = 0; adapterIndex < 64; ++adapterIndex) {
        ComPtr<IDXGIAdapter1> adapter;
        if (factory->EnumAdapters1(adapterIndex, &adapter) == DXGI_ERROR_NOT_FOUND) break;
        if (!adapter) continue;
        for (UINT outputIndex = 0; outputIndex < 64; ++outputIndex) {
            ComPtr<IDXGIOutput> output;
            if (adapter->EnumOutputs(outputIndex, &output) == DXGI_ERROR_NOT_FOUND) break;
            if (!output) continue;
            DXGI_OUTPUT_DESC identity{};
            if (FAILED(output->GetDesc(&identity)) || identity.Monitor != monitor) continue;
            ComPtr<IDXGIOutput6> output6;
            if (FAILED(output.As(&output6))) continue;
            DXGI_OUTPUT_DESC1 desc{};
            if (FAILED(output6->GetDesc1(&desc))) continue;
            const auto display = validatedWindowsHdrDisplay(int(desc.ColorSpace), desc.MinLuminance,
                desc.MaxLuminance, desc.MaxFullFrameLuminance,
                {desc.RedPrimary[0], desc.RedPrimary[1], desc.GreenPrimary[0],
                 desc.GreenPrimary[1], desc.BluePrimary[0], desc.BluePrimary[1],
                 desc.WhitePoint[0], desc.WhitePoint[1]});
            if (display && display->chromaticity) return display;
            if (display && !fallback) fallback = display;
        }
    }
    return fallback;
#else
    Q_UNUSED(window)
    return std::nullopt;
#endif
}
