#include "app/platform/WindowTheme.h"

#include <QGuiApplication>
#include <QPlatformSurfaceEvent>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QStyleHints>
#include <QtTest>

#ifdef Q_OS_WIN
#include <windows.h>
#include <dwmapi.h>
#endif

class WindowThemeTest : public QObject
{
    Q_OBJECT

private slots:
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
        QQmlEngine engine;
        QQmlPropertyMap shell;
        shell.insert("settings", QVariantMap{{"appTheme", "dark"}});
        shell.insert("previewThemePack", QString{});
        engine.rootContext()->setContextProperty("ShellStore", &shell);
        QWindow window;
        window.create();
        QList<bool> requests;
        WindowTheme theme([&](QWindow *, bool dark) { requests.append(dark); });
        theme.setTargetWindow(&window);
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
