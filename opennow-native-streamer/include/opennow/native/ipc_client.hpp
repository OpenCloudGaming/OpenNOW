#pragma once

#include <atomic>
#include <cstdint>
#include <functional>
#include <cstddef>
#include <mutex>
#include <string>
#include <thread>

namespace opennow::native {

class IpcClient {
 public:
  using MessageHandler = std::function<void(const std::string&)>;
  using StatusHandler = std::function<void(const std::string&)>;

  IpcClient();
  ~IpcClient();

  bool Connect(const std::string& host, std::uint16_t port);
  void Disconnect();
  bool SendJson(const std::string& json);
  void SetMessageHandler(MessageHandler handler);
  void SetStatusHandler(StatusHandler handler);
  std::string GetLastStatus() const;

 private:
  void ReadLoop();
  void EmitStatus(const std::string& message);

  std::mutex write_mutex_;
  MessageHandler message_handler_;
  StatusHandler status_handler_;
  std::atomic<bool> running_{false};
  std::thread read_thread_;
  std::intptr_t socket_fd_{-1};
  mutable std::mutex status_mutex_;
  std::string last_status_;
};

}  // namespace opennow::native
