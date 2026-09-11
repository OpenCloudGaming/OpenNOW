#pragma once

#include <QByteArray>
#include <QObject>
#include <QProcess>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <memory>

class MacAwdlBackend;

class MacAwdlController final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(State state READ state NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)

public:
    enum State { Unsupported, Unavailable, Unknown, Enabled, Disabled };
    Q_ENUM(State)

    explicit MacAwdlController(QObject *parent = nullptr);
    explicit MacAwdlController(std::unique_ptr<MacAwdlBackend> backend,
                               int timeoutMs = 60'000, QObject *parent = nullptr);
    ~MacAwdlController() override;

    State state() const;
    bool busy() const;
    QString error() const;

    Q_INVOKABLE void refresh();
    Q_INVOKABLE void disable();
    Q_INVOKABLE void enable();

signals:
    void statusChanged();
    void busyChanged();
    void errorChanged();

private:
    void request(State target);
    void readStatus();
    void setActionError(const QString &error);
    void updateError();
    void complete(int exitCode, QProcess::ExitStatus exitStatus);

    std::unique_ptr<MacAwdlBackend> m_backend;
    QTimer m_timeout;
    State m_state = Unknown;
    State m_target = Unknown;
    bool m_busy = false;
    bool m_timedOut = false;
    QString m_error;
    QString m_statusError;
    QString m_actionError;
    QString m_processError;
    QByteArray m_output;
};

class MacAwdlBackend : public QObject
{
    Q_OBJECT

public:
    using QObject::QObject;
    virtual MacAwdlController::State readState(QString *error) = 0;
    virtual void start(const QString &program, const QStringList &arguments) = 0;
    virtual void kill() = 0;

signals:
    void standardErrorReady(const QByteArray &output);
    void processError(QProcess::ProcessError error);
    void finished(int exitCode, QProcess::ExitStatus exitStatus);
};
