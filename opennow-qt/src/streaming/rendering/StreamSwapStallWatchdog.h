#pragma once

#include <algorithm>
#include <chrono>
#include <cstdint>

inline std::int64_t streamMonotonicClockNs()
{
    return std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::steady_clock::now().time_since_epoch()).count();
}

class StreamSwapStallWatchdog
{
public:
    struct Policy
    {
        std::int64_t stallNs = 8'000'000'000;
        std::int64_t rearmGraceNs = 2'000'000'000;
    };

    enum class Outcome
    {
        None,
        ResourceRearm,
        Unrecovered,
    };

    struct Observation
    {
        bool gated = false;
        std::uint64_t gateEpoch = 0;
        bool hasPendingSubmit = false;
        bool hasLastSwap = false;
        std::int64_t lastSwapNs = 0;
        bool upstreamStalled = false;
        bool hasUpstreamSample = false;
        std::uint64_t upstreamEpoch = 0;
        std::uint64_t upstreamOutputsTotal = 0;
    };

    void setPolicy(const Policy &policy)
    {
        m_policy = policy;
    }

    Outcome observe(const Observation &observation, std::int64_t nowNs)
    {
        if (observation.gateEpoch != m_gateEpoch) {
            reset();
            m_gateEpoch = observation.gateEpoch;
        }
        if (observation.gated || observation.upstreamStalled) {
            reset();
            return Outcome::None;
        }
        if (m_stage == Stage::Rearmed || m_stage == Stage::Unrecovered) {
            if (observation.hasLastSwap && observation.lastSwapNs > m_episodeStartNs) {
                m_stage = Stage::Tracking;
                m_episodeStartNs = 0;
                m_hasBaseline = false;
                return Outcome::None;
            }
            if (m_stage == Stage::Unrecovered) return Outcome::None;
            if (!observation.hasUpstreamSample) return Outcome::None;
            if (nowNs - m_episodeStartNs < m_policy.rearmGraceNs) return Outcome::None;
            m_stage = Stage::Unrecovered;
            ++m_unrecoveredCount;
            return Outcome::Unrecovered;
        }
        if (!observation.hasPendingSubmit) {
            reset();
            return Outcome::None;
        }
        if (m_episodeStartNs == 0) {
            m_episodeStartNs = nowNs;
            m_hasBaseline = false;
        }
        if (!observation.hasUpstreamSample) return Outcome::None;
        if (!m_hasBaseline) {
            seedBaseline(observation);
            return Outcome::None;
        }
        if (observation.upstreamEpoch != m_baselineEpoch) {
            seedBaseline(observation);
            return Outcome::None;
        }
        if (observation.hasLastSwap && observation.lastSwapNs > m_baselineSwapNs) {
            seedBaseline(observation);
            return Outcome::None;
        }
        if (observation.upstreamOutputsTotal <= m_baselineOutputs) return Outcome::None;
        const std::int64_t reference = observation.hasLastSwap
            ? std::max(observation.lastSwapNs, m_episodeStartNs)
            : m_episodeStartNs;
        const std::int64_t idleNs = nowNs > reference ? nowNs - reference : 0;
        if (idleNs < m_policy.stallNs) return Outcome::None;
        m_stage = Stage::Rearmed;
        m_episodeStartNs = nowNs;
        m_hasBaseline = false;
        ++m_rearmCount;
        return Outcome::ResourceRearm;
    }

    void onResourcesReleased(bool ownRearm, bool deviceChanged, bool generationChanged)
    {
        if (ownRearm && !deviceChanged && !generationChanged && m_stage == Stage::Rearmed) return;
        reset();
    }

    void reset()
    {
        m_stage = Stage::Tracking;
        m_episodeStartNs = 0;
        m_hasBaseline = false;
        m_baselineEpoch = 0;
        m_baselineOutputs = 0;
        m_baselineSwapNs = 0;
    }

    std::uint64_t rearmCount() const
    {
        return m_rearmCount;
    }

    std::uint64_t unrecoveredCount() const
    {
        return m_unrecoveredCount;
    }

private:
    enum class Stage
    {
        Tracking,
        Rearmed,
        Unrecovered,
    };

    void seedBaseline(const Observation &observation)
    {
        m_hasBaseline = true;
        m_baselineEpoch = observation.upstreamEpoch;
        m_baselineOutputs = observation.upstreamOutputsTotal;
        m_baselineSwapNs = observation.hasLastSwap ? observation.lastSwapNs : 0;
    }

    Policy m_policy;
    std::uint64_t m_gateEpoch = 0;
    Stage m_stage = Stage::Tracking;
    std::int64_t m_episodeStartNs = 0;
    bool m_hasBaseline = false;
    std::uint64_t m_baselineEpoch = 0;
    std::uint64_t m_baselineOutputs = 0;
    std::int64_t m_baselineSwapNs = 0;
    std::uint64_t m_rearmCount = 0;
    std::uint64_t m_unrecoveredCount = 0;
};
