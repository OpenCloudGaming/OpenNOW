#include "ControllerInput.h"

#include <QCoreApplication>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QVariantMap>

#include <algorithm>
#include <cmath>

namespace {
constexpr Sint16 axisPressThreshold = 18000;
constexpr Sint16 axisReleaseThreshold = 12000;
constexpr qint64 repeatDelayMs = 280;
constexpr qint64 repeatIntervalMs = 85;
constexpr qint64 gamepadKeepaliveMs = 100;
constexpr quint32 guideLocalAction = 1;
}

ControllerInput::ControllerInput(QObject *parent)
    : QObject(parent)
{
    SDL_SetHint(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, "1");
    m_sdlReady = SDL_InitSubSystem(SDL_INIT_GAMEPAD);
    if (!m_sdlReady) {
        qWarning("SDL gamepad initialization failed: %s", SDL_GetError());
        return;
    }

    int count = 0;
    if (auto *ids = SDL_GetGamepads(&count)) {
        for (int index = 0; index < count; ++index) openController(ids[index]);
        SDL_free(ids);
    }

    m_clock.start();
    m_pollTimer.setTimerType(Qt::PreciseTimer);
    m_pollTimer.setInterval(4);
    connect(&m_pollTimer, &QTimer::timeout, this, &ControllerInput::poll);
    m_pollTimer.start();
}

ControllerInput::~ControllerInput()
{
    m_pollTimer.stop();
    for (auto &slot : m_slots) {
        if (slot.gamepad) SDL_CloseGamepad(slot.gamepad);
        slot = {};
    }
    m_gamepadSlots.clear();
    if (m_sdlReady) SDL_QuitSubSystem(SDL_INIT_GAMEPAD);
}

int ControllerInput::controllerCount() const
{
    return m_gamepadSlots.size();
}

QVariantList ControllerInput::controllers() const
{
    QVariantList result;
    result.reserve(m_gamepadSlots.size());
    for (qsizetype index = 0; index < static_cast<qsizetype>(m_slots.size()); ++index) {
        const auto &slot = m_slots[static_cast<std::size_t>(index)];
        if (!slot.gamepad) continue;
        int batteryPercent = -1;
        const auto power = SDL_GetGamepadPowerInfo(slot.gamepad, &batteryPercent);
        const auto *name = SDL_GetGamepadName(slot.gamepad);
        result.push_back(QVariantMap{
            {QStringLiteral("slot"), index + 1},
            {QStringLiteral("instanceId"), static_cast<qulonglong>(slot.instanceId)},
            {QStringLiteral("name"), name ? QString::fromUtf8(name)
                                           : QStringLiteral("Game controller")},
            {QStringLiteral("batteryPercent"), batteryPercent},
            {QStringLiteral("charging"), power == SDL_POWERSTATE_CHARGING},
        });
    }
    return result;
}

bool ControllerInput::shellCaptureEnabled() const
{
    return m_shellCaptureEnabled;
}

void ControllerInput::setShellCaptureEnabled(bool enabled)
{
    if (m_shellCaptureEnabled == enabled) return;
    if (enabled) publishConnectedGamepads(true);
    m_shellCaptureEnabled = enabled;
    if (!enabled) {
        for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
            updateSlotSnapshot(slot);
        publishConnectedGamepads();
        m_lastGamepadSnapshotAt = m_clock.elapsed();
    } else {
        for (auto *direction : {&m_left, &m_right, &m_up, &m_down}) {
            direction->active = false;
            direction->pressedAt = 0;
            direction->repeatedAt = 0;
        }
    }
    emit shellCaptureEnabledChanged();
}

void ControllerInput::poll()
{
    SDL_Event event;
    while (SDL_PollEvent(&event)) {
        switch (event.type) {
        case SDL_EVENT_GAMEPAD_ADDED:
            openController(event.gdevice.which);
            break;
        case SDL_EVENT_GAMEPAD_REMOVED:
            closeController(event.gdevice.which);
            break;
        case SDL_EVENT_GAMEPAD_BUTTON_DOWN:
            handleButton(event.gbutton, true);
            break;
        case SDL_EVENT_GAMEPAD_BUTTON_UP:
            handleButton(event.gbutton, false);
            break;
        case SDL_EVENT_GAMEPAD_AXIS_MOTION:
            handleAxis(event.gaxis);
            break;
        default:
            break;
        }
    }

    const auto now = m_clock.elapsed();
    dispatchRepeats(now);
    if (!m_shellCaptureEnabled && now - m_lastGamepadSnapshotAt >= gamepadKeepaliveMs) {
        publishConnectedGamepads();
        m_lastGamepadSnapshotAt = now;
    }
    if (now - m_lastControllerMetadataAt >= 2'000) {
        m_lastControllerMetadataAt = now;
        emit controllersChanged();
    }
}

void ControllerInput::openController(SDL_JoystickID id)
{
    if (m_gamepadSlots.contains(id)) return;
    const auto freeSlot = std::find_if(m_slots.begin(), m_slots.end(),
                                      [](const GamepadSlot &slot) { return !slot.gamepad; });
    if (freeSlot == m_slots.end()) return;
    auto *gamepad = SDL_OpenGamepad(id);
    if (!gamepad) {
        qWarning("Could not open SDL gamepad %u: %s", id, SDL_GetError());
        return;
    }
    const auto slot = static_cast<int>(std::distance(m_slots.begin(), freeSlot));
    *freeSlot = {.gamepad = gamepad, .instanceId = id};
    m_gamepadSlots.insert(id, slot);
    updateSlotSnapshot(slot);
    if (!m_shellCaptureEnabled) publishGamepad(slot);
    emit controllerCountChanged(m_gamepadSlots.size());
    emit controllersChanged();
}

void ControllerInput::closeController(SDL_JoystickID id)
{
    if (!m_gamepadSlots.contains(id)) return;
    const auto slotIndex = m_gamepadSlots.take(id);
    if (slotIndex < 0 || slotIndex >= static_cast<int>(m_slots.size())) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad) return;
    SDL_CloseGamepad(slot.gamepad);
    slot = {};
    if (!m_shellCaptureEnabled) publishGamepad(slotIndex, true);
    for (auto *direction : {&m_left, &m_right, &m_up, &m_down}) {
        direction->active = false;
        direction->pressedAt = 0;
        direction->repeatedAt = 0;
    }
    emit controllerCountChanged(m_gamepadSlots.size());
    emit controllersChanged();
}

void ControllerInput::handleButton(const SDL_GamepadButtonEvent &event, bool pressed)
{
    const auto slotIndex = m_gamepadSlots.value(event.which, -1);
    if (slotIndex < 0) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (event.button == SDL_GAMEPAD_BUTTON_GUIDE) {
        if (pressed && !m_shellCaptureEnabled)
            emit localActionRequested(guideLocalAction);
    } else if (const auto mask = buttonMask(event.button); mask != 0) {
        if (pressed) slot.buttons |= mask;
        else slot.buttons &= static_cast<quint16>(~mask);
        if (!m_shellCaptureEnabled) publishGamepad(slotIndex);
    }

    if (!m_shellCaptureEnabled) return;
    const auto key = keyForButton(event.button);
    if (key != 0) {
        postKey(key, pressed);
        if (pressed) emit controllerActivity();
    }
}

void ControllerInput::handleAxis(const SDL_GamepadAxisEvent &event)
{
    const auto slotIndex = m_gamepadSlots.value(event.which, -1);
    if (slotIndex < 0) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    switch (event.axis) {
    case SDL_GAMEPAD_AXIS_LEFTX: slot.rawLeftX = event.value; break;
    case SDL_GAMEPAD_AXIS_LEFTY: slot.rawLeftY = event.value; break;
    case SDL_GAMEPAD_AXIS_RIGHTX: slot.rawRightX = event.value; break;
    case SDL_GAMEPAD_AXIS_RIGHTY: slot.rawRightY = event.value; break;
    case SDL_GAMEPAD_AXIS_LEFT_TRIGGER: slot.leftTrigger = triggerValue(event.value); break;
    case SDL_GAMEPAD_AXIS_RIGHT_TRIGGER: slot.rightTrigger = triggerValue(event.value); break;
    default: return;
    }
    if (!m_shellCaptureEnabled) publishGamepad(slotIndex);
    if (!m_shellCaptureEnabled) return;

    const auto value = event.value;
    if (event.axis == SDL_GAMEPAD_AXIS_LEFTX) {
        updateDirection(m_left, value < (m_left.active ? -axisReleaseThreshold : -axisPressThreshold));
        updateDirection(m_right, value > (m_right.active ? axisReleaseThreshold : axisPressThreshold));
    } else if (event.axis == SDL_GAMEPAD_AXIS_LEFTY) {
        updateDirection(m_up, value < (m_up.active ? -axisReleaseThreshold : -axisPressThreshold));
        updateDirection(m_down, value > (m_down.active ? axisReleaseThreshold : axisPressThreshold));
    }
}

quint16 ControllerInput::gamepadBitmap() const
{
    quint16 bitmap = 0;
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot) {
        if (m_slots[static_cast<std::size_t>(slot)].gamepad)
            bitmap |= static_cast<quint16>((1u << slot) | (1u << (slot + 8)));
    }
    return bitmap;
}

void ControllerInput::publishGamepad(int slotIndex, bool neutral)
{
    const auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    const auto left = neutral ? QPair<qint16, qint16>{} : radialDeadzone(slot.rawLeftX, slot.rawLeftY);
    const auto right = neutral ? QPair<qint16, qint16>{} : radialDeadzone(slot.rawRightX, slot.rawRightY);
    emit gamepadSnapshot(static_cast<quint8>(slotIndex), gamepadBitmap(),
                         neutral ? 0 : slot.buttons,
                         neutral ? 0 : slot.leftTrigger, neutral ? 0 : slot.rightTrigger,
                         left.first, static_cast<qint16>(-left.second),
                         right.first, static_cast<qint16>(-right.second));
}

void ControllerInput::publishConnectedGamepads(bool neutral)
{
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot) {
        if (m_slots[static_cast<std::size_t>(slot)].gamepad) publishGamepad(slot, neutral);
    }
}

void ControllerInput::updateSlotSnapshot(int slotIndex)
{
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad) return;
    slot.buttons = 0;
    for (int button = 0; button < SDL_GAMEPAD_BUTTON_COUNT; ++button) {
        if (SDL_GetGamepadButton(slot.gamepad, static_cast<SDL_GamepadButton>(button)))
            slot.buttons |= buttonMask(static_cast<Uint8>(button));
    }
    slot.rawLeftX = SDL_GetGamepadAxis(slot.gamepad, SDL_GAMEPAD_AXIS_LEFTX);
    slot.rawLeftY = SDL_GetGamepadAxis(slot.gamepad, SDL_GAMEPAD_AXIS_LEFTY);
    slot.rawRightX = SDL_GetGamepadAxis(slot.gamepad, SDL_GAMEPAD_AXIS_RIGHTX);
    slot.rawRightY = SDL_GetGamepadAxis(slot.gamepad, SDL_GAMEPAD_AXIS_RIGHTY);
    slot.leftTrigger = triggerValue(
        SDL_GetGamepadAxis(slot.gamepad, SDL_GAMEPAD_AXIS_LEFT_TRIGGER));
    slot.rightTrigger = triggerValue(
        SDL_GetGamepadAxis(slot.gamepad, SDL_GAMEPAD_AXIS_RIGHT_TRIGGER));
}

quint16 ControllerInput::buttonMask(Uint8 button)
{
    switch (button) {
    case SDL_GAMEPAD_BUTTON_SOUTH: return 0x1000;
    case SDL_GAMEPAD_BUTTON_EAST: return 0x2000;
    case SDL_GAMEPAD_BUTTON_WEST: return 0x4000;
    case SDL_GAMEPAD_BUTTON_NORTH: return 0x8000;
    case SDL_GAMEPAD_BUTTON_BACK: return 0x0020;
    case SDL_GAMEPAD_BUTTON_START: return 0x0010;
    case SDL_GAMEPAD_BUTTON_LEFT_STICK: return 0x0040;
    case SDL_GAMEPAD_BUTTON_RIGHT_STICK: return 0x0080;
    case SDL_GAMEPAD_BUTTON_LEFT_SHOULDER: return 0x0100;
    case SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER: return 0x0200;
    case SDL_GAMEPAD_BUTTON_DPAD_UP: return 0x0001;
    case SDL_GAMEPAD_BUTTON_DPAD_DOWN: return 0x0002;
    case SDL_GAMEPAD_BUTTON_DPAD_LEFT: return 0x0004;
    case SDL_GAMEPAD_BUTTON_DPAD_RIGHT: return 0x0008;
    default: return 0;
    }
}

QPair<qint16, qint16> ControllerInput::radialDeadzone(qint16 x, qint16 y)
{
    constexpr double deadzone = 0.15;
    const auto normalizedX = static_cast<double>(x) / 32767.0;
    const auto normalizedY = static_cast<double>(y) / 32767.0;
    const auto magnitude = std::hypot(normalizedX, normalizedY);
    if (magnitude < deadzone) return {};
    const auto scaled = std::clamp((magnitude - deadzone) / (1.0 - deadzone), 0.0, 1.0);
    const auto factor = scaled / magnitude;
    return {static_cast<qint16>(std::round(normalizedX * factor * 32767.0)),
            static_cast<qint16>(std::round(normalizedY * factor * 32767.0))};
}

quint8 ControllerInput::triggerValue(qint16 value)
{
    return static_cast<quint8>(std::round(
        std::clamp(static_cast<double>(value) / 32767.0, 0.0, 1.0) * 255.0));
}

void ControllerInput::updateDirection(RepeatingDirection &direction, bool active)
{
    if (direction.active == active) return;
    direction.active = active;
    if (active) {
        direction.pressedAt = m_clock.elapsed();
        direction.repeatedAt = direction.pressedAt;
        postKey(direction.key, true);
        postKey(direction.key, false);
        emit controllerActivity();
    }
}

void ControllerInput::dispatchRepeats(qint64 now)
{
    for (auto *direction : {&m_left, &m_right, &m_up, &m_down}) {
        if (!direction->active || now - direction->pressedAt < repeatDelayMs
            || now - direction->repeatedAt < repeatIntervalMs) continue;
        direction->repeatedAt = now;
        postKey(direction->key, true, true);
        postKey(direction->key, false, true);
    }
}

void ControllerInput::postKey(int key, bool pressed, bool autoRepeat)
{
    if (!m_shellCaptureEnabled) return;
    auto *target = QGuiApplication::focusObject();
    if (!target) target = QCoreApplication::instance();
    const auto type = pressed ? QEvent::KeyPress : QEvent::KeyRelease;
    QCoreApplication::postEvent(
        target, new QKeyEvent(type, key, Qt::NoModifier, syntheticControllerScanCode,
                              0, 0, QString(), autoRepeat));
}

int ControllerInput::keyForButton(Uint8 button)
{
    switch (button) {
    case SDL_GAMEPAD_BUTTON_SOUTH: return Qt::Key_Return;
    case SDL_GAMEPAD_BUTTON_EAST: return Qt::Key_Escape;
    case SDL_GAMEPAD_BUTTON_WEST: return Qt::Key_X;
    case SDL_GAMEPAD_BUTTON_NORTH: return Qt::Key_Y;
    case SDL_GAMEPAD_BUTTON_DPAD_LEFT: return Qt::Key_Left;
    case SDL_GAMEPAD_BUTTON_DPAD_RIGHT: return Qt::Key_Right;
    case SDL_GAMEPAD_BUTTON_DPAD_UP: return Qt::Key_Up;
    case SDL_GAMEPAD_BUTTON_DPAD_DOWN: return Qt::Key_Down;
    case SDL_GAMEPAD_BUTTON_LEFT_SHOULDER: return Qt::Key_PageUp;
    case SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER: return Qt::Key_PageDown;
    case SDL_GAMEPAD_BUTTON_START: return Qt::Key_Menu;
    case SDL_GAMEPAD_BUTTON_BACK: return Qt::Key_Back;
    case SDL_GAMEPAD_BUTTON_GUIDE: return Qt::Key_F1;
    default: return 0;
    }
}
