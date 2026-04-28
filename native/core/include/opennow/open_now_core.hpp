#pragma once

#include <memory>
#include <string>

#include "opennow/input_protocol.hpp"
#include "opennow/services.hpp"

namespace opennow {

struct CoreDependencies {
  std::shared_ptr<AuthService> auth_service;
  std::shared_ptr<CatalogService> catalog_service;
  std::shared_ptr<SubscriptionService> subscription_service;
  std::shared_ptr<SessionService> session_service;
  std::shared_ptr<SignalingClient> signaling_client;
};

class OpenNowCore {
 public:
  explicit OpenNowCore(CoreDependencies dependencies);

  AuthService& auth();
  CatalogService& catalog();
  SubscriptionService& subscription();
  SessionService& sessions();
  SignalingClient& signaling();
  InputEncoder& input_encoder();

  bool has_auth_service() const;
  bool has_catalog_service() const;
  bool has_subscription_service() const;
  bool has_session_service() const;
  bool has_signaling_client() const;

 private:
  CoreDependencies dependencies_;
  InputEncoder input_encoder_;
};

std::string native_core_version();

}  // namespace opennow
