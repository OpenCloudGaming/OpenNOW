#pragma once

#include <cstdint>
#include <string_view>

// Windows virtual keys chosen by physical key position (Linux evdev codes).
// GFN receives only a virtual key, and the remote session interprets it with
// the layout requested through the keyboardLayout setting. Qt's logical key
// cannot supply that: keys such as å, ä, ö and § have no US-layout equivalent,
// and shifted digits arrive as punctuation.
namespace PhysicalKeyMap {

enum class Family {
    Default,
    Nordic,
};

[[nodiscard]] Family familyForLayout(std::string_view layout);

// The digit row is VK_0..VK_9 on every Latin layout, shifted or not.
[[nodiscard]] std::uint16_t digitRowVirtualKey(std::uint32_t evdevCode);

// Windows Swedish, Norwegian, Danish and Finnish layouts share OEM key positions.
[[nodiscard]] std::uint16_t nordicVirtualKey(std::uint32_t evdevCode);

// Qt reports XKB keycodes on X11 and Wayland, which are evdev codes plus eight.
[[nodiscard]] std::uint32_t evdevCodeFromNativeScanCode(std::uint32_t nativeScanCode);

}
