#include "app/QmlNetworkAccessManagerFactory.h"

#include <QCoreApplication>
#include <QNetworkAccessManager>
#include <QNetworkRequest>

namespace {
class QmlNetworkAccessManager final : public QNetworkAccessManager
{
public:
    explicit QmlNetworkAccessManager(QObject *parent)
        : QNetworkAccessManager(parent),
          m_userAgent("OpenNOW/" + QCoreApplication::applicationVersion().toUtf8())
    {
    }

protected:
    QNetworkReply *createRequest(Operation operation, const QNetworkRequest &request,
                                 QIODevice *outgoingData) override
    {
        QNetworkRequest identified(request);
        identified.setRawHeader("User-Agent", m_userAgent);
        return QNetworkAccessManager::createRequest(operation, identified, outgoingData);
    }

private:
    const QByteArray m_userAgent;
};
}

QNetworkAccessManager *QmlNetworkAccessManagerFactory::create(QObject *parent)
{
    return new QmlNetworkAccessManager(parent);
}
