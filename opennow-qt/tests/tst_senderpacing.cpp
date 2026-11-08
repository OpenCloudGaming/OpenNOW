#include "streaming/rendering/StreamSenderPacer.h"

#include <QtTest>

class SenderPacingTests final : public QObject
{
    Q_OBJECT

private slots:
    void preservesVariableSenderCadenceWithoutUsingTheSelectedFps()
    {
        StreamSenderPacer pacer;
        StreamSenderTiming frame{1, 2, 10, 1'000'000, 0, true, true};
        constexpr std::uint64_t start = 2'000'000'000;
        QCOMPARE(pacer.deadline(frame, start), start);
        frame.frameIndex++;
        frame.captureTimeUs += 4'167;
        QCOMPARE(pacer.deadline(frame, start + 1'000'000), start + 4'167'000);
        frame.frameIndex++;
        frame.captureTimeUs += 11'111;
        QCOMPARE(pacer.deadline(frame, start + 9'000'000), start + 15'278'000);
    }

    void unpacedOrUnknownFramesNeverWait()
    {
        StreamSenderPacer pacer;
        StreamSenderTiming frame{1, 2, 1, 1'000'000, 0, true, true};
        QCOMPARE(pacer.deadline(frame, 100), std::uint64_t(100));
        for (auto timing : {StreamSenderTiming{}, StreamSenderTiming{1, 2, 2, 2'000'000, 0, true, false},
                 StreamSenderTiming{1, 2, 2, 0, 0, true, true}})
            QCOMPARE(pacer.deadline(timing, 200), std::uint64_t(200));
    }

    void captureCounterAndFrameIdentityWrapWithoutInventingAnRtpClock()
    {
        StreamSenderPacer pacer;
        StreamSenderTiming frame{1, 1, 0xffffffffu, 0xfffffff0u, 0, true, true};
        QCOMPARE(pacer.deadline(frame, 1'000'000), std::uint64_t(1'000'000));
        frame.frameIndex = 0;
        frame.captureTimeUs = 0x20;
        QCOMPARE(pacer.deadline(frame, 1'010'000), std::uint64_t(1'048'000));
    }

    void timestampPriorityAndClockChangesNeverMixEpochs()
    {
        StreamSenderPacer pacer;
        StreamSenderTiming frame{1, 1, 1, 0, 5'000'000, true, true};
        QCOMPARE(pacer.deadline(frame, 1'000'000), std::uint64_t(1'000'000));
        frame.frameIndex++;
        frame.presentationTimeUs += 4'000;
        QCOMPARE(pacer.deadline(frame, 2'000'000), std::uint64_t(5'000'000));
        frame.frameIndex++;
        frame.captureTimeUs = 7'000;
        QCOMPARE(pacer.deadline(frame, 3'000'000), std::uint64_t(3'000'000));
        frame.frameIndex++;
        frame.captureTimeUs += 4'000;
        frame.presentationTimeUs += 9'000'000;
        QCOMPARE(pacer.deadline(frame, 4'000'000), std::uint64_t(7'000'000));
    }

    void discontinuitiesAndLongStallsReanchorWithoutBuildingALongQueue()
    {
        for (unsigned reason = 0; reason < 6; ++reason) {
            StreamSenderPacer pacer;
            StreamSenderTiming frame{1, 1, 10, 1'000'000, 0, true, true};
            pacer.deadline(frame, 1'000'000);
            frame.frameIndex++;
            frame.captureTimeUs += 4'000;
            switch (reason) {
            case 0: frame.generation++; break;
            case 1: frame.sceneEpoch++; break;
            case 2: frame.frameIndex = 9; break;
            case 3: frame.captureTimeUs = 999'000; break;
            case 4: frame.captureTimeUs += 10'000'000; break;
            default: break;
            }
            const std::uint64_t now = reason == 5 ? 10'000'000'000 : 2'000'000;
            QCOMPARE(pacer.deadline(frame, now), now);
        }
    }

    void resetAndOverflowNeverCarryAnOldDeadline()
    {
        StreamSenderPacer pacer;
        StreamSenderTiming frame{1, 1, 1, 0, 1, true, true};
        const auto maximum = std::numeric_limits<std::uint64_t>::max();
        pacer.deadline(frame, maximum - 500);
        frame.frameIndex++;
        frame.presentationTimeUs++;
        QCOMPARE(pacer.deadline(frame, maximum - 100), maximum - 100);
        pacer.reset();
        frame.frameIndex++;
        frame.presentationTimeUs = maximum;
        QCOMPARE(pacer.deadline(frame, 100), std::uint64_t(100));
        frame.frameIndex++;
        frame.presentationTimeUs = 1;
        QCOMPARE(pacer.deadline(frame, 200), std::uint64_t(200));
    }
};

QTEST_GUILESS_MAIN(SenderPacingTests)
#include "tst_senderpacing.moc"
