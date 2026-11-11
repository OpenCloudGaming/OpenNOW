#include "translation/TranslationWorker.h"
#include "translation/OcrEngine.h"
#include "translation/TranslationProvider.h"

#include <QElapsedTimer>
#include <QMetaObject>
#include <QMutexLocker>

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
        return m_running; // already running; caller is processing it
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

// ---------------------------------------------------------------------------
// TranslationWorker
// ---------------------------------------------------------------------------

TranslationWorker::TranslationWorker(QObject *parent)
    : QObject(parent)
{
    qRegisterMetaType<TranslationJob>();
    qRegisterMetaType<TranslationJobResult>();
}

TranslationWorker::~TranslationWorker()
{
    stop();
}

void TranslationWorker::start()
{
    if (m_thread.isRunning())
        return;
    moveToThread(&m_thread);
    m_thread.start();
}

void TranslationWorker::stop()
{
    if (!m_thread.isRunning())
        return;
    // Cancel anything queued, then quit the event loop and wait. A job
    // already running finishes its current synchronous call first (bounded
    // by the 8s network timeout), then the thread exits. After the thread
    // has finished, move back to the calling thread so deletion is safe.
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
    // Runs in the worker thread (queued invocation from the GUI thread).
    TranslationJob effective = job;
    if (effective.geminiApiKey.isEmpty())
        effective.geminiApiKey = m_geminiApiKey;
    m_queue.submit(effective);
    if (!m_processing)
        QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
}

void TranslationWorker::setGeminiApiKey(const QString &key)
{
    // Runs in the worker thread; the key is never logged.
    m_geminiApiKey = key;
}

void TranslationWorker::cancelPending()
{
    // Runs in the worker thread. Drops the waiting job only; a running job
    // is left to finish (it is bounded by the network/OCR timeouts).
    m_queue.clearWaiting();
}

void TranslationWorker::processNext()
{
    // Runs in the worker thread. m_processing guards re-entrancy; the
    // queue itself decides running vs waiting.
    if (m_processing)
        return;
    std::optional<TranslationJob> job = m_queue.takeRunning();
    if (!job.has_value())
        return;
    m_processing = true;

    TranslationJobResult result;
    result.sourceRect = job->sourceRect;
    result.cropHash = job->cropHash;

    QString original;
    qint64 ocrMs = 0;
    if (!job->directText.isEmpty()) {
        original = job->directText;
    } else {
        QElapsedTimer ocrTimer;
        ocrTimer.start();
        // Fallback chain: try the selected engine first; if it is
        // unavailable (no Gemini key, no tesseract binary, stub engine),
        // try the next engine that reports available(). This is what makes
        // the default (gemini-vision, no key yet) actually work on a fresh
        // install when Tesseract is present, instead of failing outright.
        // A Failed (network/process) result is NOT retried on another
        // engine — it is a real error, not a missing engine.
        const QStringList candidates = {job->ocrEngineId, QStringLiteral("tesseract"),
                                        QStringLiteral("gemini-vision")};
        QStringList tried;
        OcrResult ocr;
        bool haveOcrResult = false;
        for (const QString &candidateId : candidates) {
            if (tried.contains(candidateId))
                continue;
            tried.append(candidateId);
            auto engine = createOcrEngine(candidateId, job->geminiApiKey);
            if (!engine)
                continue;
            if (!engine->available())
                continue;
            ocr = engine->recognize(job->crop);
            haveOcrResult = true;
            break;
        }
        ocrMs = ocrTimer.elapsed();
        if (!haveOcrResult) {
            result.error = QStringLiteral("No OCR engine available (tried: %1). "
                                          "Set a Gemini key or install Tesseract.")
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
        original = ocr.text.trimmed();
        if (original.isEmpty()) {
            // No dialogue text in this crop: not an error, just nothing to
            // translate. Report success with empty text so the engine can
            // keep the previous translation instead of showing an error.
            result.ok = true;
            result.original = {};
            result.translated = {};
            result.ocrMs = ocrMs;
            m_queue.finishRunning();
            m_processing = false;
            emit jobCompleted(result);
            QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
            return;
        }
    }
    result.original = original;

    QElapsedTimer translateTimer;
    translateTimer.start();
    auto provider = createTranslationProvider(job->providerId, job->geminiApiKey);
    if (!provider) {
        result.error = QStringLiteral("Unknown translation provider");
        result.ocrMs = ocrMs;
        m_queue.finishRunning();
        m_processing = false;
        emit jobCompleted(result);
        QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
        return;
    }
    const TranslateResult translated =
        provider->translate(original, job->sourceLanguage, job->targetLanguage);
    result.translateMs = translateTimer.elapsed();
    result.ocrMs = ocrMs;
    if (!translated.ok) {
        result.error = translated.error;
    } else {
        result.ok = true;
        result.translated = translated.text;
    }

    m_queue.finishRunning();
    m_processing = false;
    emit jobCompleted(result);
    QMetaObject::invokeMethod(this, "processNext", Qt::QueuedConnection);
}
