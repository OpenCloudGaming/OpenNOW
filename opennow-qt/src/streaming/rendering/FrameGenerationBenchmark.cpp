#include "streaming/rendering/FrameGenerationBenchmark.h"
#include "streaming/rendering/StreamFrameInterpolator.h"

#include <QDateTime>
#include <QElapsedTimer>
#include <QImage>
#include <QScreen>
#include <QThread>
#include <QVariantList>
#include <QWindow>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
#include <QVersionNumber>
#include <QVulkanInstance>
#endif

#include <algorithm>
#include <array>
#include <cmath>
#include <memory>
#include <vector>

namespace {
constexpr int kSourceFrames = 8;        // ring of uploaded frames with real motion between them
constexpr int kWarmupFrames = 6;        // pipeline creation and first dispatches are not timed
constexpr int kMaximumFrames = 360;
constexpr double kGpuHeadroom = 0.6;    // share of a source interval generation may use
constexpr double kDisplayTolerance = 0.975;

// Textured background with slow colour drift, a flat band and an occluding block, all
// translating between frames. Flat areas and occlusions are the cases a block-matching
// engine refuses, so timing them keeps the measurement honest.
QImage sceneFrame(int width, int height, int shift)
{
    QImage image(width, height, QImage::Format_RGBA8888);
    const int bandTop = height * 11 / 20, bandBottom = height * 3 / 4;
    const int blockX = width / 4 + shift * 3, blockY = height / 3;
    const int blockHalfW = std::max(8, width / 12), blockHalfH = std::max(8, height / 10);
    for (int y = 0; y < height; ++y) {
        auto *row = image.scanLine(y);
        for (int x = 0; x < width; ++x) {
            const double px = x - shift;
            int r = int(128 + 52 * std::sin(px * 0.091 + y * 0.033) + 48 * std::cos(px * 0.047 - y * 0.077));
            int g = int(128 + 53 * std::cos(px * 0.067 + y * 0.059) + 42 * std::sin(px * 0.029 - y * 0.113));
            int b = int(128 + 51 * std::sin(px * 0.053 - y * 0.043) + 43 * std::cos(px * 0.107 + y * 0.023));
            if (y >= bandTop && y < bandBottom) {
                r = 90 + int(40.0 * (px / std::max(1, width)));
                g = 120;
                b = 160;
            }
            if (std::abs(x - blockX) < blockHalfW && std::abs(y - blockY) < blockHalfH) {
                r = 230; g = 70; b = 40;
            }
            row[x * 4] = uchar(std::clamp(r, 0, 255));
            row[x * 4 + 1] = uchar(std::clamp(g, 0, 255));
            row[x * 4 + 2] = uchar(std::clamp(b, 0, 255));
            row[x * 4 + 3] = 255;
        }
    }
    return image;
}

double percentile(std::vector<double> values, double fraction)
{
    if (values.empty()) return 0.0;
    std::sort(values.begin(), values.end());
    const auto index = std::clamp<size_t>(size_t(std::llround(fraction * double(values.size() - 1))),
                                          0, values.size() - 1);
    return values[index];
}

QVariantMap failure(const QString &error)
{
    return {{QStringLiteral("ok"), false}, {QStringLiteral("error"), error}};
}

// A private QRhi on the backend OpenNOW forces for this platform (D3D11, Metal, Vulkan).
// `instanceOut` keeps the Vulkan instance alive for the QRhi's lifetime where needed.
struct PrivateRhi {
    std::unique_ptr<QRhi> rhi;
#if !defined(Q_OS_WIN) && !QT_CONFIG(metal) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    std::unique_ptr<QVulkanInstance> instance;
#endif
    QString error;
};

PrivateRhi createPrivateRhi()
{
    PrivateRhi out;
#if defined(Q_OS_WIN)
    QRhiD3D11InitParams params;
    out.rhi.reset(QRhi::create(QRhi::D3D11, &params));
#elif QT_CONFIG(metal)
    QRhiMetalInitParams params;
    out.rhi.reset(QRhi::create(QRhi::Metal, &params));
#elif QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    out.instance = std::make_unique<QVulkanInstance>();
    out.instance->setApiVersion(QVersionNumber(1, 1));
    out.instance->setExtensions(QRhiVulkanInitParams::preferredInstanceExtensions());
    if (!out.instance->create()) {
        out.error = QStringLiteral("Vulkan instance unavailable");
        return out;
    }
    QRhiVulkanInitParams params;
    params.inst = out.instance.get();
    out.rhi.reset(QRhi::create(QRhi::Vulkan, &params));
#endif
    if (!out.rhi && out.error.isEmpty())
        out.error = QStringLiteral("No GPU backend available for the capability test");
    return out;
}
}

FrameGenerationBenchmark::FrameGenerationBenchmark(QObject *parent) : QObject(parent) {}

FrameGenerationBenchmark::~FrameGenerationBenchmark()
{
    cancel();
    if (m_thread) {
        m_thread->wait();
        delete m_thread;
    }
}

bool FrameGenerationBenchmark::start(const QVariantMap &request)
{
    if (m_running) return false;
    const int width = request.value(QStringLiteral("width")).toInt();
    const int height = request.value(QStringLiteral("height")).toInt();
    if (width < 16 || height < 16 || width > 16384 || height > 16384) return false;
    if (m_thread) {
        m_thread->wait();
        delete m_thread;
        m_thread = nullptr;
    }
    m_cancelled = false;
    m_running = true;
    setProgress(0);
    emit runningChanged();
    m_thread = QThread::create([this, request] {
        const auto result = measure(request, &m_cancelled, [this](double value) {
            QMetaObject::invokeMethod(this, [this, value] { setProgress(value); }, Qt::QueuedConnection);
        });
        QMetaObject::invokeMethod(this, [this, result] {
            m_running = false;
            setProgress(1);
            emit runningChanged();
            emit finished(result);
        }, Qt::QueuedConnection);
    });
    m_thread->start();
    return true;
}

void FrameGenerationBenchmark::cancel()
{
    m_cancelled = true;
}

double FrameGenerationBenchmark::displayRefreshRate(QObject *window) const
{
    const auto *native = qobject_cast<QWindow *>(window);
    const auto *screen = native ? native->screen() : nullptr;
    return screen ? screen->refreshRate() : 0.0;
}

int FrameGenerationBenchmark::engineMaxFactor() const
{
    return StreamFrameInterpolator::MaximumFactor;
}

QString FrameGenerationBenchmark::gpuName()
{
    if (!m_gpuNameResolved) {
        m_gpuNameResolved = true;
        auto device = createPrivateRhi();
        if (device.rhi) m_gpuName = QString::fromUtf8(device.rhi->driverInfo().deviceName).trimmed();
    }
    return m_gpuName;
}

void FrameGenerationBenchmark::setProgress(double value)
{
    value = std::clamp(value, 0.0, 1.0);
    if (m_progress == value) return;
    m_progress = value;
    emit progressChanged();
}

QVariantMap FrameGenerationBenchmark::measure(const QVariantMap &request,
                                               const std::atomic_bool *cancelled,
                                               const std::function<void(double)> &progress)
{
    const int width = request.value(QStringLiteral("width")).toInt();
    const int height = request.value(QStringLiteral("height")).toInt();
    const double seconds = std::clamp(request.value(QStringLiteral("seconds"), 3.0).toDouble(), 0.5, 15.0);
    const QString engineName = request.value(QStringLiteral("engine"), QStringLiteral("motion")).toString();

    auto device = createPrivateRhi();
    if (!device.rhi) return failure(device.error);
    auto &rhi = device.rhi;

    StreamFrameInterpolator engine;
    if (!engine.initialize(rhi.get()))
        return failure(QStringLiteral("The frame-generation engine cannot run on this GPU backend"));

    std::array<std::unique_ptr<QRhiTexture>, kSourceFrames> sources;
    {
        QRhiCommandBuffer *cb = nullptr;
        if (rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess)
            return failure(QStringLiteral("Could not begin an offscreen GPU frame"));
        auto *updates = rhi->nextResourceUpdateBatch();
        for (int index = 0; index < kSourceFrames; ++index) {
            sources[index].reset(rhi->newTexture(QRhiTexture::RGBA8, QSize(width, height), 1,
                                                 QRhiTexture::UsedAsTransferSource));
            if (!sources[index]->create()) return failure(QStringLiteral("Could not allocate test frames"));
            updates->uploadTexture(sources[index].get(), sceneFrame(width, height, index * 6));
        }
        cb->resourceUpdate(updates);
        rhi->endOffscreenFrame();
    }

    // GPU timestamps are preferred, but some backends report nothing for offscreen frames
    // (D3D11 returns 0); endOffscreenFrame() blocks until the GPU is done, so the wall
    // time around it is a conservative per-frame cost on those.
    const bool timestamps = rhi->isFeatureSupported(QRhi::Timestamps);
    // Every level is timed on its own: one source frame costs the shared flow plus one
    // synthesis pass per generated frame, so 3x and 4x are measured, not extrapolated.
    constexpr int kFactors = StreamFrameInterpolator::MaximumFactor - 1;
    std::array<std::vector<double>, kFactors> samples;
    std::array<int, kFactors> gpuTimedSamples{};
    const double secondsPerFactor = seconds / double(kFactors);
    QElapsedTimer total;
    total.start();
    for (int factor = 2; factor <= StreamFrameInterpolator::MaximumFactor; ++factor) {
        auto &current = samples[factor - 2];
        current.reserve(kMaximumFrames);
        QElapsedTimer phase;
        phase.start();
        for (int frames = 0; frames < kMaximumFrames; ++frames) {
            if (cancelled && cancelled->load()) return failure(QStringLiteral("cancelled"));
            QRhiCommandBuffer *cb = nullptr;
            if (rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess)
                return failure(QStringLiteral("Could not begin an offscreen GPU frame"));
            QElapsedTimer frame;
            frame.start();
            const bool ingested = engine.ingest(cb, sources[frames % kSourceFrames].get(), factor);
            rhi->endOffscreenFrame();
            const double wallMs = double(frame.nsecsElapsed()) / 1e6;
            if (!ingested) return failure(QStringLiteral("The frame-generation engine rejected the test frames"));
            double ms = timestamps ? cb->lastCompletedGpuTime() * 1000.0 : 0.0;
            const bool fromGpu = ms > 0.0;
            if (!fromGpu) ms = wallMs;
            if (frames >= kWarmupFrames && ms > 0.0) {
                current.push_back(ms);
                if (fromGpu) ++gpuTimedSamples[factor - 2];
            }
            const double elapsed = double(phase.elapsed()) / 1000.0;
            if (progress) progress(std::min(1.0, (double(factor - 2) + std::min(1.0, elapsed / secondsPerFactor)) / kFactors));
            if (elapsed >= secondsPerFactor && current.size() >= 30) break;
        }
        if (current.size() < 10) return failure(QStringLiteral("Not enough GPU samples"));
    }
    engine.release();

    const auto &two = samples[0];
    const bool gpuTimed = gpuTimedSamples[0] == int(two.size());
    double mean = 0.0;
    for (const double value : two) mean += value;
    mean /= double(two.size());
    QVariantMap result;
    result.insert(QStringLiteral("ok"), true);
    result.insert(QStringLiteral("engine"), engineName);
    result.insert(QStringLiteral("engineMaxFactor"), StreamFrameInterpolator::MaximumFactor);
    result.insert(QStringLiteral("backend"), QString::fromLatin1(rhi->backendName()));
    result.insert(QStringLiteral("gpu"), QString::fromUtf8(rhi->driverInfo().deviceName));
    result.insert(QStringLiteral("width"), width);
    result.insert(QStringLiteral("height"), height);
    result.insert(QStringLiteral("frames"), int(two.size()));
    result.insert(QStringLiteral("gpuTimed"), gpuTimed);
    result.insert(QStringLiteral("meanMs"), mean);
    result.insert(QStringLiteral("p50Ms"), percentile(two, 0.5));
    // p95Ms is the 2x cost (one generated frame per source frame); p95Ms3 and p95Ms4 the
    // cost per source frame at 3x and 4x.
    result.insert(QStringLiteral("p95Ms"), percentile(two, 0.95));
    result.insert(QStringLiteral("p95Ms3"), percentile(samples[1], 0.95));
    result.insert(QStringLiteral("p95Ms4"), percentile(samples[2], 0.95));
    result.insert(QStringLiteral("maxMs"), *std::max_element(two.begin(), two.end()));
    result.insert(QStringLiteral("testedAt"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
    result.insert(QStringLiteral("version"), 1);
    return result;
}

QVariantMap FrameGenerationBenchmark::evaluate(const QVariantMap &measurement, int fps, double refreshHz)
{
    const double p95 = measurement.value(QStringLiteral("p95Ms")).toDouble();
    const int engineMax = std::max(2, measurement.value(QStringLiteral("engineMaxFactor"), 2).toInt());
    const double interval = fps > 0 ? 1000.0 / double(fps) : 0.0;
    const double budget = interval * kGpuHeadroom;
    QVariantList allowed;
    QVariantMap costs, reasons;
    for (int factor = 2; factor <= 4; ++factor) {
        // The measured cost per source frame at this level when the profile has it; older
        // profiles only know the 2x pass and scale it by the generated frames.
        const double measured = factor == 2 ? p95
            : measurement.value(QStringLiteral("p95Ms%1").arg(factor)).toDouble();
        const double cost = measured > 0.0 ? measured : p95 * double(factor - 1);
        costs.insert(QString::number(factor), cost);
        QString reason;
        if (factor > engineMax) reason = QStringLiteral("engine");
        else if (fps <= 0 || !measurement.value(QStringLiteral("ok")).toBool()) reason = QStringLiteral("untested");
        else if (cost > budget) reason = QStringLiteral("gpu");
        else if (refreshHz > 0 && refreshHz < double(fps * factor) * kDisplayTolerance) reason = QStringLiteral("display");
        reasons.insert(QString::number(factor), reason);
        if (reason.isEmpty()) allowed.append(factor);
    }
    return {{QStringLiteral("allowed"), allowed},
            {QStringLiteral("budgetMs"), budget},
            {QStringLiteral("costMs"), costs},
            {QStringLiteral("reasons"), reasons}};
}
