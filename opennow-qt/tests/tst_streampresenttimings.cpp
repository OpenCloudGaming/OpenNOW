#include "streaming/rendering/StreamPresentTimings.h"
#include "streaming/rendering/StreamSwapStallWatchdog.h"

#include <QTest>

#include <utility>
#include <atomic>
#include <thread>

namespace {
StreamSwapStallWatchdog::Observation decodedProgress(
    StreamSwapStallWatchdog::Observation observation,
    std::uint64_t epoch, std::uint64_t outputsTotal)
{
    observation.hasUpstreamSample = true;
    observation.upstreamEpoch = epoch;
    observation.upstreamOutputsTotal = outputsTotal;
    return observation;
}
}

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

    void gateEpochSurvivesHideShowAndResourceReset()
    {
        StreamPresentTimings timings;
        QCOMPARE(timings.snapshot().gateEpoch, std::uint64_t(0));
        timings.setGated(true);
        QVERIFY(timings.snapshot().gated);
        QCOMPARE(timings.snapshot().gateEpoch, std::uint64_t(1));
        timings.setGated(true);
        QCOMPARE(timings.snapshot().gateEpoch, std::uint64_t(1));
        timings.setGated(false);
        timings.reset();
        QVERIFY(!timings.snapshot().gated);
        QCOMPARE(timings.snapshot().gateEpoch, std::uint64_t(1));
        timings.setGated(true);
        QCOMPARE(timings.snapshot().gateEpoch, std::uint64_t(2));
    }

    void watchdogResetsAfterAnUnobservedHideShow()
    {
        StreamPresentTimings timings;
        StreamSwapStallWatchdog watchdog;
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        watchdog.onResourcesReleased(true, false, false);
        timings.setGated(true);
        timings.setGated(false);
        const auto snapshot = timings.snapshot();
        observation.gated = snapshot.gated;
        observation.gateEpoch = snapshot.gateEpoch;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 20'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 4), 27'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 5), 28'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
    }

    void gatePublicationDoesNotMutateTheRenderOwnedWatchdog()
    {
        StreamPresentTimings timings;
        StreamSwapStallWatchdog watchdog;
        std::atomic_bool start = false;
        std::thread publisher([&] {
            while (!start.load()) std::this_thread::yield();
            for (int iteration = 0; iteration < 2'000; ++iteration) {
                timings.setGated(true);
                timings.setGated(false);
            }
        });
        start.store(true);
        for (int iteration = 1; iteration <= 2'000; ++iteration) {
            timings.markSubmit(iteration);
            const auto snapshot = timings.snapshot();
            StreamSwapStallWatchdog::Observation observation;
            observation.gated = snapshot.gated;
            observation.gateEpoch = snapshot.gateEpoch;
            observation.hasPendingSubmit = snapshot.hasPendingSubmit;
            watchdog.observe(decodedProgress(observation, 1, iteration), iteration);
        }
        publisher.join();
        QCOMPARE(timings.snapshot().gateEpoch, std::uint64_t(2'000));
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
    }

    void swapWatchdogStaysSilentWithoutOutstandingWork()
    {
        StreamSwapStallWatchdog watchdog;
        StreamSwapStallWatchdog::Observation observation;
        observation.hasLastSwap = true;
        observation.lastSwapNs = 1'000'000;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 1), 60'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
    }

    void swapWatchdogStaysSilentWhenTheWindowIsGated()
    {
        StreamSwapStallWatchdog watchdog;
        StreamSwapStallWatchdog::Observation observation;
        observation.gated = true;
        observation.hasPendingSubmit = true;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 1), 60'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
    }

    void swapWatchdogSuppressesWhileTheDecodeStageOwnsTheStall()
    {
        StreamSwapStallWatchdog watchdog;
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        observation.upstreamStalled = true;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 1), 60'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
    }

    void swapWatchdogDefersWithoutDecodedOutputEvidence()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        QCOMPARE(watchdog.observe(observation, 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(observation, 40'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
    }

    void swapWatchdogIgnoresTelemetryWithoutDecodedOutputProgress()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        const auto sample = decodedProgress(observation, 1, 64);
        QCOMPARE(watchdog.observe(sample, 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(sample, 17'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(sample, 30'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
    }

    void swapWatchdogTracksNewerSwapsInItsProgressBaseline()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        observation.hasLastSwap = true;
        observation.lastSwapNs = 1'000'000'000;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        observation.lastSwapNs = 2'000'000'000;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 60), 2'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 60), 10'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 61), 11'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(1));
    }

    void swapWatchdogSeedsItsFirstDecodedSampleInsteadOfCountingZeros()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        QCOMPARE(watchdog.observe(observation, 1'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 0, 5), 10'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 0, 6), 19'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(1));
    }

    void swapWatchdogRearmsOnceThenReportsUnrecovered()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        observation.hasLastSwap = true;
        observation.lastSwapNs = 1'000'000'000;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 1), 8'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 15'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 16'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(1));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 4), 17'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 5), 18'000'000'000),
                 StreamSwapStallWatchdog::Outcome::Unrecovered);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(1));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 6), 60'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
    }

    void swapWatchdogRearmsWithoutAnyPriorSwap()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 8'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
    }

    void swapWatchdogResetClearsAnEpisodeWithoutClearingCounts()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        watchdog.reset();
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(1));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 12'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
    }

    void swapWatchdogRepeatedGatingNeverEscalatesWhileGated()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        observation.gated = true;
        for (int index = 1; index <= 20; ++index) {
            QCOMPARE(watchdog.observe(decodedProgress(observation, 1, std::uint64_t(index)),
                                      std::int64_t(index) * 10'000'000'000),
                     StreamSwapStallWatchdog::Outcome::None);
        }
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
    }

    void swapWatchdogKeepsTheEpisodeAcrossItsOwnResourceRearm()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        watchdog.onResourcesReleased(true, false, false);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 10'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 4), 11'000'000'000),
                 StreamSwapStallWatchdog::Outcome::Unrecovered);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(1));
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(1));
    }

    void swapWatchdogSurvivesClearedTimingStateAfterItsOwnRearm()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        watchdog.onResourcesReleased(true, false, false);
        StreamSwapStallWatchdog::Observation afterRearm;
        afterRearm.hasPendingSubmit = false;
        afterRearm.hasLastSwap = true;
        afterRearm.lastSwapNs = 1'000'000'000;
        QCOMPARE(watchdog.observe(decodedProgress(afterRearm, 1, 3), 10'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(afterRearm, 1, 4), 11'000'000'000),
                 StreamSwapStallWatchdog::Outcome::Unrecovered);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(1));
        QCOMPARE(watchdog.observe(decodedProgress(afterRearm, 1, 5), 60'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
    }

    void swapWatchdogSuspendsItsTerminalDecisionWithoutUsableEvidence()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        QCOMPARE(watchdog.observe(observation, 11'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 12'000'000'000),
                 StreamSwapStallWatchdog::Outcome::Unrecovered);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(1));
    }

    void swapWatchdogRearmedEpisodeIsCancelledByAnExternalTeardown()
    {
        for (const auto &combination : {std::pair<bool, bool>{true, false},
                                        std::pair<bool, bool>{false, true},
                                        std::pair<bool, bool>{true, true}}) {
            StreamSwapStallWatchdog watchdog;
            watchdog.setPolicy({8'000'000'000, 2'000'000'000});
            StreamSwapStallWatchdog::Observation observation;
            observation.hasPendingSubmit = true;
            watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
            QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                     StreamSwapStallWatchdog::Outcome::ResourceRearm);
            watchdog.onResourcesReleased(true, combination.first, combination.second);
            StreamSwapStallWatchdog::Observation afterRearm;
            afterRearm.hasPendingSubmit = false;
            QCOMPARE(watchdog.observe(afterRearm, 10'000'000'000),
                     StreamSwapStallWatchdog::Outcome::None);
            QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
        }
    }

    void swapWatchdogFinishesItsEpisodeOnANewerSwapWhilePendingRemains()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        watchdog.onResourcesReleased(true, false, false);
        auto resumed = decodedProgress(observation, 1, 3);
        resumed.hasLastSwap = true;
        resumed.lastSwapNs = 9'500'000'000;
        QCOMPARE(watchdog.observe(resumed, 10'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(resumed, 20'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(1));
    }

    void swapWatchdogResetsOnASessionScopedResourceRelease()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        watchdog.onResourcesReleased(false, false, false);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 40'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(0));
    }

    void swapWatchdogReseedsOnDecoderEpochChangeWithoutBorrowingOldProgress()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 100), 1'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 2, 1000), 20'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(0));
        QCOMPARE(watchdog.observe(decodedProgress(observation, 2, 1001), 21'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        QCOMPARE(watchdog.rearmCount(), std::uint64_t(1));
    }

    void swapWatchdogRearmsOnceFromJitteredOneHertzTelemetry()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        observation.hasLastSwap = true;
        observation.lastSwapNs = 4'000'000'000;
        std::uint64_t outputs = 900;
        std::int64_t sampleNs = 5'200'000'000;
        std::uint64_t rearms = 0;
        std::uint64_t terminals = 0;
        for (int index = 0; index < 40; ++index) {
            ++outputs;
            sampleNs += 1'000'000'000 + (index % 2) * 120'000'000;
            const auto sample = decodedProgress(observation, 1, outputs);
            for (int frame = 0; frame < 3; ++frame) {
                const auto outcome =
                    watchdog.observe(sample, sampleNs + std::int64_t(frame) * 16'000'000);
                if (outcome == StreamSwapStallWatchdog::Outcome::ResourceRearm) ++rearms;
                if (outcome == StreamSwapStallWatchdog::Outcome::Unrecovered) ++terminals;
            }
        }
        QCOMPARE(rearms, std::uint64_t(1));
        QCOMPARE(terminals, std::uint64_t(1));
    }

    void swapWatchdogReportsUnrecoveredAtMostOncePerEpisode()
    {
        StreamSwapStallWatchdog watchdog;
        watchdog.setPolicy({8'000'000'000, 2'000'000'000});
        StreamSwapStallWatchdog::Observation observation;
        observation.hasPendingSubmit = true;
        watchdog.observe(decodedProgress(observation, 1, 1), 1'000'000'000);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 2), 9'000'000'000),
                 StreamSwapStallWatchdog::Outcome::ResourceRearm);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 3), 11'000'000'000),
                 StreamSwapStallWatchdog::Outcome::Unrecovered);
        QCOMPARE(watchdog.observe(decodedProgress(observation, 1, 4), 90'000'000'000),
                 StreamSwapStallWatchdog::Outcome::None);
        QCOMPARE(watchdog.unrecoveredCount(), std::uint64_t(1));
    }
};

QTEST_MAIN(StreamPresentTimingsTest)
#include "tst_streampresenttimings.moc"
