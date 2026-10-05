#include <QFontDatabase>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class ConsoleControlsTestSetup final : public QObject
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
        for (const auto *name : {"ConsoleActionButton", "ConsoleSheetFrame", "ConsoleChoiceSheet", "ConsoleWarningSheet",
                                "MotionProgress", "ControllerGlyph", "KeyboardGlyph"}) {
            qmlRegisterType(QUrl::fromLocalFile(source + "/components/" + name + ".qml"), "OpenNOW", 1, 0, name);
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
        m_app.insert("reducedMotion", true);
        m_app.insert("inputMode", QStringLiteral("keyboard"));
        m_input.insert("controllers", QVariantList{});
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("AppController", &m_app);
        engine->rootContext()->setContextProperty("ControllerInput", &m_input);
        engine->rootContext()->setContextProperty("I18n", this);
    }

private:
    QQmlPropertyMap m_shell;
    QQmlPropertyMap m_app;
    QQmlPropertyMap m_input;
};

QUICK_TEST_MAIN_WITH_SETUP(consolecontrols, ConsoleControlsTestSetup)
#include "tst_consolecontrols.moc"
