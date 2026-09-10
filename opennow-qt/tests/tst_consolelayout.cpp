#include <QFontDatabase>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class ConsoleLayoutTestSetup final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int revision READ revision CONSTANT)

public:
    int revision() const { return 0; }
    Q_INVOKABLE QString source(const QString &text, int) const { return text; }

public slots:
    void applicationAvailable()
    {
        const auto source = QStringLiteral(OPENNOW_QML_SOURCE_DIR);
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/theme/Theme.qml"), "OpenNOW", 1, 0, "Theme");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/components/InputPromptIcons.qml"), "OpenNOW", 1, 0, "InputPromptIcons");
        for (const auto *name : {"ShellViewport", "AppChrome", "GlassPanel", "ControllerGlyph", "HintBar", "NavPill"}) {
            qmlRegisterType(QUrl::fromLocalFile(source + "/components/" + name + ".qml"), "OpenNOW", 1, 0, name);
        }
        QFontDatabase::addApplicationFont(QStringLiteral(":/qt/qml/OpenNOW/res/fonts/Nunito-Variable.ttf"));
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto importPaths = engine->importPathList();
        importPaths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(importPaths);
        m_shell.insert("settings", QVariantMap{{"appTheme", "dark"}});
        m_shell.insert("previewThemePack", QString{});
        m_shell.insert("authSession", QVariantMap{});
        m_shell.insert("signedIn", false);
        m_shell.insert("activeSession", QVariantMap{});
        m_shell.insert("regions", QVariantList{});
        m_shell.insert("regionPingResults", QVariantMap{});
        m_controller.insert("controllers", QVariantList{});
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("ControllerInput", &m_controller);
        engine->rootContext()->setContextProperty("I18n", this);
    }

private:
    QQmlPropertyMap m_shell;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(consolelayout, ConsoleLayoutTestSetup)
#include "tst_consolelayout.moc"
