#include "opennow/open_now_core.hpp"

#include <stdexcept>

namespace opennow {

OpenNowCore::OpenNowCore(CoreDependencies dependencies)
    : dependencies_(std::move(dependencies)) {}

AuthService& OpenNowCore::auth() {
  if (!dependencies_.auth_service) {
    throw std::logic_error("AuthService dependency is not configured");
  }
  return *dependencies_.auth_service;
}

CatalogService& OpenNowCore::catalog() {
  if (!dependencies_.catalog_service) {
    throw std::logic_error("CatalogService dependency is not configured");
  }
  return *dependencies_.catalog_service;
}

SubscriptionService& OpenNowCore::subscription() {
  if (!dependencies_.subscription_service) {
    throw std::logic_error("SubscriptionService dependency is not configured");
  }
  return *dependencies_.subscription_service;
}

SessionService& OpenNowCore::sessions() {
  if (!dependencies_.session_service) {
    throw std::logic_error("SessionService dependency is not configured");
  }
  return *dependencies_.session_service;
}

SignalingClient& OpenNowCore::signaling() {
  if (!dependencies_.signaling_client) {
    throw std::logic_error("SignalingClient dependency is not configured");
  }
  return *dependencies_.signaling_client;
}

InputEncoder& OpenNowCore::input_encoder() {
  return input_encoder_;
}

bool OpenNowCore::has_auth_service() const {
  return static_cast<bool>(dependencies_.auth_service);
}

bool OpenNowCore::has_catalog_service() const {
  return static_cast<bool>(dependencies_.catalog_service);
}

bool OpenNowCore::has_subscription_service() const {
  return static_cast<bool>(dependencies_.subscription_service);
}

bool OpenNowCore::has_session_service() const {
  return static_cast<bool>(dependencies_.session_service);
}

bool OpenNowCore::has_signaling_client() const {
  return static_cast<bool>(dependencies_.signaling_client);
}

std::string native_core_version() {
  return "0.1.0";
}

}  // namespace opennow
