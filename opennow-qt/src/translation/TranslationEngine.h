// TranslationEngine: the QML-facing translation controller.
//
// Exposed to QML as the context property "translationEngine". QML reads the
// properties below and calls onStreamFrame() indirectly: ApplicationStartup
// connects NativeStreamRuntime::screenshotReady to onStreamFrame() (in
// addition to the existing saveStreamScreenshot connect, which is untouched).
//
// Pipeline (onStreamFrame, GUI thread only):
//   1. Ignore when disabled or the image is null/empty.
//   2. Crop dialogueRect (normalized 0..1) and SHA-256-hash the crop.
//   3. Only proceed once the hash is unchanged for stabilityDelayMs
//      (QTimer restarts on every hash change).
//   4. Cache lookup first: WuWa dialogue DB, then the memory cache.
//      A hit emits translationReady immediately (lastCacheHit=true) with
//      no network at all.
//   5. Otherwise a job goes to TranslationWorker (own thread): OCR with
//      the selected engine, then translate with the selected provider,
//      then insert into the cache and emit translationReady.
// All network/OCR work happens in the worker thread; the GUI thread never
// blocks. On failure lastError is set, the previous translation is kept
// for 2s, and no garbage text is ever emitted.
#pragma once

#include <QHash>
#include <QImage>
#include <QList>
#include <QObject>
#include <QPair>
#include <QRectF>
#include <QString>
#include <QTimer>

#include "translation/TranslationCache.h"
#include "translation/TranslationWorker.h"
#include "translation/WuWaDialogueDb.h"

class TranslationEngine : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool enabled READ enabled WRITE setEnabled NOTIFY enabledChanged)
    Q_PROPERTY(QString sourceLanguage READ sourceLanguage WRITE setSourceLanguage NOTIFY sourceLanguageChanged)
    Q_PROPERTY(QString targetLanguage READ targetLanguage WRITE setTargetLanguage NOTIFY targetLanguageChanged)
    Q_PROPERTY(QString ocrEngine READ ocrEngine WRITE setOcrEngine NOTIFY ocrEngineChanged)
    Q_PROPERTY(QString translationProvider READ translationProvider WRITE setTranslationProvider NOTIFY translationProviderChanged)
    Q_PROPERTY(QString overlayMode READ overlayMode WRITE setOverlayMode NOTIFY overlayModeChanged)
    Q_PROPERTY(int stabilityDelayMs READ stabilityDelayMs WRITE setStabilityDelayMs NOTIFY stabilityDelayMsChanged)
    Q_PROPERTY(int fontSizePx READ fontSizePx WRITE setFontSizePx NOTIFY fontSizePxChanged)
    Q_PROPERTY(bool showOriginal READ showOriginal WRITE setShowOriginal NOTIFY showOriginalChanged)
    Q_PROPERTY(bool cacheEnabled READ cacheEnabled WRITE setCacheEnabled NOTIFY cacheEnabledChanged)
    Q_PROPERTY(QString statusText READ statusText NOTIFY statusChanged)
    Q_PROPERTY(QString lastTranslatedText READ lastTranslatedText NOTIFY translationTextChanged)
    Q_PROPERTY(QString lastOriginalText READ lastOriginalText NOTIFY translationTextChanged)
    Q_PROPERTY(QRectF dialogueRect READ dialogueRect WRITE setDialogueRect NOTIFY dialogueRectChanged)
    Q_PROPERTY(qint64 lastOcrMs READ lastOcrMs NOTIFY translationTextChanged)
    Q_PROPERTY(qint64 lastTranslateMs READ lastTranslateMs NOTIFY translationTextChanged)
    Q_PROPERTY(bool lastCacheHit READ lastCacheHit NOTIFY translationTextChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY statusChanged)
    Q_PROPERTY(bool hasTranslation READ hasTranslation NOTIFY hasTranslationChanged)

public:
    explicit TranslationEngine(QObject *parent = nullptr);
    ~TranslationEngine() override;

    // Pure, unit-testable helpers (no instance state).
    // Crops the normalized rect (clamped to the image) or returns a null
    // image when the rect is empty/out of bounds.
    static QImage cropDialogueImage(const QImage &image, const QRectF &normalizedRect);
    // SHA-256 hex of the crop's ARGB32 pixels; "" for a null/empty image.
    static QString cropHash(const QImage &image);

    bool enabled() const { return m_enabled; }
    void setEnabled(bool enabled);
    QString sourceLanguage() const { return m_sourceLanguage; }
    void setSourceLanguage(const QString &language);
    QString targetLanguage() const { return m_targetLanguage; }
    void setTargetLanguage(const QString &language);
    QString ocrEngine() const { return m_ocrEngine; }
    void setOcrEngine(const QString &engine);
    QString translationProvider() const { return m_translationProvider; }
    void setTranslationProvider(const QString &provider);
    QString overlayMode() const { return m_overlayMode; }
    void setOverlayMode(const QString &mode);
    int stabilityDelayMs() const { return m_stabilityDelayMs; }
    void setStabilityDelayMs(int ms);
    int fontSizePx() const { return m_fontSizePx; }
    void setFontSizePx(int px);
    bool showOriginal() const { return m_showOriginal; }
    void setShowOriginal(bool show);
    bool cacheEnabled() const { return m_cacheEnabled; }
    void setCacheEnabled(bool enabled);
    QString statusText() const { return m_statusText; }
    QString lastTranslatedText() const { return m_lastTranslatedText; }
    QString lastOriginalText() const { return m_lastOriginalText; }
    QRectF dialogueRect() const { return m_dialogueRect; }
    void setDialogueRect(const QRectF &rect);
    qint64 lastOcrMs() const { return m_lastOcrMs; }
    qint64 lastTranslateMs() const { return m_lastTranslateMs; }
    bool lastCacheHit() const { return m_lastCacheHit; }
    QString lastError() const { return m_lastError; }
    bool hasTranslation() const { return m_hasTranslation; }

    Q_INVOKABLE void onStreamFrame(const QImage &image);
    Q_INVOKABLE void requestTestTranslation(const QString &englishText);
    Q_INVOKABLE void clearCache();
    Q_INVOKABLE void setGeminiApiKey(const QString &key);
    Q_INVOKABLE bool hasGeminiApiKey() const;
    // "wuthering-waves" (dialogue bottom bar) or "generic" (lower third).
    Q_INVOKABLE void setDialoguePreset(const QString &preset);

signals:
    void translationReady(const QString &original, const QString &translated, const QRectF &sourceRect);
    void translationCleared();
    void translationTextChanged(); // parameterless NOTIFY for last* properties
    void statusChanged();
    void enabledChanged();
    void sourceLanguageChanged();
    void targetLanguageChanged();
    void ocrEngineChanged();
    void translationProviderChanged();
    void overlayModeChanged();
    void stabilityDelayMsChanged();
    void fontSizePxChanged();
    void showOriginalChanged();
    void cacheEnabledChanged();
    void dialogueRectChanged();
    void hasTranslationChanged();

private slots:
    void onStabilityTimeout();
    void onWorkerJobCompleted(const TranslationJobResult &result);
    void onKeepPreviousExpired();

private:
    void setStatus(const QString &status);
    void setError(const QString &error);
    void emitTranslation(const QString &original, const QString &translated,
                         const QRectF &sourceRect, qint64 ocrMs, qint64 translateMs,
                         bool cacheHit);
    void clearTranslation();

    bool m_enabled = false; // OFF by default: translation is opt-in
    QString m_sourceLanguage = QStringLiteral("en");
    QString m_targetLanguage = QStringLiteral("vi");
    QString m_ocrEngine = QStringLiteral("gemini-vision");
    QString m_translationProvider = QStringLiteral("google");
    QString m_overlayMode = QStringLiteral("inplace");
    int m_stabilityDelayMs = 400;
    int m_fontSizePx = 22;
    bool m_showOriginal = false;
    bool m_cacheEnabled = true;
    QString m_statusText;
    QString m_lastTranslatedText;
    QString m_lastOriginalText;
    QRectF m_dialogueRect = QRectF(0.05, 0.72, 0.90, 0.24);
    qint64 m_lastOcrMs = 0;
    qint64 m_lastTranslateMs = 0;
    bool m_lastCacheHit = false;
    QString m_lastError;
    bool m_hasTranslation = false;
    QString m_geminiApiKey; // persisted locally via QSettings only; never logged, never committed

    TranslationCache m_cache;
    WuWaDialogueDb m_dialogueDb;
    TranslationWorker *m_worker = nullptr; // no parent: lives in its own thread

    QTimer m_stabilityTimer;
    QTimer m_keepPreviousTimer;
    QString m_pendingHash;
    QImage m_pendingCrop; // detached copy of the stable crop
    QRectF m_pendingRect;
    QString m_lastProcessedHash; // hash of the crop last sent to cache/worker
    // Crop-hash -> (original, translated). Lets a repeated identical
    // dialogue line hit the cache BEFORE any OCR/network, which the
    // text-keyed cache cannot do (it needs the English text first).
    // In-memory only; the text-keyed TranslationCache is the persisted one.
    static constexpr int MaxHashCacheEntries = 500;
    QHash<QString, QPair<QString, QString>> m_hashCache;
    QList<QString> m_hashOrder; // oldest first; drives hash-cache eviction
};
