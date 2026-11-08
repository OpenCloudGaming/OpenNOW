#pragma once

#include <QJsonObject>
#include <QObject>
#include <QString>

// QML fixtures provide request tracking in JavaScript, but their response
// boundary must match CoreClient. A QML `var` signal converts arrays to variant
// sequences on newer Qt versions, unlike the production QJsonObject signal.
class AcceptanceCoreClient : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;

signals:
    void responseReceived(const QString &requestId, const QJsonObject &result);
    void requestFailed(const QString &requestId, const QString &code, const QString &message);
    void eventReceived(const QString &name, const QJsonObject &payload);
};
