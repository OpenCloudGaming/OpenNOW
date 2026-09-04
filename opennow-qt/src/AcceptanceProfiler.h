#pragma once

#include <QElapsedTimer>
#include <QJsonArray>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QSize>
#include <QVector>

#include <functional>

class AppController;
class QQuickWindow;

class AcceptanceProfiler final : public QObject
{
public:
    struct Options {
        QString reportPath;
        int width = 1920;
        int height = 1080;
        int cycles = 3;
        double refreshRateHz = 0.0;
        QString machineLabel;
        bool requireHardware = false;
    };

    using Completion = std::function<void(bool)>;

    AcceptanceProfiler(QQuickWindow *window,
                       AppController *controller,
                       Options options,
                       Completion completion,
                       QObject *parent = nullptr);

    [[nodiscard]] bool start();

private:
    struct Action {
        QString name;
        QString kind;
        QString value;
        bool overlay = false;
    };

    struct StepResult {
        Action action;
        bool accepted = false;
        bool focusValid = false;
        qint64 startedNs = 0;
        qint64 firstFrameNs = -1;
        qint64 lastFrameNs = -1;
        QVector<double> frameIntervalsMs;
    };

    void advance();
    void observeFrame(qint64 renderedNs);
    void finalizeActiveStep();
    void finish();
    [[nodiscard]] bool writeReport(bool passed,
                                   const QStringList &failures,
                                   const QJsonArray &aggregates) const;
    [[nodiscard]] QJsonArray aggregateMetrics(QStringList *failures) const;
    [[nodiscard]] bool focusIsValid() const;
    [[nodiscard]] double effectiveRefreshRate() const;
    [[nodiscard]] QSize actualPhysicalSize() const;
    [[nodiscard]] QString graphicsApiName() const;
    static double percentile(QVector<double> values, double quantile);

    QQuickWindow *m_window;
    AppController *m_controller;
    Options m_options;
    Completion m_completion;
    QVector<Action> m_actions;
    QVector<StepResult> m_results;
    QElapsedTimer m_timer;
    qsizetype m_activeStep = -1;
    bool m_finished = false;
};
