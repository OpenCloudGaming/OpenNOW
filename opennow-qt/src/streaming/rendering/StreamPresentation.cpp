#include "streaming/rendering/StreamPresentation.h"
#include "streaming/rendering/StreamPresentationPolicy.h"
#include "streaming/NativeStreamRuntime.h"

#include <QEvent>
#include <QPointer>
#include <QQuickWindow>
#include <QScreen>
#include <atomic>
#include <chrono>
#include <optional>

#if defined(Q_OS_WIN)
#define OPENNOW_WINDOWS_PRESENTATION
#include <QLibrary>
#include <private/qquickwindow_p.h>
#include <private/qrhid3d11_p.h>
#include <dxgi1_6.h>
#include <wrl/client.h>
#endif

struct StreamPresentation::State
{
    NativeStreamRuntime *runtime = nullptr;
    QPointer<QQuickWindow> window;
    std::atomic<unsigned> windowState{0};
    std::atomic<std::uintptr_t> monitor{0};
    std::atomic<bool> probeRequested{true};

#ifdef OPENNOW_WINDOWS_PRESENTATION
    using Clock = std::chrono::steady_clock;
    template<typename T> using ComPtr = Microsoft::WRL::ComPtr<T>;
    struct VrrInfo {
        std::uint32_t version;
        std::uint32_t flags;
        std::uint32_t reserved[4];
    };
    static_assert(sizeof(VrrInfo) == 24);
    using QueryInterface = void *(__cdecl *)(std::uint32_t);
    using Initialize = int (__cdecl *)();
    using DisplayId = int (__cdecl *)(const char *, std::uint32_t *);
    using GetVrrInfo = int (__cdecl *)(std::uint32_t, VrrInfo *);
    QLibrary nvapi;
    bool nvapiAttempted = false;
    DisplayId displayId = nullptr;
    GetVrrInfo getVrrInfo = nullptr;
    ComPtr<IDXGIDevice1> device;
    ID3D11Device *deviceIdentity = nullptr;
    bool deviceLimitOwned = false;
    UINT previousDeviceLimit = 0;
    bool tearingSupported = false;
    bool vrrPossible = false;
    bool vrrRequested = false;
    std::optional<unsigned> reportedEligibility;
    Clock::time_point nextProbe{};
    QRhiSwapChain *swapChain = nullptr;
    bool baselineNoVsync = false;
    bool flagsOwned = false;
    bool vrrFailed = false;
    bool recreationFailed = false;
    bool latencyFailureReported = false;
    std::uint64_t generation = 0;
    IDXGISwapChain *limitedSwapChain = nullptr;
    HANDLE limitedWaitable = nullptr;
    UINT previousSwapChainLimit = 0;
    bool swapChainLimitOwned = false;

    bool loadNvapi()
    {
        if (nvapiAttempted) return displayId && getVrrInfo;
        nvapiAttempted = true;
        wchar_t directory[MAX_PATH]{};
        const UINT length = GetSystemDirectoryW(directory, MAX_PATH);
        if (length == 0 || length >= MAX_PATH) return false;
        nvapi.setFileName(QString::fromWCharArray(directory, int(length))
            + (sizeof(void *) == 8 ? QStringLiteral("\\nvapi64.dll") : QStringLiteral("\\nvapi.dll")));
        nvapi.setLoadHints(QLibrary::PreventUnloadHint);
        const auto query = reinterpret_cast<QueryInterface>(nvapi.resolve("nvapi_QueryInterface"));
        if (!query) return false;
        const auto initialize = reinterpret_cast<Initialize>(query(0x0150e828));
        displayId = reinterpret_cast<DisplayId>(query(0xae457190));
        getVrrInfo = reinterpret_cast<GetVrrInfo>(query(0xdf8fda57));
        if (!initialize || !displayId || !getVrrInfo || initialize() != 0) {
            displayId = nullptr;
            getVrrInfo = nullptr;
            return false;
        }
        return true;
    }

    void restoreDeviceLimit()
    {
        if (device && deviceLimitOwned) {
            UINT current = 0;
            if (SUCCEEDED(device->GetMaximumFrameLatency(&current)) && current == 1
                    && FAILED(device->SetMaximumFrameLatency(previousDeviceLimit)))
                qWarning("Could not restore the DXGI device frame latency.");
        }
        deviceLimitOwned = false;
    }

    bool limitDevice()
    {
        if (!device) return false;
        if (deviceLimitOwned) return true;
        if (FAILED(device->GetMaximumFrameLatency(&previousDeviceLimit))
                || FAILED(device->SetMaximumFrameLatency(1))) return false;
        deviceLimitOwned = true;
        return true;
    }

    void restoreSwapChainLimit(QD3D11SwapChain *native)
    {
        if (native && native->swapChain == limitedSwapChain
                && native->frameLatencyWaitableObject == limitedWaitable && swapChainLimitOwned) {
            ComPtr<IDXGISwapChain2> sc;
            UINT current = 0;
            if (SUCCEEDED(native->swapChain->QueryInterface(IID_PPV_ARGS(&sc)))
                    && SUCCEEDED(sc->GetMaximumFrameLatency(&current)) && current == 1
                    && FAILED(sc->SetMaximumFrameLatency(previousSwapChainLimit)))
                qWarning("Could not restore the DXGI swapchain frame latency.");
        }
        limitedSwapChain = nullptr;
        limitedWaitable = nullptr;
        swapChainLimitOwned = false;
    }

    bool limitSwapChain(QD3D11SwapChain *native)
    {
        if (limitedSwapChain != native->swapChain || limitedWaitable != native->frameLatencyWaitableObject) {
            limitedSwapChain = native->swapChain;
            limitedWaitable = native->frameLatencyWaitableObject;
            swapChainLimitOwned = false;
        }
        if (swapChainLimitOwned && native->frameCount != 0) return true;
        ComPtr<IDXGISwapChain2> sc;
        UINT current = 0;
        if (!native->swapChain || FAILED(native->swapChain->QueryInterface(IID_PPV_ARGS(&sc)))
                || FAILED(sc->GetMaximumFrameLatency(&current))) return false;
        if (swapChainLimitOwned && current == 1) return true;
        previousSwapChainLimit = current;
        if (FAILED(sc->SetMaximumFrameLatency(1))) return false;
        swapChainLimitOwned = true;
        return true;
    }

    void setDevice(ID3D11Device *native)
    {
        if (native == deviceIdentity) return;
        ComPtr<IDXGIDevice1> next;
        if (native) native->QueryInterface(IID_PPV_ARGS(&next));
        if (next.Get() == device.Get()) return;
        restoreDeviceLimit();
        deviceIdentity = native;
        device = std::move(next);
        latencyFailureReported = false;
        tearingSupported = false;
        vrrPossible = false;
        vrrRequested = false;
        if (device) {
            ComPtr<IDXGIAdapter> adapter;
            ComPtr<IDXGIFactory5> factory;
            DXGI_ADAPTER_DESC desc{};
            BOOL tearing = FALSE;
            if (SUCCEEDED(device->GetAdapter(&adapter))
                    && SUCCEEDED(adapter->GetDesc(&desc)) && desc.VendorId == 0x10de
                    && SUCCEEDED(adapter->GetParent(IID_PPV_ARGS(&factory)))
                    && SUCCEEDED(factory->CheckFeatureSupport(DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                        &tearing, sizeof(tearing))))
                tearingSupported = tearing;
        }
        probeRequested.store(true);
    }

    void probe(QD3D11SwapChain *native)
    {
        vrrPossible = false;
        vrrRequested = false;
        if (!tearingSupported || !native->swapChain || !loadNvapi()) return;
        ComPtr<IDXGIOutput> output;
        DXGI_OUTPUT_DESC desc{};
        if (FAILED(native->swapChain->GetContainingOutput(&output))
                || FAILED(output->GetDesc(&desc))
                || reinterpret_cast<std::uintptr_t>(desc.Monitor) != monitor.load()) return;
        const auto name = QString::fromWCharArray(desc.DeviceName).toLatin1();
        std::uint32_t id = 0;
        VrrInfo info{};
        info.version = std::uint32_t(sizeof(info)) | (1u << 16);
        if (displayId(name.constData(), &id) != 0 || getVrrInfo(id, &info) != 0) return;
        const auto policy = streamPresentationVrrPolicy(info.flags);
        vrrPossible = policy.possible;
        vrrRequested = policy.requested;
    }

    void reset(QRhiSwapChain *current = nullptr)
    {
        if (current && current == swapChain) {
            auto *native = static_cast<QD3D11SwapChain *>(current);
            restoreSwapChainLimit(native);
            if (flagsOwned) {
                auto flags = current->flags();
                flags.setFlag(QRhiSwapChain::NoVSync, baselineNoVsync);
                current->setFlags(flags);
            }
        }
        restoreDeviceLimit();
        device.Reset();
        deviceIdentity = nullptr;
        swapChain = nullptr;
        limitedSwapChain = nullptr;
        limitedWaitable = nullptr;
        swapChainLimitOwned = false;
        flagsOwned = false;
        vrrFailed = false;
        recreationFailed = false;
        latencyFailureReported = false;
        vrrPossible = false;
        vrrRequested = false;
        reportedEligibility.reset();
        probeRequested.store(true);
    }

    ~State() { restoreDeviceLimit(); }
#endif
};

StreamPresentation::StreamPresentation(NativeStreamRuntime *runtime, QObject *parent)
    : QObject(parent), d(std::make_unique<State>())
{
    d->runtime = runtime;
    if (runtime) {
        connect(runtime, &NativeStreamRuntime::streamingChanged, this, [this] {
            d->probeRequested.store(true);
            if (d->window) d->window->update();
        });
    }
}

StreamPresentation::~StreamPresentation() = default;

void StreamPresentation::attach(QQuickWindow *window)
{
    if (!window || d->window) return;
    d->window = window;
    window->installEventFilter(this);
    const auto changed = [this] { snapshotWindow(); };
    connect(window, &QWindow::activeChanged, this, changed);
    connect(window, &QWindow::visibilityChanged, this, changed);
    connect(window, &QWindow::screenChanged, this, changed);
    connect(window, &QWindow::xChanged, this, changed);
    connect(window, &QWindow::yChanged, this, changed);
    connect(window, &QWindow::widthChanged, this, changed);
    connect(window, &QWindow::heightChanged, this, changed);
    connect(window, &QQuickWindow::beforeFrameBegin, this,
        &StreamPresentation::updateOutput, Qt::DirectConnection);
    connect(window, &QQuickWindow::sceneGraphInvalidated, this, [this] {
#ifdef OPENNOW_WINDOWS_PRESENTATION
        auto *current = d->window && d->window->rendererInterface()->graphicsApi() == QSGRendererInterface::Direct3D11
            ? QQuickWindowPrivate::get(d->window)->swapchain : nullptr;
        d->reset(current);
#endif
    }, Qt::DirectConnection);
    snapshotWindow();
}

bool StreamPresentation::eventFilter(QObject *object, QEvent *event)
{
    if (object == d->window && (event->type() == QEvent::Expose
            || event->type() == QEvent::WindowStateChange
            || event->type() == QEvent::ScreenChangeInternal))
        snapshotWindow();
    return QObject::eventFilter(object, event);
}

void StreamPresentation::snapshotWindow()
{
    if (!d->window) return;
    const unsigned snapshot = (d->window->isActive() ? 1u : 0u)
        | (d->window->isExposed() && d->window->isVisible() ? 2u : 0u)
        | (d->window->visibility() == QWindow::FullScreen ? 4u : 0u);
#ifdef OPENNOW_WINDOWS_PRESENTATION
    const auto monitor = d->window->handle()
        ? MonitorFromWindow(reinterpret_cast<HWND>(d->window->winId()), MONITOR_DEFAULTTONULL) : nullptr;
    d->monitor.store(reinterpret_cast<std::uintptr_t>(monitor));
#endif
    d->windowState.store(snapshot);
    d->probeRequested.store(true);
    d->window->update();
}

void StreamPresentation::updateOutput()
{
#ifdef OPENNOW_WINDOWS_PRESENTATION
    if (!d->window || !d->runtime
            || d->window->rendererInterface()->graphicsApi() != QSGRendererInterface::Direct3D11) return;
    auto *wd = QQuickWindowPrivate::get(d->window);
    auto *sc = wd->swapchain;
    if (!sc || !wd->rhi || !wd->hasActiveSwapchain || !wd->hasRenderableSwapchain) return;
    auto *native = static_cast<QD3D11SwapChain *>(sc);
    auto *device = static_cast<ID3D11Device *>(d->window->rendererInterface()->getResource(
        d->window, QSGRendererInterface::DeviceResource));
    d->setDevice(device);
    if (d->swapChain != sc) {
        d->swapChain = sc;
        d->baselineNoVsync = sc->flags().testFlag(QRhiSwapChain::NoVSync);
        d->flagsOwned = false;
        d->vrrFailed = false;
        d->recreationFailed = false;
        d->probeRequested.store(true);
    }
    const auto config = d->runtime->presentationConfig();
    const bool streaming = config.active && d->runtime->presentationAllowed();
    if (d->generation != config.generation) {
        d->generation = config.generation;
        d->vrrFailed = false;
        d->recreationFailed = false;
        d->probeRequested.store(true);
    }
    const auto snapshot = d->windowState.load();
    const auto now = State::Clock::now();
    if (d->probeRequested.exchange(false)) {
        d->vrrPossible = false;
        d->vrrRequested = false;
        d->vrrFailed = false;
        d->recreationFailed = false;
        d->latencyFailureReported = false;
        d->nextProbe = {};
    }
    if (streaming && config.cloud_gsync && (snapshot & 7u) == 7u && now >= d->nextProbe) {
        d->probe(native);
        d->nextProbe = now + std::chrono::seconds(2);
    }
    const auto mode = streamPresentationMode({streaming, bool(config.cloud_gsync),
        bool(snapshot & 1u), bool(snapshot & 2u), bool(snapshot & 4u),
        d->tearingSupported && !d->vrrFailed, d->vrrPossible, d->vrrRequested});
    if (streaming && config.cloud_gsync) {
        const unsigned eligibility = (snapshot & 7u) | (unsigned(d->tearingSupported) << 3)
            | (unsigned(d->vrrPossible) << 4) | (unsigned(d->vrrRequested) << 5);
        if (d->reportedEligibility != eligibility) {
            d->reportedEligibility = eligibility;
            qInfo("Cloud G-Sync eligibility: focused=%d exposed=%d fullscreen=%d tearing=%d possible=%d requested=%d; scanout unverified.",
                bool(snapshot & 1u), bool(snapshot & 2u), bool(snapshot & 4u),
                d->tearingSupported, d->vrrPossible, d->vrrRequested);
        }
    } else {
        d->reportedEligibility.reset();
    }
    bool noVsync = streamPresentationNoVsync(mode, d->baselineNoVsync);
    if (mode == StreamPresentationMode::Vrr && !d->limitDevice()) {
        d->vrrFailed = true;
        noVsync = false;
        qWarning("Cloud G-Sync requires a bounded DXGI device queue; retaining VSync.");
    }
    if (!d->recreationFailed && sc->flags().testFlag(QRhiSwapChain::NoVSync) != noVsync) {
        d->restoreSwapChainLimit(native);
        const auto format = native->colorFormat;
        wd->rhi->finish();
        auto flags = sc->flags();
        flags.setFlag(QRhiSwapChain::NoVSync, noVsync);
        const auto result = recreateStreamPresentationSwapChain(*sc, flags,
            [&] { return native->colorFormat == format; });
        wd->hasActiveSwapchain = result.created;
        wd->hasRenderableSwapchain = result.created;
        wd->swapchainJustBecameRenderable = !result.created;
        if (!result.applied) {
            d->vrrFailed = true;
            d->recreationFailed = true;
            qWarning("Stream presentation mode recreation failed; previous output restored=%s.",
                result.created ? "true" : "false");
        } else {
            d->flagsOwned = mode != StreamPresentationMode::Baseline;
            qInfo("Stream presentation: %s, hardware timed pacing=unavailable.",
                noVsync ? "VSync off with tearing; VRR scanout unverified" : "VSync on");
        }
        if (!result.created) return;
    }
    if (!streaming) {
        d->restoreSwapChainLimit(native);
        d->restoreDeviceLimit();
        return;
    }
    if (native->frameLatencyWaitableObject) {
        d->restoreDeviceLimit();
        if (!d->limitSwapChain(native) && !d->latencyFailureReported) {
            d->latencyFailureReported = true;
            qWarning("Could not set the waitable DXGI swapchain frame latency to one.");
        }
    } else if (!d->limitDevice() && !d->latencyFailureReported) {
        d->latencyFailureReported = true;
        qWarning("Could not set the nonwaitable DXGI device frame latency to one.");
    }
#endif
}
