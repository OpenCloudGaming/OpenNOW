#include "opennow/input_protocol.hpp"

#include <algorithm>
#include <cmath>

namespace opennow {

namespace {

void write_u16_be(std::vector<std::uint8_t>& bytes, std::size_t offset, std::uint16_t value) {
  bytes[offset] = static_cast<std::uint8_t>((value >> 8) & 0xFF);
  bytes[offset + 1] = static_cast<std::uint8_t>(value & 0xFF);
}

void write_i16_be(std::vector<std::uint8_t>& bytes, std::size_t offset, std::int16_t value) {
  write_u16_be(bytes, offset, static_cast<std::uint16_t>(value));
}

void write_u32_le(std::vector<std::uint8_t>& bytes, std::size_t offset, std::uint32_t value) {
  bytes[offset] = static_cast<std::uint8_t>(value & 0xFF);
  bytes[offset + 1] = static_cast<std::uint8_t>((value >> 8) & 0xFF);
  bytes[offset + 2] = static_cast<std::uint8_t>((value >> 16) & 0xFF);
  bytes[offset + 3] = static_cast<std::uint8_t>((value >> 24) & 0xFF);
}

void write_u16_le(std::vector<std::uint8_t>& bytes, std::size_t offset, std::uint16_t value) {
  bytes[offset] = static_cast<std::uint8_t>(value & 0xFF);
  bytes[offset + 1] = static_cast<std::uint8_t>((value >> 8) & 0xFF);
}

void write_i16_le(std::vector<std::uint8_t>& bytes, std::size_t offset, std::int16_t value) {
  write_u16_le(bytes, offset, static_cast<std::uint16_t>(value));
}

void write_u32_be(std::vector<std::uint8_t>& bytes, std::size_t offset, std::uint32_t value) {
  bytes[offset] = static_cast<std::uint8_t>((value >> 24) & 0xFF);
  bytes[offset + 1] = static_cast<std::uint8_t>((value >> 16) & 0xFF);
  bytes[offset + 2] = static_cast<std::uint8_t>((value >> 8) & 0xFF);
  bytes[offset + 3] = static_cast<std::uint8_t>(value & 0xFF);
}

void write_u64_be(std::vector<std::uint8_t>& bytes, std::size_t offset, std::uint64_t value) {
  for (int i = 7; i >= 0; --i) {
    bytes[offset + static_cast<std::size_t>(7 - i)] =
        static_cast<std::uint8_t>((value >> (i * 8)) & 0xFF);
  }
}

void write_u64_le(std::vector<std::uint8_t>& bytes, std::size_t offset, std::uint64_t value) {
  for (int i = 0; i < 8; ++i) {
    bytes[offset + static_cast<std::size_t>(i)] =
        static_cast<std::uint8_t>((value >> (i * 8)) & 0xFF);
  }
}

std::vector<std::uint8_t> wrap_single_event(
    const std::vector<std::uint8_t>& payload,
    int protocol_version) {
  if (protocol_version <= 2) {
    return payload;
  }
  std::vector<std::uint8_t> wrapped(10 + payload.size());
  wrapped[0] = 0x23;
  wrapped[9] = 0x22;
  std::copy(payload.begin(), payload.end(), wrapped.begin() + 10);
  return wrapped;
}

std::vector<std::uint8_t> wrap_sized_event(
    const std::vector<std::uint8_t>& payload,
    int protocol_version) {
  if (protocol_version <= 2) {
    return payload;
  }
  std::vector<std::uint8_t> wrapped(12 + payload.size());
  wrapped[0] = 0x23;
  wrapped[9] = 0x21;
  write_u16_be(wrapped, 10, static_cast<std::uint16_t>(payload.size()));
  std::copy(payload.begin(), payload.end(), wrapped.begin() + 12);
  return wrapped;
}

std::vector<std::uint8_t> wrap_gamepad_partially_reliable(
    const std::vector<std::uint8_t>& payload,
    int protocol_version,
    int gamepad_index,
    std::uint16_t sequence_number) {
  if (protocol_version <= 2) {
    return payload;
  }
  std::vector<std::uint8_t> wrapped(16 + payload.size());
  wrapped[0] = 0x23;
  wrapped[9] = 0x26;
  wrapped[10] = static_cast<std::uint8_t>(gamepad_index & 0xFF);
  write_u16_be(wrapped, 11, sequence_number);
  wrapped[13] = 0x21;
  write_u16_be(wrapped, 14, static_cast<std::uint16_t>(payload.size()));
  std::copy(payload.begin(), payload.end(), wrapped.begin() + 16);
  return wrapped;
}

}  // namespace

std::uint32_t partially_reliable_hid_mask_for_input_type(int input_type) {
  if (input_type < 0 || input_type > 31) {
    return 0;
  }
  return 1U << input_type;
}

bool is_partially_reliable_hid_transfer_eligible(int input_type) {
  return input_type == static_cast<int>(kInputMouseRel);
}

std::int16_t normalize_to_int16(double value) {
  const auto bounded = std::clamp(value, -1.0, 1.0);
  return static_cast<std::int16_t>(
      bounded < 0 ? std::round(bounded * 32768.0)
                  : std::round(bounded * 32767.0));
}

std::uint8_t normalize_to_uint8(double value) {
  const auto bounded = std::clamp(value, 0.0, 1.0);
  return static_cast<std::uint8_t>(std::round(bounded * 255.0));
}

QuantizedMouseDelta quantize_mouse_delta_with_residual(double accumulated_delta) {
  const auto send = static_cast<int>(std::round(accumulated_delta));
  return {send, accumulated_delta - send};
}

void InputEncoder::set_protocol_version(int version) {
  protocol_version_ = version;
}

std::uint16_t InputEncoder::get_next_gamepad_sequence(int gamepad_index) {
  const auto it = gamepad_sequence_.find(gamepad_index);
  const std::uint16_t current = it == gamepad_sequence_.end() ? 1 : it->second;
  gamepad_sequence_[gamepad_index] = static_cast<std::uint16_t>(current + 1);
  return current;
}

void InputEncoder::reset_gamepad_sequences() {
  gamepad_sequence_.clear();
}

std::vector<std::uint8_t> InputEncoder::encode_heartbeat() const {
  std::vector<std::uint8_t> bytes(4);
  write_u32_le(bytes, 0, kInputHeartbeat);
  return bytes;
}

std::vector<std::uint8_t> InputEncoder::encode_key_down(const KeyboardPayload& payload) const {
  return encode_key(kInputKeyDown, payload);
}

std::vector<std::uint8_t> InputEncoder::encode_key_up(const KeyboardPayload& payload) const {
  return encode_key(kInputKeyUp, payload);
}

std::vector<std::uint8_t> InputEncoder::encode_mouse_move(const MouseMovePayload& payload) const {
  std::vector<std::uint8_t> bytes(22);
  write_u32_le(bytes, 0, kInputMouseRel);
  write_i16_be(bytes, 4, payload.dx);
  write_i16_be(bytes, 6, payload.dy);
  write_u16_be(bytes, 8, 0);
  write_u32_be(bytes, 10, 0);
  write_u64_be(bytes, 14, payload.timestamp_us);
  return wrap_sized_event(bytes, protocol_version_);
}

std::vector<std::uint8_t> InputEncoder::encode_mouse_button_down(
    const MouseButtonPayload& payload) const {
  return encode_mouse_button(kInputMouseButtonDown, payload);
}

std::vector<std::uint8_t> InputEncoder::encode_mouse_button_up(
    const MouseButtonPayload& payload) const {
  return encode_mouse_button(kInputMouseButtonUp, payload);
}

std::vector<std::uint8_t> InputEncoder::encode_mouse_wheel(const MouseWheelPayload& payload) const {
  std::vector<std::uint8_t> bytes(22);
  write_u32_le(bytes, 0, kInputMouseWheel);
  write_i16_be(bytes, 4, 0);
  write_i16_be(bytes, 6, payload.delta);
  write_u16_be(bytes, 8, 0);
  write_u32_be(bytes, 10, 0);
  write_u64_be(bytes, 14, payload.timestamp_us);
  return wrap_single_event(bytes, protocol_version_);
}

std::vector<std::uint8_t> InputEncoder::encode_gamepad_state(
    const GamepadInput& payload,
    std::uint16_t bitmap,
    bool use_partially_reliable) {
  std::vector<std::uint8_t> bytes(kGamepadPacketSize);
  write_u32_le(bytes, 0, kInputGamepad);
  write_u16_le(bytes, 4, 26);
  write_u16_le(bytes, 6, static_cast<std::uint16_t>(payload.controller_id & 0x03));
  write_u16_le(bytes, 8, bitmap);
  write_u16_le(bytes, 10, 20);
  write_u16_le(bytes, 12, payload.buttons);
  const std::uint16_t packed_triggers =
      static_cast<std::uint16_t>(payload.left_trigger) |
      static_cast<std::uint16_t>(payload.right_trigger << 8);
  write_u16_le(bytes, 14, packed_triggers);
  write_i16_le(bytes, 16, payload.left_stick_x);
  write_i16_le(bytes, 18, payload.left_stick_y);
  write_i16_le(bytes, 20, payload.right_stick_x);
  write_i16_le(bytes, 22, payload.right_stick_y);
  write_u16_le(bytes, 24, 0);
  write_u16_le(bytes, 26, 85);
  write_u16_le(bytes, 28, 0);
  write_u64_le(bytes, 30, payload.timestamp_us);

  if (use_partially_reliable) {
    const auto sequence = get_next_gamepad_sequence(payload.controller_id);
    return wrap_gamepad_partially_reliable(
        bytes,
        protocol_version_,
        payload.controller_id,
        sequence);
  }
  return wrap_sized_event(bytes, protocol_version_);
}

std::vector<std::uint8_t> InputEncoder::encode_key(
    std::uint32_t type,
    const KeyboardPayload& payload) const {
  std::vector<std::uint8_t> bytes(18);
  write_u32_le(bytes, 0, type);
  write_u16_be(bytes, 4, payload.keycode);
  write_u16_be(bytes, 6, payload.modifiers);
  write_u16_be(bytes, 8, payload.scancode);
  write_u64_be(bytes, 10, payload.timestamp_us);
  return wrap_single_event(bytes, protocol_version_);
}

std::vector<std::uint8_t> InputEncoder::encode_mouse_button(
    std::uint32_t type,
    const MouseButtonPayload& payload) const {
  std::vector<std::uint8_t> bytes(18);
  write_u32_le(bytes, 0, type);
  bytes[4] = payload.button;
  bytes[5] = 0;
  write_u32_be(bytes, 6, 0);
  write_u64_be(bytes, 10, payload.timestamp_us);
  return wrap_single_event(bytes, protocol_version_);
}

}  // namespace opennow
