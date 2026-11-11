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
        QNetworkRequest request(QUrl(url));
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
