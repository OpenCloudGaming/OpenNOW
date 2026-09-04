#include "CoreClient.h"

#ifndef OPENNOW_VERSION
#define OPENNOW_VERSION "0.5.4"
#endif

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QStandardPaths>

using namespace Qt::StringLiterals;

namespace {
QString safeText(const QJsonValue &value, const QString &fallback)
{
    if (!value.isString()) {
        return fallback;
    }
    auto text = value.toString().left(512);
    text.replace(u'\n', u' ');
    text.replace(u'\r', u' ');
    return text;
}

void appendCoreDiagnostics(const QList<QByteArray> &lines)
{
    if (lines.isEmpty()) {
        return;
    }
    const auto dataRoot = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    if (dataRoot.isEmpty()) {
        return;
    }
    QDir directory(dataRoot);
    if (!directory.mkpath(u"diagnostics"_s) || !directory.cd(u"diagnostics"_s)) {
        return;
    }

    constexpr qint64 maximumLogBytes = 2 * 1024 * 1024;
    const auto path = directory.filePath(u"native-streamer.log"_s);
    const auto previousPath = directory.filePath(u"native-streamer.previous.log"_s);
    qsizetype incomingBytes = 0;
    for (const auto &line : lines) {
        incomingBytes += line.size() + 40;
    }
    if (QFileInfo(path).size() + incomingBytes > maximumLogBytes) {
        QFile::remove(previousPath);
        QFile::rename(path, previousPath);
    }

    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Append | QIODevice::Text)) {
        return;
    }
    for (const auto &line : lines) {
        file.write(QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs).toUtf8());
        file.write(" ");
        file.write(line.left(2'048));
        file.write("\n");
    }
}
}

CoreClient::CoreClient(QObject *parent)
    : QObject(parent)
{
    m_process.setProcessChannelMode(QProcess::SeparateChannels);
    connect(&m_process, &QProcess::readyReadStandardOutput, this, &CoreClient::processStdout);
    connect(&m_process, &QProcess::readyReadStandardError, this, &CoreClient::processStderr);
    connect(&m_process, &QProcess::started, this, [this] {
        setState(u"handshaking"_s);
        m_handshakeRequestId = request(u"core.hello"_s,
                                       QJsonObject{{u"protocolVersion"_s, CurrentProtocolVersion},
                                                   {u"shell"_s, u"qt"_s},
                                                   {u"shellVersion"_s, QString::fromLatin1(OPENNOW_VERSION)}},
                                       5'000);
    });
    connect(&m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        const auto message = safeText(m_process.errorString(), u"Core process error"_s);
        setLastError(message);
        setState(u"failed"_s);
        failAll(u"core_process_error"_s, message);
        if (error == QProcess::FailedToStart) {
            scheduleRestart();
        }
    });
    connect(&m_process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
            [this](int exitCode, QProcess::ExitStatus status) {
                if (m_state == u"stopping"_s) {
                    setState(u"stopped"_s);
                } else {
                    const auto message = u"Core exited (code %1, %2)"_s.arg(exitCode).arg(
                        status == QProcess::CrashExit ? u"crashed"_s : u"normal"_s);
                    setLastError(message);
                    setState(u"failed"_s);
                    failAll(u"core_exited"_s, message);
                    scheduleRestart();
                }
            });

    m_timeoutTimer.setInterval(100);
    connect(&m_timeoutTimer, &QTimer::timeout, this, &CoreClient::processTimeouts);
    m_timeoutTimer.start();
    m_restartTimer.setSingleShot(true);
    connect(&m_restartTimer, &QTimer::timeout, this, [this] {
        if (!m_manualStop && m_process.state() == QProcess::NotRunning && !m_program.isEmpty()) {
            start(m_program, m_arguments);
        }
    });
}

CoreClient::~CoreClient()
{
    stop();
}

QString CoreClient::state() const { return m_state; }
QString CoreClient::lastError() const { return m_lastError; }
int CoreClient::protocolVersion() const { return CurrentProtocolVersion; }

bool CoreClient::start(const QString &program, const QStringList &arguments)
{
    if (program.trimmed().isEmpty() || m_process.state() != QProcess::NotRunning) {
        return false;
    }
    m_program = program;
    m_arguments = arguments;
    m_manualStop = false;
    m_stdoutBuffer.clear();
    m_stderrBuffer.clear();
    m_events.clear();
    m_droppedEvents = 0;
    setLastError({});
    setState(u"starting"_s);
    m_process.start(program, arguments, QIODevice::ReadWrite | QIODevice::Unbuffered);
    return true;
}

void CoreClient::stop()
{
    m_manualStop = true;
    m_restartTimer.stop();
    if (m_process.state() == QProcess::NotRunning) {
        setState(u"stopped"_s);
        return;
    }
    setState(u"stopping"_s);
    failAll(u"core_stopping"_s, u"Core is stopping"_s);
    m_process.closeWriteChannel();
    // EOF lets the Rust core run its Drop implementations, which stop and
    // reap the out-of-process streamer. Escalate only if graceful shutdown
    // does not finish within the bounded deadline.
    if (!m_process.waitForFinished(1'500)) {
        m_process.terminate();
        if (!m_process.waitForFinished(1'000)) {
            m_process.kill();
            m_process.waitForFinished(1'000);
        }
    }
}

QString CoreClient::request(const QString &method, const QJsonObject &params, int timeoutMs)
{
    if (method.trimmed().isEmpty() || m_process.state() != QProcess::Running) {
        return {};
    }
    const auto id = QString::number(m_nextRequestId++);
    const auto deadline = QDateTime::currentMSecsSinceEpoch() + qBound(100, timeoutMs, 300'000);
    m_pending.insert(id, PendingRequest{method, deadline});
    if (!writeMessage(QJsonObject{{u"type"_s, u"request"_s},
                                  {u"id"_s, id},
                                  {u"method"_s, method},
                                  {u"params"_s, params}})) {
        m_pending.remove(id);
        emit requestFailed(id, u"core_not_writable"_s, u"Core transport is not writable"_s);
        return {};
    }
    return id;
}

bool CoreClient::cancel(const QString &requestId)
{
    if (!m_pending.remove(requestId)) {
        return false;
    }
    writeMessage(QJsonObject{{u"type"_s, u"cancel"_s}, {u"id"_s, requestId}});
    emit requestFailed(requestId, u"cancelled"_s, u"Request cancelled"_s);
    return true;
}

void CoreClient::processStdout()
{
    m_stdoutBuffer += m_process.readAllStandardOutput();
    if (m_stdoutBuffer.size() > MaximumLineBytes && !m_stdoutBuffer.contains('\n')) {
        protocolFailure(u"Core sent an oversized protocol line"_s);
        return;
    }

    qsizetype newline = -1;
    while ((newline = m_stdoutBuffer.indexOf('\n')) >= 0) {
        auto line = m_stdoutBuffer.first(newline).trimmed();
        m_stdoutBuffer.remove(0, newline + 1);
        if (line.size() > MaximumLineBytes) {
            protocolFailure(u"Core sent an oversized protocol line"_s);
            return;
        }
        if (!line.isEmpty()) {
            processLine(line);
        }
    }
}

void CoreClient::processStderr()
{
    m_stderrBuffer += m_process.readAllStandardError();
    QList<QByteArray> diagnosticLines;
    qsizetype newline = -1;
    while ((newline = m_stderrBuffer.indexOf('\n')) >= 0) {
        auto line = m_stderrBuffer.first(newline).trimmed();
        m_stderrBuffer.remove(0, newline + 1);
        if (line.isEmpty()) {
            continue;
        }
        line = line.left(2'048);
        diagnosticLines.push_back(line);
        emit coreLogReceived(QString::fromUtf8(line));
    }
    if (m_stderrBuffer.size() > MaximumLineBytes) {
        auto line = m_stderrBuffer.first(2'048);
        m_stderrBuffer.clear();
        diagnosticLines.push_back(line);
        emit coreLogReceived(QString::fromUtf8(line));
    }
    appendCoreDiagnostics(diagnosticLines);
}

void CoreClient::processTimeouts()
{
    const auto now = QDateTime::currentMSecsSinceEpoch();
    QStringList expired;
    for (auto it = m_pending.cbegin(); it != m_pending.cend(); ++it) {
        if (it->deadlineMs <= now) {
            expired.push_back(it.key());
        }
    }
    for (const auto &id : expired) {
        m_pending.remove(id);
        if (id == m_handshakeRequestId) {
            protocolFailure(u"Core handshake timed out"_s);
            return;
        }
        writeMessage(QJsonObject{{u"type"_s, u"cancel"_s}, {u"id"_s, id}});
        emit requestFailed(id, u"deadline_exceeded"_s, u"Core request timed out"_s);
    }
}

void CoreClient::drainEvents()
{
    m_eventDrainScheduled = false;
    constexpr int batchSize = 64;
    for (int count = 0; count < batchSize && !m_events.isEmpty(); ++count) {
        const auto event = m_events.dequeue();
        emit eventReceived(event.value(u"name"_s).toString(), event.value(u"payload"_s).toObject());
    }
    if (m_droppedEvents > 0) {
        emit eventsDropped(m_droppedEvents);
        m_droppedEvents = 0;
    }
    if (!m_events.isEmpty()) {
        m_eventDrainScheduled = true;
        QTimer::singleShot(0, this, &CoreClient::drainEvents);
    }
}

void CoreClient::setState(const QString &state)
{
    if (m_state == state) return;
    m_state = state;
    emit stateChanged();
}

void CoreClient::setLastError(const QString &error)
{
    if (m_lastError == error) return;
    m_lastError = error;
    emit lastErrorChanged();
}

bool CoreClient::writeMessage(const QJsonObject &message)
{
    if (m_process.state() != QProcess::Running || !m_process.isWritable()) {
        return false;
    }
    auto payload = QJsonDocument(message).toJson(QJsonDocument::Compact);
    payload.append('\n');
    return m_process.write(payload) == payload.size();
}

void CoreClient::processLine(const QByteArray &line)
{
    QJsonParseError parseError;
    const auto document = QJsonDocument::fromJson(line, &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
        protocolFailure(u"Core sent malformed JSON"_s);
        return;
    }

    const auto message = document.object();
    const auto type = message.value(u"type"_s).toString();
    if (type == u"response"_s) {
        const auto id = message.value(u"id"_s).toString();
        if (!m_pending.remove(id)) return;
        if (message.value(u"ok"_s).toBool(false)) {
            const auto result = message.value(u"result"_s).toObject();
            if (id == m_handshakeRequestId) {
                const auto version = result.value(u"protocolVersion"_s).toInt(-1);
                if (version != CurrentProtocolVersion) {
                    protocolFailure(u"Core protocol version is incompatible"_s);
                    return;
                }
                m_restartAttempts = 0;
                setState(u"ready"_s);
            }
            emit responseReceived(id, result);
        } else {
            const auto error = message.value(u"error"_s).toObject();
            const auto code = safeText(error.value(u"code"_s), u"core_error"_s);
            const auto detail = safeText(error.value(u"message"_s), u"Core request failed"_s);
            if (id == m_handshakeRequestId) {
                protocolFailure(detail);
                return;
            }
            emit requestFailed(id, code, detail);
        }
        return;
    }

    if (type == u"event"_s && message.value(u"name"_s).isString()
            && message.value(u"payload"_s).isObject()) {
        if (m_events.size() >= MaximumQueuedEvents) {
            m_events.dequeue();
            ++m_droppedEvents;
        }
        m_events.enqueue(message);
        if (!m_eventDrainScheduled) {
            m_eventDrainScheduled = true;
            QTimer::singleShot(0, this, &CoreClient::drainEvents);
        }
        return;
    }

    protocolFailure(u"Core sent an unknown protocol message"_s);
}

void CoreClient::failAll(const QString &code, const QString &message)
{
    const auto ids = m_pending.keys();
    m_pending.clear();
    for (const auto &id : ids) {
        emit requestFailed(id, code, message);
    }
}

void CoreClient::protocolFailure(const QString &message)
{
    setLastError(message);
    setState(u"failed"_s);
    failAll(u"protocol_error"_s, message);
    if (m_process.state() != QProcess::NotRunning) {
        m_process.kill();
    }
}

void CoreClient::scheduleRestart()
{
    if (m_manualStop || m_restartTimer.isActive() || m_restartAttempts >= 3 || m_program.isEmpty()) {
        return;
    }
    const auto delayMs = 250 * (1 << m_restartAttempts);
    ++m_restartAttempts;
    m_restartTimer.start(delayMs);
}
