#pragma once

#include <cstdint>
#include <optional>
#include <string>

#include "opennow/types.hpp"

namespace opennow {

struct IceCredentials {
  std::string ufrag;
  std::string pwd;
  std::string fingerprint;
};

struct RewriteResult {
  std::string sdp;
  int replacements = 0;
};

struct NvstSdpParams {
  int width = 1920;
  int height = 1080;
  int fps = 60;
  int max_bitrate_kbps = 15000;
  VideoCodec codec = VideoCodec::H264;
  ColorQuality color_quality = ColorQuality::TenBit420;
  IceCredentials credentials;
  int partial_reliable_threshold_ms = 40;
  std::optional<std::uint32_t> hid_device_mask;
  std::optional<std::uint32_t> enable_partially_reliable_transfer_gamepad;
  std::optional<std::uint32_t> enable_partially_reliable_transfer_hid;
};

std::optional<std::string> extract_public_ip(const std::string& host_or_ip);
std::string fix_server_ip(const std::string& sdp, const std::string& server_ip);
std::string extract_ice_ufrag_from_offer(const std::string& sdp);
IceCredentials extract_ice_credentials(const std::string& sdp);
RewriteResult rewrite_h265_tier_flag(const std::string& sdp, int tier_flag);
std::string munge_answer_sdp(const std::string& sdp, int max_bitrate_kbps);
std::string build_nvst_sdp(const NvstSdpParams& params);

}  // namespace opennow
