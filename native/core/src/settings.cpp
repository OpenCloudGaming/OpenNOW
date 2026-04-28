#include "opennow/settings.hpp"

#include <array>

namespace opennow {

namespace {

constexpr std::array<VideoCodec, 3> kSupportedCodecs = {
    VideoCodec::H264,
    VideoCodec::H265,
    VideoCodec::AV1,
};

constexpr std::array<ColorQuality, 4> kSupportedColorQualities = {
    ColorQuality::EightBit420,
    ColorQuality::EightBit444,
    ColorQuality::TenBit420,
    ColorQuality::TenBit444,
};

template <typename T, std::size_t Size>
bool contains(const std::array<T, Size>& values, T value) {
  for (const auto candidate : values) {
    if (candidate == value) {
      return true;
    }
  }
  return false;
}

}  // namespace

int color_quality_bit_depth(ColorQuality quality) {
  return color_quality_is_10_bit(quality) ? 10 : 0;
}

int color_quality_chroma_format(ColorQuality quality) {
  return quality == ColorQuality::EightBit444 ||
                 quality == ColorQuality::TenBit444
             ? 2
             : 0;
}

bool color_quality_requires_hevc(ColorQuality quality) {
  return quality != ColorQuality::EightBit420;
}

bool color_quality_is_10_bit(ColorQuality quality) {
  return quality == ColorQuality::TenBit420 ||
         quality == ColorQuality::TenBit444;
}

StreamPreferences normalize_stream_preferences(
    VideoCodec codec,
    ColorQuality color_quality) {
  const auto normalized_codec =
      contains(kSupportedCodecs, codec) ? codec : VideoCodec::H264;
  const auto normalized_color_quality =
      contains(kSupportedColorQualities, color_quality) ? color_quality
                                                        : ColorQuality::EightBit420;

  return {
      normalized_codec,
      normalized_color_quality,
      normalized_codec != codec || normalized_color_quality != color_quality,
  };
}

std::string to_string(VideoCodec codec) {
  switch (codec) {
    case VideoCodec::H264:
      return "H264";
    case VideoCodec::H265:
      return "H265";
    case VideoCodec::AV1:
      return "AV1";
  }
  return "H264";
}

std::string to_string(ColorQuality quality) {
  switch (quality) {
    case ColorQuality::EightBit420:
      return "8bit_420";
    case ColorQuality::EightBit444:
      return "8bit_444";
    case ColorQuality::TenBit420:
      return "10bit_420";
    case ColorQuality::TenBit444:
      return "10bit_444";
  }
  return "8bit_420";
}

std::string resolve_gfn_keyboard_layout(
    const std::string& layout,
    const std::string& platform) {
  if (platform == "darwin") {
    if (layout == "en-US") {
      return "m-us";
    }
    if (layout == "en-GB") {
      return "m-brit";
    }
    if (layout == "tr-TR") {
      return "m-tr-qty";
    }
  }

  static constexpr std::array<const char*, 16> kKnownLayouts = {
      "en-US", "en-GB", "tr-TR", "de-DE", "fr-FR", "es-ES",
      "es-MX", "it-IT", "pt-PT", "pt-BR", "pl-PL", "ru-RU",
      "ja-JP", "ko-KR", "zh-CN", "zh-TW",
  };

  for (const auto* candidate : kKnownLayouts) {
    if (layout == candidate) {
      return layout;
    }
  }

  return "en-US";
}

}  // namespace opennow
