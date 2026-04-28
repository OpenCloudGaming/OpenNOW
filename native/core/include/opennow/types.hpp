#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace opennow {

enum class VideoCodec {
  H264,
  H265,
  AV1,
};

enum class VideoAccelerationPreference {
  Auto,
  Hardware,
  Software,
};

enum class ColorQuality {
  EightBit420,
  EightBit444,
  TenBit420,
  TenBit444,
};

enum class MicrophoneMode {
  Disabled,
  PushToTalk,
  VoiceActivity,
};

enum class AspectRatio {
  Ratio16x9,
  Ratio16x10,
  Ratio21x9,
  Ratio32x9,
};

struct LoginProvider {
  std::string idp_id;
  std::string code;
  std::string display_name;
  std::string streaming_service_url;
  int priority = 0;
};

struct AuthTokens {
  std::string access_token;
  std::optional<std::string> refresh_token;
  std::optional<std::string> id_token;
  std::int64_t expires_at_ms = 0;
  std::optional<std::string> client_token;
  std::optional<std::int64_t> client_token_expires_at_ms;
  std::optional<std::int64_t> client_token_lifetime_ms;
};

struct AuthUser {
  std::string user_id;
  std::string display_name;
  std::optional<std::string> email;
  std::optional<std::string> avatar_url;
  std::string membership_tier;
};

struct AuthSession {
  LoginProvider provider;
  AuthTokens tokens;
  AuthUser user;
};

struct IceServer {
  std::vector<std::string> urls;
  std::optional<std::string> username;
  std::optional<std::string> credential;
};

struct SessionInfo {
  std::string id;
  std::string app_id;
  std::optional<std::string> server_ip;
  std::optional<std::string> signaling_server;
  std::optional<std::string> signaling_url;
  std::vector<IceServer> ice_servers;
};

struct StreamDiagnostics {
  std::string connection_state = "closed";
  bool input_ready = false;
  int connected_gamepads = 0;
  std::string resolution;
  std::string codec;
  bool is_hdr = false;
  int bitrate_kbps = 0;
  double decode_fps = 0.0;
  double render_fps = 0.0;
  int packets_lost = 0;
  int packets_received = 0;
  double packet_loss_percent = 0.0;
  double jitter_ms = 0.0;
  double rtt_ms = 0.0;
  int frames_received = 0;
  int frames_decoded = 0;
  int frames_dropped = 0;
  double decode_time_ms = 0.0;
  double render_time_ms = 0.0;
  double jitter_buffer_delay_ms = 0.0;
  std::size_t input_queue_buffered_bytes = 0;
  std::size_t input_queue_peak_buffered_bytes = 0;
  std::size_t partially_reliable_input_queue_buffered_bytes = 0;
  std::size_t partially_reliable_input_queue_peak_buffered_bytes = 0;
  std::size_t input_queue_drop_count = 0;
  double input_queue_max_scheduling_delay_ms = 0.0;
  bool partially_reliable_input_open = false;
  std::string mouse_move_transport = "reliable";
  int mouse_flush_interval_ms = 0;
  double mouse_packets_per_second = 0.0;
  double mouse_residual_magnitude = 0.0;
  bool mouse_adaptive_flush_active = false;
  std::string lag_reason = "unknown";
  std::string lag_reason_detail;
  std::string gpu_type;
  std::string server_region;
  bool decoder_pressure_active = false;
  int decoder_recovery_attempts = 0;
  std::string decoder_recovery_action;
  std::string mic_state = "disabled";
  bool mic_enabled = false;
  std::size_t input_packets_sent = 0;
  std::size_t reliable_input_packets_sent = 0;
  std::size_t partially_reliable_input_packets_sent = 0;
  std::size_t input_bytes_sent = 0;
  std::size_t ice_candidates_added = 0;
  bool negotiation_ready = false;
  std::string backend_detail;
  std::string last_error;
};

}  // namespace opennow
