#include "opennow/service_builders.hpp"

#include <cctype>
#include <iomanip>
#include <sstream>
#include <utility>

namespace opennow {

namespace {

std::string percent_encode(const std::string& value) {
  std::ostringstream out;
  out << std::uppercase << std::hex;
  for (const unsigned char ch : value) {
    if (std::isalnum(ch) || ch == '-' || ch == '_' || ch == '.' || ch == '~') {
      out << ch;
    } else {
      out << '%' << std::setw(2) << std::setfill('0') << static_cast<int>(ch);
    }
  }
  return out.str();
}

std::string json_escape(const std::string& value) {
  std::string escaped;
  escaped.reserve(value.size() + 8);
  for (const char ch : value) {
    switch (ch) {
      case '\\':
        escaped += "\\\\";
        break;
      case '"':
        escaped += "\\\"";
        break;
      case '\n':
        escaped += "\\n";
        break;
      case '\r':
        escaped += "\\r";
        break;
      case '\t':
        escaped += "\\t";
        break;
      default:
        escaped += ch;
        break;
    }
  }
  return escaped;
}

std::string quote(const std::string& value) {
  return "\"" + json_escape(value) + "\"";
}

std::string normalize_base_path(std::string path) {
  if (path.empty()) {
    return "/";
  }
  if (path.back() != '/') {
    path.push_back('/');
  }
  return path;
}

std::pair<std::string, std::string> split_url_host_path(const std::string& url) {
  const auto scheme_end = url.find("://");
  const auto host_start = scheme_end == std::string::npos ? 0 : scheme_end + 3;
  const auto path_start = url.find('/', host_start);
  if (path_start == std::string::npos) {
    return {url.substr(host_start), "/"};
  }
  return {url.substr(host_start, path_start - host_start), url.substr(path_start)};
}

}  // namespace

std::string build_auth_url(const AuthUrlRequest& request) {
  std::ostringstream url;
  url << kNvidiaAuthEndpoint
      << "?response_type=code"
      << "&device_id=" << percent_encode(request.device_id)
      << "&scope=" << percent_encode(kNvidiaScopes)
      << "&client_id=" << percent_encode(kNvidiaClientId)
      << "&redirect_uri=" << percent_encode("http://localhost:" + std::to_string(request.redirect_port))
      << "&ui_locales=en_US"
      << "&nonce=" << percent_encode(request.nonce)
      << "&prompt=select_account"
      << "&code_challenge=" << percent_encode(request.pkce_challenge)
      << "&code_challenge_method=S256"
      << "&idp_id=" << percent_encode(request.provider.idp_id);
  return url.str();
}

std::map<std::string, std::string> build_cloudmatch_headers(
    const RequestHeadersOptions& options) {
  std::map<std::string, std::string> headers = {
      {"Authorization", "GFNJWT " + options.token},
      {"Content-Type", "application/json"},
      {"User-Agent", kGfnUserAgent},
      {"nv-browser-type", "CHROME"},
      {"nv-client-id", options.client_id},
      {"nv-client-streamer", "NVIDIA-CLASSIC"},
      {"nv-client-type", "NATIVE"},
      {"nv-client-version", kGfnClientVersion},
      {"nv-device-make", "UNKNOWN"},
      {"nv-device-model", "UNKNOWN"},
      {"nv-device-os", "MACOS"},
      {"nv-device-type", "DESKTOP"},
      {"x-device-id", options.device_id},
  };

  if (options.include_origin) {
    headers.emplace("Origin", "https://play.geforcenow.com");
    headers.emplace("Referer", "https://play.geforcenow.com/");
  }

  return headers;
}

SignalingUrlResult build_signaling_url(
    const std::string& raw,
    const std::string& server_ip) {
  if (raw.rfind("rtsps://", 0) == 0 || raw.rfind("rtsp://", 0) == 0) {
    const auto without_scheme = raw.rfind("rtsps://", 0) == 0
                                    ? raw.substr(8)
                                    : raw.substr(7);
    const auto host_end = without_scheme.find_first_of(":/");
    const auto host = without_scheme.substr(0, host_end);
    if (!host.empty() && host.front() != '.') {
      return {"wss://" + host + "/nvst/", host};
    }
    return {"wss://" + server_ip + ":443/nvst/", std::nullopt};
  }

  if (raw.rfind("wss://", 0) == 0) {
    const auto [host, _path] = split_url_host_path(raw);
    return {raw, host};
  }

  if (!raw.empty() && raw.front() == '/') {
    return {"wss://" + server_ip + ":443" + raw, std::nullopt};
  }

  return {"wss://" + server_ip + ":443/nvst/", std::nullopt};
}

std::string build_signaling_sign_in_url(
    const std::string& signaling_server,
    const std::string& peer_name,
    const std::optional<std::string>& signaling_url) {
  std::string base = signaling_url && !signaling_url->empty()
                         ? *signaling_url
                         : "wss://" + signaling_server + (signaling_server.find(':') == std::string::npos ? ":443" : "") + "/nvst/";
  if (base.rfind("wss://", 0) != 0) {
    const auto scheme_end = base.find("://");
    base = scheme_end == std::string::npos ? "wss://" + base : "wss://" + base.substr(scheme_end + 3);
  }

  const auto [host, path] = split_url_host_path(base);
  return "wss://" + host + normalize_base_path(path) + "sign_in?peer_id=" +
         percent_encode(peer_name) + "&version=2";
}

std::string build_peer_info_message(int ack_id, int peer_id, const std::string& peer_name) {
  std::ostringstream json;
  json << "{\"ackid\":" << ack_id
       << ",\"peer_info\":{\"browser\":\"Chrome\",\"browserVersion\":\"131\","
       << "\"connected\":true,\"id\":" << peer_id
       << ",\"name\":" << quote(peer_name)
       << ",\"peerRole\":0,\"resolution\":\"1920x1080\",\"version\":2}}";
  return json.str();
}

std::string build_answer_message(
    int ack_id,
    int peer_id,
    const std::string& sdp,
    const std::optional<std::string>& nvst_sdp) {
  std::ostringstream inner;
  inner << "{\"type\":\"answer\",\"sdp\":" << quote(sdp);
  if (nvst_sdp && !nvst_sdp->empty()) {
    inner << ",\"nvstSdp\":" << quote(*nvst_sdp);
  }
  inner << "}";

  std::ostringstream json;
  json << "{\"peer_msg\":{\"from\":" << peer_id
       << ",\"to\":1,\"msg\":" << quote(inner.str())
       << "},\"ackid\":" << ack_id << "}";
  return json.str();
}

std::string build_ice_candidate_message(
    int ack_id,
    int peer_id,
    const std::string& candidate,
    const std::optional<std::string>& sdp_mid,
    const std::optional<int>& sdp_mline_index) {
  std::ostringstream inner;
  inner << "{\"candidate\":" << quote(candidate)
        << ",\"sdpMid\":";
  if (sdp_mid) {
    inner << quote(*sdp_mid);
  } else {
    inner << "null";
  }
  inner << ",\"sdpMLineIndex\":";
  if (sdp_mline_index) {
    inner << *sdp_mline_index;
  } else {
    inner << "null";
  }
  inner << "}";

  std::ostringstream json;
  json << "{\"peer_msg\":{\"from\":" << peer_id
       << ",\"to\":1,\"msg\":" << quote(inner.str())
       << "},\"ackid\":" << ack_id << "}";
  return json.str();
}

std::string build_keyframe_request_message(
    int ack_id,
    int peer_id,
    const std::string& reason,
    int backlog_frames,
    int attempt) {
  std::ostringstream inner;
  inner << "{\"type\":\"request_keyframe\",\"reason\":" << quote(reason)
        << ",\"backlogFrames\":" << backlog_frames
        << ",\"attempt\":" << attempt << "}";

  std::ostringstream json;
  json << "{\"peer_msg\":{\"from\":" << peer_id
       << ",\"to\":1,\"msg\":" << quote(inner.str())
       << "},\"ackid\":" << ack_id << "}";
  return json.str();
}

}  // namespace opennow
