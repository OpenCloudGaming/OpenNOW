#include "translation/TranslationEngine.h"

#include <QCryptographicHash>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QMetaObject>
#include <QSettings>
#include <QStandardPaths>

namespace {
constexpr int KeepPreviousMs = 2000;

QRectF clampNormalized(const QRectF &rect)
{
    QRectF clamped(qBound(0.0, rect.x(), 1.0), qBound(0.0, rect.y(), 1.0),
                   qBound(0.0, rect.width(), 1.0), qBound(0.0, rect.height(), 1.0));
    if (clamped.right() > 1.0)
        clamped.setWidth(1.0 - clamped.x());
    if (clamped.bottom() > 1.0)
        clamped.setHeight(1.0 - clamped.y());
    return clamped;
}

// Where the dialogue DB may live, in priority order. The shipped seed is
// src/translation/data/wuwa-dialogue-db.json; a deployed copy next to the
// executable or in AppData wins when present.
QString resolveDialogueDbPath()
{
    const QString appData = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
        + QStringLiteral("/wuwa-dialogue-db.json");
    if (QFile::exists(appData))
        return appData;
    const QString nextToExe = QCoreApplication::applicationDirPath()
        + QStringLiteral("/wuwa-dialogue-db.json");
    if (QFile::exists(nextToExe))
        return nextToExe;
    const QString inDataDir = QCoreApplication::applicationDirPath()
        + QStringLiteral("/data/wuwa-dialogue-db.json");
    if (QFile::exists(inDataDir))
        return inDataDir;
    // Bundled seed (Resources.cmake registers it under :/translation/).
    // Lowest priority: a user DB next to the exe or in AppData wins.
    if (QFile::exists(QStringLiteral(":/translation/wuwa-dialogue-db.json")))
        return QStringLiteral(":/translation/wuwa-dialogue-db.json");
    return appData; // default target: loadDialogueDb() tolerates a missing file
}
} // namespace

TranslationEngine::TranslationEngine(QObject *parent)
    : QObject(parent)
    , m_worker(new TranslationWorker(nullptr)) // no parent: it moves to its own thread
{
    m_cache.load();
    m_dialogueDb.loadDialogueDb(resolveDialogueDbPath());

    m_stabilityTimer.setSingleShot(true);
    connect(&m_stabilityTimer, &QTimer::timeout, this, &TranslationEngine::onStabilityTimeout);
    m_keepPreviousTimer.setSingleShot(true);
    connect(&m_keepPreviousTimer, &QTimer::timeout, this, &TranslationEngine::onKeepPreviousExpired);

    connect(m_worker, &TranslationWorker::jobCompleted,
            this, &TranslationEngine::onWorkerJobCompleted);
    m_worker->start();
    // Gemini key, persisted locally only (QSettings, never in a file we
    // commit, never logged). Restored here so hasGeminiApiKey() is true
    // after a restart without re-entering the key.
    {
        const QSettings settings;
        m_geminiApiKey = settings.value(QStringLiteral("translation/geminiApiKey")).toString();
        if (!m_geminiApiKey.isEmpty())
            QMetaObject::invokeMethod(m_worker, "setGeminiApiKey", Qt::QueuedConnection,
                                      Q_ARG(QString, m_geminiApiKey));
    }
    setStatus(QStringLiteral("Translation ready"));
}

TranslationEngine::~TranslationEngine()
{
    m_worker->stop();
    delete m_worker;
    m_cache.save();
}

QImage TranslationEngine::cropDialogueImage(const QImage &image, const QRectF &normalizedRect)
{
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return QImage();
    const QRectF rect = clampNormalized(normalizedRect);
    if (rect.width() <= 0.0 || rect.height() <= 0.0)
        return QImage();
    const int x = static_cast<int>(rect.x() * image.width());
    const int y = static_cast<int>(rect.y() * image.height());
    const int w = static_cast<int>(rect.width() * image.width());
    const int h = static_cast<int>(rect.height() * image.height());
    const QRect pixelRect = QRect(x, y, w, h).intersected(image.rect());
    if (pixelRect.isEmpty())
        return QImage();
    return image.copy(pixelRect); // detached copy: safe to hand across threads
}

QString TranslationEngine::cropHash(const QImage &image)
{
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return QString();
    const QImage argb = image.convertedTo(QImage::Format_ARGB32);
    const QByteArrayView pixels(reinterpret_cast<const char *>(argb.constBits()),
                                static_cast<qsizetype>(argb.sizeInBytes()));
    return QString::fromLatin1(
        QCryptographicHash::hash(pixels, QCryptographicHash::Sha256).toHex());
}

void TranslationEngine::setEnabled(bool enabled)
{
    if (m_enabled == enabled)
        return;
    m_enabled = enabled;
    if (!m_enabled) {
        m_stabilityTimer.stop();
        m_pendingHash.clear();
        m_pendingCrop = QImage();
        m_worker->cancelPending();
    }
    setStatus(m_enabled ? QStringLiteral("Translation enabled")
                        : QStringLiteral("Translation disabled"));
    emit enabledChanged();
}

void TranslationEngine::setSourceLanguage(const QString &language)
{
    if (language.isEmpty() || m_sourceLanguage == language)
        return;
    m_sourceLanguage = language;
    emit sourceLanguageChanged();
}

void TranslationEngine::setTargetLanguage(const QString &language)
{
    if (language.isEmpty() || m_targetLanguage == language)
        return;
    m_targetLanguage = language;
    emit targetLanguageChanged();
}

void TranslationEngine::setOcrEngine(const QString &engine)
{
    static const QStringList valid = {QStringLiteral("gemini-vision"), QStringLiteral("tesseract"),
                                      QStringLiteral("rapidocr"), QStringLiteral("windows-ocr")};
    if (!valid.contains(engine) || m_ocrEngine == engine)
        return;
    m_ocrEngine = engine;
    emit ocrEngineChanged();
}

void TranslationEngine::setTranslationProvider(const QString &provider)
{
    static const QStringList valid = {QStringLiteral("google"), QStringLiteral("gemini")};
    if (!valid.contains(provider) || m_translationProvider == provider)
        return;
    m_translationProvider = provider;
    emit translationProviderChanged();
}

void TranslationEngine::setOverlayMode(const QString &mode)
{
    static const QStringList valid = {QStringLiteral("inplace"), QStringLiteral("subtitle"),
                                      QStringLiteral("off")};
    if (!valid.contains(mode) || m_overlayMode == mode)
        return;
    m_overlayMode = mode;
    emit overlayModeChanged();
}

void TranslationEngine::setStabilityDelayMs(int ms)
{
    ms = qBound(0, ms, 5000);
    if (m_stabilityDelayMs == ms)
        return;
    m_stabilityDelayMs = ms;
    emit stabilityDelayMsChanged();
}

void TranslationEngine::setFontSizePx(int px)
{
    px = qBound(10, px, 64);
    if (m_fontSizePx == px)
        return;
    m_fontSizePx = px;
    emit fontSizePxChanged();
}

void TranslationEngine::setShowOriginal(bool show)
{
    if (m_showOriginal == show)
        return;
    m_showOriginal = show;
    emit showOriginalChanged();
}

void TranslationEngine::setCacheEnabled(bool enabled)
{
    if (m_cacheEnabled == enabled)
        return;
    m_cacheEnabled = enabled;
    emit cacheEnabledChanged();
}

void TranslationEngine::setDialogueRect(const QRectF &rect)
{
    const QRectF clamped = clampNormalized(rect);
    if (m_dialogueRect == clamped)
        return;
    m_dialogueRect = clamped;
    emit dialogueRectChanged();
}

void TranslationEngine::setDialoguePreset(const QString &preset)
{
    if (preset == QStringLiteral("wuthering-waves")) {
        setDialogueRect(QRectF(0.05, 0.72, 0.90, 0.24));
    } else if (preset == QStringLiteral("generic")) {
        setDialogueRect(QRectF(0.05, 0.60, 0.90, 0.30));
    } else {
        setError(QStringLiteral("Unknown dialogue preset"));
    }
}

void TranslationEngine::setGeminiApiKey(const QString &key)
{
    if (m_geminiApiKey == key)
        return;
    m_geminiApiKey = key;
    // Persist locally only (QSettings: registry on Windows, ini elsewhere).
    // Never written to the repo, never logged, never sent anywhere except
    // to Google in the x-goog-api-key header.
    QSettings settings;
    if (key.isEmpty())
        settings.remove(QStringLiteral("translation/geminiApiKey"));
    else
        settings.setValue(QStringLiteral("translation/geminiApiKey"), key);
    QMetaObject::invokeMethod(m_worker, "setGeminiApiKey", Qt::QueuedConnection,
                              Q_ARG(QString, key));
}

bool TranslationEngine::hasGeminiApiKey() const
{
    return !m_geminiApiKey.isEmpty();
}

void TranslationEngine::clearCache()
{
    m_cache.clear();
    m_cache.save();
    setStatus(QStringLiteral("Translation cache cleared"));
}

void TranslationEngine::onStreamFrame(const QImage &image)
{
    if (!m_enabled)
        return;
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return;
    const QImage crop = cropDialogueImage(image, m_dialogueRect);
    if (crop.isNull())
        return;
    const QString hash = cropHash(crop);
    if (hash.isEmpty())
        return;
    if (hash == m_lastProcessedHash)
        return; // already translated this exact crop
    if (hash != m_pendingHash) {
        // New content: (re)start the stability wait.
        m_pendingHash = hash;
        m_pendingCrop = crop;
        m_pendingRect = m_dialogueRect;
        m_stabilityTimer.start(m_stabilityDelayMs);
    }
    // Same hash as pending: the running timer already measures stability;
    // do not restart it.
}

void TranslationEngine::onStabilityTimeout()
{
    if (!m_enabled || m_pendingHash.isEmpty() || m_pendingCrop.isNull())
        return;
    if (m_pendingHash == m_lastProcessedHash)
        return;
    m_lastProcessedHash = m_pendingHash;
    const QImage crop = m_pendingCrop;
    const QRectF rect = m_pendingRect;
    m_pendingHash.clear();
    m_pendingCrop = QImage();

    if (m_overlayMode == QStringLiteral("off"))
        return;

    // Cache first, keyed by crop hash: an identical dialogue line seen
    // before emits immediately with no OCR and no network at all.
    // (The text-keyed cache/DB lookup happens for direct-text jobs in
    // requestTestTranslation, and every worker result is inserted into
    // both caches, so the two stay in sync.)
    if (m_cacheEnabled) {
        const auto hit = m_hashCache.constFind(m_lastProcessedHash);
        if (hit != m_hashCache.constEnd()) {
            emitTranslation(hit->first, hit->second, rect, 0, 0, true);
            return;
        }
    }

    TranslationJob job;
    job.crop = crop;
    job.cropHash = m_lastProcessedHash;
    job.sourceRect = rect;
    job.ocrEngineId = m_ocrEngine;
    job.providerId = m_translationProvider;
    job.sourceLanguage = m_sourceLanguage;
    job.targetLanguage = m_targetLanguage;
    job.geminiApiKey = m_geminiApiKey;
    setStatus(QStringLiteral("Reading dialogue…"));
    QMetaObject::invokeMethod(m_worker, "submitJob", Qt::QueuedConnection,
                              Q_ARG(TranslationJob, job));
}

void TranslationEngine::requestTestTranslation(const QString &englishText)
{
    const QString text = englishText.trimmed();
    if (text.isEmpty()) {
        setError(QStringLiteral("Nothing to translate"));
        return;
    }
    if (m_cacheEnabled) {
        QString cached;
        if (m_dialogueDb.lookup(text, cached) || m_cache.lookup(text, cached)) {
            m_lastCacheHit = true;
            m_lastOcrMs = 0;
            m_lastTranslateMs = 0;
            emitTranslation(text, cached, m_dialogueRect, 0, 0, true);
            return;
        }
    }
    TranslationJob job;
    job.directText = text;
    job.sourceRect = m_dialogueRect;
    job.providerId = m_translationProvider;
    job.sourceLanguage = m_sourceLanguage;
    job.targetLanguage = m_targetLanguage;
    job.geminiApiKey = m_geminiApiKey;
    setStatus(QStringLiteral("Translating…"));
    QMetaObject::invokeMethod(m_worker, "submitJob", Qt::QueuedConnection,
                              Q_ARG(TranslationJob, job));
}

void TranslationEngine::onWorkerJobCompleted(const TranslationJobResult &result)
{
    if (!result.ok) {
        setError(result.error.isEmpty() ? QStringLiteral("Translation failed") : result.error);
        // Keep the previous translation visible for 2s, then clear it.
        // Never emit garbage text.
        if (m_hasTranslation)
            m_keepPreviousTimer.start(KeepPreviousMs);
        return;
    }
    if (result.original.isEmpty() && result.translated.isEmpty()) {
        // OCR found no dialogue text in the crop: nothing to show, and
        // not an error. Leave the current translation alone.
        return;
    }
    m_keepPreviousTimer.stop();
    if (m_cacheEnabled && !result.translated.isEmpty()) {
        m_cache.insert(result.original, result.translated);
        // Remember by crop hash too, so the exact same pixels next time
        // hit the cache before any OCR/network (see onStabilityTimeout).
        if (!result.cropHash.isEmpty()) {
            if (!m_hashCache.contains(result.cropHash))
                m_hashOrder.append(result.cropHash);
            m_hashCache.insert(result.cropHash, {result.original, result.translated});
            while (m_hashCache.size() > MaxHashCacheEntries && !m_hashOrder.isEmpty())
                m_hashCache.remove(m_hashOrder.takeFirst());
        }
    }
    m_lastCacheHit = false;
    emitTranslation(result.original, result.translated, result.sourceRect,
                    result.ocrMs, result.translateMs, false);
}

void TranslationEngine::onKeepPreviousExpired()
{
    clearTranslation();
}

void TranslationEngine::emitTranslation(const QString &original, const QString &translated,
                                        const QRectF &sourceRect, qint64 ocrMs, qint64 translateMs,
                                        bool cacheHit)
{
    m_lastOriginalText = original;
    m_lastTranslatedText = translated;
    m_lastOcrMs = ocrMs;
    m_lastTranslateMs = translateMs;
    m_lastCacheHit = cacheHit;
    m_lastError.clear();
    if (!m_hasTranslation) {
        m_hasTranslation = true;
        emit hasTranslationChanged();
    }
    setStatus(QStringLiteral("Translation ready"));
    emit translationTextChanged();
    emit translationReady(original, translated, sourceRect);
}

void TranslationEngine::clearTranslation()
{
    if (!m_hasTranslation && m_lastTranslatedText.isEmpty())
        return;
    m_lastTranslatedText.clear();
    m_lastOriginalText.clear();
    m_hasTranslation = false;
    emit hasTranslationChanged();
    emit translationTextChanged();
    emit translationCleared();
}

void TranslationEngine::setStatus(const QString &status)
{
    if (m_statusText == status)
        return;
    m_statusText = status;
    emit statusChanged();
}

void TranslationEngine::setError(const QString &error)
{
    m_lastError = error;
    setStatus(error);
}
