#include "opennow/native/ipc_client.hpp"

#include <algorithm>
#include <climits>
#include <chrono>
#include <iostream>
#include <thread>
#if defined(_WIN32)
#include <winsock2.h>
#include <ws2tcpip.h>
#else
#include <arpa/inet.h>
#include <cerrno>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#endif

#include <array>
#include <cstring>
#include <vector>

namespace opennow::native {

namespace {
constexpr std::size_t kReadChunkSize = 4096;
constexpr int kInitialConnectRetryCount = 10;
constexpr int kInitialConnectBackoffMs = 100;

#if defined(_WIN32)
using SocketHandle = SOCKET;
constexpr SocketHandle kInvalidSocket = INVALID_SOCKET;
#else
using SocketHandle = int;
constexpr SocketHandle kInvalidSocket = -1;
#endif

bool SendAll(SocketHandle socket_fd, const std::uint8_t* data, std::size_t size) {
  std::size_t sent_total = 0;
  while (sent_total < size) {
#if defined(_WIN32)
    const auto remaining = static_cast<int>(std::min<std::size_t>(size - sent_total, static_cast<std::size_t>(INT_MAX)));
    const auto sent = ::send(
        socket_fd,
        reinterpret_cast<const char*>(data + sent_total),
        remaining,
        0);
#else
    const auto sent = ::send(
        socket_fd,
        reinterpret_cast<const void*>(data + sent_total),
        size - sent_total,
        0);
#endif
    if (sent <= 0) {
      return false;
    }
    sent_total += static_cast<std::size_t>(sent);
  }
  return true;
}

void CloseSocket(SocketHandle socket_fd) {
#if defined(_WIN32)
  ::shutdown(socket_fd, SD_BOTH);
  ::closesocket(socket_fd);
#else
  ::shutdown(socket_fd, SHUT_RDWR);
  ::close(socket_fd);
#endif
}

int ReceiveChunk(SocketHandle socket_fd, std::uint8_t* buffer, std::size_t size) {
#if defined(_WIN32)
  return ::recv(socket_fd, reinterpret_cast<char*>(buffer), static_cast<int>(size), 0);
#else
  return static_cast<int>(::recv(socket_fd, reinterpret_cast<void*>(buffer), size, 0));
#endif
}

std::string LastSocketErrorString() {
#if defined(_WIN32)
  const int code = WSAGetLastError();
  return "WSA error " + std::to_string(code);
#else
  return std::string(std::strerror(errno));
#endif
}

}  // namespace

IpcClient::IpcClient() = default;

IpcClient::~IpcClient() {
  Disconnect();
}

bool IpcClient::Connect(const std::string& host, std::uint16_t port) {
  Disconnect();

#if defined(_WIN32)
  WSADATA wsa_data{};
  if (WSAStartup(MAKEWORD(2, 2), &wsa_data) != 0) {
    EmitStatus("WSAStartup failed");
    return false;
  }
#endif

  sockaddr_in address{};
  address.sin_family = AF_INET;
  address.sin_port = htons(port);
  if (::inet_pton(AF_INET, host.c_str(), &address.sin_addr) != 1) {
    EmitStatus("Failed to parse IPC host address: " + host);
    Disconnect();
    return false;
  }

  std::string last_connect_error;
  for (int attempt = 1; attempt <= kInitialConnectRetryCount; ++attempt) {
    const auto socket_fd = ::socket(AF_INET, SOCK_STREAM, 0);
    if (socket_fd == kInvalidSocket) {
      last_connect_error = "Failed to create IPC socket: " + LastSocketErrorString();
      break;
    }

    if (::connect(socket_fd, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0) {
      socket_fd_ = static_cast<std::intptr_t>(socket_fd);
      running_.store(true);
      read_thread_ = std::thread([this]() { ReadLoop(); });
      if (attempt > 1) {
        EmitStatus(
            "Connected to Electron IPC server after retry " + std::to_string(attempt) + " at " + host + ":" + std::to_string(port));
      }
      return true;
    }

    last_connect_error = "IPC connect attempt " + std::to_string(attempt) + "/" + std::to_string(kInitialConnectRetryCount) + " to " + host +
                         ":" + std::to_string(port) + " failed: " + LastSocketErrorString();
    EmitStatus(last_connect_error);
    CloseSocket(socket_fd);

    if (attempt < kInitialConnectRetryCount) {
      std::this_thread::sleep_for(std::chrono::milliseconds(kInitialConnectBackoffMs * attempt));
    }
  }

  EmitStatus("Failed to connect to Electron IPC server after " + std::to_string(kInitialConnectRetryCount) + " attempts to " + host + ":" +
             std::to_string(port) + (last_connect_error.empty() ? std::string() : " (" + last_connect_error + ")"));
  Disconnect();
  return false;
}

void IpcClient::Disconnect() {
  running_.store(false);
  if (socket_fd_ != static_cast<std::intptr_t>(kInvalidSocket)) {
    CloseSocket(static_cast<SocketHandle>(socket_fd_));
    socket_fd_ = static_cast<std::intptr_t>(kInvalidSocket);
  }
  if (read_thread_.joinable()) {
    read_thread_.join();
  }
#if defined(_WIN32)
  WSACleanup();
#endif
}

bool IpcClient::SendJson(const std::string& json) {
  std::lock_guard<std::mutex> lock(write_mutex_);
  if (socket_fd_ == static_cast<std::intptr_t>(kInvalidSocket)) {
    return false;
  }

  const auto size = static_cast<std::uint32_t>(json.size());
  std::array<std::uint8_t, 4> header{
      static_cast<std::uint8_t>((size >> 24) & 0xff),
      static_cast<std::uint8_t>((size >> 16) & 0xff),
      static_cast<std::uint8_t>((size >> 8) & 0xff),
      static_cast<std::uint8_t>(size & 0xff),
  };

  if (!SendAll(static_cast<SocketHandle>(socket_fd_), header.data(), header.size())) {
    return false;
  }
  if (!SendAll(
          static_cast<SocketHandle>(socket_fd_),
          reinterpret_cast<const std::uint8_t*>(json.data()),
          json.size())) {
    return false;
  }
  return true;
}

void IpcClient::SetMessageHandler(MessageHandler handler) {
  message_handler_ = std::move(handler);
}

void IpcClient::SetStatusHandler(StatusHandler handler) {
  status_handler_ = std::move(handler);
}

std::string IpcClient::GetLastStatus() const {
  std::lock_guard<std::mutex> lock(status_mutex_);
  return last_status_;
}

void IpcClient::ReadLoop() {
  std::vector<std::uint8_t> buffer;
  buffer.reserve(kReadChunkSize * 2);
  std::array<std::uint8_t, kReadChunkSize> chunk{};

  while (running_.load()) {
    const auto received = ReceiveChunk(static_cast<SocketHandle>(socket_fd_), chunk.data(), chunk.size());
    if (received <= 0) {
      break;
    }
    buffer.insert(buffer.end(), chunk.begin(), chunk.begin() + static_cast<std::size_t>(received));

    while (buffer.size() >= 4) {
      const std::uint32_t size =
          (static_cast<std::uint32_t>(buffer[0]) << 24) |
          (static_cast<std::uint32_t>(buffer[1]) << 16) |
          (static_cast<std::uint32_t>(buffer[2]) << 8) |
          static_cast<std::uint32_t>(buffer[3]);
      if (buffer.size() < size + 4) {
        break;
      }

      const std::string json(buffer.begin() + 4, buffer.begin() + 4 + size);
      if (message_handler_) {
        message_handler_(json);
      }
      buffer.erase(buffer.begin(), buffer.begin() + 4 + size);
    }
  }

  EmitStatus("IPC connection closed");
}

void IpcClient::EmitStatus(const std::string& message) {
  {
    std::lock_guard<std::mutex> lock(status_mutex_);
    last_status_ = message;
  }
  std::cerr << "[OpenNOW Native Streamer][IPC] " << message << std::endl;
  if (status_handler_) {
    status_handler_(message);
  }
}

}  // namespace opennow::native
