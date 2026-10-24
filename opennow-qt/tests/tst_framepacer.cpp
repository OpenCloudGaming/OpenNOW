#include "streaming/rendering/StreamFramePacer.h"

#include <QTest>
#include <limits>
#include <tuple>

class FramePacerTest : public QObject
{
    Q_OBJECT
private slots:
    void insertsExactlyOneMidpoint()
    {
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        QCOMPARE(pacer.source(40, 0, 0, 120), Result::WarmingUp);
        QCOMPARE(pacer.source(41, 16'666'667, 16'666'667, 120), Result::Interpolate);
        pacer.midpoint(16'666'667);
        QVERIFY(pacer.pending());
        QCOMPARE(pacer.source(41, 16'666'667, 20'000'000, 120), Result::Duplicate);
        QVERIFY(pacer.takeOriginal(24'000'000, 120));
        QVERIFY(!pacer.pending());
        QVERIFY(!pacer.takeOriginal(25'000'000, 120));
        QCOMPARE(pacer.source(42, 33'333'334, 33'333'334, 120), Result::Interpolate);
    }

    void rejectsMissingOrDiscontinuousSources()
    {
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        pacer.source(1, 16'666'667, 0, 120);
        QCOMPARE(pacer.source(3, 50'000'000, 33'333'333, 120), Result::Discontinuity);
        QCOMPARE(pacer.source(4, 40'000'000, 40'000'000, 120), Result::Discontinuity);
        QCOMPARE(pacer.source(5, 56'666'667, 200'000'000, 120), Result::Discontinuity);
        pacer.reset();
        QCOMPARE(pacer.source(1, 16'666'667, 0, 120), Result::WarmingUp);
        QCOMPARE(pacer.source(2, std::numeric_limits<std::uint64_t>::max(), 1, 120), Result::Discontinuity);
    }

    void requiresEnoughDisplayRefresh()
    {
        using Result = StreamFramePacer::Result;
        for (double refresh : {0.0, 60.0, 90.0, std::numeric_limits<double>::quiet_NaN()}) {
            StreamFramePacer pacer;
            pacer.source(1, 0, 0, refresh);
            QCOMPARE(pacer.source(2, 16'666'667, 16'666'667, refresh), Result::DisplayTooSlow);
        }
        StreamFramePacer pacer;
        pacer.source(1, 0, 0, 59.94);
        QCOMPARE(pacer.source(2, 33'366'700, 33'366'700, 59.94), Result::Interpolate);
    }

    void overloadDoesNotBuildAQueue()
    {
        using Result = StreamFramePacer::Result;
        StreamFramePacer pacer;
        pacer.source(1, 0, 0, 120);
        pacer.source(2, 16'666'667, 16'666'667, 120);
        pacer.midpoint(16'666'667);
        // An undrained pair while the presenter has stopped swapping is overload: the
        // schedule is dropped and generation cools down for two seconds.
        QCOMPARE(pacer.source(3, 33'333'334, 33'333'334, 120, 2, true), Result::Overloaded);
        QVERIFY(!pacer.pending());
        QCOMPARE(pacer.source(4, 50'000'001, 50'000'001, 120), Result::Overloaded);
        for (std::uint64_t frame = 5; frame < 130; ++frame)
            pacer.source(frame, (frame - 1) * 16'666'667, (frame - 1) * 16'666'667, 120);
        QCOMPARE(pacer.source(130, 129 * 16'666'667ULL, 129 * 16'666'667LL, 120), Result::Interpolate);
    }

    void burstsDropTheStaleScheduleWithoutCoolingDown()
    {
        // The presenter takes the latest decoded frame and the decoder delivers in bursts:
        // a source frame that lands on an unfinished pair while frames still reach the
        // display is not overload. The stale schedule is dropped and the new pair runs.
        using Result = StreamFramePacer::Result;
        StreamFramePacer pacer;
        std::int64_t now = 0;
        pacer.source(1, 0, now, 240);
        for (std::uint64_t frame = 2; frame <= 60; ++frame) {
            now += frame % 2 ? 31'333'334 : 2'000'000;
            const auto result = pacer.source(frame, 0, now, 240);
            if (frame >= 10) {
                QCOMPARE(result, Result::Interpolate);
                QCOMPARE(pacer.interval(), 16'666'667ULL);
            }
            pacer.midpoint(now);
            QVERIFY(pacer.pending());
        }
    }

    void oneSkippedSourceFrameKeepsGenerating()
    {
        // The presenter acquires the latest decoded frame, so a burst skips one source
        // frame: the pair around it still interpolates (the generated frame stands in for
        // the skipped one) and the cadence estimate is unchanged.
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        pacer.source(1, 0, 0, 165);
        for (std::uint64_t frame = 2; frame <= 10; ++frame)
            QCOMPARE(pacer.source(frame, (frame - 1) * 16'666'667, (frame - 1) * 16'666'667, 165),
                     Result::Interpolate);
        QCOMPARE(pacer.source(12, 11 * 16'666'667ULL, 11 * 16'666'667LL, 165), Result::Interpolate);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::None);
        QCOMPARE(pacer.sequenceDelta(), 2ULL);
        QCOMPARE(pacer.interval(), 16'666'667ULL);
        QCOMPARE(pacer.source(13, 12 * 16'666'667ULL, 12 * 16'666'667LL, 165), Result::Interpolate);
        // Two or more skipped frames are a real discontinuity.
        QCOMPARE(pacer.source(16, 15 * 16'666'667ULL, 15 * 16'666'667LL, 165), Result::Discontinuity);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::SequenceGap);
        // Before the cadence is known a skip still re-warms.
        pacer.reset();
        pacer.source(1, 0, 0, 165);
        QCOMPARE(pacer.source(3, 33'333'334, 33'333'334, 165), Result::Discontinuity);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::SequenceGap);
    }

    void generatesAboveSixtyFpsWhenTheDisplayAllows()
    {
        // 120 -> 240 on a 360 Hz display is a valid target; the capability test, not a
        // fixed source-rate ceiling, decides whether the GPU can afford it.
        for (const auto interval : {8'333'333ULL, 11'111'111ULL, 13'333'333ULL}) {
            for (const bool timestamps : {false, true}) {
                StreamFramePacer pacer;
                pacer.source(1, 0, 0, 360);
                for (std::uint64_t frame = 2; frame <= 20; ++frame) {
                    QCOMPARE(pacer.source(frame, timestamps ? (frame - 1) * interval : 0,
                                          (frame - 1) * interval, 360),
                             StreamFramePacer::Result::Interpolate);
                }
                pacer.reset();
                pacer.source(1, 0, 0, 165);
                // 120 FPS needs 234 Hz for 2x: a 165 Hz display is too slow.
                QCOMPARE(pacer.source(2, 8'333'333, 8'333'333, 165),
                         StreamFramePacer::Result::DisplayTooSlow);
            }
        }
    }

    void refusesSourcesFasterThanTheSlotWidth()
    {
        StreamFramePacer pacer;
        pacer.source(1, 0, 0, 1000);
        for (std::uint64_t frame = 2; frame <= 20; ++frame) {
            QCOMPARE(pacer.source(frame, (frame - 1) * 3'000'000, (frame - 1) * 3'000'000, 1000),
                     StreamFramePacer::Result::SourceRateLimit);
            QVERIFY(!pacer.pending());
        }
    }

    void displayRequirementScalesWithTheFactor()
    {
        using Result = StreamFramePacer::Result;
        // 60 FPS source: 2x needs ~114 Hz, 3x ~171 Hz, 4x ~228 Hz. A slower display lowers
        // the factor for the pair instead of switching generation off.
        for (const auto [factor, refresh, expected, effective] : {
                 std::tuple{2, 120.0, Result::Interpolate, 2}, std::tuple{3, 120.0, Result::Interpolate, 2},
                 std::tuple{3, 180.0, Result::Interpolate, 3}, std::tuple{4, 180.0, Result::Interpolate, 3},
                 std::tuple{4, 240.0, Result::Interpolate, 4}, std::tuple{4, 239.0, Result::Interpolate, 4},
                 std::tuple{2, 100.0, Result::DisplayTooSlow, 2}}) {
            StreamFramePacer pacer;
            pacer.source(1, 0, 0, refresh, factor);
            QCOMPARE(pacer.source(2, 16'666'667, 16'666'667, refresh, factor), expected);
            QCOMPARE(pacer.factor(), effective);
        }
        // A cloud "60 FPS" stream at 61.7 FPS keeps 4x on a 240 Hz panel.
        StreamFramePacer cloud;
        cloud.source(1, 0, 0, 240.0, 4);
        for (std::uint64_t frame = 2; frame <= 20; ++frame)
            QCOMPARE(cloud.source(frame, (frame - 1) * 16'200'000, std::int64_t(frame - 1) * 16'200'000, 240.0, 4),
                     Result::Interpolate);
        QCOMPARE(cloud.factor(), 4);
    }

    void schedulesEverySlotOfAThreeAndFourTimesPair()
    {
        using Kind = StreamFramePacer::Next::Kind;
        // A 1000 Hz display keeps the 1.5-refresh lead (1.5 ms) shorter than every slot
        // (5.6 ms at 3x, 4.2 ms at 4x), so slots can be observed one at a time.
        constexpr double refresh = 1000.0;
        for (const int factor : {3, 4}) {
            StreamFramePacer pacer;
            pacer.source(1, 0, 0, refresh, factor);
            QCOMPARE(pacer.source(2, 16'666'667, 16'666'667, refresh, factor),
                     StreamFramePacer::Result::Interpolate);
            pacer.generated(16'666'667, factor - 1);
            QVERIFY(pacer.pending());
            QCOMPARE(pacer.pendingCount(), factor - 1);
            const auto slot = 16'666'667 / factor;
            // Nothing is due before its slot minus the lead.
            QCOMPARE(pacer.takeNext(16'666'667, refresh).kind, Kind::None);
            for (int index = 2; index <= factor - 1; ++index) {
                const auto next = pacer.takeNext(16'666'667 + (index - 1) * slot, refresh);
                QCOMPARE(next.kind, Kind::Generated);
                QCOMPARE(next.index, index);
                QVERIFY(pacer.pending());
            }
            const auto original = pacer.takeNext(16'666'667 + (factor - 1) * slot, refresh);
            QCOMPARE(original.kind, Kind::Original);
            QVERIFY(!pacer.pending());
            QCOMPARE(pacer.takeNext(40'000'000, refresh).kind, Kind::None);
            // A fresh pair drains in the same order again.
            QCOMPARE(pacer.source(3, 33'333'334, 33'333'334, refresh, factor),
                     StreamFramePacer::Result::Interpolate);
        }
        // With a slower display the lead can cover a whole slot: the next generated frame
        // is then due immediately, which the presenter gates on the previous swap instead.
        StreamFramePacer fast;
        fast.source(1, 0, 0, 360, 4);
        fast.source(2, 16'666'667, 16'666'667, 360, 4);
        fast.generated(16'666'667, 3);
        QCOMPARE(fast.takeNext(16'666'667, 360).kind, Kind::Generated);
    }

    void undrainedGeneratedFramesStillTriggerTheOverloadCooldown()
    {
        StreamFramePacer pacer;
        pacer.source(1, 0, 0, 360, 4);
        pacer.source(2, 16'666'667, 16'666'667, 360, 4);
        pacer.generated(16'666'667, 3);
        QCOMPARE(pacer.takeNext(16'666'667 + 4'166'666, 360).kind, StreamFramePacer::Next::Kind::Generated);
        QVERIFY(pacer.pending());
        // A partially drained pair counts like an undrained one once the presenter stalls.
        QCOMPARE(pacer.source(3, 33'333'334, 33'333'334, 360, 4, true), StreamFramePacer::Result::Overloaded);
        QVERIFY(!pacer.pending());
    }

    void highRefreshDoesNotCompressTheOriginalPair()
    {
        StreamFramePacer pacer;
        pacer.source(1, 0, 0, 360);
        pacer.source(2, 16'666'667, 16'666'667, 360);
        pacer.midpoint(16'666'667);
        QVERIFY(!pacer.takeOriginal(17'000'000, 360));
        QVERIFY(pacer.takeOriginal(22'000'000, 360));
    }

    void distinctFramesWithMissingTimestampsStillInterpolate()
    {
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        QCOMPARE(pacer.source(1, 0, 0, 165), Result::WarmingUp);
        for (std::uint64_t frame = 2; frame <= 120; ++frame) {
            const auto now = std::int64_t(frame - 1) * 16'666'667;
            QCOMPARE(pacer.source(frame, 0, now, 165), Result::Interpolate);
            QCOMPARE(pacer.timingSource(), StreamFramePacer::TimingSource::ArrivalCadence);
            QCOMPARE(pacer.timestampDelta(), 0ULL);
            QCOMPARE(pacer.arrivalDelta(), 16'666'667LL);
            pacer.midpoint(now);
            QVERIFY(pacer.takeOriginal(now + 8'333'334, 165));
        }
    }

    void groupedTimestampsDoNotResetContinuousFrames()
    {
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        QCOMPARE(pacer.source(1, 1'000'000'000, 0, 165), Result::WarmingUp);
        for (std::uint64_t frame = 2; frame <= 120; ++frame) {
            const auto now = std::int64_t(frame - 1) * 16'666'667;
            const auto timestamp = 1'000'000'000ULL + ((frame - 1) / 3) * 50'000'001;
            QCOMPARE(pacer.source(frame, timestamp, now, 165), Result::Interpolate);
            pacer.midpoint(now);
            QVERIFY(pacer.takeOriginal(now + 8'333'334, 165));
        }
    }

    void missingTimestampsStillRejectGapsAndInsufficientRefresh()
    {
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        pacer.source(1, 0, 0, 60);
        QCOMPARE(pacer.source(2, 0, 16'666'667, 60), Result::DisplayTooSlow);
        // One skipped frame is tolerated once the cadence is known; two are not.
        QCOMPARE(pacer.source(5, 0, 66'666'668, 165), Result::Discontinuity);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::SequenceGap);
        QCOMPARE(pacer.source(6, 0, 250'000'000, 165), Result::Discontinuity);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::ArrivalGap);
    }

    void arrivalCadenceAbsorbsDisplayQuantizationWithoutChangingTimestamps()
    {
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        std::int64_t now = 0;
        pacer.source(1, 100, now, 165);
        for (std::uint64_t frame = 2; frame <= 120; ++frame) {
            now += frame % 4 == 0 ? 12'121'212 : 18'181'818;
            QCOMPARE(pacer.source(frame, 100 + frame - 1, now, 165), Result::Interpolate);
            QCOMPARE(pacer.timingSource(), StreamFramePacer::TimingSource::ArrivalCadence);
            QCOMPARE(pacer.timestampDelta(), 1ULL);
            pacer.midpoint(now);
            QVERIFY(pacer.takeOriginal(now + 9'090'909, 165));
        }
    }

    void sourceClockRemainsPreferredWhenUsable()
    {
        StreamFramePacer pacer;
        pacer.source(1, 0, 0, 165);
        QCOMPARE(pacer.source(2, 16'666'667, 18'181'818, 165), StreamFramePacer::Result::Interpolate);
        QCOMPARE(pacer.timingSource(), StreamFramePacer::TimingSource::SourceTimestamps);
        QCOMPARE(pacer.interval(), 16'666'667ULL);
        pacer.reset();
        QCOMPARE(pacer.timingSource(), StreamFramePacer::TimingSource::None);
        QCOMPARE(pacer.interval(), 0ULL);
    }

    void alternatingBurstArrivalsConvergeWithoutHidingBackpressure()
    {
        StreamFramePacer pacer;
        using Result = StreamFramePacer::Result;
        std::int64_t now = 0;
        pacer.source(1, 0, now, 165);
        for (std::uint64_t frame = 2; frame <= 30; ++frame) {
            now += frame % 2 ? 31'333'334 : 2'000'000;
            const auto result = pacer.source(frame, 0, now, 165);
            if (frame >= 10) {
                QCOMPARE(result, Result::Interpolate);
                QCOMPARE(pacer.interval(), 16'666'667ULL);
            }
        }
        // An undrained pair after the bursts is dropped without a cooldown while the
        // presenter keeps swapping; once it stalls the backpressure surfaces.
        pacer.midpoint(now);
        now += 31'333'334;
        QCOMPARE(pacer.source(31, 0, now, 165), Result::Interpolate);
        QVERIFY(!pacer.pending());
        pacer.midpoint(now);
        now += 2'000'000;
        QCOMPARE(pacer.source(32, 0, now, 165, 2, true), Result::Overloaded);
        QVERIFY(!pacer.pending());
    }

    void simultaneousArrivalsCannotMasqueradeAsSlowerInput()
    {
        StreamFramePacer pacer;
        std::int64_t now = 0;
        pacer.source(1, 0, now, 60);
        for (std::uint64_t frame = 2; frame <= 30; ++frame) {
            now += frame % 2 ? 33'333'334 : 0;
            const auto result = pacer.source(frame, 0, now, 60);
            if (frame >= 10)
                QCOMPARE(result, StreamFramePacer::Result::DisplayTooSlow);
        }
    }

    void clusteredBurstsWithoutUsableCadenceFallBack()
    {
        StreamFramePacer pacer;
        std::int64_t now = 0;
        pacer.source(1, 0, now, 165);
        for (std::uint64_t frame = 2; frame <= 30; ++frame) {
            now += frame % 3 ? 1'000'000 : 48'000'000;
            const auto result = pacer.source(frame, 0, now, 165);
            if (frame >= 10) {
                QCOMPARE(result, StreamFramePacer::Result::Discontinuity);
                QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::CadenceUnavailable);
                QVERIFY(!pacer.pending());
            }
        }
    }

    void arrivalCadenceAdaptsToSourceRateChanges()
    {
        StreamFramePacer pacer;
        std::int64_t now = 0;
        pacer.source(1, 0, now, 60);
        for (std::uint64_t frame = 2; frame <= 30; ++frame) {
            now += frame <= 15 ? 16'666'667 : 33'333'334;
            const auto result = pacer.source(frame, 0, now, 60);
            if (frame <= 15)
                QCOMPARE(result, StreamFramePacer::Result::DisplayTooSlow);
            if (frame >= 24) {
                QCOMPARE(result, StreamFramePacer::Result::Interpolate);
                QCOMPARE(pacer.interval(), 33'333'334ULL);
            }
        }
    }

    void arrivalFallbackStillRejectsTimestampRewindsAndLargeJumps()
    {
        StreamFramePacer pacer;
        pacer.source(1, 1'000'000'000, 0, 165);
        QCOMPARE(pacer.source(2, 1'000'000'000, 16'666'667, 165), StreamFramePacer::Result::Interpolate);
        QCOMPARE(pacer.source(3, 900'000'000, 33'333'334, 165), StreamFramePacer::Result::Discontinuity);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::TimestampRegression);
        QCOMPARE(pacer.source(4, 2'000'000'000, 50'000'001, 165), StreamFramePacer::Result::Discontinuity);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::TimestampJump);
    }
};

QTEST_APPLESS_MAIN(FramePacerTest)
#include "tst_framepacer.moc"
