#pragma once

#include <QQmlNetworkAccessManagerFactory>

class QmlNetworkAccessManagerFactory final : public QQmlNetworkAccessManagerFactory
{
public:
    QNetworkAccessManager *create(QObject *parent) override;
};
