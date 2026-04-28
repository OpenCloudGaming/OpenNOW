#pragma once

#include <map>
#include <optional>
#include <string>

#include "opennow/services.hpp"

namespace opennow {

constexpr const char* kNvidiaClientId = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
constexpr const char* kNvidiaScopes = "openid consent email tk_client age";
constexpr const char* kNvidiaAuthEndpoint = "https://login.nvidia.com/authorize";
constexpr const char* kGfnUserAgent =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 "
    "NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
constexpr const char* kGfnClientVersion = "2.0.80.173";

struct AuthUrlRequest {
  LoginProvider provider;
  std::string pkce_challenge;
  int redirect_port = 2259;
  std::string nonce;
  std::string device_id;
};

struct RequestHeadersOptions {
  std::string token;
  std::string client_id;
  std::string device_id;
  bool include_origin = true;
};

struct SignalingUrlResult {
  std::string signaling_url;
  std::optional<std::string> signaling_host;
};

std::string build_auth_url(const AuthUrlRequest& request);
std::map<std::string, std::string> build_cloudmatch_headers(
    const RequestHeadersOptions& options);
SignalingUrlResult build_signaling_url(
    const std::string& raw,
    const std::string& server_ip);
std::string build_signaling_sign_in_url(
    const std::string& signaling_server,
    const std::string& peer_name,
    const std::optional<std::string>& signaling_url);
std::string build_peer_info_message(int ack_id, int peer_id, const std::string& peer_name);
std::string build_answer_message(
    int ack_id,
    int peer_id,
    const std::string& sdp,
    const std::optional<std::string>& nvst_sdp);
std::string build_ice_candidate_message(
    int ack_id,
    int peer_id,
    const std::string& candidate,
    const std::optional<std::string>& sdp_mid,
    const std::optional<int>& sdp_mline_index);
std::string build_keyframe_request_message(
    int ack_id,
    int peer_id,
    const std::string& reason,
    int backlog_frames,
    int attempt);

}  // namespace opennow
