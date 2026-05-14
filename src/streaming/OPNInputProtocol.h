#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <vector>

namespace OPN::Input {

constexpr uint32_t INPUT_HEARTBEAT = 2;
constexpr uint32_t INPUT_KEY_DOWN = 3;
constexpr uint32_t INPUT_KEY_UP = 4;
constexpr uint32_t INPUT_MOUSE_REL = 7;
constexpr uint32_t INPUT_MOUSE_BUTTON_DOWN = 8;
constexpr uint32_t INPUT_MOUSE_BUTTON_UP = 9;
constexpr uint32_t INPUT_MOUSE_WHEEL = 10;
constexpr uint32_t INPUT_GAMEPAD = 12;
constexpr uint32_t INPUT_HAPTICS_ENABLED = 13;

constexpr uint8_t MOUSE_LEFT = 1;
constexpr uint8_t MOUSE_MIDDLE = 2;
constexpr uint8_t MOUSE_RIGHT = 3;
constexpr uint8_t MOUSE_BACK = 4;
constexpr uint8_t MOUSE_FORWARD = 5;

constexpr uint16_t GAMEPAD_DPAD_UP = 0x0001;
constexpr uint16_t GAMEPAD_DPAD_DOWN = 0x0002;
constexpr uint16_t GAMEPAD_DPAD_LEFT = 0x0004;
constexpr uint16_t GAMEPAD_DPAD_RIGHT = 0x0008;
constexpr uint16_t GAMEPAD_START = 0x0010;
constexpr uint16_t GAMEPAD_BACK = 0x0020;
constexpr uint16_t GAMEPAD_LS = 0x0040;
constexpr uint16_t GAMEPAD_RS = 0x0080;
constexpr uint16_t GAMEPAD_LB = 0x0100;
constexpr uint16_t GAMEPAD_RB = 0x0200;
constexpr uint16_t GAMEPAD_GUIDE = 0x0400;
constexpr uint16_t GAMEPAD_A = 0x1000;
constexpr uint16_t GAMEPAD_B = 0x2000;
constexpr uint16_t GAMEPAD_X = 0x4000;
constexpr uint16_t GAMEPAD_Y = 0x8000;

constexpr int GAMEPAD_MAX_CONTROLLERS = 4;
constexpr double GAMEPAD_DEADZONE = 0.15;

struct KeyMapping {
    uint16_t vk = 0;
    uint16_t scancode = 0;
};

struct KeyboardPayload {
    uint16_t keycode = 0;
    uint16_t scancode = 0;
    uint16_t modifiers = 0;
    uint64_t timestampUs = 0;
};

struct MouseMovePayload {
    int16_t dx = 0;
    int16_t dy = 0;
    uint64_t timestampUs = 0;
};

struct MouseButtonPayload {
    uint8_t button = 0;
    uint64_t timestampUs = 0;
};

struct MouseWheelPayload {
    int16_t delta = 0;
    uint64_t timestampUs = 0;
};

struct GamepadState {
    uint16_t controllerId = 0;
    uint16_t buttons = 0;
    uint8_t leftTrigger = 0;
    uint8_t rightTrigger = 0;
    int16_t leftStickX = 0;
    int16_t leftStickY = 0;
    int16_t rightStickX = 0;
    int16_t rightStickY = 0;
    bool connected = false;
    uint64_t timestampUs = 0;
};

uint64_t TimestampUs();
std::optional<KeyMapping> MapMacKeyCode(uint16_t macKeyCode);
int16_t NormalizeAxisToInt16(double value);
uint8_t NormalizeTriggerToUint8(double value);
void ApplyRadialDeadzone(double x, double y, double &outX, double &outY);

class Encoder {
public:
    void SetProtocolVersion(uint16_t version);
    uint16_t ProtocolVersion() const { return m_protocolVersion; }

    std::vector<uint8_t> EncodeHeartbeat() const;
    std::vector<uint8_t> EncodeKeyDown(const KeyboardPayload &payload) const;
    std::vector<uint8_t> EncodeKeyUp(const KeyboardPayload &payload) const;
    std::vector<uint8_t> EncodeMouseMove(const MouseMovePayload &payload) const;
    std::vector<uint8_t> EncodeMouseButtonDown(const MouseButtonPayload &payload) const;
    std::vector<uint8_t> EncodeMouseButtonUp(const MouseButtonPayload &payload) const;
    std::vector<uint8_t> EncodeMouseWheel(const MouseWheelPayload &payload) const;
    std::vector<uint8_t> EncodeHapticsEnabled(bool enabled) const;
    std::vector<uint8_t> EncodeGamepadState(const GamepadState &payload, uint16_t bitmap, bool partiallyReliable);

private:
    std::vector<uint8_t> EncodeKey(uint32_t type, const KeyboardPayload &payload) const;
    std::vector<uint8_t> EncodeMouseButton(uint32_t type, const MouseButtonPayload &payload) const;
    std::vector<uint8_t> WrapSingleEvent(const std::vector<uint8_t> &payload) const;
    std::vector<uint8_t> WrapMouseMoveEvent(const std::vector<uint8_t> &payload) const;
    std::vector<uint8_t> WrapGamepadReliable(const std::vector<uint8_t> &payload) const;
    std::vector<uint8_t> WrapGamepadPartiallyReliable(const std::vector<uint8_t> &payload, uint8_t gamepadIndex, uint16_t sequenceNumber) const;
    uint16_t NextGamepadSequence(uint8_t gamepadIndex);

    uint16_t m_protocolVersion = 2;
    uint16_t m_gamepadSequences[GAMEPAD_MAX_CONTROLLERS] = {1, 1, 1, 1};
};

}
