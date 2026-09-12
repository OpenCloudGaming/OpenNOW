#include "app/AppController.h"
#include "app/platform/GraphicsDeviceSelection.h"
#include "acceptance/AcceptanceSession.h"
#include "app/ApplicationStartup.h"
#include "app/platform/MacAwdlController.h"
#include "input/ControllerInput.h"
#include "core/CoreClient.h"
#include "input/InputModeTracker.h"
#include "localization/Localization.h"
#include "streaming/rendering/LinuxVulkanGraphics.h"
#include "streaming/rendering/HdrOutput.h"
#include "streaming/rendering/HdrChromeEffect.h"
#ifdef OPENNOW_EMBEDDED_STREAMER
#include "streaming/NativeStreamRuntime.h"
#endif
#include "app/SingleInstance.h"
#include "streaming/StreamVideoItem.h"
#include "media/ThumbnailGenerator.h"

#include <QGuiApplication>
#include <QElapsedTimer>
#include <QFont>
#include <QFontDatabase>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QDir>
#include <QFileInfo>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QFileOpenEvent>
#include <QProcess>

#include <cstdlib>

using namespace Qt::StringLiterals;

namespace {
class FileOpenFilter final : public QObject
{
public:
    explicit FileOpenFilter(AppController *controller)
        : m_controller(controller)
    {
    }

protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if (event->type() != QEvent::FileOpen) {
            return QObject::eventFilter(watched, event);
        }
        const auto *openEvent = static_cast<QFileOpenEvent *>(event);
        if (!openEvent->url().isValid()) return false;
        const auto handled = m_controller->handleArguments(
            {QCoreApplication::applicationFilePath(), openEvent->url().toString()});
        if (handled) m_controller->activateWindow();
        return handled;
    }

private:
    AppController *m_controller;
};
}

static int runApplicationSession(int argc, char *argv[], QString &restartExecutable)
{
    qputenv("QT_TLS_BACKEND", "schannel");
    QElapsedTimer startupTimer;
    startupTimer.start();
    QGuiApplication::setApplicationName(u"OpenNOW"_s);
    QGuiApplication::setOrganizationName(u"OpenCloudGaming"_s);
    QGuiApplication::setOrganizationDomain(u"opennow.app"_s);
    QGuiApplication::setApplicationVersion(QString::fromLatin1(OPENNOW_VERSION));
    QQuickWindow::setDefaultAlphaBuffer(true);
    QQuickWindow::setTextRenderType(QQuickWindow::QtTextRendering);
#if defined(Q_OS_WIN)
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Direct3D11);
#elif defined(Q_OS_MACOS)
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Metal);
#elif defined(Q_OS_LINUX)
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Vulkan);
#endif
    QQuickStyle::setStyle(u"Basic"_s);

    QGuiApplication application(argc, argv);
#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    LinuxVulkanGraphics::requestDeviceExtensions();
#endif
    registerStreamVideoItemQmlType();
    qSetMessagePattern(u"%{time yyyy-MM-ddTHH:mm:ss.zzz} %{type} %{category}: %{message}"_s);
    const QStringList bundledFonts = {
        u":/qt/qml/OpenNOW/res/fonts/Nunito-Variable.ttf"_s,
        u":/qt/qml/OpenNOW/res/fonts/IBMPlexMono-Regular.ttf"_s,
        u":/qt/qml/OpenNOW/res/fonts/IBMPlexMono-Medium.ttf"_s,
        u":/qt/qml/OpenNOW/res/fonts/IBMPlexMono-Bold.ttf"_s,
    };
    for (const auto &fontPath : bundledFonts) {
        if (QFontDatabase::addApplicationFont(fontPath) == -1)
            qWarning("Could not load bundled font %s", qUtf8Printable(fontPath));
    }
    QFont applicationFont(QStringLiteral("Nunito"));
    applicationFont.setHintingPreference(QFont::PreferNoHinting);
    applicationFont.setStyleStrategy(QFont::PreferAntialias);
    application.setFont(applicationFont);
    const auto arguments = application.arguments();
    SingleInstance singleInstance;
    if (!arguments.contains(u"--allow-multiple-instances"_s)
            && !singleInstance.acquire(arguments)) {
        return EXIT_SUCCESS;
    }
    AppController controller;
    QObject::connect(&controller, &AppController::restartRequested, &application, [&] {
        restartExecutable = QCoreApplication::applicationFilePath();
#ifdef Q_OS_LINUX
        const auto appImage = qEnvironmentVariable("APPIMAGE");
        if (!appImage.isEmpty())
            restartExecutable = appImage;
#endif
        application.quit();
    });
    if (!controller.ensureDirectLaunchAssociation())
        qWarning("Could not register the opennow:// direct-launch association");
    FileOpenFilter fileOpenFilter(&controller);
    application.installEventFilter(&fileOpenFilter);
    ControllerInput controllerInput;
    ThumbnailGenerator thumbnailGenerator;
    Localization localization;
    application.installTranslator(&localization);
    CoreClient coreClient;
    const auto coreProgram = AcceptanceSession::coreProgram(arguments);
    GraphicsDeviceSelection graphicsDevices(GraphicsDeviceSelection::detectAdapters(),
#ifdef Q_OS_WIN
                                             CoreClient::graphicsPreference(coreProgram)
#else
                                             QString{}
#endif
    );
    QObject::connect(&localization, &Localization::localeChanged,
                     &graphicsDevices, &GraphicsDeviceSelection::choicesChanged);
#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    NativeStreamRuntime::initializeDiagnostics();
    LinuxVulkanGraphics::Device vulkanDevice;
    if (QQuickWindow::graphicsApi() == QSGRendererInterface::Vulkan
            && !vulkanDevice.initialize())
        qWarning("Embedded Vulkan Video is unavailable: %s Qt will use its default graphics device.",
                 qUtf8Printable(vulkanDevice.lastError()));
#endif
#ifdef OPENNOW_EMBEDDED_STREAMER
    NativeStreamRuntime nativeStreamRuntime(nullptr,
#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
                                           vulkanDevice.handle(),
#else
                                           nullptr,
#endif
                                           graphicsDevices.adapterLuid()
    );
    if (!nativeStreamRuntime.start())
        qWarning("Could not start the embedded streamer runtime: %s",
                 qUtf8Printable(nativeStreamRuntime.lastError()));
    StreamVideoItem::setNativeStreamRuntime(&nativeStreamRuntime);
    QObject::connect(&nativeStreamRuntime, &NativeStreamRuntime::controllerRumbleRequested,
                     &controllerInput, &ControllerInput::playRumble);
    QObject::connect(&nativeStreamRuntime, &NativeStreamRuntime::controllerRumbleStopped,
                     &controllerInput, &ControllerInput::stopRumble);
    QObject::connect(
        &controllerInput, &ControllerInput::gamepadSnapshot, &nativeStreamRuntime,
        [&nativeStreamRuntime](quint8 controllerId, quint16 bitmap, quint16 buttons,
                               quint8 leftTrigger, quint8 rightTrigger,
                               qint16 leftStickX, qint16 leftStickY,
                               qint16 rightStickX, qint16 rightStickY) {
            nativeStreamRuntime.submitGamepad(
                controllerId, bitmap, buttons, leftTrigger, rightTrigger,
                leftStickX, leftStickY, rightStickX, rightStickY);
        });
    QObject::connect(&controllerInput, &ControllerInput::localActionRequested,
                     &nativeStreamRuntime, [&nativeStreamRuntime](quint32 action) {
                         nativeStreamRuntime.submitLocalAction(action);
                     });
#endif
    InputModeTracker inputModeTracker(&controller);
    application.installEventFilter(&inputModeTracker);
    controller.setControllerCount(controllerInput.controllerCount());
    QObject::connect(&controllerInput, &ControllerInput::controllerCountChanged,
                     &controller, &AppController::setControllerCount);
    QObject::connect(&controllerInput, &ControllerInput::controllerActivity,
                     &controller, [&controller] { controller.setInputMode(u"controller"_s); });

    const auto routeIndex = arguments.indexOf(u"--route"_s);
    if (routeIndex >= 0 && routeIndex + 1 < arguments.size()) {
        controller.navigate(arguments.at(routeIndex + 1));
    }
    const auto overlayIndex = arguments.indexOf(u"--overlay"_s);
    if (overlayIndex >= 0 && overlayIndex + 1 < arguments.size()) {
        controller.showOverlay(arguments.at(overlayIndex + 1));
    }
    if (arguments.contains(u"--reduced-motion"_s)) {
        controller.setReducedMotion(true);
    }

    HdrOutput hdrOutput;
    QObject::connect(&localization, &Localization::localeChanged, &hdrOutput, &HdrOutput::changed);
    QObject::connect(&hdrOutput, &HdrOutput::changed, &coreClient, [&] {
        coreClient.setNativeHdrSupported(hdrOutput.supported());
    });
    qmlRegisterType<HdrChromeEffect>("OpenNOW", 1, 0, "HdrChromeEffect");
    qmlRegisterUncreatableType<MacAwdlController>("OpenNOW", 1, 0, "MacAwdlController",
                                                u"Use the application-owned MacAwdl instance"_s);
    MacAwdlController macAwdl;
    QQmlApplicationEngine engine;
    engine.setInitialProperties({{u"visible"_s, false}, {u"visibility"_s, QWindow::Hidden}});
    AcceptanceSession acceptance(application, engine, controller, coreClient, arguments);
    engine.rootContext()->setContextProperty(u"AppController"_s, &controller);
    engine.rootContext()->setContextProperty(u"ControllerInput"_s, &controllerInput);
    engine.rootContext()->setContextProperty(u"ThumbnailGenerator"_s, &thumbnailGenerator);
    engine.rootContext()->setContextProperty(u"I18n"_s, &localization);
    engine.rootContext()->setContextProperty(u"CoreClient"_s, &coreClient);
    engine.rootContext()->setContextProperty(u"GraphicsDevices"_s, &graphicsDevices);
    engine.rootContext()->setContextProperty(u"HdrOutput"_s, &hdrOutput);
    engine.rootContext()->setContextProperty(u"MacAwdl"_s, &macAwdl);
#ifdef OPENNOW_EMBEDDED_STREAMER
    engine.rootContext()->setContextProperty(u"NativeStreamRuntime"_s,
                                             &nativeStreamRuntime);
#endif
    engine.rootContext()->setContextProperty(
        u"LaunchModeOverride"_s,
        arguments.contains(u"--desktop"_s) ? u"desktop"_s
            : arguments.contains(u"--console"_s) ? u"console"_s : QString{});
    acceptance.configureContext();
    QObject::connect(&localization, &Localization::localeChanged,
                     &engine, &QQmlApplicationEngine::retranslate);
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed,
                     &application, [] { QCoreApplication::exit(EXIT_FAILURE); },
                     Qt::QueuedConnection);
    engine.loadFromModule(u"OpenNOW"_s, u"Main"_s);
    auto *rootWindow = engine.rootObjects().isEmpty() ? nullptr
        : qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!graphicsDevices.applyTo(rootWindow)) {
        qCritical("Could not select the graphics adapter before scene-graph initialization");
        return EXIT_FAILURE;
    }
    hdrOutput.attach(rootWindow);
#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    if (vulkanDevice.handle() && !vulkanDevice.adopt(rootWindow)) {
        qWarning("Could not adopt the embedded Vulkan device: %s",
                 qUtf8Printable(vulkanDevice.lastError()));
        return EXIT_FAILURE;
    }
    if (!vulkanDevice.handle() && rootWindow
            && rootWindow->rendererInterface()->graphicsApi() == QSGRendererInterface::Vulkan
            && !vulkanDevice.adoptFallback(rootWindow)) {
        qWarning("Could not adopt the fallback Vulkan instance: %s", qUtf8Printable(vulkanDevice.lastError()));
        return EXIT_FAILURE;
    }
#endif
    if (acceptance.prepareWindow() != EXIT_SUCCESS) return EXIT_FAILURE;
    if (rootWindow && !rootWindow->isVisible()) rootWindow->show();
    const auto qmlReadyMs = startupTimer.elapsed();
    controller.handleArguments(arguments);
    QObject::connect(&controller, &AppController::activationRequested,
                     &application, [&engine] {
                         if (engine.rootObjects().isEmpty()) return;
                         auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
                         if (!window) return;
                         window->show();
                         window->raise();
                         window->requestActivate();
                     });
    QObject::connect(&singleInstance, &SingleInstance::activationRequested,
                     &application, [&controller](const QStringList &forwardedArguments) {
                         controller.activationRequested();
                         controller.handleArguments(forwardedArguments);
                     });

    if (acceptance.measureStartup(startupTimer, qmlReadyMs) != EXIT_SUCCESS)
        return EXIT_FAILURE;

    if (!coreProgram.isEmpty()) coreClient.start(coreProgram);

    if (acceptance.startWorkload() != EXIT_SUCCESS) return EXIT_FAILURE;

    const auto exitCode = application.exec();
#ifdef OPENNOW_EMBEDDED_STREAMER
    const auto roots = engine.rootObjects();
    for (auto *root : roots) delete root;
    StreamVideoItem::setNativeStreamRuntime(nullptr);
    if (!nativeStreamRuntime.shutdown()) {
        qWarning("Could not complete embedded streamer shutdown: %s",
                 qUtf8Printable(nativeStreamRuntime.lastError()));
    }
#endif
    return exitCode;
}

int runApplication(int argc, char *argv[])
{
    QString restartExecutable;
    const auto exitCode = runApplicationSession(argc, argv, restartExecutable);
    if (restartExecutable.isEmpty())
        return exitCode;
    if (!QProcess::startDetached(restartExecutable, {}, QFileInfo(restartExecutable).absolutePath())) {
        qCritical("Could not restart OpenNOW. Reopen the application to continue setup.");
        return EXIT_FAILURE;
    }
    return exitCode;
}
