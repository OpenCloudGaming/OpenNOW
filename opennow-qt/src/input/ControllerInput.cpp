#include "input/ControllerInput.h"

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
    : QObject(parent), m_pollTimer(this)
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
    m_pollTimer.setObjectName(QStringLiteral("controllerPollTimer"));
    updatePollInterval();
    connect(&m_pollTimer, &QTimer::timeout, this, &ControllerInput::poll);
    m_pollTimer.start();
}

ControllerInput::~ControllerInput()
{
    m_pollTimer.stop();
    stopRumble();
    publishConnectedGamepads(true);
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
        releaseShellButtons(slot);
    for (auto &slot : m_slots) {
        if (slot.gamepad) SDL_CloseGamepad(slot.gamepad);
        slot = {};
    }
    m_gamepadSlots.clear();
    if (m_sdlReady) SDL_QuitSubSystem(SDL_INIT_GAMEPAD);
}

int ControllerInput::controllerCount() const
{
    return m_inputControllerId ? static_cast<int>(m_gamepadSlots.contains(m_inputControllerId))
                               : m_gamepadSlots.size();
}

QVariantList ControllerInput::controllers() const
{
    return m_controllerMetadata;
}

QVariantList ControllerInput::availableControllers() const
{
    return m_availableControllerMetadata;
}

quint32 ControllerInput::inputControllerId() const
{
    return m_inputControllerId;
}

bool ControllerInput::acceptsController(SDL_JoystickID id) const
{
    return id != 0 && (m_inputControllerId == 0 || m_inputControllerId == id);
}

void ControllerInput::setInputControllerId(quint32 id)
{
    if (id == m_inputControllerId || (id != 0 && !m_gamepadSlots.contains(id))) return;
    stopRumble();
    publishConnectedGamepads(true);
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
        releaseShellButtons(slot);
    resetDirections();
    m_inputControllerId = id;
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
        updateSlotSnapshot(slot);
    if (!m_shellCaptureEnabled) publishConnectedGamepads();
    refreshControllerMetadata();
    updatePollInterval();
    emit controllerCountChanged(controllerCount());
    emit inputControllerIdChanged();
}

void ControllerInput::updatePollInterval()
{
    // Keep hotplug discovery alive without waking the GUI 250 times/second
    // when there is no controller. Gameplay retains its low-latency cadence.
    const auto interval = controllerCount() == 0 || m_inputSuspended ? 100 : m_shellCaptureEnabled ? 16 : 4;
    m_pollTimer.setTimerType(interval == 4 ? Qt::PreciseTimer : Qt::CoarseTimer);
    if (m_pollTimer.interval() != interval) m_pollTimer.setInterval(interval);
}

void ControllerInput::refreshControllerMetadata()
{
    QVariantList result;
    QVariantList available;
    result.reserve(m_gamepadSlots.size());
    for (qsizetype index = 0; index < static_cast<qsizetype>(m_slots.size()); ++index) {
        const auto &slot = m_slots[static_cast<std::size_t>(index)];
        if (!slot.gamepad) continue;
        int batteryPercent = -1;
        const auto power = SDL_GetGamepadPowerInfo(slot.gamepad, &batteryPercent);
        QString family = QStringLiteral("generic");
        switch (SDL_GetRealGamepadType(slot.gamepad)) {
        case SDL_GAMEPAD_TYPE_PS3:
        case SDL_GAMEPAD_TYPE_PS4:
        case SDL_GAMEPAD_TYPE_PS5:
            family = QStringLiteral("playstation");
            break;
        case SDL_GAMEPAD_TYPE_XBOX360:
        case SDL_GAMEPAD_TYPE_XBOXONE:
            family = QStringLiteral("xbox");
            break;
        default:
            break;
        }
        QString powerState = QStringLiteral("unknown");
        switch (power) {
        case SDL_POWERSTATE_ON_BATTERY: powerState = QStringLiteral("onBattery"); break;
        case SDL_POWERSTATE_NO_BATTERY: powerState = QStringLiteral("noBattery"); break;
        case SDL_POWERSTATE_CHARGING: powerState = QStringLiteral("charging"); break;
        case SDL_POWERSTATE_CHARGED: powerState = QStringLiteral("charged"); break;
        default: break;
        }
        if (power == SDL_POWERSTATE_UNKNOWN || power == SDL_POWERSTATE_ERROR
            || power == SDL_POWERSTATE_NO_BATTERY || batteryPercent < 0 || batteryPercent > 100)
            batteryPercent = -1;
        const auto *name = SDL_GetGamepadName(slot.gamepad);
        QVariantMap metadata{
            {QStringLiteral("slot"), index + 1},
            {QStringLiteral("instanceId"), static_cast<qulonglong>(slot.instanceId)},
            {QStringLiteral("name"), name ? QString::fromUtf8(name)
                                           : QStringLiteral("Game controller")},
            {QStringLiteral("family"), family},
            {QStringLiteral("powerState"), powerState},
            {QStringLiteral("batteryPercent"), batteryPercent},
            {QStringLiteral("charging"), power == SDL_POWERSTATE_CHARGING},
        };
        available.push_back(metadata);
        if (acceptsController(slot.instanceId)) {
            if (m_inputControllerId) metadata[QStringLiteral("slot")] = 1;
            result.push_back(metadata);
        }
    }
    if (result != m_controllerMetadata) {
        m_controllerMetadata = std::move(result);
        emit controllersChanged();
    }
    if (available != m_availableControllerMetadata) {
        m_availableControllerMetadata = std::move(available);
        emit availableControllersChanged();
    }
}

bool ControllerInput::shellCaptureEnabled() const
{
    return m_shellCaptureEnabled;
}

bool ControllerInput::inputSuspended() const
{
    return m_inputSuspended;
}

int ControllerInput::leftStickDeadzone() const { return m_leftStickDeadzone; }
int ControllerInput::rightStickDeadzone() const { return m_rightStickDeadzone; }
int ControllerInput::vibrationIntensity() const { return m_vibrationIntensity; }

void ControllerInput::setLeftStickDeadzone(int percent)
{
    percent = std::clamp(percent, 0, 50);
    if (m_leftStickDeadzone == percent) return;
    m_leftStickDeadzone = percent;
    if (!m_shellCaptureEnabled) publishConnectedGamepads();
    emit leftStickDeadzoneChanged();
}

void ControllerInput::setRightStickDeadzone(int percent)
{
    percent = std::clamp(percent, 0, 50);
    if (m_rightStickDeadzone == percent) return;
    m_rightStickDeadzone = percent;
    if (!m_shellCaptureEnabled) publishConnectedGamepads();
    emit rightStickDeadzoneChanged();
}

void ControllerInput::setVibrationIntensity(int percent)
{
    percent = std::clamp(percent, 0, 100);
    if (m_vibrationIntensity == percent) return;
    stopRumble();
    m_vibrationIntensity = percent;
    emit vibrationIntensityChanged();
}

void ControllerInput::playRumble(quint8 controllerId, quint16 lowFrequency,
                               quint16 highFrequency, quint32 durationMs)
{
    if (m_shellCaptureEnabled || m_inputSuspended || m_vibrationIntensity == 0) return;
    const auto slotIndex = m_inputControllerId
        ? (controllerId == 0 ? m_gamepadSlots.value(m_inputControllerId, -1) : -1)
        : static_cast<int>(controllerId);
    if (slotIndex < 0 || slotIndex >= static_cast<int>(m_slots.size())) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad || !acceptsController(slot.instanceId)) return;
    const auto properties = SDL_GetGamepadProperties(slot.gamepad);
    if (!SDL_GetBooleanProperty(properties, SDL_PROP_GAMEPAD_CAP_RUMBLE_BOOLEAN, false)) return;
    if (!SDL_RumbleGamepad(slot.gamepad,
                     static_cast<quint16>(quint32(lowFrequency) * m_vibrationIntensity / 100),
                     static_cast<quint16>(quint32(highFrequency) * m_vibrationIntensity / 100),
                     std::min(durationMs, quint32(65'535))) && !slot.rumbleFailureReported) {
        slot.rumbleFailureReported = true;
        qWarning("Controller vibration failed for player %u: %s",
                 static_cast<unsigned>(controllerId) + 1, SDL_GetError());
    }
}

void ControllerInput::stopRumble()
{
    for (const auto &slot : m_slots) {
        if (slot.gamepad
            && SDL_GetBooleanProperty(SDL_GetGamepadProperties(slot.gamepad),
                                      SDL_PROP_GAMEPAD_CAP_RUMBLE_BOOLEAN, false))
            SDL_RumbleGamepad(slot.gamepad, 0, 0, 0);
    }
}

void ControllerInput::setInputSuspended(bool suspended)
{
    if (m_inputSuspended == suspended) return;
    if (suspended) {
        stopRumble();
        publishConnectedGamepads(true);
        for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
            releaseShellButtons(slot);
    }
    m_inputSuspended = suspended;
    resetDirections();
    if (!suspended && !m_shellCaptureEnabled) {
        for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
            updateSlotSnapshot(slot);
        publishConnectedGamepads();
    }
    updatePollInterval();
    emit inputSuspendedChanged();
}

void ControllerInput::setShellCaptureEnabled(bool enabled)
{
    if (m_shellCaptureEnabled == enabled) return;
    if (enabled) {
        stopRumble();
        publishConnectedGamepads(true);
    } else {
        for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
            releaseShellButtons(slot);
    }
    m_shellCaptureEnabled = enabled;
    if (!enabled) {
        for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
            updateSlotSnapshot(slot);
        publishConnectedGamepads();
        m_lastGamepadSnapshotAt = m_clock.elapsed();
    } else {
        resetDirections();
    }
    updatePollInterval();
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
        refreshControllerMetadata();
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
    emit controllerCountChanged(controllerCount());
    updatePollInterval();
    refreshControllerMetadata();
}

void ControllerInput::closeController(SDL_JoystickID id)
{
    if (!m_gamepadSlots.contains(id)) return;
    const auto slotIndex = m_gamepadSlots.take(id);
    if (slotIndex < 0 || slotIndex >= static_cast<int>(m_slots.size())) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad) return;
    const auto accepted = acceptsController(id);
    releaseShellButtons(slotIndex);
    SDL_CloseGamepad(slot.gamepad);
    slot = {};
    if (accepted)
        emit gamepadSnapshot(m_inputControllerId ? 0 : static_cast<quint8>(slotIndex),
                             gamepadBitmap(), 0, 0, 0, 0, 0, 0, 0);
    if (m_inputControllerId == id) {
        setInputControllerId(0);
        return;
    }
    emit controllerCountChanged(controllerCount());
    updatePollInterval();
    refreshControllerMetadata();
}

void ControllerInput::handleButton(const SDL_GamepadButtonEvent &event, bool pressed)
{
    const auto slotIndex = m_gamepadSlots.value(event.which, -1);
    if (slotIndex < 0 || !acceptsController(event.which)) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (event.button == SDL_GAMEPAD_BUTTON_GUIDE) {
        if (pressed && !m_shellCaptureEnabled && !m_inputSuspended)
            emit localActionRequested(guideLocalAction);
    } else if (const auto mask = buttonMask(event.button); mask != 0) {
        if (pressed) slot.buttons |= mask;
        else slot.buttons &= static_cast<quint16>(~mask);
        if (!m_shellCaptureEnabled) publishGamepad(slotIndex);
    }

    if (!m_shellCaptureEnabled || m_inputSuspended) return;
    const auto key = keyForButton(event.button);
    if (key != 0) {
        handleShellButton(slotIndex, key, pressed);
        if (pressed) reportActivity(slotIndex, QStringLiteral("button:%1").arg(event.button), 1);
    }
}

void ControllerInput::handleAxis(const SDL_GamepadAxisEvent &event)
{
    const auto slotIndex = m_gamepadSlots.value(event.which, -1);
    if (slotIndex < 0 || !acceptsController(event.which)) return;
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
    if (!m_shellCaptureEnabled || m_inputSuspended) return;

    const auto value = event.value;
    bool activated = false;
    auto &left = slot.directions[0];
    auto &right = slot.directions[1];
    auto &up = slot.directions[2];
    auto &down = slot.directions[3];
    if (event.axis == SDL_GAMEPAD_AXIS_LEFTX) {
        activated |= updateDirection(left, value < (left.active ? -axisReleaseThreshold : -axisPressThreshold));
        activated |= updateDirection(right, value > (right.active ? axisReleaseThreshold : axisPressThreshold));
    } else if (event.axis == SDL_GAMEPAD_AXIS_LEFTY) {
        activated |= updateDirection(up, value < (up.active ? -axisReleaseThreshold : -axisPressThreshold));
        activated |= updateDirection(down, value > (down.active ? axisReleaseThreshold : axisPressThreshold));
    }
    if (activated) reportActivity(slotIndex, QStringLiteral("axis:%1").arg(event.axis), value);
}

quint16 ControllerInput::gamepadBitmap() const
{
    if (m_inputControllerId) return m_gamepadSlots.contains(m_inputControllerId) ? 0x0101 : 0;
    quint16 bitmap = 0;
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot) {
        if (m_slots[static_cast<std::size_t>(slot)].gamepad)
            bitmap |= static_cast<quint16>((1u << slot) | (1u << (slot + 8)));
    }
    return bitmap;
}

void ControllerInput::publishGamepad(int slotIndex, bool neutral)
{
    if (m_inputSuspended && !neutral) return;
    const auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad || !acceptsController(slot.instanceId)) return;
    const auto left = neutral ? QPair<qint16, qint16>{}
        : radialDeadzone(slot.rawLeftX, slot.rawLeftY, m_leftStickDeadzone);
    const auto right = neutral ? QPair<qint16, qint16>{}
        : radialDeadzone(slot.rawRightX, slot.rawRightY, m_rightStickDeadzone);
    emit gamepadSnapshot(m_inputControllerId ? 0 : static_cast<quint8>(slotIndex), gamepadBitmap(),
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

QPair<qint16, qint16> ControllerInput::radialDeadzone(qint16 x, qint16 y, int percent)
{
    const auto deadzone = percent / 100.0;
    const auto normalizedX = std::clamp(static_cast<double>(x) / 32767.0, -1.0, 1.0);
    const auto normalizedY = std::clamp(static_cast<double>(y) / 32767.0, -1.0, 1.0);
    const auto magnitude = std::hypot(normalizedX, normalizedY);
    if (magnitude <= deadzone) return {};
    const auto scaled = (magnitude - deadzone) / (1.0 - deadzone);
    const auto factor = scaled / magnitude;
    return {static_cast<qint16>(std::round(std::clamp(normalizedX * factor, -1.0, 1.0) * 32767.0)),
            static_cast<qint16>(std::round(std::clamp(normalizedY * factor, -1.0, 1.0) * 32767.0))};
}

quint8 ControllerInput::triggerValue(qint16 value)
{
    return static_cast<quint8>(std::round(
        std::clamp(static_cast<double>(value) / 32767.0, 0.0, 1.0) * 255.0));
}

void ControllerInput::reportActivity(int slotIndex, const QString &control, int value)
{
    const auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    const auto *name = SDL_GetGamepadName(slot.gamepad);
    const auto device = QStringLiteral("slot:%1 id:%2 %3")
        .arg(slotIndex + 1).arg(slot.instanceId)
        .arg(name ? QString::fromUtf8(name).left(128) : QStringLiteral("unknown"));
    emit controllerActivity();
    emit controllerActivityDetailed(device, control, value);
}

bool ControllerInput::updateDirection(RepeatingDirection &direction, bool active)
{
    if (direction.active == active) return false;
    direction.active = active;
    if (active) {
        direction.pressedAt = m_clock.elapsed();
        direction.repeatedAt = direction.pressedAt;
        postKey(direction.key, true);
        postKey(direction.key, false);
    }
    return active;
}

void ControllerInput::dispatchRepeats(qint64 now)
{
    if (!m_shellCaptureEnabled || m_inputSuspended) return;
    for (auto &slot : m_slots) {
        if (!slot.gamepad || !acceptsController(slot.instanceId)) continue;
        for (auto &direction : slot.directions) {
            if (!direction.active || now - direction.pressedAt < repeatDelayMs
                || now - direction.repeatedAt < repeatIntervalMs) continue;
            direction.repeatedAt = now;
            postKey(direction.key, true, true);
            postKey(direction.key, false, true);
        }
    }
}

void ControllerInput::resetDirections()
{
    for (auto &slot : m_slots) {
        for (auto &direction : slot.directions)
            direction = RepeatingDirection{false, 0, 0, direction.key};
    }
}

void ControllerInput::handleShellButton(int slotIndex, int key, bool pressed)
{
    auto &keys = m_slots[static_cast<std::size_t>(slotIndex)].shellKeys;
    if (keys.contains(key) == pressed) return;
    QPointer<QObject> target = pressed ? QPointer<QObject>(QGuiApplication::focusObject()) : keys.take(key);
    if (pressed && !target) target = QCoreApplication::instance();
    for (const auto &slot : m_slots) {
        if (!slot.shellKeys.contains(key)) continue;
        if (pressed) keys.insert(key, slot.shellKeys.value(key));
        return;
    }
    if (pressed) keys.insert(key, target);
    if (target) postKey(key, pressed, false, target);
}

void ControllerInput::releaseShellButtons(int slotIndex)
{
    const auto keys = m_slots[static_cast<std::size_t>(slotIndex)].shellKeys.keys();
    for (const auto key : keys) handleShellButton(slotIndex, key, false);
}

void ControllerInput::postKey(int key, bool pressed, bool autoRepeat, QObject *target)
{
    if (!m_shellCaptureEnabled || m_inputSuspended) return;
    if (!target) target = QGuiApplication::focusObject();
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
