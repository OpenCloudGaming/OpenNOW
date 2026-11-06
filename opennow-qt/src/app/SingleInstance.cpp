#include "app/SingleInstance.h"

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QElapsedTimer>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLocalSocket>
#include <QStandardPaths>

#include <cstdlib>

using namespace Qt::StringLiterals;

SingleInstance::SingleInstance(QObject *parent)
    : QObject(parent)
{
    m_server.setSocketOptions(QLocalServer::UserAccessOption);
    connect(&m_server, &QLocalServer::newConnection, this, &SingleInstance::acceptConnection);
}

int SingleInstance::requestQuit()
{
    QLocalSocket peer;
    peer.connectToServer(serverName(), QIODevice::ReadWrite);
    if (!peer.waitForConnected(1'000)) return EXIT_FAILURE;
    const QByteArray payload("[\"--request-quit\"]\n");
    if (peer.write(payload) != payload.size()) return EXIT_FAILURE;
    if (peer.bytesToWrite() > 0 && !peer.waitForBytesWritten(1'000)) return EXIT_FAILURE;
    QElapsedTimer deadline;
    deadline.start();
    for (;;) {
        const auto remaining = 120'000 - deadline.elapsed();
        if (remaining <= 0) return EXIT_FAILURE;
        if (!peer.canReadLine() && !peer.waitForReadyRead(int(remaining)))
            return EXIT_FAILURE;
        const auto result = peer.readLine().trimmed();
        if (result == "closed") return EXIT_SUCCESS;
        if (result == "cancelled") return 2;
        if (result != "pending") return EXIT_FAILURE;
    }
}

void SingleInstance::cancelQuitRequest()
{
    if (!m_quitPeer) return;
    auto *peer = m_quitPeer.data();
    m_quitPeer.clear();
    peer->write("cancelled\n");
    peer->disconnectFromServer();
}

void SingleInstance::finishQuitRequest()
{
    if (!m_quitPeer) return;
    auto *peer = m_quitPeer.data();
    m_quitPeer.clear();
    m_server.close();
    m_lock.reset();
    peer->write("closed\n");
    peer->flush();
    if (peer->bytesToWrite() > 0) peer->waitForBytesWritten(1'000);
    peer->disconnectFromServer();
}

SingleInstance::Acquisition SingleInstance::acquire(const QStringList &arguments)
{
    const auto name = serverName();
    if (forwardToPrimary(name, arguments)) return Acquisition::Forwarded;
    m_lock = std::make_unique<QLockFile>(QDir::temp().filePath(name + u".lock"_s));
    m_lock->setStaleLockTime(0);
    if (!m_lock->tryLock()) {
        if (forwardToPrimary(name, arguments)) return Acquisition::Forwarded;
        qWarning("Could not acquire the single-instance endpoint ownership");
        return Acquisition::Failed;
    }

    QLocalServer::removeServer(name);
    if (!m_server.listen(name)) {
        qWarning("Could not create the single-instance endpoint: %s",
                 qUtf8Printable(m_server.errorString()));
        return Acquisition::Failed;
    }
    return Acquisition::Primary;
}

bool SingleInstance::forwardToPrimary(const QString &name, const QStringList &arguments)
{
    QLocalSocket peer;
    peer.connectToServer(name, QIODevice::ReadWrite);
    if (peer.waitForConnected(250)) {
        QJsonArray encodedArguments;
        for (const auto &argument : arguments) encodedArguments.append(argument);
        auto payload = QJsonDocument(encodedArguments).toJson(QJsonDocument::Compact);
        payload.append('\n');
        const auto queued = peer.write(payload);
        if (queued != payload.size()) {
            qWarning("Could not queue the single-instance activation payload: %s",
                     qUtf8Printable(peer.errorString()));
        }
        peer.flush();
        while (peer.bytesToWrite() > 0 && peer.waitForBytesWritten(500)) {
        }
        if (!peer.waitForReadyRead(1'000) || peer.readLine().trimmed() != QByteArray("ok")) {
            qWarning("The primary instance did not acknowledge the activation payload");
        }
        peer.disconnectFromServer();
        return true;
    }
    return false;
}

void SingleInstance::acceptConnection()
{
    while (auto *socket = m_server.nextPendingConnection()) {
        socket->setParent(this);
        socket->setReadBufferSize(65'536);
        connect(socket, &QLocalSocket::readyRead, this,
                [this, socket] { readConnection(socket); });
        connect(socket, &QLocalSocket::disconnected, this, [this, socket] {
            readConnection(socket);
            if (m_quitPeer == socket) {
                m_quitPeer.clear();
                emit quitRequestAbandoned();
            }
            socket->deleteLater();
        });
        // A short-lived secondary can write and disconnect before newConnection
        // is dispatched. Consume bytes that were already buffered in that case.
        readConnection(socket);
    }
}

void SingleInstance::readConnection(QLocalSocket *socket)
{
    auto buffer = socket->property("opennowBuffer").toByteArray();
    buffer += socket->readAll();
    if (buffer.size() > 65'536) {
        socket->abort();
        return;
    }
    qsizetype newline = -1;
    while ((newline = buffer.indexOf('\n')) >= 0) {
        const auto document = QJsonDocument::fromJson(buffer.first(newline).trimmed());
        buffer.remove(0, newline + 1);
        if (!document.isArray()) continue;
        QStringList arguments;
        for (const auto &value : document.array()) {
            if (value.isString()) arguments.push_back(value.toString());
        }
        if (arguments == QStringList{u"--request-quit"_s}) {
            if (m_quitPeer) {
                socket->write("busy\n");
                socket->disconnectFromServer();
                return;
            }
            m_quitPeer = socket;
            socket->write("pending\n");
            socket->flush();
            emit quitRequested();
            continue;
        }
        emit activationRequested(arguments);
        socket->write("ok\n");
        socket->flush();
    }
    socket->setProperty("opennowBuffer", buffer);
}

QString SingleInstance::serverName()
{
    const auto scope = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation).toUtf8();
    const auto digest = QCryptographicHash::hash(scope, QCryptographicHash::Sha256).toHex().first(16);
    return u"opennow-%1"_s.arg(QString::fromLatin1(digest));
}
