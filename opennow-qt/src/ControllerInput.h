#pragma once

#include <QObject>
#include <QElapsedTimer>
#include <QHash>
#include <QTimer>
#include <QVariantList>

#include <array>

#include <SDL3/SDL.h>

class ControllerInput final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int controllerCount READ controllerCount NOTIFY controllerCountChanged)
    Q_PROPERTY(QVariantList controllers READ controllers NOTIFY controllersChanged)
    Q_PROPERTY(bool shellCaptureEnabled READ shellCaptureEnabled WRITE setShellCaptureEnabled NOTIFY shellCaptureEnabledChanged)

public:
    static constexpr quint32 syntheticControllerScanCode = 0x4f504e57;

    explicit ControllerInput(QObject *parent = nullptr);
    ~ControllerInput() override;

    [[nodiscard]] int controllerCount() const;
    [[nodiscard]] QVariantList controllers() const;
    [[nodiscard]] bool shellCaptureEnabled() const;
    void setShellCaptureEnabled(bool enabled);

signals:
    void controllerCountChanged(int count);
    void controllersChanged();
    void shellCaptureEnabledChanged();
    void controllerActivity();
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
    };

    void openController(SDL_JoystickID id);
    void closeController(SDL_JoystickID id);
    void handleButton(const SDL_GamepadButtonEvent &event, bool pressed);
    void handleAxis(const SDL_GamepadAxisEvent &event);
    void updateDirection(RepeatingDirection &direction, bool active);
    void dispatchRepeats(qint64 now);
    void postKey(int key, bool pressed, bool autoRepeat = false);
    static int keyForButton(Uint8 button);
    [[nodiscard]] quint16 gamepadBitmap() const;
    void publishGamepad(int slot, bool neutral = false);
    void publishConnectedGamepads(bool neutral = false);
    void updateSlotSnapshot(int slot);
    static quint16 buttonMask(Uint8 button);
    static QPair<qint16, qint16> radialDeadzone(qint16 x, qint16 y);
    static quint8 triggerValue(qint16 value);

    QTimer m_pollTimer;
    QElapsedTimer m_clock;
    QHash<SDL_JoystickID, int> m_gamepadSlots;
    std::array<GamepadSlot, 4> m_slots;
    bool m_sdlReady = false;
    bool m_shellCaptureEnabled = true;
    qint64 m_lastControllerMetadataAt = 0;
    qint64 m_lastGamepadSnapshotAt = 0;
    RepeatingDirection m_left{false, 0, 0, Qt::Key_Left};
    RepeatingDirection m_right{false, 0, 0, Qt::Key_Right};
    RepeatingDirection m_up{false, 0, 0, Qt::Key_Up};
    RepeatingDirection m_down{false, 0, 0, Qt::Key_Down};
};
