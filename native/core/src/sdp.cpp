#include "opennow/sdp.hpp"

#include <regex>
#include <sstream>
#include <string_view>
#include <vector>

#include "opennow/input_protocol.hpp"
#include "opennow/settings.hpp"

namespace opennow {

namespace {

std::vector<std::string> split_lines(const std::string& value) {
  std::vector<std::string> lines;
  std::string current;
  for (const char ch : value) {
    if (ch == '\r') {
      continue;
    }
    if (ch == '\n') {
      lines.push_back(current);
      current.clear();
    } else {
      current.push_back(ch);
    }
  }
  if (!current.empty() || (!value.empty() && value.back() == '\n')) {
    lines.push_back(current);
  }
  return lines;
}

std::string join_lines(const std::vector<std::string>& lines, const std::string& ending) {
  std::ostringstream out;
  for (std::size_t i = 0; i < lines.size(); ++i) {
    if (i > 0) {
      out << ending;
    }
    out << lines[i];
  }
  return out.str();
}

bool starts_with(std::string_view value, std::string_view prefix) {
  return value.substr(0, prefix.size()) == prefix;
}

std::string line_ending_for(const std::string& sdp) {
  return sdp.find("\r\n") == std::string::npos ? "\n" : "\r\n";
}

std::string replace_all(std::string value, const std::string& from, const std::string& to) {
  std::size_t position = 0;
  while ((position = value.find(from, position)) != std::string::npos) {
    value.replace(position, from.size(), to);
    position += to.size();
  }
  return value;
}

}  // namespace

std::optional<std::string> extract_public_ip(const std::string& host_or_ip) {
  static const std::regex dotted_ip(R"(^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$)");
  if (std::regex_match(host_or_ip, dotted_ip)) {
    return host_or_ip;
  }

  const auto first_dot = host_or_ip.find('.');
  const auto first_label = host_or_ip.substr(0, first_dot);
  static const std::regex dashed_ip(R"(^(\d{1,3})-(\d{1,3})-(\d{1,3})-(\d{1,3})$)");
  std::smatch match;
  if (std::regex_match(first_label, match, dashed_ip)) {
    return match[1].str() + "." + match[2].str() + "." + match[3].str() + "." + match[4].str();
  }

  return std::nullopt;
}

std::string fix_server_ip(const std::string& sdp, const std::string& server_ip) {
  const auto ip = extract_public_ip(server_ip);
  if (!ip) {
    return sdp;
  }

  auto fixed = replace_all(sdp, "c=IN IP4 0.0.0.0", "c=IN IP4 " + *ip);
  fixed = replace_all(fixed, " 0.0.0.0 ", " " + *ip + " ");
  return fixed;
}

std::string extract_ice_ufrag_from_offer(const std::string& sdp) {
  static const std::regex pattern(R"(a=ice-ufrag:([^\r\n]+))");
  std::smatch match;
  return std::regex_search(sdp, match, pattern) ? match[1].str() : "";
}

IceCredentials extract_ice_credentials(const std::string& sdp) {
  IceCredentials credentials;
  for (const auto& line : split_lines(sdp)) {
    if (starts_with(line, "a=ice-ufrag:")) {
      credentials.ufrag = line.substr(12);
    } else if (starts_with(line, "a=ice-pwd:")) {
      credentials.pwd = line.substr(10);
    } else if (starts_with(line, "a=fingerprint:sha-256 ")) {
      credentials.fingerprint = line.substr(std::string("a=fingerprint:sha-256 ").size());
    }
  }
  return credentials;
}

RewriteResult rewrite_h265_tier_flag(const std::string& sdp, int tier_flag) {
  const auto ending = line_ending_for(sdp);
  auto lines = split_lines(sdp);
  std::vector<std::string> h265_payloads;
  bool in_video = false;

  for (const auto& line : lines) {
    if (starts_with(line, "m=video")) {
      in_video = true;
      continue;
    }
    if (starts_with(line, "m=") && in_video) {
      in_video = false;
    }
    if (!in_video || !starts_with(line, "a=rtpmap:")) {
      continue;
    }
    const auto colon = line.find(':');
    const auto space = line.find(' ', colon + 1);
    if (colon == std::string::npos || space == std::string::npos) {
      continue;
    }
    const auto payload = line.substr(colon + 1, space - colon - 1);
    const auto codec_start = space + 1;
    const auto slash = line.find('/', codec_start);
    const auto codec = line.substr(codec_start, slash - codec_start);
    if (codec == "H265" || codec == "HEVC" || codec == "h265" || codec == "hevc") {
      h265_payloads.push_back(payload);
    }
  }

  int replacements = 0;
  for (auto& line : lines) {
    if (!starts_with(line, "a=fmtp:")) {
      continue;
    }
    const auto colon = line.find(':');
    const auto space = line.find(' ', colon + 1);
    if (colon == std::string::npos || space == std::string::npos) {
      continue;
    }
    const auto payload = line.substr(colon + 1, space - colon - 1);
    bool is_h265 = false;
    for (const auto& candidate : h265_payloads) {
      if (candidate == payload) {
        is_h265 = true;
        break;
      }
    }
    if (!is_h265) {
      continue;
    }
    const auto next = std::regex_replace(
        line,
        std::regex("tier-flag=1", std::regex_constants::icase),
        "tier-flag=" + std::to_string(tier_flag));
    if (next != line) {
      ++replacements;
      line = next;
    }
  }

  return {join_lines(lines, ending), replacements};
}

std::string munge_answer_sdp(const std::string& sdp, int max_bitrate_kbps) {
  const auto ending = line_ending_for(sdp);
  const auto lines = split_lines(sdp);
  std::vector<std::string> result;

  for (std::size_t i = 0; i < lines.size(); ++i) {
    auto line = lines[i];
    if (starts_with(line, "a=fmtp:") && line.find("minptime=") != std::string::npos &&
        line.find("stereo=1") == std::string::npos) {
      line += ";stereo=1";
    }
    result.push_back(line);

    if (starts_with(line, "m=video") || starts_with(line, "m=audio")) {
      const auto next_line = i + 1 < lines.size() ? lines[i + 1] : "";
      if (!starts_with(next_line, "b=")) {
        result.push_back(
            "b=AS:" + std::to_string(starts_with(line, "m=video") ? max_bitrate_kbps : 128));
      }
    }
  }

  return join_lines(result, ending);
}

std::string build_nvst_sdp(const NvstSdpParams& params) {
  const auto min_bitrate = std::max(5000, static_cast<int>(params.max_bitrate_kbps * 0.35));
  const auto initial_bitrate = std::max(min_bitrate, static_cast<int>(params.max_bitrate_kbps * 0.7));
  const auto bit_depth = color_quality_is_10_bit(params.color_quality) ? 10 : 8;
  const auto hid_device_mask =
      params.hid_device_mask.value_or(kPartiallyReliableHidDeviceMaskAll);
  const auto pr_gamepad = params.enable_partially_reliable_transfer_gamepad.value_or(
      kPartiallyReliableGamepadMaskAll);
  const auto pr_hid =
      params.enable_partially_reliable_transfer_hid.value_or(hid_device_mask);

  std::vector<std::string> lines = {
      "v=0",
      "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1",
      "s=-",
      "t=0 0",
      "a=general.icePassword:" + params.credentials.pwd,
      "a=general.iceUserNameFragment:" + params.credentials.ufrag,
      "a=general.dtlsFingerprint:" + params.credentials.fingerprint,
      "m=video 0 RTP/AVP",
      "a=msid:fbc-video-0",
      "a=vqos.fec.rateDropWindow:10",
      "a=vqos.fec.minRequiredFecPackets:2",
      "a=vqos.fec.repairMinPercent:5",
      "a=vqos.fec.repairPercent:5",
      "a=vqos.fec.repairMaxPercent:35",
      "a=vqos.drc.enable:0",
      "a=vqos.dfc.enable:0",
      "a=video.dx9EnableNv12:1",
      "a=video.dx9EnableHdr:1",
      "a=vqos.qpg.enable:1",
      "a=bwe.useOwdCongestionControl:1",
      "a=video.enableRtpNack:1",
      "a=video.clientViewportWd:" + std::to_string(params.width),
      "a=video.clientViewportHt:" + std::to_string(params.height),
      "a=video.maxFPS:" + std::to_string(params.fps),
      "a=video.initialBitrateKbps:" + std::to_string(initial_bitrate),
      "a=video.initialPeakBitrateKbps:" + std::to_string(params.max_bitrate_kbps),
      "a=vqos.bw.maximumBitrateKbps:" + std::to_string(params.max_bitrate_kbps),
      "a=vqos.bw.minimumBitrateKbps:" + std::to_string(min_bitrate),
      "a=vqos.bw.peakBitrateKbps:" + std::to_string(params.max_bitrate_kbps),
      "a=vqos.bw.serverPeakBitrateKbps:" + std::to_string(params.max_bitrate_kbps),
      "a=vqos.bw.enableBandwidthEstimation:1",
      "a=vqos.bw.disableBitrateLimit:0",
      "a=vqos.grc.maximumBitrateKbps:" + std::to_string(params.max_bitrate_kbps),
      "a=vqos.grc.enable:0",
      "a=video.bitDepth:" + std::to_string(bit_depth),
      std::string("a=video.scalingFeature1:") + (params.codec == VideoCodec::AV1 ? "1" : "0"),
      "m=audio 0 RTP/AVP",
      "a=msid:audio",
      "m=mic 0 RTP/AVP",
      "a=msid:mic",
      "a=rtpmap:0 PCMU/8000",
      "m=application 0 RTP/AVP",
      "a=msid:input_1",
      "a=ri.partialReliableThresholdMs:" + std::to_string(params.partial_reliable_threshold_ms),
      "a=ri.hidDeviceMask:" + std::to_string(hid_device_mask),
      "a=ri.enablePartiallyReliableTransferGamepad:" + std::to_string(pr_gamepad),
      "a=ri.enablePartiallyReliableTransferHid:" + std::to_string(pr_hid),
      "",
  };

  return join_lines(lines, "\n");
}

}  // namespace opennow
