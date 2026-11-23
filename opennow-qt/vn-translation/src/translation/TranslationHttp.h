// TranslationHttp: one synchronous HTTP helper shared by the OCR engines
// and translation providers.
//
// All calls run on the translation worker thread only (never the GUI
// thread). A request waits at most NetworkTimeoutMs for a reply; on HTTP
// 429 it retries exactly ONCE after the server's Retry-After delay
// (default 1s, clamped to NetworkTimeoutMs). No API key ever appears in a
// URL: keys travel in the x-goog-api-key header and are never logged.
#pragma once

#include <QByteArray>
#include <QList>
#include <QPair>
#include <QString>
#include <functional>

struct TranslationHttpResult
{
    int statusCode = 0; // 0 = transport failure / timeout (see error)
    QByteArray body;
    QString error; // non-empty on failure; safe to show (never contains a key)
    bool ok() const { return statusCode >= 200 && statusCode < 300; }
};

class TranslationHttp
{
public:
    static constexpr int NetworkTimeoutMs = 8000;
    static constexpr int MaxRetryAfterMs = 10000;
    // Hard cap on one streaming response, even while chunks keep coming.
    static constexpr int MaxStreamTotalMs = 60000;

    // Synchronous request on the calling thread. `headers` are raw
    // name/value pairs (e.g. {"Content-Type", "application/json"}).
    // `body` is sent as-is for POST; ignored for GET.
    static TranslationHttpResult request(
        const QString &method, const QString &url,
        const QList<QPair<QByteArray, QByteArray>> &headers,
        const QByteArray &body = {});

    static TranslationHttpResult get(const QString &url)
    {
        return request(QStringLiteral("GET"), url, {});
    }
    static TranslationHttpResult postJson(const QString &url, const QByteArray &jsonBody,
                                          const QList<QPair<QByteArray, QByteArray>> &headers = {})
    {
        QList<QPair<QByteArray, QByteArray>> all = headers;
        all.append({QByteArrayLiteral("Content-Type"), QByteArrayLiteral("application/json")});
        return request(QStringLiteral("POST"), url, all, jsonBody);
    }

    // Synchronous streaming POST (SSE): `onChunk` is invoked with each
    // batch of response bytes AS IT ARRIVES, so the caller can show
    // partial text while the model is still generating. The 8s timeout
    // is INACTIVITY-based — the clock restarts on every chunk — and the
    // whole stream is capped at MaxStreamTotalMs. Like request(), a 429
    // retries exactly once. The returned `body` holds only bytes that
    // arrived after the last readyRead (normally empty: chunks go to
    // `onChunk`); check `error` / `statusCode` as usual.
    static TranslationHttpResult postStream(
        const QString &url, const QByteArray &jsonBody,
        const QList<QPair<QByteArray, QByteArray>> &headers,
        const std::function<void(const QByteArray &chunk)> &onChunk);
};
