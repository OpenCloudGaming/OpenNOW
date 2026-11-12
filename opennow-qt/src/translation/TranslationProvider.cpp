#include "translation/TranslationProvider.h"
#include "translation/TranslationHttp.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QUrl>
#include <QUrlQuery>

namespace {
constexpr auto GeminiModel = "gemini-2.5-flash-lite";

// Fixed prompt for WuWa story translation. Proper names stay in English,
// Rover keeps the player's chosen pronouns (the model is told not to
// re-gender Rover), and the Vietnamese must read naturally, not word-for-word.
QString geminiTranslatePrompt(const QString &text)
{
    return QStringLiteral(
        "Translate the following English game dialogue from Wuthering Waves into natural, "
        "fluent Vietnamese. Keep character and place names in English (e.g. Rover, Jinzhou, "
        "Sentinel). Do not change Rover's pronouns or gender the character. Do not add "
        "explanations, quotes, or any text besides the translation.\n\n")
        + text;
}

bool parseGeminiTranslateText(const QByteArray &body, QString &text, QString &error)
{
    const QJsonDocument doc = QJsonDocument::fromJson(body);
    if (!doc.isObject()) {
        error = QStringLiteral("Invalid response from translation service");
        return false;
    }
    const QJsonArray candidates = doc.object().value(QStringLiteral("candidates")).toArray();
    if (candidates.isEmpty()) {
        error = QStringLiteral("Translation service returned no result");
        return false;
    }
    const QJsonArray parts = candidates.at(0).toObject()
        .value(QStringLiteral("content")).toObject()
        .value(QStringLiteral("parts")).toArray();
    if (parts.isEmpty()) {
        error = QStringLiteral("Translation service returned no text");
        return false;
    }
    text = parts.at(0).toObject().value(QStringLiteral("text")).toString().trimmed();
    if (text.isEmpty()) {
        error = QStringLiteral("Translation service returned empty text");
        return false;
    }
    return true;
}
} // namespace

TranslateResult GoogleTranslateProvider::translate(const QString &englishText,
                                                   const QString &sourceLanguage,
                                                   const QString &targetLanguage)
{
    if (englishText.trimmed().isEmpty())
        return TranslateResult{false, {}, QStringLiteral("Nothing to translate")};
    QUrl url(QStringLiteral("https://translate.googleapis.com/translate_a/single"));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("client"), QStringLiteral("gtx"));
    query.addQueryItem(QStringLiteral("sl"), sourceLanguage);
    query.addQueryItem(QStringLiteral("tl"), targetLanguage);
    query.addQueryItem(QStringLiteral("dt"), QStringLiteral("t"));
    query.addQueryItem(QStringLiteral("q"), englishText);
    url.setQuery(query);

    const TranslationHttpResult result = TranslationHttp::get(url.toString(QUrl::FullyEncoded));
    if (!result.ok())
        return TranslateResult{false, {}, result.error};

    // Response: [[["translated", "original", ...], ...], ...]. Join the
    // translated segments of the first array.
    const QJsonDocument doc = QJsonDocument::fromJson(result.body);
    if (!doc.isArray() || doc.array().isEmpty() || !doc.array().at(0).isArray())
        return TranslateResult{false, {}, QStringLiteral("Invalid response from translation service")};
    QString translated;
    for (const QJsonValue &segment : doc.array().at(0).toArray()) {
        if (segment.isArray() && !segment.toArray().isEmpty())
            translated += segment.toArray().at(0).toString();
    }
    translated = translated.trimmed();
    if (translated.isEmpty())
        return TranslateResult{false, {}, QStringLiteral("Translation service returned empty text")};
    return TranslateResult{true, translated, {}};
}

GeminiTranslateProvider::GeminiTranslateProvider(const QString &apiKey)
    : m_apiKey(apiKey)
{
}

void GeminiTranslateProvider::setApiKey(const QString &apiKey)
{
    m_apiKey = apiKey;
}

TranslateResult GeminiTranslateProvider::translate(const QString &englishText,
                                                   const QString &sourceLanguage,
                                                   const QString &targetLanguage)
{
    Q_UNUSED(sourceLanguage)
    Q_UNUSED(targetLanguage)
    if (m_apiKey.isEmpty())
        return TranslateResult{false, {}, QStringLiteral("Gemini API key not set")};
    if (englishText.trimmed().isEmpty())
        return TranslateResult{false, {}, QStringLiteral("Nothing to translate")};

    QJsonObject textPart;
    textPart.insert(QStringLiteral("text"), geminiTranslatePrompt(englishText));
    QJsonObject content;
    content.insert(QStringLiteral("parts"), QJsonArray{textPart});
    QJsonObject requestBody;
    requestBody.insert(QStringLiteral("contents"), QJsonArray{content});
    const QByteArray payload = QJsonDocument(requestBody).toJson(QJsonDocument::Compact);

    const QString url = QStringLiteral("https://generativelanguage.googleapis.com/v1beta/models/")
        + QString::fromLatin1(GeminiModel) + QStringLiteral(":generateContent");
    // Key travels in the x-goog-api-key header, never in the URL, and is
    // never logged.
    const TranslationHttpResult result = TranslationHttp::postJson(
        url, payload, {{QByteArrayLiteral("x-goog-api-key"), m_apiKey.toUtf8()}});
    if (!result.ok())
        return TranslateResult{false, {}, result.error};

    QString text;
    QString error;
    if (!parseGeminiTranslateText(result.body, text, error))
        return TranslateResult{false, {}, error};
    return TranslateResult{true, text, {}};
}

std::unique_ptr<TranslationProvider> createTranslationProvider(const QString &id,
                                                               const QString &geminiApiKey)
{
    if (id == QStringLiteral("google"))
        return std::make_unique<GoogleTranslateProvider>();
    if (id == QStringLiteral("gemini"))
        return std::make_unique<GeminiTranslateProvider>(geminiApiKey);
    return nullptr;
}
