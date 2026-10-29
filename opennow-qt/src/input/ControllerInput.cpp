#include "input/ControllerInput.h"
#include "input/GenericHidInput.h"

#include <QCoreApplication>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QSet>
#include <QVariantMap>
#include <QWindow>

#include <algorithm>
#include <cmath>
#include <utility>

namespace {
constexpr Sint16 axisPressThreshold = 18000;
constexpr Sint16 axisReleaseThreshold = 12000;
constexpr qint64 repeatDelayMs = 280;
constexpr qint64 repeatIntervalMs = 85;
constexpr qint64 gamepadKeepaliveMs = 100;
constexpr quint32 guideLocalAction = 1;
constexpr quint16 sessionChordButtons = 0x0030;
constexpr qint64 sessionChordWindowMs = 80;
constexpr quint32 testRumbleDurationMs = 280;
constexpr int maximumSerialChars = 120;

int profileField(const QVariantMap &profile, QLatin1StringView name, int fallback, int maximum)
{
    const auto value = profile.value(name);
    bool ok = false;
    const auto number = value.toDouble(&ok);
    return ok && std::isfinite(number) ? std::clamp(static_cast<int>(number), 0, maximum) : fallback;
}
}

ControllerInput::ControllerInput(QObject *parent, std::shared_ptr<GenericHidBackend> hidBackend)
    : QObject(parent), m_pollTimer(this)
{
    SDL_SetHint(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, "1");
    SDL_SetHint(SDL_HINT_HIDAPI_ENUMERATE_ONLY_CONTROLLERS, "0");
    qRegisterMetaType<ControllerInput::SonySnapshot>();
    qRegisterMetaType<GenericHidReport>();
    m_sdlReady = SDL_InitSubSystem(SDL_INIT_GAMEPAD);
    if (!m_sdlReady) {
        qWarning("SDL gamepad initialization failed: %s", SDL_GetError());
    }
    m_genericHid = std::make_unique<GenericHidInput>(std::move(hidBackend));
    connect(m_genericHid.get(), &GenericHidInput::inventoryChanged, this, [this] {
        updatePollInterval();
        poll();
    }, Qt::QueuedConnection);

    int count = 0;
    if (auto *ids = m_sdlReady ? SDL_GetGamepads(&count) : nullptr) {
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
    m_genericHid->stop();
    emit genericDeviceClaimsChanged();
    m_genericHid.reset();
    stopRumble();
    publishConnectedInputs(true);
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

QVariantList ControllerInput::supportedFlightDevices() const
{
    return GenericHidInput::supportedDevices();
}

void ControllerInput::setForwardFlightControls(bool enabled)
{
    if (m_forwardFlightControls == enabled) return;
    m_forwardFlightControls = enabled;
    m_genericHid->setForwardingEnabled(enabled);
    m_genericHidAdmissions.clear();
    emit genericDeviceClaimsChanged();
    if (enabled) {
        QList<SDL_JoystickID> closing;
        for (const auto &slot : m_slots)
            if (slot.gamepad && GenericHidInput::allowed(slot.vendor, slot.product)) closing.append(slot.instanceId);
        for (const auto id : closing) closeController(id);
    } else if (m_sdlReady) {
        int count = 0;
        if (auto *ids = SDL_GetGamepads(&count)) {
            for (int index = 0; index < count; ++index) openController(ids[index]);
            SDL_free(ids);
        }
    }
    refreshFlightDevices();
    updatePollInterval();
    emit forwardFlightControlsChanged();
}

void ControllerInput::setGenericHidAdmission(quint8 slot, quint64 incarnation, GenericHidAdmission admission)
{
    const auto claims = genericHidClaims();
    const auto claim = std::find_if(claims.cbegin(), claims.cend(), [&](const auto &entry) {
        return entry.slot == slot && entry.incarnation == incarnation;
    });
    if (!incarnation || claim == claims.cend()) return;
    if (m_genericHidAdmissions.value(incarnation, GenericHidAdmission::Pending) == admission) return;
    m_genericHidAdmissions.insert(incarnation, admission);
    if (admission != GenericHidAdmission::Pending) {
        emit inputDiagnostic(QStringLiteral("generic-hid %1 slot=%2 %3 by server")
            .arg(GenericHidInput::deviceMetadata(claim->vendor, claim->product).value(QStringLiteral("vidPid")).toString())
            .arg(slot)
            .arg(admission == GenericHidAdmission::Admitted ? QStringLiteral("admitted") : QStringLiteral("declined")));
    }
    refreshFlightDevices();
}

void ControllerInput::resetGenericHidAdmissions()
{
    if (m_genericHidAdmissions.isEmpty()) return;
    m_genericHidAdmissions.clear();
    refreshFlightDevices();
}

void ControllerInput::refreshFlightDevices()
{
    const auto devices = m_genericHid ? m_genericHid->devices() : QList<GenericHidInput::DetectedDevice>{};
    const auto claims = genericHidClaims();
    for (auto it = m_genericHidAdmissions.begin(); it != m_genericHidAdmissions.end();) {
        if (std::none_of(claims.cbegin(), claims.cend(), [&](const auto &claim) { return claim.incarnation == it.key(); }))
            it = m_genericHidAdmissions.erase(it);
        else ++it;
    }
    QVariantList result;
    QSet<SDL_JoystickID> matched;
    QSet<SDL_JoystickID> noHidPath;
    QHash<SDL_JoystickID, quint64> firstSeenScan;
    m_hidPathCheckPending = false;
    int count = 0;
    auto *ids = m_sdlReady ? SDL_GetJoysticks(&count) : nullptr;
    QHash<SDL_JoystickID, QByteArray> physicalKeys;
    for (int index = 0; index < count; ++index) {
        const auto id = ids[index];
        physicalKeys.insert(id, GenericHidInput::physicalDeviceKey(SDL_GetJoystickVendorForID(id),
            SDL_GetJoystickProductForID(id), SDL_GetJoystickPathForID(id)));
    }
    for (const auto &device : devices) {
        auto metadata = GenericHidInput::deviceMetadata(device.vendor, device.product);
        metadata.remove(QStringLiteral("evidence"));
        metadata.remove(QStringLiteral("uncertainty"));
        metadata.insert(QStringLiteral("deviceId"), device.deviceId);
        QString status = m_forwardFlightControls ? QStringLiteral("ready") : QStringLiteral("ignored");
        int player = 0;
        if (m_forwardFlightControls && device.noAccess) status = QStringLiteral("no-access");
        if (m_forwardFlightControls && device.incarnation) {
            switch (m_genericHidAdmissions.value(device.incarnation, GenericHidAdmission::Pending)) {
            case GenericHidAdmission::Pending: break;
            case GenericHidAdmission::Admitted: status = QStringLiteral("active"); break;
            case GenericHidAdmission::Declined: status = QStringLiteral("declined"); break;
            }
        }
        QList<SDL_JoystickID> candidates;
        for (int index = 0; index < count; ++index) {
            const auto id = ids[index];
            if (matched.contains(id) || SDL_GetJoystickVendorForID(id) != device.vendor
                || SDL_GetJoystickProductForID(id) != device.product) continue;
            auto *joystick = SDL_GetJoystickFromID(id);
            const auto *serial = joystick ? SDL_GetJoystickSerial(joystick) : nullptr;
            const auto samePhysicalDevice = !device.key.isEmpty() && physicalKeys.value(id) == device.key;
            const auto sameSerial = !device.serial.isEmpty() && serial && QString::fromUtf8(serial) == device.serial;
            if (!samePhysicalDevice && !sameSerial) continue;
            candidates.append(id);
        }
        const auto matchingDevices = std::count_if(devices.cbegin(), devices.cend(), [&](const auto &entry) {
            return entry.vendor == device.vendor && entry.product == device.product
                && ((!entry.key.isEmpty() && entry.key == device.key)
                    || (!device.serial.isEmpty() && entry.serial == device.serial));
        });
        if (candidates.size() == 1 && matchingDevices == 1) {
            const auto id = candidates.first();
            matched.insert(id);
            if (!m_forwardFlightControls && m_gamepadSlots.contains(id)) {
                status = QStringLiteral("controller");
                player = m_gamepadSlots.value(id) + 1;
            }
        }
        metadata.insert(QStringLiteral("status"), status);
        metadata.insert(QStringLiteral("player"), player);
        result.append(metadata);
    }
    for (int index = 0; index < count; ++index) {
        const auto id = ids[index];
        if (matched.contains(id)) continue;
        auto metadata = GenericHidInput::deviceMetadata(SDL_GetJoystickVendorForID(id), SDL_GetJoystickProductForID(id));
        if (metadata.isEmpty()) continue;
        metadata.remove(QStringLiteral("evidence"));
        metadata.remove(QStringLiteral("uncertainty"));
        metadata.insert(QStringLiteral("deviceId"), QStringLiteral("sdl:%1").arg(id));
        const auto open = !m_forwardFlightControls && m_gamepadSlots.contains(id);
        const auto vendor = SDL_GetJoystickVendorForID(id);
        const auto product = SDL_GetJoystickProductForID(id);
        const auto seenAt = m_flightFirstSeenScan.value(id, m_hidScanGeneration);
        firstSeenScan.insert(id, seenAt);
        const auto noHidSource = m_forwardFlightControls && m_genericHid
            && std::none_of(devices.cbegin(), devices.cend(), [&](const auto &device) {
                return device.vendor == vendor && device.product == product;
            });
        const auto hidSourceMissing = noHidSource && m_hidScanGeneration >= seenAt + 2;
        if (noHidSource && !hidSourceMissing) m_hidPathCheckPending = true;
        if (hidSourceMissing) noHidPath.insert(id);
        metadata.insert(QStringLiteral("status"), hidSourceMissing ? QStringLiteral("no-hid-path")
            : m_forwardFlightControls ? QStringLiteral("ready")
            : open ? QStringLiteral("controller") : QStringLiteral("ignored"));
        metadata.insert(QStringLiteral("player"), open ? m_gamepadSlots.value(id) + 1 : 0);
        result.append(metadata);
    }
    SDL_free(ids);
    for (const auto id : std::as_const(noHidPath)) {
        if (m_noHidPathDevices.contains(id)) continue;
        const auto *path = SDL_GetJoystickPathForID(id);
        emit inputDiagnostic(QStringLiteral("generic-hid %1 has no hidraw node (SDL path %2); not forwarded and kept out of the gamepad path")
            .arg(GenericHidInput::deviceMetadata(SDL_GetJoystickVendorForID(id), SDL_GetJoystickProductForID(id)).value(QStringLiteral("vidPid")).toString(),
                path ? QString::fromLocal8Bit(path) : QStringLiteral("unknown")));
    }
    m_noHidPathDevices = std::move(noHidPath);
    m_flightFirstSeenScan = std::move(firstSeenScan);
    std::sort(result.begin(), result.end(), [](const auto &a, const auto &b) {
        return a.toMap().value(QStringLiteral("deviceId")).toString() < b.toMap().value(QStringLiteral("deviceId")).toString();
    });
    if (result == m_flightDevices) return;
    m_flightDevices = std::move(result);
    emit flightDevicesChanged();
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
    publishConnectedInputs(true);
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
        releaseShellButtons(slot);
    resetDirections();
    m_inputControllerId = id;
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
        updateSlotSnapshot(slot);
    emit controllerCountChanged(controllerCount());
    emit deviceClaimsChanged();
    if (!m_shellCaptureEnabled) publishConnectedInputs();
    refreshControllerMetadata();
    updatePollInterval();
    emit inputControllerIdChanged();
}

void ControllerInput::updatePollInterval()
{
    const auto interval = m_genericHid && !m_genericHid->claims().isEmpty() ? 4
        : controllerCount() == 0 || m_inputSuspended ? 100 : m_shellCaptureEnabled ? 16 : 4;
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
            {QStringLiteral("profileKey"), slot.profileKey},
            {QStringLiteral("hasSerial"), slot.hasSerial},
            {QStringLiteral("leftStickDeadzone"), slot.leftDeadzone},
            {QStringLiteral("rightStickDeadzone"), slot.rightDeadzone},
            {QStringLiteral("vibrationIntensity"), slot.vibration},
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
    refreshFlightDevices();
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
    applySlotTuning();
    emit leftStickDeadzoneChanged();
}

void ControllerInput::setRightStickDeadzone(int percent)
{
    percent = std::clamp(percent, 0, 50);
    if (m_rightStickDeadzone == percent) return;
    m_rightStickDeadzone = percent;
    applySlotTuning();
    emit rightStickDeadzoneChanged();
}

void ControllerInput::setVibrationIntensity(int percent)
{
    percent = std::clamp(percent, 0, 100);
    if (m_vibrationIntensity == percent) return;
    m_vibrationIntensity = percent;
    applySlotTuning();
    emit vibrationIntensityChanged();
}

void ControllerInput::setControllerProfiles(const QVariantMap &profiles)
{
    if (m_controllerProfiles == profiles) return;
    m_controllerProfiles = profiles;
    applySlotTuning();
    emit controllerProfilesChanged();
}

void ControllerInput::applySlotTuning()
{
    bool sticksChanged = false;
    bool vibrationChanged = false;
    for (auto &slot : m_slots) {
        if (!slot.gamepad) continue;
        const auto left = slot.leftDeadzone;
        const auto right = slot.rightDeadzone;
        const auto vibration = slot.vibration;
        tuneSlot(slot);
        sticksChanged |= left != slot.leftDeadzone || right != slot.rightDeadzone;
        vibrationChanged |= vibration != slot.vibration;
    }
    if (vibrationChanged) stopRumble();
    if (sticksChanged && !m_shellCaptureEnabled) publishConnectedInputs();
    if (sticksChanged || vibrationChanged) refreshControllerMetadata();
}

void ControllerInput::tuneSlot(GamepadSlot &slot) const
{
    const auto profile = m_controllerProfiles.value(slot.profileKey).toMap();
    slot.leftDeadzone = profileField(profile, QLatin1StringView("leftStickDeadzone"), m_leftStickDeadzone, 50);
    slot.rightDeadzone = profileField(profile, QLatin1StringView("rightStickDeadzone"), m_rightStickDeadzone, 50);
    slot.vibration = profileField(profile, QLatin1StringView("vibrationIntensity"), m_vibrationIntensity, 100);
}

QString ControllerInput::profileKey(quint16 vendor, quint16 product, const char *serial, SDL_GUID guid)
{
    const auto trimmedSerial = serial ? QString::fromUtf8(serial).trimmed() : QString();
    if (!trimmedSerial.isEmpty()) {
        return QStringLiteral("serial:%1:%2:%3")
            .arg(vendor, 4, 16, QLatin1Char('0'))
            .arg(product, 4, 16, QLatin1Char('0'))
            .arg(trimmedSerial.left(maximumSerialChars));
    }
    char text[33] = {};
    SDL_GUIDToString(guid, text, sizeof(text));
    return QStringLiteral("guid:") + QString::fromLatin1(text);
}

void ControllerInput::setLeftStickNavigationEnabled(bool enabled)
{
    if (m_leftStickNavigationEnabled == enabled) return;
    m_leftStickNavigationEnabled = enabled;
    resetDirections();
    emit leftStickNavigationEnabledChanged();
}

QVariantMap ControllerInput::stickSample(quint32 instanceId) const
{
    const auto slotIndex = m_gamepadSlots.value(instanceId, -1);
    if (slotIndex < 0) return {};
    const auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad) return {};
    const auto axis = [&slot](SDL_GamepadAxis which) { return SDL_GetGamepadAxis(slot.gamepad, which); };
    const auto leftX = axis(SDL_GAMEPAD_AXIS_LEFTX);
    const auto leftY = axis(SDL_GAMEPAD_AXIS_LEFTY);
    const auto rightX = axis(SDL_GAMEPAD_AXIS_RIGHTX);
    const auto rightY = axis(SDL_GAMEPAD_AXIS_RIGHTY);
    const auto left = radialDeadzone(leftX, leftY, slot.leftDeadzone);
    const auto right = radialDeadzone(rightX, rightY, slot.rightDeadzone);
    const auto unit = [](qint16 value) { return std::clamp(value / 32767.0, -1.0, 1.0); };
    return {
        {QStringLiteral("leftX"), unit(leftX)}, {QStringLiteral("leftY"), unit(leftY)},
        {QStringLiteral("rightX"), unit(rightX)}, {QStringLiteral("rightY"), unit(rightY)},
        {QStringLiteral("leftOutX"), unit(left.first)}, {QStringLiteral("leftOutY"), unit(left.second)},
        {QStringLiteral("rightOutX"), unit(right.first)}, {QStringLiteral("rightOutY"), unit(right.second)},
    };
}

bool ControllerInput::testRumble(quint32 instanceId)
{
    const auto slotIndex = m_gamepadSlots.value(instanceId, -1);
    if (slotIndex < 0) return false;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad || slot.vibration == 0
        || !SDL_GetBooleanProperty(SDL_GetGamepadProperties(slot.gamepad),
                                   SDL_PROP_GAMEPAD_CAP_RUMBLE_BOOLEAN, false))
        return false;
    return SDL_RumbleGamepad(slot.gamepad, static_cast<quint16>(0x6000 * slot.vibration / 100),
                             static_cast<quint16>(0xA000 * slot.vibration / 100), testRumbleDurationMs);
}

void ControllerInput::playRumble(quint8 controllerId, quint16 lowFrequency,
                               quint16 highFrequency, quint32 durationMs,
                               quint64 sourceIncarnation)
{
    if (m_shellCaptureEnabled || m_inputSuspended) return;
    const auto slotIndex = m_inputControllerId
        ? (controllerId == 0 ? m_gamepadSlots.value(m_inputControllerId, -1) : -1)
        : static_cast<int>(controllerId);
    if (slotIndex < 0 || slotIndex >= static_cast<int>(m_slots.size())) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad || !acceptsController(slot.instanceId) || slot.vibration == 0) return;
    if (sourceIncarnation != 0 && slot.incarnation != sourceIncarnation) return;
    const auto properties = SDL_GetGamepadProperties(slot.gamepad);
    if (!SDL_GetBooleanProperty(properties, SDL_PROP_GAMEPAD_CAP_RUMBLE_BOOLEAN, false)) return;
    if (!SDL_RumbleGamepad(slot.gamepad,
                     static_cast<quint16>(quint32(lowFrequency) * slot.vibration / 100),
                     static_cast<quint16>(quint32(highFrequency) * slot.vibration / 100),
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

void ControllerInput::setShellInputBlocked(bool blocked)
{
    if (m_shellInputBlocked == blocked) return;
    m_shellInputBlocked = blocked;
    if (blocked) {
        for (auto &slot : m_slots) slot.shellKeys.clear();
        resetDirections();
    }
    updateShellInputDrain();
    emit shellInputBlockedChanged();
}

void ControllerInput::updateShellInputDrain()
{
    bool draining = false;
    if (m_shellInputBlocked || m_shellInputDraining) {
        for (const auto &slot : m_slots) {
            if (!slot.gamepad || !acceptsController(slot.instanceId)) continue;
            for (int button = 0; button < SDL_GAMEPAD_BUTTON_COUNT; ++button)
                draining |= SDL_GetGamepadButton(slot.gamepad, SDL_GamepadButton(button));
            for (int axis = 0; axis < SDL_GAMEPAD_AXIS_COUNT; ++axis)
                draining |= std::abs(int(SDL_GetGamepadAxis(slot.gamepad, SDL_GamepadAxis(axis))))
                    > axisReleaseThreshold;
        }
    }
    if (m_shellInputDraining == draining) return;
    m_shellInputDraining = draining;
    emit shellInputDrainingChanged();
}

void ControllerInput::setInputSuspended(bool suspended)
{
    if (m_inputSuspended == suspended) return;
    cancelChordReplays();
    if (suspended) {
        stopRumble();
        publishConnectedInputs(true);
        for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
            releaseShellButtons(slot);
    }
    m_inputSuspended = suspended;
    resetDirections();
    if (!suspended && !m_shellCaptureEnabled) {
        resumeStreamInput();
        publishConnectedInputs();
    }
    updatePollInterval();
    emit inputSuspendedChanged();
}

void ControllerInput::setShellCaptureEnabled(bool enabled)
{
    if (m_shellCaptureEnabled == enabled) return;
    cancelChordReplays();
    if (enabled) {
        stopRumble();
        publishConnectedInputs(true);
    } else {
        for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot)
            releaseShellButtons(slot);
    }
    m_shellCaptureEnabled = enabled;
    if (!enabled) {
        resumeStreamInput();
        publishConnectedInputs();
        m_lastGamepadSnapshotAt = m_clock.elapsed();
    } else {
        resetDirections();
    }
    updatePollInterval();
    emit shellCaptureEnabledChanged();
}

void ControllerInput::cancelChordReplays()
{
    for (auto &slot : m_slots) {
        slot.chordDeadline = 0;
        slot.chordStartedNs = 0;
        slot.tappedButtons = 0;
        slot.tapReleaseAt = 0;
    }
}

void ControllerInput::resumeStreamInput()
{
    if (m_sdlReady) SDL_UpdateGamepads();
    m_streamInputStartedNs = SDL_GetTicksNS();
    for (int index = 0; index < static_cast<int>(m_slots.size()); ++index) {
        updateSlotSnapshot(index);
        auto &slot = m_slots[static_cast<std::size_t>(index)];
        if (!slot.gamepad) continue;
        slot.withheldButtons = slot.buttons & sessionChordButtons;
        slot.guideLatched = SDL_GetGamepadButton(slot.gamepad, SDL_GAMEPAD_BUTTON_GUIDE);
        slot.touchpadClick = SDL_GetGamepadButton(slot.gamepad, SDL_GAMEPAD_BUTTON_TOUCHPAD);
    }
}

void ControllerInput::poll()
{
    if (m_genericHid) {
        auto batch = m_genericHid->drain();
        const auto scanAdvanced = std::exchange(m_hidScanGeneration, batch.scanGeneration) != batch.scanGeneration;
        if (scanAdvanced && m_hidPathCheckPending && !batch.inventoryChanged) refreshFlightDevices();
        for (const auto &message : std::as_const(batch.diagnostics)) emit inputDiagnostic(message);
        if (batch.inventoryChanged) {
            updatePollInterval();
            emit genericDeviceClaimsChanged();
            refreshFlightDevices();
        }
        if (m_forwardFlightControls) for (const auto &report : batch.reports) emit genericHidReport(report);
    }
    if (!m_sdlReady) return;
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
        case SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN:
        case SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION:
        case SDL_EVENT_GAMEPAD_TOUCHPAD_UP:
            handleTouchpad(event.gtouchpad);
            break;
        default:
            break;
        }
        if (event.type == SDL_EVENT_JOYSTICK_ADDED || event.type == SDL_EVENT_JOYSTICK_REMOVED
            || event.type == SDL_EVENT_GAMEPAD_ADDED || event.type == SDL_EVENT_GAMEPAD_REMOVED) refreshFlightDevices();
    }

    const auto now = m_clock.elapsed();
    dispatchRepeats(now);
    for (int index = 0; index < static_cast<int>(m_slots.size()); ++index) {
        auto &slot = m_slots[static_cast<std::size_t>(index)];
        bool changed = false;
        if (slot.chordDeadline != 0 && now >= slot.chordDeadline) {
            slot.chordDeadline = 0;
            slot.withheldButtons &= static_cast<quint16>(~sessionChordButtons);
            changed = true;
        }
        if (slot.tappedButtons != 0 && now >= slot.tapReleaseAt) {
            slot.tappedButtons = 0;
            changed = true;
        }
        if (changed && slot.gamepad && !m_shellCaptureEnabled) publishSlotSnapshot(index);
    }
    if (!m_shellCaptureEnabled && now - m_lastGamepadSnapshotAt >= gamepadKeepaliveMs) {
        publishConnectedInputs();
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
    if (m_forwardFlightControls && GenericHidInput::allowed(SDL_GetGamepadVendorForID(id), SDL_GetGamepadProductForID(id))) return;
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
    freeSlot->incarnation = m_nextIncarnation++;
    freeSlot->vendor = SDL_GetGamepadVendor(gamepad);
    freeSlot->product = SDL_GetGamepadProduct(gamepad);
    const auto *serial = SDL_GetGamepadSerial(gamepad);
    freeSlot->hasSerial = serial && !QString::fromUtf8(serial).trimmed().isEmpty();
    freeSlot->profileKey = profileKey(freeSlot->vendor, freeSlot->product, serial, SDL_GetGamepadGUIDForID(id));
    tuneSlot(*freeSlot);
    if (isSonySlot(slot)) {
        for (auto &contact : freeSlot->contacts) {
            contact.x = sonyContactCenter;
            contact.y = sonyContactCenter;
        }
    }
    m_gamepadSlots.insert(id, slot);
    updateSlotSnapshot(slot);
    if (!m_shellCaptureEnabled) publishSlotSnapshot(slot);
    emit deviceClaimsChanged();
    emit controllerCountChanged(controllerCount());
    updatePollInterval();
    refreshControllerMetadata();
}

QList<GenericHidDeviceClaim> ControllerInput::genericHidClaims() const
{
    return m_forwardFlightControls && m_genericHid ? m_genericHid->claims() : QList<GenericHidDeviceClaim>{};
}

void ControllerInput::handleGenericHidOutput(const GenericHidReport &report)
{
    if (m_forwardFlightControls && m_genericHid) m_genericHid->submitOutput(report);
}

void ControllerInput::stopGenericHidOutput()
{
    if (m_genericHid) m_genericHid->clearOutputs();
}

void ControllerInput::closeController(SDL_JoystickID id)
{
    if (!m_gamepadSlots.contains(id)) return;
    const auto slotIndex = m_gamepadSlots.take(id);
    if (slotIndex < 0 || slotIndex >= static_cast<int>(m_slots.size())) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad) return;
    const auto accepted = acceptsController(id);
    const auto publishedSlot = effectiveSlot(slotIndex);
    const auto sony = isSonySlot(slotIndex);
    releaseShellButtons(slotIndex);
    if (sony) releaseSonyContacts(slotIndex);
    SDL_CloseGamepad(slot.gamepad);
    slot = {};
    emit deviceClaimsChanged();
    if (accepted && !sony)
        emit gamepadSnapshot(static_cast<quint8>(publishedSlot), gamepadBitmap(), 0, 0, 0, 0, 0, 0, 0);
    emit controllerCountChanged(controllerCount());
    updatePollInterval();
    refreshControllerMetadata();
}

void ControllerInput::handleButton(const SDL_GamepadButtonEvent &event, bool pressed)
{
    if (!m_shellCaptureEnabled && event.timestamp < m_streamInputStartedNs) return;
    const auto slotIndex = m_gamepadSlots.value(event.which, -1);
    if (slotIndex < 0 || !acceptsController(event.which)) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (event.button == SDL_GAMEPAD_BUTTON_GUIDE) {
        if (pressed) {
            if (slot.guideLatched) return;
            slot.guideLatched = true;
            if (!m_shellCaptureEnabled && !m_inputSuspended)
                emit localActionRequested(guideLocalAction);
        } else {
            slot.guideLatched = false;
        }
    } else if (event.button == SDL_GAMEPAD_BUTTON_TOUCHPAD) {
        slot.touchpadClick = pressed;
        if (isSonySlot(slotIndex)) publishSonySnapshot(slotIndex);
    } else if (const auto mask = buttonMask(event.button); mask != 0) {
        const auto now = m_clock.elapsed();
        const bool chordButton = (mask & sessionChordButtons) != 0;
        if (pressed) {
            if (chordButton && slot.tappedButtons != 0) {
                slot.tappedButtons = 0;
                if (!m_shellCaptureEnabled) publishSlotSnapshot(slotIndex);
            }
            slot.buttons |= mask;
            if (chordButton && slot.chordDeadline != 0
                    && event.timestamp - slot.chordStartedNs
                        <= static_cast<quint64>(sessionChordWindowMs) * 1'000'000
                    && (slot.buttons & sessionChordButtons) == sessionChordButtons) {
                slot.chordDeadline = 0;
                slot.withheldButtons |= sessionChordButtons;
                if (!m_shellCaptureEnabled && !m_inputSuspended)
                    emit localActionRequested(guideLocalAction);
            } else if (chordButton && (slot.buttons & sessionChordButtons) == mask) {
                slot.withheldButtons |= mask;
                slot.chordDeadline = now + sessionChordWindowMs;
                slot.chordStartedNs = event.timestamp;
            }
        } else {
            slot.buttons &= static_cast<quint16>(~mask);
            if (slot.chordDeadline != 0 && (slot.withheldButtons & mask) != 0) {
                slot.chordDeadline = 0;
                if (!m_shellCaptureEnabled && !m_inputSuspended) {
                    slot.tappedButtons = mask;
                    slot.tapReleaseAt = now + sessionChordWindowMs;
                }
            }
            slot.withheldButtons &= static_cast<quint16>(~mask);
        }
        if (!m_shellCaptureEnabled) publishSlotSnapshot(slotIndex);
    }

    if (!m_shellCaptureEnabled || m_inputSuspended) return;
    if (m_shellInputBlocked || m_shellInputDraining) {
        updateShellInputDrain();
        if (pressed && m_shellInputBlocked) emit shellInputSkipRequested();
        return;
    }
    const auto key = keyForButton(event.button);
    if (key != 0) {
        handleShellButton(slotIndex, key, pressed);
        if (pressed) reportActivity(slotIndex, QStringLiteral("button:%1").arg(event.button), 1);
    }
}

void ControllerInput::handleAxis(const SDL_GamepadAxisEvent &event)
{
    if (!m_shellCaptureEnabled && event.timestamp < m_streamInputStartedNs) return;
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
    if (!m_shellCaptureEnabled) publishSlotSnapshot(slotIndex);
    if (!m_shellCaptureEnabled || m_inputSuspended) return;

    if (m_shellInputBlocked || m_shellInputDraining) {
        updateShellInputDrain();
        if (m_shellInputBlocked && std::abs(int(event.value)) > axisPressThreshold)
            emit shellInputSkipRequested();
        return;
    }

    const auto value = event.value;
    bool activated = false;
    auto &left = slot.directions[0];
    auto &right = slot.directions[1];
    auto &up = slot.directions[2];
    auto &down = slot.directions[3];
    if (!m_leftStickNavigationEnabled) return;
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
    quint16 bitmap = 0;
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot) {
        const auto &entry = m_slots[static_cast<std::size_t>(slot)];
        if (!entry.gamepad || !acceptsController(entry.instanceId)) continue;
        const auto published = effectiveSlot(slot);
        bitmap |= static_cast<quint16>((1u << published) | (1u << (published + 8)));
    }
    return bitmap;
}

int ControllerInput::effectiveSlot(int slotIndex) const
{
    if (m_inputControllerId == 0) return slotIndex;
    return m_slots[static_cast<std::size_t>(slotIndex)].instanceId == m_inputControllerId ? 0
                                                                                          : slotIndex;
}

void ControllerInput::publishSlotSnapshot(int slotIndex, bool neutral)
{
    if (isSonySlot(slotIndex)) publishSonySnapshot(slotIndex, neutral);
    else publishGamepad(slotIndex, neutral);
}

void ControllerInput::publishGamepad(int slotIndex, bool neutral)
{
    if (m_inputSuspended && !neutral) return;
    const auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad || !acceptsController(slot.instanceId)) return;
    const auto left = neutral ? QPair<qint16, qint16>{}
        : radialDeadzone(slot.rawLeftX, slot.rawLeftY, slot.leftDeadzone);
    const auto right = neutral ? QPair<qint16, qint16>{}
        : radialDeadzone(slot.rawRightX, slot.rawRightY, slot.rightDeadzone);
    emit gamepadSnapshot(static_cast<quint8>(effectiveSlot(slotIndex)), gamepadBitmap(),
                         neutral ? 0 : slot.forwardedButtons(),
                         neutral ? 0 : slot.leftTrigger, neutral ? 0 : slot.rightTrigger,
                         left.first, static_cast<qint16>(-left.second),
                         right.first, static_cast<qint16>(-right.second));
}

void ControllerInput::publishConnectedGamepads(bool neutral)
{
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot) {
        if (m_slots[static_cast<std::size_t>(slot)].gamepad && !isSonySlot(slot))
            publishGamepad(slot, neutral);
    }
}

bool ControllerInput::isSonySlot(int slotIndex) const
{
    const auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (slot.vendor != 0x054c) return false;
    switch (slot.product) {
    case 0x05c4:
    case 0x09cc:
    case 0x0ba0:
    case 0x0ce6:
    case 0x0df2:
        return true;
    default:
        return false;
    }
}

void ControllerInput::sampleSonyContacts(int slotIndex)
{
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad || !isSonySlot(slotIndex)) return;
    for (int finger = 0; finger < 2; ++finger) {
        bool down = false;
        float x = 0.0f;
        float y = 0.0f;
        float pressure = 0.0f;
        if (!SDL_GetGamepadTouchpadFinger(slot.gamepad, 0, finger, &down, &x, &y, &pressure))
            continue;
        auto &contact = slot.contacts[static_cast<std::size_t>(finger)];
        if (!std::isfinite(x) || !std::isfinite(y)) continue;
        contact.x = x;
        contact.y = y;
        contact.hasPosition = true;
        contact.active = down;
    }
}

void ControllerInput::publishSonySnapshot(int slotIndex, bool neutral)
{
    const auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    if (!slot.gamepad || !isSonySlot(slotIndex) || !acceptsController(slot.instanceId)) return;
    if (m_inputSuspended && !neutral) return;
    if (m_shellCaptureEnabled) return;
    const auto cleared = neutral || m_inputSuspended;
    const auto left = cleared ? QPair<qint16, qint16>{}
        : radialDeadzone(slot.rawLeftX, slot.rawLeftY, slot.leftDeadzone);
    const auto right = cleared ? QPair<qint16, qint16>{}
        : radialDeadzone(slot.rawRightX, slot.rawRightY, slot.rightDeadzone);
    SonySnapshot snapshot;
    snapshot.slot = static_cast<quint8>(effectiveSlot(slotIndex));
    snapshot.incarnation = slot.incarnation;
    snapshot.buttons = cleared ? 0 : slot.forwardedButtons();
    snapshot.leftTrigger = cleared ? 0 : slot.leftTrigger;
    snapshot.rightTrigger = cleared ? 0 : slot.rightTrigger;
    snapshot.leftStickX = left.first;
    snapshot.leftStickY = left.second;
    snapshot.rightStickX = right.first;
    snapshot.rightStickY = right.second;
    snapshot.touchpadClick = !cleared && slot.touchpadClick;
    for (std::size_t finger = 0; finger < snapshot.contacts.size(); ++finger)
        snapshot.contacts[finger] = slot.contacts[finger];
    if (cleared) {
        for (auto &contact : snapshot.contacts) contact.active = false;
    }
    snapshot.observedAtUs = static_cast<quint64>(m_clock.elapsed() * 1000);
    emit sonySnapshot(snapshot);
}

void ControllerInput::publishConnectedSony(bool neutral)
{
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot) {
        if (m_slots[static_cast<std::size_t>(slot)].gamepad && isSonySlot(slot))
            publishSonySnapshot(slot, neutral);
    }
}

void ControllerInput::publishConnectedInputs(bool neutral)
{
    publishConnectedGamepads(neutral);
    publishConnectedSony(neutral);
}

void ControllerInput::releaseSonyContacts(int slotIndex)
{
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    slot.contacts[0].active = false;
    slot.contacts[1].active = false;
    slot.touchpadClick = false;
    publishSonySnapshot(slotIndex, true);
}

void ControllerInput::handleTouchpad(const SDL_GamepadTouchpadEvent &event)
{
    if (!m_shellCaptureEnabled && event.timestamp < m_streamInputStartedNs) return;
    const auto slotIndex = m_gamepadSlots.value(event.which, -1);
    if (slotIndex < 0 || !acceptsController(event.which)) return;
    if (event.touchpad != 0) return;
    if (event.finger < 0 || event.finger > 1) return;
    auto &slot = m_slots[static_cast<std::size_t>(slotIndex)];
    auto &contact = slot.contacts[static_cast<std::size_t>(event.finger)];
    const auto finite = std::isfinite(event.x) && std::isfinite(event.y);
    switch (event.type) {
    case SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN:
    case SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION:
        if (!finite) return;
        contact.x = event.x;
        contact.y = event.y;
        contact.hasPosition = true;
        contact.active = true;
        break;
    case SDL_EVENT_GAMEPAD_TOUCHPAD_UP:
        contact.active = false;
        if (finite) {
            contact.x = event.x;
            contact.y = event.y;
            contact.hasPosition = true;
        }
        break;
    default:
        return;
    }
    if (event.type == SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN && (m_shellCaptureEnabled || m_inputSuspended))
        return;
    publishSonySnapshot(slotIndex);
}

QList<SdlDeviceClaim> ControllerInput::deviceClaims() const
{
    QList<SdlDeviceClaim> claims;
    for (int slot = 0; slot < static_cast<int>(m_slots.size()); ++slot) {
        const auto &entry = m_slots[static_cast<std::size_t>(slot)];
        if (!entry.gamepad || !acceptsController(entry.instanceId)) continue;
        claims.append(SdlDeviceClaim{static_cast<quint8>(effectiveSlot(slot)), entry.incarnation,
                                     entry.vendor, entry.product});
    }
    return claims;
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
    sampleSonyContacts(slotIndex);
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
    updateShellInputDrain();
    if (m_shellInputBlocked || m_shellInputDraining) return;
    if (!m_shellCaptureEnabled || m_inputSuspended) return;
    QSet<int> digitalDirections;
    for (auto &slot : m_slots) {
        if (!slot.gamepad || !acceptsController(slot.instanceId)) continue;
        for (auto held = slot.shellKeys.begin(); held != slot.shellKeys.end(); ++held) {
            const auto key = held.key();
            if (key != Qt::Key_Left && key != Qt::Key_Right && key != Qt::Key_Up && key != Qt::Key_Down)
                continue;
            if (digitalDirections.contains(key)) continue;
            digitalDirections.insert(key);
            if (now - held->pressedAt < repeatDelayMs || now - held->repeatedAt < repeatIntervalMs)
                continue;
            if (held->target) {
                postKey(key, true, true, held->target);
                postKey(key, false, true, held->target);
            }
            for (auto &owner : m_slots) {
                auto shared = owner.shellKeys.find(key);
                if (shared != owner.shellKeys.end()) shared->repeatedAt = now;
            }
        }
    }
    for (auto &slot : m_slots) {
        if (!slot.gamepad || !acceptsController(slot.instanceId)) continue;
        for (auto &direction : slot.directions) {
            if (!direction.active || digitalDirections.contains(direction.key)
                || now - direction.pressedAt < repeatDelayMs
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
    QPointer<QObject> target = pressed ? QPointer<QObject>(QGuiApplication::focusWindow()) : keys.take(key).target;
    if (pressed && !target) target = QCoreApplication::instance();
    for (const auto &slot : m_slots) {
        if (!slot.shellKeys.contains(key)) continue;
        if (pressed) keys.insert(key, slot.shellKeys.value(key));
        return;
    }
    if (pressed) {
        const auto now = m_clock.elapsed();
        keys.insert(key, ShellKeyPress{target, now, now});
    }
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
    if (!target) target = QGuiApplication::focusWindow();
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
