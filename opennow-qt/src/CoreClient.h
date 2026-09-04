#pragma once

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QProcess>
#include <QQueue>
#include <QStringList>
#include <QTimer>

class CoreClient final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)
    Q_PROPERTY(int protocolVersion READ protocolVersion CONSTANT)

public:
    static constexpr int CurrentProtocolVersion = 1;
    static constexpr qsizetype MaximumLineBytes = 1024 * 1024;
    static constexpr qsizetype MaximumQueuedEvents = 512;

    explicit CoreClient(QObject *parent = nullptr);
    ~CoreClient() override;

    [[nodiscard]] QString state() const;
    [[nodiscard]] QString lastError() const;
    [[nodiscard]] int protocolVersion() const;

    Q_INVOKABLE bool start(const QString &program, const QStringList &arguments = {});
    Q_INVOKABLE void stop();
    Q_INVOKABLE QString request(const QString &method,
                                const QJsonObject &params = {},
                                int timeoutMs = 15'000);
    Q_INVOKABLE bool cancel(const QString &requestId);

signals:
    void stateChanged();
    void lastErrorChanged();
    void responseReceived(const QString &requestId, const QJsonObject &result);
    void requestFailed(const QString &requestId, const QString &code, const QString &message);
    void eventReceived(const QString &name, const QJsonObject &payload);
    void eventsDropped(int count);
    void coreLogReceived(const QString &line);

private slots:
    void processStdout();
    void processStderr();
    void processTimeouts();
    void drainEvents();

private:
    struct PendingRequest {
        QString method;
        qint64 deadlineMs = 0;
    };

    void setState(const QString &state);
    void setLastError(const QString &error);
    bool writeMessage(const QJsonObject &message);
    void processLine(const QByteArray &line);
    void failAll(const QString &code, const QString &message);
    void protocolFailure(const QString &message);
    void scheduleRestart();

    QProcess m_process;
    QTimer m_timeoutTimer;
    QByteArray m_stdoutBuffer;
    QByteArray m_stderrBuffer;
    QHash<QString, PendingRequest> m_pending;
    QQueue<QJsonObject> m_events;
    QString m_state = QStringLiteral("stopped");
    QString m_lastError;
    quint64 m_nextRequestId = 1;
    int m_droppedEvents = 0;
    bool m_eventDrainScheduled = false;
    QString m_handshakeRequestId;
    QString m_program;
    QStringList m_arguments;
    QTimer m_restartTimer;
    int m_restartAttempts = 0;
    bool m_manualStop = false;
};
