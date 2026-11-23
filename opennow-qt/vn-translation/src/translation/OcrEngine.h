// OcrEngine: English text extraction from a cropped stream frame.
//
// All implementations run on the translation worker thread (never the GUI
// thread) and are synchronous with an internal timeout.
//
// Implementations:
//  - WindowsOcrEngine: REAL on Windows 10/11 via C++/WinRT Windows.Media.Ocr
//    (DualApiPartition — works in classic Win32 desktop apps without MSIX
//    package identity). Runs 100% offline in ~10-20ms with pixel-accurate
//    per-word/per-line bounding boxes.
//  - GeminiVisionOcrEngine: REAL, Gemini 2.5 Flash-Lite vision, Qt Network.
//  - CustomVisionOcrEngine: REAL, any OpenAI-compatible vision endpoint.
//  - TesseractOcrEngine: REAL, optional `tesseract` binary via QProcess (TSV).
//  - RapidOcrEngine: STUB (NotAvailable until ONNX Runtime is vendored).
#pragma once

#include <QList>
#include <QMetaType>
#include <QRectF>
#include <QString>
#include <memory>

#include "translation/CustomApi.h"

class QImage;

struct OcrBlock
{
    QString text;
    QRectF rect;
};
Q_DECLARE_METATYPE(OcrBlock)

struct OcrResult
{
    enum class Status { Ok, NotAvailable, Failed };
    Status status = Status::Failed;
    QString text;
    QList<OcrBlock> blocks;
    double confidence = -1.0;
    QString error;
};

class OcrEngine
{
public:
    virtual ~OcrEngine() = default;
    virtual QString id() const = 0;
    virtual QString displayName() const = 0;
    virtual bool available() const = 0;
    virtual OcrResult recognize(const QImage &image) = 0;
};

class GeminiVisionOcrEngine final : public OcrEngine
{
public:
    explicit GeminiVisionOcrEngine(const QString &apiKey = {});
    void setApiKey(const QString &apiKey);
    QString id() const override { return QStringLiteral("gemini-vision"); }
    QString displayName() const override { return QStringLiteral("Gemini Vision"); }
    bool available() const override { return !m_apiKey.isEmpty(); }
    OcrResult recognize(const QImage &image) override;

private:
    QString m_apiKey;
};

class CustomVisionOcrEngine final : public OcrEngine
{
public:
    explicit CustomVisionOcrEngine(const CustomApiConfig &config = {});
    void setConfig(const CustomApiConfig &config);
    QString id() const override { return QStringLiteral("custom-vision"); }
    QString displayName() const override { return QStringLiteral("Custom API (Vision)"); }
    bool available() const override { return m_config.isConfigured(); }
    OcrResult recognize(const QImage &image) override;

private:
    CustomApiConfig m_config;
};

class TesseractOcrEngine final : public OcrEngine
{
public:
    QString id() const override { return QStringLiteral("tesseract"); }
    QString displayName() const override { return QStringLiteral("Tesseract"); }
    bool available() const override;
    OcrResult recognize(const QImage &image) override;
};

class RapidOcrEngine final : public OcrEngine
{
public:
    QString id() const override { return QStringLiteral("rapidocr"); }
    QString displayName() const override { return QStringLiteral("RapidOCR (ONNX)"); }
    bool available() const override { return false; }
    OcrResult recognize(const QImage &) override;
};

class WindowsOcrEngine final : public OcrEngine
{
public:
    QString id() const override { return QStringLiteral("windows-ocr"); }
    QString displayName() const override { return QStringLiteral("Windows Native OCR"); }
    bool available() const override;
    OcrResult recognize(const QImage &image) override;
};

// Samples the dominant text foreground color (e.g. off-white #F4F6FA or warm
// gold #FFE066) and dark inpaint background color (e.g. #0C0E14) from a
// normalized block rectangle inside `cropImage`. Runs in < 0.02ms on CPU.
void sampleOcrBlockColors(const QImage &cropImage, const QRectF &normalizedBlockRect,
                          QString &fgColorOut, QString &bgColorOut);

std::unique_ptr<OcrEngine> createOcrEngine(const QString &id, const QString &geminiApiKey = {},
                                           const CustomApiConfig &customApi = {});
