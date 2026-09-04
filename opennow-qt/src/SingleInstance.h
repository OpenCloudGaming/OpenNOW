#pragma once

#include <QLocalServer>
#include <QObject>
#include <QStringList>

class QLocalSocket;

class SingleInstance final : public QObject
{
    Q_OBJECT

public:
    explicit SingleInstance(QObject *parent = nullptr);

    // Returns true for the primary instance. A false result means the arguments
    // were forwarded to the already-running primary process.
    bool acquire(const QStringList &arguments);

signals:
    void activationRequested(const QStringList &arguments);

private:
    void acceptConnection();
    void readConnection(QLocalSocket *socket);
    static QString serverName();

    QLocalServer m_server;
};
