#pragma once

#include <QLocalServer>
#include <QLockFile>
#include <QObject>
#include <QStringList>
#include <memory>

class QLocalSocket;

class SingleInstance final : public QObject
{
    Q_OBJECT

public:
    explicit SingleInstance(QObject *parent = nullptr);

    enum class Acquisition { Primary, Forwarded, Failed };
    Acquisition acquire(const QStringList &arguments);

signals:
    void activationRequested(const QStringList &arguments);

private:
    void acceptConnection();
    void readConnection(QLocalSocket *socket);
    static QString serverName();
    static bool forwardToPrimary(const QString &name, const QStringList &arguments);

    std::unique_ptr<QLockFile> m_lock;
    QLocalServer m_server;
};
