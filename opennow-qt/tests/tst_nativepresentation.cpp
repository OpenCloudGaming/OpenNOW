#include "streaming/NativeStreamRuntime.h"

#include <QTest>

namespace {
OpenNowStreamerStatus create(const OpenNowStreamerConfig *, OpenNowStreamer **output)
{
    *output = reinterpret_cast<OpenNowStreamer *>(new int(1));
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus destroy(OpenNowStreamer *handle)
{
    delete reinterpret_cast<int *>(handle);
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus send(const OpenNowStreamer *, const std::uint8_t *, std::size_t)
{
    return OPENNOW_STREAMER_OK;
}

NativeStreamRuntime::Api api()
{
    NativeStreamRuntime::Api result;
    result.create = &create;
    result.destroy = &destroy;
    result.send = &send;
    return result;
}
}

class NativePresentationTest : public QObject
{
    Q_OBJECT
private slots:
    void absentOrStoppedRuntimeCannotAdvertisePacing()
    {
        NativeStreamRuntime runtime(api());
        QVERIFY(!runtime.presentationConfig().active);
        QVERIFY(runtime.start());
        QVERIFY(!runtime.presentationConfig().active);
        OpenNowStreamerPresentationSample sample{};
        QCOMPARE(runtime.submitPresentationSample(sample), OPENNOW_STREAMER_CLOSED);
    }

    void readsTheNativeNegotiatedConfigWithoutParsingSessionJson()
    {
        auto native = api();
        native.getPresentationConfig = [](const OpenNowStreamer *, OpenNowStreamerPresentationConfig *config) {
            if (config->version != OPENNOW_STREAMER_PRESENTATION_VERSION
                    || config->struct_size != sizeof(*config)) return OPENNOW_STREAMER_INVALID_CONFIG;
            config->active = 1;
            config->generation = 41;
            config->scene_epoch = 3;
            config->fps = 120;
            config->cloud_gsync = 0;
            config->feedback_mode = 1;
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(native);
        QVERIFY(runtime.start());
        const auto config = runtime.presentationConfig();
        QCOMPARE(config.generation, std::uint64_t(41));
        QCOMPARE(config.scene_epoch, std::uint64_t(3));
        QCOMPARE(config.fps, std::uint32_t(120));
        QCOMPARE(config.cloud_gsync, std::uint32_t(0));
        QCOMPARE(config.feedback_mode, std::uint32_t(1));
        QVERIFY(runtime.shutdown());
        QVERIFY(!runtime.presentationConfig().active);
    }

    void failureDoesNotLeakPartiallyFilledConfiguration()
    {
        auto native = api();
        native.getPresentationConfig = [](const OpenNowStreamer *, OpenNowStreamerPresentationConfig *config) {
            config->active = 1;
            config->cloud_gsync = 1;
            return OPENNOW_STREAMER_CLOSED;
        };
        NativeStreamRuntime runtime(native);
        QVERIFY(runtime.start());
        QVERIFY(!runtime.presentationConfig().active);
        QVERIFY(!runtime.presentationConfig().cloud_gsync);
    }

    void preservesFrameIdentityEpochAndTimingProvenance()
    {
        auto native = api();
        native.submitPresentationSample = [](const OpenNowStreamer *, const OpenNowStreamerPresentationSample *sample) {
            return sample->generation == 41 && sample->scene_epoch == 3
                    && sample->presentation_time_ns == 700 && sample->completion_age_ns == 900
                    && sample->render_duration_ns == 100 && sample->render_present_to_complete_ns == 200
                    && sample->vsync_interval_ns == 0
                    && sample->completion_kind == OPENNOW_STREAMER_PRESENTATION_PLATFORM_PRESENTED
                    && sample->valid_fields == OPENNOW_STREAMER_PRESENTATION_REQUIRED_FIELDS
                ? OPENNOW_STREAMER_OK : OPENNOW_STREAMER_INVALID_CONFIG;
        };
        NativeStreamRuntime runtime(native);
        QVERIFY(runtime.start());
        OpenNowStreamerPresentationSample sample{};
        sample.version = OPENNOW_STREAMER_PRESENTATION_VERSION;
        sample.struct_size = sizeof(sample);
        sample.generation = 41;
        sample.scene_epoch = 3;
        sample.presentation_time_ns = 700;
        sample.render_duration_ns = 100;
        sample.render_present_to_complete_ns = 200;
        sample.completion_age_ns = 900;
        sample.completion_kind = OPENNOW_STREAMER_PRESENTATION_PLATFORM_PRESENTED;
        sample.valid_fields = OPENNOW_STREAMER_PRESENTATION_REQUIRED_FIELDS;
        QCOMPARE(runtime.submitPresentationSample(sample), OPENNOW_STREAMER_OK);
        QVERIFY(runtime.shutdown());
        QCOMPARE(runtime.submitPresentationSample(sample), OPENNOW_STREAMER_CLOSED);
    }
};

QTEST_GUILESS_MAIN(NativePresentationTest)
#include "tst_nativepresentation.moc"
