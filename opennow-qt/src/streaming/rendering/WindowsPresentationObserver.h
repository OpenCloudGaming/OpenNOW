#pragma once

#include "streaming/rendering/StreamPresentationFeedback.h"

#include <QQuickWindow>
#include <rhi/qrhi.h>

#if defined(Q_OS_WIN)
#include <private/qrhid3d11_p.h>
#endif

class WindowsPresentationObserver
{
public:
    void beginFrame(QQuickWindow *window)
    {
        m_source.reset();
        m_presentStartNs = 0;
        m_hasBefore = false;
#if defined(Q_OS_WIN)
        auto *sc = swapChain(window);
        if (!sc || !sc->swapChain) {
            reset();
            return;
        }
        IDXGIOutput *output = nullptr;
        DXGI_OUTPUT_DESC description{};
        if (FAILED(sc->swapChain->GetContainingOutput(&output))) {
            reset();
            return;
        }
        const auto outputStatus = output->GetDesc(&description);
        output->Release();
        const auto hwnd = reinterpret_cast<HWND>(window->winId());
        RECT client{};
        POINT origin{};
        if (FAILED(outputStatus) || !GetClientRect(hwnd, &client) || !ClientToScreen(hwnd, &origin)
                || origin.x < description.DesktopCoordinates.left
                || origin.y < description.DesktopCoordinates.top
                || origin.x + client.right > description.DesktopCoordinates.right
                || origin.y + client.bottom > description.DesktopCoordinates.bottom) {
            reset();
            return;
        }
        const bool noVsync = sc->swapInterval == 0;
        if (m_swapChain != sc->swapChain || m_noVsync != noVsync
                || m_monitor != description.Monitor) {
            reset();
            m_swapChain = sc->swapChain;
            m_noVsync = noVsync;
            m_monitor = description.Monitor;
        }
        UINT before = 0;
        if (SUCCEEDED(sc->swapChain->GetLastPresentCount(&before))) {
            m_before = before;
            m_hasBefore = true;
        }
#else
        Q_UNUSED(window);
#endif
    }

    void source(std::uint64_t generation, std::uint64_t sceneEpoch, std::uint64_t timestampNs,
                std::uint64_t renderStartNs)
    {
        m_source = StreamPresentationFeedback::Source{generation, sceneEpoch, timestampNs, renderStartNs};
    }

    void beforePresent()
    {
        m_presentStartNs = clockNs();
    }

    std::optional<StreamPresentationFeedback::Sample> frameSwapped(QQuickWindow *window)
    {
#if defined(Q_OS_WIN)
        auto *sc = swapChain(window);
        if (!sc || !sc->swapChain || sc->swapChain != m_swapChain
                || (sc->swapInterval == 0) != m_noVsync || !m_hasBefore
                || !m_presentStartNs) {
            reset();
            return {};
        }
        m_hasBefore = false;
        UINT after = 0;
        if (FAILED(sc->swapChain->GetLastPresentCount(&after))) {
            reset();
            return {};
        }
        m_feedback.presented(m_before, after, m_source, m_presentStartNs, m_noVsync);
        m_source.reset();
        DXGI_FRAME_STATISTICS stats{};
        const auto status = sc->swapChain->GetFrameStatistics(&stats);
        if (FAILED(status)) {
            m_feedback.reset();
            return {};
        }
        return m_feedback.observe({stats.PresentCount, stats.PresentRefreshCount,
                                   stats.SyncRefreshCount, ticksToNs(stats.SyncQPCTime.QuadPart)},
                                  clockNs());
#else
        Q_UNUSED(window);
        return {};
#endif
    }

    void reset()
    {
        m_feedback.reset();
        m_source.reset();
        m_presentStartNs = 0;
        m_hasBefore = false;
        m_swapChain = nullptr;
        m_monitor = nullptr;
    }

    static std::uint64_t clockNs()
    {
#if defined(Q_OS_WIN)
        LARGE_INTEGER counter{};
        if (QueryPerformanceCounter(&counter)) return ticksToNs(counter.QuadPart);
#endif
        return 0;
    }

private:
#if defined(Q_OS_WIN)
    static QD3D11SwapChain *swapChain(QQuickWindow *window)
    {
        if (!window || !window->rhi() || window->rhi()->backend() != QRhi::D3D11) return nullptr;
        return static_cast<QD3D11SwapChain *>(window->swapChain());
    }

    static std::uint64_t ticksToNs(std::int64_t ticks)
    {
        static const auto frequency = [] {
            LARGE_INTEGER value{};
            return QueryPerformanceFrequency(&value) ? value.QuadPart : 0;
        }();
        if (ticks <= 0 || frequency <= 0) return 0;
        const auto seconds = std::uint64_t(ticks / frequency);
        if (seconds > UINT64_MAX / 1'000'000'000) return 0;
        const auto fractional = static_cast<long double>(ticks % frequency)
            * 1'000'000'000.0L / static_cast<long double>(frequency);
        return seconds * 1'000'000'000 + std::uint64_t(fractional);
    }
#endif
    StreamPresentationFeedback m_feedback;
    std::optional<StreamPresentationFeedback::Source> m_source;
    void *m_swapChain = nullptr;
    void *m_monitor = nullptr;
    std::uint32_t m_before = 0;
    std::uint64_t m_presentStartNs = 0;
    bool m_hasBefore = false;
    bool m_noVsync = false;
};
