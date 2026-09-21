#include "streaming/PhysicalKeyMap.h"
#include "streaming/PhysicalKeyMapData.h"

namespace PhysicalKeyMap {

namespace {

char normalizedCharacter(char c)
{
    if (c == '_') return '-';
    return c >= 'A' && c <= 'Z' ? static_cast<char>(c - 'A' + 'a') : c;
}

bool matchesLocale(std::string_view left, std::string_view right)
{
    if (left.size() != right.size()) return false;
    for (std::size_t index = 0; index < left.size(); ++index) {
        if (normalizedCharacter(left[index]) != normalizedCharacter(right[index])) return false;
    }
    return true;
}

}

const Layout *layoutFor(std::string_view locale)
{
    if (matchesLocale(locale, "ja-106") || matchesLocale(locale, "Japanese106")) locale = "ja-JP";
    if (matchesLocale(locale, "es-ES_tradnl")) locale = "es-ES";
    if (matchesLocale(locale, "no-NO") || matchesLocale(locale, "nn-NO")
        || matchesLocale(locale, "no") || matchesLocale(locale, "nn")) locale = "nb-NO";
    for (const auto &layout : layouts) {
        if (matchesLocale(locale, layout.locale)
            || matchesLocale(locale, layout.locale.substr(0, layout.locale.find('-'))))
            return &layout;
    }
    return nullptr;
}

std::uint16_t virtualKey(const Layout *layout, std::uint32_t evdevCode)
{
    return layout && evdevCode < layout->virtualKeys.size() ? layout->virtualKeys[evdevCode] : 0;
}

std::uint32_t evdevCodeFromNativeScanCode(std::uint32_t nativeScanCode)
{
    return nativeScanCode >= 8 ? nativeScanCode - 8 : 0;
}

}
