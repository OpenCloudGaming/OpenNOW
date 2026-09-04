#include "StreamVideoItem.h"

#include "NativeStreamRuntime.h"

#include <QColor>
#include <QCursor>
#include <QFocusEvent>
#include <QGuiApplication>
#include <QHoverEvent>
#include <QImage>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMetaObject>
#include <QMouseEvent>
#include <QPixmap>
#include <QPointer>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QThread>
#include <QWheelEvent>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>
#include <rhi/qshader.h>

#include <algorithm>
#include <cmath>
#include <utility>

#if defined(Q_OS_WIN)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

namespace {
QPointer<NativeStreamRuntime> nativeRuntime;

QShader streamShader(const char *encoded)
{
    return QShader::fromSerialized(QByteArray::fromBase64(encoded));
}

class ExternalCommandScope final
{
public:
    explicit ExternalCommandScope(QRhiCommandBuffer *commandBuffer)
        : m_commandBuffer(commandBuffer)
    {
        m_commandBuffer->beginExternal();
    }

    ~ExternalCommandScope()
    {
        m_commandBuffer->endExternal();
    }

    ExternalCommandScope(const ExternalCommandScope &) = delete;
    ExternalCommandScope &operator=(const ExternalCommandScope &) = delete;

private:
    QRhiCommandBuffer *m_commandBuffer;
};

class NativeStreamRenderCallback final : public StreamVideoRenderCallback
{
public:
    explicit NativeStreamRenderCallback(NativeStreamRuntime *runtime)
        : m_runtime(runtime)
    {
    }

    void initialize(QRhi *rhi, QRhiCommandBuffer *commandBuffer,
                    QRhiRenderTarget *renderTarget) override
    {
        if (m_rhi == rhi && m_renderTarget == renderTarget) return;
        releaseResources();
        m_rhi = rhi;
        m_renderTarget = renderTarget;
        if (!commandBuffer) return;

        OpenNowStreamerGraphicsContext context{};
        context.version = OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION;
        context.struct_size = sizeof(context);
        OpenNowStreamerStatus status = OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        {
            // The native streamer creates decoder/video-processor resources on Qt's adopted
            // graphics device. QRhi requires every external native command to be bracketed so it
            // can invalidate and restore its internal backend state.
            ExternalCommandScope externalCommands(commandBuffer);
            switch (rhi->backend()) {
#if defined(Q_OS_WIN)
            case QRhi::D3D11: {
                const auto *handles = static_cast<const QRhiD3D11NativeHandles *>(
                    rhi->nativeHandles());
                if (!handles) return;
                context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_D3D11;
                context.device = handles->dev;
                context.queue = handles->context;
                break;
            }
#endif
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
            case QRhi::Vulkan: {
                const auto *handles = static_cast<const QRhiVulkanNativeHandles *>(
                    rhi->nativeHandles());
                if (!handles || !handles->inst) return;
                context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_VULKAN;
                context.instance = reinterpret_cast<void *>(handles->inst->vkInstance());
                context.physical_device = reinterpret_cast<void *>(handles->physDev);
                context.device = reinterpret_cast<void *>(handles->dev);
                context.queue = reinterpret_cast<void *>(handles->gfxQueue);
                context.queue_family_index = handles->gfxQueueFamilyIdx;
                break;
            }
#endif
#if QT_CONFIG(metal)
            case QRhi::Metal: {
                const auto *handles = static_cast<const QRhiMetalNativeHandles *>(
                    rhi->nativeHandles());
                if (!handles) return;
                context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_METAL;
                context.device = handles->dev;
                context.queue = handles->cmdQueue;
                break;
            }
#endif
            default:
                return;
            }
            status = m_runtime ? m_runtime->setGraphicsContext(context)
                               : OPENNOW_STREAMER_CLOSED;
        }
        m_graphicsReady = status == OPENNOW_STREAMER_OK;
    }

    void prepareFrame(QRhiCommandBuffer *commandBuffer) override
    {
        if (!m_graphicsReady || !m_rhi || !commandBuffer) return;

        OpenNowStreamerRecordCommand command{};
        command.version = OPENNOW_STREAMER_RENDER_COMMAND_VERSION;
        command.struct_size = sizeof(command);
        command.frame_slot = static_cast<std::uint32_t>(m_rhi->currentFrameSlot());
        finishFrame();
        OpenNowStreamerFrameInfo info{};
        OpenNowStreamerRecordedFrame recorded{};
        OpenNowStreamerFrame *frame = nullptr;
        OpenNowStreamerStatus status = OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        {
            ExternalCommandScope externalCommands(commandBuffer);
            // Native command-buffer handles can change when QRhi begins an external section, so
            // query them only after beginExternal(), as required by the QRhi contract.
            switch (m_rhi->backend()) {
#if defined(Q_OS_WIN)
            case QRhi::D3D11: {
                const auto *handles = static_cast<const QRhiD3D11NativeHandles *>(
                    m_rhi->nativeHandles());
                command.command_buffer = handles ? handles->context : nullptr;
                break;
            }
#endif
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
            case QRhi::Vulkan: {
                const auto *handles = static_cast<const QRhiVulkanCommandBufferNativeHandles *>(
                    commandBuffer->nativeHandles());
                command.command_buffer = handles
                    ? reinterpret_cast<void *>(handles->commandBuffer) : nullptr;
                break;
            }
#endif
#if QT_CONFIG(metal)
            case QRhi::Metal: {
                const auto *handles = static_cast<const QRhiMetalCommandBufferNativeHandles *>(
                    commandBuffer->nativeHandles());
                command.command_buffer = handles ? handles->commandBuffer : nullptr;
                break;
            }
#endif
            default:
                return;
            }
            if (!command.command_buffer) return;
            status = m_runtime->recordLatestFrame(command, &info, &recorded, &frame);
        }

        if (status == OPENNOW_STREAMER_NO_FRAME) return;
        if (status != OPENNOW_STREAMER_OK || !frame || recorded.resource == 0
            || recorded.width == 0 || recorded.height == 0
            || (recorded.texture_format != OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8
                && recorded.texture_format != OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2)) {
            if (frame) m_runtime->releaseFrame(frame);
            return;
        }
        m_preparedFrame = frame;

        if (m_texture && m_textureObject == recorded.resource
            && m_textureFormat == recorded.texture_format
            && m_textureSize == QSize(static_cast<int>(recorded.width),
                                      static_cast<int>(recorded.height))) {
            m_frameReady = true;
            return;
        }

        delete m_bindings;
        m_bindings = nullptr;
        delete m_texture;
        const auto textureFormat = recorded.texture_format
                == OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2
            ? QRhiTexture::RGB10A2 : QRhiTexture::RGBA8;
        m_texture = m_rhi->newTexture(
            textureFormat,
            QSize(static_cast<int>(recorded.width), static_cast<int>(recorded.height)), 1);
        QRhiTexture::NativeTexture nativeTexture{};
        nativeTexture.object = recorded.resource;
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        if (recorded.graphics_api == OPENNOW_STREAMER_GRAPHICS_API_VULKAN)
            nativeTexture.layout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
#endif
        if (!m_texture->createFrom(nativeTexture)) {
            delete m_texture;
            m_texture = nullptr;
            return;
        }
        m_textureObject = recorded.resource;
        m_textureFormat = recorded.texture_format;
        m_textureSize = m_texture->pixelSize();

        if (!m_sampler) {
            m_sampler = m_rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear,
                                          QRhiSampler::None,
                                          QRhiSampler::ClampToEdge,
                                          QRhiSampler::ClampToEdge);
            if (!m_sampler->create()) return;
        }
        m_bindings = m_rhi->newShaderResourceBindings();
        m_bindings->setBindings({QRhiShaderResourceBinding::sampledTexture(
            0, QRhiShaderResourceBinding::FragmentStage, m_texture, m_sampler)});
        if (!m_bindings->create()) return;

        if (!m_pipeline) {
            static const auto vertexShader = streamShader(
                "AAASBHic7VZtT9tWFDav6UwZ7SgtG3S7vLRzWAgmUFQRoK3aiSJVgxXGJkVR5MY3qaXEtvwSwVCk/Yr9nf2tfZm2c+49Tq6T0AKbtH3YlWzfc+5zXn39XGuaNq51xxBco1qsteA5jIo/aWg3G4lvu6safT0yvJTBULp2S9sg7V0Ih8EnwASf+2+O3+TDyM5vPjFx/VNtpJPgFNgJT3A1LcfFOYNrGa41uF5rY9qX5HeCnoluiHRDim6YdBhhH+6Jbh8iLNPaCsnoP/E9DSjELYDNCNnch9k45fMA8BlqAq7NgHyL1nB8Q/InFHseZB2eGdIxkCdI1gmPviaF9S+/5SD/jOiN1E0qmCmBef48wdwhHWIS3V2Sp4RuXMT4jPzdofXE3z0l53FRi8jhWeLrPmFmFMwDadLJYZZ0M0oOn5PdpJLDF+Rvltaxj3PUx1Glb/P0HrFvD2l9Xrwf+V4Tm4fUy6+oz7hWJJmRDt/NAslDFAPlRaob7ZdobYHsUV4mHa4/pneVITlHvudIzpOcIXuU10i3BTkPU30a6X4HzRhhcR/cVmxRniZ5D9C36d3hfJp6uEs1PKI8X4A3zPFr8vWIMOjDIP33gMGeZUmnKboV0mG9R2A1J/oksSvK+8c4WPsq9XOR8lolfJKX2ZPXOtVkKnkVSI/2ayT/ARk8FVloo/T9o3CmdcbQ7FKLB6HjuWy9YOp61XPDiLV4tcAq609LG2W2K6RS2cCHsbqeN7M5oTI28maOqQqc5xios9liylNhq9+TiVjFuNAjmx3v4ExvWcG549alv7iFGs+xGRZlZPULncGoNypHXuhEWIyItWlgDaA+5UHEzw5elXOsE7YobOIWQDE/FVXU27qu9GyUtgGN8eUlP7DqTYtVGxbkZDtW3fXCyKkyByYBt9ni6o9NJwwh41U/8CIvOvd5uKhfx+5dYFXRRl9y3GojtjnbafLIalSA6xvOuz1FHzpNew1v+fd7uh6jOXOtJg99cMGEFTQs4k2/YUV8B5PBZXaSY6HzM69E7Lu4uaeHURBXIxb6rR/c0KrxF0FgnVNvTxhv8CZ3o7AEWPYMLdg2Wy/LNopb9D7gls1OHjPP54EVeQErlZlBIXwvzBJEoKVfHAGP4sDtBgAkuW2Lu9hJ/MwP5OwqcVTgNaOJm81bDrTuQyEk5G+WcoU4KvAmpfSGFHfLja4QNIHeJKzsfT3wYv8KO0Lg/pFt8fGIfehrxG0XiSKxgekPZafW8KyoAPy3h9QJtHLpunHBpJDQKUkqoSoASamsnZDqR2IXtq4WWyFg0hT6NF0SFuETikBeNCteHBE7SDSSaakUhzwwGl7VNbNl6ptY3kzxcwnbKqYIwra2BPt2XcuZETtQbZecXRswpZIEVxy7XE7Yv2uI1y67aMvgIOXTJ4NMR5wN4NxIO8/2nxHooXNODLKQMNo3gE7OjzGREx26Nj17Dl3z/0N38KGLvfpV+VG51+3Zk/9wz3Dz/Sv9GqZ/vuS3eA6IAn8tZHn0hXZadcFEE2rZ/NmZyj81WUotzUA10awakEBRH+g26ZtKLjVRQi1FL306UwmJ3hP3/ZTRidxPCMWUFbWeqOr46ODtaeVl4IVh5cD1O5Q1iFe22fFpp8GSlQZ4OYwjfwDzbbOTb396eXj49pV5KeuJAN2C2skOQTqrfGCbKIyVTlnulaSrl+4WARUbpr8QuT372sSgpXVecXCuJKW2axdfhaHg8j3EKPNRipOKATlIJ54QJEjV9PB3alP0YUX5uAMUTk67b/d+Nj3/92L0fE5y/AUa/w92");
            static const auto fragmentShader = streamShader(
                "AAAJEHicxVXdahNBFB6Ttmk31rYqahFktDcphBhDK9I0uWlUCoVKE6oQwrJuJtuF/Qm7s6FSeucz+BA+gE/jg3gjes7MbHY2idY7l0xm5pvzP+ecIYSsEEKWYNwSc0ImMBdgkF/qI/kP6Qxik5B4MCLF/SfqWc5HZAQ8FvEJIz0Yl4SDzghWhFRu4F/0rSi5lxr2eWfCotgNA/qiUTeMJHBHYeTT2PLHHosaHTqKLJ/12CVPItY0jIkVfXIDh06Y3aDJBJHQHVLfcoPKrnFlUPgcz3wTWU7H4la/PqAtyiV7o1PRpVWBf7dpXBsGxgUlqPjc1sLwdccNbC8ZMnroM255ZsyHnvuxbWR47PrD5/hXu2iDBzGaF4CaeGzZjAouMDPmUWJzYWjdDBOubB15ocX3qB16YUT7fTFX6ruDARg2y+UGOhO6DxxJzKKKF9pBxgROOj4LNGVyVUnFUPj1+zG3HAa7waA6jdDwUMhu58IOtOpcWFZNbydH1AUIPVBngjK9kMwOHC16BVYiDrua9LyVk1WTQipz8qtgeU3cGrJHDA4ClJLe4rLQpO5uqF3j92Jhp4TZZ5BVcl+hW1A7mI9lyEyc3550T2pwv7W9fSHjDilOa2GDlERu4FCpQtaEDEI2gQ7xEsqAFeJPlA7cr8P8bGb/VNtvafT3wKaCOC8K7bh+ACusHQrjIdCXSNYJKPyvwlxU+ptqv6awbVgZWu3pX7p/DJRlQgQdykvzv6zk4X5dYah/Q8krKPrNaYwk/aaKC2IvybKgW1L8aNMPQJZhbgFaFnGW8ltK9l3F/x6oSiImkgbxNkhYU9hP0PBKepGWL6pqZB4Wtntp6cvE3pvJ7ANIIseNOSQsr0NadWX2drnFGTV1UjNNeo0lRhaoUYu7dq6SmzlQNSpVy913x2fn5lEUxrF5HIzznUAU9QHtvf5wdHp61qnnuoDOeZrw8eImckC752bPihzGFbvokNgUzFybXFh5MgCVha5nDXPeEtmB53yjaZOBdaoXPGzpeA2jI9zITJTAAjWSLxQbSaQj026ibkHrEnnGa73nYwZ90epCe5P2/+VNwqYG79H07qG7/u15Sm1Me+qNbxLaN9Tq9VtmX71ujCNmu2J34ToXY5kMzTnYDQCc+iKx//bKph1Be2nnutNMKc+dk99YTBT/");
            if (!vertexShader.isValid() || !fragmentShader.isValid()) return;
            m_pipeline = m_rhi->newGraphicsPipeline();
            m_pipeline->setTopology(QRhiGraphicsPipeline::Triangles);
            m_pipeline->setShaderStages({
                {QRhiShaderStage::Vertex, vertexShader},
                {QRhiShaderStage::Fragment, fragmentShader},
            });
            m_pipeline->setShaderResourceBindings(m_bindings);
            m_pipeline->setRenderPassDescriptor(m_renderTarget->renderPassDescriptor());
            if (!m_pipeline->create()) {
                delete m_pipeline;
                m_pipeline = nullptr;
                return;
            }
        }
        m_frameReady = true;
    }

    void recordFrame(QRhiCommandBuffer *commandBuffer, const QRect &) override
    {
        if (!m_frameReady || !m_pipeline || !m_bindings) return;
        commandBuffer->setGraphicsPipeline(m_pipeline);
        commandBuffer->setShaderResources(m_bindings);
        commandBuffer->draw(3);
    }

    void finishFrame() override
    {
        if (m_preparedFrame && m_runtime)
            m_runtime->releaseFrame(std::exchange(m_preparedFrame, nullptr));
    }

    void releaseResources() override
    {
        finishFrame();
        m_frameReady = false;
        delete m_pipeline;
        m_pipeline = nullptr;
        delete m_bindings;
        m_bindings = nullptr;
        delete m_sampler;
        m_sampler = nullptr;
        delete m_texture;
        m_texture = nullptr;
        m_textureObject = 0;
        m_textureFormat = 0;
        m_textureSize = {};
        if (m_graphicsReady && m_runtime)
            m_runtime->sceneGraphShutdown();
        m_graphicsReady = false;
        m_rhi = nullptr;
        m_renderTarget = nullptr;
    }

private:
    NativeStreamRuntime *m_runtime;
    QRhi *m_rhi = nullptr;
    QRhiRenderTarget *m_renderTarget = nullptr;
    QRhiTexture *m_texture = nullptr;
    QRhiSampler *m_sampler = nullptr;
    QRhiShaderResourceBindings *m_bindings = nullptr;
    QRhiGraphicsPipeline *m_pipeline = nullptr;
    QSize m_textureSize;
    std::uint64_t m_textureObject = 0;
    std::uint32_t m_textureFormat = 0;
    OpenNowStreamerFrame *m_preparedFrame = nullptr;
    bool m_graphicsReady = false;
    bool m_frameReady = false;
};

class StreamVideoItemRenderer final : public QQuickRhiItemRenderer
{
public:
    ~StreamVideoItemRenderer() override
    {
        releaseCallbackResources();
    }

protected:
    void initialize(QRhiCommandBuffer *commandBuffer) override
    {
        initializeCallback(commandBuffer);
    }

    void synchronize(QQuickRhiItem *item) override
    {
        const auto *videoItem = static_cast<StreamVideoItem *>(item);
        const auto callback = videoItem->renderCallback();
        if (callback != m_callback) {
            releaseCallbackResources();
            m_callback = callback;
        }
        m_videoSize = videoItem->videoSize();
    }

    void render(QRhiCommandBuffer *commandBuffer) override
    {
        initializeCallback(commandBuffer);

        if (m_callback) m_callback->prepareFrame(commandBuffer);

        auto *target = renderTarget();
        if (!target) {
            if (m_callback) m_callback->finishFrame();
            return;
        }

        commandBuffer->beginPass(target, QColor(Qt::black), {1.0f, 0});
        if (m_callback) {
            const auto viewport = StreamVideoItem::aspectFitRect(m_videoSize,
                                                                  target->pixelSize());
            commandBuffer->setViewport(QRhiViewport(viewport.x(), viewport.y(),
                                                     viewport.width(), viewport.height()));
            m_callback->recordFrame(commandBuffer, viewport);
        }
        commandBuffer->endPass();
        if (m_callback) m_callback->finishFrame();
    }

private:
    void initializeCallback(QRhiCommandBuffer *commandBuffer)
    {
        if (!m_callback) return;
        auto *currentRhi = rhi();
        auto *currentTarget = renderTarget();
        if (!currentRhi || !currentTarget) return;
        if (m_initializedCallback == m_callback && m_initializedRhi == currentRhi
            && m_initializedTarget == currentTarget) {
            return;
        }
        releaseCallbackResources();
        m_callback->initialize(currentRhi, commandBuffer, currentTarget);
        m_initializedCallback = m_callback;
        m_initializedRhi = currentRhi;
        m_initializedTarget = currentTarget;
    }

    void releaseCallbackResources()
    {
        if (m_initializedCallback) m_initializedCallback->releaseResources();
        m_initializedCallback.reset();
        m_initializedRhi = nullptr;
        m_initializedTarget = nullptr;
    }

    QSize m_videoSize;
    std::shared_ptr<StreamVideoRenderCallback> m_callback;
    std::shared_ptr<StreamVideoRenderCallback> m_initializedCallback;
    QRhi *m_initializedRhi = nullptr;
    QRhiRenderTarget *m_initializedTarget = nullptr;
};
}

StreamVideoItem::StreamVideoItem(QQuickItem *parent)
    : QQuickRhiItem(parent)
{
    setAlphaBlending(false);
    setSampleCount(1);
    setActiveFocusOnTab(true);
    setAcceptedMouseButtons(Qt::AllButtons);
    setAcceptHoverEvents(true);
    if (nativeRuntime) {
        setRenderCallback(std::make_shared<NativeStreamRenderCallback>(nativeRuntime));
        connect(nativeRuntime, &NativeStreamRuntime::frameAvailable,
                this, &StreamVideoItem::requestFrame, Qt::QueuedConnection);
        connect(nativeRuntime, &NativeStreamRuntime::cursorUpdated,
                this, &StreamVideoItem::applyRemoteCursor, Qt::QueuedConnection);
        connect(nativeRuntime, &NativeStreamRuntime::runningChanged, this, [this] {
            if (!nativeRuntime || !nativeRuntime->running()) {
                m_remoteCursorKnown = false;
                m_remoteCursorVisible = false;
                if (m_relativeMouse) setRelativeMouse(false);
                else unsetCursor();
            }
            syncCaptureState();
        });
    }
    connect(this, &QQuickItem::windowChanged, this, [this](QQuickWindow *currentWindow) {
        if (currentWindow) {
            connect(currentWindow, &QWindow::activeChanged,
                    this, &StreamVideoItem::syncCaptureState, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::xChanged,
                    this, &StreamVideoItem::updateCursorConfinement, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::yChanged,
                    this, &StreamVideoItem::updateCursorConfinement, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::widthChanged,
                    this, &StreamVideoItem::updateCursorConfinement, Qt::UniqueConnection);
            connect(currentWindow, &QWindow::heightChanged,
                    this, &StreamVideoItem::updateCursorConfinement, Qt::UniqueConnection);
        }
        syncCaptureState();
    });
}

StreamVideoItem::~StreamVideoItem()
{
    if (nativeRuntime && nativeRuntime->running()) {
        bool rawInput = false;
        nativeRuntime->setCaptureActive(false, false, 0, &rawInput);
    }
    releaseInput();
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
    return nativeRuntime && nativeRuntime->running();
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
    if (wasAvailable != renderCallbackAvailable()) emit renderCallbackAvailableChanged();
    update();
}

void StreamVideoItem::setNativeStreamRuntime(NativeStreamRuntime *runtime)
{
    nativeRuntime = runtime;
}

NativeStreamRuntime *StreamVideoItem::nativeStreamRuntime()
{
    return nativeRuntime.data();
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

quint16 StreamVideoItem::windowsVirtualKey(int key)
{
    if (key >= Qt::Key_A && key <= Qt::Key_Z) return static_cast<quint16>(key);
    if (key >= Qt::Key_0 && key <= Qt::Key_9) return static_cast<quint16>(key);
    if (key >= Qt::Key_F1 && key <= Qt::Key_F24)
        return static_cast<quint16>(0x70 + key - Qt::Key_F1);
    switch (key) {
    case Qt::Key_Return:
    case Qt::Key_Enter: return 0x0d;
    case Qt::Key_Escape: return 0x1b;
    case Qt::Key_Backspace: return 0x08;
    case Qt::Key_Tab: return 0x09;
    case Qt::Key_Space: return 0x20;
    case Qt::Key_Minus: return 0xbd;
    case Qt::Key_Equal: return 0xbb;
    case Qt::Key_BracketLeft: return 0xdb;
    case Qt::Key_BracketRight: return 0xdd;
    case Qt::Key_Backslash: return 0xdc;
    case Qt::Key_Semicolon: return 0xba;
    case Qt::Key_Apostrophe: return 0xde;
    case Qt::Key_QuoteLeft: return 0xc0;
    case Qt::Key_Comma: return 0xbc;
    case Qt::Key_Period: return 0xbe;
    case Qt::Key_Slash: return 0xbf;
    case Qt::Key_Right: return 0x27;
    case Qt::Key_Left: return 0x25;
    case Qt::Key_Down: return 0x28;
    case Qt::Key_Up: return 0x26;
    case Qt::Key_Control: return 0xa2;
    case Qt::Key_Shift: return 0xa0;
    case Qt::Key_Alt: return 0xa4;
    case Qt::Key_Meta: return 0x5b;
    case Qt::Key_CapsLock: return 0x14;
    case Qt::Key_NumLock: return 0x90;
    case Qt::Key_Insert: return 0x2d;
    case Qt::Key_Delete: return 0x2e;
    case Qt::Key_Home: return 0x24;
    case Qt::Key_End: return 0x23;
    case Qt::Key_PageUp: return 0x21;
    case Qt::Key_PageDown: return 0x22;
    case Qt::Key_Print: return 0x2a;
    case Qt::Key_ScrollLock: return 0x91;
    case Qt::Key_Pause: return 0x13;
    case Qt::Key_Menu: return 0x5d;
    case Qt::Key_Plus: return 0x6b;
    case Qt::Key_Asterisk: return 0x6a;
    default: return 0;
    }
}

quint16 StreamVideoItem::inputModifiers(Qt::KeyboardModifiers modifiers, int key)
{
    quint16 result = 0;
    if (key != Qt::Key_Shift && modifiers.testFlag(Qt::ShiftModifier)) result |= 0x01;
    if (key != Qt::Key_Control && modifiers.testFlag(Qt::ControlModifier)) result |= 0x02;
    if (key != Qt::Key_Alt && modifiers.testFlag(Qt::AltModifier)) result |= 0x04;
    if (key != Qt::Key_Meta && modifiers.testFlag(Qt::MetaModifier)) result |= 0x08;
    return result;
}

QString StreamVideoItem::shortcutActionForInput(
    const QVariantMap &bindings, int key, Qt::KeyboardModifiers modifiers)
{
    constexpr auto shortcutModifiers = Qt::ControlModifier | Qt::ShiftModifier
        | Qt::AltModifier | Qt::MetaModifier;
    const auto normalizedModifiers = modifiers & shortcutModifiers;
    for (auto binding = bindings.cbegin(); binding != bindings.cend(); ++binding) {
        QStringList sequences;
        if (binding.value().metaType().id() == QMetaType::QString) {
            sequences.push_back(binding.value().toString());
        } else {
            const auto values = binding.value().toList();
            sequences.reserve(values.size());
            for (const auto &value : values) sequences.push_back(value.toString());
        }
        for (const auto &text : std::as_const(sequences)) {
            const QKeySequence sequence(text, QKeySequence::PortableText);
            if (sequence.count() != 1) continue;
            const auto combination = sequence[0];
            if (combination.key() == static_cast<Qt::Key>(key)
                && (combination.keyboardModifiers() & shortcutModifiers)
                    == normalizedModifiers) {
                return binding.key();
            }
        }
    }
    return {};
}

void StreamVideoItem::focusInEvent(QFocusEvent *event)
{
    QQuickRhiItem::focusInEvent(event);
    syncCaptureState();
}

void StreamVideoItem::focusOutEvent(QFocusEvent *event)
{
    releaseInput();
    syncCaptureState();
    QQuickRhiItem::focusOutEvent(event);
}

quint32 StreamVideoItem::keyIdentity(const QKeyEvent *event) const
{
    return event->nativeScanCode() != 0 ? event->nativeScanCode()
                                        : static_cast<quint32>(event->key());
}

void StreamVideoItem::keyPressEvent(QKeyEvent *event)
{
    if (!m_captureActive || event->isAutoRepeat()) {
        event->ignore();
        return;
    }
    const auto identity = keyIdentity(event);
    const auto shortcutAction = shortcutActionForInput(
        m_shortcutBindings, event->key(), event->modifiers());
    if (!shortcutAction.isEmpty()) {
        // A fullscreen transition can prevent Windows from delivering the key-up
        // that belongs to the key which initiated it.  Keep the identity only so
        // the eventual release is consumed; QKeyEvent::isAutoRepeat() already
        // prevents repeats, so a stale identity must never suppress the next
        // deliberate F11 press.
        m_pressedShortcuts.insert(identity);
        emit localShortcutRequested(shortcutAction);
        event->accept();
        return;
    }
    const auto virtualKey = windowsVirtualKey(event->key());
    if (virtualKey == 0) {
        event->ignore();
        return;
    }
    if (!m_pressedKeys.contains(identity)) {
        const auto modifiers = inputModifiers(event->modifiers(), event->key());
        m_pressedKeys.insert(identity, {virtualKey, modifiers});
        nativeRuntime->submitKey(virtualKey, modifiers, true);
    }
    event->accept();
}

void StreamVideoItem::keyReleaseEvent(QKeyEvent *event)
{
    if (event->isAutoRepeat()) {
        event->accept();
        return;
    }
    const auto identity = keyIdentity(event);
    if (m_pressedShortcuts.remove(identity)) {
        event->accept();
        return;
    }
    const auto pressed = m_pressedKeys.take(identity);
    if (pressed.virtualKey == 0) {
        event->ignore();
        return;
    }
    nativeRuntime->submitKey(pressed.virtualKey,
                             inputModifiers(event->modifiers(), event->key()), false);
    event->accept();
}

quint8 StreamVideoItem::mouseButton(Qt::MouseButton button)
{
    switch (button) {
    case Qt::LeftButton: return 1;
    case Qt::MiddleButton: return 2;
    case Qt::RightButton: return 3;
    case Qt::BackButton: return 4;
    case Qt::ForwardButton: return 5;
    default: return 0;
    }
}

void StreamVideoItem::mousePressEvent(QMouseEvent *event)
{
    forceActiveFocus(Qt::MouseFocusReason);
    syncCaptureState();
    const auto button = mouseButton(event->button());
    if (m_captureActive && button != 0) {
        if (!m_rawInputActive && !m_pressedMouseButtons.contains(button)) {
            // In absolute cursor mode position and button must have one owner and
            // preserve their queue order.  A move event is not guaranteed before
            // a click (notably after a fullscreen viewport change).
            if (!m_relativeMouse) submitAbsoluteMouse(event->position());
            m_pressedMouseButtons.insert(button);
            nativeRuntime->submitMouseButton(button, true);
        }
        m_lastMousePosition = event->position();
        event->accept();
        return;
    }
    event->ignore();
}

void StreamVideoItem::mouseReleaseEvent(QMouseEvent *event)
{
    const auto button = mouseButton(event->button());
    if (m_captureActive && button != 0) {
        if (!m_rawInputActive && m_pressedMouseButtons.remove(button)) {
            if (!m_relativeMouse) submitAbsoluteMouse(event->position());
            nativeRuntime->submitMouseButton(button, false);
        }
        event->accept();
        return;
    }
    event->ignore();
}

void StreamVideoItem::mouseMoveEvent(QMouseEvent *event)
{
    if (!m_captureActive) {
        event->ignore();
        return;
    }
    if (m_relativeMouse) {
        if (!m_rawInputActive) {
            const auto delta = event->position() - m_lastMousePosition;
            const auto deltaX = std::clamp(qRound(delta.x()), -32768, 32767);
            const auto deltaY = std::clamp(qRound(delta.y()), -32768, 32767);
            if (deltaX != 0 || deltaY != 0)
                nativeRuntime->submitMouseRelative(static_cast<qint16>(deltaX),
                                                   static_cast<qint16>(deltaY));
            const auto anchor = mapToGlobal(QPointF(width() / 2.0, height() / 2.0)).toPoint();
            QCursor::setPos(anchor);
            m_lastMousePosition = mapFromGlobal(QCursor::pos());
        }
    } else {
        submitAbsoluteMouse(event->position());
        m_lastMousePosition = event->position();
    }
    event->accept();
}

void StreamVideoItem::hoverEnterEvent(QHoverEvent *event)
{
    if (!m_captureActive || m_relativeMouse) {
        event->ignore();
        return;
    }
    // QQuickItem sends ordinary no-button movement through hover events once
    // hover delivery is enabled. Publish the entry point as well so the remote
    // cursor cannot retain a stale position when it re-enters the stream item.
    submitAbsoluteMouse(event->position());
    m_lastMousePosition = event->position();
    event->accept();
}

void StreamVideoItem::hoverMoveEvent(QHoverEvent *event)
{
    if (!m_captureActive || m_relativeMouse) {
        event->ignore();
        return;
    }
    // With no button held Qt does not call mouseMoveEvent for this item. Keep
    // the absolute GFN pointer current so remote hover and click hit-testing
    // use the same coordinates.
    submitAbsoluteMouse(event->position());
    m_lastMousePosition = event->position();
    event->accept();
}

void StreamVideoItem::wheelEvent(QWheelEvent *event)
{
    if (!m_captureActive) {
        event->ignore();
        return;
    }
    if (!m_rawInputActive) {
        if (!m_relativeMouse) submitAbsoluteMouse(event->position());
        const auto delta = event->pixelDelta().isNull() ? event->angleDelta()
                                                        : event->pixelDelta();
        nativeRuntime->submitMouseWheel(
            static_cast<qint16>(std::clamp(delta.x(), -32768, 32767)),
            static_cast<qint16>(std::clamp(delta.y(), -32768, 32767)));
    }
    event->accept();
}

void StreamVideoItem::itemChange(ItemChange change, const ItemChangeData &data)
{
    QQuickRhiItem::itemChange(change, data);
    if (change == ItemVisibleHasChanged && !isVisible()) {
        m_remoteCursorKnown = false;
        m_remoteCursorVisible = false;
        if (m_relativeMouse) setRelativeMouse(false);
        else unsetCursor();
    }
    if (change == ItemVisibleHasChanged || change == ItemSceneChange)
        syncCaptureState();
}

void StreamVideoItem::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry)
{
    QQuickRhiItem::geometryChange(newGeometry, oldGeometry);
    updateCursorConfinement();
}

QRect StreamVideoItem::scaledCaptureRect(const QRectF &itemRect,
                                         const QSizeF &windowSize,
                                         const QRect &clientScreenRect)
{
    if (!itemRect.isValid() || itemRect.isEmpty() || !windowSize.isValid()
        || windowSize.isEmpty() || !clientScreenRect.isValid()
        || clientScreenRect.isEmpty()) {
        return {};
    }
    const auto scaleX = clientScreenRect.width() / windowSize.width();
    const auto scaleY = clientScreenRect.height() / windowSize.height();
    const auto left = clientScreenRect.left()
        + static_cast<int>(std::floor(itemRect.left() * scaleX));
    const auto top = clientScreenRect.top()
        + static_cast<int>(std::floor(itemRect.top() * scaleY));
    const auto right = clientScreenRect.left()
        + static_cast<int>(std::ceil(itemRect.right() * scaleX));
    const auto bottom = clientScreenRect.top()
        + static_cast<int>(std::ceil(itemRect.bottom() * scaleY));
    return QRect(left, top, std::max(0, right - left),
                 std::max(0, bottom - top)).intersected(clientScreenRect);
}

QRect StreamVideoItem::absoluteMouseCoordinates(const QPointF &position,
                                                const QSize &videoSize,
                                                const QSizeF &itemSize)
{
    const auto target = QSize(std::max(1, qRound(itemSize.width())),
                              std::max(1, qRound(itemSize.height())));
    const auto viewport = aspectFitRect(videoSize, target);
    const auto x = std::clamp(qRound(position.x()) - viewport.x(), 0,
                              std::max(0, viewport.width() - 1));
    const auto y = std::clamp(qRound(position.y()) - viewport.y(), 0,
                              std::max(0, viewport.height() - 1));
    return QRect(x, y, viewport.width(), viewport.height());
}

StreamVideoItem::RemoteCursorMetadata StreamVideoItem::remoteCursorMetadata(
    const QByteArray &bytes)
{
    RemoteCursorMetadata result;
    if (bytes.size() < 7) return result;
    const auto messageType = static_cast<quint8>(bytes[0]);
    if (messageType > 1) return result;
    const auto mimeLength = static_cast<qsizetype>(static_cast<quint8>(bytes[4]));
    const auto lengthOffset = qsizetype{5} + mimeLength;
    if (lengthOffset < 5 || lengthOffset + 2 > bytes.size()) return result;
    const auto imageLength = static_cast<qsizetype>(
        static_cast<quint8>(bytes[lengthOffset])
        | (static_cast<quint16>(static_cast<quint8>(bytes[lengthOffset + 1])) << 8));
    const auto imageOffset = lengthOffset + 2;
    if (imageLength < 0 || imageOffset > bytes.size()
        || imageLength > bytes.size() - imageOffset) {
        return result;
    }
    result.imageOffset = imageOffset;
    result.imageLength = imageLength;
    const auto positionOffset = imageOffset + imageLength;
    if (positionOffset + 4 <= bytes.size()) {
        result.normalizedPosition = QPoint(
            static_cast<quint8>(bytes[positionOffset])
                | (static_cast<quint16>(static_cast<quint8>(bytes[positionOffset + 1])) << 8),
            static_cast<quint8>(bytes[positionOffset + 2])
                | (static_cast<quint16>(static_cast<quint8>(bytes[positionOffset + 3])) << 8));
    }
    const auto scaleOffset = positionOffset + 4;
    if (scaleOffset + 2 <= bytes.size()) {
        const auto scalePercent = static_cast<quint16>(
            static_cast<quint8>(bytes[scaleOffset])
            | (static_cast<quint16>(static_cast<quint8>(bytes[scaleOffset + 1])) << 8));
        if (scalePercent > 0) result.scale = scalePercent / 100.0;
    }
    return result;
}

QPoint StreamVideoItem::mapRemoteCursorPosition(const QPoint &normalizedPosition,
                                                const QSize &videoSize,
                                                const QSizeF &itemSize)
{
    const auto target = QSize(std::max(1, qRound(itemSize.width())),
                              std::max(1, qRound(itemSize.height())));
    const auto viewport = aspectFitRect(videoSize, target);
    const auto coordinate = [](int value, int extent) {
        const auto safeExtent = std::max(1, extent);
        return static_cast<int>(std::min<qint64>(
            (static_cast<qint64>(std::clamp(value, 0, 65535)) * safeExtent) / 65535,
            safeExtent - 1));
    };
    return QPoint(viewport.x() + coordinate(normalizedPosition.x(), viewport.width()),
                  viewport.y() + coordinate(normalizedPosition.y(), viewport.height()));
}

void StreamVideoItem::resynchronizeInput()
{
    syncCaptureState();
    if (m_captureActive && m_relativeMouse && !m_rawInputActive) {
        const auto anchor = mapToGlobal(QPointF(width() / 2.0, height() / 2.0)).toPoint();
        QCursor::setPos(anchor);
    } else if (m_captureActive && !m_relativeMouse) {
        // Fullscreen changes the absolute viewport dimensions without requiring
        // the physical cursor to move. Re-publish the current point immediately.
        submitAbsoluteMouse(mapFromGlobal(QCursor::pos()));
    }
    m_lastMousePosition = mapFromGlobal(QCursor::pos());
    updateCursorConfinement();
}

void StreamVideoItem::submitAbsoluteMouse(const QPointF &position)
{
    const auto coordinates = absoluteMouseCoordinates(
        position, m_videoSize, QSizeF(width(), height()));
    nativeRuntime->submitMouseAbsolute(
        static_cast<quint16>(std::min(coordinates.x(), 65535)),
        static_cast<quint16>(std::min(coordinates.y(), 65535)),
        static_cast<quint16>(std::min(coordinates.width(), 65535)),
        static_cast<quint16>(std::min(coordinates.height(), 65535)));
}

void StreamVideoItem::syncCaptureState()
{
    const auto desired = m_inputEnabled && isVisible() && hasActiveFocus()
        && window() && window()->isActive() && nativeRuntime && nativeRuntime->running();
    if (!desired && m_captureActive) releaseInput();
    bool rawInput = false;
    if (nativeRuntime && nativeRuntime->running()) {
        nativeRuntime->setCaptureActive(
            desired, m_relativeMouse,
            window() ? static_cast<std::uintptr_t>(window()->winId()) : 0,
            &rawInput);
    }
    m_rawInputActive = desired && rawInput;
    const auto changed = m_captureActive != desired;
    m_captureActive = desired;
    if (m_captureActive) {
        m_lastMousePosition = mapFromGlobal(QCursor::pos());
        if (m_relativeMouse) grabMouse();
    } else {
        ungrabMouse();
    }
    updateCursorConfinement();
    if (changed) emit captureActiveChanged();
}

void StreamVideoItem::releaseInput()
{
    if (nativeRuntime) {
        for (const auto &pressed : std::as_const(m_pressedKeys))
            nativeRuntime->submitKey(pressed.virtualKey, 0, false);
    }
    m_pressedKeys.clear();
    m_pressedShortcuts.clear();
    if (!m_rawInputActive) releaseQtMouseButtons();
    else m_pressedMouseButtons.clear();
    ungrabMouse();
    releaseCursorConfinement();
}

void StreamVideoItem::releaseQtMouseButtons()
{
    if (nativeRuntime) {
        for (const auto button : std::as_const(m_pressedMouseButtons))
            nativeRuntime->submitMouseButton(button, false);
    }
    m_pressedMouseButtons.clear();
}

void StreamVideoItem::updateCursorConfinement()
{
#if defined(Q_OS_WIN)
    if (!m_captureActive || !window() || !window()->isActive()) {
        releaseCursorConfinement();
        return;
    }
    const auto handle = reinterpret_cast<HWND>(window()->winId());
    RECT client{};
    POINT topLeft{};
    if (!handle || !GetClientRect(handle, &client)
        || !ClientToScreen(handle, &topLeft)) {
        releaseCursorConfinement();
        return;
    }
    POINT bottomRight{client.right, client.bottom};
    if (!ClientToScreen(handle, &bottomRight)) {
        releaseCursorConfinement();
        return;
    }
    const auto *content = window()->contentItem();
    if (!content) {
        releaseCursorConfinement();
        return;
    }
    const auto first = mapToItem(content, QPointF(0, 0));
    const auto second = mapToItem(content, QPointF(width(), height()));
    const QRectF itemRect(QPointF(std::min(first.x(), second.x()),
                                 std::min(first.y(), second.y())),
                          QPointF(std::max(first.x(), second.x()),
                                  std::max(first.y(), second.y())));
    const auto captureRect = scaledCaptureRect(
        itemRect, QSizeF(window()->width(), window()->height()),
        QRect(topLeft.x, topLeft.y, bottomRight.x - topLeft.x,
              bottomRight.y - topLeft.y));
    if (captureRect.isEmpty()) {
        releaseCursorConfinement();
        return;
    }
    const RECT screenRect{captureRect.left(), captureRect.top(),
                          captureRect.left() + captureRect.width(),
                          captureRect.top() + captureRect.height()};
    m_cursorConfined = ClipCursor(&screenRect) != FALSE;
#else
    m_cursorConfined = false;
#endif
}

void StreamVideoItem::releaseCursorConfinement()
{
#if defined(Q_OS_WIN)
    if (m_cursorConfined) ClipCursor(nullptr);
#endif
    m_cursorConfined = false;
}

void StreamVideoItem::setRelativeMouse(bool relative)
{
    if (m_relativeMouse == relative) return;
    // Qt owns buttons while the remote cursor is visible; Windows Raw Input
    // owns them in relative mode. A raw release cannot match a button that Qt
    // pressed, so close the old ownership epoch before enabling Raw Input.
    if (relative && !m_rawInputActive) releaseQtMouseButtons();
    m_relativeMouse = relative;
    if (relative) {
        setCursor(Qt::BlankCursor);
        if (m_captureActive) {
            grabMouse();
            const auto anchor = mapToGlobal(QPointF(width() / 2.0, height() / 2.0)).toPoint();
            QCursor::setPos(anchor);
            m_lastMousePosition = mapFromGlobal(QCursor::pos());
        }
    } else {
        ungrabMouse();
        releaseCursorConfinement();
        unsetCursor();
    }
    syncCaptureState();
    emit relativeMouseChanged();
}

void StreamVideoItem::applyRemoteCursor(const QByteArray &bytes)
{
    if (bytes.size() < 2) return;
    const auto messageType = static_cast<quint8>(bytes[0]);
    const auto cursorId = static_cast<quint8>(bytes[1]);
    if (messageType > 1) return;
    const auto hidden = messageType == 0 && cursorId == 0;
    const auto reposition = !m_remoteCursorKnown || !m_remoteCursorVisible;
    const auto metadata = remoteCursorMetadata(bytes);
    m_remoteCursorKnown = true;
    m_remoteCursorVisible = !hidden;
    setRelativeMouse(hidden);
    if (hidden) return;

    if (reposition && metadata.normalizedPosition) {
        const auto local = mapRemoteCursorPosition(
            *metadata.normalizedPosition, m_videoSize, QSizeF(width(), height()));
        QCursor::setPos(mapToGlobal(local).toPoint());
        m_lastMousePosition = local;
    }

    if (messageType == 1 && metadata.imageOffset >= 0 && metadata.imageLength > 0) {
        QPixmap pixmap;
        const auto image = QByteArray::fromBase64(
            bytes.mid(metadata.imageOffset, metadata.imageLength));
        if (pixmap.loadFromData(image) && pixmap.width() <= 256 && pixmap.height() <= 256) {
            const auto scaledSize = QSize(
                std::clamp(qRound(pixmap.width() / metadata.scale), 1, 256),
                std::clamp(qRound(pixmap.height() / metadata.scale), 1, 256));
            if (scaledSize != pixmap.size())
                pixmap = pixmap.scaled(scaledSize, Qt::IgnoreAspectRatio,
                                       Qt::SmoothTransformation);
            const auto hotspot = QPoint(
                std::clamp(qRound(static_cast<quint8>(bytes[2]) / metadata.scale),
                           0, pixmap.width() - 1),
                std::clamp(qRound(static_cast<quint8>(bytes[3]) / metadata.scale),
                           0, pixmap.height() - 1));
            setCursor(QCursor(pixmap, hotspot.x(), hotspot.y()));
            return;
        }
    }
    switch (cursorId) {
    case 2: setCursor(Qt::IBeamCursor); break;
    case 3: setCursor(Qt::WaitCursor); break;
    case 4: setCursor(Qt::CrossCursor); break;
    case 6: setCursor(Qt::SizeFDiagCursor); break;
    case 7: setCursor(Qt::SizeBDiagCursor); break;
    case 8: setCursor(Qt::SizeHorCursor); break;
    case 9: setCursor(Qt::SizeVerCursor); break;
    case 10: setCursor(Qt::SizeAllCursor); break;
    case 12: setCursor(Qt::PointingHandCursor); break;
    default: setCursor(Qt::ArrowCursor); break;
    }
}

QQuickRhiItemRenderer *StreamVideoItem::createRenderer()
{
    return new StreamVideoItemRenderer;
}

void registerStreamVideoItemQmlType()
{
    qmlRegisterType<StreamVideoItem>("OpenNOW", 1, 0, "StreamVideoItem");
}
