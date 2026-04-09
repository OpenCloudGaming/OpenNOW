#include "opennow/native/video_decoder_selector.hpp"

#include <algorithm>
#include <cctype>
#include <fstream>

namespace opennow::native {

namespace {

std::string NormalizeCodec(std::string codec) {
  std::transform(codec.begin(), codec.end(), codec.begin(), [](unsigned char c) {
    return static_cast<char>(std::toupper(c));
  });
  if (codec == "HEVC") {
    return "H265";
  }
  return codec;
}

std::string SoftwarePathDescription(const std::string& normalized_codec, bool prefer_rgba_upload) {
  if (normalized_codec == "AV1") {
    return prefer_rgba_upload ? "video path: software AV1 decode + RGBA upload fallback"
                              : "video path: software AV1 decode + SDL YUV/RGBA upload fallback";
  }
  return prefer_rgba_upload ? "video path: software decode + RGBA upload fallback"
                            : "video path: software decode + SDL YUV/RGBA upload fallback";
}

#if defined(__linux__) && (defined(__arm__) || defined(__aarch64__))
std::string ReadSmallTextFile(const char* path) {
  std::ifstream stream(path, std::ios::in | std::ios::binary);
  if (!stream) {
    return {};
  }
  std::string contents((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
  return contents;
}
#endif

}  // namespace

bool IsRaspberryPi4Runtime() {
#if defined(__linux__) && (defined(__arm__) || defined(__aarch64__))
  const std::string model =
      ReadSmallTextFile("/proc/device-tree/model").empty() ? ReadSmallTextFile("/sys/firmware/devicetree/base/model")
                                                           : ReadSmallTextFile("/proc/device-tree/model");
  return model.find("Raspberry Pi 4") != std::string::npos;
#else
  return false;
#endif
}

std::vector<VideoDecoderCandidate> BuildVideoDecoderCandidates(const std::string& codec, bool force_software_decode, bool prefer_rgba_upload) {
  const std::string normalized_codec = NormalizeCodec(codec);
  std::vector<VideoDecoderCandidate> candidates;

  if (force_software_decode) {
    candidates.push_back({"", false, false, SoftwarePathDescription(normalized_codec, prefer_rgba_upload)});
    return candidates;
  }

#if defined(__APPLE__)
  candidates.push_back({"", true, true, "video path: macOS VideoToolbox hardware decode + SDL YUV GPU upload"});
#endif

  if (IsRaspberryPi4Runtime()) {
    if (normalized_codec == "H264") {
      candidates.push_back({"h264_v4l2m2m", true, false, "video path: Raspberry Pi 4 V4L2 M2M H264 hardware decode + SDL YUV/RGBA upload"});
    } else if (normalized_codec == "H265") {
      candidates.push_back({"hevc_v4l2m2m", true, false, "video path: Raspberry Pi 4 V4L2 M2M HEVC hardware decode + SDL YUV/RGBA upload"});
    }
  }

  candidates.push_back({"", false, false, SoftwarePathDescription(normalized_codec, prefer_rgba_upload)});
  return candidates;
}

}  // namespace opennow::native
