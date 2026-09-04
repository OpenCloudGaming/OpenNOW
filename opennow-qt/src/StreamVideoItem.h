#pragma once

#include <QQuickRhiItem>
#include <QHash>
#include <QPoint>
#include <QSet>
#include <QRect>
#include <QSize>
#include <QVariantMap>

#include <memory>
#include <optional>

class QRhi;
class QRhiCommandBuffer;
class QRhiRenderTarget;
class QHoverEvent;
class NativeStreamRuntime;

class StreamVideoRenderCallback
{
public:
    virtual ~StreamVideoRenderCallback() = default;

    virtual void initialize(QRhi *rhi,
                            QRhiCommandBuffer *commandBuffer,
                            QRhiRenderTarget *renderTarget) = 0;
    virtual void prepareFrame(QRhiCommandBuffer *commandBuffer) = 0;
    virtual void recordFrame(QRhiCommandBuffer *commandBuffer,
                             const QRect &videoViewport) = 0;
    virtual void finishFrame() = 0;
    virtual void releaseResources() = 0;
};

class StreamVideoItem : public QQuickRhiItem
{
    Q_OBJECT
    Q_PROPERTY(QSize videoSize READ videoSize WRITE setVideoSize NOTIFY videoSizeChanged)
    Q_PROPERTY(bool renderCallbackAvailable READ renderCallbackAvailable
                   NOTIFY renderCallbackAvailableChanged)
    Q_PROPERTY(bool nativeRuntimeAvailable READ nativeRuntimeAvailable CONSTANT)
    Q_PROPERTY(bool inputEnabled READ inputEnabled WRITE setInputEnabled
                   NOTIFY inputEnabledChanged)
    Q_PROPERTY(bool captureActive READ captureActive NOTIFY captureActiveChanged)
    Q_PROPERTY(bool relativeMouse READ relativeMouse WRITE setRelativeMouse
                   NOTIFY relativeMouseChanged)
    Q_PROPERTY(QVariantMap shortcutBindings READ shortcutBindings WRITE setShortcutBindings
                   NOTIFY shortcutBindingsChanged)

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
    [[nodiscard]] bool relativeMouse() const;
    void setRelativeMouse(bool relative);
    [[nodiscard]] QVariantMap shortcutBindings() const;
    void setShortcutBindings(const QVariantMap &bindings);
    [[nodiscard]] std::shared_ptr<StreamVideoRenderCallback> renderCallback() const;
    void setRenderCallback(std::shared_ptr<StreamVideoRenderCallback> callback);

    static void setNativeStreamRuntime(NativeStreamRuntime *runtime);
    [[nodiscard]] static NativeStreamRuntime *nativeStreamRuntime();

    Q_INVOKABLE void requestFrame();
    Q_INVOKABLE void resynchronizeInput();

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
    [[nodiscard]] static quint16 windowsVirtualKey(int key);
    [[nodiscard]] static quint16 inputModifiers(Qt::KeyboardModifiers modifiers, int key);
    [[nodiscard]] static QString shortcutActionForInput(
        const QVariantMap &bindings, int key, Qt::KeyboardModifiers modifiers);

signals:
    void videoSizeChanged();
    void renderCallbackAvailableChanged();
    void inputEnabledChanged();
    void captureActiveChanged();
    void relativeMouseChanged();
    void shortcutBindingsChanged();
    void localShortcutRequested(const QString &action);

protected:
    QQuickRhiItemRenderer *createRenderer() override;
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
    struct PressedKey {
        quint16 virtualKey = 0;
        quint16 modifiers = 0;
    };

    void applyRemoteCursor(const QByteArray &bytes);
    void syncCaptureState();
    void releaseInput();
    void releaseQtMouseButtons();
    void updateCursorConfinement();
    void releaseCursorConfinement();
    void submitAbsoluteMouse(const QPointF &position);
    [[nodiscard]] quint32 keyIdentity(const QKeyEvent *event) const;
    [[nodiscard]] static quint8 mouseButton(Qt::MouseButton button);

    QSize m_videoSize;
    QVariantMap m_shortcutBindings;
    std::shared_ptr<StreamVideoRenderCallback> m_renderCallback;
    QHash<quint32, PressedKey> m_pressedKeys;
    QSet<quint32> m_pressedShortcuts;
    QSet<quint8> m_pressedMouseButtons;
    QPointF m_lastMousePosition;
    bool m_inputEnabled = true;
    bool m_captureActive = false;
    bool m_relativeMouse = false;
    bool m_rawInputActive = false;
    bool m_cursorConfined = false;
    bool m_remoteCursorKnown = false;
    bool m_remoteCursorVisible = false;
};

void registerStreamVideoItemQmlType();
