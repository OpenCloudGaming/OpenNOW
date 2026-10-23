#pragma once

#include <cstdint>
#include <cstddef>
#include <deque>
#include <optional>
#include <algorithm>
#include <iterator>

class StreamPresentationFeedback
{
public:
    static constexpr std::size_t Capacity = 64;
    static constexpr std::uint64_t MaximumAgeNs = 1'000'000'000;

    struct Source {
        std::uint64_t generation = 0;
        std::uint64_t sceneEpoch = 0;
        std::uint64_t timestampNs = 0;
        std::uint64_t renderStartNs = 0;
    };

    struct Statistics {
        std::uint32_t presentCount = 0;
        std::uint32_t presentRefreshCount = 0;
        std::uint32_t syncRefreshCount = 0;
        std::uint64_t syncTimeNs = 0;
    };

    struct Sample {
        Source source;
        std::uint64_t renderDurationNs = 0;
        std::uint64_t presentDurationNs = 0;
        std::uint64_t completionAgeNs = 0;
        std::uint64_t vsyncIntervalNs = 0;
    };

    void presented(std::uint32_t before, std::uint32_t after,
                   std::optional<Source> source, std::uint64_t presentStartNs,
                   bool vsyncDisabled)
    {
        if (std::uint32_t(after - before) != 1) {
            reset();
            return;
        }
        if (source && (!source->generation || !source->timestampNs
                       || !source->renderStartNs || presentStartNs < source->renderStartNs))
            source.reset();
        if (m_pending.size() == Capacity) m_pending.pop_front();
        m_pending.push_back({after, source, presentStartNs, vsyncDisabled});
    }

    std::optional<Sample> observe(const Statistics &stats, std::uint64_t nowNs)
    {
        if (!stats.syncTimeNs || stats.syncTimeNs > nowNs) {
            reset();
            return {};
        }
        std::optional<std::uint64_t> interval;
        if (m_previousSync && stats.syncTimeNs > m_previousSync->syncTimeNs
                && std::uint32_t(stats.syncRefreshCount - m_previousSync->syncRefreshCount) == 1) {
            const auto delta = stats.syncTimeNs - m_previousSync->syncTimeNs;
            if (delta >= 1'000'000 && delta <= 100'000'000) interval = delta;
        }
        if (!m_previousSync || stats.syncTimeNs > m_previousSync->syncTimeNs)
            m_previousSync = stats;
        auto found = std::find_if(m_pending.begin(), m_pending.end(), [&](const Entry &entry) {
            return entry.presentCount == stats.presentCount;
        });
        if (found == m_pending.end()) return {};
        const auto entry = *found;
        m_pending.erase(m_pending.begin(), std::next(found));
        if (!entry.source || stats.presentRefreshCount != stats.syncRefreshCount
                || stats.syncTimeNs < entry.presentStartNs
                || nowNs - stats.syncTimeNs > MaximumAgeNs
                || stats.syncTimeNs - entry.presentStartNs > MaximumAgeNs
                || (!entry.vsyncDisabled && !interval)) return {};
        return Sample{*entry.source,
                      entry.presentStartNs - entry.source->renderStartNs,
                      stats.syncTimeNs - entry.presentStartNs,
                      nowNs - stats.syncTimeNs,
                      entry.vsyncDisabled ? 0 : *interval};
    }

    void reset()
    {
        m_pending.clear();
        m_previousSync.reset();
    }

    std::size_t pending() const { return m_pending.size(); }

private:
    struct Entry {
        std::uint32_t presentCount;
        std::optional<Source> source;
        std::uint64_t presentStartNs;
        bool vsyncDisabled;
    };
    std::deque<Entry> m_pending;
    std::optional<Statistics> m_previousSync;
};
