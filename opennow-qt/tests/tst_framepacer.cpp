#include "streaming/rendering/StreamFramePacer.h"

#include <QTest>
#include <limits>

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
        QCOMPARE(pacer.source(3, 33'333'334, 33'333'334, 120), Result::Overloaded);
        QVERIFY(!pacer.pending());
        QCOMPARE(pacer.source(4, 50'000'001, 50'000'001, 120), Result::Overloaded);
        for (std::uint64_t frame = 5; frame < 130; ++frame)
            pacer.source(frame, (frame - 1) * 16'666'667, (frame - 1) * 16'666'667, 120);
        QCOMPARE(pacer.source(130, 129 * 16'666'667ULL, 129 * 16'666'667LL, 120), Result::Interpolate);
    }

    void doesNotGenerateAboveThe120FpsTarget()
    {
        for (const auto interval : {8'333'333ULL, 11'111'111ULL, 13'333'333ULL}) {
            for (const bool timestamps : {false, true}) {
                StreamFramePacer pacer;
                pacer.source(1, 0, 0, 360);
                for (std::uint64_t frame = 2; frame <= 20; ++frame) {
                    QCOMPARE(pacer.source(frame, timestamps ? (frame - 1) * interval : 0,
                                          (frame - 1) * interval, 360),
                             StreamFramePacer::Result::SourceRateLimit);
                    QVERIFY(!pacer.pending());
                }
                pacer.reset();
                pacer.source(1, 0, 0, 360);
                QCOMPARE(pacer.source(2, 16'666'667, 16'666'667, 360),
                         StreamFramePacer::Result::Interpolate);
            }
        }
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
        QCOMPARE(pacer.source(4, 0, 33'333'334, 165), Result::Discontinuity);
        QCOMPARE(pacer.rejection(), StreamFramePacer::Rejection::SequenceGap);
        QCOMPARE(pacer.source(5, 0, 200'000'000, 165), Result::Discontinuity);
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
        pacer.midpoint(now);
        QCOMPARE(pacer.source(31, 0, now + 31'333'334, 165), Result::Overloaded);
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
