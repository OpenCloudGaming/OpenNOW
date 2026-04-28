#pragma once

#include <optional>
#include <string>
#include <vector>

#include "opennow/services.hpp"
#include "opennow/service_transport.hpp"

namespace opennow {

enum class CatalogRequestKind {
  Library,
  Public,
  Browse,
};

HttpRequest build_providers_request(const std::string& service_base_url);
HttpRequest build_auth_session_request(
    const std::string& service_base_url,
    const AuthLoginRequest& request);
HttpRequest build_catalog_request(
    const std::string& service_base_url,
    CatalogRequestKind kind,
    const CatalogBrowseRequest& request);
HttpRequest build_subscription_request(const SubscriptionFetchRequest& request);
HttpRequest build_session_create_request(const SessionCreateRequest& request);
HttpRequest build_session_poll_request(
    const std::string& streaming_base_url,
    const std::string& session_id);
HttpRequest build_session_stop_request(
    const std::string& streaming_base_url,
    const std::string& session_id);

std::vector<LoginProvider> parse_login_providers_response(const std::string& body);
AuthSession parse_auth_session_response(const std::string& body);
std::vector<GameInfo> parse_catalog_games_response(const std::string& body);
SubscriptionInfo parse_subscription_response(
    const std::string& body,
    const std::optional<std::string>& server_region_id);
SessionInfo parse_session_response(const std::string& body);

struct ParsedSignalingMessage {
  std::vector<std::string> outbound_messages;
  std::vector<SignalingEvent> events;
};

ParsedSignalingMessage parse_signaling_message(
    const std::string& text,
    int peer_id);

}  // namespace opennow
