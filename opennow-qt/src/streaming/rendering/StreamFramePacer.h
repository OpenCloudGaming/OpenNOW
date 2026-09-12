#pragma once

#include <cmath>
#include <cstdint>
#include <algorithm>
#include <array>

class StreamFramePacer
{
public:
    enum class Result { Duplicate, WarmingUp, Interpolate, Discontinuity, DisplayTooSlow, Overloaded, SourceRateLimit };
    enum class TimingSource { None, SourceTimestamps, ArrivalCadence };
    enum class Rejection { None, SequenceGap, TimestampRegression, TimestampJump, ArrivalGap, CadenceUnavailable };

    Result source(std::uint64_t sequence, std::uint64_t timestamp,
                  std::int64_t now, double refreshRate)
    {
        if (m_hasSource && sequence == m_sequence && timestamp == m_timestamp)
            return Result::Duplicate;
        const bool consecutive = sequence == m_sequence + 1;
        m_sequenceDelta = sequence >= m_sequence ? sequence - m_sequence : 0;
        const bool rewound = timestamp && m_timestamp && timestamp < m_timestamp;
        m_timestampDelta = timestamp >= m_timestamp ? timestamp - m_timestamp : 0;
        m_arrivalDelta = now >= m_arrival ? now - m_arrival : 0;
        const bool jumped = m_timestamp && timestamp && m_timestampDelta > 250'000'000;
        const auto gapLimit = m_interval ? std::max<std::int64_t>(50'000'000, m_interval * 2) : 75'000'000;
        const bool pending = m_pending;
        const bool first = !m_hasSource;
        m_hasSource = true;
        m_sequence = sequence;
        m_timestamp = timestamp;
        m_arrival = now;
        m_pending = false;
        m_rejection = Rejection::None;
        m_timingSource = TimingSource::None;
        if (first) {
            m_sequenceDelta = 0;
            m_timestampDelta = 0;
            m_arrivalDelta = 0;
            return Result::WarmingUp;
        }
        const auto reject = [this](Rejection reason) {
            m_rejection = reason;
            m_interval = 0;
            m_arrivalCount = 0;
            m_arrivalSlot = 0;
            return Result::Discontinuity;
        };
        if (!consecutive) return reject(Rejection::SequenceGap);
        if (rewound) return reject(Rejection::TimestampRegression);
        if (jumped) return reject(Rejection::TimestampJump);
        if (m_arrivalDelta > gapLimit) return reject(Rejection::ArrivalGap);
        m_arrivals[m_arrivalSlot] = m_arrivalDelta;
        m_arrivalSlot = (m_arrivalSlot + 1) % m_arrivals.size();
        m_arrivalCount = std::min(m_arrivalCount + 1, m_arrivals.size());
        auto sorted = m_arrivals;
        std::sort(sorted.begin(), sorted.begin() + m_arrivalCount);
        const auto observed = m_arrivalCount
            ? (sorted[(m_arrivalCount - 1) / 2] + sorted[m_arrivalCount / 2]) / 2 : 0;
        const bool arrivalUsable = observed >= 8'000'000 && observed <= 40'000'000;
        const bool sourceUsable = m_timestampDelta >= 8'000'000 && m_timestampDelta <= 40'000'000
            && (!arrivalUsable || (m_timestampDelta * 4 >= std::uint64_t(observed) * 3
                                  && m_timestampDelta * 4 <= std::uint64_t(observed) * 5));
        if (sourceUsable) {
            m_interval = m_timestampDelta;
            m_timingSource = TimingSource::SourceTimestamps;
        } else if (arrivalUsable) {
            m_interval = observed;
            m_timingSource = TimingSource::ArrivalCadence;
        } else {
            m_interval = 0;
            m_rejection = Rejection::CadenceUnavailable;
            return Result::Discontinuity;
        }
        if (m_interval < 16'000'000) return Result::SourceRateLimit;
        if (!std::isfinite(refreshRate) || refreshRate < 1.95e9 / double(m_interval))
            return Result::DisplayTooSlow;
        if (pending) m_cooldownUntil = now + 2'000'000'000;
        if (now < m_cooldownUntil) return Result::Overloaded;
        return Result::Interpolate;
    }

    void midpoint(std::int64_t now)
    {
        m_pending = true;
        m_originalDue = now + std::int64_t(m_interval / 2);
    }

    bool takeOriginal(std::int64_t now, double refreshRate)
    {
        if (!m_pending) return false;
        const auto lead = refreshRate > 0 ? std::int64_t(1.5e9 / refreshRate) : 0;
        if (now + lead < m_originalDue) return false;
        m_pending = false;
        return true;
    }

    bool pending() const { return m_pending; }
    TimingSource timingSource() const { return m_timingSource; }
    Rejection rejection() const { return m_rejection; }
    std::uint64_t interval() const { return m_interval; }
    std::uint64_t timestampDelta() const { return m_timestampDelta; }
    std::int64_t arrivalDelta() const { return m_arrivalDelta; }
    std::uint64_t sequenceDelta() const { return m_sequenceDelta; }
    void reset() { *this = {}; }

private:
    std::uint64_t m_sequence = 0;
    std::uint64_t m_timestamp = 0;
    std::uint64_t m_interval = 0;
    std::uint64_t m_timestampDelta = 0;
    std::uint64_t m_sequenceDelta = 0;
    std::int64_t m_arrivalDelta = 0;
    std::array<std::int64_t, 8> m_arrivals{};
    std::size_t m_arrivalCount = 0;
    std::size_t m_arrivalSlot = 0;
    TimingSource m_timingSource = TimingSource::None;
    Rejection m_rejection = Rejection::None;
    std::int64_t m_arrival = 0;
    std::int64_t m_originalDue = 0;
    std::int64_t m_cooldownUntil = 0;
    bool m_hasSource = false;
    bool m_pending = false;
};
