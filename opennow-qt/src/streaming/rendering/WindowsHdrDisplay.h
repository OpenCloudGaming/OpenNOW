#pragma once

#include <optional>

class QWindow;

struct HdrChromaticity {
    double redX;
    double redY;
    double greenX;
    double greenY;
    double blueX;
    double blueY;
    double whiteX;
    double whiteY;

    bool operator==(const HdrChromaticity &) const = default;
};

struct WindowsHdrDisplay {
    double minimumNits;
    double maximumNits;
    std::optional<double> maximumFullFrameNits;
    std::optional<HdrChromaticity> chromaticity;

    bool operator==(const WindowsHdrDisplay &) const = default;
};

[[nodiscard]] std::optional<WindowsHdrDisplay> validatedWindowsHdrDisplay(
    int colorSpace, double minimumNits, double maximumNits, double maximumFullFrameNits,
    const HdrChromaticity &chromaticity);
[[nodiscard]] std::optional<WindowsHdrDisplay> activeWindowsHdrDisplay(QWindow *window);
