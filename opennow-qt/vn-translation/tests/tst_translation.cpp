// Unit tests for the self-contained vn-translation module:
//  - Fast strided FNV-1a crop hash & dialogue crop geometry
//  - Cache normalization, eviction, and persistence
//  - WuWaDialogueDb: RichText stripping, Viterbi off-by-one shift repair,
//    dynamic {q_count} template matching, fuzzy lookup (including first-word
//    OCR typos), HUD noise filtering, and flat binary cache (.bin) round-trip
//  - Multi-line Paragraph Line-Merger (mergeAndFilterOcrBlocks)
//  - Auto foreground/background color sampling (sampleOcrBlockColors)
//  - Bounded worker queue, custom API helpers, and frame pump
#include "translation/TranslationCache.h"
#include "translation/TranslationEngine.h"
#include "translation/TranslationWorker.h"
#include "translation/WuWaDialogueDb.h"
#include "translation/CustomApi.h"
#include "translation/OcrEngine.h"
#include "translation/TranslationProvider.h"

#include <QColor>
#include <QFile>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaObject>
#include <QSignalSpy>
#include <QStringList>
#include <QTemporaryDir>
#include <QTest>
#include <QVariant>

class TranslationTest : public QObject
{
    Q_OBJECT
private slots:
    void cropHashChangesWhenPixelsChange()
    {
        QImage first(64, 32, QImage::Format_ARGB32);
        first.fill(Qt::white);
        QImage identical(64, 32, QImage::Format_ARGB32);
        identical.fill(Qt::white);
        QImage changed(64, 32, QImage::Format_ARGB32);
        changed.fill(Qt::white);
        changed.setPixel(10, 10, qRgb(0, 0, 0));

        const QString hashFirst = TranslationEngine::cropHash(first);
        QVERIFY(!hashFirst.isEmpty());
        QCOMPARE(TranslationEngine::cropHash(identical), hashFirst);
        QVERIFY(TranslationEngine::cropHash(changed) != hashFirst);
        QVERIFY(TranslationEngine::cropHash(QImage()).isEmpty());
    }

    void cropDialogueImageRespectsNormalizedRect()
    {
        QImage frame(200, 100, QImage::Format_ARGB32);
        frame.fill(Qt::black);
        const QImage crop = TranslationEngine::cropDialogueImage(
            frame, QRectF(0.05, 0.72, 0.90, 0.24));
        QCOMPARE(crop.width(), 180);
        QCOMPARE(crop.height(), 24);
        QCOMPARE(crop.size(), QSize(180, 24));
        QVERIFY(TranslationEngine::cropDialogueImage(frame, QRectF()).isNull());
        QVERIFY(TranslationEngine::cropDialogueImage(QImage(), QRectF(0, 0, 1, 1)).isNull());
    }

    void cacheLookupHitAndMiss()
    {
        TranslationCache cache(100);
        QString out;
        QVERIFY(!cache.lookup(QStringLiteral("hello"), out));
        cache.insert(QStringLiteral("hello"), QStringLiteral("xin chào"));
        QVERIFY(cache.lookup(QStringLiteral("hello"), out));
        QCOMPARE(out, QStringLiteral("xin chào"));
        QCOMPARE(cache.size(), 1);
    }

    void cacheNormalizationIsCaseAndWhitespaceInsensitive()
    {
        TranslationCache cache(100);
        cache.insert(QStringLiteral("  Hello   World  "), QStringLiteral("xin chào thế giới"));
        QString out;
        QVERIFY(cache.lookup(QStringLiteral("hello world"), out));
        QCOMPARE(out, QStringLiteral("xin chào thế giới"));
        QVERIFY(cache.lookup(QStringLiteral("HELLO   WORLD"), out));
        QVERIFY(cache.lookup(QStringLiteral("\nhello\tworld\n"), out));
        QCOMPARE(TranslationCache::normalizeKey(QStringLiteral("  A   B ")),
                 QStringLiteral("a b"));
    }

    void cacheClearEmptiesIt()
    {
        TranslationCache cache(100);
        cache.insert(QStringLiteral("a"), QStringLiteral("b"));
        QCOMPARE(cache.size(), 1);
        cache.clear();
        QCOMPARE(cache.size(), 0);
        QString out;
        QVERIFY(!cache.lookup(QStringLiteral("a"), out));
    }

    void cacheEvictsOldestAtMaxEntries()
    {
        TranslationCache cache(3);
        cache.insert(QStringLiteral("one"), QStringLiteral("1"));
        cache.insert(QStringLiteral("two"), QStringLiteral("2"));
        cache.insert(QStringLiteral("three"), QStringLiteral("3"));
        cache.insert(QStringLiteral("four"), QStringLiteral("4"));
        QCOMPARE(cache.size(), 3);
        QString out;
        QVERIFY(!cache.lookup(QStringLiteral("one"), out));
        QVERIFY(cache.lookup(QStringLiteral("four"), out));
    }

    void cachePersistsToJson()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cache.json"));
        {
            TranslationCache cache(path, 100);
            cache.insert(QStringLiteral("hello"), QStringLiteral("xin chào"));
            QVERIFY(cache.save());
        }
        TranslationCache reloaded(path, 100);
        QVERIFY(reloaded.load());
        QString out;
        QVERIFY(reloaded.lookup(QStringLiteral("hello"), out));
        QCOMPARE(out, QStringLiteral("xin chào"));
    }

    void dialogueDbLookupMissesOnEmptyDb()
    {
        WuWaDialogueDb db;
        QString out;
        QVERIFY(!db.lookup(QStringLiteral("hello, Rover"), out));
        QCOMPARE(db.size(), 0);
    }

    void dialogueDbLoadsSeedFile()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("wuwa.json"));
        QJsonObject entries;
        entries.insert(QStringLiteral("Hello, Rover"), QStringLiteral("Chào, Rover"));
        QJsonObject root;
        root.insert(QStringLiteral("version"), 1);
        root.insert(QStringLiteral("entries"), entries);
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QJsonDocument(root).toJson(QJsonDocument::Compact));
        file.close();

        WuWaDialogueDb db;
        QVERIFY(db.loadDialogueDb(path));
        QCOMPARE(db.size(), 1);
        QString out;
        QVERIFY(db.lookup(QStringLiteral("hello, rover"), out));
        QCOMPARE(out, QStringLiteral("Chào, Rover"));
        QVERIFY(!db.lookup(QStringLiteral("unknown line"), out));
        WuWaDialogueDb missing;
        QVERIFY(missing.loadDialogueDb(dir.filePath(QStringLiteral("nope.json"))));
        QCOMPARE(missing.size(), 0);
    }

    void dialogueDbChunkFileMergesWithoutClearing()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString seedPath = dir.filePath(QStringLiteral("seed.json"));
        const QString chunkPath = dir.filePath(QStringLiteral("chunk.json"));
        const QString badPath = dir.filePath(QStringLiteral("bad.json"));
        QJsonObject seedEntries;
        seedEntries.insert(QStringLiteral("Hello, Rover"), QStringLiteral("Chào, Rover"));
        QJsonObject seedRoot;
        seedRoot.insert(QStringLiteral("version"), 1);
        seedRoot.insert(QStringLiteral("entries"), seedEntries);
        QFile seedFile(seedPath);
        QVERIFY(seedFile.open(QIODevice::WriteOnly));
        seedFile.write(QJsonDocument(seedRoot).toJson(QJsonDocument::Compact));
        seedFile.close();
        QJsonObject chunkEntries;
        chunkEntries.insert(QStringLiteral("Hello, Rover"), QStringLiteral("Chào Rover 2"));
        chunkEntries.insert(QStringLiteral("Talk to Shilang"), QStringLiteral("Nói chuyện với Shilang"));
        chunkEntries.insert(QStringLiteral("Good morning"), QStringLiteral("Chào buổi sáng"));
        QJsonObject chunkRoot;
        chunkRoot.insert(QStringLiteral("version"), 1);
        chunkRoot.insert(QStringLiteral("entries"), chunkEntries);
        QFile chunkFile(chunkPath);
        QVERIFY(chunkFile.open(QIODevice::WriteOnly));
        chunkFile.write(QJsonDocument(chunkRoot).toJson(QJsonDocument::Compact));
        chunkFile.close();
        QFile badFile(badPath);
        QVERIFY(badFile.open(QIODevice::WriteOnly));
        badFile.write(QByteArrayLiteral("{not json"));
        badFile.close();

        WuWaDialogueDb db;
        QVERIFY(db.loadDialogueDb(seedPath));
        QCOMPARE(db.size(), 1);
        QVERIFY(db.loadChunkFile(chunkPath));
        QCOMPARE(db.size(), 3);
        QString out;
        QVERIFY(db.lookup(QStringLiteral("hello, rover"), out));
        QCOMPARE(out, QStringLiteral("Chào Rover 2"));
        QVERIFY(db.lookup(QStringLiteral("talk to shilang"), out));
        QCOMPARE(out, QStringLiteral("Nói chuyện với Shilang"));
        QVERIFY(db.loadChunkFile(dir.filePath(QStringLiteral("nope.json"))));
        QCOMPARE(db.size(), 3);
        QVERIFY(!db.loadChunkFile(badPath));
        QCOMPARE(db.size(), 3);
    }

    void dialogueDbClearEmptiesIt()
    {
        WuWaDialogueDb db;
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("wuwa.json"));
        QJsonObject entries;
        entries.insert(QStringLiteral("Hello, Rover"), QStringLiteral("Chào, Rover"));
        QJsonObject root;
        root.insert(QStringLiteral("version"), 1);
        root.insert(QStringLiteral("entries"), entries);
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QJsonDocument(root).toJson(QJsonDocument::Compact));
        file.close();
        QVERIFY(db.loadDialogueDb(path));
        QCOMPARE(db.size(), 1);
        db.clear();
        QCOMPARE(db.size(), 0);
        QString out;
        QVERIFY(!db.lookup(QStringLiteral("hello, rover"), out));
        QVERIFY(!db.lookupFuzzy(QStringLiteral("hello, rover"), out));
    }

    void dialogueDbFuzzyLookupToleratesOcrNoise()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("wuwa.json"));
        QJsonObject entries;
        entries.insert(QStringLiteral("Talk to Shilang"), QStringLiteral("Nói chuyện với Shilang"));
        entries.insert(QStringLiteral("The Rover walks into the city"),
                       QStringLiteral("Rover bước vào thành phố"));
        entries.insert(QStringLiteral("Leave the room and take a look outside"),
                       QStringLiteral("Rời khỏi phòng và nhìn ra ngoài"));
        QJsonObject root;
        root.insert(QStringLiteral("version"), 1);
        root.insert(QStringLiteral("entries"), entries);
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QJsonDocument(root).toJson(QJsonDocument::Compact));
        file.close();

        WuWaDialogueDb db;
        QVERIFY(db.loadDialogueDb(path));
        QString out;
        QVERIFY(db.lookupFuzzy(QStringLiteral("Talk to Shilang"), out));
        QCOMPARE(out, QStringLiteral("Nói chuyện với Shilang"));
        QVERIFY(db.lookupFuzzy(QStringLiteral("Talk to Shilang!"), out));
        QCOMPARE(out, QStringLiteral("Nói chuyện với Shilang"));
        QVERIFY(db.lookupFuzzy(QStringLiteral("Talk to Shilanj"), out));
        QCOMPARE(out, QStringLiteral("Nói chuyện với Shilang"));
        // Even a typo in the first word ("Ta1k to Shilang") hits via lastWord fallback!
        QVERIFY(db.lookupFuzzy(QStringLiteral("Ta1k to Shilang"), out));
        QCOMPARE(out, QStringLiteral("Nói chuyện với Shilang"));
        QVERIFY(!db.lookupFuzzy(QStringLiteral("Talk to the vendor"), out));
        QVERIFY(!db.lookupFuzzy(QStringLiteral("   "), out));
        QCOMPARE(WuWaDialogueDb::similarityRatio(QStringLiteral("abc"), QStringLiteral("abc")), 1.0);
        QVERIFY(qAbs(WuWaDialogueDb::similarityRatio(QStringLiteral("abc"), QStringLiteral("abd"))
                     - (1.0 - 1.0 / 3.0))
                < 0.001);
        QCOMPARE(WuWaDialogueDb::similarityRatio(QString(), QString()), 1.0);
        QCOMPARE(WuWaDialogueDb::similarityRatio(QStringLiteral("a"), QString()), 0.0);
    }

    void dialogueDbRichTextTagsAndTemplates()
    {
        QCOMPARE(WuWaDialogueDb::stripRichTextTags(
                     QStringLiteral("<color=#ffd12f>Base HP</color>: <size=32>100</size>")),
                 QStringLiteral("Base HP: 100"));

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("rich.json"));
        QJsonObject entries;
        entries.insert(QStringLiteral("<color=yellow>Time Left: {q_count}s</color>"),
                       QStringLiteral("<color=yellow>Thời gian còn lại: {q_count} giây</color>"));
        entries.insert(QStringLiteral("Defeat all enemies within 120s ({q_count}/5)"),
                       QStringLiteral("Đánh bại tất cả kẻ địch trong 120 giây ({q_count}/5)"));
        QJsonObject root;
        root.insert(QStringLiteral("version"), 1);
        root.insert(QStringLiteral("entries"), entries);
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QJsonDocument(root).toJson(QJsonDocument::Compact));
        file.close();

        WuWaDialogueDb db;
        QVERIFY(db.loadDialogueDb(path));
        QString out;
        QVERIFY(db.lookup(QStringLiteral("Time Left: 45s"), out));
        QCOMPARE(out, QStringLiteral("Thời gian còn lại: 45 giây"));
        QVERIFY(db.lookupFuzzy(QStringLiteral("Defeat all enemies within 120s (3/5)"), out));
        QCOMPARE(out, QStringLiteral("Đánh bại tất cả kẻ địch trong 120 giây (3/5)"));
    }

    void dialogueDbRepairsOffByOneShiftedSlices()
    {
        // Reproduces the exact off-by-one shift pattern found in vi_chunk_001.json
        const QByteArray rawJson = R"JSON({
            "version": 1,
            "entries": {
                "Talk to Shilang": "Nói chuyện với Shilang",
                "Adjust the time meter of the Chronosorter to the center": "Đọc",
                "Read": "Dùng Chronosorter trong phòng đá để nâng phiến đá",
                "Use the Chronosorter in the stone room to raise the flagstone": "Dùng Levitator lên Tacetite Fulminate",
                "Use Levitator on the Tacetite Fulminate": "Dùng Levitator lên Tacetite Fulminate"
            }
        })JSON";
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("shifted.json"));
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(rawJson);
        file.close();

        WuWaDialogueDb db;
        QVERIFY(db.loadDialogueDb(path));
        QString out;
        QVERIFY(db.lookup(QStringLiteral("Read"), out));
        QCOMPARE(out, QStringLiteral("Đọc"));
        QVERIFY(db.lookup(QStringLiteral("Use the Chronosorter in the stone room to raise the flagstone"), out));
        QCOMPARE(out, QStringLiteral("Dùng Chronosorter trong phòng đá để nâng phiến đá"));
    }

    void dialogueDbBinaryCacheRoundTrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString jsonPath = dir.filePath(QStringLiteral("wuwa.json"));
        const QString binPath = dir.filePath(QStringLiteral("wuwa.bin"));
        QJsonObject entries;
        entries.insert(QStringLiteral("Talk to Shilang"), QStringLiteral("Nói chuyện với Shilang"));
        entries.insert(QStringLiteral("Defeat TD: {q_count}/7"), QStringLiteral("Đánh bại TD: {q_count}/7"));
        QJsonObject root;
        root.insert(QStringLiteral("version"), 1);
        root.insert(QStringLiteral("entries"), entries);
        QFile file(jsonPath);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QJsonDocument(root).toJson(QJsonDocument::Compact));
        file.close();

        WuWaDialogueDb db1;
        QVERIFY(db1.loadDialogueDb(jsonPath));
        QVERIFY(db1.saveBinaryCache(binPath));

        WuWaDialogueDb db2;
        QVERIFY(db2.loadBinaryCache(binPath));
        QCOMPARE(db2.size(), 2);
        QString out;
        QVERIFY(db2.lookup(QStringLiteral("Talk to Shilang"), out));
        QCOMPARE(out, QStringLiteral("Nói chuyện với Shilang"));
        QVERIFY(db2.lookup(QStringLiteral("Defeat TD: 4/7"), out));
        QCOMPARE(out, QStringLiteral("Đánh bại TD: 4/7"));
    }

    void hudNoiseFilterAndParagraphLineMerger()
    {
        QVERIFY(WuWaDialogueDb::isHudNoise(QStringLiteral("60 FPS")));
        QVERIFY(WuWaDialogueDb::isHudNoise(QStringLiteral("28ms")));
        QVERIFY(WuWaDialogueDb::isHudNoise(QStringLiteral("UID: 900123456")));
        QVERIFY(WuWaDialogueDb::isHudNoise(QStringLiteral("LV.90")));
        QVERIFY(WuWaDialogueDb::isHudNoise(QStringLiteral("18450/18450")));
        QVERIFY(WuWaDialogueDb::isHudNoise(QStringLiteral("E")));
        QVERIFY(!WuWaDialogueDb::isHudNoise(QStringLiteral("Talk to Shilang")));

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString jsonPath = dir.filePath(QStringLiteral("wuwa.json"));
        QJsonObject entries;
        entries.insert(
            QStringLiteral("Recently, a mysterious rift has appeared in the mine, and the ore merchant, Shilang is troubled by this matter..."),
            QStringLiteral("Gần đây, một vết nứt bí ẩn đã xuất hiện trong mỏ, và thương nhân quặng Shilang đang đau đầu vì chuyện này..."));
        QJsonObject root;
        root.insert(QStringLiteral("version"), 1);
        root.insert(QStringLiteral("entries"), entries);
        QFile file(jsonPath);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QJsonDocument(root).toJson(QJsonDocument::Compact));
        file.close();

        WuWaDialogueDb db;
        QVERIFY(db.loadDialogueDb(jsonPath));

        const QList<OcrBlock> rawBlocks = {
            OcrBlock{QStringLiteral("60 FPS"), QRectF(0.92, 0.02, 0.06, 0.025)},
            OcrBlock{QStringLiteral("Recently, a mysterious rift has appeared in the mine,"),
                     QRectF(0.15, 0.78, 0.70, 0.035)},
            OcrBlock{QStringLiteral("and the ore merchant, Shilang is troubled by this matter..."),
                     QRectF(0.15, 0.82, 0.70, 0.035)},
            OcrBlock{QStringLiteral("UID: 900555111"), QRectF(0.85, 0.96, 0.12, 0.02)}
        };

        const QList<OcrBlock> merged = mergeAndFilterOcrBlocks(rawBlocks, &db, true);
        QCOMPARE(merged.size(), 1);
        QString vi;
        QVERIFY(db.lookup(merged.first().text, vi));
        QVERIFY(vi.startsWith(QStringLiteral("Gần đây, một vết nứt bí ẩn")));
    }

    void ocrBlockColorSamplingDetectsGoldAndWhite()
    {
        QImage img(100, 40, QImage::Format_ARGB32);
        img.fill(qRgb(12, 14, 20)); // dark scene background
        // Draw warm gold pixels in the middle
        for (int y = 10; y < 30; ++y) {
            for (int x = 20; x < 80; ++x)
                img.setPixel(x, y, qRgb(255, 210, 60));
        }
        QString fg, bg;
        sampleOcrBlockColors(img, QRectF(0.0, 0.0, 1.0, 1.0), fg, bg);
        QCOMPARE(fg, QStringLiteral("#FFE066"));
        QVERIFY(bg.startsWith(QChar(u'#')));
    }

    void boundedQueueDropsStaleJob()
    {
        TranslationJobQueue queue;
        TranslationJob first;
        first.directText = QStringLiteral("first");
        TranslationJob second;
        second.directText = QStringLiteral("second");
        TranslationJob third;
        third.directText = QStringLiteral("third");

        QVERIFY(queue.submit(first));
        QVERIFY(queue.hasRunning());
        QVERIFY(!queue.hasWaiting());
        QVERIFY(queue.submit(second));
        QVERIFY(queue.hasWaiting());
        QCOMPARE(queue.droppedStale(), 0);
        QVERIFY(queue.submit(third));
        QCOMPARE(queue.droppedStale(), 1);
        QVERIFY(queue.hasWaiting());

        const auto running = queue.takeRunning();
        QVERIFY(running.has_value());
        QCOMPARE(running->directText, QStringLiteral("first"));
        queue.finishRunning();
        const auto next = queue.takeRunning();
        QVERIFY(next.has_value());
        QCOMPARE(next->directText, QStringLiteral("third"));
        queue.finishRunning();
        QVERIFY(!queue.takeRunning().has_value());
    }

    void stubOcrEnginesReportNotAvailable()
    {
        RapidOcrEngine rapid;
        QVERIFY(!rapid.available());
        QImage image(10, 10, QImage::Format_ARGB32);
        image.fill(Qt::white);
        QCOMPARE(rapid.recognize(image).status, OcrResult::Status::NotAvailable);
        // In offscreen test mode, WindowsOcrEngine also deterministically reports NotAvailable
        WindowsOcrEngine windows;
        QVERIFY(!windows.available());
        QCOMPARE(windows.recognize(image).status, OcrResult::Status::NotAvailable);
        GeminiVisionOcrEngine gemini;
        QVERIFY(!gemini.available());
        QCOMPARE(gemini.recognize(image).status, OcrResult::Status::NotAvailable);
        QVERIFY(createOcrEngine(QStringLiteral("bogus")) == nullptr);
    }

    void dialoguePresetRects()
    {
        QCOMPARE(TranslationEngine::dialogueRectForPreset(QStringLiteral("wuthering-waves")),
                 QRectF(0.05, 0.72, 0.90, 0.24));
        QCOMPARE(TranslationEngine::dialogueRectForPreset(QStringLiteral("generic")),
                 QRectF(0.05, 0.60, 0.90, 0.30));
        QCOMPARE(TranslationEngine::dialogueRectForPreset(QStringLiteral("full-screen")),
                 QRectF(0, 0, 1, 1));
        QVERIFY(TranslationEngine::dialogueRectForPreset(QStringLiteral("bogus")).isNull());
    }

    void migrateDialoguePresetCases()
    {
        QCOMPARE(TranslationEngine::migrateDialoguePreset(QStringLiteral("wuthering-waves")),
                 QStringLiteral("full-screen"));
        QCOMPARE(TranslationEngine::migrateDialoguePreset(QStringLiteral("generic")),
                 QStringLiteral("generic"));
        QCOMPARE(TranslationEngine::migrateDialoguePreset(QStringLiteral("full-screen")),
                 QStringLiteral("full-screen"));
        QCOMPARE(TranslationEngine::migrateDialoguePreset(QString()),
                 QStringLiteral("full-screen"));
        QCOMPARE(TranslationEngine::migrateDialoguePreset(QStringLiteral("bogus")),
                 QStringLiteral("full-screen"));
    }

    void chunkDownloadCompleteCases()
    {
        QVERIFY(!TranslationEngine::isChunkDownloadComplete(0, 0));
        QVERIFY(!TranslationEngine::isChunkDownloadComplete(0, 220764));
        QVERIFY(!TranslationEngine::isChunkDownloadComplete(220763, 220764));
        QVERIFY(TranslationEngine::isChunkDownloadComplete(220764, 220764));
        QVERIFY(TranslationEngine::isChunkDownloadComplete(220765, 220764));
        QVERIFY(!TranslationEngine::isChunkDownloadComplete(5, 0));
    }

    void mapBlockRectToFrameCases()
    {
        auto nearRect = [](const QRectF &a, const QRectF &b) {
            return qAbs(a.x() - b.x()) < 1e-9 && qAbs(a.y() - b.y()) < 1e-9
                && qAbs(a.width() - b.width()) < 1e-9 && qAbs(a.height() - b.height()) < 1e-9;
        };
        QVERIFY(nearRect(
            ::mapBlockRectToFrame(QRectF(0.1, 0.2, 0.5, 0.3), QRectF(0.05, 0.72, 0.90, 0.24)),
            QRectF(0.14, 0.768, 0.45, 0.072)));
        QVERIFY(nearRect(
            ::mapBlockRectToFrame(QRectF(0.2, 0.1, 0.4, 0.05), QRectF(0, 0, 1, 1)),
            QRectF(0.2, 0.1, 0.4, 0.05)));
        QVERIFY(nearRect(
            ::mapBlockRectToFrame(QRectF(0.9, 0.9, 0.5, 0.5), QRectF(0, 0, 1, 1)),
            QRectF(0.9, 0.9, 0.1, 0.1)));
    }

    void blocksToVariantListRoundTrip()
    {
        const QList<TranslatedBlock> blocks = {
            TranslatedBlock{QStringLiteral("Hello"), QStringLiteral("Xin chào"),
                            QRectF(0.1, 0.2, 0.5, 0.1)},
            TranslatedBlock{QStringLiteral("World"), QStringLiteral("Thế giới"),
                            QRectF(0.1, 0.35, 0.4, 0.1)},
        };
        const QVariantList list = TranslationEngine::blocksToVariantList(blocks);
        QCOMPARE(list.size(), 2);
        QCOMPARE(list.at(0).toMap().value(QStringLiteral("translated")).toString(),
                 QStringLiteral("Xin chào"));
        QCOMPARE(list.at(0).toMap().value(QStringLiteral("fgColor")).toString(),
                 QStringLiteral("#F4F6FA"));
        QCOMPARE(list.at(1).toMap().value(QStringLiteral("x")).toDouble(), 0.1);
        QCOMPARE(list.at(1).toMap().value(QStringLiteral("h")).toDouble(), 0.1);
        const QVariantList single = TranslationEngine::singleBlockVariantList(
            QStringLiteral("Hi"), QStringLiteral("Chào"), QRectF(0.05, 0.72, 0.90, 0.24));
        QCOMPARE(single.size(), 1);
        QCOMPARE(single.at(0).toMap().value(QStringLiteral("y")).toDouble(), 0.72);
    }

    void testJobDoesNotShowOnStream()
    {
        TranslationEngine engine;
        TranslationJobResult testResult;
        testResult.ok = true;
        testResult.testJob = true;
        testResult.original = QStringLiteral("Hello world");
        testResult.translated = QStringLiteral("Xin chào thế giới");
        testResult.ocrMs = 10;
        testResult.translateMs = 20;
        QVERIFY(QMetaObject::invokeMethod(&engine, "onWorkerJobCompleted",
                                          Q_ARG(TranslationJobResult, testResult)));
        QCOMPARE(engine.lastTranslatedText(), QStringLiteral("Xin chào thế giới"));
        QVERIFY(!engine.hasTranslation());
        QVERIFY(engine.translationBlocks().isEmpty());

        TranslationJobResult streamResult;
        streamResult.ok = true;
        streamResult.testJob = false;
        streamResult.original = QStringLiteral("Hello world");
        streamResult.translated = QStringLiteral("Xin chào thế giới");
        streamResult.blocks = {TranslatedBlock{QStringLiteral("Hello world"),
                                               QStringLiteral("Xin chào thế giới"),
                                               QRectF(0.1, 0.2, 0.5, 0.1)}};
        QVERIFY(QMetaObject::invokeMethod(&engine, "onWorkerJobCompleted",
                                          Q_ARG(TranslationJobResult, streamResult)));
        QVERIFY(engine.hasTranslation());
        QCOMPARE(engine.translationBlocks().size(), 1);
        QCOMPARE(engine.translationBlocks().at(0).toMap()
                     .value(QStringLiteral("translated")).toString(),
                 QStringLiteral("Xin chào thế giới"));
    }

    void customApiBaseNormalizationCases()
    {
        QCOMPARE(normalizeCustomApiBase(QStringLiteral("https://api.openai.com/v1")),
                 QStringLiteral("https://api.openai.com/v1"));
        QCOMPARE(normalizeCustomApiBase(QStringLiteral("https://api.openai.com/v1/")),
                 QStringLiteral("https://api.openai.com/v1"));
        QCOMPARE(normalizeCustomApiBase(QStringLiteral("https://x.ai/v1/chat/completions")),
                 QStringLiteral("https://x.ai/v1"));
        QCOMPARE(normalizeCustomApiBase(QStringLiteral("  https://x.ai/v1/  ")),
                 QStringLiteral("https://x.ai/v1"));
        QVERIFY(normalizeCustomApiBase(QString()).isEmpty());
        QCOMPARE(customApiModelsUrl(QStringLiteral("https://x.ai/v1/")),
                 QStringLiteral("https://x.ai/v1/models"));
        QCOMPARE(customApiChatUrl(QStringLiteral("https://x.ai/v1/chat/completions")),
                 QStringLiteral("https://x.ai/v1/chat/completions"));
        QVERIFY(customApiModelsUrl(QString()).isEmpty());
        QVERIFY(customApiChatUrl(QStringLiteral("   ")).isEmpty());
    }

    void customApiModelsParsing()
    {
        QString error;
        const QByteArray body = R"({"object":"list","data":[{"id":"gpt-4o-mini"},{"id":"llama-3.3-70b"},{"id":"gpt-4o-mini"}]})";
        const QStringList models = parseCustomApiModels(body, error);
        QCOMPARE(models.size(), 2);
        QCOMPARE(models.at(0), QStringLiteral("gpt-4o-mini"));
        QCOMPARE(models.at(1), QStringLiteral("llama-3.3-70b"));
        QVERIFY(error.isEmpty());
        QVERIFY(parseCustomApiModels(QByteArrayLiteral("not json"), error).isEmpty());
        QVERIFY(!error.isEmpty());
        QVERIFY(parseCustomApiModels(QByteArrayLiteral("{\"data\":[]}"), error).isEmpty());
        QVERIFY(parseCustomApiModels(QByteArrayLiteral("{\"data\":[{\"id\":\"\"}]}"), error).isEmpty());
    }

    void sseStreamTextParsing()
    {
        const QByteArray sse =
            "data: {\"choices\":[{\"delta\":{\"content\":\"Xin\"}}]}\n\n"
            "data: {\"choices\":[{\"delta\":{\"content\":\" chào\"}}]}\n\n"
            "data: [DONE]\n\n";
        QCOMPARE(parseSseStreamText(sse), QStringLiteral("Xin chào"));
        const QByteArray firstHalf =
            "data: {\"choices\":[{\"delta\":{\"content\":\"Xin\"}}]}\n\ndata: {\"cho";
        QCOMPARE(parseSseStreamText(firstHalf), QStringLiteral("Xin"));
        const QByteArray full = firstHalf + "ices\":[{\"delta\":{\"content\":\" chào\"}}]}\n\ndata: [DONE]\n\n";
        QCOMPARE(parseSseStreamText(full), QStringLiteral("Xin chào"));
        QCOMPARE(parseSseStreamText(QByteArrayLiteral(": ping\n\ndata: [DONE]\n\n")), QString());
        QCOMPARE(parseSseStreamText(
                     QByteArrayLiteral("data: {\"choices\":[{\"message\":{\"content\":\"Rover\"}}]}\n\n")),
                 QStringLiteral("Rover"));
    }

    void chatCompletionTextParsing()
    {
        QString text;
        QString error;
        QVERIFY(parseChatCompletionText(
            QByteArrayLiteral("{\"choices\":[{\"message\":{\"content\":\"  Xin chào Rover  \"}}]}"),
            text, error));
        QCOMPARE(text, QStringLiteral("Xin chào Rover"));
        QVERIFY(parseChatCompletionText(
            QByteArrayLiteral("{\"choices\":[{\"text\":\"Chào\"}]}"), text, error));
        QCOMPARE(text, QStringLiteral("Chào"));
        QVERIFY(!parseChatCompletionText(QByteArrayLiteral("{\"choices\":[]}"), text, error));
        QVERIFY(!parseChatCompletionText(QByteArrayLiteral("not json"), text, error));
    }

    void chatCompletionPayloadShape()
    {
        const QByteArray payload = buildChatCompletionPayload(
            QStringLiteral("gpt-4o-mini"), QStringLiteral("sys"), QStringLiteral("Hello"), {}, true);
        const QJsonDocument doc = QJsonDocument::fromJson(payload);
        QVERIFY(doc.isObject());
        const QJsonObject obj = doc.object();
        QCOMPARE(obj.value(QStringLiteral("model")).toString(), QStringLiteral("gpt-4o-mini"));
        QVERIFY(obj.value(QStringLiteral("stream")).toBool());
        const QJsonArray messages = obj.value(QStringLiteral("messages")).toArray();
        QCOMPARE(messages.size(), 2);
        QCOMPARE(messages.at(0).toObject().value(QStringLiteral("role")).toString(),
                 QStringLiteral("system"));
        QCOMPARE(messages.at(1).toObject().value(QStringLiteral("content")).toString(),
                 QStringLiteral("Hello"));
        const QByteArray vision = buildChatCompletionPayload(
            QStringLiteral("gpt-4o"), QStringLiteral("sys"), QStringLiteral("Read."),
            QStringLiteral("aGVsbG8="), false);
        const QJsonObject vobj = QJsonDocument::fromJson(vision).object();
        QVERIFY(!vobj.contains(QStringLiteral("stream")));
        const QJsonArray vcontent = vobj.value(QStringLiteral("messages")).toArray()
            .at(1).toObject().value(QStringLiteral("content")).toArray();
        QCOMPARE(vcontent.size(), 2);
        QCOMPARE(vcontent.at(1).toObject().value(QStringLiteral("type")).toString(),
                 QStringLiteral("image_url"));
        QVERIFY(vcontent.at(1).toObject().value(QStringLiteral("image_url")).toObject()
                    .value(QStringLiteral("url")).toString()
                    .startsWith(QStringLiteral("data:image/jpeg;base64,")));
    }

    void customProviderAndOcrFactory()
    {
        const CustomApiConfig config{QStringLiteral("https://x.ai/v1"),
                                     QStringLiteral("k"), QStringLiteral("m")};
        QVERIFY(config.isConfigured());
        QVERIFY(!CustomApiConfig{}.isConfigured());
        auto provider = createTranslationProvider(QStringLiteral("custom"), {}, config);
        QVERIFY(provider != nullptr);
        QCOMPARE(provider->id(), QStringLiteral("custom"));
        auto ocr = createOcrEngine(QStringLiteral("custom-vision"), {}, config);
        QVERIFY(ocr != nullptr);
        QCOMPARE(ocr->id(), QStringLiteral("custom-vision"));
        QVERIFY(ocr->available());
        CustomVisionOcrEngine unconfigured;
        QVERIFY(!unconfigured.available());
        QVERIFY(createTranslationProvider(QStringLiteral("bogus")) == nullptr);
        QVERIFY(createOcrEngine(QStringLiteral("bogus")) == nullptr);
    }

    void framePumpFiresWhenEnabled()
    {
        TranslationEngine engine;
        engine.setEnabled(false);
        QSignalSpy spy(&engine, &TranslationEngine::frameRequested);
        engine.setEnabled(true);
        QVERIFY(spy.wait(1000)); // pump interval is 200ms
        QVERIFY(spy.count() >= 1);
        engine.setEnabled(false);
    }
};

QTEST_MAIN(TranslationTest)
#include "tst_translation.moc"
