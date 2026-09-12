#pragma once

#include <QObject>
#include <QElapsedTimer>
#include <QHash>
#include <QPointer>
#include <QTimer>
#include <QVariantList>

#include <array>

#include <SDL3/SDL.h>

class ControllerInput final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int controllerCount READ controllerCount NOTIFY controllerCountChanged)
    Q_PROPERTY(QVariantList controllers READ controllers NOTIFY controllersChanged)
    Q_PROPERTY(QVariantList availableControllers READ availableControllers NOTIFY availableControllersChanged)
    Q_PROPERTY(quint32 inputControllerId READ inputControllerId WRITE setInputControllerId NOTIFY inputControllerIdChanged)
    Q_PROPERTY(bool shellCaptureEnabled READ shellCaptureEnabled WRITE setShellCaptureEnabled NOTIFY shellCaptureEnabledChanged)
    Q_PROPERTY(bool inputSuspended READ inputSuspended WRITE setInputSuspended NOTIFY inputSuspendedChanged)
    Q_PROPERTY(int leftStickDeadzone READ leftStickDeadzone WRITE setLeftStickDeadzone NOTIFY leftStickDeadzoneChanged)
    Q_PROPERTY(int rightStickDeadzone READ rightStickDeadzone WRITE setRightStickDeadzone NOTIFY rightStickDeadzoneChanged)
    Q_PROPERTY(int vibrationIntensity READ vibrationIntensity WRITE setVibrationIntensity NOTIFY vibrationIntensityChanged)

public:
    static constexpr quint32 syntheticControllerScanCode = 0x4f504e57;

    explicit ControllerInput(QObject *parent = nullptr);
    ~ControllerInput() override;

    [[nodiscard]] int controllerCount() const;
    [[nodiscard]] QVariantList controllers() const;
    [[nodiscard]] QVariantList availableControllers() const;
    [[nodiscard]] quint32 inputControllerId() const;
    void setInputControllerId(quint32 id);
    [[nodiscard]] bool shellCaptureEnabled() const;
    void setShellCaptureEnabled(bool enabled);
    [[nodiscard]] bool inputSuspended() const;
    void setInputSuspended(bool suspended);
    [[nodiscard]] int leftStickDeadzone() const;
    [[nodiscard]] int rightStickDeadzone() const;
    [[nodiscard]] int vibrationIntensity() const;
    void setLeftStickDeadzone(int percent);
    void setRightStickDeadzone(int percent);
    void setVibrationIntensity(int percent);
    void playRumble(quint8 controllerId, quint16 lowFrequency, quint16 highFrequency, quint32 durationMs);
    void stopRumble();

signals:
    void controllerCountChanged(int count);
    void controllersChanged();
    void availableControllersChanged();
    void inputControllerIdChanged();
    void shellCaptureEnabledChanged();
    void inputSuspendedChanged();
    void leftStickDeadzoneChanged();
    void rightStickDeadzoneChanged();
    void vibrationIntensityChanged();
    void controllerActivity();
    void controllerActivityDetailed(const QString &device, const QString &control, int value);
    void gamepadSnapshot(quint8 controllerId, quint16 bitmap, quint16 buttons,
                         quint8 leftTrigger, quint8 rightTrigger,
                         qint16 leftStickX, qint16 leftStickY,
                         qint16 rightStickX, qint16 rightStickY);
    void localActionRequested(quint32 action);

private slots:
    void poll();

private:
    struct RepeatingDirection {
        bool active = false;
        qint64 pressedAt = 0;
        qint64 repeatedAt = 0;
        int key = 0;
    };

    struct GamepadSlot {
        SDL_Gamepad *gamepad = nullptr;
        SDL_JoystickID instanceId = 0;
        quint16 buttons = 0;
        qint16 rawLeftX = 0;
        qint16 rawLeftY = 0;
        qint16 rawRightX = 0;
        qint16 rawRightY = 0;
        quint8 leftTrigger = 0;
        quint8 rightTrigger = 0;
        bool rumbleFailureReported = false;
        QHash<int, QPointer<QObject>> shellKeys;
        std::array<RepeatingDirection, 4> directions{{
            {false, 0, 0, Qt::Key_Left}, {false, 0, 0, Qt::Key_Right},
            {false, 0, 0, Qt::Key_Up}, {false, 0, 0, Qt::Key_Down}}};
    };

    void openController(SDL_JoystickID id);
    void closeController(SDL_JoystickID id);
    void handleButton(const SDL_GamepadButtonEvent &event, bool pressed);
    void handleAxis(const SDL_GamepadAxisEvent &event);
    bool updateDirection(RepeatingDirection &direction, bool active);
    void reportActivity(int slot, const QString &control, int value);
    void dispatchRepeats(qint64 now);
    void resetDirections();
    void handleShellButton(int slot, int key, bool pressed);
    void releaseShellButtons(int slot);
    [[nodiscard]] bool acceptsController(SDL_JoystickID id) const;
    void postKey(int key, bool pressed, bool autoRepeat = false, QObject *target = nullptr);
    static int keyForButton(Uint8 button);
    [[nodiscard]] quint16 gamepadBitmap() const;
    void publishGamepad(int slot, bool neutral = false);
    void publishConnectedGamepads(bool neutral = false);
    void updateSlotSnapshot(int slot);
    void updatePollInterval();
    void refreshControllerMetadata();
    static quint16 buttonMask(Uint8 button);
    static QPair<qint16, qint16> radialDeadzone(qint16 x, qint16 y, int percent);
    static quint8 triggerValue(qint16 value);

    QTimer m_pollTimer;
    QVariantList m_controllerMetadata;
    QVariantList m_availableControllerMetadata;
    SDL_JoystickID m_inputControllerId = 0;
    QElapsedTimer m_clock;
    QHash<SDL_JoystickID, int> m_gamepadSlots;
    std::array<GamepadSlot, 4> m_slots;
    bool m_sdlReady = false;
    bool m_shellCaptureEnabled = true;
    bool m_inputSuspended = false;
    int m_leftStickDeadzone = 5;
    int m_rightStickDeadzone = 5;
    int m_vibrationIntensity = 100;
    qint64 m_lastControllerMetadataAt = 0;
    qint64 m_lastGamepadSnapshotAt = 0;
};
