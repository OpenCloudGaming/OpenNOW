#include "streaming/rendering/StreamPresentationPolicy.h"

#include <QtTest>
#include <vector>

class StreamPresentationTests final : public QObject
{
    Q_OBJECT

private slots:
    void requiresEveryVrrCondition()
    {
        const StreamPresentationInputs ready{true, true, true, true, true, true, true, true};
        QCOMPARE(streamPresentationMode(ready), StreamPresentationMode::Vrr);
        bool StreamPresentationInputs::*conditions[] = {
            &StreamPresentationInputs::cloudGsync, &StreamPresentationInputs::active,
            &StreamPresentationInputs::exposed, &StreamPresentationInputs::fullscreen,
            &StreamPresentationInputs::tearingSupported, &StreamPresentationInputs::vrrPossible,
            &StreamPresentationInputs::vrrRequested};
        for (auto condition : conditions) {
            auto input = ready;
            input.*condition = false;
            QCOMPARE(streamPresentationMode(input), StreamPresentationMode::FixedRefresh);
        }
        auto stopped = ready;
        stopped.streaming = false;
        QCOMPARE(streamPresentationMode(stopped), StreamPresentationMode::Baseline);
        QCOMPARE(streamPresentationMode({}), StreamPresentationMode::Baseline);
    }

    void focusAndSessionTransitionsRestoreConservativeMode()
    {
        StreamPresentationInputs input{true, true, true, true, true, true, true, true};
        input.active = false;
        QCOMPARE(streamPresentationMode(input), StreamPresentationMode::FixedRefresh);
        input.active = true;
        QCOMPARE(streamPresentationMode(input), StreamPresentationMode::Vrr);
        input.streaming = false;
        QCOMPARE(streamPresentationMode(input), StreamPresentationMode::Baseline);
        input.streaming = true;
        input.cloudGsync = false;
        QCOMPARE(streamPresentationMode(input), StreamPresentationMode::FixedRefresh);
    }

    void restoresOriginalNoVsyncPreference()
    {
        for (bool original : {false, true}) {
            QCOMPARE(streamPresentationNoVsync(StreamPresentationMode::Baseline, original), original);
            QVERIFY(!streamPresentationNoVsync(StreamPresentationMode::FixedRefresh, original));
            QVERIFY(streamPresentationNoVsync(StreamPresentationMode::Vrr, original));
            QCOMPARE(streamPresentationNoVsync(StreamPresentationMode::Baseline, original), original);
        }
    }

    void driverRequestDoesNotRequireAlreadyActiveScanout()
    {
        const auto requested = streamPresentationVrrPolicy(2u | 4u);
        QVERIFY(requested.possible);
        QVERIFY(requested.requested);
        const auto enabled = streamPresentationVrrPolicy(1u);
        QVERIFY(enabled.possible);
        QVERIFY(enabled.requested);
        const auto capableOnly = streamPresentationVrrPolicy(2u);
        QVERIFY(capableOnly.possible);
        QVERIFY(!capableOnly.requested);
        const auto scanoutOnly = streamPresentationVrrPolicy(16u);
        QVERIFY(!scanoutOnly.possible);
        QVERIFY(!scanoutOnly.requested);
        const auto unavailable = streamPresentationVrrPolicy(0);
        QVERIFY(!unavailable.possible);
        QVERIFY(!unavailable.requested);
    }

    void recreatesFlagsWithoutChangingFormatOrProxy()
    {
        FakeSwapChain sc;
        const auto result = recreateStreamPresentationSwapChain(sc, 0x19);
        QVERIFY(result.created);
        QVERIFY(result.applied);
        QCOMPARE(sc.currentFlags, 0x19);
        QCOMPARE(sc.proxy, 42);
        QCOMPARE(sc.format, 2);
        QCOMPARE(sc.renderPass, 73);
        QCOMPARE(sc.attempts, std::vector<int>{0x19});
        QCOMPARE(sc.destroys, 1);
    }

    void restoresFlagsAfterCreationFailure()
    {
        FakeSwapChain sc;
        sc.failures = 1;
        const auto result = recreateStreamPresentationSwapChain(sc, 0x19);
        QVERIFY(result.created);
        QVERIFY(!result.applied);
        QCOMPARE(sc.currentFlags, 9);
        QCOMPARE(sc.proxy, 42);
        QCOMPARE(sc.attempts, (std::vector<int>{0x19, 9}));
        QCOMPARE(sc.destroys, 2);
    }

    void reportsFailedRollbackWithoutRetryLoop()
    {
        FakeSwapChain sc;
        sc.failures = 2;
        const auto result = recreateStreamPresentationSwapChain(sc, 0x19);
        QVERIFY(!result.created);
        QVERIFY(!result.applied);
        QCOMPARE(sc.currentFlags, 9);
        QCOMPARE(sc.attempts.size(), std::size_t(2));
    }

    void rejectsSilentOutputFormatFallback()
    {
        FakeSwapChain sc;
        int checks = 0;
        const auto result = recreateStreamPresentationSwapChain(sc, 0x19,
            [&] { return ++checks > 1; });
        QVERIFY(result.created);
        QVERIFY(!result.applied);
        QCOMPARE(sc.currentFlags, 9);
        QCOMPARE(checks, 2);
    }

    void restoresPreferenceAcrossHdrNativeRecreation()
    {
        constexpr int noVsync = 0x10;
        for (int initial : {9, 9 | noVsync}) {
            FakeSwapChain sc;
            sc.currentFlags = initial;
            const bool baseline = bool(initial & noVsync);
            QVERIFY(recreateStreamPresentationSwapChain(sc, sc.flags() | noVsync).created);
            sc.destroy();
            sc.setProxyData(42);
            QVERIFY(sc.createOrResize());
            const bool restore = streamPresentationNoVsync(StreamPresentationMode::Baseline, baseline);
            const int flags = (sc.flags() & ~noVsync) | (restore ? noVsync : 0);
            QVERIFY(recreateStreamPresentationSwapChain(sc, flags).created);
            QCOMPARE(sc.flags(), initial);
            QCOMPARE(sc.format, 2);
            QCOMPARE(sc.renderPass, 73);
            QCOMPARE(sc.proxy, 42);
        }
    }

private:
    struct FakeSwapChain {
        int currentFlags = 9;
        int proxy = 42;
        int format = 2;
        int renderPass = 73;
        int failures = 0;
        int destroys = 0;
        std::vector<int> attempts;
        int flags() const { return currentFlags; }
        int proxyData() const { return proxy; }
        void setFlags(int value) { currentFlags = value; }
        void setProxyData(int value) { proxy = value; }
        void destroy() { ++destroys; proxy = 0; }
        bool createOrResize()
        {
            attempts.push_back(currentFlags);
            if (failures == 0) return true;
            --failures;
            return false;
        }
    };
};

QTEST_GUILESS_MAIN(StreamPresentationTests)
#include "tst_streampresentation.moc"
