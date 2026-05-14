#include "OPNInputProtocol.h"

#include <algorithm>
#include <chrono>
#include <cmath>

namespace OPN::Input {

static void WriteU16BE(std::vector<uint8_t> &out, size_t offset, uint16_t value) {
    out[offset] = (uint8_t)((value >> 8) & 0xff);
    out[offset + 1] = (uint8_t)(value & 0xff);
}

static void WriteU16LE(std::vector<uint8_t> &out, size_t offset, uint16_t value) {
    out[offset] = (uint8_t)(value & 0xff);
    out[offset + 1] = (uint8_t)((value >> 8) & 0xff);
}

static void WriteI16BE(std::vector<uint8_t> &out, size_t offset, int16_t value) {
    WriteU16BE(out, offset, (uint16_t)value);
}

static void WriteI16LE(std::vector<uint8_t> &out, size_t offset, int16_t value) {
    WriteU16LE(out, offset, (uint16_t)value);
}

static void WriteU32LE(std::vector<uint8_t> &out, size_t offset, uint32_t value) {
    out[offset] = (uint8_t)(value & 0xff);
    out[offset + 1] = (uint8_t)((value >> 8) & 0xff);
    out[offset + 2] = (uint8_t)((value >> 16) & 0xff);
    out[offset + 3] = (uint8_t)((value >> 24) & 0xff);
}

static void WriteU64BE(std::vector<uint8_t> &out, size_t offset, uint64_t value) {
    for (int i = 7; i >= 0; --i) {
        out[offset + (7 - i)] = (uint8_t)((value >> (i * 8)) & 0xff);
    }
}

static void WriteU64LE(std::vector<uint8_t> &out, size_t offset, uint64_t value) {
    for (int i = 0; i < 8; ++i) {
        out[offset + i] = (uint8_t)((value >> (i * 8)) & 0xff);
    }
}

uint64_t TimestampUs() {
    static const auto start = std::chrono::steady_clock::now();
    auto elapsed = std::chrono::steady_clock::now() - start;
    return (uint64_t)std::chrono::duration_cast<std::chrono::microseconds>(elapsed).count();
}

std::optional<KeyMapping> MapMacKeyCode(uint16_t key) {
    switch (key) {
        case 0: return KeyMapping{0x41, 0x001e};
        case 1: return KeyMapping{0x53, 0x001f};
        case 2: return KeyMapping{0x44, 0x0020};
        case 3: return KeyMapping{0x46, 0x0021};
        case 4: return KeyMapping{0x48, 0x0023};
        case 5: return KeyMapping{0x47, 0x0022};
        case 6: return KeyMapping{0x5a, 0x002c};
        case 7: return KeyMapping{0x58, 0x002d};
        case 8: return KeyMapping{0x43, 0x002e};
        case 9: return KeyMapping{0x56, 0x002f};
        case 11: return KeyMapping{0x42, 0x0030};
        case 12: return KeyMapping{0x51, 0x0010};
        case 13: return KeyMapping{0x57, 0x0011};
        case 14: return KeyMapping{0x45, 0x0012};
        case 15: return KeyMapping{0x52, 0x0013};
        case 16: return KeyMapping{0x59, 0x0015};
        case 17: return KeyMapping{0x54, 0x0014};
        case 18: return KeyMapping{0x31, 0x0002};
        case 19: return KeyMapping{0x32, 0x0003};
        case 20: return KeyMapping{0x33, 0x0004};
        case 21: return KeyMapping{0x34, 0x0005};
        case 22: return KeyMapping{0x36, 0x0007};
        case 23: return KeyMapping{0x35, 0x0006};
        case 24: return KeyMapping{0xbb, 0x000d};
        case 25: return KeyMapping{0x39, 0x000a};
        case 26: return KeyMapping{0x37, 0x0008};
        case 27: return KeyMapping{0xbd, 0x000c};
        case 28: return KeyMapping{0x38, 0x0009};
        case 29: return KeyMapping{0x30, 0x000b};
        case 30: return KeyMapping{0xdd, 0x001b};
        case 31: return KeyMapping{0x4f, 0x0018};
        case 32: return KeyMapping{0x55, 0x0016};
        case 33: return KeyMapping{0xdb, 0x001a};
        case 34: return KeyMapping{0x49, 0x0017};
        case 35: return KeyMapping{0x50, 0x0019};
        case 36: return KeyMapping{0x0d, 0x001c};
        case 37: return KeyMapping{0x4c, 0x0026};
        case 38: return KeyMapping{0x4a, 0x0024};
        case 39: return KeyMapping{0xde, 0x0028};
        case 40: return KeyMapping{0x4b, 0x0025};
        case 41: return KeyMapping{0xba, 0x0027};
        case 42: return KeyMapping{0xdc, 0x002b};
        case 43: return KeyMapping{0xbc, 0x0033};
        case 44: return KeyMapping{0xbf, 0x0035};
        case 45: return KeyMapping{0x4e, 0x0031};
        case 46: return KeyMapping{0x4d, 0x0032};
        case 47: return KeyMapping{0xbe, 0x0034};
        case 48: return KeyMapping{0x09, 0x000f};
        case 49: return KeyMapping{0x20, 0x0039};
        case 50: return KeyMapping{0xc0, 0x0029};
        case 51: return KeyMapping{0x08, 0x000e};
        case 53: return KeyMapping{0x1b, 0x0001};
        case 55: return KeyMapping{0x5b, 0xe05b};
        case 56: return KeyMapping{0xa0, 0x002a};
        case 57: return KeyMapping{0x14, 0x003a};
        case 58: return KeyMapping{0xa4, 0x0038};
        case 59: return KeyMapping{0xa2, 0x001d};
        case 60: return KeyMapping{0xa1, 0x0036};
        case 61: return KeyMapping{0xa5, 0xe038};
        case 62: return KeyMapping{0xa3, 0xe01d};
        case 65: return KeyMapping{0x6e, 0x0053};
        case 67: return KeyMapping{0x6a, 0x0037};
        case 69: return KeyMapping{0x6b, 0x004e};
        case 71: return KeyMapping{0x90, 0xe045};
        case 75: return KeyMapping{0x6f, 0xe035};
        case 76: return KeyMapping{0x0d, 0xe01c};
        case 78: return KeyMapping{0x6d, 0x004a};
        case 81: return KeyMapping{0xbb, 0x0059};
        case 82: return KeyMapping{0x60, 0x0052};
        case 83: return KeyMapping{0x61, 0x004f};
        case 84: return KeyMapping{0x62, 0x0050};
        case 85: return KeyMapping{0x63, 0x0051};
        case 86: return KeyMapping{0x64, 0x004b};
        case 87: return KeyMapping{0x65, 0x004c};
        case 88: return KeyMapping{0x66, 0x004d};
        case 89: return KeyMapping{0x67, 0x0047};
        case 91: return KeyMapping{0x68, 0x0048};
        case 92: return KeyMapping{0x69, 0x0049};
        case 96: return KeyMapping{0x74, 0x003f};
        case 97: return KeyMapping{0x75, 0x0040};
        case 98: return KeyMapping{0x76, 0x0041};
        case 99: return KeyMapping{0x72, 0x003d};
        case 100: return KeyMapping{0x77, 0x0042};
        case 101: return KeyMapping{0x78, 0x0043};
        case 103: return KeyMapping{0x7a, 0x0057};
        case 105: return KeyMapping{0x7c, 0x0064};
        case 106: return KeyMapping{0x7f, 0x0067};
        case 107: return KeyMapping{0x7d, 0x0065};
        case 109: return KeyMapping{0x79, 0x0044};
        case 111: return KeyMapping{0x7b, 0x0058};
        case 113: return KeyMapping{0x7e, 0x0066};
        case 114: return KeyMapping{0x2d, 0xe052};
        case 115: return KeyMapping{0x24, 0xe047};
        case 116: return KeyMapping{0x21, 0xe049};
        case 117: return KeyMapping{0x2e, 0xe053};
        case 118: return KeyMapping{0x73, 0x003e};
        case 119: return KeyMapping{0x23, 0xe04f};
        case 120: return KeyMapping{0x71, 0x003c};
        case 121: return KeyMapping{0x22, 0xe051};
        case 122: return KeyMapping{0x70, 0x003b};
        case 123: return KeyMapping{0x25, 0xe04b};
        case 124: return KeyMapping{0x27, 0xe04d};
        case 125: return KeyMapping{0x28, 0xe050};
        case 126: return KeyMapping{0x26, 0xe048};
        default: return std::nullopt;
    }
}

void ApplyRadialDeadzone(double x, double y, double &outX, double &outY) {
    double magnitude = std::sqrt(x * x + y * y);
    if (magnitude < GAMEPAD_DEADZONE) {
        outX = 0;
        outY = 0;
        return;
    }
    double scaled = std::min(1.0, (magnitude - GAMEPAD_DEADZONE) / (1.0 - GAMEPAD_DEADZONE));
    outX = (x / magnitude) * scaled;
    outY = (y / magnitude) * scaled;
}

int16_t NormalizeAxisToInt16(double value) {
    value = std::max(-1.0, std::min(1.0, value));
    return (int16_t)std::max(-32768.0, std::min(32767.0, std::round(value * 32767.0)));
}

uint8_t NormalizeTriggerToUint8(double value) {
    value = std::max(0.0, std::min(1.0, value));
    return (uint8_t)std::max(0.0, std::min(255.0, std::round(value * 255.0)));
}

void Encoder::SetProtocolVersion(uint16_t version) {
    m_protocolVersion = version == 0 ? 2 : version;
}

std::vector<uint8_t> Encoder::EncodeHeartbeat() const {
    std::vector<uint8_t> bytes(4, 0);
    WriteU32LE(bytes, 0, INPUT_HEARTBEAT);
    return bytes;
}

std::vector<uint8_t> Encoder::EncodeKeyDown(const KeyboardPayload &payload) const {
    return EncodeKey(INPUT_KEY_DOWN, payload);
}

std::vector<uint8_t> Encoder::EncodeKeyUp(const KeyboardPayload &payload) const {
    return EncodeKey(INPUT_KEY_UP, payload);
}

std::vector<uint8_t> Encoder::EncodeKey(uint32_t type, const KeyboardPayload &payload) const {
    std::vector<uint8_t> bytes(18, 0);
    WriteU32LE(bytes, 0, type);
    WriteU16BE(bytes, 4, payload.keycode);
    WriteU16BE(bytes, 6, payload.modifiers);
    WriteU16BE(bytes, 8, payload.scancode);
    WriteU64BE(bytes, 10, payload.timestampUs);
    return WrapSingleEvent(bytes);
}

std::vector<uint8_t> Encoder::EncodeMouseMove(const MouseMovePayload &payload) const {
    std::vector<uint8_t> bytes(22, 0);
    WriteU32LE(bytes, 0, INPUT_MOUSE_REL);
    WriteI16BE(bytes, 4, payload.dx);
    WriteI16BE(bytes, 6, payload.dy);
    WriteU64BE(bytes, 14, payload.timestampUs);
    return WrapMouseMoveEvent(bytes);
}

std::vector<uint8_t> Encoder::EncodeMouseButtonDown(const MouseButtonPayload &payload) const {
    return EncodeMouseButton(INPUT_MOUSE_BUTTON_DOWN, payload);
}

std::vector<uint8_t> Encoder::EncodeMouseButtonUp(const MouseButtonPayload &payload) const {
    return EncodeMouseButton(INPUT_MOUSE_BUTTON_UP, payload);
}

std::vector<uint8_t> Encoder::EncodeMouseButton(uint32_t type, const MouseButtonPayload &payload) const {
    std::vector<uint8_t> bytes(18, 0);
    WriteU32LE(bytes, 0, type);
    bytes[4] = payload.button;
    WriteU64BE(bytes, 10, payload.timestampUs);
    return WrapSingleEvent(bytes);
}

std::vector<uint8_t> Encoder::EncodeMouseWheel(const MouseWheelPayload &payload) const {
    std::vector<uint8_t> bytes(22, 0);
    WriteU32LE(bytes, 0, INPUT_MOUSE_WHEEL);
    WriteI16BE(bytes, 6, payload.delta);
    WriteU64BE(bytes, 14, payload.timestampUs);
    return WrapSingleEvent(bytes);
}

std::vector<uint8_t> Encoder::EncodeHapticsEnabled(bool enabled) const {
    std::vector<uint8_t> bytes(6, 0);
    WriteU32LE(bytes, 0, INPUT_HAPTICS_ENABLED);
    WriteU16BE(bytes, 4, enabled ? 1 : 0);
    return WrapSingleEvent(bytes);
}

std::vector<uint8_t> Encoder::EncodeGamepadState(const GamepadState &payload, uint16_t bitmap, bool partiallyReliable) {
    std::vector<uint8_t> bytes(38, 0);
    WriteU32LE(bytes, 0, INPUT_GAMEPAD);
    WriteU16LE(bytes, 4, 26);
    WriteU16LE(bytes, 6, payload.controllerId & 0x03);
    WriteU16LE(bytes, 8, bitmap);
    WriteU16LE(bytes, 10, 20);
    WriteU16LE(bytes, 12, payload.buttons);
    WriteU16LE(bytes, 14, (uint16_t)(payload.leftTrigger | (payload.rightTrigger << 8)));
    WriteI16LE(bytes, 16, payload.leftStickX);
    WriteI16LE(bytes, 18, payload.leftStickY);
    WriteI16LE(bytes, 20, payload.rightStickX);
    WriteI16LE(bytes, 22, payload.rightStickY);
    WriteU16LE(bytes, 26, 85);
    WriteU64LE(bytes, 30, payload.timestampUs);
    if (partiallyReliable) {
        uint8_t idx = (uint8_t)(payload.controllerId & 0x03);
        return WrapGamepadPartiallyReliable(bytes, idx, NextGamepadSequence(idx));
    }
    return WrapGamepadReliable(bytes);
}

std::vector<uint8_t> Encoder::WrapSingleEvent(const std::vector<uint8_t> &payload) const {
    if (m_protocolVersion <= 2) return payload;
    std::vector<uint8_t> wrapped(10 + payload.size(), 0);
    wrapped[0] = 0x23;
    WriteU64BE(wrapped, 1, TimestampUs());
    wrapped[9] = 0x22;
    std::copy(payload.begin(), payload.end(), wrapped.begin() + 10);
    return wrapped;
}

std::vector<uint8_t> Encoder::WrapMouseMoveEvent(const std::vector<uint8_t> &payload) const {
    if (m_protocolVersion <= 2) return payload;
    std::vector<uint8_t> wrapped(12 + payload.size(), 0);
    wrapped[0] = 0x23;
    WriteU64BE(wrapped, 1, TimestampUs());
    wrapped[9] = 0x21;
    WriteU16BE(wrapped, 10, (uint16_t)payload.size());
    std::copy(payload.begin(), payload.end(), wrapped.begin() + 12);
    return wrapped;
}

std::vector<uint8_t> Encoder::WrapGamepadReliable(const std::vector<uint8_t> &payload) const {
    if (m_protocolVersion <= 2) return payload;
    std::vector<uint8_t> wrapped(12 + payload.size(), 0);
    wrapped[0] = 0x23;
    WriteU64BE(wrapped, 1, TimestampUs());
    wrapped[9] = 0x21;
    WriteU16BE(wrapped, 10, (uint16_t)payload.size());
    std::copy(payload.begin(), payload.end(), wrapped.begin() + 12);
    return wrapped;
}

std::vector<uint8_t> Encoder::WrapGamepadPartiallyReliable(const std::vector<uint8_t> &payload, uint8_t gamepadIndex, uint16_t sequenceNumber) const {
    if (m_protocolVersion <= 2) return payload;
    std::vector<uint8_t> wrapped(16 + payload.size(), 0);
    wrapped[0] = 0x23;
    WriteU64BE(wrapped, 1, TimestampUs());
    wrapped[9] = 0x26;
    wrapped[10] = gamepadIndex;
    WriteU16BE(wrapped, 11, sequenceNumber);
    wrapped[13] = 0x21;
    WriteU16BE(wrapped, 14, (uint16_t)payload.size());
    std::copy(payload.begin(), payload.end(), wrapped.begin() + 16);
    return wrapped;
}

uint16_t Encoder::NextGamepadSequence(uint8_t gamepadIndex) {
    uint8_t idx = gamepadIndex % GAMEPAD_MAX_CONTROLLERS;
    uint16_t current = m_gamepadSequences[idx];
    m_gamepadSequences[idx] = (uint16_t)(current + 1);
    return current;
}

}
