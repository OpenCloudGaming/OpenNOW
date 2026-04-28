#include "opennow/service_clients.hpp"

#include <stdexcept>
#include <utility>

#include "opennow/service_builders.hpp"

namespace opennow {

namespace {

void require_success(const HttpResponse& response, const std::string& label) {
  if (response.status < 200 || response.status >= 300) {
    throw std::runtime_error(label + " failed with status " + std::to_string(response.status));
  }
}

}  // namespace

TransportAuthService::TransportAuthService(
    std::shared_ptr<ServiceTransport> transport,
    std::string service_base_url)
    : transport_(std::move(transport)), service_base_url_(std::move(service_base_url)) {}

std::vector<LoginProvider> TransportAuthService::get_providers() {
  const auto response = transport_->send(build_providers_request(service_base_url_));
  require_success(response, "Provider request");
  return parse_login_providers_response(response.body);
}

AuthSessionResult TransportAuthService::get_session(bool force_refresh) {
  AuthRefreshStatus refresh;
  refresh.forced = force_refresh;
  refresh.attempted = force_refresh && session_.has_value();
  refresh.outcome = refresh.attempted ? "refreshed" : "not_attempted";
  refresh.message = refresh.attempted ? "Using fixture-backed refreshed session" : "Using cached session";
  return {session_, refresh};
}

AuthSession TransportAuthService::login(const AuthLoginRequest& request) {
  const auto response = transport_->send(build_auth_session_request(service_base_url_, request));
  require_success(response, "Auth session request");
  session_ = parse_auth_session_response(response.body);
  return *session_;
}

void TransportAuthService::logout() {
  session_.reset();
}

TransportCatalogService::TransportCatalogService(
    std::shared_ptr<ServiceTransport> transport,
    std::string service_base_url)
    : transport_(std::move(transport)), service_base_url_(std::move(service_base_url)) {}

std::vector<GameInfo> TransportCatalogService::fetch_library_games(
    const CatalogBrowseRequest& request) {
  return fetch(CatalogRequestKind::Library, request);
}

std::vector<GameInfo> TransportCatalogService::fetch_public_games(
    const CatalogBrowseRequest& request) {
  return fetch(CatalogRequestKind::Public, request);
}

std::vector<GameInfo> TransportCatalogService::browse_catalog(
    const CatalogBrowseRequest& request) {
  return fetch(CatalogRequestKind::Browse, request);
}

std::vector<GameInfo> TransportCatalogService::fetch(
    CatalogRequestKind kind,
    const CatalogBrowseRequest& request) {
  const auto response = transport_->send(build_catalog_request(service_base_url_, kind, request));
  require_success(response, "Catalog request");
  return parse_catalog_games_response(response.body);
}

TransportSubscriptionService::TransportSubscriptionService(
    std::shared_ptr<ServiceTransport> transport)
    : transport_(std::move(transport)) {}

SubscriptionInfo TransportSubscriptionService::fetch_subscription(
    const SubscriptionFetchRequest& request) {
  const auto response = transport_->send(build_subscription_request(request));
  require_success(response, "Subscription request");
  return parse_subscription_response(response.body, request.vpc_id.value_or("NP-AMS-08"));
}

TransportSessionService::TransportSessionService(std::shared_ptr<ServiceTransport> transport)
    : transport_(std::move(transport)) {}

SessionInfo TransportSessionService::create_session(const SessionCreateRequest& request) {
  last_streaming_base_url_ = request.streaming_base_url;
  const auto response = transport_->send(build_session_create_request(request));
  require_success(response, "Session create request");
  return parse_session_response(response.body);
}

SessionInfo TransportSessionService::poll_session(const std::string& session_id) {
  if (last_streaming_base_url_.empty()) {
    throw std::logic_error("Cannot poll a session before create_session establishes a base URL");
  }
  const auto response = transport_->send(build_session_poll_request(last_streaming_base_url_, session_id));
  require_success(response, "Session poll request");
  return parse_session_response(response.body);
}

void TransportSessionService::stop_session(const std::string& session_id) {
  if (last_streaming_base_url_.empty()) {
    throw std::logic_error("Cannot stop a session before create_session establishes a base URL");
  }
  const auto response = transport_->send(build_session_stop_request(last_streaming_base_url_, session_id));
  require_success(response, "Session stop request");
}

TransportSignalingClient::TransportSignalingClient(
    std::shared_ptr<SignalingTransport> transport,
    std::string signaling_server,
    std::string session_id,
    std::string peer_name,
    std::optional<std::string> signaling_url)
    : transport_(std::move(transport)),
      signaling_server_(std::move(signaling_server)),
      session_id_(std::move(session_id)),
      peer_name_(std::move(peer_name)),
      signaling_url_(std::move(signaling_url)) {
  transport_->set_text_handler([this](const std::string& text) {
    handle_text(text);
  });
}

void TransportSignalingClient::set_listener(SignalingListener listener) {
  listener_ = std::move(listener);
}

void TransportSignalingClient::connect() {
  const auto url = build_signaling_sign_in_url(signaling_server_, peer_name_, signaling_url_);
  transport_->connect({url, "x-nv-sessionid." + session_id_, {{"Origin", "https://play.geforcenow.com"}}});
  transport_->send_text(build_peer_info_message(next_ack_id(), peer_id_, peer_name_));
  if (listener_) {
    listener_({"connected", ""});
  }
}

void TransportSignalingClient::disconnect() {
  transport_->disconnect();
  if (listener_) {
    listener_({"disconnected", "socket closed"});
  }
}

void TransportSignalingClient::send_answer(const std::string& sdp) {
  transport_->send_text(build_answer_message(next_ack_id(), peer_id_, sdp, std::nullopt));
}

void TransportSignalingClient::send_ice_candidate(const std::string& candidate) {
  transport_->send_text(build_ice_candidate_message(next_ack_id(), peer_id_, candidate, std::nullopt, std::nullopt));
}

void TransportSignalingClient::request_keyframe() {
  transport_->send_text(build_keyframe_request_message(next_ack_id(), peer_id_, "decoder", 0, 1));
}

int TransportSignalingClient::next_ack_id() {
  ack_counter_ += 1;
  return ack_counter_;
}

void TransportSignalingClient::handle_text(const std::string& text) {
  const auto parsed = parse_signaling_message(text, peer_id_);
  for (const auto& message : parsed.outbound_messages) {
    transport_->send_text(message);
  }
  if (!listener_) {
    return;
  }
  for (const auto& event : parsed.events) {
    listener_(event);
  }
}

}  // namespace opennow
