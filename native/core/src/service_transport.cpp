#include "opennow/service_transport.hpp"

#include <stdexcept>
#include <utility>

namespace opennow {

void FixtureServiceTransport::add_response(
    HttpMethod method,
    std::string url,
    HttpResponse response) {
  routes_.push_back({method, std::move(url), std::move(response)});
}

HttpResponse FixtureServiceTransport::send(const HttpRequest& request) {
  requests_.push_back(request);
  for (const auto& route : routes_) {
    if (route.method == request.method && route.url == request.url) {
      return route.response;
    }
  }
  throw std::runtime_error("No fixture response configured for service request: " + request.url);
}

const std::vector<HttpRequest>& FixtureServiceTransport::requests() const {
  return requests_;
}

void FixtureSignalingTransport::set_text_handler(SignalingTextHandler handler) {
  handler_ = std::move(handler);
}

void FixtureSignalingTransport::connect(const SignalingConnectRequest& request) {
  connected_ = true;
  connect_request_ = request;
}

void FixtureSignalingTransport::send_text(const std::string& text) {
  sent_messages_.push_back(text);
}

void FixtureSignalingTransport::disconnect() {
  connected_ = false;
}

void FixtureSignalingTransport::receive_text(const std::string& text) {
  if (handler_) {
    handler_(text);
  }
}

bool FixtureSignalingTransport::connected() const {
  return connected_;
}

const std::optional<SignalingConnectRequest>& FixtureSignalingTransport::connect_request() const {
  return connect_request_;
}

const std::vector<std::string>& FixtureSignalingTransport::sent_messages() const {
  return sent_messages_;
}

}  // namespace opennow
