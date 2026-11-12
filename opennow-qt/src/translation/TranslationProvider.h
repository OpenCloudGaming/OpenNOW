// TranslationProvider: English -> Vietnamese translation.
//
// Both implementations run on the translation worker thread (never the GUI
// thread) and are synchronous with an internal timeout (see
// TranslationHttp::NetworkTimeoutMs, 8s, single 429 retry).
//
//  - GoogleTranslateProvider: REAL, free unauthenticated endpoint
//    (translate.googleapis.com/translate_a/single?client=gtx). No key.
//  - GeminiTranslateProvider: REAL, Gemini 2.5 Flash-Lite with a fixed
//    Wuthering Waves prompt (keeps proper names, Rover pronouns, natural
//    Vietnamese). Key is passed in, never logged, never stored in code.
#pragma once

#include <QString>
#include <memory>

struct TranslateResult
{
    bool ok = false;
    QString text;  // Vietnamese translation on success
    QString error; // human-readable, safe to show; never contains a key
};

class TranslationProvider
{
public:
    virtual ~TranslationProvider() = default;
    virtual QString id() const = 0;
    virtual QString displayName() const = 0;
    virtual TranslateResult translate(const QString &englishText,
                                      const QString &sourceLanguage,
                                      const QString &targetLanguage) = 0;
};

class GoogleTranslateProvider final : public TranslationProvider
{
public:
    QString id() const override { return QStringLiteral("google"); }
    QString displayName() const override { return QStringLiteral("Google Translate"); }
    TranslateResult translate(const QString &englishText,
                              const QString &sourceLanguage,
                              const QString &targetLanguage) override;
};

class GeminiTranslateProvider final : public TranslationProvider
{
public:
    explicit GeminiTranslateProvider(const QString &apiKey = {});
    void setApiKey(const QString &apiKey); // key is held in memory only
    QString id() const override { return QStringLiteral("gemini"); }
    QString displayName() const override { return QStringLiteral("Gemini"); }
    TranslateResult translate(const QString &englishText,
                              const QString &sourceLanguage,
                              const QString &targetLanguage) override;

private:
    QString m_apiKey; // never logged, never written to disk by this class
};

// Factory. `geminiApiKey` is forwarded only to GeminiTranslateProvider.
// Returns nullptr for an unknown id.
std::unique_ptr<TranslationProvider> createTranslationProvider(const QString &id,
                                                               const QString &geminiApiKey = {});
