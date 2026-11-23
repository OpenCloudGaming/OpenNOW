#include "translation/TranslationWorker.h"
#include "translation/CustomApi.h"
#include "translation/OcrEngine.h"
#include "translation/TranslationHttp.h"
#include "translation/TranslationProvider.h"

#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QMetaObject>
#include <QMutexLocker>
#include <algorithm>

namespace {
bool areLinesVerticallyAdjacent(const OcrBlock &upper, const OcrBlock &lower)
{
    const double gapY = lower.rect.top() - upper.rect.bottom();
    const double maxGap = std::max(0.04, upper.rect.height() * 1.65);
    if (gapY < -upper.rect.height() * 0.40 || gapY > maxGap)
        return false;
    const double overlapLeft = std::max(upper.rect.left(), lower.rect.left());
    const double overlapRight = std::min(upper.rect.right(), lower.rect.right());
    if (overlapRight <= overlapLeft)
        return false;
    if (lower.rect.height() < upper.rect.height() * 0.45
        || lower.rect.height() > upper.rect.height() * 2.1)
        return false;
    return true;
}

bool endsWithTerminalPunctuation(const QString &s)
{
    const QString t = s.trimmed();
    if (t.isEmpty())
        return true;
    const QChar last = t.at(t.size() - 1);
    return last == QChar(u'.') || last == QChar(u'!') || last == QChar(u'?')
        || last == QChar(u'"') || last == QChar(u'\'') || last == QChar(u'…');
}
} // namespace

// ---------------------------------------------------------------------------
// TranslationJobQueue
// ---------------------------------------------------------------------------

bool TranslationJobQueue::submit(const TranslationJob &job)
{
    QMutexLocker locker(&m_mutex);
    if (!m_running.has_value()) {
        m_running = job;
        return true;
    }
    if (m_waiting.has_value())
        ++m_droppedStale;
    m_waiting = job;
    return true;
}

std::optional<TranslationJob> TranslationJobQueue::takeRunning()
{
    QMutexLocker locker(&m_mutex);
    if (m_running.has_value())
        return m_running;
    if (!m_waiting.has_value())
        return std::nullopt;
    m_running = std::move(m_waiting);
    m_waiting.reset();
    return m_running;
}

void TranslationJobQueue::finishRunning()
{
    QMutexLocker locker(&m_mutex);
    m_running.reset();
}

bool TranslationJobQueue::hasRunning() const
{
    QMutexLocker locker(&m_mutex);
    return m_running.has_value();
}

bool TranslationJobQueue::hasWaiting() const
{
    QMutexLocker locker(&m_mutex);
    return m_waiting.has_value();
}

void TranslationJobQueue::clear()
{
    QMutexLocker locker(&m_mutex);
    m_running.reset();
    m_waiting.reset();
}

void TranslationJobQueue::clearWaiting()
{
    QMutexLocker locker(&m_mutex);
    m_waiting.reset();
}

QRectF mapBlockRectToFrame(const QRectF &blockRect, const QRectF &sourceRect)
{
    const double x = qBound(0.0, sourceRect.x() + blockRect.x() * sourceRect.width(), 1.0);
    const double y = qBound(0.0, sourceRect.y() + blockRect.y() * sourceRect.height(), 1.0);
    const double w = qBound(0.0, blockRect.width() * sourceRect.width(), 1.0 - x);
    const double h = qBound(0.0, blockRect.height() * sourceRect.height(), 1.0 - y);
    return QRectF(x, y, w, h);
}

QList<OcrBlock> mergeAndFilterOcrBlocks(const QList<OcrBlock> &rawBlocks,
                                        const WuWaDialogueDb *chunkDb,
                                        bool chunkEnabled)
{
    // 1. Strip HUD / numeric combat noise (FPS, ms, UID, LV, single hotkeys)
    QList<OcrBlock> filtered;
    filtered.reserve(rawBlocks.size());
    for (const OcrBlock &b : rawBlocks) {
        const QString clean = b.text.trimmed();
        if (clean.isEmpty() || WuWaDialogueDb::isHudNoise(clean))
            continue;
        filtered.append(OcrBlock{clean, b.rect});
    }
    if (filtered.size() <= 1)
        return filtered;

    // 2. Multi-line Paragraph Merger:
    // Long Wuthering Waves sentences wrap across 2-4 screen lines while the
    // chunk database stores the complete sentence as one entry. Try merging
    // adjacent vertical lines (longest span first: 4, 3, 2) and check if the
    // combined sentence hits the chunk DB.
    QList<OcrBlock> merged;
    merged.reserve(filtered.size());
    int i = 0;
    const int n = filtered.size();
    while (i < n) {
        int maxSpan = 1;
        while (i + maxSpan < n && maxSpan < 4
               && areLinesVerticallyAdjacent(filtered.at(i + maxSpan - 1), filtered.at(i + maxSpan))) {
            ++maxSpan;
        }

        bool matchedMultiLine = false;
        if (maxSpan >= 2 && chunkEnabled && chunkDb) {
            for (int span = maxSpan; span >= 2; --span) {
                QStringList parts;
                QRectF united = filtered.at(i).rect;
                for (int k = 0; k < span; ++k) {
                    parts.append(filtered.at(i + k).text);
                    united = united.united(filtered.at(i + k).rect);
                }
                const QString spaceJoined = parts.join(QChar(u' '));
                QString dummyVi;
                if (chunkDb->lookup(spaceJoined, dummyVi)
                    || chunkDb->lookupFuzzy(spaceJoined, dummyVi)) {
                    merged.append(OcrBlock{spaceJoined, united});
                    i += span;
                    matchedMultiLine = true;
                    break;
                }
            }
        }

        // Also merge wrapped subtitle lines in the bottom dialogue area even
        // when the line is not in the DB yet, if the upper line does not end
        // with terminal punctuation.
        if (!matchedMultiLine && maxSpan >= 2
            && filtered.at(i).rect.top() >= 0.68
            && !endsWithTerminalPunctuation(filtered.at(i).text)) {
            QStringList parts;
            QRectF united = filtered.at(i).rect;
            int span = 1;
            parts.append(filtered.at(i).text);
            while (span < maxSpan && !endsWithTerminalPunctuation(filtered.at(i + span - 1).text)) {
                parts.append(filtered.at(i + span).text);
                united = united.united(filtered.at(i + span).rect);
                ++span;
            }
            merged.append(OcrBlock{parts.join(QChar(u' ')), united});
            i += span;
            matchedMultiLine = true;
        }

        if (!matchedMultiLine) {
            merged.append(filtered.at(i));
            ++i;
        }
    }
    return merged;
}

// ---------------------------------------------------------------------------
// TranslationWorker
// ---------------------------------------------------------------------------

TranslationWorker::TranslationWorker(QObject *parent)
    : QObject(parent)
{
    qRegisterMetaType<TranslationJob>();
    qRegisterMetaType<TranslationJobResult>();
    qRegisterMetaType<TranslatedBlock>();
    qRegisterMetaType<CustomApiConfig>();
    qRegisterMetaType<WuWaDialogueDb *>();
}

TranslationWorker::~TranslationWorker()
{
    stop();
}

void TranslationWorker::start()
{
    if (m_thread.isRunning())
        return;
    m_abortRequested.store(false, std::memory_order_release);
    moveToThread(&m_thread);
    m_thread.start();
}

void TranslationWorker::stop()
{
    m_abortRequested.store(true, std::memory_order_release);
    if (!m_thread.isRunning())
        return;
    m_queue.clearWaiting();
    QMetaObject::invokeMethod(this, "cancelPending", Qt::QueuedConnection);
    m_thread.quit();
    m_thread.wait();
    moveToThread(QThread::currentThread());
}

int TranslationWorker::droppedStaleJobs() const
{
    return m_queue.droppedStale();
}

void TranslationWorker::submitJob(const TranslationJob &job)
{
    if (m_abortRequested.load(std::memory_order_acquire))
        return;
    TranslationJob effective = job;
    if (effective.geminiApiKey.isEmpty())
        effective.geminiApiKey = m_geminiApiKey;
    if (effective.customApi.baseUrl.isEmpty())
        effective.customApi.baseUrl = m_customApi.baseUrl;
    if (effective.customApi.apiKey.isEmpty())
        effective.customApi.apiKey = m_customApi.apiKey;
    if (effective.customApi.model.isEmpty())
        effective.customApi.model = m_customApi.model;
    m_queue.submit(effective);
    if (!m_processing)
        QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
}

void TranslationWorker::setGeminiApiKey(const QString &key)
{
    m_geminiApiKey = key;
}

void TranslationWorker::setCustomApiConfig(const CustomApiConfig &config)
{
    m_customApi = config;
}

void TranslationWorker::setChunkDb(WuWaDialogueDb *db)
{
    m_chunkDb = db;
}

void TranslationWorker::setChunkEnabled(bool enabled)
{
    m_chunkEnabled = enabled;
}

void TranslationWorker::startChunkDownload(const QString &manifestUrl, const QString &cacheDir)
{
    if (m_abortRequested.load(std::memory_order_acquire))
        return;
    m_chunkManifestUrl = manifestUrl;
    m_chunkBaseUrl = manifestUrl;
    if (m_chunkBaseUrl.endsWith(QStringLiteral("manifest.json")))
        m_chunkBaseUrl.chop(QStringLiteral("manifest.json").size());
    m_chunkCacheDir = cacheDir;
    m_chunkFiles.clear();
    m_chunkIndex = 0;
    m_chunkError.clear();
    m_chunkManifestBody.clear();
    m_chunkVersion.clear();
    const TranslationHttpResult result = TranslationHttp::get(manifestUrl);
    if (!result.ok()) {
        emit chunkDownloadFinished(result.error);
        return;
    }
    m_chunkManifestBody = result.body;
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(result.body, &parseError);
    if (parseError.error != QJsonParseError::NoError || !doc.isObject()) {
        emit chunkDownloadFinished(QStringLiteral("Chunk manifest is not valid JSON"));
        return;
    }
    const QJsonObject manifest = doc.object();
    m_chunkVersion = manifest.value(QStringLiteral("version")).toString();
    for (const QJsonValue &value : manifest.value(QStringLiteral("files")).toArray()) {
        const QString file = value.toString();
        if (!file.isEmpty())
            m_chunkFiles.append(file);
    }
    if (m_chunkFiles.isEmpty() || m_chunkVersion.isEmpty()) {
        emit chunkDownloadFinished(QStringLiteral("Chunk manifest lists no files"));
        return;
    }
    emit chunkDownloadProgress(0, m_chunkFiles.size());
    QMetaObject::invokeMethod(this, "downloadNextChunkFile", Qt::QueuedConnection);
}

void TranslationWorker::downloadNextChunkFile()
{
    if (m_abortRequested.load(std::memory_order_acquire))
        return;
    if (m_chunkIndex >= m_chunkFiles.size()) {
        const QString manifestPath = m_chunkCacheDir + QChar(u'/') + m_chunkVersion
            + QStringLiteral("/manifest.json");
        QDir().mkpath(QFileInfo(manifestPath).path());
        QFile manifestFile(manifestPath);
        if (manifestFile.open(QIODevice::WriteOnly | QIODevice::Truncate))
            manifestFile.write(m_chunkManifestBody);
        emit chunkDownloadFinished(m_chunkError);
        return;
    }
    const QString relative = m_chunkFiles.at(m_chunkIndex);
    const QString url = m_chunkBaseUrl + relative;
    const QString destPath = m_chunkCacheDir + QChar(u'/') + m_chunkVersion
        + QChar(u'/') + relative;
    QDir().mkpath(QFileInfo(destPath).path());

    // Skip network fetch if this chunk file is already cached on disk
    QFileInfo existing(destPath);
    if (!existing.exists() || existing.size() <= 16) {
        const TranslationHttpResult result = TranslationHttp::get(url);
        if (result.ok()) {
            QFile out(destPath);
            if (out.open(QIODevice::WriteOnly | QIODevice::Truncate))
                out.write(result.body);
            else if (m_chunkError.isEmpty())
                m_chunkError = QStringLiteral("Cannot write chunk file: ") + relative;
        } else if (m_chunkError.isEmpty()) {
            m_chunkError = result.error;
        }
    }
    ++m_chunkIndex;
    emit chunkDownloadProgress(m_chunkIndex, m_chunkFiles.size());
    if (!m_abortRequested.load(std::memory_order_acquire))
        QMetaObject::invokeMethod(this, "downloadNextChunkFile", Qt::QueuedConnection);
}

void TranslationWorker::fetchCustomApiModels(const QString &base, const QString &apiKey)
{
    const QString url = customApiModelsUrl(base);
    if (url.isEmpty()) {
        emit customApiModelsFetched({}, QStringLiteral("Custom API base URL is empty"));
        return;
    }
    if (apiKey.isEmpty()) {
        emit customApiModelsFetched({}, QStringLiteral("Custom API key is empty"));
        return;
    }
    const TranslationHttpResult result = TranslationHttp::request(
        QStringLiteral("GET"), url,
        {{QByteArrayLiteral("Authorization"), QByteArrayLiteral("Bearer ") + apiKey.toUtf8()}});
    if (!result.ok()) {
        emit customApiModelsFetched({}, result.error);
        return;
    }
    QString error;
    const QStringList models = parseCustomApiModels(result.body, error);
    emit customApiModelsFetched(models, error);
}

void TranslationWorker::cancelPending()
{
    m_queue.clearWaiting();
}

void TranslationWorker::processNext()
{
    if (m_processing || m_abortRequested.load(std::memory_order_acquire))
        return;
    std::optional<TranslationJob> job = m_queue.takeRunning();
    if (!job.has_value())
        return;
    m_processing = true;

    TranslationJobResult result;
    result.sourceRect = job->sourceRect;
    result.cropHash = job->cropHash;
    result.testJob = job->testJob;

    QString original;
    QList<OcrBlock> ocrBlocks;
    qint64 ocrMs = 0;
    if (!job->directText.isEmpty()) {
        original = job->directText;
    } else {
        QElapsedTimer ocrTimer;
        ocrTimer.start();
        // Hybrid Offline-First fallback chain:
        // Try the user's selected engine first, then Windows Native WinRT OCR
        // (~15ms offline), then Tesseract, then Gemini Vision, then Custom Vision.
        // If an online engine fails (e.g. quota/404), fall back to local OCR
        // instead of dropping the frame.
        const QStringList candidates = {
            job->ocrEngineId,
            QStringLiteral("windows-ocr"),
            QStringLiteral("tesseract"),
            QStringLiteral("gemini-vision"),
            QStringLiteral("custom-vision")
        };
        QStringList tried;
        OcrResult ocr;
        bool haveOcrResult = false;
        for (const QString &candidateId : candidates) {
            if (candidateId.isEmpty() || tried.contains(candidateId))
                continue;
            tried.append(candidateId);
            auto engine = createOcrEngine(candidateId, job->geminiApiKey, job->customApi);
            if (!engine || !engine->available())
                continue;
            ocr = engine->recognize(job->crop);
            haveOcrResult = true;
            if (ocr.status == OcrResult::Status::Ok)
                break;
        }
        ocrMs = ocrTimer.elapsed();
        if (!haveOcrResult) {
            result.error = QStringLiteral("No OCR engine available (tried: %1).")
                               .arg(tried.join(QStringLiteral(", ")));
            result.ocrMs = ocrMs;
            m_queue.finishRunning();
            m_processing = false;
            emit jobCompleted(result);
            QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
            return;
        }
        if (ocr.status != OcrResult::Status::Ok) {
            result.error = ocr.error.isEmpty() ? QStringLiteral("OCR failed") : ocr.error;
            result.ocrMs = ocrMs;
            m_queue.finishRunning();
            m_processing = false;
            emit jobCompleted(result);
            QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
            return;
        }

        // Filter HUD noise and merge multi-line wrapped dialogue paragraphs
        ocrBlocks = mergeAndFilterOcrBlocks(ocr.blocks, m_chunkDb, m_chunkEnabled);
        QStringList joinedLines;
        joinedLines.reserve(ocrBlocks.size());
        for (const OcrBlock &b : ocrBlocks)
            joinedLines.append(b.text);
        original = joinedLines.join(QChar(u'\n')).trimmed();

        if (original.isEmpty()) {
            result.ok = true;
            result.original.clear();
            result.translated.clear();
            result.ocrMs = ocrMs;
            m_queue.finishRunning();
            m_processing = false;
            emit jobCompleted(result);
            QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
            return;
        }
    }
    result.original = original;

    auto makeTranslatedBlock = [&](const OcrBlock &b, const QString &vi) -> TranslatedBlock {
        QString fg = QStringLiteral("#F4F6FA");
        QString bg = QStringLiteral("#0B0D14");
        sampleOcrBlockColors(job->crop, b.rect, fg, bg);
        return TranslatedBlock{
            b.text,
            WuWaDialogueDb::stripRichTextTags(vi),
            mapBlockRectToFrame(b.rect, job->sourceRect),
            fg,
            bg
        };
    };

    // Look every OCR'd block up in the WuWa chunk DB first (exact + template + fuzzy)
    QHash<int, QString> dbHits;
    QList<int> missIndices;
    QStringList missTexts;
    for (int i = 0; i < ocrBlocks.size(); ++i) {
        QString vi;
        if (m_chunkEnabled && m_chunkDb
            && (m_chunkDb->lookup(ocrBlocks.at(i).text, vi)
                || m_chunkDb->lookupFuzzy(ocrBlocks.at(i).text, vi))) {
            dbHits.insert(i, vi);
        } else {
            missIndices.append(i);
            missTexts.append(ocrBlocks.at(i).text);
        }
    }

    QElapsedTimer translateTimer;
    translateTimer.start();
    if (!ocrBlocks.isEmpty() && missIndices.isEmpty()) {
        result.ok = true;
        result.translateMs = 0;
        result.ocrMs = ocrMs;
        result.chunkHit = true;
        result.cacheHit = true;
        QStringList allTranslated;
        for (int i = 0; i < ocrBlocks.size(); ++i) {
            const QString vi = dbHits.value(i);
            if (vi.isEmpty())
                continue;
            allTranslated.append(vi);
            result.blocks.append(makeTranslatedBlock(ocrBlocks.at(i), vi));
        }
        result.translated = allTranslated.join(QChar(u'\n'));
    } else {
        auto provider = createTranslationProvider(job->providerId, job->geminiApiKey, job->customApi);
        if (!provider) {
            result.error = QStringLiteral("Unknown translation provider");
            result.ocrMs = ocrMs;
            m_queue.finishRunning();
            m_processing = false;
            emit jobCompleted(result);
            QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
            return;
        }
        const QString providerInput = ocrBlocks.isEmpty()
            ? original
            : missTexts.join(QChar(u'\n'));
        const TranslateResult translated = provider->translateStreaming(
            providerInput, job->sourceLanguage, job->targetLanguage,
            [&](const QString &partial) {
                if (dbHits.isEmpty())
                    emit jobProgress(original, partial, job->sourceRect, job->testJob);
            });
        result.translateMs = translateTimer.elapsed();
        result.ocrMs = ocrMs;
        if (!translated.ok) {
            // Even if online provider failed for missed lines, still emit any
            // blocks that ALREADY hit the local WuWa chunk DB!
            if (!dbHits.isEmpty()) {
                result.ok = true;
                result.chunkHit = true;
                QStringList partialHits;
                for (int i = 0; i < ocrBlocks.size(); ++i) {
                    if (dbHits.contains(i)) {
                        partialHits.append(dbHits.value(i));
                        result.blocks.append(makeTranslatedBlock(ocrBlocks.at(i), dbHits.value(i)));
                    }
                }
                result.translated = partialHits.join(QChar(u'\n'));
            } else {
                result.error = translated.error;
            }
        } else if (ocrBlocks.isEmpty()) {
            result.ok = true;
            result.translated = WuWaDialogueDb::stripRichTextTags(translated.text);
            result.blocks.append(TranslatedBlock{
                original, result.translated,
                mapBlockRectToFrame(QRectF(0, 0, 1, 1), job->sourceRect)});
        } else {
            QStringList lines = translated.text.split(QChar(u'\n'));
            while (!lines.isEmpty() && lines.last().isEmpty())
                lines.removeLast();
            for (QString &line : lines)
                line = line.trimmed();
            if (lines.size() == missIndices.size()) {
                result.ok = true;
                QStringList allTranslated;
                int missPos = 0;
                for (int i = 0; i < ocrBlocks.size(); ++i) {
                    if (dbHits.contains(i)) {
                        allTranslated.append(dbHits.value(i));
                        result.blocks.append(makeTranslatedBlock(ocrBlocks.at(i), dbHits.value(i)));
                    } else {
                        const QString line = lines.at(missPos++);
                        if (line.isEmpty())
                            continue;
                        allTranslated.append(line);
                        result.blocks.append(makeTranslatedBlock(ocrBlocks.at(i), line));
                    }
                }
                result.translated = allTranslated.join(QChar(u'\n'));
                if (result.blocks.isEmpty()) {
                    result.original.clear();
                    result.translated.clear();
                }
            } else {
                QStringList perBlock;
                QString lastError;
                for (int i = 0; i < ocrBlocks.size(); ++i) {
                    if (dbHits.contains(i)) {
                        perBlock.append(dbHits.value(i));
                        result.blocks.append(makeTranslatedBlock(ocrBlocks.at(i), dbHits.value(i)));
                        continue;
                    }
                    const OcrBlock &block = ocrBlocks.at(i);
                    const TranslateResult one =
                        provider->translate(block.text, job->sourceLanguage, job->targetLanguage);
                    if (!one.ok) {
                        lastError = one.error;
                        continue;
                    }
                    const QString oneText = one.text.trimmed();
                    if (oneText.isEmpty())
                        continue;
                    perBlock.append(oneText);
                    result.blocks.append(makeTranslatedBlock(block, oneText));
                }
                if (result.blocks.isEmpty()) {
                    result.error = lastError.isEmpty() ? QStringLiteral("Translation failed")
                                                       : lastError;
                } else {
                    result.ok = true;
                    result.translated = perBlock.join(QChar(u'\n'));
                }
            }
        }
    }

    m_queue.finishRunning();
    m_processing = false;
    emit jobCompleted(result);
    if (!m_abortRequested.load(std::memory_order_acquire))
        QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
}
