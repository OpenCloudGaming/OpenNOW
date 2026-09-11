#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class ThemeTestSetup final : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        const auto source = QStringLiteral(OPENNOW_QML_SOURCE_DIR);
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/theme/Theme.qml"), "OpenNOW.ThemeTests", 1, 0, "Theme");
        qmlRegisterType(QUrl::fromLocalFile(source + "/state/settings/SettingsState.qml"), "OpenNOW.ThemeTests", 1, 0, "SettingsState");
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        m_shell.insert("settings", QVariantMap{});
        m_shell.insert("previewThemePack", QString{});
        m_controller.insert("reducedMotion", true);
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("AppController", &m_controller);
    }

private:
    QQmlPropertyMap m_shell;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(theme, ThemeTestSetup)
#include "tst_theme.moc"
