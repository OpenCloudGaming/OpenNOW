#pragma once

#include <cstdint>
#include <functional>
#include <optional>
#include <string>
#include <vector>

namespace opennow::native {

struct DepacketizedAv1Frame {
  std::vector<std::uint8_t> bitstream;
  std::uint64_t timestamp_us = 0;
};

class Av1RtpDepacketizer {
 public:
  using LogFn = std::function<void(const std::string&)>;

  void SetLogger(LogFn logger);
  void Reset();
  std::optional<DepacketizedAv1Frame> PushRtpPacket(const std::vector<std::uint8_t>& packet, std::uint64_t arrival_timestamp_us);

 private:
  struct RtpPacketView {
    const std::uint8_t* payload = nullptr;
    std::size_t payload_size = 0;
    std::uint16_t sequence_number = 0;
    std::uint32_t timestamp = 0;
    bool marker = false;
    bool first_obu_is_continuation = false;
  };

  void Log(const std::string& message) const;
  static std::optional<RtpPacketView> ParseRtpPacket(const std::vector<std::uint8_t>& packet);
  static std::optional<std::vector<std::uint8_t>> AssembleFrame(const std::vector<std::vector<std::uint8_t>>& rtp_payloads);

  LogFn logger_;
  bool has_pending_frame_ = false;
  std::uint16_t expected_sequence_number_ = 0;
  std::vector<std::vector<std::uint8_t>> pending_rtp_payloads_;
};

}  // namespace opennow::native
