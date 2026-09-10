#include <QFontDatabase>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class StreamToastTestSetup final : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        const auto source = QStringLiteral(OPENNOW_QML_SOURCE_DIR);
        QFontDatabase::addApplicationFont(source + "/../res/fonts/Nunito-Variable.ttf");
        QFontDatabase::addApplicationFont(source + "/../res/fonts/IBMPlexMono-Medium.ttf");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/theme/Theme.qml"), "OpenNOW", 1, 0, "Theme");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/components/InputPromptIcons.qml"), "OpenNOW", 1, 0, "InputPromptIcons");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/stream/DesktopStreamToast.qml"), "OpenNOW", 1, 0, "DesktopStreamToast");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/stream/DesktopStreamToasts.qml"), "OpenNOW", 1, 0, "DesktopStreamToasts");
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto paths = engine->importPathList();
        paths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(paths);
        m_shell.insert("settings", QVariantMap{});
        m_shell.insert("previewThemePack", QString{});
        m_shell.insert("streamer", QVariantMap{{"status", "streaming"}});
        m_shell.insert("activeSession", QVariantMap{{"sessionId", "test-session"}});
        m_input.insert("controllers", QVariantList{});
        m_controller.insert("reducedMotion", true);
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("ControllerInput", &m_input);
        engine->rootContext()->setContextProperty("AppController", &m_controller);
    }

private:
    QQmlPropertyMap m_shell;
    QQmlPropertyMap m_input;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(streamtoasts, StreamToastTestSetup)
#include "tst_streamtoasts.moc"
