#pragma once

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <limits>

struct StreamSenderTiming
{
    std::uint64_t generation = 0;
    std::uint64_t sceneEpoch = 0;
    std::uint32_t frameIndex = 0;
    std::uint32_t captureTimeUs = 0;
    std::uint64_t presentationTimeUs = 0;
    bool recognized = false;
    bool paced = false;
};

class StreamSenderPacer
{
public:
    static std::uint64_t clockNs()
    {
        return std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now().time_since_epoch()).count();
    }

    void reset() { *this = {}; }

    std::uint64_t deadline(const StreamSenderTiming &timing, std::uint64_t now)
    {
        const unsigned clock = timing.captureTimeUs != 0 ? 1
            : timing.presentationTimeUs != 0 ? 2 : 0;
        if (!timing.recognized || !timing.paced || timing.generation == 0 || clock == 0) {
            reset();
            return now;
        }
        const auto anchor = [&] {
            m_timing = timing;
            m_clock = clock;
            m_deadline = now;
            return now;
        };
        if (m_clock != clock || m_timing.generation != timing.generation
                || m_timing.sceneEpoch != timing.sceneEpoch)
            return anchor();
        const std::uint32_t frames = timing.frameIndex - m_timing.frameIndex;
        if (frames == 0 || frames >= 0x80000000u) return anchor();
        std::uint64_t deltaUs = 0;
        if (clock == 1) {
            const std::uint32_t delta = timing.captureTimeUs - m_timing.captureTimeUs;
            if (delta == 0 || delta >= 0x80000000u) return anchor();
            deltaUs = delta;
        } else {
            if (timing.presentationTimeUs <= m_timing.presentationTimeUs) return anchor();
            deltaUs = timing.presentationTimeUs - m_timing.presentationTimeUs;
        }
        if (deltaUs > std::numeric_limits<std::uint64_t>::max() / 1000) return anchor();
        const auto deltaNs = deltaUs * 1000;
        if (deltaNs > std::numeric_limits<std::uint64_t>::max() - m_deadline) return anchor();
        const auto target = m_deadline + deltaNs;
        const auto budget = std::min<std::uint64_t>(deltaNs, 1'000'000'000ull / 30) * 2;
        if ((target > now && target - now > budget)
                || (now > target && now - target > budget)) return anchor();
        m_timing = timing;
        m_deadline = target;
        return target;
    }

private:
    StreamSenderTiming m_timing;
    unsigned m_clock = 0;
    std::uint64_t m_deadline = 0;
};
