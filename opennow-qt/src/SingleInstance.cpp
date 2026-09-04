#include "SingleInstance.h"

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLocalSocket>
#include <QStandardPaths>

using namespace Qt::StringLiterals;

SingleInstance::SingleInstance(QObject *parent)
    : QObject(parent)
{
    connect(&m_server, &QLocalServer::newConnection, this, &SingleInstance::acceptConnection);
}

bool SingleInstance::acquire(const QStringList &arguments)
{
    const auto name = serverName();
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
        return false;
    }

    // Only remove a stale endpoint after proving that no peer accepts it.
    QLocalServer::removeServer(name);
    if (!m_server.listen(name)) {
        qWarning("Could not create the single-instance endpoint: %s",
                 qUtf8Printable(m_server.errorString()));
        return true;
    }
    return true;
}

void SingleInstance::acceptConnection()
{
    while (auto *socket = m_server.nextPendingConnection()) {
        socket->setParent(this);
        connect(socket, &QLocalSocket::readyRead, this,
                [this, socket] { readConnection(socket); });
        connect(socket, &QLocalSocket::disconnected, this, [this, socket] {
            readConnection(socket);
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
    qsizetype newline = -1;
    while ((newline = buffer.indexOf('\n')) >= 0) {
        const auto document = QJsonDocument::fromJson(buffer.first(newline).trimmed());
        buffer.remove(0, newline + 1);
        if (!document.isArray()) continue;
        QStringList arguments;
        for (const auto &value : document.array()) {
            if (value.isString()) arguments.push_back(value.toString());
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
