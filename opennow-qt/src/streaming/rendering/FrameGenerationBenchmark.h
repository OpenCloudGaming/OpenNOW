#pragma once

#include <QObject>
#include <QVariantMap>
#include <atomic>
#include <functional>

class QThread;

// Measures how long the frame-generation engine needs per generated frame on this
// machine. It runs offscreen on a private QRhi of the same backend the stream uses,
// with procedural frames at the requested stream resolution, so Settings can enable
// only the factors the GPU and the display can afford for the selected profile.
// Nothing here touches the scene graph or the running stream.
class FrameGenerationBenchmark : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool running READ running NOTIFY runningChanged)
    Q_PROPERTY(double progress READ progress NOTIFY progressChanged)
public:
    explicit FrameGenerationBenchmark(QObject *parent = nullptr);
    ~FrameGenerationBenchmark() override;

    bool running() const { return m_running; }
    double progress() const { return m_progress; }

    // request: {engine: "motion", width, height, seconds (default 3)}; every level up to the
    // engine maximum is timed in turn.
    // Returns false when a run is already in progress or the request is malformed.
    Q_INVOKABLE bool start(const QVariantMap &request);
    Q_INVOKABLE void cancel();

    // Name of the GPU the stream backend would use, resolved once on first call (creates
    // and discards a private QRhi). Part of the profile key so a GPU change invalidates
    // measurements.
    Q_INVOKABLE QString gpuName();

    // Refresh rate in Hz of the screen `window` is on (0 when unknown). QML's Screen type
    // does not expose it, so the shell asks here.
    Q_INVOKABLE double displayRefreshRate(QObject *window) const;

    // Highest factor the installed engine can synthesise. A property of the engine, not of
    // a stored measurement: profiles saved by an older engine do not cap newer ones.
    Q_INVOKABLE int engineMaxFactor() const;

    // Pure decision: which factors fit the measured cost for `fps` and `refreshHz`.
    // Result: {allowed: [..], budgetMs, costMs: {"2": .., "3": .., "4": ..},
    //          reasons: {"2": "", "3": "gpu"|"display"|"engine", ...}}
    Q_INVOKABLE static QVariantMap evaluate(const QVariantMap &measurement, int fps, double refreshHz);

    // Runs the measurement on the calling thread. Exposed for tests.
    static QVariantMap measure(const QVariantMap &request, const std::atomic_bool *cancelled,
                               const std::function<void(double)> &progress);

signals:
    void runningChanged();
    void progressChanged();
    void finished(const QVariantMap &result);

private:
    void setProgress(double value);

    QThread *m_thread = nullptr;
    std::atomic_bool m_cancelled = false;
    bool m_running = false;
    bool m_gpuNameResolved = false;
    double m_progress = 0;
    QString m_gpuName;
};
