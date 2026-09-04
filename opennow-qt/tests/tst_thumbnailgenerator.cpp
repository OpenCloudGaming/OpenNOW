#include "ThumbnailGenerator.h"

#include <QDir>
#include <QFile>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QtTest>

class ThumbnailGeneratorTest final : public QObject
{
    Q_OBJECT

private slots:
    void rejectsUntrustedAndNonVideoPaths();
};

void ThumbnailGeneratorTest::rejectsUntrustedAndNonVideoPaths()
{
    ThumbnailGenerator generator;
    QVERIFY(!generator.regenerate(QStringLiteral("/tmp/outside.mkv")));
    const auto pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    QDir directory(pictures);
    QVERIFY(directory.mkpath(QStringLiteral("OpenNOW/Recordings")));
    const auto invalid = directory.filePath(QStringLiteral("OpenNOW/Recordings/not-video.txt"));
    QFile file(invalid);
    QVERIFY(file.open(QIODevice::WriteOnly));
    QCOMPARE(file.write("fixture"), 7);
    file.close();
    QVERIFY(!generator.regenerate(invalid));
    QVERIFY(!generator.busy());
    QFile::remove(invalid);
}

QTEST_MAIN(ThumbnailGeneratorTest)
#include "tst_thumbnailgenerator.moc"
