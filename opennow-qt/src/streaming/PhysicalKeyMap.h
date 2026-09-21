#pragma once

#include <array>
#include <cstdint>
#include <string_view>

namespace PhysicalKeyMap {

struct Layout {
    std::string_view locale;
    std::array<std::uint16_t, 128> virtualKeys;
};

[[nodiscard]] const Layout *layoutFor(std::string_view locale);
[[nodiscard]] std::uint16_t virtualKey(const Layout *layout, std::uint32_t evdevCode);
[[nodiscard]] std::uint32_t evdevCodeFromNativeScanCode(std::uint32_t nativeScanCode);

}
