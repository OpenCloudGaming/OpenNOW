#include "translation/TranslationHttp.h"

#include <QEventLoop>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QTimer>
#include <QUrl>

namespace {
// Waits synchronously for `reply` (worker thread only). Returns false on
// timeout; the caller then aborts the reply.
bool waitForReply(QNetworkReply *reply, int timeoutMs)
{
    QEventLoop loop;
    QTimer timer;
    timer.setSingleShot(true);
    QObject::connect(&timer, &QTimer::timeout, &loop, &QEventLoop::quit);
    QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
    timer.start(timeoutMs);
    loop.exec();
    return reply->isFinished();
}

int retryAfterMs(QNetworkReply *reply)
{
    bool ok = false;
    const int seconds = reply->rawHeader(QByteArrayLiteral("Retry-After")).toInt(&ok);
    if (!ok || seconds <= 0)
        return 1000;
    return qMin(seconds * 1000, TranslationHttp::MaxRetryAfterMs);
}

void sleepMs(int ms)
{
    QEventLoop loop;
    QTimer::singleShot(ms, &loop, &QEventLoop::quit);
    loop.exec();
}
} // namespace

TranslationHttpResult TranslationHttp::request(
    const QString &method, const QString &url,
    const QList<QPair<QByteArray, QByteArray>> &headers,
    const QByteArray &body)
{
    // QNetworkAccessManager is created here, on the calling (worker) thread,
    // used only on this thread, and destroyed before returning. Never create
    // or use it on the GUI thread from the worker.
    for (int attempt = 0; attempt < 2; ++attempt) {
        QNetworkAccessManager manager;
        QNetworkRequest request(QUrl{url});
        for (const auto &header : headers)
            request.setRawHeader(header.first, header.second);
        QNetworkReply *reply = nullptr;
        if (method == QStringLiteral("POST"))
            reply = manager.post(request, body);
        else
            reply = manager.get(request);

        if (!waitForReply(reply, NetworkTimeoutMs)) {
            reply->abort();
            reply->deleteLater();
            return TranslationHttpResult{0, {}, QStringLiteral("Network timeout")};
        }
        TranslationHttpResult result;
        result.statusCode = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        result.body = reply->readAll();
        if (reply->error() != QNetworkReply::NoError && result.statusCode == 0)
            result.error = reply->errorString();
        const int waitMs = retryAfterMs(reply);
        const bool rateLimited = result.statusCode == 429;
        reply->deleteLater();
        if (rateLimited && attempt == 0) {
            sleepMs(waitMs); // single retry; capped at MaxRetryAfterMs
            continue;
        }
        if (!result.ok() && result.error.isEmpty())
            result.error = QStringLiteral("HTTP %1").arg(result.statusCode);
        return result;
    }
    return TranslationHttpResult{429, {}, QStringLiteral("Rate limited")};
}

TranslationHttpResult TranslationHttp::postStream(
    const QString &url, const QByteArray &jsonBody,
    const QList<QPair<QByteArray, QByteArray>> &headers,
    const std::function<void(const QByteArray &chunk)> &onChunk)
{
    QList<QPair<QByteArray, QByteArray>> all = headers;
    all.append({QByteArrayLiteral("Content-Type"), QByteArrayLiteral("application/json")});
    for (int attempt = 0; attempt < 2; ++attempt) {
        QNetworkAccessManager manager;
        QNetworkRequest request(QUrl{url});
        for (const auto &header : all)
            request.setRawHeader(header.first, header.second);
        QNetworkReply *reply = manager.post(request, jsonBody);

        // Inactivity timeout: the timer restarts on every chunk, so a
        // slow-but-alive stream is never cut off; a dead stream dies
        // after NetworkTimeoutMs of silence. The total timer caps the
        // whole stream at MaxStreamTotalMs even while chunks keep coming.
        QEventLoop loop;
        QTimer timer;
        timer.setSingleShot(true);
        QTimer totalTimer;
        totalTimer.setSingleShot(true);
        bool timedOut = false;
        QObject::connect(&timer, &QTimer::timeout, [&] {
            timedOut = true;
            loop.quit();
        });
        QObject::connect(&totalTimer, &QTimer::timeout, [&] {
            timedOut = true;
            loop.quit();
        });
        QObject::connect(reply, &QNetworkReply::readyRead, [&] {
            timer.start(NetworkTimeoutMs);
            if (onChunk)
                onChunk(reply->readAll());
        });
        QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
        timer.start(NetworkTimeoutMs);
        totalTimer.start(MaxStreamTotalMs);
        loop.exec();
        if (timedOut || !reply->isFinished()) {
            reply->abort();
            reply->deleteLater();
            return TranslationHttpResult{0, {}, QStringLiteral("Network timeout")};
        }
        // Bytes that arrived with `finished` instead of a readyRead.
        const QByteArray tail = reply->readAll();
        if (!tail.isEmpty() && onChunk)
            onChunk(tail);
        TranslationHttpResult result;
        result.statusCode = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        result.body = tail;
        if (reply->error() != QNetworkReply::NoError && result.statusCode == 0)
            result.error = reply->errorString();
        const int waitMs = retryAfterMs(reply);
        const bool rateLimited = result.statusCode == 429;
        reply->deleteLater();
        if (rateLimited && attempt == 0) {
            sleepMs(waitMs); // single retry; capped at MaxRetryAfterMs
            continue;
        }
        if (!result.ok() && result.error.isEmpty())
            result.error = QStringLiteral("HTTP %1").arg(result.statusCode);
        return result;
    }
    return TranslationHttpResult{429, {}, QStringLiteral("Rate limited")};
}
