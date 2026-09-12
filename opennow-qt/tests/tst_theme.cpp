#include <QFontDatabase>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>
#include <utility>

class ThemeTestShell final : public QQmlPropertyMap
{
    Q_OBJECT

public:
    ThemeTestShell() : QQmlPropertyMap(this, nullptr) {}

    Q_INVOKABLE QString artworkUrl(const QString &source) const { return source; }
    Q_INVOKABLE bool streamOverlayBlocksGameplayInput(const QString &overlay) const { return !overlay.isEmpty(); }

signals:
    void readyChanged();
};

class ThemeTestSetup final : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        const auto source = QStringLiteral(OPENNOW_QML_SOURCE_DIR);
        QFontDatabase::addApplicationFont(source + "/../res/fonts/Nunito-Variable.ttf");
        QFontDatabase::addApplicationFont(source + "/../res/fonts/IBMPlexMono-Medium.ttf");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/theme/Theme.qml"), "OpenNOW.ThemeTests", 1, 0, "Theme");
        qmlRegisterType(QUrl::fromLocalFile(source + "/state/settings/SettingsState.qml"), "OpenNOW.ThemeTests", 1, 0, "SettingsState");
        for (const auto &entry : {std::pair{"Theme", "theme/Theme.qml"},
                                 {"DesktopTokens", "desktop/components/DesktopTokens.qml"},
                                 {"InputPromptIcons", "components/InputPromptIcons.qml"}}) {
            qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/" + entry.second), "OpenNOW", 1, 0, entry.first);
        }
        for (const auto &entry : {std::pair{"DesktopSessionStarting", "desktop/stream/DesktopSessionStarting.qml"},
                                 {"DesktopButton", "desktop/components/DesktopButton.qml"},
                                 {"DesktopGlyph", "desktop/components/DesktopGlyph.qml"},
                                 {"DesktopSettingsIcon", "desktop/settings/controls/DesktopSettingsIcon.qml"},
                                 {"KeyboardGlyph", "components/KeyboardGlyph.qml"},
                                 {"ArtworkSource", "components/ArtworkSource.qml"}}) {
            qmlRegisterType(QUrl::fromLocalFile(source + "/" + entry.second), "OpenNOW", 1, 0, entry.first);
        }
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto paths = engine->importPathList();
        paths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(paths);
        m_shell.insert("settings", QVariantMap{});
        m_shell.insert("previewThemePack", QString{});
        m_shell.insert("selectedGame", QVariantMap{{"title", "Dead by Daylight"}});
        m_shell.insert("activeSession", QVariantMap{{"queuePosition", 21}});
        m_shell.insert("streamer", QVariantMap{});
        m_shell.insert("streamState", "preparing");
        m_shell.insert("streamerRestartAttempts", 0);
        m_shell.insert("sessionReconnectAttempts", 0);
        m_shell.insert("launchConflictDetected", false);
        m_shell.insert("streamMessage", QString{});
        m_shell.insert("streamBusy", false);
        m_shell.insert("pendingLaunchParams", QVariantMap{});
        m_shell.insert("conflictSession", QVariantMap{});
        m_controller.insert("reducedMotion", true);
        m_controller.insert("route", "inserting");
        m_controller.insert("overlay", QString{});
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("AppController", &m_controller);
    }

private:
    ThemeTestShell m_shell;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(theme, ThemeTestSetup)
#include "tst_theme.moc"
