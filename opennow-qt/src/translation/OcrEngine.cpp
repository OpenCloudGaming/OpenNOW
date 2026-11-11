#include "translation/OcrEngine.h"
#include "translation/TranslationHttp.h"

#include <QBuffer>
#include <QByteArray>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QStandardPaths>

namespace {
constexpr auto GeminiModel = "gemini-2.5-flash-lite";
constexpr auto GeminiOcrPrompt =
    "Extract only the English dialogue or subtitle text visible in this image. "
    "Reply with JSON only, no other text: {\"text\": \"...\", \"confidence\": 0.0}. "
    "confidence is 0.0-1.0 for how sure you are the text is correct. "
    "If no English dialogue text is visible, reply {\"text\": \"\", \"confidence\": 0}.";

QString geminiUrl()
{
    return QStringLiteral("https://generativelanguage.googleapis.com/v1beta/models/")
        + QString::fromLatin1(GeminiModel) + QStringLiteral(":generateContent");
}

// Parses the model's JSON reply out of the Gemini response envelope.
// Strips ```json fences if the model added them despite the prompt.
bool parseGeminiOcrText(const QByteArray &body, QString &text, double &confidence, QString &error)
{
    const QJsonDocument doc = QJsonDocument::fromJson(body);
    if (!doc.isObject()) {
        error = QStringLiteral("Invalid response from OCR service");
        return false;
    }
    const QJsonArray candidates = doc.object().value(QStringLiteral("candidates")).toArray();
    if (candidates.isEmpty()) {
        error = QStringLiteral("OCR service returned no result");
        return false;
    }
    const QJsonArray parts = candidates.at(0).toObject()
        .value(QStringLiteral("content")).toObject()
        .value(QStringLiteral("parts")).toArray();
    if (parts.isEmpty()) {
        error = QStringLiteral("OCR service returned no text");
        return false;
    }
    QString raw = parts.at(0).toObject().value(QStringLiteral("text")).toString().trimmed();
    if (raw.startsWith(QStringLiteral("```"))) {
        const int firstNewline = raw.indexOf(QChar(u'\n'));
        if (firstNewline >= 0)
            raw = raw.mid(firstNewline + 1);
        if (raw.endsWith(QStringLiteral("```")))
            raw.chop(3);
        raw = raw.trimmed();
    }
    const QJsonDocument inner = QJsonDocument::fromJson(raw.toUtf8());
    if (!inner.isObject()) {
        error = QStringLiteral("OCR service returned unparsable text");
        return false;
    }
    text = inner.object().value(QStringLiteral("text")).toString().trimmed();
    confidence = inner.object().value(QStringLiteral("confidence")).toDouble(-1.0);
    return true;
}
} // namespace

GeminiVisionOcrEngine::GeminiVisionOcrEngine(const QString &apiKey)
    : m_apiKey(apiKey)
{
}

void GeminiVisionOcrEngine::setApiKey(const QString &apiKey)
{
    m_apiKey = apiKey;
}

OcrResult GeminiVisionOcrEngine::recognize(const QImage &image)
{
    if (m_apiKey.isEmpty())
        return OcrResult{OcrResult::Status::NotAvailable, {}, -1.0,
                         QStringLiteral("Gemini API key not set")};
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return OcrResult{OcrResult::Status::Failed, {}, -1.0,
                         QStringLiteral("Empty image passed to OCR")};

    QByteArray jpeg;
    {
        QBuffer buffer(&jpeg);
        buffer.open(QIODevice::WriteOnly);
        // JPEG quality 90: small payload, text stays legible.
        if (!image.convertedTo(QImage::Format_RGB32).save(&buffer, "JPG", 90))
            return OcrResult{OcrResult::Status::Failed, {}, -1.0,
                             QStringLiteral("Could not encode image for OCR")};
    }

    QJsonObject inlineData;
    inlineData.insert(QStringLiteral("mimeType"), QStringLiteral("image/jpeg"));
    inlineData.insert(QStringLiteral("data"), QString::fromLatin1(jpeg.toBase64()));
    QJsonObject imagePart;
    imagePart.insert(QStringLiteral("inlineData"), inlineData);
    QJsonObject textPart;
    textPart.insert(QStringLiteral("text"), QString::fromLatin1(GeminiOcrPrompt));
    QJsonObject content;
    content.insert(QStringLiteral("parts"), QJsonArray{imagePart, textPart});
    QJsonObject requestBody;
    requestBody.insert(QStringLiteral("contents"), QJsonArray{content});
    const QByteArray payload = QJsonDocument(requestBody).toJson(QJsonDocument::Compact);

    // Key travels in the x-goog-api-key header, never in the URL, and is
    // never logged.
    const TranslationHttpResult result = TranslationHttp::postJson(
        geminiUrl(), payload,
        {{QByteArrayLiteral("x-goog-api-key"), m_apiKey.toUtf8()}});
    if (!result.ok())
        return OcrResult{OcrResult::Status::Failed, {}, -1.0, result.error};

    QString text;
    double confidence = -1.0;
    QString error;
    if (!parseGeminiOcrText(result.body, text, confidence, error))
        return OcrResult{OcrResult::Status::Failed, {}, -1.0, error};
    return OcrResult{OcrResult::Status::Ok, text, confidence, {}};
}

bool TesseractOcrEngine::available() const
{
    return !QStandardPaths::findExecutable(QStringLiteral("tesseract")).isEmpty();
}

OcrResult TesseractOcrEngine::recognize(const QImage &image)
{
    if (!available())
        return OcrResult{OcrResult::Status::NotAvailable, {}, -1.0,
                         QStringLiteral("tesseract binary not found")};
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return OcrResult{OcrResult::Status::Failed, {}, -1.0,
                         QStringLiteral("Empty image passed to OCR")};

    QByteArray png;
    {
        QBuffer buffer(&png);
        buffer.open(QIODevice::WriteOnly);
        if (!image.save(&buffer, "PNG"))
            return OcrResult{OcrResult::Status::Failed, {}, -1.0,
                             QStringLiteral("Could not encode image for OCR")};
    }
    QProcess process;
    process.start(QStringLiteral("tesseract"), {QStringLiteral("stdin"), QStringLiteral("stdout"),
                                                QStringLiteral("--psm"), QStringLiteral("6"),
                                                QStringLiteral("-l"), QStringLiteral("eng")});
    if (!process.waitForStarted(3000))
        return OcrResult{OcrResult::Status::Failed, {}, -1.0,
                         QStringLiteral("Could not start tesseract")};
    process.write(png);
    process.closeWriteChannel();
    if (!process.waitForFinished(8000)) {
        process.kill();
        return OcrResult{OcrResult::Status::Failed, {}, -1.0,
                         QStringLiteral("tesseract timed out")};
    }
    if (process.exitCode() != 0)
        return OcrResult{OcrResult::Status::Failed, {}, -1.0,
                         QString::fromUtf8(process.readAllStandardError()).trimmed()};
    // Tesseract reports no per-call confidence on stdout; -1 = unknown.
    return OcrResult{OcrResult::Status::Ok,
                     QString::fromUtf8(process.readAllStandardOutput()).trimmed(), -1.0, {}};
}

OcrResult RapidOcrEngine::recognize(const QImage &)
{
    return OcrResult{OcrResult::Status::NotAvailable, {}, -1.0,
                     QStringLiteral("RapidOCR is not available in this build: "
                                    "ONNX Runtime is not vendored in v1")};
}

OcrResult WindowsOcrEngine::recognize(const QImage &)
{
    return OcrResult{OcrResult::Status::NotAvailable, {}, -1.0,
                     QStringLiteral("Windows OCR is not available: WinRT OCR requires "
                                    "package identity, which this Win32 build does not have")};
}

std::unique_ptr<OcrEngine> createOcrEngine(const QString &id, const QString &geminiApiKey)
{
    if (id == QStringLiteral("gemini-vision"))
        return std::make_unique<GeminiVisionOcrEngine>(geminiApiKey);
    if (id == QStringLiteral("tesseract"))
        return std::make_unique<TesseractOcrEngine>();
    if (id == QStringLiteral("rapidocr"))
        return std::make_unique<RapidOcrEngine>();
    if (id == QStringLiteral("windows-ocr"))
        return std::make_unique<WindowsOcrEngine>();
    return nullptr;
}
