#include "app/QmlNetworkAccessManagerFactory.h"

#include <QBuffer>
#include <QImage>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTest>

#include <memory>

class QmlNetworkTests : public QObject
{
    Q_OBJECT

private slots:
    void imageRequestsIdentifyOpenNow()
    {
        QCoreApplication::setApplicationVersion(QStringLiteral("test"));
        QImage image(4, 6, QImage::Format_RGB32);
        image.fill(Qt::blue);
        QByteArray png;
        QBuffer buffer(&png);
        QVERIFY(buffer.open(QIODevice::WriteOnly));
        QVERIFY(image.save(&buffer, "PNG"));

        QTcpServer server;
        QVERIFY(server.listen(QHostAddress::LocalHost));
        QByteArray request;
        connect(&server, &QTcpServer::newConnection, &server, [&] {
            auto *socket = server.nextPendingConnection();
            connect(socket, &QTcpSocket::readyRead, socket, [&, socket] {
                request += socket->readAll();
                if (!request.contains("\r\n\r\n"))
                    return;
                const bool identified = request.toLower().contains("\r\nuser-agent: opennow/test\r\n");
                const QByteArray body = identified ? png : QByteArray();
                socket->write(QByteArray("HTTP/1.1 ") + (identified ? "200 OK" : "403 Forbidden")
                              + "\r\nContent-Type: image/png\r\nContent-Length: "
                              + QByteArray::number(body.size()) + "\r\nConnection: close\r\n\r\n" + body);
                socket->disconnectFromHost();
            });
        });

        QmlNetworkAccessManagerFactory factory;
        QQmlEngine engine;
        engine.setNetworkAccessManagerFactory(&factory);
        engine.rootContext()->setContextProperty(QStringLiteral("posterUrl"),
            QUrl(QStringLiteral("http://127.0.0.1:%1/poster.png").arg(server.serverPort())));
        QQmlComponent component(&engine);
        component.setData("import QtQuick; Image { source: posterUrl; asynchronous: true; cache: false }", QUrl());
        std::unique_ptr<QObject> item(component.create());
        QVERIFY2(item, qPrintable(component.errorString()));
        QTRY_VERIFY_WITH_TIMEOUT(item->property("status").toInt() == 1 || item->property("status").toInt() == 3, 5000);
        QCOMPARE(item->property("status").toInt(), 1);
        QCOMPARE(item->property("sourceSize").toSize(), QSize(4, 6));
        QVERIFY(request.toLower().contains("\r\nuser-agent: opennow/test\r\n"));
        QVERIFY(!request.toLower().contains("\r\nauthorization:"));
        QVERIFY(!request.toLower().contains("\r\ncookie:"));
    }
};

QTEST_MAIN(QmlNetworkTests)
#include "tst_qmlnetwork.moc"
