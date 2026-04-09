#include "opennow/native/av1_rtp_depacketizer.hpp"

#include <array>
#include <cstring>
#include <sstream>

namespace opennow::native {

namespace {

constexpr std::uint8_t kAv1ObuHasSizeBit = 0b0000'0010;

bool ObuHasExtension(std::uint8_t obu_header) {
  return (obu_header & 0b0000'0100u) != 0;
}

bool ObuHasSize(std::uint8_t obu_header) {
  return (obu_header & kAv1ObuHasSizeBit) != 0;
}

bool RtpStartsWithFragment(std::uint8_t aggregation_header) {
  return (aggregation_header & 0b1000'0000u) != 0;
}

bool RtpEndsWithFragment(std::uint8_t aggregation_header) {
  return (aggregation_header & 0b0100'0000u) != 0;
}

int RtpNumObus(std::uint8_t aggregation_header) {
  return static_cast<int>((aggregation_header & 0b0011'0000u) >> 4);
}

std::optional<std::pair<std::uint64_t, std::size_t>> ReadLeb128(const std::uint8_t* data, std::size_t size) {
  std::uint64_t value = 0;
  std::size_t shift = 0;
  for (std::size_t i = 0; i < size && i < 8; ++i) {
    const std::uint8_t byte = data[i];
    value |= static_cast<std::uint64_t>(byte & 0x7Fu) << shift;
    if ((byte & 0x80u) == 0) {
      return std::make_pair(value, i + 1);
    }
    shift += 7;
  }
  return std::nullopt;
}

std::size_t WriteLeb128(std::uint32_t value, std::uint8_t* buffer) {
  std::size_t size = 0;
  while (value >= 0x80) {
    buffer[size++] = static_cast<std::uint8_t>(0x80 | (value & 0x7F));
    value >>= 7;
  }
  buffer[size++] = static_cast<std::uint8_t>(value);
  return size;
}

struct ObuFragment {
  std::vector<std::uint8_t> data;
};

bool CalculateObuPayload(const ObuFragment& fragment, std::vector<std::uint8_t>& output, std::string* error) {
  if (fragment.data.empty()) {
    if (error) {
      *error = "Empty AV1 OBU fragment";
    }
    return false;
  }

  std::size_t offset = 0;
  const std::uint8_t obu_header = fragment.data[offset++];
  output.push_back(static_cast<std::uint8_t>(obu_header | kAv1ObuHasSizeBit));

  if (ObuHasExtension(obu_header)) {
    if (offset >= fragment.data.size()) {
      if (error) {
        *error = "Missing AV1 OBU extension byte";
      }
      return false;
    }
    output.push_back(fragment.data[offset++]);
  }

  std::size_t payload_offset = offset;
  if (ObuHasSize(obu_header)) {
    const auto obu_size = ReadLeb128(fragment.data.data() + offset, fragment.data.size() - offset);
    if (!obu_size) {
      if (error) {
        *error = "Invalid AV1 OBU size leb128";
      }
      return false;
    }
    offset += obu_size->second;
    const std::size_t actual_payload_size = fragment.data.size() - offset;
    if (obu_size->first != actual_payload_size) {
      if (error) {
        std::ostringstream message;
        message << "Mismatched AV1 OBU size; signaled=" << obu_size->first << " actual=" << actual_payload_size;
        *error = message.str();
      }
      return false;
    }
    payload_offset = offset;
  }

  const std::size_t payload_size = fragment.data.size() - payload_offset;
  std::array<std::uint8_t, 8> leb128{};
  const auto leb128_size = WriteLeb128(static_cast<std::uint32_t>(payload_size), leb128.data());
  output.insert(output.end(), leb128.begin(), leb128.begin() + static_cast<std::ptrdiff_t>(leb128_size));
  output.insert(output.end(), fragment.data.begin() + static_cast<std::ptrdiff_t>(payload_offset), fragment.data.end());
  return true;
}

}  // namespace

void Av1RtpDepacketizer::SetLogger(LogFn logger) {
  logger_ = std::move(logger);
}

void Av1RtpDepacketizer::Reset() {
  has_pending_frame_ = false;
  expected_sequence_number_ = 0;
  pending_rtp_payloads_.clear();
}

std::optional<DepacketizedAv1Frame> Av1RtpDepacketizer::PushRtpPacket(const std::vector<std::uint8_t>& packet, std::uint64_t arrival_timestamp_us) {
  const auto parsed = ParseRtpPacket(packet);
  if (!parsed) {
    Log("Dropping malformed AV1 RTP packet");
    Reset();
    return std::nullopt;
  }

  if (has_pending_frame_ && parsed->sequence_number != expected_sequence_number_) {
    std::ostringstream message;
    message << "AV1 RTP sequence discontinuity; expected=" << expected_sequence_number_ << " got=" << parsed->sequence_number
            << ". Resetting depacketizer state.";
    Log(message.str());
    Reset();
  }

  if (!has_pending_frame_ && parsed->first_obu_is_continuation) {
    Log("Dropping AV1 RTP fragment that starts in the middle of a frame without prior state");
    return std::nullopt;
  }

  has_pending_frame_ = true;
  expected_sequence_number_ = static_cast<std::uint16_t>(parsed->sequence_number + 1);
  pending_rtp_payloads_.emplace_back(parsed->payload, parsed->payload + parsed->payload_size);

  if (!parsed->marker) {
    return std::nullopt;
  }

  const std::uint32_t rtp_timestamp = parsed->timestamp;
  auto bitstream = AssembleFrame(pending_rtp_payloads_);
  pending_rtp_payloads_.clear();
  has_pending_frame_ = false;
  if (!bitstream) {
    Log("Failed to assemble AV1 frame from RTP payloads; resetting depacketizer state");
    return std::nullopt;
  }

  DepacketizedAv1Frame frame;
  frame.bitstream = std::move(*bitstream);
  frame.timestamp_us = rtp_timestamp != 0 ? static_cast<std::uint64_t>(rtp_timestamp) : arrival_timestamp_us;
  return frame;
}

void Av1RtpDepacketizer::Log(const std::string& message) const {
  if (logger_) {
    logger_(message);
  }
}

std::optional<Av1RtpDepacketizer::RtpPacketView> Av1RtpDepacketizer::ParseRtpPacket(const std::vector<std::uint8_t>& packet) {
  if (packet.size() < 13) {
    return std::nullopt;
  }

  const std::uint8_t version = static_cast<std::uint8_t>((packet[0] >> 6) & 0x03u);
  if (version != 2) {
    return std::nullopt;
  }
  const bool padding = (packet[0] & 0x20u) != 0;
  const bool extension = (packet[0] & 0x10u) != 0;
  const std::size_t csrc_count = packet[0] & 0x0Fu;
  std::size_t offset = 12 + csrc_count * 4;
  if (packet.size() < offset + 1) {
    return std::nullopt;
  }
  if (extension) {
    if (packet.size() < offset + 4) {
      return std::nullopt;
    }
    const std::uint16_t extension_words = static_cast<std::uint16_t>((packet[offset + 2] << 8) | packet[offset + 3]);
    offset += 4 + static_cast<std::size_t>(extension_words) * 4;
    if (packet.size() < offset + 1) {
      return std::nullopt;
    }
  }

  std::size_t payload_size = packet.size() - offset;
  if (padding) {
    const std::uint8_t padding_size = packet.back();
    if (padding_size == 0 || padding_size > payload_size) {
      return std::nullopt;
    }
    payload_size -= padding_size;
  }
  if (payload_size < 1) {
    return std::nullopt;
  }

  RtpPacketView view;
  view.payload = packet.data() + static_cast<std::ptrdiff_t>(offset);
  view.payload_size = payload_size;
  view.sequence_number = static_cast<std::uint16_t>((packet[2] << 8) | packet[3]);
  view.timestamp = (static_cast<std::uint32_t>(packet[4]) << 24) | (static_cast<std::uint32_t>(packet[5]) << 16) |
                   (static_cast<std::uint32_t>(packet[6]) << 8) | static_cast<std::uint32_t>(packet[7]);
  view.marker = (packet[1] & 0x80u) != 0;
  view.first_obu_is_continuation = RtpStartsWithFragment(view.payload[0]);
  return view;
}

std::optional<std::vector<std::uint8_t>> Av1RtpDepacketizer::AssembleFrame(const std::vector<std::vector<std::uint8_t>>& rtp_payloads) {
  std::vector<ObuFragment> obus;
  bool expect_continuation = false;

  for (const auto& payload : rtp_payloads) {
    if (payload.empty()) {
      return std::nullopt;
    }
    std::size_t offset = 0;
    const std::uint8_t aggregation_header = payload[offset++];
    const bool continues_obu = RtpStartsWithFragment(aggregation_header);
    if (continues_obu != expect_continuation) {
      return std::nullopt;
    }
    const int num_obus = RtpNumObus(aggregation_header);
    if (offset == payload.size()) {
      if (num_obus != 1) {
        return std::nullopt;
      }
      if (!continues_obu) {
        obus.emplace_back();
      }
      expect_continuation = RtpEndsWithFragment(aggregation_header);
      continue;
    }

    for (int obu_index = 1; offset < payload.size(); ++obu_index) {
      ObuFragment* target = nullptr;
      if (obu_index == 1 && continues_obu) {
        if (obus.empty()) {
          return std::nullopt;
        }
        target = &obus.back();
      } else {
        obus.emplace_back();
        target = &obus.back();
      }

      std::uint64_t fragment_size = 0;
      const bool has_fragment_size = (num_obus == 0) || (obu_index != num_obus);
      if (has_fragment_size) {
        const auto parsed_size = ReadLeb128(payload.data() + static_cast<std::ptrdiff_t>(offset), payload.size() - offset);
        if (!parsed_size) {
          return std::nullopt;
        }
        fragment_size = parsed_size->first;
        offset += parsed_size->second;
        if (fragment_size > payload.size() - offset) {
          return std::nullopt;
        }
      } else {
        fragment_size = payload.size() - offset;
      }

      if (fragment_size != 0) {
        target->data.insert(
            target->data.end(),
            payload.begin() + static_cast<std::ptrdiff_t>(offset),
            payload.begin() + static_cast<std::ptrdiff_t>(offset + fragment_size));
        offset += static_cast<std::size_t>(fragment_size);
      }
    }

    expect_continuation = RtpEndsWithFragment(aggregation_header);
  }

  if (expect_continuation || obus.empty()) {
    return std::nullopt;
  }

  std::vector<std::uint8_t> bitstream;
  for (const auto& obu : obus) {
    std::string error;
    if (!CalculateObuPayload(obu, bitstream, &error)) {
      return std::nullopt;
    }
  }
  return bitstream;
}

}  // namespace opennow::native
