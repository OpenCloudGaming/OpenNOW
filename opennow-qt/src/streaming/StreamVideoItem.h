#pragma once

#include "streaming/rendering/StreamVideoRenderCallback.h"

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
    Q_PROPERTY(QString inputCaptureError READ inputCaptureError NOTIFY inputCaptureErrorChanged)
    Q_PROPERTY(bool relativeMouse READ relativeMouse WRITE setRelativeMouse
                   NOTIFY relativeMouseChanged)
    Q_PROPERTY(QVariantMap shortcutBindings READ shortcutBindings WRITE setShortcutBindings
                   NOTIFY shortcutBindingsChanged)
    Q_PROPERTY(bool frameGeneration READ frameGeneration WRITE setFrameGeneration
                   NOTIFY frameGenerationChanged)
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
    [[nodiscard]] QString inputCaptureError() const;
    [[nodiscard]] bool relativeMouse() const;
    void setRelativeMouse(bool relative);
    [[nodiscard]] QVariantMap shortcutBindings() const;
    void setShortcutBindings(const QVariantMap &bindings);
    [[nodiscard]] std::shared_ptr<StreamVideoRenderCallback> renderCallback() const;
    void setRenderCallback(std::shared_ptr<StreamVideoRenderCallback> callback);
    bool frameGeneration() const;
    void setFrameGeneration(bool enabled);
    bool metalFxUpscaling() const;
    void setMetalFxUpscaling(bool enabled);
    bool fsrUpscaling() const;
    void setFsrUpscaling(bool enabled);
    int upscalingSharpness() const;
    void setUpscalingSharpness(int value);
    int upscalingDenoise() const;
    void setUpscalingDenoise(int value);
    QVariantMap frameGenerationStats() const;

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
        int key, Qt::KeyboardModifiers modifiers = Qt::NoModifier);
    [[nodiscard]] static quint16 inputModifiers(Qt::KeyboardModifiers modifiers, int key);
    [[nodiscard]] static QString shortcutActionForInput(
        const QVariantMap &bindings, int key, Qt::KeyboardModifiers modifiers);

signals:
    void videoSizeChanged();
    void renderCallbackAvailableChanged();
    void inputEnabledChanged();
    void captureActiveChanged();
    void clipboardPasteChanged();
    void clipboardPasteFailed();
    void inputCaptureErrorChanged();
    void relativeMouseChanged();
    void shortcutBindingsChanged();
    void frameGenerationChanged();
    void metalFxUpscalingChanged();
    void fsrUpscalingChanged();
    void upscalingSharpnessChanged();
    void upscalingDenoiseChanged();
    void frameGenerationStatsChanged();
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

private:
    friend class StreamVideoItemTest;
    StreamVideoItem(std::unique_ptr<class MacPointerCapture> pointerCapture,
                    bool usesMacPointerCapture, QQuickItem *parent);
    struct PressedKey {
        quint16 virtualKey = 0;
        quint16 modifiers = 0;
    };

    void applyRemoteCursor(const QByteArray &bytes);
    void resetRemoteCursor();
    void setRemoteCursorShape(const QCursor &cursor);
    void updateLocalCursor();
    void syncCaptureState();
    void connectFrameSwaps();
    void releaseInput();
    void releaseQtMouseButtons();
    void updateCursorConfinement();
    [[nodiscard]] static QRect cursorConfinementRect(const QRect &viewport, bool rawRelative);
    void releaseCursorConfinement();
    void submitAbsoluteMouse(const QPointF &position);
    [[nodiscard]] quint32 keyIdentity(const QKeyEvent *event) const;
    [[nodiscard]] static quint8 mouseButton(Qt::MouseButton button);

    static QPointer<NativeStreamRuntime> s_nativeRuntime;

    QSize m_videoSize;
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
    bool m_frameGeneration = false;
    bool m_metalFxUpscaling = false;
    bool m_fsrUpscaling = false;
    int m_upscalingSharpness = 10;
    int m_upscalingDenoise = 0;
    QTimer m_frameStatsTimer;
    QMetaObject::Connection m_frameSwapConnection;
    QMetaObject::Connection m_frameUpdateConnection;
    bool m_captureActive = false;
    bool m_relativeMouse = false;
    bool m_rawInputActive = false;
    bool m_cursorConfined = false;
    bool m_remoteCursorKnown = false;
    bool m_serverCursorComposited = true;
    bool m_remoteCursorVisible = false;
    QCursor m_remoteCursor;
    std::optional<bool> m_pendingRelativeMouse;
    std::optional<bool> m_manualRelativeMouse;
};

void registerStreamVideoItemQmlType();
