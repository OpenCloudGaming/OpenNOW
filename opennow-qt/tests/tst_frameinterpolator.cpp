#include "streaming/rendering/StreamFrameInterpolator.h"

#include <QGuiApplication>
#include <QImage>
#include <QTest>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>
#include <atomic>
#include <cmath>
#include <cstring>
#include <memory>

class FrameInterpolatorTest : public QObject
{
    Q_OBJECT
private:
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    QVulkanInstance m_instance;
#endif
    std::unique_ptr<QRhi> m_rhi;
    std::atomic<int> m_validationErrors = 0;

    static QImage scene(QSize size, int shift)
    {
        QImage image(size, QImage::Format_RGBA8888);
        for (int y = 0; y < size.height(); ++y) {
            auto *row = image.scanLine(y);
            for (int x = 0; x < size.width(); ++x) {
                const double px = x - shift;
                row[x * 4] = uchar(128 + 52 * std::sin(px * 0.091 + y * 0.033)
                                      + 48 * std::cos(px * 0.047 - y * 0.077));
                row[x * 4 + 1] = uchar(128 + 53 * std::cos(px * 0.067 + y * 0.059)
                                          + 42 * std::sin(px * 0.029 - y * 0.113));
                row[x * 4 + 2] = uchar(128 + 51 * std::sin(px * 0.053 - y * 0.043)
                                          + 43 * std::cos(px * 0.107 + y * 0.023));
                row[x * 4 + 3] = 255;
            }
        }
        return image;
    }

    QImage submit(StreamFrameInterpolator &interpolator, QRhiTexture *source,
                  const QImage &image, bool midpoint)
    {
        QRhiCommandBuffer *cb = nullptr;
        if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return {};
        auto *updates = m_rhi->nextResourceUpdateBatch();
        updates->uploadTexture(source, image);
        cb->resourceUpdate(updates);
        const bool ingested = interpolator.ingest(cb, source);
        QRhiReadbackResult result;
        bool completed = false;
        if (ingested) {
            auto *output = midpoint ? interpolator.midpointTexture() : interpolator.currentTexture();
            if (output) {
                result.completed = [&completed] { completed = true; };
                auto *readback = m_rhi->nextResourceUpdateBatch();
                readback->readBackTexture(QRhiReadbackDescription(output), &result);
                cb->resourceUpdate(readback);
            }
        }
        const auto ended = m_rhi->endOffscreenFrame();
        if (!ingested || ended != QRhi::FrameOpSuccess || !completed) return {};
        return QImage(reinterpret_cast<const uchar *>(result.data.constData()),
                      result.pixelSize.width(), result.pixelSize.height(), QImage::Format_RGBA8888).copy();
    }

    static double error(const QImage &a, const QImage &b)
    {
        double sum = 0.0;
        int count = 0;
        for (int y = 48; y < a.height() - 48; ++y) {
            for (int x = 64; x < a.width() - 64; ++x) {
                for (int c = 0; c < 3; ++c) {
                    const double d = int(a.constScanLine(y)[x * 4 + c]) - int(b.constScanLine(y)[x * 4 + c]);
                    sum += d * d;
                    ++count;
                }
            }
        }
        return sum / count;
    }

private slots:
    void initTestCase()
    {
#if defined(Q_OS_WIN)
        QRhiD3D11InitParams params;
        m_rhi.reset(QRhi::create(QRhi::D3D11, &params));
#elif QT_CONFIG(metal)
        QRhiMetalInitParams params;
        m_rhi.reset(QRhi::create(QRhi::Metal, &params));
#elif QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        m_instance.setApiVersion(QVersionNumber(1, 1));
        m_instance.setExtensions(QRhiVulkanInitParams::preferredInstanceExtensions());
        if (qEnvironmentVariableIsSet("OPENNOW_FRAMEGEN_VALIDATION")) {
            QVERIFY2(m_instance.supportedLayers().contains("VK_LAYER_KHRONOS_validation"),
                     "Vulkan validation requested but Khronos layer is unavailable");
            m_instance.setLayers({"VK_LAYER_KHRONOS_validation"});
            m_instance.installDebugOutputFilter(QVulkanInstance::DebugUtilsFilter(
                [this](QVulkanInstance::DebugMessageSeverityFlags severity,
                       QVulkanInstance::DebugMessageTypeFlags, const void *) {
                    if (severity.testFlag(QVulkanInstance::ErrorSeverity)) ++m_validationErrors;
                    return false;
                }));
        }
        if (!m_instance.create()) QSKIP("Vulkan instance unavailable");
        QRhiVulkanInitParams params;
        params.inst = &m_instance;
        m_rhi.reset(QRhi::create(QRhi::Vulkan, &params));
#endif
        if (!m_rhi) QSKIP("No supported GPU backend available");
        if (!m_rhi->isTextureFormatSupported(QRhiTexture::RGBA16F, QRhiTexture::RenderTarget))
            QSKIP("Renderable RGBA16F unavailable");
    }

    void cleanupTestCase()
    {
        m_rhi.reset();
        QCOMPARE(m_validationErrors.load(), 0);
    }

    void translatedTextureMovesInsteadOfBlending_data()
    {
        QTest::addColumn<int>("translation");
        QTest::newRow("eight-pixels") << 8;
        QTest::newRow("sixteen-pixels") << 16;
        QTest::newRow("twenty-four-pixels") << 24;
        QTest::newRow("thirty-two-pixels") << 32;
        QTest::newRow("negative-translation") << -24;
        QTest::newRow("sub-flow-pixel") << 10;
    }

    void translatedTextureMovesInsteadOfBlending()
    {
        QFETCH(int, translation);
        const QSize size(320, 224);
        auto source = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, size));
        QVERIFY(source->create());
        StreamFrameInterpolator interpolator;
        QVERIFY(interpolator.initialize(m_rhi.get()));
        const QImage previous = scene(size, 0);
        const QImage current = scene(size, translation);
        const QImage expected = scene(size, translation / 2);
        QCOMPARE(submit(interpolator, source.get(), previous, false), previous);
        QVERIFY(!interpolator.hasPair());
        QVERIFY(!interpolator.midpointTexture());
        const QImage midpoint = submit(interpolator, source.get(), current, true);
        QVERIFY(!midpoint.isNull());
        QVERIFY(interpolator.hasPair());
        QImage blend(size, QImage::Format_RGBA8888);
        for (int y = 0; y < size.height(); ++y)
            for (int x = 0; x < size.width() * 4; ++x)
                blend.scanLine(y)[x] = (int(previous.constScanLine(y)[x]) + int(current.constScanLine(y)[x])) / 2;
        const double midpointError = error(midpoint, expected);
        const double actualError = error(current, expected);
        const double blendError = error(blend, expected);
        qInfo() << "Midpoint/source/blend mean squared errors:" << midpointError << actualError << blendError;
        QVERIFY(midpointError < actualError * 0.45);
        QVERIFY(midpointError < blendError * 0.70);
        const QImage third = submit(interpolator, source.get(), scene(size, translation * 2), true);
        QVERIFY(!third.isNull());
        QVERIFY(error(third, scene(size, translation * 3 / 2)) < actualError * 0.45);
    }

    void detailedTextureProducesIntermediateMotion_data()
    {
        QTest::addColumn<int>("translation");
        QTest::addColumn<QSize>("size");
        for (const int shift : {2, 4, 8, 10, 16})
            QTest::newRow(qPrintable(QString::number(shift))) << shift << QSize(640, 360);
        QTest::newRow("1440p-small-motion") << 4 << QSize(2560, 1440);
    }

    void detailedTextureProducesIntermediateMotion()
    {
        QFETCH(int, translation);
        QFETCH(QSize, size);
        const auto detailed = [&](int shift) {
            QImage image = scene(size, shift);
            for (int y = 0; y < size.height(); ++y) {
                auto *row = image.scanLine(y);
                for (int x = 0; x < size.width(); ++x) {
                    const auto px = std::uint32_t(x - shift);
                    const auto hash = (px * 73856093u) ^ (std::uint32_t(y) * 19349663u);
                    const int detail = int((hash ^ (hash >> 13)) % 97) - 48;
                    for (int channel = 0; channel < 3; ++channel)
                        row[x * 4 + channel] = uchar(qBound(0, int(row[x * 4 + channel]) + detail, 255));
                }
            }
            return image;
        };
        auto source = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, size));
        QVERIFY(source->create());
        StreamFrameInterpolator interpolator;
        QVERIFY(interpolator.initialize(m_rhi.get()));
        const auto previous = detailed(0);
        const auto current = detailed(translation);
        const auto expected = detailed(translation / 2);
        QCOMPARE(submit(interpolator, source.get(), previous, false), previous);
        const auto midpoint = submit(interpolator, source.get(), current, true);
        QVERIFY(!midpoint.isNull());
        const auto midpointError = error(midpoint, expected);
        const auto sourceError = error(current, expected);
        qInfo() << "Detailed midpoint/source errors:" << midpointError << sourceError;
        QVERIFY(midpointError < sourceError * 0.45);
    }

    void sceneCutUsesActualAndResetRetainsAllocation()
    {
        const QSize size(128, 96);
        auto source = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, size));
        QVERIFY(source->create());
        StreamFrameInterpolator interpolator;
        QVERIFY(interpolator.initialize(m_rhi.get()));
        QImage previous(size, QImage::Format_RGBA8888);
        previous.fill(QColor(233, 41, 13));
        QImage current(size, QImage::Format_RGBA8888);
        current.fill(QColor(7, 51, 229));
        QCOMPARE(submit(interpolator, source.get(), previous, false), previous);
        auto *firstHistory = interpolator.currentTexture();
        QCOMPARE(submit(interpolator, source.get(), current, true), current);
        interpolator.reset();
        QVERIFY(!interpolator.currentTexture());
        QVERIFY(!interpolator.midpointTexture());
        QVERIFY(!interpolator.hasPair());
        QCOMPARE(submit(interpolator, source.get(), current, false), current);
        QCOMPARE(interpolator.currentTexture(), firstHistory);
        interpolator.release();
        QVERIFY(!interpolator.currentTexture());
        QVERIFY(!interpolator.hasPair());
    }

    void rgb10PreservesLowBits()
    {
        if (!m_rhi->isTextureFormatSupported(QRhiTexture::RGB10A2, QRhiTexture::RenderTarget))
            QSKIP("Renderable RGB10A2 unavailable");
        const QSize size(64, 64);
        auto source = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGB10A2, size));
        QVERIFY(source->create());
        StreamFrameInterpolator interpolator;
        QVERIFY(interpolator.initialize(m_rhi.get()));
        QByteArray pixels(size.width() * size.height() * 4, Qt::Uninitialized);
        for (int i = 0; i < size.width() * size.height(); ++i) {
            const quint32 pixel = quint32((i % 1024) | (((i + 257) % 1024) << 10)
                                         | (((i + 519) % 1024) << 20)) | 0xc0000000u;
            std::memcpy(pixels.data() + i * 4, &pixel, 4);
        }
        for (int frame = 0; frame < 2; ++frame) {
            QRhiCommandBuffer *cb = nullptr;
            QCOMPARE(m_rhi->beginOffscreenFrame(&cb), QRhi::FrameOpSuccess);
            auto *updates = m_rhi->nextResourceUpdateBatch();
            updates->uploadTexture(source.get(), QRhiTextureUploadDescription({
                QRhiTextureUploadEntry(0, 0, QRhiTextureSubresourceUploadDescription(pixels))}));
            cb->resourceUpdate(updates);
            const bool ingested = interpolator.ingest(cb, source.get());
            QRhiReadbackResult result;
            bool completed = false;
            result.completed = [&completed] { completed = true; };
            if (ingested) {
                auto *output = frame == 0 ? interpolator.currentTexture() : interpolator.midpointTexture();
                auto *readback = m_rhi->nextResourceUpdateBatch();
                readback->readBackTexture(QRhiReadbackDescription(output), &result);
                cb->resourceUpdate(readback);
            }
            QCOMPARE(m_rhi->endOffscreenFrame(), QRhi::FrameOpSuccess);
            QVERIFY(ingested);
            QVERIFY(completed);
            QCOMPARE(result.format, QRhiTexture::RGB10A2);
            QCOMPARE(result.data, pixels);
        }
    }

    void sceneCutRejectsLocallyMatchableMotion()
    {
        const QSize size(320, 224);
        auto source = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, size));
        QVERIFY(source->create());
        StreamFrameInterpolator interpolator;
        QVERIFY(interpolator.initialize(m_rhi.get()));
        QImage previous = scene(size, 0);
        QImage current = scene(size, 32);
        for (int y = 0; y < size.height(); ++y) {
            for (int x = 112; x < size.width(); ++x) {
                previous.setPixelColor(x, y, QColor(245, 15, 21));
                current.setPixelColor(x, y, QColor(11, 19, 243));
            }
        }
        QCOMPARE(submit(interpolator, source.get(), previous, false), previous);
        QCOMPARE(submit(interpolator, source.get(), current, true), current);
    }

    void resourceRecreationAndInvalidInputs()
    {
        StreamFrameInterpolator interpolator;
        QVERIFY(!interpolator.ingest(nullptr, nullptr));
        QVERIFY(!interpolator.initialize(nullptr));
        QVERIFY(interpolator.initialize(m_rhi.get()));
        for (const QSize size : {QSize(96, 80), QSize(192, 128)}) {
            auto source = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, size));
            QVERIFY(source->create());
            const QImage image = scene(size, 0);
            QCOMPARE(submit(interpolator, source.get(), image, false), image);
            QVERIFY(!interpolator.hasPair());
            QCOMPARE(interpolator.currentTexture()->pixelSize(), size);
            QCOMPARE(submit(interpolator, source.get(), image, true), image);
            QVERIFY(interpolator.hasPair());
        }
        auto oversized = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, {4097, 32}));
        auto excessiveArea = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, {4096, 4096}));
        auto unsupported = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::BGRA8, {96, 80}));
        QRhiCommandBuffer *cb = nullptr;
        QCOMPARE(m_rhi->beginOffscreenFrame(&cb), QRhi::FrameOpSuccess);
        const bool acceptedOversized = interpolator.ingest(cb, oversized.get());
        const bool acceptedExcessiveArea = interpolator.ingest(cb, excessiveArea.get());
        const bool acceptedUnsupported = interpolator.ingest(cb, unsupported.get());
        QCOMPARE(m_rhi->endOffscreenFrame(), QRhi::FrameOpSuccess);
        QVERIFY(!acceptedOversized);
        QVERIFY(!acceptedExcessiveArea);
        QVERIFY(!acceptedUnsupported);
        QVERIFY(!interpolator.hasPair());
        QVERIFY(!interpolator.currentTexture());
        QVERIFY(!interpolator.midpointTexture());
        auto source = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8, {96, 80}));
        QVERIFY(source->create());
        QCOMPARE(submit(interpolator, source.get(), scene({96, 80}, 0), false), scene({96, 80}, 0));
        QVERIFY(!interpolator.hasPair());
        QVERIFY(!interpolator.initialize(nullptr));
        QVERIFY(!interpolator.currentTexture());
        QVERIFY(interpolator.initialize(m_rhi.get()));
        QCOMPARE(submit(interpolator, source.get(), scene({96, 80}, 0), false), scene({96, 80}, 0));
    }
};

QTEST_MAIN(FrameInterpolatorTest)
#include "tst_frameinterpolator.moc"
