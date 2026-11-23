#include "translation/TranslationEngine.h"
#include "translation/OcrEngine.h"

#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QFile>
#include <QFontDatabase>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocale>
#include <QMetaObject>
#include <QSettings>
#include <QStandardPaths>
#include <QVariant>
#include <algorithm>

namespace {
constexpr int KeepPreviousMs = 2000;
// Local OCR (Windows Native WinRT OCR / Tesseract) runs in ~15-30ms, so we
// can throttle full-screen scans at 320ms for near-instant responsiveness.
// Cloud Vision OCR throttles at 1500ms to protect API rate limits.
constexpr int LocalFullScreenIntervalMs = 320;
constexpr int CloudFullScreenIntervalMs = 1500;
constexpr int FramePumpIntervalMs = 200;

bool isAutomatedTestEnvironment()
{
    if (qEnvironmentVariable("QT_QPA_PLATFORM") == QStringLiteral("offscreen"))
        return true;
    const QStringList args = QCoreApplication::arguments();
    return args.contains(QStringLiteral("--smoke-test"))
        || args.contains(QStringLiteral("--performance-report"));
}

QString chunkCacheBase()
{
    return QStandardPaths::writableLocation(QStandardPaths::CacheLocation)
        + QStringLiteral("/wuwa-chunks");
}

struct ChunkGameCatalogEntry
{
    QString id;
    QString title;
    QString version;
    QString repoUrl;
    QString manifestUrl;
    int words = 0;
    int strings = 0;
};

ChunkGameCatalogEntry chunkCatalogEntry(const QString &id)
{
    if (id == QStringLiteral("wuthering-waves"))
        return {id, QStringLiteral("Wuthering Waves"), QStringLiteral("3.8"),
                QStringLiteral("https://github.com/huyzoz/wuwa-vi-chunks"),
                QStringLiteral("https://raw.githubusercontent.com/huyzoz/wuwa-vi-chunks/main/manifest.json"),
                3206219, 220764};
    return {};
}

bool isFullScreenRect(const QRectF &rect)
{
    return rect.width() >= 0.99 && rect.height() >= 0.99;
}

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
    if (QFile::exists(QStringLiteral(":/translation/wuwa-dialogue-db.json")))
        return QStringLiteral(":/translation/wuwa-dialogue-db.json");
    return appData;
}

void saveTranslationSetting(const QString &key, const QVariant &value)
{
    QSettings settings;
    settings.setValue(QStringLiteral("translation/") + key, value);
}
} // namespace

TranslationEngine::TranslationEngine(QObject *parent)
    : QObject(parent)
    , m_worker(new TranslationWorker(nullptr))
{
    m_cache.load();
    m_dialogueDb.loadDialogueDb(resolveDialogueDbPath());

    m_stabilityTimer.setSingleShot(true);
    connect(&m_stabilityTimer, &QTimer::timeout, this, &TranslationEngine::onStabilityTimeout);
    m_keepPreviousTimer.setSingleShot(true);
    connect(&m_keepPreviousTimer, &QTimer::timeout, this, &TranslationEngine::onKeepPreviousExpired);
    m_frameTimer.setInterval(FramePumpIntervalMs);
    connect(&m_frameTimer, &QTimer::timeout, this, &TranslationEngine::onFrameTimerTimeout);

    connect(m_worker, &TranslationWorker::jobCompleted,
            this, &TranslationEngine::onWorkerJobCompleted);
    connect(m_worker, &TranslationWorker::jobProgress,
            this, &TranslationEngine::onWorkerJobProgress);
    connect(m_worker, &TranslationWorker::customApiModelsFetched,
            this, &TranslationEngine::onWorkerModelsFetched);
    connect(m_worker, &TranslationWorker::chunkDownloadProgress,
            this, &TranslationEngine::onChunkDownloadProgress);
    connect(m_worker, &TranslationWorker::chunkDownloadFinished,
            this, &TranslationEngine::onChunkDownloadFinished);
    m_worker->start();

    {
        const QSettings settings;
        m_geminiApiKey = settings.value(QStringLiteral("translation/geminiApiKey")).toString();
        if (!m_geminiApiKey.isEmpty())
            QMetaObject::invokeMethod(m_worker, "setGeminiApiKey", Qt::QueuedConnection,
                                      Q_ARG(QString, m_geminiApiKey));
        m_customApiBase = settings.value(QStringLiteral("translation/customApiBase")).toString();
        m_customApiKey = settings.value(QStringLiteral("translation/customApiKey")).toString();
        m_customApiModel = settings.value(QStringLiteral("translation/customApiModel")).toString();
    }

    {
        const QSettings settings;
        m_enabled = settings.value(QStringLiteral("translation/enabled"), false).toBool();
        m_sourceLanguage = settings.value(QStringLiteral("translation/sourceLanguage"),
                                          QStringLiteral("en")).toString();
        m_targetLanguage = settings.value(QStringLiteral("translation/targetLanguage"),
                                          QStringLiteral("vi")).toString();
        // Default to Windows Native WinRT OCR on Windows for instant ~15ms
        // offline recognition; fall back to gemini-vision when explicitly saved.
        const QString defaultOcr = QStringLiteral("windows-ocr");
        m_ocrEngine = settings.value(QStringLiteral("translation/ocrEngine"), defaultOcr).toString();
        m_translationProvider = settings.value(QStringLiteral("translation/translationProvider"),
                                               QStringLiteral("google")).toString();
        m_overlayMode = settings.value(QStringLiteral("translation/overlayMode"),
                                       QStringLiteral("inplace")).toString();
        m_stabilityDelayMs = settings.value(QStringLiteral("translation/stabilityDelayMs"), 220).toInt();
        m_fontSizePx = settings.value(QStringLiteral("translation/fontSizePx"), 22).toInt();
        m_showOriginal = settings.value(QStringLiteral("translation/showOriginal"), false).toBool();
        m_showTimingPill = settings.value(QStringLiteral("translation/showTimingPill"), false).toBool();
        m_cacheEnabled = settings.value(QStringLiteral("translation/cacheEnabled"), true).toBool();
        m_chunkEnabled = settings.value(QStringLiteral("translation/chunkEnabled"), true).toBool();
        m_chunkGame = settings.value(QStringLiteral("translation/chunkGame"),
                                     QStringLiteral("wuthering-waves")).toString();
        m_dialoguePreset = settings.value(QStringLiteral("translation/dialoguePreset"),
                                          QStringLiteral("full-screen")).toString();
        m_dialoguePreset = migrateDialoguePreset(m_dialoguePreset);
        saveTranslationSetting(QStringLiteral("dialoguePreset"), m_dialoguePreset);
        const QRectF presetRect = dialogueRectForPreset(m_dialoguePreset);
        if (!presetRect.isNull())
            m_dialogueRect = presetRect;
        else
            m_dialoguePreset = QStringLiteral("full-screen");

        if (m_sourceLanguage.isEmpty())
            m_sourceLanguage = QStringLiteral("en");
        if (m_targetLanguage.isEmpty())
            m_targetLanguage = QStringLiteral("vi");
        m_customApiBase = normalizeCustomApiBase(m_customApiBase);
        if (!QStringList{QStringLiteral("windows-ocr"), QStringLiteral("gemini-vision"),
                         QStringLiteral("tesseract"), QStringLiteral("rapidocr"),
                         QStringLiteral("custom-vision")}
                 .contains(m_ocrEngine))
            m_ocrEngine = QStringLiteral("windows-ocr");
        if (!QStringList{QStringLiteral("google"), QStringLiteral("gemini"),
                         QStringLiteral("custom")}
                 .contains(m_translationProvider))
            m_translationProvider = QStringLiteral("google");
        if (m_chunkGame != QStringLiteral("wuthering-waves"))
            m_chunkGame = QStringLiteral("wuthering-waves");
        if (!QStringList{QStringLiteral("inplace"), QStringLiteral("subtitle"), QStringLiteral("off")}
                 .contains(m_overlayMode))
            m_overlayMode = QStringLiteral("inplace");
        m_stabilityDelayMs = qBound(0, m_stabilityDelayMs, 5000);
        m_fontSizePx = qBound(10, m_fontSizePx, 64);
    }

    if (!m_customApiBase.isEmpty() || !m_customApiKey.isEmpty() || !m_customApiModel.isEmpty())
        syncCustomApiToWorker();

    for (const QString &font : {QStringLiteral(":/fonts/BeVietnamPro-Regular.ttf"),
                                QStringLiteral(":/fonts/BeVietnamPro-Medium.ttf"),
                                QStringLiteral(":/fonts/BeVietnamPro-SemiBold.ttf"),
                                QStringLiteral(":/fonts/BeVietnamPro-Bold.ttf")}) {
        if (QFontDatabase::addApplicationFont(font) < 0 && !isAutomatedTestEnvironment())
            qWarning() << "TranslationEngine: failed to load bundled font" << font;
    }

    applyChunkCatalog();
    QMetaObject::invokeMethod(m_worker, "setChunkDb", Qt::QueuedConnection,
                              Q_ARG(WuWaDialogueDb *, &m_chunkDb));
    QMetaObject::invokeMethod(m_worker, "setChunkEnabled", Qt::QueuedConnection,
                              Q_ARG(bool, m_chunkEnabled));
    loadCachedChunks();

    // Never auto-start network chunk downloads during offscreen unit/smoke tests
    if (!isAutomatedTestEnvironment() && m_enabled) {
        ensureChunksDownloaded();
        m_frameTimer.start();
    }
    setStatus(QStringLiteral("Translation ready"));
}

TranslationEngine::~TranslationEngine()
{
    m_worker->stop();
    delete m_worker;
    m_cache.save();
}

int TranslationEngine::effectiveFullScreenIntervalMs() const
{
    if (m_ocrEngine == QStringLiteral("windows-ocr") || m_ocrEngine == QStringLiteral("tesseract"))
        return LocalFullScreenIntervalMs;
    return CloudFullScreenIntervalMs;
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
    return image.copy(pixelRect);
}

QString TranslationEngine::cropHash(const QImage &image)
{
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return QString();

    const int w = image.width();
    const int h = image.height();
    // For small images (like unit-test fixtures <= 128x128), hash every pixel.
    // For full 1080p frames, sample a 3x2 grid with 64-bit FNV-1a (< 0.08ms on GUI thread).
    const bool smallImage = (w * h) <= (128 * 128);
    const int stepX = smallImage ? 1 : 3;
    const int stepY = smallImage ? 1 : 2;

    quint64 hash1 = 14695981039346656037ULL;
    quint64 hash2 = 1099511628211ULL ^ static_cast<quint64>((w << 16) ^ h);

    if (image.depth() == 32) {
        for (int y = 0; y < h; y += stepY) {
            const auto *line = reinterpret_cast<const QRgb *>(image.constScanLine(y));
            for (int x = 0; x < w; x += stepX) {
                const quint32 rgb = static_cast<quint32>(line[x]) & 0x00FFFFFFu;
                hash1 ^= rgb;
                hash1 *= 1099511628211ULL;
                hash2 += (rgb ^ (hash1 >> 17));
                hash2 *= 14695981039346656037ULL;
            }
        }
    } else {
        for (int y = 0; y < h; y += stepY) {
            for (int x = 0; x < w; x += stepX) {
                const quint32 rgb = static_cast<quint32>(image.pixel(x, y)) & 0x00FFFFFFu;
                hash1 ^= rgb;
                hash1 *= 1099511628211ULL;
                hash2 += (rgb ^ (hash1 >> 17));
                hash2 *= 14695981039346656037ULL;
            }
        }
    }

    return QStringLiteral("%1%2")
        .arg(hash1, 16, 16, QChar(u'0'))
        .arg(hash2, 16, 16, QChar(u'0'));
}

void TranslationEngine::setEnabled(bool enabled)
{
    if (m_enabled == enabled)
        return;
    m_enabled = enabled;
    saveTranslationSetting(QStringLiteral("enabled"), enabled);
    if (!m_enabled) {
        m_stabilityTimer.stop();
        m_frameTimer.stop();
        m_pendingHash.clear();
        m_pendingCrop = QImage();
        m_worker->cancelPending();
    } else {
        m_frameTimer.start();
        if (!isAutomatedTestEnvironment())
            ensureChunksDownloaded();
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
    saveTranslationSetting(QStringLiteral("sourceLanguage"), language);
    emit sourceLanguageChanged();
}

void TranslationEngine::setTargetLanguage(const QString &language)
{
    if (language.isEmpty() || m_targetLanguage == language)
        return;
    m_targetLanguage = language;
    saveTranslationSetting(QStringLiteral("targetLanguage"), language);
    emit targetLanguageChanged();
}

void TranslationEngine::setOcrEngine(const QString &engine)
{
    static const QStringList valid = {QStringLiteral("windows-ocr"), QStringLiteral("gemini-vision"),
                                      QStringLiteral("tesseract"), QStringLiteral("rapidocr"),
                                      QStringLiteral("custom-vision")};
    if (!valid.contains(engine) || m_ocrEngine == engine)
        return;
    m_ocrEngine = engine;
    saveTranslationSetting(QStringLiteral("ocrEngine"), engine);
    emit ocrEngineChanged();
    emit visionKeyChanged();
}

void TranslationEngine::setTranslationProvider(const QString &provider)
{
    static const QStringList valid = {QStringLiteral("google"), QStringLiteral("gemini"),
                                      QStringLiteral("custom")};
    if (!valid.contains(provider) || m_translationProvider == provider)
        return;
    m_translationProvider = provider;
    saveTranslationSetting(QStringLiteral("translationProvider"), provider);
    emit translationProviderChanged();
}

CustomApiConfig TranslationEngine::customApiConfig() const
{
    return CustomApiConfig{m_customApiBase, m_customApiKey, m_customApiModel};
}

void TranslationEngine::syncCustomApiToWorker()
{
    QMetaObject::invokeMethod(m_worker, "setCustomApiConfig", Qt::QueuedConnection,
                              Q_ARG(CustomApiConfig, customApiConfig()));
}

void TranslationEngine::setCustomApiBase(const QString &base)
{
    const QString normalized = normalizeCustomApiBase(base);
    if (m_customApiBase == normalized)
        return;
    m_customApiBase = normalized;
    saveTranslationSetting(QStringLiteral("customApiBase"), normalized);
    syncCustomApiToWorker();
    emit customApiBaseChanged();
}

void TranslationEngine::setCustomApiModel(const QString &model)
{
    const QString trimmed = model.trimmed();
    if (m_customApiModel == trimmed)
        return;
    m_customApiModel = trimmed;
    saveTranslationSetting(QStringLiteral("customApiModel"), trimmed);
    syncCustomApiToWorker();
    emit customApiModelChanged();
}

void TranslationEngine::setCustomApiKey(const QString &key)
{
    if (m_customApiKey == key)
        return;
    m_customApiKey = key;
    QSettings settings;
    if (key.isEmpty())
        settings.remove(QStringLiteral("translation/customApiKey"));
    else
        settings.setValue(QStringLiteral("translation/customApiKey"), key);
    syncCustomApiToWorker();
    emit visionKeyChanged();
}

bool TranslationEngine::hasCustomApiKey() const
{
    return !m_customApiKey.isEmpty();
}

bool TranslationEngine::visionKeyConfigured() const
{
    return hasGeminiApiKey() || hasCustomApiKey();
}

bool TranslationEngine::ocrReady() const
{
    if (visionKeyConfigured())
        return true;
    WindowsOcrEngine winOcr;
    if (winOcr.available())
        return true;
    TesseractOcrEngine tessOcr;
    return tessOcr.available();
}

void TranslationEngine::fetchCustomApiModels()
{
    if (m_customApiBase.isEmpty()) {
        setError(QStringLiteral("Custom API base URL is empty"));
        return;
    }
    if (m_customApiKey.isEmpty()) {
        setError(QStringLiteral("Custom API key is empty"));
        return;
    }
    setStatus(QStringLiteral("Fetching models…"));
    QMetaObject::invokeMethod(m_worker, "fetchCustomApiModels", Qt::QueuedConnection,
                              Q_ARG(QString, m_customApiBase), Q_ARG(QString, m_customApiKey));
}

void TranslationEngine::setOverlayMode(const QString &mode)
{
    static const QStringList valid = {QStringLiteral("inplace"), QStringLiteral("subtitle"),
                                      QStringLiteral("off")};
    if (!valid.contains(mode) || m_overlayMode == mode)
        return;
    m_overlayMode = mode;
    saveTranslationSetting(QStringLiteral("overlayMode"), mode);
    emit overlayModeChanged();
}

void TranslationEngine::setStabilityDelayMs(int ms)
{
    ms = qBound(0, ms, 5000);
    if (m_stabilityDelayMs == ms)
        return;
    m_stabilityDelayMs = ms;
    saveTranslationSetting(QStringLiteral("stabilityDelayMs"), ms);
    emit stabilityDelayMsChanged();
}

void TranslationEngine::setFontSizePx(int px)
{
    px = qBound(10, px, 64);
    if (m_fontSizePx == px)
        return;
    m_fontSizePx = px;
    saveTranslationSetting(QStringLiteral("fontSizePx"), px);
    emit fontSizePxChanged();
}

void TranslationEngine::setShowOriginal(bool show)
{
    if (m_showOriginal == show)
        return;
    m_showOriginal = show;
    saveTranslationSetting(QStringLiteral("showOriginal"), show);
    emit showOriginalChanged();
}

void TranslationEngine::setShowTimingPill(bool show)
{
    if (m_showTimingPill == show)
        return;
    m_showTimingPill = show;
    saveTranslationSetting(QStringLiteral("showTimingPill"), show);
    emit showTimingPillChanged();
}

void TranslationEngine::setCacheEnabled(bool enabled)
{
    if (m_cacheEnabled == enabled)
        return;
    m_cacheEnabled = enabled;
    saveTranslationSetting(QStringLiteral("cacheEnabled"), enabled);
    emit cacheEnabledChanged();
}

QStringList TranslationEngine::chunkGames() const
{
    return {QStringLiteral("wuthering-waves")};
}

void TranslationEngine::applyChunkCatalog()
{
    const ChunkGameCatalogEntry entry = chunkCatalogEntry(m_chunkGame);
    m_chunkGameTitle = entry.title;
    m_chunkVersion = entry.version;
    m_chunkRepoUrl = entry.repoUrl;
    const QLocale vi(QStringLiteral("vi_VN"));
    m_chunkGameWordsText = vi.toString(entry.words) + QStringLiteral(" từ");
    m_chunkGameStringsText = vi.toString(entry.strings) + QStringLiteral(" câu");
    emit chunkGameInfoChanged();
}

QString TranslationEngine::chunkGameCacheBase() const
{
    return chunkCacheBase() + QChar(u'/') + m_chunkGame;
}

void TranslationEngine::setChunkEnabled(bool enabled)
{
    if (m_chunkEnabled == enabled)
        return;
    m_chunkEnabled = enabled;
    saveTranslationSetting(QStringLiteral("chunkEnabled"), enabled);
    QMetaObject::invokeMethod(m_worker, "setChunkEnabled", Qt::QueuedConnection,
                              Q_ARG(bool, enabled));
    if (enabled) {
        if (m_chunkDb.size() == 0)
            loadCachedChunks();
        if (!isAutomatedTestEnvironment())
            ensureChunksDownloaded();
    } else {
        setChunkStatus(QStringLiteral("Chunk mode off"));
    }
    emit chunkEnabledChanged();
}

void TranslationEngine::setChunkGame(const QString &game)
{
    if (game != QStringLiteral("wuthering-waves") || m_chunkGame == game)
        return;
    m_chunkGame = game;
    saveTranslationSetting(QStringLiteral("chunkGame"), game);
    applyChunkCatalog();
    if (m_chunkEnabled)
        loadCachedChunks();
    emit chunkGameChanged();
}

void TranslationEngine::downloadChunks()
{
    if (m_chunkDownloading)
        return;
    m_chunkDownloading = true;
    const ChunkGameCatalogEntry entry = chunkCatalogEntry(m_chunkGame);
    if (entry.manifestUrl.isEmpty()) {
        m_chunkDownloading = false;
        setError(QStringLiteral("No chunks for this game"));
        return;
    }
    m_chunkProgress = 0.0;
    emit chunkProgressChanged();
    setChunkStatus(QStringLiteral("Downloading chunks…"));
    QMetaObject::invokeMethod(m_worker, "startChunkDownload", Qt::QueuedConnection,
                              Q_ARG(QString, entry.manifestUrl),
                              Q_ARG(QString, chunkGameCacheBase()));
}

void TranslationEngine::setChunkStatus(const QString &status)
{
    if (m_chunkStatus == status)
        return;
    m_chunkStatus = status;
    emit chunkStatusChanged();
}

void TranslationEngine::ensureChunksDownloaded()
{
    if (isAutomatedTestEnvironment() || !m_chunkEnabled || chunkDownloaded() || m_chunkDownloading)
        return;
    downloadChunks();
}

bool TranslationEngine::isChunkDownloadComplete(int dbSize, int expectedStrings)
{
    return expectedStrings > 0 && dbSize >= expectedStrings;
}

void TranslationEngine::loadCachedChunks()
{
    const QString base = chunkGameCacheBase();
    m_chunkExpectedStrings = 0;
    QDir dir(base);
    const QStringList versions = dir.entryList(QDir::Dirs | QDir::NoDotAndDotDot, QDir::Name);
    if (versions.isEmpty()) {
        setChunkStatus(QStringLiteral("Chunk not downloaded — press Download chunks"));
        return;
    }
    const QString version = versions.last();
    const QString versionDir = base + QChar(u'/') + version;
    QStringList files;
    QFile manifestFile(versionDir + QStringLiteral("/manifest.json"));
    if (manifestFile.open(QIODevice::ReadOnly)) {
        const QJsonDocument doc = QJsonDocument::fromJson(manifestFile.readAll());
        manifestFile.close();
        if (doc.isObject()) {
            const QJsonObject manifest = doc.object();
            const int translated = manifest.value(QStringLiteral("translatedStrings")).toInt();
            const int total = manifest.value(QStringLiteral("uniqueStrings")).toInt();
            if (translated > 0)
                m_chunkExpectedStrings = translated;
            const QLocale vi(QStringLiteral("vi_VN"));
            if (translated > 0 && total > 0)
                m_chunkTranslatedText = QStringLiteral("Đã dịch %1 / %2 câu")
                    .arg(vi.toString(translated), vi.toString(total));
            for (const QJsonValue &value : manifest.value(QStringLiteral("files")).toArray()) {
                const QString file = value.toString();
                if (!file.isEmpty())
                    files.append(file);
            }
        }
    }
    if (files.isEmpty()) {
        QDir chunksDir(versionDir + QStringLiteral("/chunks"));
        for (const QString &name : chunksDir.entryList({QStringLiteral("*.json")},
                                                       QDir::Files, QDir::Name))
            files.append(QStringLiteral("chunks/") + name);
    }

    const QString binCachePath = versionDir + QStringLiteral("/wuwa-chunks-v2.bin");
    bool loadedFromBin = false;
    if (QFile::exists(binCachePath)) {
        loadedFromBin = m_chunkDb.loadBinaryCache(binCachePath);
        if (loadedFromBin && m_chunkExpectedStrings > 0 && m_chunkDb.size() < m_chunkExpectedStrings) {
            loadedFromBin = false;
        }
    }
    if (!loadedFromBin) {
        m_chunkDb.clear();
        for (const QString &file : files)
            m_chunkDb.loadChunkFile(versionDir + QChar(u'/') + file, false);
        m_chunkDb.rebuildIndex();
        if (m_chunkDb.size() > 0 && isChunkDownloadComplete(m_chunkDb.size(), m_chunkExpectedStrings))
            m_chunkDb.saveBinaryCache(binCachePath);
    }

    m_chunkVersion = version;
    const QLocale vi(QStringLiteral("vi_VN"));
    if (m_chunkDb.size() > 0)
        setChunkStatus(QStringLiteral("Downloaded %1 lines").arg(vi.toString(m_chunkDb.size())));
    else
        setChunkStatus(QStringLiteral("Chunk not downloaded — press Download chunks"));
    emit chunkGameInfoChanged();
    emit chunkDbSizeChanged();
}

void TranslationEngine::onChunkDownloadProgress(int done, int total)
{
    m_chunkProgress = total > 0 ? static_cast<double>(done) / total : 0.0;
    setChunkStatus(QStringLiteral("Downloading chunks… %1/%2").arg(done).arg(total));
    emit chunkProgressChanged();
}

void TranslationEngine::onChunkDownloadFinished(const QString &error)
{
    m_chunkDownloading = false;
    m_chunkProgress = error.isEmpty() ? 1.0 : 0.0;
    emit chunkProgressChanged();
    if (!error.isEmpty()) {
        setError(error);
        return;
    }
    loadCachedChunks();
}

QVariantList TranslationEngine::applyStableRects(const QVariantList &blocks)
{
    QVariantList out = blocks;
    for (QVariant &entry : out) {
        QVariantMap map = entry.toMap();
        const QString key = TranslationCache::normalizeKey(
            map.value(QStringLiteral("original")).toString());
        if (key.isEmpty())
            continue;
        const QRectF rect(map.value(QStringLiteral("x")).toDouble(),
                          map.value(QStringLiteral("y")).toDouble(),
                          map.value(QStringLiteral("w")).toDouble(),
                          map.value(QStringLiteral("h")).toDouble());
        auto it = m_stableRects.find(key);
        if (it == m_stableRects.end()) {
            if (m_stableRects.size() >= MaxStableRects && !m_stableRectOrder.isEmpty())
                m_stableRects.remove(m_stableRectOrder.takeFirst());
            m_stableRects.insert(key, StableRect{rect, 1});
            m_stableRectOrder.append(key);
            continue;
        }
        const QRectF stored = it->rect;
        const bool close = qAbs(rect.x() - stored.x()) < 0.02
            && qAbs(rect.y() - stored.y()) < 0.02
            && qAbs(rect.width() - stored.width()) < 0.02
            && qAbs(rect.height() - stored.height()) < 0.02;
        if (close) {
            it->hits += 1;
            if (it->hits >= 2) {
                map.insert(QStringLiteral("x"), stored.x());
                map.insert(QStringLiteral("y"), stored.y());
                map.insert(QStringLiteral("w"), stored.width());
                map.insert(QStringLiteral("h"), stored.height());
                entry = map;
            }
        } else {
            it->rect = rect;
            it->hits = 1;
        }
    }
    return out;
}

void TranslationEngine::setDialogueRect(const QRectF &rect)
{
    const QRectF clamped = clampNormalized(rect);
    if (m_dialogueRect == clamped)
        return;
    m_dialogueRect = clamped;
    emit dialogueRectChanged();
}

QRectF TranslationEngine::dialogueRectForPreset(const QString &preset)
{
    if (preset == QStringLiteral("wuthering-waves"))
        return QRectF(0.05, 0.72, 0.90, 0.24);
    if (preset == QStringLiteral("generic"))
        return QRectF(0.05, 0.60, 0.90, 0.30);
    if (preset == QStringLiteral("full-screen"))
        return QRectF(0, 0, 1, 1);
    return QRectF();
}

QString TranslationEngine::migrateDialoguePreset(const QString &saved)
{
    if (saved == QStringLiteral("generic") || saved == QStringLiteral("full-screen"))
        return saved;
    return QStringLiteral("full-screen");
}

void TranslationEngine::setDialoguePreset(const QString &preset)
{
    const QRectF rect = dialogueRectForPreset(preset);
    if (rect.isNull()) {
        setError(QStringLiteral("Unknown dialogue preset"));
        return;
    }
    const bool changed = m_dialoguePreset != preset;
    m_dialoguePreset = preset;
    if (changed) {
        saveTranslationSetting(QStringLiteral("dialoguePreset"), preset);
        emit dialoguePresetChanged();
    }
    setDialogueRect(rect);
}

void TranslationEngine::setGeminiApiKey(const QString &key)
{
    if (m_geminiApiKey == key)
        return;
    m_geminiApiKey = key;
    QSettings settings;
    if (key.isEmpty())
        settings.remove(QStringLiteral("translation/geminiApiKey"));
    else
        settings.setValue(QStringLiteral("translation/geminiApiKey"), key);
    QMetaObject::invokeMethod(m_worker, "setGeminiApiKey", Qt::QueuedConnection,
                              Q_ARG(QString, key));
    emit visionKeyChanged();
}

bool TranslationEngine::hasGeminiApiKey() const
{
    return !m_geminiApiKey.isEmpty();
}

void TranslationEngine::clearCache()
{
    m_cache.clear();
    m_cache.save();
    m_hashCache.clear();
    m_hashOrder.clear();
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
        return;
    if (isFullScreenRect(m_dialogueRect)) {
        const int intervalMs = effectiveFullScreenIntervalMs();
        if (m_fullScreenThrottle.isValid() && m_fullScreenThrottle.elapsed() < intervalMs)
            return;
        m_fullScreenThrottle.restart();
        submitCrop(crop, hash, m_dialogueRect);
        return;
    }
    if (hash != m_pendingHash) {
        m_pendingHash = hash;
        m_pendingCrop = crop;
        m_pendingRect = m_dialogueRect;
        m_stabilityTimer.start(m_stabilityDelayMs);
    }
}

void TranslationEngine::submitCrop(const QImage &crop, const QString &hash, const QRectF &rect)
{
    m_lastProcessedHash = hash;
    if (m_overlayMode == QStringLiteral("off"))
        return;

    if (m_cacheEnabled) {
        const auto hit = m_hashCache.constFind(hash);
        if (hit != m_hashCache.constEnd()) {
            emitTranslation(hit->original, hit->translated, rect, 0, 0, true, hit->blocks);
            return;
        }
    }

    if (!ocrReady()) {
        setStatus(QStringLiteral("Vision key required — use Windows Native OCR or paste a Gemini/Custom API key"));
        return;
    }

    TranslationJob job;
    job.crop = crop;
    job.cropHash = hash;
    job.sourceRect = rect;
    job.ocrEngineId = m_ocrEngine;
    job.providerId = m_translationProvider;
    job.sourceLanguage = m_sourceLanguage;
    job.targetLanguage = m_targetLanguage;
    job.geminiApiKey = m_geminiApiKey;
    job.customApi = customApiConfig();
    setStatus(QStringLiteral("Reading dialogue…"));
    QMetaObject::invokeMethod(m_worker, "submitJob", Qt::QueuedConnection,
                              Q_ARG(TranslationJob, job));
}

void TranslationEngine::onStabilityTimeout()
{
    if (!m_enabled || m_pendingHash.isEmpty() || m_pendingCrop.isNull())
        return;
    if (m_pendingHash == m_lastProcessedHash)
        return;
    const QImage crop = m_pendingCrop;
    const QString hash = m_pendingHash;
    const QRectF rect = m_pendingRect;
    m_pendingHash.clear();
    m_pendingCrop = QImage();
    submitCrop(crop, hash, rect);
}

void TranslationEngine::requestTestTranslation(const QString &englishText)
{
    const QString text = englishText.trimmed();
    if (text.isEmpty()) {
        setError(QStringLiteral("Nothing to translate"));
        return;
    }
    if (m_chunkEnabled) {
        QString chunked;
        if (m_chunkDb.lookup(text, chunked) || m_chunkDb.lookupFuzzy(text, chunked)) {
            m_lastChunkHit = true;
            setTestResult(text, chunked, 0, 0, true);
            return;
        }
    }
    m_lastChunkHit = false;
    if (m_cacheEnabled) {
        QString cached;
        if (m_dialogueDb.lookup(text, cached) || m_cache.lookup(text, cached)) {
            setTestResult(text, cached, 0, 0, true);
            return;
        }
    }
    TranslationJob job;
    job.directText = text;
    job.testJob = true;
    job.sourceRect = m_dialogueRect;
    job.providerId = m_translationProvider;
    job.sourceLanguage = m_sourceLanguage;
    job.targetLanguage = m_targetLanguage;
    job.geminiApiKey = m_geminiApiKey;
    job.customApi = customApiConfig();
    setStatus(QStringLiteral("Translating…"));
    QMetaObject::invokeMethod(m_worker, "submitJob", Qt::QueuedConnection,
                              Q_ARG(TranslationJob, job));
}

void TranslationEngine::onWorkerJobProgress(const QString &original,
                                            const QString &partialTranslated,
                                            const QRectF &sourceRect, bool testJob)
{
    setPartialTranslation(original, partialTranslated, sourceRect, testJob);
}

void TranslationEngine::onWorkerModelsFetched(const QStringList &models, const QString &error)
{
    if (!error.isEmpty()) {
        setError(error);
        return;
    }
    m_customApiModels = models;
    emit customApiModelsChanged();
    if (m_customApiModel.isEmpty() && !models.isEmpty())
        setCustomApiModel(models.first());
    setStatus(QStringLiteral("Fetched %1 models").arg(models.size()));
}

void TranslationEngine::setPartialTranslation(const QString &original, const QString &partial,
                                              const QRectF &sourceRect, bool testJob)
{
    m_lastOriginalText = original;
    m_lastTranslatedText = partial;
    m_lastError.clear();
    setStatus(QStringLiteral("Translating…"));
    if (testJob) {
        emit translationTextChanged();
        return;
    }
    m_translationBlocks = singleBlockVariantList(original, partial, sourceRect);
    if (!m_hasTranslation) {
        m_hasTranslation = true;
        emit hasTranslationChanged();
    }
    emit translationBlocksChanged();
    emit translationTextChanged();
}

void TranslationEngine::onWorkerJobCompleted(const TranslationJobResult &result)
{
    m_lastChunkHit = result.chunkHit;
    if (!result.ok) {
        setError(result.error.isEmpty() ? QStringLiteral("Translation failed") : result.error);
        if (result.testJob)
            return;
        if (m_hasTranslation)
            m_keepPreviousTimer.start(KeepPreviousMs);
        return;
    }
    if (result.testJob) {
        setTestResult(result.original, result.translated,
                      result.ocrMs, result.translateMs, result.cacheHit);
        return;
    }
    if (result.original.isEmpty() && result.translated.isEmpty()) {
        setStatus(QStringLiteral("No text in scanned area"));
        return;
    }
    m_keepPreviousTimer.stop();
    if (m_cacheEnabled && !result.translated.isEmpty()) {
        m_cache.insert(result.original, result.translated);
        if (!result.cropHash.isEmpty()) {
            if (!m_hashCache.contains(result.cropHash))
                m_hashOrder.append(result.cropHash);
            m_hashCache.insert(result.cropHash,
                               HashCacheEntry{result.original, result.translated,
                                              blocksToVariantList(result.blocks)});
            while (m_hashCache.size() > MaxHashCacheEntries && !m_hashOrder.isEmpty())
                m_hashCache.remove(m_hashOrder.takeFirst());
        }
    }
    m_lastCacheHit = false;
    emitTranslation(result.original, result.translated, result.sourceRect,
                    result.ocrMs, result.translateMs, false,
                    blocksToVariantList(result.blocks), result.chunkHit);
}

void TranslationEngine::onKeepPreviousExpired()
{
    clearTranslation();
}

void TranslationEngine::onFrameTimerTimeout()
{
    if (m_enabled)
        emit frameRequested();
}

QVariantList TranslationEngine::blocksToVariantList(const QList<TranslatedBlock> &blocks)
{
    QVariantList list;
    list.reserve(blocks.size());
    for (const TranslatedBlock &block : blocks) {
        QVariantMap map;
        map.insert(QStringLiteral("original"), block.original);
        map.insert(QStringLiteral("translated"), block.translated);
        map.insert(QStringLiteral("x"), block.rect.x());
        map.insert(QStringLiteral("y"), block.rect.y());
        map.insert(QStringLiteral("w"), block.rect.width());
        map.insert(QStringLiteral("h"), block.rect.height());
        map.insert(QStringLiteral("fgColor"),
                   block.fgColor.isEmpty() ? QStringLiteral("#F4F6FA") : block.fgColor);
        map.insert(QStringLiteral("bgColor"),
                   block.bgColor.isEmpty() ? QStringLiteral("#0B0D14") : block.bgColor);
        list.append(map);
    }
    return list;
}

QVariantList TranslationEngine::singleBlockVariantList(const QString &original,
                                                      const QString &translated,
                                                      const QRectF &rect)
{
    return blocksToVariantList({TranslatedBlock{original, translated, rect}});
}

void TranslationEngine::emitTranslation(const QString &original, const QString &translated,
                                        const QRectF &sourceRect, qint64 ocrMs, qint64 translateMs,
                                        bool cacheHit, const QVariantList &blocks, bool chunkHit)
{
    m_lastOriginalText = original;
    m_lastTranslatedText = translated;
    m_lastOcrMs = ocrMs;
    m_lastTranslateMs = translateMs;
    m_lastCacheHit = cacheHit;
    m_lastChunkHit = chunkHit;
    m_lastError.clear();
    m_translationBlocks = applyStableRects(blocks.isEmpty()
        ? singleBlockVariantList(original, translated, sourceRect)
        : blocks);
    if (!m_hasTranslation) {
        m_hasTranslation = true;
        emit hasTranslationChanged();
    }
    setStatus(QStringLiteral("Translation ready"));
    emit translationBlocksChanged();
    emit translationTextChanged();
    emit translationReady(original, translated, sourceRect);
}

void TranslationEngine::setTestResult(const QString &original, const QString &translated,
                                      qint64 ocrMs, qint64 translateMs, bool cacheHit)
{
    m_lastOriginalText = original;
    m_lastTranslatedText = translated;
    m_lastOcrMs = ocrMs;
    m_lastTranslateMs = translateMs;
    m_lastCacheHit = cacheHit;
    m_lastError.clear();
    setStatus(QStringLiteral("Translation ready"));
    emit translationTextChanged();
}

void TranslationEngine::clearTranslation()
{
    if (!m_hasTranslation && m_lastTranslatedText.isEmpty())
        return;
    m_lastTranslatedText.clear();
    m_lastOriginalText.clear();
    m_translationBlocks.clear();
    m_hasTranslation = false;
    emit hasTranslationChanged();
    emit translationBlocksChanged();
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
