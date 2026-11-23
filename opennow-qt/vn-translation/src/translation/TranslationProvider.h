// TranslationProvider: English -> Vietnamese translation.
//
// All implementations run on the translation worker thread (never the GUI
// thread) and are synchronous with an internal timeout (see
// TranslationHttp::NetworkTimeoutMs, 8s, single 429 retry).
//
//  - GoogleTranslateProvider: REAL, free unauthenticated endpoint
//    (translate.googleapis.com/translate_a/single?client=gtx). No key.
//  - GeminiTranslateProvider: REAL, Gemini 3.5 Flash-Lite with a fixed
//    Wuthering Waves prompt (keeps proper names, Rover pronouns, natural
//    Vietnamese). Key is passed in, never logged, never stored in code.
//  - CustomApiTranslateProvider: REAL, any OpenAI-compatible base the
//    user pastes (OpenAI, OpenRouter, Groq, DeepSeek, ...). The model
//    list comes from GET {base}/models with the user's key, and the
//    translation streams (SSE) so partial text shows while the model is
//    still generating instead of waiting for the whole reply.
#pragma once

#include <QString>
#include <functional>
#include <memory>

#include "translation/CustomApi.h"

struct TranslateResult
{
    bool ok = false;
    QString text;  // Vietnamese translation on success
    QString error; // human-readable, safe to show; never contains a key
};

// Called with the progressively translated text while a streaming
// provider is still generating. The final text is always delivered
// through translateStreaming's return value as well.
using TranslateChunkCallback = std::function<void(const QString &partialText)>;

class TranslationProvider
{
public:
    virtual ~TranslationProvider() = default;
    virtual QString id() const = 0;
    virtual QString displayName() const = 0;
    virtual TranslateResult translate(const QString &englishText,
                                      const QString &sourceLanguage,
                                      const QString &targetLanguage) = 0;
    // Streaming translation. The default implementation calls
    // translate() once and reports the whole text as a single chunk, so
    // non-streaming providers work unchanged through this path.
    virtual TranslateResult translateStreaming(const QString &englishText,
                                               const QString &sourceLanguage,
                                               const QString &targetLanguage,
                                               const TranslateChunkCallback &onChunk);
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

class CustomApiTranslateProvider final : public TranslationProvider
{
public:
    explicit CustomApiTranslateProvider(const CustomApiConfig &config = {});
    void setConfig(const CustomApiConfig &config); // key held in memory only
    QString id() const override { return QStringLiteral("custom"); }
    QString displayName() const override { return QStringLiteral("Custom API"); }
    TranslateResult translate(const QString &englishText,
                              const QString &sourceLanguage,
                              const QString &targetLanguage) override;
    TranslateResult translateStreaming(const QString &englishText,
                                       const QString &sourceLanguage,
                                       const QString &targetLanguage,
                                       const TranslateChunkCallback &onChunk) override;

private:
    CustomApiConfig m_config; // base + key + model; key never logged
};

// Factory. `geminiApiKey` is forwarded only to GeminiTranslateProvider;
// `customApi` only to CustomApiTranslateProvider.
// Returns nullptr for an unknown id.
std::unique_ptr<TranslationProvider> createTranslationProvider(
    const QString &id, const QString &geminiApiKey = {},
    const CustomApiConfig &customApi = {});
