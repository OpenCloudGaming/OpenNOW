#include "streaming/PhysicalKeyMap.h"

namespace PhysicalKeyMap {

namespace {

constexpr std::uint32_t kXkbKeycodeOffset = 8;

constexpr std::uint16_t kVkOemPlus = 0xbb;
constexpr std::uint16_t kVkOemComma = 0xbc;
constexpr std::uint16_t kVkOemMinus = 0xbd;
constexpr std::uint16_t kVkOemPeriod = 0xbe;
constexpr std::uint16_t kVkOem1 = 0xba;
constexpr std::uint16_t kVkOem2 = 0xbf;
constexpr std::uint16_t kVkOem3 = 0xc0;
constexpr std::uint16_t kVkOem4 = 0xdb;
constexpr std::uint16_t kVkOem5 = 0xdc;
constexpr std::uint16_t kVkOem6 = 0xdd;
constexpr std::uint16_t kVkOem7 = 0xde;
constexpr std::uint16_t kVkOem102 = 0xe2;

constexpr std::uint32_t kEvdevKey1 = 2;
constexpr std::uint32_t kEvdevKey0 = 11;

char lowerAscii(char c)
{
    return c >= 'A' && c <= 'Z' ? static_cast<char>(c - 'A' + 'a') : c;
}

}

Family familyForLayout(std::string_view layout)
{
    // Only the language subtag matters: "sv-SE" and "sv_se" both select Swedish.
    const auto language = layout.substr(0, layout.find_first_of("-_"));
    if (language.size() != 2) return Family::Default;
    for (const std::string_view nordic : {"sv", "nb", "nn", "no", "da", "fi"}) {
        if (lowerAscii(language[0]) == nordic[0] && lowerAscii(language[1]) == nordic[1])
            return Family::Nordic;
    }
    return Family::Default;
}

std::uint16_t digitRowVirtualKey(std::uint32_t evdevCode)
{
    if (evdevCode < kEvdevKey1 || evdevCode > kEvdevKey0) return 0;
    // evdev orders the row 1..9 then 0, matching VK_1..VK_9 then VK_0.
    return evdevCode == kEvdevKey0 ? 0x30 : static_cast<std::uint16_t>(0x31 + evdevCode - kEvdevKey1);
}

std::uint16_t nordicVirtualKey(std::uint32_t evdevCode)
{
    switch (evdevCode) {
    case 12: return kVkOemPlus;   // + ?
    case 13: return kVkOem4;      // ´ `
    case 26: return kVkOem6;      // å
    case 27: return kVkOem1;      // ¨ ^ ~
    case 39: return kVkOem3;      // ö (æ on Danish)
    case 40: return kVkOem7;      // ä (ø on Danish, æ on Norwegian)
    case 41: return kVkOem5;      // § (½ on Danish)
    case 43: return kVkOem2;      // ' *
    case 51: return kVkOemComma;  // , ;
    case 52: return kVkOemPeriod; // . :
    case 53: return kVkOemMinus;  // - _
    case 86: return kVkOem102;    // < > |
    default: return 0;
    }
}

std::uint32_t evdevCodeFromNativeScanCode(std::uint32_t nativeScanCode)
{
    return nativeScanCode >= kXkbKeycodeOffset ? nativeScanCode - kXkbKeycodeOffset : 0;
}

}
