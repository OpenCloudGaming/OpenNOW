#pragma once

#include <functional>
#include <optional>
#include <string>
#include <vector>

#include "opennow/types.hpp"

namespace opennow {

struct AuthLoginRequest {
  std::optional<std::string> provider_idp_id;
};

struct AuthRefreshStatus {
  bool attempted = false;
  bool forced = false;
  std::string outcome = "not_attempted";
  std::string message;
  std::optional<std::string> error;
};

struct AuthSessionResult {
  std::optional<AuthSession> session;
  AuthRefreshStatus refresh;
};

struct SubscriptionFetchRequest {
  std::string token;
  std::string user_id;
  std::optional<std::string> provider_streaming_base_url;
  std::optional<std::string> vpc_id;
};

struct EntitledResolution {
  int width = 0;
  int height = 0;
  int fps = 0;
};

struct StorageAddon {
  std::string type = "PERMANENT_STORAGE";
  std::optional<int> size_gb;
  std::optional<int> used_gb;
  std::optional<std::string> region_name;
  std::optional<std::string> region_code;
};

struct SubscriptionInfo {
  std::string membership_tier = "FREE";
  std::optional<std::string> subscription_type;
  std::optional<std::string> subscription_sub_type;
  double allotted_hours = 0.0;
  double purchased_hours = 0.0;
  double rolled_over_hours = 0.0;
  double used_hours = 0.0;
  double remaining_hours = 0.0;
  double total_hours = 0.0;
  std::optional<std::string> first_entitlement_start_date_time;
  std::optional<std::string> server_region_id;
  std::optional<std::string> current_span_start_date_time;
  std::optional<std::string> current_span_end_date_time;
  std::optional<int> notify_user_when_time_remaining_in_minutes;
  std::optional<int> notify_user_on_session_when_remaining_time_in_minutes;
  std::optional<std::string> state;
  std::optional<bool> is_game_play_allowed;
  bool is_unlimited = false;
  std::optional<StorageAddon> storage_addon;
  std::vector<EntitledResolution> entitled_resolutions;
};

struct CatalogBrowseRequest {
  std::optional<std::string> token;
  std::optional<std::string> provider_streaming_base_url;
  std::optional<std::string> search_query;
  std::optional<std::string> sort_id;
  std::vector<std::string> filter_ids;
  std::optional<int> fetch_count;
};

struct GameVariant {
  std::string id;
  std::string store;
  std::vector<std::string> supported_controls;
  std::optional<std::string> library_status;
};

struct GameInfo {
  std::string id;
  std::optional<std::string> uuid;
  std::optional<std::string> launch_app_id;
  std::string title;
  int selected_variant_index = 0;
  std::vector<GameVariant> variants;
};

struct SessionCreateRequest {
  std::string app_id;
  std::string access_token;
  std::string client_token;
  std::string streaming_base_url;
  std::string region;
  std::string resolution;
  int fps = 60;
  int max_bitrate_kbps = 15000;
  VideoCodec codec = VideoCodec::H264;
  ColorQuality color_quality = ColorQuality::TenBit420;
};

struct SignalingEvent {
  std::string type;
  std::string message;
};

using SignalingListener = std::function<void(const SignalingEvent&)>;

class AuthService {
 public:
  virtual ~AuthService() = default;
  virtual std::vector<LoginProvider> get_providers() = 0;
  virtual AuthSessionResult get_session(bool force_refresh) = 0;
  virtual AuthSession login(const AuthLoginRequest& request) = 0;
  virtual void logout() = 0;
};

class CatalogService {
 public:
  virtual ~CatalogService() = default;
  virtual std::vector<GameInfo> fetch_library_games(const CatalogBrowseRequest& request) = 0;
  virtual std::vector<GameInfo> fetch_public_games(const CatalogBrowseRequest& request) = 0;
  virtual std::vector<GameInfo> browse_catalog(const CatalogBrowseRequest& request) = 0;
};

class SubscriptionService {
 public:
  virtual ~SubscriptionService() = default;
  virtual SubscriptionInfo fetch_subscription(const SubscriptionFetchRequest& request) = 0;
};

class SessionService {
 public:
  virtual ~SessionService() = default;
  virtual SessionInfo create_session(const SessionCreateRequest& request) = 0;
  virtual SessionInfo poll_session(const std::string& session_id) = 0;
  virtual void stop_session(const std::string& session_id) = 0;
};

class SignalingClient {
 public:
  virtual ~SignalingClient() = default;
  virtual void set_listener(SignalingListener listener) = 0;
  virtual void connect() = 0;
  virtual void disconnect() = 0;
  virtual void send_answer(const std::string& sdp) = 0;
  virtual void send_ice_candidate(const std::string& candidate) = 0;
  virtual void request_keyframe() = 0;
};

}  // namespace opennow
