#pragma once

#include <QObject>
#include <QStringList>

class AppController;
class CoreClient;
class QElapsedTimer;
class QGuiApplication;
class QQmlApplicationEngine;

class AcceptanceSession final : public QObject
{
public:
    AcceptanceSession(QGuiApplication &application, QQmlApplicationEngine &engine,
                      AppController &controller, CoreClient &coreClient,
                      const QStringList &arguments);

    void configureContext();
    [[nodiscard]] int prepareWindow();
    [[nodiscard]] int measureStartup(const QElapsedTimer &startupTimer, qint64 qmlReadyMs);
    [[nodiscard]] int startWorkload();
    static QString coreProgram(const QStringList &arguments);

private:
    [[nodiscard]] int startSmokeWorkload();
    [[nodiscard]] int startThemeSettingsWorkload();
    [[nodiscard]] int startGpuSettingsWorkload();
    [[nodiscard]] int startFrameGenerationStatsWorkload();
    [[nodiscard]] int startStreamExitWorkload();
    [[nodiscard]] int startSessionFullscreenWorkload();
    [[nodiscard]] int startSessionLaunchWorkload();

    QGuiApplication &m_application;
    QQmlApplicationEngine &m_engine;
    AppController &m_controller;
    CoreClient &m_coreClient;
    const QStringList m_arguments;
    const bool m_smokeTest;
    const bool m_smokeConsolePersistenceRollback;
    const bool m_smokeStreamerEvent;
    const bool m_smokeFullscreenStatsShortcut;
    const qsizetype m_performanceReportIndex;
    const bool m_performanceMode;
    bool m_qmlWarningOccurred = false;
};
