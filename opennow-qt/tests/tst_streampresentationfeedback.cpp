#include "streaming/rendering/StreamPresentationFeedback.h"

#include <QTest>
#include <limits>

class StreamPresentationFeedbackTest : public QObject
{
    Q_OBJECT

private slots:
    void correlatesAnOsPresentWithTheOriginalSource()
    {
        StreamPresentationFeedback feedback;
        feedback.presented(40, 41, {{7, 3, 900'000'000, 10'000'000}}, 12'000'000, true);
        const auto sample = feedback.observe({41, 100, 100, 15'000'000}, 16'000'000);
        QVERIFY(sample);
        QCOMPARE(sample->source.generation, std::uint64_t(7));
        QCOMPARE(sample->source.sceneEpoch, std::uint64_t(3));
        QCOMPARE(sample->source.timestampNs, std::uint64_t(900'000'000));
        QCOMPARE(sample->renderDurationNs, std::uint64_t(2'000'000));
        QCOMPARE(sample->presentDurationNs, std::uint64_t(3'000'000));
        QCOMPARE(sample->completionAgeNs, std::uint64_t(1'000'000));
        QCOMPARE(sample->vsyncIntervalNs, std::uint64_t(0));
        QVERIFY(!feedback.observe({41, 100, 100, 15'000'000}, 17'000'000));
    }

    void measuresEnabledVsyncInsteadOfUsingNominalFps()
    {
        StreamPresentationFeedback feedback;
        QVERIFY(!feedback.observe({0, 20, 20, 10'000'000}, 11'000'000));
        feedback.presented(0, 1, {{1, 2, 700, 12'000'000}}, 13'000'000, false);
        const auto sample = feedback.observe({1, 21, 21, 18'333'333}, 19'000'000);
        QVERIFY(sample);
        QCOMPARE(sample->vsyncIntervalNs, std::uint64_t(8'333'333));
    }

    void unknownEnabledVsyncIsNotTheSameAsDisabled()
    {
        StreamPresentationFeedback feedback;
        feedback.presented(0, 1, {{1, 1, 700, 10'000'000}}, 12'000'000, false);
        QVERIFY(!feedback.observe({1, 20, 20, 15'000'000}, 16'000'000));
    }

    void doesNotReconstructVrrPresentationTimesFromRefreshCounts()
    {
        StreamPresentationFeedback feedback;
        feedback.presented(0, 1, {{1, 1, 700, 10'000'000}}, 12'000'000, true);
        QVERIFY(!feedback.observe({1, 19, 20, 15'000'000}, 16'000'000));
    }

    void overlayAndInterpolatedPresentsCannotCompleteVideoFrames()
    {
        StreamPresentationFeedback feedback;
        feedback.presented(0, 1, {{1, 1, 700, 10'000'000}}, 12'000'000, true);
        feedback.presented(1, 2, {}, 13'000'000, true);
        QVERIFY(!feedback.observe({2, 20, 20, 15'000'000}, 16'000'000));
        QCOMPARE(feedback.pending(), std::size_t(0));
        QVERIFY(!feedback.observe({1, 19, 19, 14'000'000}, 16'000'000));
    }

    void suppressesFailedAndUnattributedPresents()
    {
        StreamPresentationFeedback feedback;
        feedback.presented(0, 1, {{1, 1, 700, 10'000'000}}, 12'000'000, true);
        feedback.presented(1, 1, {}, 13'000'000, true);
        QCOMPARE(feedback.pending(), std::size_t(0));
        feedback.presented(1, 3, {{1, 1, 800, 14'000'000}}, 15'000'000, true);
        QVERIFY(!feedback.observe({3, 20, 20, 16'000'000}, 17'000'000));
    }

    void acceptsWrappingPresentIdsWithoutUsingThemAsSenderIds()
    {
        StreamPresentationFeedback feedback;
        feedback.presented(std::numeric_limits<std::uint32_t>::max(), 0,
                           {{5, 2, 700, 10'000'000}}, 12'000'000, true);
        const auto sample = feedback.observe({0, 20, 20, 15'000'000}, 16'000'000);
        QVERIFY(sample);
        QCOMPARE(sample->source.timestampNs, std::uint64_t(700));
    }

    void boundsPendingObservations()
    {
        StreamPresentationFeedback feedback;
        for (std::uint32_t count = 0; count < 200; ++count)
            feedback.presented(count, count + 1, {{1, 1, count + 1, 1'000'000}},
                               2'000'000, true);
        QCOMPARE(feedback.pending(), StreamPresentationFeedback::Capacity);
        QVERIFY(!feedback.observe({1, 20, 20, 3'000'000}, 4'000'000));
        QVERIFY(feedback.observe({200, 20, 20, 3'000'000}, 4'000'000));
    }

    void rejectsInvalidClockAndStaleCompletions_data()
    {
        QTest::addColumn<qulonglong>("completion");
        QTest::addColumn<qulonglong>("now");
        QTest::newRow("no-clock") << qulonglong(0) << qulonglong(16'000'000);
        QTest::newRow("clock-in-future") << qulonglong(17'000'000) << qulonglong(16'000'000);
        QTest::newRow("before-render-present") << qulonglong(11'000'000) << qulonglong(16'000'000);
        QTest::newRow("old-completion") << qulonglong(15'000'000) << qulonglong(2'000'000'000);
        QTest::newRow("late-presentation") << qulonglong(2'000'000'000) << qulonglong(2'001'000'000);
    }

    void rejectsInvalidClockAndStaleCompletions()
    {
        QFETCH(qulonglong, completion);
        QFETCH(qulonglong, now);
        StreamPresentationFeedback feedback;
        feedback.presented(0, 1, {{1, 1, 700, 10'000'000}}, 12'000'000, true);
        QVERIFY(!feedback.observe({1, 20, 20, completion}, now));
    }

    void resetRetiresEveryOldSceneObservation()
    {
        StreamPresentationFeedback feedback;
        feedback.presented(0, 1, {{1, 1, 700, 10'000'000}}, 12'000'000, true);
        feedback.reset();
        QVERIFY(!feedback.observe({1, 20, 20, 15'000'000}, 16'000'000));
    }

    void invalidSourceMetadataCannotBecomeACompletedFrame()
    {
        const StreamPresentationFeedback::Source invalid[]{
            {0, 1, 700, 10'000'000}, {1, 1, 0, 10'000'000},
            {1, 1, 700, 0}, {1, 1, 700, 13'000'000}};
        for (const auto &source : invalid) {
            StreamPresentationFeedback feedback;
            feedback.presented(0, 1, source, 12'000'000, true);
            QVERIFY(!feedback.observe({1, 20, 20, 15'000'000}, 16'000'000));
        }
    }
};

QTEST_GUILESS_MAIN(StreamPresentationFeedbackTest)
#include "tst_streampresentationfeedback.moc"
