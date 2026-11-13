#include "app/ApplicationIcon.h"

#include <QGuiApplication>
#include <QIcon>
#include <QImage>
#include <QWindow>
#include <QtTest>

using namespace Qt::StringLiterals;

class ApplicationIconsTest : public QObject
{
    Q_OBJECT

private slots:
    void bundledIconsReachNewWindows()
    {
        const QIcon icon = applicationWindowIcon();
        for (const int size : {16, 24, 32, 48, 64, 128, 256, 512, 1024}) {
            const QString path = applicationIconResource(size);
            const QImage image(path);
            QVERIFY2(!image.isNull(), qPrintable(path));
            QCOMPARE(image.size(), QSize(size, size));
            QVERIFY(image.hasAlphaChannel());
            QCOMPARE(image.pixelColor(0, 0).alpha(), 0);
#if defined(Q_OS_MACOS)
            QVERIFY(path.contains(u"opennow-macos-"_s));
            // The light tile is part of the bundled macOS artwork.
            if (size >= 128) {
                const QColor tile = image.pixelColor(size / 2, size / 5);
                QCOMPARE(tile.alpha(), 255);
                QVERIFY(tile.red() > 200 && tile.green() > 200 && tile.blue() > 200);
            }
#else
            QVERIFY(!path.contains(u"macos"_s));
#endif
        }
        QCOMPARE(icon.availableSizes().size(), 9);
        QGuiApplication::setWindowIcon(icon);
        QWindow window;
        QCOMPARE(window.icon().cacheKey(), icon.cacheKey());
        for (const qreal scale : {1.0, 2.0}) {
            const QPixmap pixmap = window.icon().pixmap(QSize(32, 32), scale);
            QVERIFY(!pixmap.isNull());
            QCOMPARE(pixmap.size(), QSize(qRound(32 * scale), qRound(32 * scale)));
            QCOMPARE(pixmap.devicePixelRatio(), scale);
        }
    }
};

QTEST_MAIN(ApplicationIconsTest)
#include "tst_applicationicons.moc"
