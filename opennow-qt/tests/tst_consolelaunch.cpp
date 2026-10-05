#include <QQmlEngine>
#include <QQmlContext>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class ConsoleLaunchTestSetup final : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        qmlRegisterType(QUrl::fromLocalFile(QStringLiteral(OPENNOW_QML_SOURCE_DIR)
            + "/components/ConsoleLaunchAnimation.qml"), "OpenNOW", 1, 0, "ConsoleLaunchAnimation");
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto paths = engine->importPathList();
        paths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(paths);
        m_theme.insert("bodyFont", QStringLiteral("Sans Serif"));
        engine->rootContext()->setContextProperty("Theme", &m_theme);
    }

private:
    QQmlPropertyMap m_theme;
};

QUICK_TEST_MAIN_WITH_SETUP(consolelaunch, ConsoleLaunchTestSetup)
#include "tst_consolelaunch.moc"
