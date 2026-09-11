#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class ControllerIconTestSetup final : public QObject
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
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/desktop/components/DesktopTokens.qml"), "OpenNOW", 1, 0, "DesktopTokens");
        qmlRegisterType(QUrl::fromLocalFile(source + "/components/ControllerGlyph.qml"), "OpenNOW", 1, 0, "ControllerGlyph");
        qmlRegisterType(QUrl::fromLocalFile(source + "/components/KeyboardGlyph.qml"), "OpenNOW", 1, 0, "KeyboardGlyph");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/components/DesktopKeyHint.qml"), "OpenNOW", 1, 0, "DesktopKeyHint");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/components/DesktopButton.qml"), "OpenNOW", 1, 0, "DesktopButton");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/settings/controls/DesktopSettingsButton.qml"), "OpenNOW", 1, 0, "DesktopSettingsButton");
        qmlRegisterType(QUrl::fromLocalFile(source + "/components/GlassButton.qml"), "OpenNOW", 1, 0, "GlassButton");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/components/DesktopGlyph.qml"), "OpenNOW", 1, 0, "DesktopGlyph");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/settings/controls/DesktopSettingsIcon.qml"), "OpenNOW", 1, 0, "DesktopSettingsIcon");
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto importPaths = engine->importPathList();
        importPaths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(importPaths);
        m_shell.insert("settings", QVariantMap{});
        m_shell.insert("previewThemePack", QString{});
        m_controller.insert("reducedMotion", true);
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("AppController", &m_controller);
        engine->rootContext()->setContextProperty("I18n", this);
    }

private:
    QQmlPropertyMap m_shell;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(controllericons, ControllerIconTestSetup)
#include "tst_controllericons.moc"
