#include "streaming/StreamVideoItem.h"

#include "input/platform/WaylandPointerCapture.h"
#include "streaming/NativeStreamRuntime.h"
#include "streaming/rendering/NativeStreamRenderCallback.h"

#include <QCursor>
#include <QMetaObject>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QThread>

#include <algorithm>
#include <cmath>
#include <utility>

QPointer<NativeStreamRuntime> StreamVideoItem::s_nativeRuntime;

StreamVideoItem::StreamVideoItem(QQuickItem *parent)
    : QQuickItem(parent), m_waylandPointer(std::make_unique<WaylandPointerCapture>())
{
    connect(m_waylandPointer.get(), &WaylandPointerCapture::stateChanged, this, [this] {
        syncCaptureState();
        emit inputCaptureErrorChanged();
    }, Qt::QueuedConnection);
    connect(m_waylandPointer.get(), &WaylandPointerCapture::relativeMotion, this,
            [this](qint16 x, qint16 y) {
        if (m_captureActive && m_inputEnabled && m_relativeMouse && s_nativeRuntime)
            s_nativeRuntime->submitMouseRelative(x, y);
    });
    setFlag(ItemHasContents, true);
    setActiveFocusOnTab(true);
    setAcceptedMouseButtons(Qt::AllButtons);
    setKeepMouseGrab(true);
    setAcceptHoverEvents(true);
    m_frameStatsTimer.setInterval(1000);
    connect(&m_frameStatsTimer, &QTimer::timeout,
            this, &StreamVideoItem::frameGenerationStatsChanged);
    if (s_nativeRuntime) {
        connect(s_nativeRuntime, &NativeStreamRuntime::inputAllowedChanged,
                this, &StreamVideoItem::syncCaptureState);
        connect(s_nativeRuntime, &NativeStreamRuntime::inputCaptureReset, this, [this] {
            releaseInput();
            m_rawInputActive = false;
            if (std::exchange(m_captureActive, false)) emit captureActiveChanged();
        });
        setRenderCallback(createNativeStreamRenderCallback(s_nativeRuntime));
        connect(s_nativeRuntime, &NativeStreamRuntime::frameAvailable,
                this, &StreamVideoItem::requestFrame);
        connect(s_nativeRuntime, &NativeStreamRuntime::cursorUpdated,
                this, &StreamVideoItem::applyRemoteCursor, Qt::QueuedConnection);
        connect(s_nativeRuntime, &NativeStreamRuntime::runningChanged, this, [this] {
            if (!s_nativeRuntime || !s_nativeRuntime->running()) {
                m_remoteCursorKnown = false;
                m_remoteCursorVisible = false;
                m_remoteCursor = QCursor();
                if (m_relativeMouse) setRelativeMouse(false);
                else unsetCursor();
            }
            syncCaptureState();
        });
    }
    connect(this, &QQuickItem::windowChanged, this, [this](QQuickWindow *currentWindow) {
        connectFrameSwaps();
        if (currentWindow) {
            connect(currentWindow, &QWindow::activeChanged,
                    this, &StreamVideoItem::syncCaptureState, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::xChanged,
                    this, &StreamVideoItem::updateCursorConfinement, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::yChanged,
                    this, &StreamVideoItem::updateCursorConfinement, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::widthChanged,
                    this, &StreamVideoItem::resynchronizeInput, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::heightChanged,
                    this, &StreamVideoItem::resynchronizeInput, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::screenChanged,
                    this, &StreamVideoItem::resynchronizeInput, Qt::UniqueConnection);
        }
        syncCaptureState();
    });
}

StreamVideoItem::~StreamVideoItem()
{
    disconnect(m_frameSwapConnection);
    disconnect(m_frameUpdateConnection);
    releaseInput();
    if (s_nativeRuntime && s_nativeRuntime->running()) {
        bool rawInput = false;
        s_nativeRuntime->setCaptureActive(false, false, 0, &rawInput);
    }
}

QSize StreamVideoItem::videoSize() const
{
    return m_videoSize;
}

void StreamVideoItem::setVideoSize(const QSize &size)
{
    const auto normalized = size.isValid() && !size.isEmpty() ? size : QSize{};
    if (m_videoSize == normalized) return;
    m_videoSize = normalized;
    emit videoSizeChanged();
    update();
}

bool StreamVideoItem::renderCallbackAvailable() const
{
    return static_cast<bool>(m_renderCallback);
}

bool StreamVideoItem::nativeRuntimeAvailable() const
{
    return s_nativeRuntime && s_nativeRuntime->running();
}

bool StreamVideoItem::inputEnabled() const
{
    return m_inputEnabled;
}

void StreamVideoItem::setInputEnabled(bool enabled)
{
    if (m_inputEnabled == enabled) return;
    m_inputEnabled = enabled;
    syncCaptureState();
    emit inputEnabledChanged();
}

bool StreamVideoItem::captureActive() const
{
    return m_captureActive;
}

QString StreamVideoItem::inputCaptureError() const
{
    return m_waylandPointer->error();
}

bool StreamVideoItem::relativeMouse() const
{
    return m_relativeMouse;
}

QVariantMap StreamVideoItem::shortcutBindings() const
{
    return m_shortcutBindings;
}

void StreamVideoItem::setShortcutBindings(const QVariantMap &bindings)
{
    if (m_shortcutBindings == bindings) return;
    m_shortcutBindings = bindings;
    emit shortcutBindingsChanged();
}

std::shared_ptr<StreamVideoRenderCallback> StreamVideoItem::renderCallback() const
{
    return m_renderCallback;
}

void StreamVideoItem::setRenderCallback(std::shared_ptr<StreamVideoRenderCallback> callback)
{
    if (m_renderCallback == callback) return;
    const auto wasAvailable = renderCallbackAvailable();
    m_renderCallback = std::move(callback);
    connectFrameSwaps();
    if (wasAvailable != renderCallbackAvailable()) emit renderCallbackAvailableChanged();
    update();
}

bool StreamVideoItem::frameGeneration() const
{
    return m_frameGeneration;
}

void StreamVideoItem::setFrameGeneration(bool enabled)
{
    if (m_frameGeneration == enabled) return;
    m_frameGeneration = enabled;
    if (enabled) m_frameStatsTimer.start();
    else m_frameStatsTimer.stop();
    emit frameGenerationChanged();
    emit frameGenerationStatsChanged();
    update();
}

QVariantMap StreamVideoItem::frameGenerationStats() const
{
    return m_renderCallback ? m_renderCallback->frameGenerationStats() : QVariantMap{};
}

void StreamVideoItem::connectFrameSwaps()
{
    disconnect(m_frameSwapConnection);
    disconnect(m_frameUpdateConnection);
    if (!window() || !m_renderCallback) return;
    const std::weak_ptr<StreamVideoRenderCallback> callback = m_renderCallback;
    m_frameSwapConnection = connect(window(), &QQuickWindow::frameSwapped, this, [callback] {
        if (auto renderer = callback.lock()) renderer->frameSwapped();
    }, Qt::DirectConnection);
    m_frameUpdateConnection = connect(window(), &QQuickWindow::frameSwapped, this, [this] {
        if (m_frameGeneration && isVisible() && m_renderCallback && m_renderCallback->needsFrame())
            update();
    }, Qt::QueuedConnection);
}

void StreamVideoItem::setNativeStreamRuntime(NativeStreamRuntime *runtime)
{
    s_nativeRuntime = runtime;
}

NativeStreamRuntime *StreamVideoItem::nativeStreamRuntime()
{
    return s_nativeRuntime.data();
}

void StreamVideoItem::requestFrame()
{
    if (QThread::currentThread() != thread()) {
        QMetaObject::invokeMethod(this, &StreamVideoItem::requestFrame, Qt::QueuedConnection);
        return;
    }
    update();
}

QRect StreamVideoItem::aspectFitRect(const QSize &videoSize, const QSize &targetSize)
{
    if (!targetSize.isValid() || targetSize.isEmpty()) return {};
    if (!videoSize.isValid() || videoSize.isEmpty()) return QRect(QPoint{}, targetSize);

    const auto scale = std::min(static_cast<double>(targetSize.width()) / videoSize.width(),
                                static_cast<double>(targetSize.height()) / videoSize.height());
    const auto width = std::max(1, static_cast<int>(std::floor(videoSize.width() * scale)));
    const auto height = std::max(1, static_cast<int>(std::floor(videoSize.height() * scale)));
    return QRect((targetSize.width() - width) / 2,
                 (targetSize.height() - height) / 2,
                 width, height);
}

void registerStreamVideoItemQmlType()
{
    qmlRegisterType<StreamVideoItem>("OpenNOW", 1, 0, "StreamVideoItem");
}
