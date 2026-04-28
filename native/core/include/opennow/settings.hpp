#pragma once

#include <string>

#include "opennow/types.hpp"

namespace opennow {

struct StreamPreferences {
  VideoCodec codec = VideoCodec::H264;
  ColorQuality color_quality = ColorQuality::TenBit420;
  bool migrated = false;
};

int color_quality_bit_depth(ColorQuality quality);
int color_quality_chroma_format(ColorQuality quality);
bool color_quality_requires_hevc(ColorQuality quality);
bool color_quality_is_10_bit(ColorQuality quality);

StreamPreferences normalize_stream_preferences(
    VideoCodec codec,
    ColorQuality color_quality);

std::string to_string(VideoCodec codec);
std::string to_string(ColorQuality quality);
std::string resolve_gfn_keyboard_layout(
    const std::string& layout,
    const std::string& platform);

}  // namespace opennow
