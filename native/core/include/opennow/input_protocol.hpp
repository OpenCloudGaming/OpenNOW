#pragma once

#include <cstdint>
#include <unordered_map>
#include <vector>

namespace opennow {

constexpr std::uint32_t kInputHeartbeat = 2;
constexpr std::uint32_t kInputKeyDown = 3;
constexpr std::uint32_t kInputKeyUp = 4;
constexpr std::uint32_t kInputMouseRel = 7;
constexpr std::uint32_t kInputMouseButtonDown = 8;
constexpr std::uint32_t kInputMouseButtonUp = 9;
constexpr std::uint32_t kInputMouseWheel = 10;
constexpr std::uint32_t kInputGamepad = 12;

constexpr std::uint8_t kMouseLeft = 1;
constexpr std::uint8_t kMouseMiddle = 2;
constexpr std::uint8_t kMouseRight = 3;
constexpr std::uint8_t kMouseBack = 4;
constexpr std::uint8_t kMouseForward = 5;

constexpr int kGamepadMaxControllers = 4;
constexpr int kGamepadPacketSize = 38;
constexpr std::uint32_t kPartiallyReliableGamepadMaskAll =
    (1U << kGamepadMaxControllers) - 1U;
constexpr std::uint32_t kPartiallyReliableHidDeviceMaskAll = 0xFFFFFFFFU;

struct KeyboardPayload {
  std::uint16_t keycode = 0;
  std::uint16_t scancode = 0;
  std::uint16_t modifiers = 0;
  std::uint64_t timestamp_us = 0;
};

struct MouseMovePayload {
  std::int16_t dx = 0;
  std::int16_t dy = 0;
  std::uint64_t timestamp_us = 0;
};

struct MouseButtonPayload {
  std::uint8_t button = 0;
  std::uint64_t timestamp_us = 0;
};

struct MouseWheelPayload {
  std::int16_t delta = 0;
  std::uint64_t timestamp_us = 0;
};

struct GamepadInput {
  std::uint16_t controller_id = 0;
  std::uint16_t buttons = 0;
  std::uint8_t left_trigger = 0;
  std::uint8_t right_trigger = 0;
  std::int16_t left_stick_x = 0;
  std::int16_t left_stick_y = 0;
  std::int16_t right_stick_x = 0;
  std::int16_t right_stick_y = 0;
  bool connected = false;
  std::uint64_t timestamp_us = 0;
};

struct QuantizedMouseDelta {
  int send = 0;
  double residual = 0.0;
};

std::uint32_t partially_reliable_hid_mask_for_input_type(int input_type);
bool is_partially_reliable_hid_transfer_eligible(int input_type);
std::int16_t normalize_to_int16(double value);
std::uint8_t normalize_to_uint8(double value);
QuantizedMouseDelta quantize_mouse_delta_with_residual(double accumulated_delta);

class InputEncoder {
 public:
  void set_protocol_version(int version);
  std::uint16_t get_next_gamepad_sequence(int gamepad_index);
  void reset_gamepad_sequences();

  std::vector<std::uint8_t> encode_heartbeat() const;
  std::vector<std::uint8_t> encode_key_down(const KeyboardPayload& payload) const;
  std::vector<std::uint8_t> encode_key_up(const KeyboardPayload& payload) const;
  std::vector<std::uint8_t> encode_mouse_move(const MouseMovePayload& payload) const;
  std::vector<std::uint8_t> encode_mouse_button_down(const MouseButtonPayload& payload) const;
  std::vector<std::uint8_t> encode_mouse_button_up(const MouseButtonPayload& payload) const;
  std::vector<std::uint8_t> encode_mouse_wheel(const MouseWheelPayload& payload) const;
  std::vector<std::uint8_t> encode_gamepad_state(
      const GamepadInput& payload,
      std::uint16_t bitmap,
      bool use_partially_reliable);

 private:
  std::vector<std::uint8_t> encode_key(
      std::uint32_t type,
      const KeyboardPayload& payload) const;
  std::vector<std::uint8_t> encode_mouse_button(
      std::uint32_t type,
      const MouseButtonPayload& payload) const;

  int protocol_version_ = 2;
  std::unordered_map<int, std::uint16_t> gamepad_sequence_;
};

}  // namespace opennow
