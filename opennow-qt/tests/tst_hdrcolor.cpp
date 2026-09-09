#include <QFile>
#include "streaming/rendering/HdrOutputPass.h"
#include "streaming/rendering/HdrOutput.h"
#include "streaming/rendering/HdrChromeEffect.h"
#include <QQmlEngine>
#include <QQmlComponent>
#include <QQuickWindow>
#include <QQuickGraphicsConfiguration>
#include <QGuiApplication>
#include <QTest>
#include <QVector3D>
#include <qfloat16.h>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>
#include <cmath>
#include <memory>

class HdrColorTest final : public QObject
{
    Q_OBJECT
    std::unique_ptr<QRhi> m_rhi;
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    QVulkanInstance m_instance;
#endif

    static QShader shader(const char *path)
    {
        QFile file(QString::fromLatin1(path));
        if (!file.open(QIODevice::ReadOnly)) return {};
        return QShader::fromSerialized(file.readAll());
    }

    QVector3D convert(QVector3D input, int source, int output, bool supported = true)
    {
        const QSize size(4, 4);
        std::unique_ptr<QRhiTexture> texture(m_rhi->newTexture(QRhiTexture::RGBA16F, size, 1,
            QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
        if (!texture->create()) return {};
        std::unique_ptr<QRhiTextureRenderTarget> target(m_rhi->newTextureRenderTarget({texture.get()}));
        std::unique_ptr<QRhiRenderPassDescriptor> pass(target->newCompatibleRenderPassDescriptor());
        target->setRenderPassDescriptor(pass.get());
        if (!target->create()) return {};
        std::unique_ptr<QRhiBuffer> uniforms(m_rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 32));
        if (!uniforms->create()) return {};
        std::unique_ptr<QRhiShaderResourceBindings> bindings(m_rhi->newShaderResourceBindings());
        bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0, QRhiShaderResourceBinding::FragmentStage, uniforms.get())});
        if (!bindings->create()) return {};
        std::unique_ptr<QRhiGraphicsPipeline> pipeline(m_rhi->newGraphicsPipeline());
        pipeline->setShaderStages({{QRhiShaderStage::Vertex, shader(":/hdr-test/hdrcolor_test.vert.qsb")},
                                   {QRhiShaderStage::Fragment, shader(":/hdr-test/hdrcolor_test.frag.qsb")}});
        pipeline->setShaderResourceBindings(bindings.get());
        pipeline->setRenderPassDescriptor(pass.get());
        if (!pipeline->create()) return {};
        QRhiCommandBuffer *cb = nullptr;
        if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return {};
        const float values[] = {input.x(), input.y(), input.z(), 1.0f,
                                float(source), float(output), 203.0f, supported ? 1.0f : 0.0f};
        auto *updates = m_rhi->nextResourceUpdateBatch();
        updates->updateDynamicBuffer(uniforms.get(), 0, sizeof(values), values);
        cb->beginPass(target.get(), Qt::black, {1.0f, 0}, updates);
        cb->setGraphicsPipeline(pipeline.get());
        cb->setViewport({0, 0, 4, 4});
        cb->setShaderResources();
        cb->draw(3);
        cb->endPass();
        QRhiReadbackResult result;
        bool complete = false;
        result.completed = [&] { complete = true; };
        auto *readback = m_rhi->nextResourceUpdateBatch();
        readback->readBackTexture(QRhiReadbackDescription(texture.get()), &result);
        cb->resourceUpdate(readback);
        if (m_rhi->endOffscreenFrame() != QRhi::FrameOpSuccess || !complete || result.data.size() < 8) return {};
        const auto *pixel = reinterpret_cast<const qfloat16 *>(result.data.constData());
        return {float(pixel[0]), float(pixel[1]), float(pixel[2])};
    }

    static float pq(float nits)
    {
        const double p = std::pow(double(nits) / 10000.0, 0.1593017578125);
        return float(std::pow((0.8359375 + 18.8515625 * p) / (1.0 + 18.6875 * p), 78.84375));
    }

public:
    static void initMain()
    {
#if defined(Q_OS_WIN)
        QGuiApplication::setDesktopSettingsAware(false);
#endif
    }

private slots:
    void initTestCase()
    {
#if defined(Q_OS_WIN)
        QQuickWindow::setGraphicsApi(QSGRendererInterface::Direct3D11);
        QRhiD3D11InitParams params;
        m_rhi.reset(QRhi::create(QRhi::D3D11, &params, QRhi::PreferSoftwareRenderer));
#elif QT_CONFIG(metal)
        QQuickWindow::setGraphicsApi(QSGRendererInterface::Metal);
        QRhiMetalInitParams params;
        m_rhi.reset(QRhi::create(QRhi::Metal, &params));
#elif QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        QQuickWindow::setGraphicsApi(QSGRendererInterface::Vulkan);
        m_instance.setApiVersion(QVersionNumber(1, 1));
        m_instance.setExtensions(QRhiVulkanInitParams::preferredInstanceExtensions());
        QVERIFY(m_instance.create());
        QRhiVulkanInitParams params;
        params.inst = &m_instance;
        m_rhi.reset(QRhi::create(QRhi::Vulkan, &params));
#endif
        QVERIFY2(m_rhi, "An actual GPU backend is required for HDR shader verification");
        QVERIFY(m_rhi->isTextureFormatSupported(QRhiTexture::RGBA16F, QRhiTexture::RenderTarget));
    }

    void cleanupTestCase() { m_rhi.reset(); }

    void linuxOutputRemainsSdr()
    {
#if defined(Q_OS_LINUX)
        if (QGuiApplication::platformName() != QStringLiteral("xcb"))
            QSKIP("This fallback test requires an X11 surface without Wayland HDR output metadata");
        QQuickWindow window;
        window.setVulkanInstance(&m_instance);
        window.resize(96, 80);
        HdrOutput output;
        output.attach(&window);
        window.show();
        QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto verifySdr = [&] {
            QVERIFY(!window.grabWindow().isNull());
            QCoreApplication::processEvents();
            QVERIFY(!output.supported());
            QCOMPARE(output.outputMode(), 0);
            QVERIFY(!output.chromeRequired());
            const auto state = HdrOutput::renderState();
            QCOMPARE(state.mode, 0);
            QVERIFY(!state.supported);
        };
        verifySdr();
        QCOMPARE(output.status(), QStringLiteral("HDR unavailable on this display. Enable HDR in your operating system and use a supported GPU and compositor."));
        window.showFullScreen();
        QTRY_COMPARE(window.visibility(), QWindow::FullScreen);
        verifySdr();
        window.showNormal();
        window.resize(128, 96);
        verifySdr();
        QTest::qWait(1600);
        verifySdr();
#else
        QSKIP("Linux-only HDR output policy");
#endif
    }

    void sdrRoundTripsAndWhiteLevel()
    {
        const QVector3D encoded(0.2f, 0.5f, 1.0f);
        QVERIFY((convert(encoded, 0, 0) - encoded).length() < 0.002f);
        const auto linear = convert({1, 1, 1}, 0, 1);
        QVERIFY(std::abs(linear.x() - 203.0f / 80.0f) < 0.005f);
        const auto hdr10 = convert({1, 1, 1}, 0, 2);
        QVERIFY(std::abs(hdr10.x() - pq(203)) < 0.002f);
        const auto gray = convert({0.5f, 0.5f, 0.5f}, 0, 1);
        QVERIFY(std::abs(gray.x() - 0.214041f * 203.0f / 80.0f) < 0.002f);
    }

    void pqPreservesHighlightEnergy_data()
    {
        QTest::addColumn<float>("nits");
        for (float nits : {0.0f, 80.0f, 203.0f, 1000.0f, 4000.0f, 10000.0f})
            QTest::newRow(qPrintable(QString::number(nits))) << nits;
    }

    void displayReferredOutputUsesSdrWhite()
    {
        const auto white = convert({1, 1, 1}, 0, HdrOutput::LinearDisplayReferred);
        QVERIFY((white - QVector3D(1, 1, 1)).length() < 0.002f);
        const auto gray = convert({0.5f, 0.5f, 0.5f}, 0, HdrOutput::LinearDisplayReferred);
        QVERIFY(std::abs(gray.x() - 0.214041f) < 0.002f);
        for (float nits : {0.0f, 80.0f, 203.0f, 1000.0f, 4000.0f, 10000.0f}) {
            const auto encoded = pq(nits);
            const auto result = convert({encoded, encoded, encoded}, 1, HdrOutput::LinearDisplayReferred);
            QVERIFY(std::abs(result.x() - nits / 203.0f)
                < std::max(0.004f, nits / 203.0f * 0.002f));
        }
        const auto hlg = convert({1, 1, 1}, 2, HdrOutput::LinearDisplayReferred);
        QVERIFY(std::abs(hlg.x() - 1000.0f / 203.0f) < 0.01f);
        const auto red = convert({pq(1000), 0, 0}, 1, HdrOutput::LinearDisplayReferred);
        QVERIFY(red.y() < 0.0f);
        QVERIFY(red.z() < 0.0f);
    }

    void displayReferredOutputToneMapsWhenHeadroomIsLost()
    {
        float previous = -1;
        for (float nits : {0.0f, 80.0f, 203.0f, 1000.0f, 4000.0f, 10000.0f}) {
            const auto encoded = pq(nits);
            const auto result = convert({encoded, encoded, encoded}, 1,
                HdrOutput::LinearDisplayReferred, false);
            QVERIFY(result.x() >= previous);
            QVERIFY(result.x() >= 0.0f && result.x() <= 1.0f);
            previous = result.x();
        }
    }

    void pqPreservesHighlightEnergy()
    {
        QFETCH(float, nits);
        const auto value = pq(nits);
        const auto scrgb = convert({value, value, value}, 1, 1);
        QVERIFY(std::abs(scrgb.x() - nits / 80.0f) < std::max(0.004f, nits / 80.0f * 0.002f));
        const auto hdr10 = convert({value, value, value}, 1, 2);
        QVERIFY(std::abs(hdr10.x() - value) < 0.002f);
    }

    void hlgAppliesDisplayOotf()
    {
        const auto white = convert({1, 1, 1}, 2, 1);
        QVERIFY(std::abs(white.x() - 12.5f) < 0.02f);
        const auto gray = convert({0.5f, 0.5f, 0.5f}, 2, 1);
        QVERIFY(std::abs(gray.x() - 1000.0f * std::pow(1.0f / 12.0f, 1.2f) / 80.0f) < 0.003f);
    }

    void rec2020GamutIsConvertedRatherThanClipped()
    {
        const auto red = convert({pq(1000), 0, 0}, 1, 1);
        QVERIFY(std::abs(red.x() - 1.660491f * 12.5f) < 0.04f);
        QVERIFY(red.y() < 0.0f);
        QVERIFY(red.z() < 0.0f);
    }

    void sdrFallbackRetainsHighlightGradation()
    {
        float previous = -1;
        for (const float nits : {0.0f, 80.0f, 203.0f, 1000.0f, 4000.0f, 10000.0f}) {
            const auto value = pq(nits);
            const auto mapped = convert({value, value, value}, 1, 0, false);
            QVERIFY(mapped.x() > previous);
            QVERIFY(mapped.x() <= 1.0f);
            QVERIFY(std::isfinite(mapped.x()));
            previous = mapped.x();
        }
        const auto red = convert({pq(1000), 0, 0}, 1, 0, false);
        QVERIFY(red.x() <= 1.0f && red.y() >= 0.0f && red.z() >= 0.0f);
        const auto lostDisplay = convert({pq(4000), pq(4000), pq(4000)}, 1, 1, false);
        QVERIFY(lostDisplay.x() <= 203.0f / 80.0f);
    }

    void hdr10EncodesAfterLinearChromeBlending()
    {
        const QSize size(8, 8);
        std::unique_ptr<QRhiTexture> texture(m_rhi->newTexture(QRhiTexture::RGBA16F, size, 1,
            QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
        QVERIFY(texture->create());
        std::unique_ptr<QRhiTextureRenderTarget> output(m_rhi->newTextureRenderTarget({texture.get()}));
        std::unique_ptr<QRhiRenderPassDescriptor> outputPass(output->newCompatibleRenderPassDescriptor());
        output->setRenderPassDescriptor(outputPass.get());
        QVERIFY(output->create());
        HdrOutputPass encoder;
        QVERIFY(encoder.initialize(m_rhi.get(), output.get()));
        std::unique_ptr<QRhiBuffer> uniforms(m_rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 32));
        QVERIFY(uniforms->create());
        std::unique_ptr<QRhiShaderResourceBindings> bindings(m_rhi->newShaderResourceBindings());
        bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0, QRhiShaderResourceBinding::FragmentStage, uniforms.get())});
        QVERIFY(bindings->create());
        std::unique_ptr<QRhiGraphicsPipeline> chrome(m_rhi->newGraphicsPipeline());
        chrome->setShaderStages({{QRhiShaderStage::Vertex, shader(":/hdr-test/hdrcolor_test.vert.qsb")},
                                 {QRhiShaderStage::Fragment, shader(":/hdr-test/hdrcolor_test.frag.qsb")}});
        chrome->setShaderResourceBindings(bindings.get());
        chrome->setRenderPassDescriptor(encoder.target()->renderPassDescriptor());
        QRhiGraphicsPipeline::TargetBlend blend;
        blend.enable = true;
        chrome->setTargetBlends({blend});
        QVERIFY(chrome->create());
        QRhiCommandBuffer *cb = nullptr;
        QCOMPARE(m_rhi->beginOffscreenFrame(&cb), QRhi::FrameOpSuccess);
        const float values[] = {1, 1, 1, 0.5f, 0, 1, 203, 1};
        auto *updates = m_rhi->nextResourceUpdateBatch();
        updates->updateDynamicBuffer(uniforms.get(), 0, sizeof(values), values);
        cb->beginPass(encoder.target(), QColor::fromRgbF(12.5f, 12.5f, 12.5f), {1.0f, 0}, updates);
        cb->setGraphicsPipeline(chrome.get());
        cb->setViewport({0, 0, 8, 8});
        cb->setShaderResources();
        cb->draw(3);
        cb->endPass();
        encoder.record(cb, output.get());
        QRhiReadbackResult result;
        bool complete = false;
        result.completed = [&] { complete = true; };
        auto *readback = m_rhi->nextResourceUpdateBatch();
        readback->readBackTexture(QRhiReadbackDescription(texture.get()), &result);
        cb->resourceUpdate(readback);
        QCOMPARE(m_rhi->endOffscreenFrame(), QRhi::FrameOpSuccess);
        QVERIFY(complete);
        const auto *pixel = reinterpret_cast<const qfloat16 *>(result.data.constData());
        const float expected = pq((1000.0f + 203.0f) * 0.5f);
        const float incorrectPqBlend = (pq(1000.0f) + pq(203.0f)) * 0.5f;
        QVERIFY(std::abs(float(pixel[0]) - expected) < 0.002f);
        QVERIFY(std::abs(float(pixel[0]) - incorrectPqBlend) > 0.015f);
        QVERIFY(encoder.initialize(m_rhi.get(), output.get()));
    }

    void chromeLayerPreservesColorAlphaAndSurfaceChanges()
    {
        qmlRegisterType<HdrChromeEffect>("HdrTest", 1, 0, "HdrChromeEffect");
        QQuickWindow window;
#if defined(Q_OS_WIN)
        QQuickGraphicsConfiguration configuration;
        configuration.setPreferSoftwareDevice(true);
        window.setGraphicsConfiguration(configuration);
#endif
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        if (window.rendererInterface()->graphicsApi() == QSGRendererInterface::Vulkan)
            window.setVulkanInstance(&m_instance);
#endif
        window.resize(64, 64);
        QQmlEngine engine;
        QQmlComponent component(&engine);
        component.setData(R"(
            import QtQuick
            import HdrTest 1.0
            Rectangle {
                width: 64; height: 64; color: "blue"
                Rectangle {
                    objectName: "chrome"; width: 32; height: 64; color: "red"
                    layer.enabled: true
                    layer.effect: HdrChromeEffect {}
                }
            }
        )", QUrl());
        std::unique_ptr<QQuickItem> root(qobject_cast<QQuickItem *>(component.create()));
        QVERIFY2(root, qPrintable(component.errorString()));
        root->setParentItem(window.contentItem());
        window.show();
        QVERIFY(QTest::qWaitForWindowExposed(&window));
        QImage image;
        QTRY_VERIFY(!(image = window.grabWindow()).isNull());
        QCOMPARE(image.pixelColor(16, 32), QColor(Qt::red));
        QCOMPARE(image.pixelColor(48, 32), QColor(Qt::blue));
        auto *chrome = root->findChild<QQuickItem *>(QStringLiteral("chrome"));
        QVERIFY(chrome);
        chrome->setProperty("color", QColor::fromRgbF(1, 0, 0, 0.5));
        QTRY_VERIFY((image = window.grabWindow()).pixelColor(16, 32).blue() > 120);
        QVERIFY(std::abs(image.pixelColor(16, 32).red() - 128) <= 2);
        QVERIFY(std::abs(image.pixelColor(16, 32).blue() - 128) <= 2);
        window.showFullScreen();
        QTRY_COMPARE(window.visibility(), QWindow::FullScreen);
        QVERIFY(!window.grabWindow().isNull());
        window.showNormal();
        window.resize(96, 80);
        QVERIFY(!window.grabWindow().isNull());
    }
};

QTEST_MAIN(HdrColorTest)
#include "tst_hdrcolor.moc"
