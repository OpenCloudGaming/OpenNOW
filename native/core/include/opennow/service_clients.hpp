#pragma once

#include <memory>
#include <string>

#include "opennow/service_protocol.hpp"
#include "opennow/service_transport.hpp"
#include "opennow/services.hpp"

namespace opennow {

class TransportAuthService final : public AuthService {
 public:
  TransportAuthService(std::shared_ptr<ServiceTransport> transport, std::string service_base_url);

  std::vector<LoginProvider> get_providers() override;
  AuthSessionResult get_session(bool force_refresh) override;
  AuthSession login(const AuthLoginRequest& request) override;
  void logout() override;

 private:
  std::shared_ptr<ServiceTransport> transport_;
  std::string service_base_url_;
  std::optional<AuthSession> session_;
};

class TransportCatalogService final : public CatalogService {
 public:
  TransportCatalogService(std::shared_ptr<ServiceTransport> transport, std::string service_base_url);

  std::vector<GameInfo> fetch_library_games(const CatalogBrowseRequest& request) override;
  std::vector<GameInfo> fetch_public_games(const CatalogBrowseRequest& request) override;
  std::vector<GameInfo> browse_catalog(const CatalogBrowseRequest& request) override;

 private:
  std::vector<GameInfo> fetch(CatalogRequestKind kind, const CatalogBrowseRequest& request);

  std::shared_ptr<ServiceTransport> transport_;
  std::string service_base_url_;
};

class TransportSubscriptionService final : public SubscriptionService {
 public:
  explicit TransportSubscriptionService(std::shared_ptr<ServiceTransport> transport);

  SubscriptionInfo fetch_subscription(const SubscriptionFetchRequest& request) override;

 private:
  std::shared_ptr<ServiceTransport> transport_;
};

class TransportSessionService final : public SessionService {
 public:
  explicit TransportSessionService(std::shared_ptr<ServiceTransport> transport);

  SessionInfo create_session(const SessionCreateRequest& request) override;
  SessionInfo poll_session(const std::string& session_id) override;
  void stop_session(const std::string& session_id) override;

 private:
  std::shared_ptr<ServiceTransport> transport_;
  std::string last_streaming_base_url_;
};

class TransportSignalingClient final : public SignalingClient {
 public:
  TransportSignalingClient(
      std::shared_ptr<SignalingTransport> transport,
      std::string signaling_server,
      std::string session_id,
      std::string peer_name,
      std::optional<std::string> signaling_url = std::nullopt);

  void set_listener(SignalingListener listener) override;
  void connect() override;
  void disconnect() override;
  void send_answer(const std::string& sdp) override;
  void send_ice_candidate(const std::string& candidate) override;
  void request_keyframe() override;

 private:
  int next_ack_id();
  void handle_text(const std::string& text);

  std::shared_ptr<SignalingTransport> transport_;
  std::string signaling_server_;
  std::string session_id_;
  std::string peer_name_;
  std::optional<std::string> signaling_url_;
  SignalingListener listener_;
  int peer_id_ = 2;
  int ack_counter_ = 0;
};

}  // namespace opennow
