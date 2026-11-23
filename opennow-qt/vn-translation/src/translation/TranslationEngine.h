// TranslationEngine: the QML-facing translation controller.
#pragma once

#include <QElapsedTimer>
#include <QHash>
#include <QImage>
#include <QList>
#include <QObject>
#include <QPair>
#include <QRectF>
#include <QString>
#include <QTimer>
#include <QVariant>

#include "translation/CustomApi.h"
#include "translation/TranslationCache.h"
#include "translation/TranslationWorker.h"
#include "translation/WuWaDialogueDb.h"

class TranslationEngine : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool enabled READ enabled WRITE setEnabled NOTIFY enabledChanged)
    Q_PROPERTY(bool visionKeyConfigured READ visionKeyConfigured NOTIFY visionKeyChanged)
    Q_PROPERTY(bool ocrReady READ ocrReady NOTIFY visionKeyChanged)
    Q_PROPERTY(QString sourceLanguage READ sourceLanguage WRITE setSourceLanguage NOTIFY sourceLanguageChanged)
    Q_PROPERTY(QString targetLanguage READ targetLanguage WRITE setTargetLanguage NOTIFY targetLanguageChanged)
    Q_PROPERTY(QString ocrEngine READ ocrEngine WRITE setOcrEngine NOTIFY ocrEngineChanged)
    Q_PROPERTY(QString translationProvider READ translationProvider WRITE setTranslationProvider NOTIFY translationProviderChanged)
    Q_PROPERTY(QString overlayMode READ overlayMode WRITE setOverlayMode NOTIFY overlayModeChanged)
    Q_PROPERTY(int stabilityDelayMs READ stabilityDelayMs WRITE setStabilityDelayMs NOTIFY stabilityDelayMsChanged)
    Q_PROPERTY(int fontSizePx READ fontSizePx WRITE setFontSizePx NOTIFY fontSizePxChanged)
    Q_PROPERTY(bool showOriginal READ showOriginal WRITE setShowOriginal NOTIFY showOriginalChanged)
    Q_PROPERTY(bool showTimingPill READ showTimingPill WRITE setShowTimingPill NOTIFY showTimingPillChanged)
    Q_PROPERTY(bool cacheEnabled READ cacheEnabled WRITE setCacheEnabled NOTIFY cacheEnabledChanged)
    Q_PROPERTY(QString statusText READ statusText NOTIFY statusChanged)
    Q_PROPERTY(QString lastTranslatedText READ lastTranslatedText NOTIFY translationTextChanged)
    Q_PROPERTY(QString lastOriginalText READ lastOriginalText NOTIFY translationTextChanged)
    Q_PROPERTY(QRectF dialogueRect READ dialogueRect WRITE setDialogueRect NOTIFY dialogueRectChanged)
    Q_PROPERTY(QString dialoguePreset READ dialoguePreset WRITE setDialoguePreset NOTIFY dialoguePresetChanged)
    Q_PROPERTY(qint64 lastOcrMs READ lastOcrMs NOTIFY translationTextChanged)
    Q_PROPERTY(qint64 lastTranslateMs READ lastTranslateMs NOTIFY translationTextChanged)
    Q_PROPERTY(bool lastCacheHit READ lastCacheHit NOTIFY translationTextChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY statusChanged)
    Q_PROPERTY(bool hasTranslation READ hasTranslation NOTIFY hasTranslationChanged)
    // One entry per translated block: {original, translated, x, y, w, h, fgColor, bgColor}
    Q_PROPERTY(QVariantList translationBlocks READ translationBlocks NOTIFY translationBlocksChanged)
    Q_PROPERTY(QString customApiBase READ customApiBase WRITE setCustomApiBase NOTIFY customApiBaseChanged)
    Q_PROPERTY(QString customApiModel READ customApiModel WRITE setCustomApiModel NOTIFY customApiModelChanged)
    Q_PROPERTY(QStringList customApiModels READ customApiModels NOTIFY customApiModelsChanged)
    Q_PROPERTY(bool chunkEnabled READ chunkEnabled WRITE setChunkEnabled NOTIFY chunkEnabledChanged)
    Q_PROPERTY(QString chunkGame READ chunkGame WRITE setChunkGame NOTIFY chunkGameChanged)
    Q_PROPERTY(QStringList chunkGames READ chunkGames NOTIFY chunkGameChanged)
    Q_PROPERTY(QString chunkGameTitle READ chunkGameTitle NOTIFY chunkGameInfoChanged)
    Q_PROPERTY(QString chunkGameVersion READ chunkGameVersion NOTIFY chunkGameInfoChanged)
    Q_PROPERTY(QString chunkGameWordsText READ chunkGameWordsText NOTIFY chunkGameInfoChanged)
    Q_PROPERTY(QString chunkGameStringsText READ chunkGameStringsText NOTIFY chunkGameInfoChanged)
    Q_PROPERTY(QString chunkTranslatedText READ chunkTranslatedText NOTIFY chunkGameInfoChanged)
    Q_PROPERTY(QString chunkRepoUrl READ chunkRepoUrl NOTIFY chunkGameInfoChanged)
    Q_PROPERTY(QString chunkStatus READ chunkStatus NOTIFY chunkStatusChanged)
    Q_PROPERTY(double chunkProgress READ chunkProgress NOTIFY chunkProgressChanged)
    Q_PROPERTY(int chunkDbSize READ chunkDbSize NOTIFY chunkDbSizeChanged)
    Q_PROPERTY(bool chunkDownloaded READ chunkDownloaded NOTIFY chunkDbSizeChanged)
    Q_PROPERTY(bool lastChunkHit READ lastChunkHit NOTIFY translationTextChanged)

public:
    explicit TranslationEngine(QObject *parent = nullptr);
    ~TranslationEngine() override;

    static QImage cropDialogueImage(const QImage &image, const QRectF &normalizedRect);
    // Fast FNV-1a 64-bit strided pixel hash (< 0.08ms on 1080p); "" for null/empty image.
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
    bool showTimingPill() const { return m_showTimingPill; }
    void setShowTimingPill(bool show);
    bool cacheEnabled() const { return m_cacheEnabled; }
    void setCacheEnabled(bool enabled);
    QString statusText() const { return m_statusText; }
    QString lastTranslatedText() const { return m_lastTranslatedText; }
    QString lastOriginalText() const { return m_lastOriginalText; }
    QRectF dialogueRect() const { return m_dialogueRect; }
    void setDialogueRect(const QRectF &rect);
    QString dialoguePreset() const { return m_dialoguePreset; }
    static QRectF dialogueRectForPreset(const QString &preset);
    static QString migrateDialoguePreset(const QString &saved);
    static QVariantList blocksToVariantList(const QList<TranslatedBlock> &blocks);
    static QVariantList singleBlockVariantList(const QString &original, const QString &translated,
                                               const QRectF &rect);
    qint64 lastOcrMs() const { return m_lastOcrMs; }
    qint64 lastTranslateMs() const { return m_lastTranslateMs; }
    bool lastCacheHit() const { return m_lastCacheHit; }
    QString lastError() const { return m_lastError; }
    bool hasTranslation() const { return m_hasTranslation; }
    QVariantList translationBlocks() const { return m_translationBlocks; }
    QString customApiBase() const { return m_customApiBase; }
    void setCustomApiBase(const QString &base);
    QString customApiModel() const { return m_customApiModel; }
    void setCustomApiModel(const QString &model);
    QStringList customApiModels() const { return m_customApiModels; }

    bool chunkEnabled() const { return m_chunkEnabled; }
    void setChunkEnabled(bool enabled);
    QString chunkGame() const { return m_chunkGame; }
    void setChunkGame(const QString &game);
    QStringList chunkGames() const;
    QString chunkGameTitle() const { return m_chunkGameTitle; }
    QString chunkGameVersion() const { return m_chunkVersion; }
    QString chunkGameWordsText() const { return m_chunkGameWordsText; }
    QString chunkGameStringsText() const { return m_chunkGameStringsText; }
    QString chunkTranslatedText() const { return m_chunkTranslatedText; }
    QString chunkRepoUrl() const { return m_chunkRepoUrl; }
    QString chunkStatus() const { return m_chunkStatus; }
    double chunkProgress() const { return m_chunkProgress; }
    int chunkDbSize() const { return m_chunkDb.size(); }
    bool chunkDownloaded() const { return isChunkDownloadComplete(m_chunkDb.size(), m_chunkExpectedStrings); }
    static bool isChunkDownloadComplete(int dbSize, int expectedStrings);
    bool lastChunkHit() const { return m_lastChunkHit; }

    Q_INVOKABLE void onStreamFrame(const QImage &image);
    Q_INVOKABLE void requestTestTranslation(const QString &englishText);
    Q_INVOKABLE void clearCache();
    Q_INVOKABLE void downloadChunks();
    Q_INVOKABLE void setGeminiApiKey(const QString &key);
    Q_INVOKABLE bool hasGeminiApiKey() const;
    Q_INVOKABLE void setCustomApiKey(const QString &key);
    Q_INVOKABLE bool hasCustomApiKey() const;
    bool visionKeyConfigured() const;
    bool ocrReady() const;
    Q_INVOKABLE void fetchCustomApiModels();
    Q_INVOKABLE void setDialoguePreset(const QString &preset);

signals:
    void translationReady(const QString &original, const QString &translated, const QRectF &sourceRect);
    void translationCleared();
    void translationTextChanged();
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
    void showTimingPillChanged();
    void cacheEnabledChanged();
    void dialogueRectChanged();
    void dialoguePresetChanged();
    void hasTranslationChanged();
    void translationBlocksChanged();
    void customApiBaseChanged();
    void customApiModelChanged();
    void customApiModelsChanged();
    void chunkEnabledChanged();
    void chunkGameChanged();
    void chunkGameInfoChanged();
    void chunkStatusChanged();
    void chunkProgressChanged();
    void chunkDbSizeChanged();
    void visionKeyChanged();
    void frameRequested();

private slots:
    void onStabilityTimeout();
    void onWorkerJobCompleted(const TranslationJobResult &result);
    void onWorkerJobProgress(const QString &original, const QString &partialTranslated,
                             const QRectF &sourceRect, bool testJob);
    void onWorkerModelsFetched(const QStringList &models, const QString &error);
    void onChunkDownloadProgress(int done, int total);
    void onChunkDownloadFinished(const QString &error);
    void onKeepPreviousExpired();
    void onFrameTimerTimeout();

private:
    void setStatus(const QString &status);
    void setError(const QString &error);
    int effectiveFullScreenIntervalMs() const;
    CustomApiConfig customApiConfig() const;
    void syncCustomApiToWorker();
    void loadCachedChunks();
    void ensureChunksDownloaded();
    void setChunkStatus(const QString &status);
    void applyChunkCatalog();
    QString chunkGameCacheBase() const;
    QVariantList applyStableRects(const QVariantList &blocks);
    void submitCrop(const QImage &crop, const QString &hash, const QRectF &rect);
    void emitTranslation(const QString &original, const QString &translated,
                         const QRectF &sourceRect, qint64 ocrMs, qint64 translateMs,
                         bool cacheHit, const QVariantList &blocks = {},
                         bool chunkHit = false);
    void setPartialTranslation(const QString &original, const QString &partial,
                               const QRectF &sourceRect, bool testJob);
    void setTestResult(const QString &original, const QString &translated,
                       qint64 ocrMs, qint64 translateMs, bool cacheHit);
    void clearTranslation();

    bool m_enabled = false;
    QString m_sourceLanguage = QStringLiteral("en");
    QString m_targetLanguage = QStringLiteral("vi");
    QString m_ocrEngine = QStringLiteral("windows-ocr");
    QString m_translationProvider = QStringLiteral("google");
    QString m_overlayMode = QStringLiteral("inplace");
    int m_stabilityDelayMs = 220;
    int m_fontSizePx = 22;
    bool m_showOriginal = false;
    bool m_showTimingPill = false; // clean native look by default
    bool m_cacheEnabled = true;
    QString m_statusText;
    QString m_lastTranslatedText;
    QString m_lastOriginalText;
    QRectF m_dialogueRect = QRectF(0, 0, 1, 1);
    QString m_dialoguePreset = QStringLiteral("full-screen");
    qint64 m_lastOcrMs = 0;
    qint64 m_lastTranslateMs = 0;
    bool m_lastCacheHit = false;
    QString m_lastError;
    bool m_hasTranslation = false;
    QString m_geminiApiKey;
    QString m_customApiBase;
    QString m_customApiKey;
    QString m_customApiModel;
    QStringList m_customApiModels;

    TranslationCache m_cache;
    WuWaDialogueDb m_dialogueDb;
    WuWaDialogueDb m_chunkDb;
    bool m_chunkEnabled = true;
    QString m_chunkGame = QStringLiteral("wuthering-waves");
    QString m_chunkVersion;
    QString m_chunkGameTitle = QStringLiteral("Wuthering Waves");
    QString m_chunkGameWordsText;
    QString m_chunkGameStringsText;
    QString m_chunkTranslatedText;
    QString m_chunkRepoUrl = QStringLiteral("https://github.com/huyzoz/wuwa-vi-chunks");
    QString m_chunkStatus;
    double m_chunkProgress = 0.0;
    bool m_lastChunkHit = false;
    int m_chunkExpectedStrings = 0;
    bool m_chunkDownloading = false;

    struct StableRect
    {
        QRectF rect;
        int hits = 1;
    };
    static constexpr int MaxStableRects = 200;
    QHash<QString, StableRect> m_stableRects;
    QList<QString> m_stableRectOrder;
    TranslationWorker *m_worker = nullptr;

    QTimer m_stabilityTimer;
    QTimer m_keepPreviousTimer;
    QTimer m_frameTimer;
    QVariantList m_translationBlocks;
    QElapsedTimer m_fullScreenThrottle;
    QString m_pendingHash;
    QImage m_pendingCrop;
    QRectF m_pendingRect;
    QString m_lastProcessedHash;

    struct HashCacheEntry
    {
        QString original;
        QString translated;
        QVariantList blocks;
    };
    static constexpr int MaxHashCacheEntries = 500;
    QHash<QString, HashCacheEntry> m_hashCache;
    QList<QString> m_hashOrder;
};
