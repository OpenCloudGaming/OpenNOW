#include "streaming/rendering/HdrOutput.h"
#include "streaming/rendering/HdrOutputPass.h"
#include "streaming/rendering/HdrSwapChainRecovery.h"
#include "streaming/rendering/WaylandHdrOutput.h"
#if defined(Q_OS_MACOS)
#include "streaming/rendering/MetalHdrOutput.h"
#endif

#include <QQuickWindow>
#include <QQuickRenderTarget>
#include <QScreen>
#include <private/qquickwindow_p.h>
#include <rhi/qrhi.h>
#include <cmath>
#include <memory>

std::atomic<int> HdrOutput::s_mode{0};
std::atomic<float> HdrOutput::s_whiteNits{203.0f};
std::atomic<bool> HdrOutput::s_supported{false};

HdrOutput::HdrOutput(QObject *parent) : QObject(parent)
{
    m_waylandOutput = std::make_unique<WaylandHdrOutput>();
    connect(m_waylandOutput.get(), &WaylandHdrOutput::changed, this, [this] {
        if (!m_waylandOutput->state().supported && m_supported) {
            m_supported = false;
            emit changed();
        }
        m_probeRequested.store(true);
        if (m_window) m_window->update();
    });
    m_probeTimer.setInterval(1500);
    connect(&m_probeTimer, &QTimer::timeout, this, [this] {
        m_probeRequested.store(true);
        if (m_window && m_window->isVisible()) m_window->update();
    });
}

HdrOutput::~HdrOutput() = default;

void HdrOutput::attach(QQuickWindow *window)
{
    if (!window || m_window) return;
    const auto api = window->rendererInterface()->graphicsApi();
    if (api != QSGRendererInterface::Direct3D11 && api != QSGRendererInterface::Vulkan
            && api != QSGRendererInterface::Metal) return;
    m_window = window;
    m_waylandOutput->attach(window);
    connect(window, &QQuickWindow::beforeFrameBegin, this,
            &HdrOutput::updateOutput, Qt::DirectConnection);
    connect(window, &QQuickWindow::sceneGraphInvalidated, this, [this] {
        m_probeRequested.store(true);
        m_chromeSynchronized = false;
        m_window->setRenderTarget({});
        QQuickWindowPrivate::get(m_window)->redirect.commandBuffer = nullptr;
        m_outputPass.reset();
        publish({});
    }, Qt::DirectConnection);
    connect(window, &QQuickWindow::beforeSynchronizing, this, [this] {
        if (m_outputPass) {
            auto *d = QQuickWindowPrivate::get(m_window);
            d->redirect.commandBuffer = d->swapchain->currentFrameCommandBuffer();
        }
    }, Qt::DirectConnection);
    connect(window, &QQuickWindow::afterRendering, this, [this] {
        if (m_outputPass) m_outputPass->record(QQuickWindowPrivate::get(m_window)->swapchain);
    }, Qt::DirectConnection);
    connect(window, &QQuickWindow::afterSynchronizing, this, [this] {
        if (!m_chromeSynchronized && m_chromeGuiReady.load()) {
            m_chromeSynchronized = true;
            m_probeRequested.store(true);
            QMetaObject::invokeMethod(m_window, &QQuickWindow::update, Qt::QueuedConnection);
        }
    }, Qt::DirectConnection);
    connect(window, &QWindow::screenChanged, this, [this] {
        if (m_supported) {
            m_supported = false;
            emit changed();
        }
        m_probeRequested.store(true);
        m_window->update();
    });
    m_probeTimer.start();
}

HdrOutput::State HdrOutput::renderState()
{
    return {s_mode.load(), s_whiteNits.load(), s_supported.load()};
}

QString HdrOutput::status() const
{
    return m_supported ? tr("HDR output ready. Applies to the next stream.")
                       : tr("HDR unavailable on this display. Enable HDR in your operating system and use a supported GPU and compositor.");
}

void HdrOutput::publish(State state)
{
    s_mode.store(state.mode);
    s_whiteNits.store(state.whiteNits);
    s_supported.store(state.supported);
    QMetaObject::invokeMethod(this, [this, state]() mutable {
#if defined(Q_OS_LINUX)
        state.supported = state.supported && m_waylandOutput->state().supported;
#endif
        if (m_supported == state.supported && m_mode == state.outputMode) return;
        m_supported = state.supported;
        m_mode = state.outputMode;
        emit changed();
    }, Qt::QueuedConnection);
}

void HdrOutput::requestChrome(bool required)
{
    if (!required) m_chromeGuiReady.store(false);
    QMetaObject::invokeMethod(this, [this, required] {
        if (m_chromeRequired != required) {
            m_chromeRequired = required;
            emit changed();
        }
        m_chromeGuiReady.store(required);
        if (m_window) m_window->update();
    }, Qt::QueuedConnection);
}

void HdrOutput::updateOutput()
{
    if (!m_window) return;
    const bool probe = m_probeRequested.exchange(false);
    auto *d = QQuickWindowPrivate::get(m_window);
    auto *sc = d->swapchain;
    if (!sc || !d->rhi) {
        m_probeRequested.store(true);
        publish({});
        return;
    }
    if (m_outputPass) {
        d->redirect.commandBuffer = sc->currentFrameCommandBuffer();
        auto target = m_window->renderTarget();
        if (target.devicePixelRatio() != m_window->devicePixelRatio()) {
            target.setDevicePixelRatio(m_window->devicePixelRatio());
            m_window->setRenderTarget(target);
        }
    }
    if (!probe && d->hasActiveSwapchain && d->hasRenderableSwapchain
            && (sc->format() != QRhiSwapChain::HDR10
                || (m_outputPass && m_outputPass->matches(d->rhi, sc)))) return;
    const auto waylandOutput = m_waylandOutput->state();
#if defined(Q_OS_LINUX)
    const bool platformReady = waylandOutput.supported;
#else
    const bool platformReady = true;
#endif
    const bool outputReady = platformReady && (d->rhi->backend() == QRhi::D3D11
        || d->rhi->backend() == QRhi::Vulkan || d->rhi->backend() == QRhi::Metal);
    const bool linearSupported = outputReady
        && sc->isFormatSupported(QRhiSwapChain::HDRExtendedSrgbLinear);
    const auto desired = preferredHdrSwapChainFormat(outputReady, linearSupported,
        outputReady && !linearSupported && sc->isFormatSupported(QRhiSwapChain::HDR10));
    const bool hdrAvailable = desired != QRhiSwapChain::SDR;
    if (desired != QRhiSwapChain::SDR && !m_chromeSynchronized) {
        requestChrome(true);
        m_probeRequested.store(true);
        return;
    }
    const auto changeFormat = [&](QRhiSwapChain::Format format) {
        d->rhi->finish();
        m_window->setRenderTarget({});
        d->redirect.commandBuffer = nullptr;
        m_outputPass.reset();
        auto *previousPass = d->rpDescForSwapchain;
        QRhiRenderPassDescriptor *nextPass = nullptr;
        const auto result = createHdrSwapChainWithSdrFallback(format, [&](auto attempt) {
            destroyHdrSwapChainPreservingProxy(*sc);
            delete nextPass;
            nextPass = nullptr;
            sc->setFormat(attempt);
            nextPass = sc->newCompatibleRenderPassDescriptor();
            if (!nextPass) return false;
            sc->setRenderPassDescriptor(nextPass);
            if (!sc->createOrResize()) return false;
#if defined(Q_OS_MACOS)
            if (d->rhi->backend() == QRhi::Metal && attempt == QRhiSwapChain::SDR
                    && !resetMetalSdrOutput(m_window, sc->proxyData())) {
                qWarning("Metal SDR recovery could not restore the output layer color space.");
                m_probeRequested.store(true);
            }
#endif
            return true;
        });
        if (!result.created || result.format != format) {
            qWarning("HDR output format change failed; recovery format=%d %s.",
                int(result.format), result.created ? "active" : "unavailable");
        }
        if (nextPass) {
            d->rpDescForSwapchain = nextPass;
            delete previousPass;
        } else {
            sc->setRenderPassDescriptor(previousPass);
        }
        d->hasActiveSwapchain = result.created;
        d->hasRenderableSwapchain = result.created;
        d->swapchainJustBecameRenderable = !result.created;
    };
    if (desired != sc->format() || !d->hasActiveSwapchain || !d->hasRenderableSwapchain)
        changeFormat(desired);
#if defined(Q_OS_MACOS)
    else if (d->rhi->backend() == QRhi::Metal && desired == QRhiSwapChain::SDR
            && !resetMetalSdrOutput(m_window, sc->proxyData())) {
        m_probeRequested.store(true);
        publish({});
        return;
    }
#endif
    if (sc->format() == QRhiSwapChain::HDR10 && d->hasActiveSwapchain && d->hasRenderableSwapchain) {
        if (!m_outputPass || !m_outputPass->matches(d->rhi, sc)) {
            d->rhi->finish();
            m_window->setRenderTarget({});
            m_outputPass.reset();
            auto pass = std::make_unique<HdrOutputPass>();
            if (pass->initialize(d->rhi, sc)) {
                auto target = QQuickRenderTarget::fromRhiRenderTarget(pass->target());
                target.setDevicePixelRatio(m_window->devicePixelRatio());
                m_window->setRenderTarget(target);
                d->redirect.commandBuffer = sc->currentFrameCommandBuffer();
                m_outputPass = std::move(pass);
            } else {
                d->redirect.commandBuffer = nullptr;
                changeFormat(QRhiSwapChain::SDR);
                qWarning("HDR10 linear composition resources unavailable; using SDR output.");
            }
        }
    } else if (m_outputPass) {
        d->rhi->finish();
        m_window->setRenderTarget({});
        d->redirect.commandBuffer = nullptr;
        m_outputPass.reset();
    }
    State state;
    const auto info = sc->hdrInfo();
    state.outputMode = sc->format() == QRhiSwapChain::HDRExtendedSrgbLinear
        ? (info.luminanceBehavior == QRhiSwapChainHdrInfo::DisplayReferred
            ? LinearDisplayReferred : LinearScRgb)
        : sc->format() == QRhiSwapChain::HDR10 ? Hdr10 : Sdr;
    state.mode = state.outputMode == Hdr10 ? LinearScRgb : state.outputMode;
    state.supported = hdrAvailable && state.mode != Sdr && sc->format() == desired
        && (state.outputMode != 2 || bool(m_outputPass))
        && d->hasActiveSwapchain && d->hasRenderableSwapchain;
    if (info.limitsType == QRhiSwapChainHdrInfo::ColorComponentValue)
        state.supported = state.supported
            && std::isfinite(info.limits.colorComponentValue.maxColorComponentValue)
            && info.limits.colorComponentValue.maxColorComponentValue > 1.0f;
    if (!d->hasActiveSwapchain || !d->hasRenderableSwapchain)
        m_probeRequested.store(true);
    if (state.mode == 0) {
        m_chromeSynchronized = false;
        requestChrome(false);
    }
    if (info.luminanceBehavior == QRhiSwapChainHdrInfo::SceneReferred
            && std::isfinite(info.sdrWhiteLevel) && info.sdrWhiteLevel >= 80.0f
            && info.sdrWhiteLevel <= 500.0f)
        state.whiteNits = info.sdrWhiteLevel;
    if (waylandOutput.supported)
        state.whiteNits = waylandOutput.whiteNits;
    publish(state);
}
