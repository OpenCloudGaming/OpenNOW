#include "streaming/rendering/StreamFsrUpscaler.h"
#include "streaming/rendering/StreamVideoTextureRenderer.h"

#include <QGuiApplication>
#include <QDir>
#include <QImage>
#include <QTest>
#include <qfloat16.h>
#include <rhi/qrhi_platform.h>
#include <atomic>
#include <cmath>
#include <cstring>

class FsrUpscalerTest : public QObject
{
    Q_OBJECT
private:
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    QVulkanInstance m_instance;
#endif
    std::unique_ptr<QRhi> m_rhi;
    std::atomic<int> m_validationErrors = 0;

    static QImage scene(QSize size)
    {
        QImage image(size, QImage::Format_RGBA8888);
        for (int y = 0; y < size.height(); ++y) {
            for (int x = 0; x < size.width(); ++x) {
                auto *pixel = image.scanLine(y) + x * 4;
                pixel[0] = uchar(128 + 85 * std::sin(x * 0.7 + y * 0.3));
                pixel[1] = uchar(x + y > size.width() ? 210 : 35);
                pixel[2] = uchar(35 + 180 * y / size.height());
                pixel[3] = 255;
            }
        }
        return image;
    }

    std::unique_ptr<QRhiTexture> source(QSize size)
    {
        auto texture = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(
            QRhiTexture::RGBA8, size, 1, QRhiTexture::UsedAsTransferSource));
        if (!texture->create()) return {};
        QRhiCommandBuffer *cb = nullptr;
        if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return {};
        auto *updates = m_rhi->nextResourceUpdateBatch();
        updates->uploadTexture(texture.get(), scene(size));
        cb->resourceUpdate(updates);
        if (m_rhi->endOffscreenFrame() != QRhi::FrameOpSuccess) return {};
        return texture;
    }

    QImage upscale(StreamFsrUpscaler &scaler, QRhiTexture *input, QSize size,
                   bool enabled, bool sdr, int sharpness, quint64 *id = nullptr)
    {
        QRhiCommandBuffer *cb = nullptr;
        if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return {};
        auto *output = scaler.render(m_rhi.get(), cb, input, size, enabled, sdr, sharpness);
        QRhiReadbackResult result;
        bool completed = false;
        if (output) {
            if (id) *id = output->globalResourceId();
            result.completed = [&completed] { completed = true; };
            auto *updates = m_rhi->nextResourceUpdateBatch();
            updates->readBackTexture(QRhiReadbackDescription(output), &result);
            cb->resourceUpdate(updates);
        }
        if (m_rhi->endOffscreenFrame() != QRhi::FrameOpSuccess || !completed) return {};
        return QImage(reinterpret_cast<const uchar *>(result.data.constData()),
            result.pixelSize.width(), result.pixelSize.height(), QImage::Format_RGBA8888).copy();
    }

    QImage compose(StreamVideoTextureRenderer &renderer, QRhiTexture *input, QSize size,
                   bool enabled, bool sdr, int sharpness, QRhiTexture *external = nullptr)
    {
        auto texture = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA8,
            size, 1, QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
        if (!texture->create()) return {};
        auto depth = std::unique_ptr<QRhiRenderBuffer>(m_rhi->newRenderBuffer(
            QRhiRenderBuffer::DepthStencil, size));
        if (!depth->create()) return {};
        QRhiTextureRenderTargetDescription description(QRhiColorAttachment(texture.get()));
        description.setDepthStencilBuffer(depth.get());
        auto target = std::unique_ptr<QRhiTextureRenderTarget>(m_rhi->newTextureRenderTarget(description));
        auto pass = std::unique_ptr<QRhiRenderPassDescriptor>(target->newCompatibleRenderPassDescriptor());
        target->setRenderPassDescriptor(pass.get());
        if (!target->create()) return {};
        renderer.initialize(m_rhi.get(), target.get());
        QMatrix4x4 projection;
        projection.ortho(0.0f, float(size.width()), float(size.height()), 0.0f, -1.0f, 1.0f);
        const QRectF bounds{QPointF(), QSizeF(size)};
        renderer.setComposition(m_rhi->clipSpaceCorrMatrix() * projection, bounds, bounds, 1.0f);
        QRhiCommandBuffer *cb = nullptr;
        if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return {};
        const bool ready = renderer.prepare(cb)
            && renderer.importFrame(0, input->nativeTexture(), input->format(), input->pixelSize())
            && (!external || renderer.selectTexture(external));
        QRhiReadbackResult result;
        bool completed = false;
        if (ready) {
            renderer.prepareUpscaling(cb, size, enabled, sdr, sharpness);
            cb->beginPass(target.get(), Qt::transparent, {1.0f, 0});
            cb->setViewport(QRhiViewport(0, 0, size.width(), size.height()));
            cb->setScissor(QRhiScissor(0, 0, size.width(), size.height()));
            renderer.render(cb, false, 0);
            cb->endPass();
            result.completed = [&completed] { completed = true; };
            auto *updates = m_rhi->nextResourceUpdateBatch();
            updates->readBackTexture(QRhiReadbackDescription(texture.get()), &result);
            cb->resourceUpdate(updates);
        }
        if (m_rhi->endOffscreenFrame() != QRhi::FrameOpSuccess || !completed) return {};
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        if (m_rhi->backend() == QRhi::Vulkan && (!external || external == input))
            input->setNativeLayout(VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL);
#endif
        return QImage(reinterpret_cast<const uchar *>(result.data.constData()),
            result.pixelSize.width(), result.pixelSize.height(), QImage::Format_RGBA8888).copy();
    }

private slots:
    void initTestCase()
    {
#if defined(Q_OS_WIN)
        QRhiD3D11InitParams params;
        m_rhi.reset(QRhi::create(QRhi::D3D11, &params));
#elif QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        m_instance.setApiVersion(QVersionNumber(1, 1));
        m_instance.setExtensions(QRhiVulkanInitParams::preferredInstanceExtensions());
        if (qEnvironmentVariableIsSet("OPENNOW_FSR_VALIDATION")) {
            QVERIFY(m_instance.supportedLayers().contains("VK_LAYER_KHRONOS_validation"));
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
        if (!m_rhi) QSKIP("No supported FSR GPU backend available");
    }

    void cleanupTestCase()
    {
        m_rhi.reset();
        QCOMPARE(m_validationErrors.load(), 0);
    }

    void easuAndRcasChangeOutputWithoutChangingSource()
    {
        auto input = source({64, 48});
        QVERIFY(input);
        StreamFsrUpscaler scaler;
        quint64 easuId = 0, reusedId = 0;
        const auto easu = upscale(scaler, input.get(), {128, 96}, true, true, 0, &easuId);
        QCOMPARE(easu.size(), QSize(128, 96));
        QCOMPARE(upscale(scaler, input.get(), {128, 96}, true, true, 0, &reusedId), easu);
        QCOMPARE(reusedId, easuId);
        const auto sharp = upscale(scaler, input.get(), {128, 96}, true, true, 15);
        QCOMPARE(sharp.size(), easu.size());
        QVERIFY(sharp != easu);
        QVERIFY(upscale(scaler, input.get(), {128, 96}, true, true, 1) != sharp);
        QCOMPARE(upscale(scaler, input.get(), {128, 96}, true, true, 0), easu);
        QVERIFY(easu.pixelColor(64, 10).blue() < easu.pixelColor(64, 85).blue());
        QCOMPARE(upscale(scaler, input.get(), {128, 96}, false, true, 15), scene({64, 48}));
    }

    void disabledHdrAndUnsupportedTargetsKeepOriginal_data()
    {
        QTest::addColumn<QSize>("size");
        QTest::addColumn<bool>("enabled");
        QTest::addColumn<bool>("sdr");
        QTest::newRow("off") << QSize(128, 96) << false << true;
        QTest::newRow("hdr") << QSize(128, 96) << true << false;
        QTest::newRow("same-size") << QSize(64, 48) << true << true;
        QTest::newRow("downscale") << QSize(32, 24) << true << true;
        QTest::newRow("empty") << QSize() << true << true;
        QTest::newRow("oversized") << QSize(32768, 32768) << true << true;
        QTest::newRow("mixed-scale") << QSize(128, 24) << true << true;
    }

    void disabledHdrAndUnsupportedTargetsKeepOriginal()
    {
        QFETCH(QSize, size);
        QFETCH(bool, enabled);
        QFETCH(bool, sdr);
        auto input = source({64, 48});
        QVERIFY(input);
        StreamFsrUpscaler scaler;
        QCOMPARE(upscale(scaler, input.get(), {128, 96}, true, true, 15).size(), QSize(128, 96));
        quint64 id = 0;
        QCOMPARE(upscale(scaler, input.get(), size, enabled, sdr, 15, &id), scene({64, 48}));
        QCOMPARE(id, input->globalResourceId());
    }

    void finalCompositionPreservesOffAndHdrFallback()
    {
        auto input = source({64, 48});
        QVERIFY(input);
        StreamVideoTextureRenderer renderer;
        const auto normal = compose(renderer, input.get(), {128, 96}, false, true, 0);
        QCOMPARE(normal.size(), QSize(128, 96));
        const auto easu = compose(renderer, input.get(), {128, 96}, true, true, 0);
        QCOMPARE(easu.size(), normal.size());
        QVERIFY(easu != normal);
        const auto sharp = compose(renderer, input.get(), {128, 96}, true, true, 15);
        QVERIFY(sharp != easu);
        const auto dump = qEnvironmentVariable("OPENNOW_FSR_IMAGE_DIR");
        if (!dump.isEmpty()) {
            QVERIFY(QDir().mkpath(dump));
            QVERIFY(normal.save(dump + QStringLiteral("/normal.png")));
            QVERIFY(easu.save(dump + QStringLiteral("/easu.png")));
            QVERIFY(sharp.save(dump + QStringLiteral("/rcas.png")));
        }
        QCOMPARE(compose(renderer, input.get(), {128, 96}, false, true, 15), normal);
        QCOMPARE(compose(renderer, input.get(), {128, 96}, true, false, 15), normal);
        QCOMPARE(compose(renderer, input.get(), {64, 48}, true, true, 15),
                 compose(renderer, input.get(), {64, 48}, false, true, 15));
        renderer.clearFrames();
        QCOMPARE(compose(renderer, input.get(), {128, 96}, true, true, 0), easu);
        QCOMPARE(compose(renderer, input.get(), {128, 96}, true, true, 15, input.get()), sharp);
        renderer.clearExternalTextures();
        auto external = source({48, 32});
        QVERIFY(external);
        const auto selected = compose(renderer, external.get(), {128, 96}, true, true, 15);
        QVERIFY(selected != sharp);
        QCOMPARE(compose(renderer, input.get(), {128, 96}, true, true, 15, external.get()), selected);
        renderer.clearFrames();
    }

    void resizeAndSessionReleaseRecreateOnlyRequiredResources()
    {
        auto input = source({64, 48});
        QVERIFY(input);
        StreamFsrUpscaler scaler;
        quint64 first = 0, resized = 0, restarted = 0;
        QCOMPARE(upscale(scaler, input.get(), {128, 96}, true, true, 0, &first).size(), QSize(128, 96));
        QCOMPARE(upscale(scaler, input.get(), {192, 144}, true, true, 0, &resized).size(), QSize(192, 144));
        QVERIFY(first != resized);
        scaler.release();
        QCOMPARE(upscale(scaler, input.get(), {192, 144}, true, true, 0, &restarted).size(), QSize(192, 144));
        QVERIFY(resized != restarted);
    }

    void floatFlatFieldsStayFinite_data()
    {
        QTest::addColumn<float>("value");
        QTest::newRow("black") << 0.0f;
        QTest::newRow("white") << 1.0f;
        QTest::newRow("gray") << 0.5f;
    }

    void floatFlatFieldsStayFinite()
    {
        QFETCH(float, value);
        if (!m_rhi->isTextureFormatSupported(QRhiTexture::RGBA16F, QRhiTexture::RenderTarget))
            QSKIP("RGBA16F unavailable");
        auto input = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(QRhiTexture::RGBA16F, {8, 8}));
        QVERIFY(input->create());
        std::array<qfloat16, 8 * 8 * 4> pixels;
        for (size_t i = 0; i < pixels.size(); ++i) pixels[i] = qfloat16(i % 4 == 3 ? 1.0f : value);
        StreamFsrUpscaler scaler;
        for (int sharpness : {0, 15}) {
            QRhiCommandBuffer *cb = nullptr;
            QCOMPARE(m_rhi->beginOffscreenFrame(&cb), QRhi::FrameOpSuccess);
            auto *updates = m_rhi->nextResourceUpdateBatch();
            updates->uploadTexture(input.get(), QRhiTextureUploadDescription({
                QRhiTextureUploadEntry(0, 0, QRhiTextureSubresourceUploadDescription(
                    QByteArray(reinterpret_cast<const char *>(pixels.data()), sizeof(pixels))))}));
            cb->resourceUpdate(updates);
            auto *output = scaler.render(m_rhi.get(), cb, input.get(), {16, 16}, true, true, sharpness);
            QRhiReadbackResult result;
            bool completed = false;
            result.completed = [&completed] { completed = true; };
            updates = m_rhi->nextResourceUpdateBatch();
            updates->readBackTexture(QRhiReadbackDescription(output), &result);
            cb->resourceUpdate(updates);
            QCOMPARE(m_rhi->endOffscreenFrame(), QRhi::FrameOpSuccess);
            QVERIFY(completed);
            QCOMPARE(result.pixelSize, QSize(16, 16));
            QCOMPARE(result.format, QRhiTexture::RGBA16F);
            QCOMPARE(result.data.size(), 16 * 16 * 4 * qsizetype(sizeof(qfloat16)));
            for (int i = 0; i < 16 * 16 * 4; ++i) {
                qfloat16 channel;
                std::memcpy(&channel, result.data.constData() + i * sizeof(channel), sizeof(channel));
                QVERIFY(std::isfinite(float(channel)));
                QVERIFY(std::abs(float(channel) - (i % 4 == 3 ? 1.0f : value)) < 0.004f);
            }
        }
    }
};

int main(int argc, char **argv)
{
    QGuiApplication application(argc, argv);
    FsrUpscalerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_fsrupscaler.moc"
