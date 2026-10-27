#include "app/platform/WindowTheme.h"

#include <QGuiApplication>
#include <QPlatformSurfaceEvent>
#include <QProcess>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QStyleHints>
#include <QTemporaryDir>
#include <QScopeGuard>
#include <QtTest>

#ifdef Q_OS_WIN
#include <windows.h>
#include <dwmapi.h>
#endif

class WindowThemeTest : public QObject
{
    Q_OBJECT

private slots:
    void preparationPreservesNativeWindows()
    {
        QVERIFY(!WindowTheme::prepareWindow(nullptr));
        for (const auto surface : {QSurface::RasterSurface, QSurface::OpenGLSurface, QSurface::VulkanSurface}) {
            QWindow window;
            window.setSurfaceType(surface);
            window.setFlags(Qt::Window | Qt::WindowTitleHint | Qt::WindowCloseButtonHint
                            | Qt::WindowStaysOnTopHint);
            const auto flags = window.flags();
            QVERIFY(WindowTheme::prepareWindow(&window));
            QVERIFY(!window.handle());
            QVERIFY(!window.isVisible());
            const bool custom = QGuiApplication::platformName().startsWith("wayland")
                && surface == QSurface::VulkanSurface;
            QCOMPARE(window.flags(), custom ? flags | Qt::FramelessWindowHint : flags);
            QVERIFY(WindowTheme::prepareWindow(&window));
            QCOMPARE(window.flags(), custom ? flags | Qt::FramelessWindowHint : flags);
        }
    }

    void waylandVulkanLifecycle()
    {
        if (!QGuiApplication::platformName().startsWith("wayland"))
            QSKIP("Requires a Wayland compositor");
        QWindow window;
        window.setSurfaceType(QSurface::VulkanSurface);
        window.setFlags(Qt::Window | Qt::WindowTitleHint | Qt::WindowCloseButtonHint);
        WindowTheme theme;
        theme.setTargetWindow(&window);
        QSignalSpy ownership(&theme, &WindowTheme::clientDecorationsChanged);
        QVERIFY(WindowTheme::prepareWindow(&window));
        QVERIFY(!window.handle());
        QVERIFY(window.flags().testFlag(Qt::FramelessWindowHint));
        const auto flags = window.flags();
        window.show();
        QTRY_VERIFY(window.isExposed());
        QVERIFY(theme.clientDecorations());
        QCOMPARE(ownership.size(), 1);
        const auto id = window.winId();
        QVERIFY(WindowTheme::prepareWindow(&window));
        window.showFullScreen();
        QTRY_COMPARE(window.visibility(), QWindow::FullScreen);
        QTRY_COMPARE(window.size(), QSize(1280, 720));
        QVERIFY(theme.clientDecorations());
        QCOMPARE(window.winId(), id);
        QCOMPARE(window.flags(), flags);
        window.showNormal();
        QTRY_COMPARE(window.visibility(), QWindow::Windowed);
        QTRY_COMPARE(window.size(), QSize(640, 480));
        QCOMPARE(window.winId(), id);
        window.destroy();
        QVERIFY(!window.handle());
        QVERIFY(WindowTheme::prepareWindow(&window));
        window.show();
        QTRY_VERIFY(window.isExposed());
        QVERIFY(window.winId() != id);
        QVERIFY(theme.clientDecorations());
        QCOMPARE(window.flags(), flags);
        QCOMPARE(ownership.size(), 1);
        QWindow unprepared;
        unprepared.setSurfaceType(QSurface::VulkanSurface);
        unprepared.create();
        const auto originalFlags = unprepared.flags();
        QVERIFY(!WindowTheme::prepareWindow(&unprepared));
        QCOMPARE(unprepared.flags(), originalFlags);
        QVERIFY(!unprepared.isVisible());
    }

    void waylandProtocol_data()
    {
        QTest::addColumn<QString>("compositorOption");
        QTest::newRow("client-mode") << QString{};
        QTest::newRow("server-mode") << QStringLiteral("--server-decorations");
        QTest::newRow("no-manager") << QStringLiteral("--no-decoration-manager");
    }

    void waylandProtocol()
    {
#ifdef OPENNOW_WINDOWTHEME_COMPOSITOR
        QFETCH(QString, compositorOption);
        QTemporaryDir runtime;
        QVERIFY(runtime.isValid());
        auto environment = QProcessEnvironment::systemEnvironment();
        environment.insert("XDG_RUNTIME_DIR", runtime.path());
        environment.insert("QT_QPA_PLATFORM", "offscreen");
        QProcess compositor;
        compositor.setProcessEnvironment(environment);
        compositor.start(QStringLiteral(OPENNOW_WINDOWTHEME_COMPOSITOR),
                         compositorOption.isEmpty() ? QStringList{} : QStringList{compositorOption});
        const auto cleanup = qScopeGuard([&] {
            compositor.terminate();
            if (!compositor.waitForFinished(2000)) {
                compositor.kill();
                compositor.waitForFinished();
            }
        });
        QVERIFY(compositor.waitForStarted());
        QVERIFY(compositor.waitForReadyRead(5000));
        QVERIFY(compositor.readAllStandardOutput().contains("READY"));
        environment.insert("QT_QPA_PLATFORM", "wayland");
        environment.insert("QT_WAYLAND_SHELL_INTEGRATION", "xdg-shell");
        environment.insert("WAYLAND_DISPLAY", "opennow-windowtheme-test");
        environment.insert("WAYLAND_DEBUG", "client");
        QProcess client;
        client.setProcessEnvironment(environment);
        client.start(QCoreApplication::applicationFilePath(), {"waylandVulkanLifecycle"});
        QVERIFY(client.waitForFinished(10000));
        const auto trace = client.readAllStandardError();
        const auto results = client.readAllStandardOutput();
        QVERIFY2(client.exitStatus() == QProcess::NormalExit && client.exitCode() == 0,
                 qPrintable(QString::fromUtf8(results + trace)));
        QCOMPARE(trace.contains("zxdg_decoration_manager_v1"), compositorOption != "--no-decoration-manager");
        QVERIFY2(!trace.contains("get_toplevel_decoration"), trace.constData());
        QVERIFY2(!trace.contains("zxdg_toplevel_decoration_v1"), trace.constData());
        QVERIFY(trace.contains("get_toplevel"));
        QVERIFY(trace.contains("set_fullscreen"));
        QVERIFY(trace.contains("unset_fullscreen"));
#else
        QSKIP("Qt Wayland Compositor is unavailable");
#endif
    }

    void nativeLifecycle()
    {
        QWindow window;
        QList<bool> requests;
        WindowTheme theme([&](QWindow *target, bool dark) {
            QCOMPARE(target, &window);
            requests.append(dark);
        });
        theme.setDarkMode(true);
        theme.setTargetWindow(&window);
        QVERIFY(!window.handle());
        QVERIFY(requests.isEmpty());
        window.create();
        QCOMPARE(requests, QList<bool>{true});
        const auto handle = window.winId();
        const auto flags = window.flags();
        const auto geometry = window.geometry();
        theme.setDarkMode(false);
        QCOMPARE(requests, (QList<bool>{true, false}));
        theme.setDarkMode(false);
        QCOMPARE(requests.size(), 2);
        QCOMPARE(window.winId(), handle);
        QCOMPARE(window.flags(), flags);
        QCOMPARE(window.geometry(), geometry);
        QVERIFY(!window.isVisible());
        window.destroy();
        QCOMPARE(requests.size(), 2);
        theme.setDarkMode(true);
        QVERIFY(!window.handle());
        QCOMPARE(requests.size(), 2);
        window.create();
        QCOMPARE(requests, (QList<bool>{true, false, true}));
        QEvent paletteChange(QEvent::ApplicationPaletteChange);
        QCoreApplication::sendEvent(&window, &paletteChange);
        QTRY_COMPARE(requests.size(), 4);
        QCOMPARE(requests.last(), true);
        theme.setTargetWindow(nullptr);
        window.destroy();
        window.create();
        QCOMPARE(requests.size(), 4);
    }

    void attachExistingAndReplaceWindow()
    {
        QWindow first;
        QWindow second;
        first.create();
        second.create();
        QList<QWindow *> targets;
        WindowTheme theme([&](QWindow *target, bool) { targets.append(target); });
        theme.setTargetWindow(&first);
        theme.setTargetWindow(&second);
        QCOMPARE(targets, (QList<QWindow *>{&first, &second}));
        first.destroy();
        first.create();
        QCOMPARE(targets.size(), 2);
        second.destroy();
        second.create();
        QCOMPARE(targets.size(), 3);
        QCOMPARE(targets.last(), &second);
        auto *temporary = new QWindow;
        theme.setTargetWindow(temporary);
        delete temporary;
        QVERIFY(!theme.targetWindow());
        theme.setDarkMode(true);
    }

    void resolvedQmlTheme()
    {
        QQmlPropertyMap shell;
        QQmlPropertyMap controller;
        shell.insert("settings", QVariantMap{{"appTheme", "dark"}});
        shell.insert("previewThemePack", QString{});
        controller.insert("reducedMotion", true);
        QWindow window;
        window.create();
        QList<bool> requests;
        WindowTheme theme([&](QWindow *, bool dark) { requests.append(dark); });
        theme.setTargetWindow(&window);
        QQmlEngine engine;
        engine.rootContext()->setContextProperty("ShellStore", &shell);
        engine.rootContext()->setContextProperty("AppController", &controller);
        engine.rootContext()->setContextProperty("TestWindowTheme", &theme);
        QQmlComponent component(&engine);
        component.setData(R"(
            import QtQuick
            import OpenNOW.WindowThemeTests
            Binding { target: TestWindowTheme; property: "darkMode"; value: !Theme.lightMode }
        )", QUrl{});
        QScopedPointer<QObject> binding(component.create());
        QVERIFY2(binding, qPrintable(component.errorString()));
        QCOMPARE(requests.last(), true);
        shell.insert("settings", QVariantMap{{"appTheme", "light"}});
        QCOMPARE(requests.last(), false);
        shell.insert("settings", QVariantMap{{"appTheme", "auto"}});
        QCOMPARE(requests.last(), QGuiApplication::styleHints()->colorScheme() != Qt::ColorScheme::Light);
        shell.insert("previewThemePack", "bone");
        QCOMPARE(requests.last(), false);
        shell.insert("previewThemePack", "aurora");
        QCOMPARE(requests.last(), true);
        shell.insert("previewThemePack", QString{});
        QCOMPARE(requests.last(), QGuiApplication::styleHints()->colorScheme() != Qt::ColorScheme::Light);
    }

    void windowsDwmAttribute()
    {
#ifdef Q_OS_WIN
        if (QGuiApplication::platformName() != "windows")
            QSKIP("DWM requires the Windows QPA plugin and a native desktop");
        QWindow window;
        WindowTheme theme;
        theme.setDarkMode(true);
        theme.setTargetWindow(&window);
        window.create();
        DWORD attribute = 20;
        BOOL actual = FALSE;
        auto result = DwmGetWindowAttribute(reinterpret_cast<HWND>(window.winId()), attribute, &actual, sizeof(actual));
        if (result == E_INVALIDARG) {
            attribute = 19;
            result = DwmGetWindowAttribute(reinterpret_cast<HWND>(window.winId()), attribute, &actual, sizeof(actual));
        }
        if (result == E_INVALIDARG) QSKIP("This OS does not support native dark titlebars");
        QVERIFY(SUCCEEDED(result));
        QCOMPARE(actual, TRUE);
        theme.setDarkMode(false);
        QVERIFY(SUCCEEDED(DwmGetWindowAttribute(reinterpret_cast<HWND>(window.winId()), attribute, &actual, sizeof(actual))));
        QCOMPARE(actual, FALSE);
        window.destroy();
        theme.setDarkMode(true);
        window.create();
        QVERIFY(SUCCEEDED(DwmGetWindowAttribute(reinterpret_cast<HWND>(window.winId()), attribute, &actual, sizeof(actual))));
        QCOMPARE(actual, TRUE);
#else
        QSKIP("DWM is only available on Windows");
#endif
    }
};

int main(int argc, char **argv)
{
    QGuiApplication application(argc, argv);
    qmlRegisterSingletonType(QUrl::fromLocalFile(QStringLiteral(OPENNOW_QML_SOURCE_DIR "/theme/Theme.qml")),
                            "OpenNOW.WindowThemeTests", 1, 0, "Theme");
    WindowThemeTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_windowtheme.moc"
