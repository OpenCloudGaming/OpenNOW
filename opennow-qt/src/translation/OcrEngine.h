// OcrEngine: English text extraction from a cropped dialogue image.
//
// All implementations run on the translation worker thread (never the GUI
// thread) and are synchronous with an internal timeout; the worker bounds
// how many can run at once. `confidence` is 0..1 when the engine reports
// one, -1 when it does not (e.g. Tesseract).
//
// Implementations:
//  - GeminiVisionOcrEngine: REAL, Gemini 2.5 Flash-Lite vision, Qt Network.
//    API key is passed in (never logged, never stored in code).
//  - TesseractOcrEngine: REAL, optional `tesseract` binary via QProcess.
//    Gracefully unavailable when the binary is missing.
//  - RapidOcrEngine: STUB (NotAvailable). ONNX Runtime is not vendored in
//    v1: bundling it would break upstream CI and add a large native
//    dependency. The interface is ready; a later version can drop in a
//    RapidOCR PP-OCRv4 ONNX implementation behind this same class.
//  - WindowsOcrEngine: STUB (NotAvailable). WinRT Windows.Media.Ocr
//    requires package identity (MSIX/AppContainer); OpenNOW ships as a
//    classic Win32 executable, so the API cannot be activated.
#pragma once

#include <QString>
#include <memory>

class QImage;

struct OcrResult
{
    enum class Status { Ok, NotAvailable, Failed };
    Status status = Status::Failed;
    QString text;       // English dialogue text, "" when none found
    double confidence = -1.0; // 0..1, or -1 when the engine reports none
    QString error;      // human-readable, safe to show; never contains a key
};

class OcrEngine
{
public:
    virtual ~OcrEngine() = default;
    virtual QString id() const = 0;
    virtual QString displayName() const = 0;
    // False means recognize() will return NotAvailable without trying.
    virtual bool available() const = 0;
    virtual OcrResult recognize(const QImage &image) = 0;
};

class GeminiVisionOcrEngine final : public OcrEngine
{
public:
    explicit GeminiVisionOcrEngine(const QString &apiKey = {});
    void setApiKey(const QString &apiKey); // key is held in memory only
    QString id() const override { return QStringLiteral("gemini-vision"); }
    QString displayName() const override { return QStringLiteral("Gemini Vision"); }
    bool available() const override { return !m_apiKey.isEmpty(); }
    OcrResult recognize(const QImage &image) override;

private:
    QString m_apiKey; // never logged, never written to disk by this class
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
    QString displayName() const override { return QStringLiteral("Windows OCR"); }
    bool available() const override { return false; }
    OcrResult recognize(const QImage &) override;
};

// Factory. `geminiApiKey` is forwarded only to GeminiVisionOcrEngine.
// Returns nullptr for an unknown id.
std::unique_ptr<OcrEngine> createOcrEngine(const QString &id, const QString &geminiApiKey = {});
