#include "app/AppController.h"

#include <QFontDatabase>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>
#include <utility>

class ConsoleActionTestSetup final : public QObject
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
        for (const auto &entry : {std::pair{"Theme", "theme/Theme.qml"},
                 {"InputPromptIcons", "components/InputPromptIcons.qml"},
                 {"DesktopTokens", "desktop/components/DesktopTokens.qml"}}) {
            qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/" + entry.second),
                                     "OpenNOW", 1, 0, entry.first);
        }
        for (const auto *name : {"AppChrome", "GlassPanel", "GlassButton", "ControllerGlyph",
                 "KeyboardGlyph", "HintBar", "NavPill", "ScreenBackground", "ArtworkSource",
                 "RoundedArtwork", "PlatformPicker", "StoreBadge", "FilterDropdown",
                 "PosterTile", "VirtualKeyboard"}) {
            qmlRegisterType(QUrl::fromLocalFile(source + "/components/" + name + ".qml"),
                            "OpenNOW", 1, 0, name);
        }
        for (const auto *name : {"GameDetailScreen", "LibraryScreen"}) {
            qmlRegisterType(QUrl::fromLocalFile(source + "/screens/" + name + ".qml"),
                            "OpenNOW", 1, 0, name);
        }
        QFontDatabase::addApplicationFont(QStringLiteral(":/qt/qml/OpenNOW/res/fonts/Nunito-Variable.ttf"));
        m_app.setReducedMotion(true);
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto paths = engine->importPathList();
        paths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(paths);
        QQmlComponent fixture(engine, QUrl::fromLocalFile(
            QStringLiteral(OPENNOW_CONSOLE_ACTION_TEST_DIR) + "/ConsoleActionStore.qml"));
        auto *store = fixture.create();
        if (!store) qFatal("Console action fixture failed: %s", qPrintable(fixture.errorString()));
        store->setParent(engine);
        m_controller.insert("controllers", QVariantList{});
        engine->rootContext()->setContextProperty("ShellStore", store);
        engine->rootContext()->setContextProperty("AppController", &m_app);
        engine->rootContext()->setContextProperty("ControllerInput", &m_controller);
        engine->rootContext()->setContextProperty("I18n", this);
    }

private:
    AppController m_app;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(consoleactions, ConsoleActionTestSetup)
#include "tst_consoleactions.moc"
