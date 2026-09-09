#include "streaming/rendering/StreamVideoTextureRenderer.h"

#include <QGuiApplication>
#include <QTest>
#include <qfloat16.h>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>
#include <atomic>
#include <cmath>
#include <cstring>
#include <memory>

class StreamColorTest : public QObject
{
    Q_OBJECT
private:
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    QVulkanInstance m_instance;
#endif
    std::unique_ptr<QRhi> m_rhi;
    std::atomic<int> m_validationErrors = 0;
    static constexpr int tileSize = 8;

    struct Target {
        std::unique_ptr<QRhiTexture> texture;
        std::unique_ptr<QRhiRenderBuffer> depthStencil;
        std::unique_ptr<QRhiRenderPassDescriptor> pass;
        std::unique_ptr<QRhiTextureRenderTarget> target;
    };

    std::unique_ptr<Target> makeTarget(QRhiTexture::Format format, QSize size)
    {
        auto result = std::make_unique<Target>();
        result->texture.reset(m_rhi->newTexture(
            format, size, 1, QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
        if (!result->texture->create()) return {};
        result->depthStencil.reset(m_rhi->newRenderBuffer(QRhiRenderBuffer::DepthStencil, size));
        if (!result->depthStencil->create()) return {};
        QRhiTextureRenderTargetDescription description(QRhiColorAttachment(result->texture.get()));
        description.setDepthStencilBuffer(result->depthStencil.get());
        result->target.reset(m_rhi->newTextureRenderTarget(description));
        result->pass.reset(result->target->newCompatibleRenderPassDescriptor());
        result->target->setRenderPassDescriptor(result->pass.get());
        if (!result->target->create()) return {};
        return result;
    }

    static QByteArray patches(const QList<int> &codes, bool tenBit)
    {
        const int width = int(codes.size()) * tileSize;
        QByteArray data(width * tileSize * 4, Qt::Uninitialized);
        for (int y = 0; y < tileSize; ++y) {
            for (int x = 0; x < width; ++x) {
                const quint32 code = quint32(codes[x / tileSize]);
                char *pixel = data.data() + (y * width + x) * 4;
                if (tenBit) {
                    const quint32 packed = code | (code << 10) | (code << 20) | (3u << 30);
                    std::memcpy(pixel, &packed, sizeof(packed));
                } else {
                    pixel[0] = pixel[1] = pixel[2] = char(code);
                    pixel[3] = char(255);
                }
            }
        }
        return data;
    }

    std::unique_ptr<QRhiTexture> makeSource(const QList<int> &codes, bool tenBit = true)
    {
        const QSize size(int(codes.size()) * tileSize, tileSize);
        auto texture = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(
            tenBit ? QRhiTexture::RGB10A2 : QRhiTexture::RGBA8, size, 1,
            QRhiTexture::UsedAsTransferSource));
        if (!texture->create()) return {};
        QRhiCommandBuffer *cb = nullptr;
        if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return {};
        auto *updates = m_rhi->nextResourceUpdateBatch();
        updates->uploadTexture(texture.get(), QRhiTextureUploadDescription({
            QRhiTextureUploadEntry(0, 0, QRhiTextureSubresourceUploadDescription(patches(codes, tenBit)))
        }));
        cb->resourceUpdate(updates);
        if (m_rhi->endOffscreenFrame() != QRhi::FrameOpSuccess) return {};
        return texture;
    }

    QByteArray render(StreamVideoTextureRenderer &renderer, QRhiTexture *source,
                      Target &target, QRhiTexture *external = nullptr)
    {
        const QSize size = target.texture->pixelSize();
        renderer.initialize(m_rhi.get(), target.target.get());
        QMatrix4x4 projection;
        projection.ortho(0.0f, float(size.width()), float(size.height()), 0.0f, -1.0f, 1.0f);
        const QRectF bounds(QPointF(0, 0), QSizeF(size));
        renderer.setComposition(m_rhi->clipSpaceCorrMatrix() * projection, bounds, bounds, 1.0f);
        QRhiCommandBuffer *cb = nullptr;
        if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return {};
        const bool ready = renderer.prepare(cb)
            && renderer.importFrame(0, source->nativeTexture(), source->format(), source->pixelSize())
            && (!external || renderer.selectTexture(external));
        QRhiReadbackResult result;
        bool completed = false;
        if (ready) {
            cb->beginPass(target.target.get(), QColor(255, 0, 255), {1.0f, 0});
            cb->setViewport(QRhiViewport(0, 0, float(size.width()), float(size.height())));
            cb->setScissor(QRhiScissor(0, 0, size.width(), size.height()));
            renderer.render(cb, false, 0);
            cb->endPass();
            result.completed = [&completed] { completed = true; };
            auto *readback = m_rhi->nextResourceUpdateBatch();
            readback->readBackTexture(QRhiReadbackDescription(target.texture.get()), &result);
            cb->resourceUpdate(readback);
        }
        const auto ended = m_rhi->endOffscreenFrame();
        if (!ready || ended != QRhi::FrameOpSuccess || !completed
            || result.pixelSize != size || result.format != target.texture->format()) return {};
        return result.data;
    }

    static double mean(const QByteArray &data, int patch, int patchCount)
    {
        int sum = 0;
        for (int y = 0; y < tileSize; ++y)
            for (int x = 0; x < tileSize; ++x)
                sum += quint8(data[(y * patchCount * tileSize + patch * tileSize + x) * 4]);
        return double(sum) / (tileSize * tileSize);
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
        if (qEnvironmentVariableIsSet("OPENNOW_STREAMCOLOR_VALIDATION")) {
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
        if (!m_rhi->isTextureFormatSupported(QRhiTexture::RGB10A2))
            QSKIP("RGB10A2 source textures unavailable");
    }

    void cleanupTestCase()
    {
        m_rhi.reset();
        QCOMPARE(m_validationErrors.load(), 0);
    }

    void fractionalGrayPreservesMeanAndAdjacentCodes()
    {
        const QList<int> codes{1, 2, 3, 4, 127, 128, 255, 256, 510, 511, 512, 513, 514, 768, 1020, 1021, 1022};
        auto source = makeSource(codes);
        QVERIFY(source);
        auto target = makeTarget(QRhiTexture::RGBA8, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        const auto data = render(renderer, source.get(), *target);
        QCOMPARE(data.size(), source->pixelSize().width() * tileSize * 4);
        for (int patch = 0; patch < codes.size(); ++patch) {
            const double expected = codes[patch] * 255.0 / 1023.0;
            const double actual = mean(data, patch, int(codes.size()));
            QVERIFY2(std::abs(actual - expected) <= 0.025,
                     qPrintable(QString("10-bit code %1: mean %2, expected %3")
                                    .arg(codes[patch]).arg(actual).arg(expected)));
            if (patch > 0 && codes[patch] == codes[patch - 1] + 1)
                QVERIFY(actual > mean(data, patch - 1, int(codes.size())) + 0.20);
            for (int y = 0; y < tileSize; ++y) {
                for (int x = 0; x < tileSize; ++x) {
                    const int offset = (y * source->pixelSize().width() + patch * tileSize + x) * 4;
                    const int value = quint8(data[offset]);
                    QVERIFY(value == int(std::floor(expected)) || value == int(std::ceil(expected)));
                    QCOMPARE(quint8(data[offset + 1]), value);
                    QCOMPARE(quint8(data[offset + 2]), value);
                    QCOMPARE(quint8(data[offset + 3]), 255);
                }
            }
        }
    }

    void blackAndWhiteAreExact()
    {
        auto source = makeSource({0, 1023});
        QVERIFY(source);
        auto target = makeTarget(QRhiTexture::RGBA8, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        QCOMPARE(render(renderer, source.get(), *target), patches({0, 255}, false));
    }

    void tenBitRgbPreservesIndependentFullResolutionChannels()
    {
        auto source = makeSource({0});
        QVERIFY(source);
        QByteArray pixels(tileSize * tileSize * 4, Qt::Uninitialized);
        for (int pixel = 0; pixel < tileSize * tileSize; ++pixel) {
            const quint32 red = pixel % 2 ? 513 : 512;
            const quint32 green = pixel % 2 ? 1023 : 0;
            const quint32 blue = pixel % 2 ? 0 : 1023;
            const quint32 packed = red | (green << 10) | (blue << 20) | (3u << 30);
            std::memcpy(pixels.data() + pixel * 4, &packed, sizeof(packed));
        }
        QRhiCommandBuffer *cb = nullptr;
        QCOMPARE(m_rhi->beginOffscreenFrame(&cb), QRhi::FrameOpSuccess);
        auto *updates = m_rhi->nextResourceUpdateBatch();
        updates->uploadTexture(source.get(), QRhiTextureUploadDescription({
            QRhiTextureUploadEntry(0, 0, QRhiTextureSubresourceUploadDescription(pixels))
        }));
        cb->resourceUpdate(updates);
        QCOMPARE(m_rhi->endOffscreenFrame(), QRhi::FrameOpSuccess);
        auto target = makeTarget(QRhiTexture::RGBA16F, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        const auto data = render(renderer, source.get(), *target);
        QCOMPARE(data.size(), tileSize * tileSize * 8);
        const auto *values = reinterpret_cast<const qfloat16 *>(data.constData());
        for (int pixel = 0; pixel < tileSize * tileSize; ++pixel) {
            QVERIFY(std::abs(float(values[pixel * 4]) - (pixel % 2 ? 513.0f : 512.0f) / 1023.0f) < 0.0005f);
            QCOMPARE(float(values[pixel * 4 + 1]), pixel % 2 ? 1.0f : 0.0f);
            QCOMPARE(float(values[pixel * 4 + 2]), pixel % 2 ? 0.0f : 1.0f);
            QCOMPARE(float(values[pixel * 4 + 3]), 1.0f);
        }
    }

    void hdrToneMappingPrecedesFinalSdrDither()
    {
        const QList<int> codes{128, 256, 384, 512, 580, 581, 582, 768, 769, 770, 896};
        auto source = makeSource(codes);
        QVERIFY(source);
        auto target = makeTarget(QRhiTexture::RGBA8, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        renderer.setColorSpace(1, 0, 203.0f, false);
        const auto data = render(renderer, source.get(), *target);
        QCOMPARE(data.size(), source->pixelSize().width() * tileSize * 4);
        for (int patch = 0; patch < codes.size(); ++patch) {
            const double p = std::pow(codes[patch] / 1023.0, 1.0 / 78.84375);
            const double nits = 10000.0 * std::pow(std::max(p - 0.8359375, 0.0)
                / (18.8515625 - 18.6875 * p), 1.0 / 0.1593017578125);
            const double linear = nits / (203.0 + nits);
            const double expected = 255.0 * (linear <= 0.0031308 ? linear * 12.92
                : 1.055 * std::pow(linear, 1.0 / 2.4) - 0.055);
            const double actual = mean(data, patch, int(codes.size()));
            QVERIFY2(std::abs(actual - expected) <= 0.04,
                     qPrintable(QString("PQ code %1: SDR mean %2, expected %3")
                         .arg(codes[patch]).arg(actual).arg(expected)));
        }
    }

    void hdrLinearOutputIsNotSdrClampedOrDithered()
    {
        auto source = makeSource({769});
        QVERIFY(source);
        auto target = makeTarget(QRhiTexture::RGBA16F, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        renderer.setColorSpace(1, 1, 203.0f, true);
        const auto data = render(renderer, source.get(), *target);
        QCOMPARE(data.size(), source->pixelSize().width() * tileSize * 8);
        const auto *pixels = reinterpret_cast<const qfloat16 *>(data.constData());
        const float first = float(pixels[0]);
        QVERIFY(first > 10.0f);
        for (int pixel = 0; pixel < source->pixelSize().width() * tileSize; ++pixel) {
            QCOMPARE(float(pixels[pixel * 4]), first);
            QVERIFY(std::abs(float(pixels[pixel * 4 + 1]) - first) < 0.01f);
            QVERIFY(std::abs(float(pixels[pixel * 4 + 2]) - first) < 0.01f);
            QCOMPARE(float(pixels[pixel * 4 + 3]), 1.0f);
        }
    }

    void exactEightBitInputsAreUnchanged()
    {
        QList<int> codes;
        for (int code = 0; code <= 255; ++code) codes.append(code);
        auto source = makeSource(codes, false);
        QVERIFY(source);
        auto target = makeTarget(QRhiTexture::RGBA8, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        QCOMPARE(render(renderer, source.get(), *target), patches(codes, false));
    }

    void highPrecisionTargetRetainsCodesWithoutDither()
    {
        if (!m_rhi->isTextureFormatSupported(QRhiTexture::RGB10A2, QRhiTexture::RenderTarget))
            QSKIP("Renderable RGB10A2 unavailable");
        const QList<int> codes{0, 1, 2, 3, 4, 127, 128, 255, 256, 510, 511, 512, 513, 514, 768, 1021, 1022, 1023};
        auto source = makeSource(codes);
        QVERIFY(source);
        auto target = makeTarget(QRhiTexture::RGB10A2, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        QCOMPARE(render(renderer, source.get(), *target), patches(codes, true));
    }

    void targetFormatSwitchingUpdatesDither()
    {
        if (!m_rhi->isTextureFormatSupported(QRhiTexture::RGB10A2, QRhiTexture::RenderTarget))
            QSKIP("Renderable RGB10A2 unavailable");
        const QList<int> codes{511, 512, 513};
        auto source = makeSource(codes);
        QVERIFY(source);
        auto eightBit = makeTarget(QRhiTexture::RGBA8, source->pixelSize());
        auto tenBit = makeTarget(QRhiTexture::RGB10A2, source->pixelSize());
        QVERIFY(eightBit);
        QVERIFY(tenBit);
        StreamVideoTextureRenderer renderer;
        const auto first = render(renderer, source.get(), *eightBit);
        QCOMPARE(first.size(), patches(codes, false).size());
        QVERIFY(std::abs(mean(first, 1, 3) - 512.0 * 255.0 / 1023.0) <= 0.025);
        QCOMPARE(render(renderer, source.get(), *tenBit), patches(codes, true));
        QCOMPARE(render(renderer, source.get(), *eightBit), first);
        QCOMPARE(render(renderer, source.get(), *tenBit), patches(codes, true));
    }

    void externalTextureMatchesImportWithoutAccumulation()
    {
        const QList<int> codes{1, 128, 511, 512, 513, 1022};
        auto source = makeSource(codes);
        auto external = makeSource(codes);
        QVERIFY(source);
        QVERIFY(external);
        auto target = makeTarget(QRhiTexture::RGBA8, source->pixelSize());
        QVERIFY(target);
        StreamVideoTextureRenderer renderer;
        const auto imported = render(renderer, source.get(), *target);
        QCOMPARE(imported.size(), patches(codes, false).size());
        QVERIFY(std::abs(mean(imported, 3, int(codes.size())) - 512.0 * 255.0 / 1023.0) <= 0.025);
        for (int frame = 0; frame < 8; ++frame)
            QCOMPARE(render(renderer, source.get(), *target, external.get()), imported);
        renderer.clearExternalTextures();
        QCOMPARE(render(renderer, source.get(), *target), imported);
        if (m_rhi->isTextureFormatSupported(QRhiTexture::RGB10A2, QRhiTexture::RenderTarget)) {
            auto highPrecision = makeTarget(QRhiTexture::RGB10A2, source->pixelSize());
            QVERIFY(highPrecision);
            QCOMPARE(render(renderer, source.get(), *highPrecision, external.get()), patches(codes, true));
        }
    }
};

QTEST_MAIN(StreamColorTest)
#include "tst_streamcolor.moc"
