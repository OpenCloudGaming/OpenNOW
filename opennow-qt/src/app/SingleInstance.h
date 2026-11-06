#pragma once

#include <QLocalServer>
#include <QLockFile>
#include <QObject>
#include <QPointer>
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
    static int requestQuit();
    void finishQuitRequest();

public slots:
    void cancelQuitRequest();

signals:
    void activationRequested(const QStringList &arguments);
    void quitRequested();
    void quitRequestAbandoned();

private:
    void acceptConnection();
    void readConnection(QLocalSocket *socket);
    static QString serverName();
    static bool forwardToPrimary(const QString &name, const QStringList &arguments);

    std::unique_ptr<QLockFile> m_lock;
    QLocalServer m_server;
    QPointer<QLocalSocket> m_quitPeer;
};
