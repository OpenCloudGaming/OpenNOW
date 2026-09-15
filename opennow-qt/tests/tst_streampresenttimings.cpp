#include "streaming/rendering/StreamPresentTimings.h"

#include <QTest>

class StreamPresentTimingsTest : public QObject
{
    Q_OBJECT
private slots:
    void reportsNothingBeforeAFrameIsPresented()
    {
        StreamPresentTimings timings;
        const auto snapshot = timings.snapshot();
        QVERIFY(!snapshot.available);
        QCOMPARE(snapshot.windowSamples, std::size_t(0));
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(0));
        QVERIFY(!snapshot.hasLastSwap);
    }

    void swapWithoutSubmitRecordsNoSample()
    {
        StreamPresentTimings timings;
        timings.markSwap(5'000'000);
        const auto snapshot = timings.snapshot();
        QVERIFY(!snapshot.available);
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(0));
        QVERIFY(!snapshot.hasLastSwap);
    }

    void measuresSubmitToSwapForEachMarkedFrame()
    {
        StreamPresentTimings timings;
        timings.markSubmit(1'000'000);
        timings.markSwap(4'500'000);
        const auto snapshot = timings.snapshot();
        QVERIFY(snapshot.available);
        QCOMPARE(snapshot.submitToSwap.p50Ns, std::int64_t(3'500'000));
        QCOMPARE(snapshot.submitToSwap.p95Ns, std::int64_t(3'500'000));
        QCOMPARE(snapshot.submitToSwap.maxNs, std::int64_t(3'500'000));
        QCOMPARE(snapshot.windowSamples, std::size_t(1));
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(1));
        QVERIFY(snapshot.hasLastSwap);
        QCOMPARE(snapshot.lastSwapNs, std::int64_t(4'500'000));
    }

    void oneSubmitIsConsumedByOneSwap()
    {
        StreamPresentTimings timings;
        timings.markSubmit(1'000'000);
        timings.markSwap(2'000'000);
        timings.markSwap(3'000'000);
        const auto snapshot = timings.snapshot();
        QCOMPARE(snapshot.windowSamples, std::size_t(1));
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(1));
        QCOMPARE(snapshot.submitToSwap.p50Ns, std::int64_t(1'000'000));
    }

    void outOfOrderClockReadsReportZeroInsteadOfNegative()
    {
        StreamPresentTimings timings;
        timings.markSubmit(9'000'000);
        timings.markSwap(8'000'000);
        const auto snapshot = timings.snapshot();
        QVERIFY(snapshot.available);
        QCOMPARE(snapshot.submitToSwap.p50Ns, std::int64_t(0));
        QCOMPARE(snapshot.submitToSwap.maxNs, std::int64_t(0));
    }

    void percentilesUseNearestRankMeasuredValues()
    {
        StreamPresentTimings timings;
        for (std::int64_t sample = 1; sample <= 20; ++sample) {
            timings.markSubmit(0);
            timings.markSwap(sample * 1'000'000);
        }
        const auto snapshot = timings.snapshot();
        QCOMPARE(snapshot.windowSamples, std::size_t(20));
        QCOMPARE(snapshot.submitToSwap.p50Ns, std::int64_t(10'000'000));
        QCOMPARE(snapshot.submitToSwap.p95Ns, std::int64_t(19'000'000));
        QCOMPARE(snapshot.submitToSwap.maxNs, std::int64_t(20'000'000));
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(20));
    }

    void windowSamplesSaturateButTotalsKeepCounting()
    {
        StreamPresentTimings timings;
        const std::size_t frames = StreamPresentTimings::WindowCapacity * 3;
        for (std::size_t frame = 0; frame < frames; ++frame) {
            timings.markSubmit(0);
            timings.markSwap(1'000'000);
        }
        const auto snapshot = timings.snapshot();
        QCOMPARE(snapshot.windowSamples, StreamPresentTimings::WindowCapacity);
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(frames));
        QCOMPARE(snapshot.submitToSwap.maxNs, std::int64_t(1'000'000));
    }

    void discardPendingDropsTheSubmitWithoutRecordingASample()
    {
        StreamPresentTimings timings;
        timings.markSubmit(1'000'000);
        timings.discardPending();
        timings.markSwap(9'000'000);
        const auto snapshot = timings.snapshot();
        QVERIFY(!snapshot.available);
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(0));
        QVERIFY(!snapshot.hasLastSwap);
        timings.markSubmit(9'500'000);
        timings.markSwap(9'750'000);
        QCOMPARE(timings.snapshot().submitToSwap.p50Ns, std::int64_t(250'000));
    }

    void resetClearsTheWindowAndKeepsCumulativeProgress()
    {
        StreamPresentTimings timings;
        timings.markSubmit(1'000'000);
        timings.markSwap(2'000'000);
        const auto before = timings.snapshot();
        QCOMPARE(before.epoch, std::uint64_t(0));
        timings.reset();
        const auto snapshot = timings.snapshot();
        QVERIFY(!snapshot.available);
        QCOMPARE(snapshot.windowSamples, std::size_t(0));
        QCOMPARE(snapshot.swappedFramesTotal, before.swappedFramesTotal);
        QVERIFY(snapshot.hasLastSwap);
        QCOMPARE(snapshot.lastSwapNs, before.lastSwapNs);
        QCOMPARE(snapshot.epoch, std::uint64_t(1));
        timings.markSubmit(5'000'000);
        timings.markSwap(6'000'000);
        const auto after = timings.snapshot();
        QCOMPARE(after.submitToSwap.p50Ns, std::int64_t(1'000'000));
        QCOMPARE(after.swappedFramesTotal, before.swappedFramesTotal + 1);
        QCOMPARE(after.windowSamples, std::size_t(1));
    }

    void gatingRefusesSubmitsAndSwapsUntilReleased()
    {
        StreamPresentTimings timings;
        timings.setGated(true);
        timings.markSubmit(1'000'000);
        timings.markSwap(9'000'000);
        auto snapshot = timings.snapshot();
        QVERIFY(!snapshot.available);
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(0));
        timings.setGated(false);
        timings.markSwap(9'500'000);
        QVERIFY(!timings.snapshot().available);
        timings.markSubmit(9'500'000);
        timings.markSwap(9'750'000);
        snapshot = timings.snapshot();
        QCOMPARE(snapshot.submitToSwap.p50Ns, std::int64_t(250'000));
        QCOMPARE(snapshot.swappedFramesTotal, std::uint64_t(1));
    }

    void gatingDiscardsAnInFlightSubmit()
    {
        StreamPresentTimings timings;
        timings.markSubmit(1'000'000);
        timings.setGated(true);
        timings.setGated(false);
        timings.markSwap(9'000'000);
        QVERIFY(!timings.snapshot().available);
        QCOMPARE(timings.snapshot().swappedFramesTotal, std::uint64_t(0));
    }
};

QTEST_MAIN(StreamPresentTimingsTest)
#include "tst_streampresenttimings.moc"
