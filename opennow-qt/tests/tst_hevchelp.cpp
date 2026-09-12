#include <QFontDatabase>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class HevcHelpTestSetup final : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        const auto source = QStringLiteral(OPENNOW_QML_SOURCE_DIR);
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/theme/Theme.qml"), "OpenNOW", 1, 0, "Theme");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/desktop/components/DesktopTokens.qml"), "OpenNOW", 1, 0, "DesktopTokens");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/components/InputPromptIcons.qml"), "OpenNOW", 1, 0, "InputPromptIcons");
        qmlRegisterType(QUrl::fromLocalFile(source + "/components/KeyboardGlyph.qml"), "OpenNOW", 1, 0, "KeyboardGlyph");
        for (const auto *name : {"DesktopSettingsHevcHelp", "DesktopSettingsButton", "DesktopSettingsIcon"}) {
            qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/settings/controls/" + name + ".qml"), "OpenNOW", 1, 0, name);
        }
        QFontDatabase::addApplicationFont(source + "/../res/fonts/Nunito-Variable.ttf");
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto paths = engine->importPathList();
        paths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(paths);
        m_shell.insert("settings", QVariantMap{{"appTheme", "dark"}});
        m_shell.insert("previewThemePack", QString{});
        m_controller.insert("reducedMotion", true);
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("AppController", &m_controller);
    }

private:
    QQmlPropertyMap m_shell;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(hevchelp, HevcHelpTestSetup)
#include "tst_hevchelp.moc"
