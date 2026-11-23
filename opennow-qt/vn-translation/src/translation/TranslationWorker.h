// TranslationWorker: OCR + translation off the GUI thread.
#pragma once

#include <QImage>
#include <QMetaType>
#include <QMutex>
#include <QObject>
#include <QRectF>
#include <QString>
#include <QThread>
#include <atomic>
#include <optional>

#include "translation/CustomApi.h"
#include "translation/OcrEngine.h"
#include "translation/WuWaDialogueDb.h"

struct TranslatedBlock
{
    QString original;
    QString translated;
    QRectF rect;
    QString fgColor = QStringLiteral("#F4F6FA");
    QString bgColor = QStringLiteral("#0B0D14");
};
Q_DECLARE_METATYPE(TranslatedBlock)

struct TranslationJob
{
    QImage crop;
    QString cropHash;
    QRectF sourceRect;
    QString directText;
    bool testJob = false;
    QString ocrEngineId = QStringLiteral("windows-ocr");
    QString providerId = QStringLiteral("google");
    QString sourceLanguage = QStringLiteral("en");
    QString targetLanguage = QStringLiteral("vi");
    QString geminiApiKey;
    CustomApiConfig customApi;
};
Q_DECLARE_METATYPE(TranslationJob)

struct TranslationJobResult
{
    bool ok = false;
    QString original;
    QString translated;
    QList<TranslatedBlock> blocks;
    bool testJob = false;
    QRectF sourceRect;
    qint64 ocrMs = 0;
    qint64 translateMs = 0;
    QString cropHash;
    bool cacheHit = false;
    bool chunkHit = false;
    QString error;
};
Q_DECLARE_METATYPE(TranslationJobResult)

QRectF mapBlockRectToFrame(const QRectF &blockRect, const QRectF &sourceRect);

// Filters HUD noise (FPS, ms, UID, LV, pure numbers, 1-char hotkeys) and
// merges consecutive wrapped dialogue lines that match a multi-line entry
// in the WuWa chunk database.
QList<OcrBlock> mergeAndFilterOcrBlocks(const QList<OcrBlock> &rawBlocks,
                                        const WuWaDialogueDb *chunkDb,
                                        bool chunkEnabled);

class TranslationJobQueue
{
public:
    bool submit(const TranslationJob &job);
    std::optional<TranslationJob> takeRunning();
    void finishRunning();
    bool hasRunning() const;
    bool hasWaiting() const;
    int droppedStale() const { return m_droppedStale; }
    void clear();
    void clearWaiting();

private:
    mutable QMutex m_mutex;
    std::optional<TranslationJob> m_running;
    std::optional<TranslationJob> m_waiting;
    int m_droppedStale = 0;
};

class TranslationWorker : public QObject
{
    Q_OBJECT
public:
    explicit TranslationWorker(QObject *parent = nullptr);
    ~TranslationWorker() override;

    void start();
    void stop();

    int droppedStaleJobs() const;

public slots:
    void submitJob(const TranslationJob &job);
    void setGeminiApiKey(const QString &key);
    void setCustomApiConfig(const CustomApiConfig &config);
    void fetchCustomApiModels(const QString &base, const QString &apiKey);
    void cancelPending();
    void setChunkDb(WuWaDialogueDb *db);
    void setChunkEnabled(bool enabled);
    void startChunkDownload(const QString &manifestUrl, const QString &cacheDir);

signals:
    void jobCompleted(const TranslationJobResult &result);
    void jobProgress(const QString &original, const QString &partialTranslated,
                     const QRectF &sourceRect, bool testJob);
    void customApiModelsFetched(const QStringList &models, const QString &error);
    void chunkDownloadProgress(int done, int total);
    void chunkDownloadFinished(const QString &error);

private slots:
    void processNext();
    void downloadNextChunkFile();

private:
    TranslationJobQueue m_queue;
    QThread m_thread;
    std::atomic_bool m_abortRequested{false};
    QString m_geminiApiKey;
    CustomApiConfig m_customApi;
    bool m_processing = false;
    WuWaDialogueDb *m_chunkDb = nullptr;
    bool m_chunkEnabled = false;
    QString m_chunkManifestUrl;
    QString m_chunkBaseUrl;
    QByteArray m_chunkManifestBody;
    QString m_chunkVersion;
    QString m_chunkCacheDir;
    QStringList m_chunkFiles;
    int m_chunkIndex = 0;
    QString m_chunkError;
};
