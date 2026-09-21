#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <vector>

class StreamPresentTimings
{
public:
    static constexpr std::size_t WindowCapacity = 256;

    struct Stage
    {
        std::int64_t p50Ns = 0;
        std::int64_t p95Ns = 0;
        std::int64_t maxNs = 0;
    };

    struct Snapshot
    {
        bool available = false;
        Stage submitToSwap;
        std::size_t windowSamples = 0;
        std::uint64_t swappedFramesTotal = 0;
        bool hasLastSwap = false;
        std::int64_t lastSwapNs = 0;
        bool hasPendingSubmit = false;
        std::uint64_t epoch = 0;
        bool gated = false;
        std::uint64_t gateEpoch = 0;
    };

    void markSubmit(std::int64_t nowNs)
    {
        const std::lock_guard lock(m_mutex);
        if (m_gated) return;
        m_submitNs = nowNs;
        m_hasPendingSubmit = true;
    }

    void markSwap(std::int64_t nowNs)
    {
        const std::lock_guard lock(m_mutex);
        if (m_gated || !m_hasPendingSubmit) return;
        m_hasPendingSubmit = false;
        const std::int64_t delta = nowNs > m_submitNs ? nowNs - m_submitNs : 0;
        m_samples[m_sampleCount % WindowCapacity] = delta;
        ++m_sampleCount;
        m_windowSamples = std::min(m_sampleCount, WindowCapacity);
        ++m_swappedFramesTotal;
        m_lastSwapNs = nowNs;
        m_hasLastSwap = true;
    }

    Snapshot snapshot() const
    {
        const std::lock_guard lock(m_mutex);
        Snapshot result;
        result.windowSamples = m_windowSamples;
        result.swappedFramesTotal = m_swappedFramesTotal;
        result.hasLastSwap = m_hasLastSwap;
        result.lastSwapNs = m_lastSwapNs;
        result.hasPendingSubmit = m_hasPendingSubmit;
        result.epoch = m_epoch;
        result.gated = m_gated;
        result.gateEpoch = m_gateEpoch;
        if (m_windowSamples == 0) return result;
        std::vector<std::int64_t> sorted(m_samples.begin(), m_samples.begin() + m_windowSamples);
        std::sort(sorted.begin(), sorted.end());
        result.available = true;
        result.submitToSwap.p50Ns = percentile(sorted, 50);
        result.submitToSwap.p95Ns = percentile(sorted, 95);
        result.submitToSwap.maxNs = sorted.back();
        return result;
    }

    void setGated(bool gated)
    {
        const std::lock_guard lock(m_mutex);
        if (gated && !m_gated) ++m_gateEpoch;
        m_gated = gated;
        if (gated) m_hasPendingSubmit = false;
    }

    void discardPending()
    {
        const std::lock_guard lock(m_mutex);
        m_hasPendingSubmit = false;
    }

    void reset()
    {
        const std::lock_guard lock(m_mutex);
        m_hasPendingSubmit = false;
        m_sampleCount = 0;
        m_windowSamples = 0;
        ++m_epoch;
    }

private:
    static std::int64_t percentile(const std::vector<std::int64_t> &sorted, std::size_t percent)
    {
        const std::size_t rank = std::max<std::size_t>(1, (sorted.size() * percent + 99) / 100);
        return sorted[std::min(rank, sorted.size()) - 1];
    }

    mutable std::mutex m_mutex;
    std::vector<std::int64_t> m_samples = std::vector<std::int64_t>(WindowCapacity);
    std::size_t m_sampleCount = 0;
    std::size_t m_windowSamples = 0;
    std::uint64_t m_swappedFramesTotal = 0;
    std::int64_t m_submitNs = 0;
    bool m_hasPendingSubmit = false;
    std::int64_t m_lastSwapNs = 0;
    bool m_hasLastSwap = false;
    bool m_gated = false;
    std::uint64_t m_gateEpoch = 0;
    std::uint64_t m_epoch = 0;
};
