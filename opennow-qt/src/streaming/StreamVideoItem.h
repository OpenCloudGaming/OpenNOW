#pragma once

#include "streaming/PhysicalKeyMap.h"
#include "streaming/rendering/StreamVideoRenderCallback.h"

#include <QAbstractNativeEventFilter>
#include <QQuickItem>
#include <QCursor>
#include <QHash>
#include <QPoint>
#include <QPointer>
#include <QSet>
#include <QRect>
#include <QSize>
#include <QVariantMap>
#include <QTimer>

#include <memory>
#include <optional>

class QHoverEvent;
class NativeStreamRuntime;

class StreamVideoItem : public QQuickItem
{
    Q_OBJECT
    Q_PROPERTY(QSize videoSize READ videoSize WRITE setVideoSize NOTIFY videoSizeChanged)
    Q_PROPERTY(bool renderCallbackAvailable READ renderCallbackAvailable
                   NOTIFY renderCallbackAvailableChanged)
    Q_PROPERTY(bool nativeRuntimeAvailable READ nativeRuntimeAvailable CONSTANT)
    Q_PROPERTY(bool inputEnabled READ inputEnabled WRITE setInputEnabled
                   NOTIFY inputEnabledChanged)
    Q_PROPERTY(bool captureActive READ captureActive NOTIFY captureActiveChanged)
    Q_PROPERTY(bool clipboardPaste READ clipboardPaste WRITE setClipboardPaste
                   NOTIFY clipboardPasteChanged)
    Q_PROPERTY(QString keyboardLayout READ keyboardLayout WRITE setKeyboardLayout
                   NOTIFY keyboardLayoutChanged)
    Q_PROPERTY(QString inputCaptureError READ inputCaptureError NOTIFY inputCaptureErrorChanged)
    Q_PROPERTY(bool relativeMouse READ relativeMouse WRITE setRelativeMouse
                   NOTIFY relativeMouseChanged)
    Q_PROPERTY(QVariantMap shortcutBindings READ shortcutBindings WRITE setShortcutBindings
                   NOTIFY shortcutBindingsChanged)
    Q_PROPERTY(bool frameGeneration READ frameGeneration WRITE setFrameGeneration
                   NOTIFY frameGenerationChanged)
    // 0 = off, 2..4 = displayed frames per source frame. `frameGeneration` mirrors it as
    // a boolean for existing bindings (true selects 2x).
    Q_PROPERTY(int frameGenerationFactor READ frameGenerationFactor WRITE setFrameGenerationFactor
                   NOTIFY frameGenerationChanged)
    // Manual display refresh in Hz for frame-generation pacing; 0 uses the window's screen.
    Q_PROPERTY(int displayRefreshOverrideHz READ displayRefreshOverrideHz WRITE setDisplayRefreshOverrideHz
                   NOTIFY displayRefreshOverrideHzChanged)
    Q_PROPERTY(bool metalFxUpscaling READ metalFxUpscaling WRITE setMetalFxUpscaling
                   NOTIFY metalFxUpscalingChanged)
    Q_PROPERTY(bool fsrUpscaling READ fsrUpscaling WRITE setFsrUpscaling
                   NOTIFY fsrUpscalingChanged)
    Q_PROPERTY(int upscalingSharpness READ upscalingSharpness WRITE setUpscalingSharpness
                   NOTIFY upscalingSharpnessChanged)
    Q_PROPERTY(int upscalingDenoise READ upscalingDenoise WRITE setUpscalingDenoise
                   NOTIFY upscalingDenoiseChanged)
    Q_PROPERTY(QVariantMap frameGenerationStats READ frameGenerationStats
                   NOTIFY frameGenerationStatsChanged)
    Q_PROPERTY(QVariantMap swapStats READ swapStats NOTIFY swapStatsChanged)

public:
    struct RemoteCursorMetadata {
        qsizetype imageOffset = -1;
        qsizetype imageLength = 0;
        std::optional<QPoint> normalizedPosition;
        qreal scale = 1.0;
    };

    explicit StreamVideoItem(QQuickItem *parent = nullptr);
    ~StreamVideoItem() override;

    [[nodiscard]] QSize videoSize() const;
    void setVideoSize(const QSize &size);

    [[nodiscard]] bool renderCallbackAvailable() const;
    [[nodiscard]] bool nativeRuntimeAvailable() const;
    [[nodiscard]] bool inputEnabled() const;
    void setInputEnabled(bool enabled);
    [[nodiscard]] bool captureActive() const;
    [[nodiscard]] bool clipboardPaste() const;
    void setClipboardPaste(bool enabled);
    [[nodiscard]] QString keyboardLayout() const;
    void setKeyboardLayout(const QString &layout);
    [[nodiscard]] QString inputCaptureError() const;
    [[nodiscard]] bool relativeMouse() const;
    void setRelativeMouse(bool relative);
    [[nodiscard]] QVariantMap shortcutBindings() const;
    void setShortcutBindings(const QVariantMap &bindings);
    [[nodiscard]] std::shared_ptr<StreamVideoRenderCallback> renderCallback() const;
    void setRenderCallback(std::shared_ptr<StreamVideoRenderCallback> callback);
    bool frameGeneration() const;
    void setFrameGeneration(bool enabled);
    int frameGenerationFactor() const;
    void setFrameGenerationFactor(int factor);
    int displayRefreshOverrideHz() const;
    void setDisplayRefreshOverrideHz(int hz);
    bool metalFxUpscaling() const;
    void setMetalFxUpscaling(bool enabled);
    bool fsrUpscaling() const;
    void setFsrUpscaling(bool enabled);
    int upscalingSharpness() const;
    void setUpscalingSharpness(int value);
    int upscalingDenoise() const;
    void setUpscalingDenoise(int value);
    QVariantMap frameGenerationStats() const;
    QVariantMap swapStats() const;

    static void setNativeStreamRuntime(NativeStreamRuntime *runtime);
    [[nodiscard]] static NativeStreamRuntime *nativeStreamRuntime();

    Q_INVOKABLE void requestFrame();
    Q_INVOKABLE void resynchronizeInput();
    Q_INVOKABLE void togglePointerLock();

    [[nodiscard]] static QRect aspectFitRect(const QSize &videoSize,
                                              const QSize &targetSize);
    [[nodiscard]] static QRect scaledCaptureRect(const QRectF &itemRect,
                                                 const QSizeF &windowSize,
                                                 const QRect &clientScreenRect);
    [[nodiscard]] static QRect absoluteMouseCoordinates(const QPointF &position,
                                                        const QSize &videoSize,
                                                        const QSizeF &itemSize);
    [[nodiscard]] static RemoteCursorMetadata remoteCursorMetadata(const QByteArray &bytes);
    [[nodiscard]] static QPoint mapRemoteCursorPosition(const QPoint &normalizedPosition,
                                                        const QSize &videoSize,
                                                        const QSizeF &itemSize);
    [[nodiscard]] static quint16 windowsVirtualKey(
        int key, Qt::KeyboardModifiers modifiers = Qt::NoModifier,
        quint32 nativeVirtualKey = 0);
    [[nodiscard]] static quint16 linuxPhysicalVirtualKey(quint32 nativeScanCode);
    [[nodiscard]] static quint16 windowsGameplayVirtualKey(
        int key, Qt::KeyboardModifiers modifiers, quint32 nativeScanCode,
        quint32 nativeVirtualKey);
    [[nodiscard]] static quint16 macGameplayVirtualKey(
        int key, Qt::KeyboardModifiers modifiers, quint32 nativeVirtualKey,
        bool nativeEvent = false);
    [[nodiscard]] static quint16 macGameplayVirtualKey(const QKeyEvent *event);
    [[nodiscard]] static quint16 inputModifiers(Qt::KeyboardModifiers modifiers, int key);
    [[nodiscard]] static QString shortcutActionForInput(
        const QVariantMap &bindings, int key, Qt::KeyboardModifiers modifiers);

signals:
    void videoSizeChanged();
    void renderCallbackAvailableChanged();
    void inputEnabledChanged();
    void captureActiveChanged();
    void clipboardPasteChanged();
    void keyboardLayoutChanged();
    void clipboardPasteFailed();
    void inputCaptureErrorChanged();
    void relativeMouseChanged();
    void shortcutBindingsChanged();
    void frameGenerationChanged();
    void displayRefreshOverrideHzChanged();
    void metalFxUpscalingChanged();
    void fsrUpscalingChanged();
    void upscalingSharpnessChanged();
    void upscalingDenoiseChanged();
    void frameGenerationStatsChanged();
    void swapStatsChanged();
    void localShortcutRequested(const QString &action);

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) override;
    void focusInEvent(QFocusEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void hoverEnterEvent(QHoverEvent *event) override;
    void hoverMoveEvent(QHoverEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void itemChange(ItemChange change, const ItemChangeData &data) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    friend class StreamVideoItemTest;
    class NativeWindowMoveLoopFilter final : public QAbstractNativeEventFilter
    {
    public:
        explicit NativeWindowMoveLoopFilter(StreamVideoItem &item) : m_item(item) {}
        bool nativeEventFilter(const QByteArray &eventType, void *message,
                               qintptr *result) override;

    private:
        StreamVideoItem &m_item;
    };
    StreamVideoItem(std::unique_ptr<class MacPointerCapture> pointerCapture,
                    bool usesMacPointerCapture, QQuickItem *parent);
    struct PressedKey {
        quint16 virtualKey = 0;
        quint16 modifiers = 0;
        bool altGr = false;
    };

    void applyRemoteCursor(const QByteArray &bytes);
    void resetRemoteCursor();
    void setRemoteCursorShape(const QCursor &cursor);
    void updateLocalCursor();
    void refreshVideoSize();
    void syncCaptureState();
    void setNativeWindowMoveLoop(bool active);
    void connectFrameSwaps();
    void releaseInput();
    void releaseQtMouseButtons();
    void updateCursorConfinement();
    void updateSwapGate();
    void syncSwapGate();
    [[nodiscard]] QString currentSwapGateSource() const;
    void pushSwapGate();
    [[nodiscard]] static QRect cursorConfinementRect(const QRect &viewport, bool rawRelative);
    void releaseCursorConfinement();
    void submitAbsoluteMouse(const QPointF &position);
    [[nodiscard]] quint16 eventVirtualKey(const QKeyEvent *event) const;
    [[nodiscard]] Qt::KeyboardModifiers eventModifiers(const QKeyEvent *event) const;
    [[nodiscard]] quint32 keyIdentity(const QKeyEvent *event) const;
    [[nodiscard]] static quint8 mouseButton(Qt::MouseButton button);

    static QPointer<NativeStreamRuntime> s_nativeRuntime;

    QSize m_videoSize;
    QSize m_requestedVideoSize;
    QSize m_decodedVideoSize;
    QVariantMap m_shortcutBindings;
    std::shared_ptr<StreamVideoRenderCallback> m_renderCallback;
    QHash<quint32, PressedKey> m_pressedKeys;
    QSet<quint32> m_pressedShortcuts;
    QSet<quint8> m_pressedMouseButtons;
    QPointF m_lastMousePosition;
    std::unique_ptr<class WaylandPointerCapture> m_waylandPointer;
    std::unique_ptr<class MacPointerCapture> m_macPointer;
    bool m_usesMacPointerCapture = false;
    bool m_inputEnabled = true;
    bool m_clipboardPaste = false;
    QString m_keyboardLayout = QStringLiteral("en-US");
    const PhysicalKeyMap::Layout *m_keyboardMap = PhysicalKeyMap::layoutFor("en-US");
    int m_frameGenerationFactor = 0;
    int m_displayRefreshOverrideHz = 0;
    bool m_metalFxUpscaling = false;
    bool m_fsrUpscaling = false;
    int m_upscalingSharpness = 10;
    int m_upscalingDenoise = 0;
    QTimer m_frameStatsTimer;
    QTimer m_swapStatsTimer;
    // Measures how late the GUI thread runs a 10 ms timer while a stream is shown: frames
    // are requested from the GUI thread, so a GUI stall is a presentation stall.
    QTimer m_guiStallProbe;
    qint64 m_guiStallLastNs = 0;
    qint64 m_guiStallLoggedNs = 0;
    QString m_swapGateSource;
    QMetaObject::Connection m_frameSwapConnection;
    QMetaObject::Connection m_frameUpdateConnection;
    QPointer<QQuickWindow> m_inputWindow;
    bool m_captureActive = false;
    bool m_relativeMouse = false;
    bool m_rawInputActive = false;
    bool m_cursorConfined = false;
    bool m_nativeWindowMoveLoop = false;
    NativeWindowMoveLoopFilter m_nativeWindowMoveLoopFilter{*this};
    bool m_remoteCursorKnown = false;
    bool m_serverCursorComposited = true;
    bool m_remoteCursorVisible = false;
    QCursor m_remoteCursor;
    std::optional<bool> m_pendingRelativeMouse;
    std::optional<bool> m_manualRelativeMouse;
};

void registerStreamVideoItemQmlType();
