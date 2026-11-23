#include "translation/OcrEngine.h"
#include "translation/TranslationHttp.h"

#include <QBuffer>
#include <QByteArray>
#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QList>
#include <QMap>
#include <QProcess>
#include <QStandardPaths>
#include <algorithm>

#if defined(Q_OS_WIN) && __has_include(<winrt/Windows.Media.Ocr.h>)
#define OPENNOW_HAS_WINRT_OCR 1
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Globalization.h>
#include <winrt/Windows.Graphics.Imaging.h>
#include <winrt/Windows.Media.Ocr.h>
#include <winrt/Windows.Storage.Streams.h>
#endif

namespace {
constexpr auto GeminiModel = "gemini-2.5-flash-lite";
constexpr auto GeminiOcrPrompt =
    "Extract all English text visible in this image (dialogue, subtitles, menus, labels), "
    "one block per line of text, in reading order. "
    "Reply with JSON only, no other text: {\"blocks\": [{\"text\": \"...\", \"x\": 0.0, \"y\": 0.0, "
    "\"w\": 0.0, \"h\": 0.0}], \"confidence\": 0.0}. "
    "x, y, w, h are the block's position and size as fractions of the image (0.0-1.0). "
    "If no English text is visible, reply {\"blocks\": [], \"confidence\": 0}.";

QString geminiUrl()
{
    return QStringLiteral("https://generativelanguage.googleapis.com/v1beta/models/")
        + QString::fromLatin1(GeminiModel) + QStringLiteral(":generateContent");
}

QString stripCodeFences(QString raw)
{
    raw = raw.trimmed();
    if (raw.startsWith(QStringLiteral("```"))) {
        const int firstNewline = raw.indexOf(QChar(u'\n'));
        if (firstNewline >= 0)
            raw = raw.mid(firstNewline + 1);
        if (raw.endsWith(QStringLiteral("```")))
            raw.chop(3);
        raw = raw.trimmed();
    }
    return raw;
}

bool parseOcrBlocksJson(const QString &rawText, QString &text, QList<OcrBlock> &blocks,
                        double &confidence, QString &error)
{
    const QString raw = stripCodeFences(rawText);
    const QJsonDocument inner = QJsonDocument::fromJson(raw.toUtf8());
    if (!inner.isObject()) {
        error = QStringLiteral("OCR service returned unparsable text");
        return false;
    }
    const QJsonObject obj = inner.object();
    confidence = obj.value(QStringLiteral("confidence")).toDouble(-1.0);
    blocks.clear();
    const QJsonArray blockArray = obj.value(QStringLiteral("blocks")).toArray();
    for (const QJsonValue &value : blockArray) {
        const QJsonObject block = value.toObject();
        const QString blockText = block.value(QStringLiteral("text")).toString().trimmed();
        if (blockText.isEmpty())
            continue;
        const double x = qBound(0.0, block.value(QStringLiteral("x")).toDouble(0.0), 1.0);
        const double y = qBound(0.0, block.value(QStringLiteral("y")).toDouble(0.0), 1.0);
        const double w = qBound(0.0, block.value(QStringLiteral("w")).toDouble(0.0), 1.0 - x);
        const double h = qBound(0.0, block.value(QStringLiteral("h")).toDouble(0.0), 1.0 - y);
        if (w <= 0.0 || h <= 0.0)
            continue;
        blocks.append(OcrBlock{blockText, QRectF(x, y, w, h)});
    }
    if (blocks.isEmpty()) {
        const QString flat = obj.value(QStringLiteral("text")).toString().trimmed();
        if (!flat.isEmpty())
            blocks.append(OcrBlock{flat, QRectF(0, 0, 1, 1)});
    }
    QStringList lines;
    lines.reserve(blocks.size());
    for (const OcrBlock &block : blocks)
        lines.append(block.text);
    text = lines.join(QChar(u'\n'));
    return true;
}

bool parseGeminiOcrText(const QByteArray &body, QString &text, QList<OcrBlock> &blocks,
                        double &confidence, QString &error)
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
    const QString raw = parts.at(0).toObject().value(QStringLiteral("text")).toString().trimmed();
    return parseOcrBlocksJson(raw, text, blocks, confidence, error);
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
        return OcrResult{OcrResult::Status::NotAvailable, {}, {}, -1.0,
                         QStringLiteral("Gemini API key not set")};
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                         QStringLiteral("Empty image passed to OCR")};

    QByteArray jpeg;
    {
        QBuffer buffer(&jpeg);
        buffer.open(QIODevice::WriteOnly);
        if (!image.convertedTo(QImage::Format_RGB32).save(&buffer, "JPG", 90))
            return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
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

    const TranslationHttpResult result = TranslationHttp::postJson(
        geminiUrl(), payload,
        {{QByteArrayLiteral("x-goog-api-key"), m_apiKey.toUtf8()}});
    if (!result.ok())
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0, result.error};

    QString text;
    QList<OcrBlock> blocks;
    double confidence = -1.0;
    QString error;
    if (!parseGeminiOcrText(result.body, text, blocks, confidence, error))
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0, error};
    return OcrResult{OcrResult::Status::Ok, text, blocks, confidence, {}};
}

CustomVisionOcrEngine::CustomVisionOcrEngine(const CustomApiConfig &config)
    : m_config(config)
{
}

void CustomVisionOcrEngine::setConfig(const CustomApiConfig &config)
{
    m_config = config;
}

OcrResult CustomVisionOcrEngine::recognize(const QImage &image)
{
    if (!m_config.isConfigured())
        return OcrResult{OcrResult::Status::NotAvailable, {}, {}, -1.0,
                         QStringLiteral("Custom API not configured: set base URL, API key and model")};
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                         QStringLiteral("Empty image passed to OCR")};

    QByteArray jpeg;
    {
        QBuffer buffer(&jpeg);
        buffer.open(QIODevice::WriteOnly);
        if (!image.convertedTo(QImage::Format_RGB32).save(&buffer, "JPG", 90))
            return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                             QStringLiteral("Could not encode image for OCR")};
    }

    const QByteArray payload = buildChatCompletionPayload(
        m_config.model, QString::fromLatin1(GeminiOcrPrompt),
        QStringLiteral("Read the text in this image."), QString::fromLatin1(jpeg.toBase64()),
        false);
    const TranslationHttpResult result = TranslationHttp::postJson(
        customApiChatUrl(m_config.baseUrl), payload,
        {{QByteArrayLiteral("Authorization"),
          QByteArrayLiteral("Bearer ") + m_config.apiKey.toUtf8()}});
    if (!result.ok())
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0, result.error};

    QString rawText;
    QString error;
    if (!parseChatCompletionText(result.body, rawText, error))
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0, error};
    QString text;
    QList<OcrBlock> blocks;
    double confidence = -1.0;
    if (!parseOcrBlocksJson(rawText, text, blocks, confidence, error))
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0, error};
    return OcrResult{OcrResult::Status::Ok, text, blocks, confidence, {}};
}

bool TesseractOcrEngine::available() const
{
    return !QStandardPaths::findExecutable(QStringLiteral("tesseract")).isEmpty();
}

OcrResult TesseractOcrEngine::recognize(const QImage &image)
{
    if (!available())
        return OcrResult{OcrResult::Status::NotAvailable, {}, {}, -1.0,
                         QStringLiteral("tesseract binary not found")};
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                         QStringLiteral("Empty image passed to OCR")};

    QByteArray png;
    {
        QBuffer buffer(&png);
        buffer.open(QIODevice::WriteOnly);
        if (!image.save(&buffer, "PNG"))
            return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                             QStringLiteral("Could not encode image for OCR")};
    }
    auto runTesseract = [&png](const QString &mode, QByteArray *out, QString *err) {
        QProcess process;
        process.start(QStringLiteral("tesseract"),
                      {QStringLiteral("stdin"), QStringLiteral("stdout"), mode,
                       QStringLiteral("--psm"), QStringLiteral("6"),
                       QStringLiteral("-l"), QStringLiteral("eng")});
        if (!process.waitForStarted(3000)) {
            *err = QStringLiteral("Could not start tesseract");
            return false;
        }
        process.write(png);
        process.closeWriteChannel();
        if (!process.waitForFinished(8000)) {
            process.kill();
            *err = QStringLiteral("tesseract timed out");
            return false;
        }
        if (process.exitCode() != 0) {
            *err = QString::fromUtf8(process.readAllStandardError()).trimmed();
            return false;
        }
        *out = process.readAllStandardOutput();
        return true;
    };
    QByteArray tsv;
    QString tsvError;
    if (runTesseract(QStringLiteral("tsv"), &tsv, &tsvError)) {
        QList<OcrBlock> blocks;
        QMap<QPair<int, QPair<int, int>>, QList<QPair<QString, QRectF>>> lines;
        const QList<QByteArray> rows = tsv.split('\n');
        for (qsizetype i = 1; i < rows.size(); ++i) {
            const QList<QByteArray> cols = rows.at(i).split('\t');
            if (cols.size() < 12)
                continue;
            bool ok = false;
            const int level = cols.at(0).toInt(&ok);
            if (!ok || level != 5)
                continue;
            const QString word = QString::fromUtf8(cols.at(11)).trimmed();
            if (word.isEmpty())
                continue;
            const int blockNum = cols.at(2).toInt();
            const int parNum = cols.at(3).toInt();
            const int lineNum = cols.at(4).toInt();
            const int left = cols.at(6).toInt();
            const int top = cols.at(7).toInt();
            const int width = cols.at(8).toInt();
            const int height = cols.at(9).toInt();
            if (width <= 0 || height <= 0)
                continue;
            const QRectF rect(qBound(0.0, double(left) / image.width(), 1.0),
                              qBound(0.0, double(top) / image.height(), 1.0),
                              qBound(0.0, double(width) / image.width(), 1.0),
                              qBound(0.0, double(height) / image.height(), 1.0));
            lines[qMakePair(blockNum, qMakePair(parNum, lineNum))]
                .append(qMakePair(word, rect));
        }
        for (auto it = lines.constBegin(); it != lines.constEnd(); ++it) {
            QStringList words;
            QRectF united;
            for (const auto &wordRect : it.value()) {
                words.append(wordRect.first);
                united = united.united(wordRect.second);
            }
            if (!words.isEmpty() && united.isValid())
                blocks.append(OcrBlock{words.join(QChar(u' ')), united});
        }
        if (!blocks.isEmpty()) {
            QStringList texts;
            texts.reserve(blocks.size());
            for (const OcrBlock &block : blocks)
                texts.append(block.text);
            return OcrResult{OcrResult::Status::Ok, texts.join(QChar(u'\n')), blocks, -1.0, {}};
        }
    }
    QByteArray plain;
    QString plainError;
    if (!runTesseract(QStringLiteral("txt"), &plain, &plainError))
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0, plainError};
    const QString text = QString::fromUtf8(plain).trimmed();
    QList<OcrBlock> blocks;
    if (!text.isEmpty())
        blocks.append(OcrBlock{text, QRectF(0, 0, 1, 1)});
    return OcrResult{OcrResult::Status::Ok, text, blocks, -1.0, {}};
}

OcrResult RapidOcrEngine::recognize(const QImage &)
{
    return OcrResult{OcrResult::Status::NotAvailable, {}, {}, -1.0,
                     QStringLiteral("RapidOCR is not available in this build: "
                                    "ONNX Runtime is not vendored")};
}

bool WindowsOcrEngine::available() const
{
#if defined(OPENNOW_HAS_WINRT_OCR)
    // In offscreen unit-test mode we keep WindowsOcrEngine deterministic and
    // avoid probing OS language packs unless explicitly called.
    if (qEnvironmentVariable("QT_QPA_PLATFORM") == QStringLiteral("offscreen"))
        return false;
    try {
        try {
            winrt::init_apartment(winrt::apartment_type::multi_threaded);
        } catch (const winrt::hresult_error &) {
        }
        using namespace winrt::Windows::Globalization;
        using namespace winrt::Windows::Media::Ocr;
        auto engine = OcrEngine::TryCreateFromLanguage(Language(L"en-US"));
        if (!engine)
            engine = OcrEngine::TryCreateFromUserProfileLanguages();
        return static_cast<bool>(engine);
    } catch (...) {
        return false;
    }
#else
    return false;
#endif
}

OcrResult WindowsOcrEngine::recognize(const QImage &image)
{
#if defined(OPENNOW_HAS_WINRT_OCR)
    if (qEnvironmentVariable("QT_QPA_PLATFORM") == QStringLiteral("offscreen")) {
        return OcrResult{OcrResult::Status::NotAvailable, {}, {}, -1.0,
                         QStringLiteral("Windows OCR is disabled in offscreen unit-test mode")};
    }
    if (image.isNull() || image.width() <= 0 || image.height() <= 0)
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                         QStringLiteral("Empty image passed to Windows OCR")};
    try {
        try {
            winrt::init_apartment(winrt::apartment_type::multi_threaded);
        } catch (const winrt::hresult_error &) {
        }
        using namespace winrt::Windows::Globalization;
        using namespace winrt::Windows::Graphics::Imaging;
        using namespace winrt::Windows::Media::Ocr;
        using namespace winrt::Windows::Storage::Streams;

        auto engine = OcrEngine::TryCreateFromLanguage(Language(L"en-US"));
        if (!engine)
            engine = OcrEngine::TryCreateFromUserProfileLanguages();
        if (!engine)
            return OcrResult{OcrResult::Status::NotAvailable, {}, {}, -1.0,
                             QStringLiteral("Windows English OCR language pack is not installed")};

        const QImage bgra = image.convertedTo(QImage::Format_ARGB32);
        const int w = bgra.width();
        const int h = bgra.height();
        const uint32_t byteCount = static_cast<uint32_t>(bgra.sizeInBytes());

        DataWriter writer;
        const auto *rawPtr = reinterpret_cast<const uint8_t *>(bgra.constBits());
        writer.WriteBytes(winrt::array_view<const uint8_t>(rawPtr, rawPtr + byteCount));
        IBuffer buffer = writer.DetachBuffer();

        SoftwareBitmap bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            buffer, BitmapPixelFormat::Bgra8, w, h, BitmapAlphaMode::Ignore);

        auto ocrResult = engine.RecognizeAsync(bitmap).get();
        QList<OcrBlock> blocks;
        QStringList lineTexts;
        const double invW = 1.0 / std::max(1, w);
        const double invH = 1.0 / std::max(1, h);

        for (const auto &line : ocrResult.Lines()) {
            const QString lineText = QString::fromWCharArray(line.Text().c_str()).trimmed();
            if (lineText.isEmpty())
                continue;
            QRectF united;
            for (const auto &word : line.Words()) {
                const auto r = word.BoundingRect();
                const double x = qBound(0.0, static_cast<double>(r.X) * invW, 1.0);
                const double y = qBound(0.0, static_cast<double>(r.Y) * invH, 1.0);
                const double rw = qBound(0.0, static_cast<double>(r.Width) * invW, 1.0 - x);
                const double rh = qBound(0.0, static_cast<double>(r.Height) * invH, 1.0 - y);
                if (rw > 0.0 && rh > 0.0) {
                    const QRectF wr(x, y, rw, rh);
                    united = united.isNull() ? wr : united.united(wr);
                }
            }
            if (!united.isNull() && united.width() > 0.0 && united.height() > 0.0) {
                blocks.append(OcrBlock{lineText, united});
                lineTexts.append(lineText);
            }
        }
        return OcrResult{OcrResult::Status::Ok, lineTexts.join(QChar(u'\n')), blocks, 1.0, {}};
    } catch (const winrt::hresult_error &ex) {
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                         QString::fromWCharArray(ex.message().c_str())};
    } catch (...) {
        return OcrResult{OcrResult::Status::Failed, {}, {}, -1.0,
                         QStringLiteral("Windows OCR failed with an unexpected error")};
    }
#else
    Q_UNUSED(image)
    return OcrResult{OcrResult::Status::NotAvailable, {}, {}, -1.0,
                     QStringLiteral("Windows OCR is only available on Windows")};
#endif
}

void sampleOcrBlockColors(const QImage &cropImage, const QRectF &normalizedBlockRect,
                          QString &fgColorOut, QString &bgColorOut)
{
    fgColorOut = QStringLiteral("#F4F6FA");
    bgColorOut = QStringLiteral("#0B0D14");
    if (cropImage.isNull() || cropImage.width() <= 0 || cropImage.height() <= 0)
        return;

    const int imgW = cropImage.width();
    const int imgH = cropImage.height();
    const int x0 = qBound(0, static_cast<int>(normalizedBlockRect.x() * imgW), imgW - 1);
    const int y0 = qBound(0, static_cast<int>(normalizedBlockRect.y() * imgH), imgH - 1);
    const int x1 = qBound(x0 + 1, static_cast<int>((normalizedBlockRect.x() + normalizedBlockRect.width()) * imgW), imgW);
    const int y1 = qBound(y0 + 1, static_cast<int>((normalizedBlockRect.y() + normalizedBlockRect.height()) * imgH), imgH);

    const int stepX = std::max(1, (x1 - x0) / 32);
    const int stepY = std::max(1, (y1 - y0) / 10);

    qint64 fgR = 0, fgG = 0, fgB = 0, fgCount = 0;
    qint64 bgR = 0, bgG = 0, bgB = 0, bgCount = 0;

    for (int y = y0; y < y1; y += stepY) {
        for (int x = x0; x < x1; x += stepX) {
            const QRgb px = cropImage.pixel(x, y);
            const int r = qRed(px);
            const int g = qGreen(px);
            const int b = qBlue(px);
            const int luma = (r * 77 + g * 150 + b * 29) >> 8;
            if (luma >= 165) {
                fgR += r;
                fgG += g;
                fgB += b;
                ++fgCount;
            } else if (luma <= 95) {
                bgR += r;
                bgG += g;
                bgB += b;
                ++bgCount;
            }
        }
    }

    if (fgCount > 0) {
        const int r = static_cast<int>(fgR / fgCount);
        const int g = static_cast<int>(fgG / fgCount);
        const int b = static_cast<int>(fgB / fgCount);
        // Detect warm golden / yellow highlights (NPC names, quest titles, key terms)
        if (r >= 185 && g >= 145 && (r - b) >= 42) {
            fgColorOut = QStringLiteral("#FFE066");
        } else if (std::max({r, g, b}) - std::min({r, g, b}) >= 55) {
            QColor c(r, g, b);
            c = c.lighter(115);
            fgColorOut = c.name(QColor::HexRgb);
        } else {
            fgColorOut = QStringLiteral("#F4F6FA");
        }
    }

    if (bgCount > 0) {
        // Darken the sampled background so the dark inpaint hides the bright
        // original English glyphs while keeping the local ambient color cast.
        const int r = qBound(6, static_cast<int>(bgR / bgCount) / 3, 24);
        const int g = qBound(7, static_cast<int>(bgG / bgCount) / 3, 26);
        const int b = qBound(10, static_cast<int>(bgB / bgCount) / 3, 34);
        bgColorOut = QColor(r, g, b).name(QColor::HexRgb);
    }
}

std::unique_ptr<OcrEngine> createOcrEngine(const QString &id, const QString &geminiApiKey,
                                           const CustomApiConfig &customApi)
{
    if (id == QStringLiteral("gemini-vision"))
        return std::make_unique<GeminiVisionOcrEngine>(geminiApiKey);
    if (id == QStringLiteral("custom-vision"))
        return std::make_unique<CustomVisionOcrEngine>(customApi);
    if (id == QStringLiteral("tesseract"))
        return std::make_unique<TesseractOcrEngine>();
    if (id == QStringLiteral("rapidocr"))
        return std::make_unique<RapidOcrEngine>();
    if (id == QStringLiteral("windows-ocr"))
        return std::make_unique<WindowsOcrEngine>();
    return nullptr;
}
