#include "streaming/rendering/LinuxVulkanGraphics.h"

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>

#include <atomic>
#include <limits>

using LinuxVulkanGraphics::Device;

namespace {
class WindowExposureObserver final : public QObject
{
public:
    bool exposed = false;

protected:
    bool eventFilter(QObject *object, QEvent *event) override
    {
        if (qobject_cast<QQuickWindow *>(object) && event->type() == QEvent::Show)
            exposed = true;
        return false;
    }
};

OpenNowStreamerVulkanDeviceInfo deviceInfo()
{
    OpenNowStreamerVulkanDeviceInfo info{};
    info.version = OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION;
    info.struct_size = sizeof(info);
    info.instance = reinterpret_cast<void *>(1);
    info.physical_device = reinterpret_cast<void *>(2);
    info.device = reinterpret_cast<void *>(3);
    info.graphics_queue = reinterpret_cast<void *>(4);
    info.graphics_queue_family_index = 2;
    info.graphics_queue_index = 0;
    info.api_version = VK_API_VERSION_1_3;
    return info;
}

OpenNowStreamerGraphicsContext graphicsContext(const OpenNowStreamerVulkanDeviceInfo &info)
{
    OpenNowStreamerGraphicsContext context{};
    context.version = OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION;
    context.struct_size = sizeof(context);
    context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_VULKAN;
    context.instance = info.instance;
    context.physical_device = info.physical_device;
    context.device = info.device;
    context.queue = info.graphics_queue;
    context.queue_family_index = info.graphics_queue_family_index;
    return context;
}
}

class LinuxVulkanGraphicsTest final : public QObject
{
    Q_OBJECT

private slots:
    void initialPropertiesKeepVisibleQmlRootHidden()
    {
        WindowExposureObserver observer;
        qGuiApp->installEventFilter(&observer);
        QQmlApplicationEngine engine;
        engine.setInitialProperties({{QStringLiteral("visible"), false},
                                     {QStringLiteral("visibility"), QWindow::Hidden}});
        engine.loadData("import QtQuick\nWindow { visible: true; visibility: Window.Windowed; width: 320; height: 240 }");
        QCOMPARE(engine.rootObjects().size(), 1);
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        QVERIFY(window);
        QCoreApplication::processEvents();
        QVERIFY(!window->isVisible());
        QVERIFY(!observer.exposed);
        if (window->rendererInterface()->graphicsApi() == QSGRendererInterface::Software) {
            QQuickWindow neverExposed;
            QVERIFY(!neverExposed.isVisible());
            QCOMPARE(window->isSceneGraphInitialized(), neverExposed.isSceneGraphInitialized());
        } else {
            QVERIFY(!window->isSceneGraphInitialized());
        }
        QCOMPARE(window->visibility(), QWindow::Hidden);
        window->create();
        QCoreApplication::processEvents();
        QVERIFY(!window->isVisible());
        QVERIFY(!observer.exposed);
        if (window->rendererInterface()->graphicsApi() != QSGRendererInterface::Software)
            QVERIFY(!window->isSceneGraphInitialized());
        window->show();
        QTRY_VERIFY(window->isVisible());
    }

    void validatesDeviceContract()
    {
        const auto valid = deviceInfo();
        QVERIFY(Device::validInfo(valid));
        auto info = valid;
        info.version++;
        QVERIFY(!Device::validInfo(info));
        info = valid;
        info.struct_size--;
        QVERIFY(!Device::validInfo(info));
        info = valid;
        info.api_version = VK_API_VERSION_1_0;
        QVERIFY(!Device::validInfo(info));
        info = valid;
        info.graphics_queue_family_index = VK_QUEUE_FAMILY_IGNORED;
        QVERIFY(!Device::validInfo(info));
        info = valid;
        info.graphics_queue_index = std::numeric_limits<uint32_t>::max();
        QVERIFY(!Device::validInfo(info));
        for (auto member : {&OpenNowStreamerVulkanDeviceInfo::instance,
                            &OpenNowStreamerVulkanDeviceInfo::physical_device,
                            &OpenNowStreamerVulkanDeviceInfo::device,
                            &OpenNowStreamerVulkanDeviceInfo::graphics_queue}) {
            info = valid;
            info.*member = nullptr;
            QVERIFY(!Device::validInfo(info));
        }
    }

    void fallbackInstanceEnablesAdvertisedHdrColorspace()
    {
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("The offscreen platform has no Vulkan surface");
        Device device;
        QQuickWindow window;
        QVERIFY(device.adoptFallback(&window));
        QVERIFY(!device.handle());
        auto *instance = window.vulkanInstance();
        QVERIFY(instance);
        QVERIFY(instance->isValid());
        QVERIFY(instance->apiVersion() >= QVersionNumber(1, 1));
        const QByteArray extension("VK_EXT_swapchain_colorspace");
        QCOMPARE(instance->extensions().contains(extension), instance->supportedExtensions().contains(extension));
        QVERIFY(!window.isVisible());
    }

    void rejectsDifferentQtGraphicsDevice()
    {
        const auto info = deviceInfo();
        const auto valid = graphicsContext(info);
        QVERIFY(Device::matchesContext(info, valid));
        for (auto member : {&OpenNowStreamerGraphicsContext::instance,
                            &OpenNowStreamerGraphicsContext::physical_device,
                            &OpenNowStreamerGraphicsContext::device,
                            &OpenNowStreamerGraphicsContext::queue}) {
            auto context = valid;
            context.*member = reinterpret_cast<void *>(9);
            QVERIFY(!Device::matchesContext(info, context));
        }
        auto context = valid;
        context.queue_family_index++;
        QVERIFY(!Device::matchesContext(info, context));
        context = valid;
        context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_D3D11;
        QVERIFY(!Device::matchesContext(info, context));
    }

    void failedBootstrapLeavesNoNativeOwner()
    {
        static int destroys = 0;
        destroys = 0;
        Device::Api api;
        api.create = [](OpenNowStreamerVulkanDevice **output) {
            *output = nullptr;
            return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        };
        api.destroy = [](OpenNowStreamerVulkanDevice *) {
            ++destroys;
            return OPENNOW_STREAMER_OK;
        };
        {
            Device device(api);
            QVERIFY(!device.initialize());
            QVERIFY(!device.handle());
            QVERIFY(!device.lastError().isEmpty());
            QQuickWindow shell;
            QVERIFY(!device.adopt(&shell));
            QVERIFY(!shell.vulkanInstance());
        }
        QCOMPARE(destroys, 0);
        api.create = [](OpenNowStreamerVulkanDevice **output) {
            *output = reinterpret_cast<OpenNowStreamerVulkanDevice *>(1);
            return OPENNOW_STREAMER_OK;
        };
        api.info = [](const OpenNowStreamerVulkanDevice *, OpenNowStreamerVulkanDeviceInfo *info) {
            *info = deviceInfo();
            info->version++;
            return OPENNOW_STREAMER_OK;
        };
        {
            Device device(api);
            QVERIFY(!device.initialize());
            QVERIFY(!device.handle());
            QCOMPARE(destroys, 1);
        }
        QCOMPARE(destroys, 1);
    }

    void adoptsBeforeExposureAndSurvivesSceneGraphRecreation()
    {
        const bool requireVulkanVideo = qEnvironmentVariableIntValue("OPENNOW_TEST_VULKAN_VIDEO") == 1;
        if (QGuiApplication::platformName() == QStringLiteral("offscreen")
                || QGuiApplication::platformName() == QStringLiteral("minimal")) {
            QVERIFY2(!requireVulkanVideo,
                     "OPENNOW_TEST_VULKAN_VIDEO=1 requires a Vulkan-capable window system.");
            QSKIP("A Vulkan-capable window system is required for real-device adoption.");
        }
        Device device;
        if (!device.initialize()) {
            QVERIFY2(!requireVulkanVideo, qPrintable(device.lastError()));
            QSKIP(qPrintable(device.lastError()));
        }
        const auto *owner = device.handle();
        OpenNowStreamerVulkanDeviceInfo info{};
        info.version = OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION;
        info.struct_size = sizeof(info);
        QCOMPARE(opennow_streamer_vulkan_device_info(owner, &info), OPENNOW_STREAMER_OK);

        std::atomic_bool matches{false};
        std::atomic_int initialized{0};
        QQmlApplicationEngine engine;
        engine.setInitialProperties({{QStringLiteral("visible"), false},
                                     {QStringLiteral("visibility"), QWindow::Hidden}});
        engine.loadData("import QtQuick\nWindow { visible: true; visibility: Window.Windowed; width: 320; height: 240; Rectangle { anchors.fill: parent; color: 'blue' } }");
        QCOMPARE(engine.rootObjects().size(), 1);
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        QVERIFY(window);
        QVERIFY(!window->isVisible());
        QVERIFY(!window->isSceneGraphInitialized());
        QVERIFY2(device.adopt(window), qPrintable(device.lastError()));
        QVERIFY(!window->isVisible());
        QVERIFY(!window->isSceneGraphInitialized());
        QCOMPARE(reinterpret_cast<void *>(window->vulkanInstance()->vkInstance()), info.instance);
        window->setPersistentGraphics(false);
        window->setPersistentSceneGraph(false);
        connect(window, &QQuickWindow::sceneGraphInitialized, window, [&] {
            auto *rhi = window->rhi();
            if (rhi && rhi->backend() == QRhi::Vulkan) {
                const auto *handles = static_cast<const QRhiVulkanNativeHandles *>(rhi->nativeHandles());
                auto context = graphicsContext(info);
                context.instance = reinterpret_cast<void *>(handles->inst->vkInstance());
                context.physical_device = reinterpret_cast<void *>(handles->physDev);
                context.device = reinterpret_cast<void *>(handles->dev);
                context.queue = reinterpret_cast<void *>(handles->gfxQueue);
                context.queue_family_index = handles->gfxQueueFamilyIdx;
                matches.store(Device::matchesContext(info, context));
            }
            initialized.fetch_add(1);
        }, Qt::DirectConnection);
        QSignalSpy invalidated(window, &QQuickWindow::sceneGraphInvalidated);
        window->show();
        QVERIFY(QTest::qWaitForWindowExposed(window));
        QTRY_VERIFY(initialized.load() > 0);
        QVERIFY(matches.load());
        QVERIFY(!device.adopt(window));
        window->hide();
        window->releaseResources();
        QTRY_VERIFY(!invalidated.isEmpty());
        const int previous = initialized.load();
        window->show();
        QVERIFY(QTest::qWaitForWindowExposed(window));
        QTRY_VERIFY(initialized.load() > previous);
        QVERIFY(matches.load());
        QCOMPARE(device.handle(), owner);
        window->showFullScreen();
        QTRY_COMPARE(window->visibility(), QWindow::FullScreen);
        window->showNormal();
        window->resize(400, 300);
        QVERIFY(!window->grabWindow().isNull());
    }
};

int main(int argc, char **argv)
{
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Vulkan);
    QGuiApplication application(argc, argv);
    LinuxVulkanGraphicsTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_linuxvulkangraphics.moc"
