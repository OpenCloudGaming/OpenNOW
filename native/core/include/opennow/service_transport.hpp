#pragma once

#include <functional>
#include <map>
#include <optional>
#include <string>
#include <vector>

namespace opennow {

enum class HttpMethod {
  Get,
  Post,
  Delete,
};

struct HttpRequest {
  HttpMethod method = HttpMethod::Get;
  std::string url;
  std::map<std::string, std::string> headers;
  std::string body;
};

struct HttpResponse {
  int status = 0;
  std::map<std::string, std::string> headers;
  std::string body;
};

class ServiceTransport {
 public:
  virtual ~ServiceTransport() = default;
  virtual HttpResponse send(const HttpRequest& request) = 0;
};

class FixtureServiceTransport final : public ServiceTransport {
 public:
  void add_response(HttpMethod method, std::string url, HttpResponse response);
  HttpResponse send(const HttpRequest& request) override;

  const std::vector<HttpRequest>& requests() const;

 private:
  struct Route {
    HttpMethod method;
    std::string url;
    HttpResponse response;
  };

  std::vector<Route> routes_;
  std::vector<HttpRequest> requests_;
};

struct SignalingConnectRequest {
  std::string url;
  std::string protocol;
  std::map<std::string, std::string> headers;
};

using SignalingTextHandler = std::function<void(const std::string&)>;

class SignalingTransport {
 public:
  virtual ~SignalingTransport() = default;
  virtual void set_text_handler(SignalingTextHandler handler) = 0;
  virtual void connect(const SignalingConnectRequest& request) = 0;
  virtual void send_text(const std::string& text) = 0;
  virtual void disconnect() = 0;
};

class FixtureSignalingTransport final : public SignalingTransport {
 public:
  void set_text_handler(SignalingTextHandler handler) override;
  void connect(const SignalingConnectRequest& request) override;
  void send_text(const std::string& text) override;
  void disconnect() override;

  void receive_text(const std::string& text);

  bool connected() const;
  const std::optional<SignalingConnectRequest>& connect_request() const;
  const std::vector<std::string>& sent_messages() const;

 private:
  bool connected_ = false;
  std::optional<SignalingConnectRequest> connect_request_;
  std::vector<std::string> sent_messages_;
  SignalingTextHandler handler_;
};

}  // namespace opennow
