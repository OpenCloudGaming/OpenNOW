#include "translation/CustomApi.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

QString normalizeCustomApiBase(const QString &base)
{
    QString normalized = base.trimmed();
    while (normalized.endsWith(QChar(u'/')))
        normalized.chop(1);
    // Users often paste the full chat endpoint they copied from docs.
    const QString chatSuffix = QStringLiteral("/chat/completions");
    if (normalized.endsWith(chatSuffix))
        normalized.chop(chatSuffix.size());
    while (normalized.endsWith(QChar(u'/')))
        normalized.chop(1);
    return normalized;
}

QString customApiModelsUrl(const QString &base)
{
    const QString normalized = normalizeCustomApiBase(base);
    if (normalized.isEmpty())
        return QString();
    return normalized + QStringLiteral("/models");
}

QString customApiChatUrl(const QString &base)
{
    const QString normalized = normalizeCustomApiBase(base);
    if (normalized.isEmpty())
        return QString();
    return normalized + QStringLiteral("/chat/completions");
}

QStringList parseCustomApiModels(const QByteArray &body, QString &error)
{
    const QJsonDocument doc = QJsonDocument::fromJson(body);
    if (!doc.isObject()) {
        error = QStringLiteral("Invalid response from API");
        return {};
    }
    const QJsonArray data = doc.object().value(QStringLiteral("data")).toArray();
    if (data.isEmpty()) {
        error = QStringLiteral("API returned no models");
        return {};
    }
    QStringList models;
    for (const QJsonValue &value : data) {
        const QString id = value.toObject().value(QStringLiteral("id")).toString().trimmed();
        if (!id.isEmpty() && !models.contains(id))
            models.append(id);
    }
    if (models.isEmpty())
        error = QStringLiteral("API returned no models");
    return models;
}

QString parseSseStreamText(const QByteArray &sseBytes)
{
    QString text;
    // Split on newlines; a trailing partial line simply has no "data: "
    // prefix match or unparsable JSON and is skipped until the next chunk
    // completes it (the caller re-parses the accumulated buffer).
    const QList<QByteArray> lines = sseBytes.split('\n');
    for (QByteArray line : lines) {
        line = line.trimmed();
        if (!line.startsWith("data:"))
            continue;
        const QByteArray payload = line.mid(5).trimmed();
        if (payload.isEmpty() || payload == "[DONE]")
            continue;
        const QJsonDocument doc = QJsonDocument::fromJson(payload);
        if (!doc.isObject())
            continue;
        const QJsonArray choices = doc.object().value(QStringLiteral("choices")).toArray();
        if (choices.isEmpty())
            continue;
        const QJsonObject choice = choices.at(0).toObject();
        // Streaming shape: choices[0].delta.content. Some servers send
        // the full message on the last event instead; take it only when
        // there is no delta, so the same text is never counted twice.
        QString delta = choice.value(QStringLiteral("delta")).toObject()
                            .value(QStringLiteral("content")).toString();
        if (delta.isEmpty())
            delta = choice.value(QStringLiteral("message")).toObject()
                        .value(QStringLiteral("content")).toString();
        text += delta;
    }
    return text;
}

bool parseChatCompletionText(const QByteArray &body, QString &text, QString &error)
{
    const QJsonDocument doc = QJsonDocument::fromJson(body);
    if (!doc.isObject()) {
        error = QStringLiteral("Invalid response from API");
        return false;
    }
    const QJsonArray choices = doc.object().value(QStringLiteral("choices")).toArray();
    if (choices.isEmpty()) {
        error = QStringLiteral("API returned no result");
        return false;
    }
    const QJsonObject choice = choices.at(0).toObject();
    text = choice.value(QStringLiteral("message")).toObject()
               .value(QStringLiteral("content")).toString().trimmed();
    if (text.isEmpty())
        text = choice.value(QStringLiteral("text")).toString().trimmed();
    if (text.isEmpty()) {
        error = QStringLiteral("API returned empty text");
        return false;
    }
    return true;
}

QByteArray buildChatCompletionPayload(const QString &model, const QString &systemPrompt,
                                      const QString &userText, const QString &imageBase64,
                                      bool stream)
{
    QJsonObject systemMessage;
    systemMessage.insert(QStringLiteral("role"), QStringLiteral("system"));
    systemMessage.insert(QStringLiteral("content"), systemPrompt);

    QJsonObject userMessage;
    userMessage.insert(QStringLiteral("role"), QStringLiteral("user"));
    if (imageBase64.isEmpty()) {
        userMessage.insert(QStringLiteral("content"), userText);
    } else {
        QJsonObject textPart;
        textPart.insert(QStringLiteral("type"), QStringLiteral("text"));
        textPart.insert(QStringLiteral("text"), userText);
        QJsonObject imageUrl;
        imageUrl.insert(QStringLiteral("url"),
                        QStringLiteral("data:image/jpeg;base64,") + imageBase64);
        QJsonObject imagePart;
        imagePart.insert(QStringLiteral("type"), QStringLiteral("image_url"));
        imagePart.insert(QStringLiteral("image_url"), imageUrl);
        userMessage.insert(QStringLiteral("content"), QJsonArray{textPart, imagePart});
    }

    QJsonObject requestBody;
    requestBody.insert(QStringLiteral("model"), model);
    requestBody.insert(QStringLiteral("messages"), QJsonArray{systemMessage, userMessage});
    if (stream)
        requestBody.insert(QStringLiteral("stream"), true);
    return QJsonDocument(requestBody).toJson(QJsonDocument::Compact);
}
