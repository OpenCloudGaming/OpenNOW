// CustomApi: OpenAI-compatible custom endpoint.
//
// The user pastes ANY provider's base URL + API key (OpenAI, OpenRouter,
// Groq, DeepSeek, Together, xAI, a self-hosted llama.cpp/vLLM server,
// ...), the app lists that base's real models via GET {base}/models, and
// the user picks one. No provider is hardcoded and no model list is
// bundled — the list always comes from the user's own base + key.
//
// All helpers here are pure (no network, no threads): the network calls
// live in TranslationHttp / TranslationWorker on the worker thread, and
// these functions are what the unit tests exercise.
#pragma once

#include <QByteArray>
#include <QMetaType>
#include <QString>
#include <QStringList>

// Base URL + key + model for one custom endpoint. The key is held in
// memory only by callers; this struct never logs it.
struct CustomApiConfig
{
    QString baseUrl; // e.g. https://api.openai.com/v1 (no trailing slash)
    QString apiKey;  // Bearer token
    QString model;   // e.g. gpt-4o-mini
    bool isConfigured() const
    {
        return !baseUrl.isEmpty() && !apiKey.isEmpty() && !model.isEmpty();
    }
};
Q_DECLARE_METATYPE(CustomApiConfig)

// Trims whitespace, strips a trailing "/" and a trailing
// "/chat/completions" (users often paste the full chat URL). "" in -> "".
QString normalizeCustomApiBase(const QString &base);

// Normalized base + "/models" (the OpenAI-compatible list endpoint).
// "" when the base is empty.
QString customApiModelsUrl(const QString &base);

// Normalized base + "/chat/completions".
// "" when the base is empty.
QString customApiChatUrl(const QString &base);

// Parses a GET {base}/models reply: {"data": [{"id": "model-a"}, ...]}.
// Returns the model ids in server order (duplicates removed); on a bad
// body returns an empty list and sets `error`.
QStringList parseCustomApiModels(const QByteArray &body, QString &error);

// Parses a (possibly partial) SSE stream body: lines of the form
// "data: {\"choices\": [{\"delta\": {\"content\": \"...\"}}]}", ending
// with "data: [DONE]". Returns the concatenated delta content. A chunk
// split mid-line is fine — this parses the ACCUMULATED buffer each time,
// so callers re-run it on the whole buffer after every chunk. Lines that
// are not data events, "[DONE]", or unparsable JSON are skipped.
QString parseSseStreamText(const QByteArray &sseBytes);

// Parses a non-stream chat completion reply:
// {"choices": [{"message": {"content": "..."}}]} (also accepts the legacy
// {"choices": [{"text": "..."}]} shape). Returns false with `error` set
// on a bad body or an empty content string.
bool parseChatCompletionText(const QByteArray &body, QString &text, QString &error);

// Builds the chat/completions JSON payload. `imageBase64` empty means a
// text-only call (user content is a plain string); non-empty means a
// vision call (user content is [{type: "text", ...}, {type: "image_url",
// url: "data:image/jpeg;base64,..."}]). `stream` adds "stream": true.
QByteArray buildChatCompletionPayload(const QString &model, const QString &systemPrompt,
                                      const QString &userText, const QString &imageBase64,
                                      bool stream);
