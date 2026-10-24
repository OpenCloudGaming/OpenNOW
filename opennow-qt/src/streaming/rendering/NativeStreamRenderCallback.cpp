#include "streaming/rendering/NativeStreamRenderCallback.h"
#include "streaming/rendering/HdrOutput.h"

#include "streaming/NativeStreamRuntime.h"
#include "streaming/rendering/LinuxVulkanGraphics.h"
#include "streaming/rendering/StreamPresentTimings.h"
#include "streaming/rendering/StreamSwapStallWatchdog.h"
#include "streaming/rendering/StreamVideoRenderCallback.h"
#include "streaming/rendering/StreamVideoTextureRenderer.h"
#include "streaming/rendering/StreamFrameInterpolator.h"
#include "streaming/rendering/StreamFramePacer.h"

#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>
#include <QDebug>
#include <QImage>

#include <utility>
#include <vector>
#include <memory>
#include <atomic>
#include <chrono>

namespace {
class ExternalCommandScope final
{
public:
    explicit ExternalCommandScope(QRhiCommandBuffer *commandBuffer)
        : m_commandBuffer(commandBuffer)
    {
        m_commandBuffer->beginExternal();
    }

    ~ExternalCommandScope()
    {
        m_commandBuffer->endExternal();
    }

    ExternalCommandScope(const ExternalCommandScope &) = delete;
    ExternalCommandScope &operator=(const ExternalCommandScope &) = delete;

private:
    QRhiCommandBuffer *m_commandBuffer;
};

class NativeStreamRenderCallback final : public StreamVideoRenderCallback
{
    enum class FrameGenerationState { Off, WarmingUp, Active, DisplayTooSlow, Overloaded, Unavailable, Discontinuity, SourceRateLimit, HdrUnsupported };
public:
    explicit NativeStreamRenderCallback(NativeStreamRuntime *runtime)
        : m_runtime(runtime)
    {
    }

    void initialize(QRhi *rhi, QRhiCommandBuffer *commandBuffer,
                    QRhiRenderTarget *renderTarget) override
    {
        const bool ownRearm = std::exchange(m_resourceRearmPending, false);
        const bool deviceChanged = m_rhi != rhi;
        const bool generationChanged = m_runtime
            && m_presentationGeneration != m_runtime->presentationGeneration();
        if (ownRearm || deviceChanged || generationChanged) {
            tearDownResources(ownRearm, deviceChanged, generationChanged);
            if (generationChanged)
                m_presentationGeneration = m_runtime->presentationGeneration();
        }
        m_textures.initialize(rhi, renderTarget);
        if (m_rhi == rhi && m_graphicsReady) return;
        m_rhi = rhi;
        if (!commandBuffer) return;

        OpenNowStreamerGraphicsContext context{};
        context.version = OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION;
        context.struct_size = sizeof(context);
        OpenNowStreamerStatus status = OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        {
            // The native streamer creates decoder/video-processor resources on Qt's adopted
            // graphics device. QRhi requires every external native command to be bracketed so it
            // can invalidate and restore its internal backend state.
            ExternalCommandScope externalCommands(commandBuffer);
            switch (rhi->backend()) {
#if defined(Q_OS_WIN)
            case QRhi::D3D11: {
                const auto *handles = static_cast<const QRhiD3D11NativeHandles *>(
                    rhi->nativeHandles());
                if (!handles) return;
                context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_D3D11;
                context.device = handles->dev;
                context.queue = handles->context;
                break;
            }
#endif
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
            case QRhi::Vulkan: {
                const auto *handles = static_cast<const QRhiVulkanNativeHandles *>(
                    rhi->nativeHandles());
                if (!handles || !handles->inst) return;
                context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_VULKAN;
                context.instance = reinterpret_cast<void *>(handles->inst->vkInstance());
                context.physical_device = reinterpret_cast<void *>(handles->physDev);
                context.device = reinterpret_cast<void *>(handles->dev);
                context.queue = reinterpret_cast<void *>(handles->gfxQueue);
                context.queue_family_index = handles->gfxQueueFamilyIdx;
#if defined(Q_OS_LINUX)
                if (m_runtime && m_runtime->vulkanDevice()) {
                    OpenNowStreamerVulkanDeviceInfo info{};
                    info.version = OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION;
                    info.struct_size = sizeof(info);
                    if (opennow_streamer_vulkan_device_info(m_runtime->vulkanDevice(), &info)
                            != OPENNOW_STREAMER_OK
                            || !LinuxVulkanGraphics::Device::matchesContext(info, context)) {
                        reportFailure(QStringLiteral("Qt is not using the embedded Vulkan Video device. Restart OpenNOW to recreate the shared graphics device."));
                        return;
                    }
                } else {
                    const auto contract = LinuxVulkanGraphics::importContract(
                        handles->inst, handles->physDev);
                    context.enabled_capabilities = contract.capabilities;
                    if (m_runtime) m_runtime->logRenderDiagnostic(contract.summary);
                }
#endif
                break;
            }
#endif
#if QT_CONFIG(metal)
            case QRhi::Metal: {
                const auto *handles = static_cast<const QRhiMetalNativeHandles *>(
                    rhi->nativeHandles());
                if (!handles) return;
                context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_METAL;
                context.device = handles->dev;
                context.queue = handles->cmdQueue;
                break;
            }
#endif
            default:
                reportFailure(QStringLiteral("This graphics backend cannot present native video. Use Auto in Stream settings."));
                return;
            }
            status = m_runtime ? m_runtime->setGraphicsContext(context)
                               : OPENNOW_STREAMER_CLOSED;
        }
        m_graphicsReady = status == OPENNOW_STREAMER_OK;
        if (!m_graphicsReady)
            reportFailure(QStringLiteral("Could not initialize native graphics (status %1). Update the GPU driver and use Auto in Stream settings.").arg(int(status)));
    }

    void prepareFrame(QRhiCommandBuffer *commandBuffer) override
    {
        prepareNativeFrame(commandBuffer);
        if (!m_runtime || !m_runtime->presentationAllowed()
            || m_presentationGeneration != m_runtime->presentationGeneration()
            || !m_graphicsReady || !m_rhi || !commandBuffer) return;
        m_textures.prepareUpscaling(commandBuffer, m_upscalingTarget, m_fsrUpscaling,
            m_sourceColorSpace == OPENNOW_STREAMER_COLOR_SPACE_SDR709, m_upscalingSharpness);
    }

    void prepareNativeFrame(QRhiCommandBuffer *commandBuffer)
    {
        if (!m_runtime || !m_runtime->presentationAllowed()
                || m_presentationGeneration != m_runtime->presentationGeneration()) return;
        if (!m_graphicsReady || !m_rhi || !commandBuffer) return;
        if (m_resetFrameGeneration) {
            resetFrameGeneration();
            m_resetFrameGeneration = false;
        }
        const auto output = HdrOutput::renderState();
        m_textures.setColorSpace(m_sourceColorSpace, output.mode, output.whiteNits, output.supported);
        if (!m_textures.prepare(commandBuffer)) {
            reportFailure(QStringLiteral("Could not create the video shaders or GPU resources. Check the packaged shaders and GPU driver."));
            return;
        }

        OpenNowStreamerRecordCommand command{};
        command.version = OPENNOW_STREAMER_RENDER_COMMAND_VERSION;
        command.struct_size = sizeof(command);
        command.frame_slot = static_cast<std::uint32_t>(m_rhi->currentFrameSlot());
        if (m_rhi->backend() == QRhi::Metal && !m_fsrUpscaling && !m_upscalingTarget.isEmpty()) {
            command.upscale_width = static_cast<std::uint32_t>(m_upscalingTarget.width());
            command.upscale_height = static_cast<std::uint32_t>(m_upscalingTarget.height());
            command.upscale_sharpness = static_cast<std::uint32_t>(m_upscalingSharpness);
            command.upscale_denoise = static_cast<std::uint32_t>(m_upscalingDenoise);
        }
        finishFrame();
        OpenNowStreamerFrameInfo info{};
        OpenNowStreamerRecordedFrame recorded{};
        OpenNowStreamerFrame *frame = nullptr;
        OpenNowStreamerStatus status = OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        {
            ExternalCommandScope externalCommands(commandBuffer);
            // Native command-buffer handles can change when QRhi begins an external section, so
            // query them only after beginExternal(), as required by the QRhi contract.
            switch (m_rhi->backend()) {
#if defined(Q_OS_WIN)
            case QRhi::D3D11: {
                const auto *handles = static_cast<const QRhiD3D11NativeHandles *>(
                    m_rhi->nativeHandles());
                command.command_buffer = handles ? handles->context : nullptr;
                break;
            }
#endif
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
            case QRhi::Vulkan: {
                const auto *handles = static_cast<const QRhiVulkanCommandBufferNativeHandles *>(
                    commandBuffer->nativeHandles());
                command.command_buffer = handles
                    ? reinterpret_cast<void *>(handles->commandBuffer) : nullptr;
                break;
            }
#endif
#if QT_CONFIG(metal)
            case QRhi::Metal: {
                const auto *handles = static_cast<const QRhiMetalCommandBufferNativeHandles *>(
                    commandBuffer->nativeHandles());
                command.command_buffer = handles ? handles->commandBuffer : nullptr;
                break;
            }
#endif
            default:
                return;
            }
            if (!command.command_buffer) return;
            status = m_runtime->recordLatestFrame(command, &info, &recorded, &frame);
        }

        if (m_presentationGeneration != m_runtime->presentationGeneration()
                || !m_runtime->presentationAllowed()) {
            if (frame) m_runtime->releaseFrame(frame);
            m_textures.clearFrames();
            return;
        }
        if (status == OPENNOW_STREAMER_NO_FRAME || status == OPENNOW_STREAMER_STALE_FRAME) {
            prepareNext();
            return;
        }
        if (status == OPENNOW_STREAMER_GRAPHICS_UNSUPPORTED) {
            if (frame) m_runtime->releaseFrame(frame);
            reportFailure(QStringLiteral("OpenNOW's current Vulkan graphics device cannot import the hardware decoder's video frames, so reconnecting would fail the same way. Select Video backend: Software (CPU) in Stream settings, or export diagnostics. Your cloud session is still running."),
                          NativeStreamRuntime::PresentationFailure::Unsupported);
            return;
        }
        if (status != OPENNOW_STREAMER_OK || !frame || recorded.resource == 0
            || recorded.width == 0 || recorded.height == 0
            || (recorded.texture_format != OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8
                && recorded.texture_format != OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2
                && recorded.texture_format != OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA16F)
            || (recorded.color_space != OPENNOW_STREAMER_COLOR_SPACE_SDR709
                && recorded.texture_format == OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8)
            || recorded.color_space > OPENNOW_STREAMER_COLOR_SPACE_HLG2020) {
            if (frame) m_runtime->releaseFrame(frame);
            reportFailure(QStringLiteral("Could not present the decoded GPU frame (status %1). Check native-streamer.log for decoder or device errors.").arg(int(status)));
            return;
        }
        m_preparedFrame = frame;
        if (m_sourceColorSpace != int(recorded.color_space)) {
            resetFrameGeneration();
            m_sourceColorSpace = int(recorded.color_space);
            m_textures.updateColorSpace(commandBuffer, m_sourceColorSpace);
        }

        QRhiTexture::NativeTexture texture{};
        texture.object = recorded.resource;
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        if (recorded.graphics_api == OPENNOW_STREAMER_GRAPHICS_API_VULKAN)
            texture.layout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
#endif
        if (!m_textures.importFrame(command.frame_slot, texture,
            recorded.texture_format == OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2
                ? QRhiTexture::RGB10A2
                : recorded.texture_format == OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA16F
                    ? QRhiTexture::RGBA16F : QRhiTexture::RGBA8,
            QSize(int(recorded.width), int(recorded.height)))) {
            reportFailure(QStringLiteral("Could not import the decoded video texture into Qt. The decoder and graphics backend must use compatible GPU resources."));
            return;
        }
        if (m_reportedColorFormat != recorded.texture_format
                || m_reportedColorSpace != recorded.color_space
                || m_reportedOutputBits != m_textures.outputBits()) {
            m_reportedColorFormat = recorded.texture_format;
            m_reportedColorSpace = recorded.color_space;
            m_reportedOutputBits = m_textures.outputBits();
            qInfo("Video composition: sourceColorSpace=%s textureBits=%u outputBits=%d dither=%s",
                  recorded.color_space == OPENNOW_STREAMER_COLOR_SPACE_PQ2020 ? "PQ2020"
                    : recorded.color_space == OPENNOW_STREAMER_COLOR_SPACE_HLG2020 ? "HLG2020" : "SDR709",
                  recorded.texture_format == OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA16F ? 16u
                    : recorded.texture_format == OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2 ? 10u : 8u,
                  m_reportedOutputBits, m_reportedOutputBits == 8 ? "ordered-8x8" : "none");
        }
        auto *source = m_textures.importedTexture();
        if ((m_frameGenerationFactor >= 2) && (m_historySize != source->pixelSize()
                                  || m_historyFormat != source->format())) {
            resetFrameGeneration();
            m_historySize = source->pixelSize();
            m_historyFormat = source->format();
        }
        m_outputDirty = true;
        m_outputKind = 1;
        if (m_sourceColorSpace != OPENNOW_STREAMER_COLOR_SPACE_SDR709) {
            m_frameGenerationStatus.store(m_frameGenerationFactor >= 2 ? FrameGenerationState::HdrUnsupported : FrameGenerationState::Off);
            return;
        }
        if (m_frameGenerationFactor < 2 || m_frameGenerationFailed) return;

        const auto now = clockNs();
        // The presenter is stalled when nothing new has reached the display for a while;
        // a source frame arriving early on its own is a burst, not overload.
        const auto swaps = m_swapTimings.snapshot();
        const bool presenterStalled = swaps.hasLastSwap && now - swaps.lastSwapNs
            > std::max<std::int64_t>(100'000'000, 4 * std::int64_t(m_pacer.interval()));
        const auto decision = m_pacer.source(info.sequence, info.presentation_time_ns, now,
                                             m_refreshRate, m_frameGenerationFactor, presenterStalled);
        updateTimingStats();
        if (decision == StreamFramePacer::Result::Duplicate) {
            m_outputDirty = false;
            if (m_pacer.pending())
                m_textures.selectTexture(m_interpolator.generatedTexture(m_pacer.pendingIndex()));
            else if (m_interpolator.hasPair())
                m_textures.selectTexture(m_interpolator.currentTexture());
            prepareNext();
            return;
        }
        m_needsFrame.store(false);
        if (decision != StreamFramePacer::Result::Interpolate) {
            m_interpolator.reset();
            switch (decision) {
            case StreamFramePacer::Result::SourceRateLimit: m_frameGenerationStatus.store(FrameGenerationState::SourceRateLimit); return;
            case StreamFramePacer::Result::DisplayTooSlow: m_frameGenerationStatus.store(FrameGenerationState::DisplayTooSlow); return;
            case StreamFramePacer::Result::Overloaded: m_frameGenerationStatus.store(FrameGenerationState::Overloaded); return;
            case StreamFramePacer::Result::Discontinuity: m_frameGenerationStatus.store(FrameGenerationState::Discontinuity); break;
            default: m_frameGenerationStatus.store(FrameGenerationState::WarmingUp); break;
            }
        }
        if (!m_interpolator.initialize(m_rhi)
            || !m_interpolator.ingest(commandBuffer, source, m_pacer.factor())) {
            disableFrameGeneration();
            return;
        }
        if (!m_interpolator.hasPair()) {
            if (decision != StreamFramePacer::Result::Discontinuity)
                m_frameGenerationStatus.store(FrameGenerationState::WarmingUp);
            return;
        }
        // The engine may synthesise fewer frames than requested (the block-matching
        // engine knows only the midpoint); the schedule follows what exists.
        const int generated = std::min(m_interpolator.generatedCount(), m_pacer.factor() - 1);
        m_effectiveFactor.store(generated + 1);
        if (generated < 1 || !m_textures.selectTexture(m_interpolator.generatedTexture(1))) {
            disableFrameGeneration();
            return;
        }
        m_midpointSwapped.store(false);
        m_pacer.generated(clockNs(), generated);
        m_needsFrame.store(true);
        m_outputKind = 2;
        m_frameGenerationStatus.store(FrameGenerationState::Active);
        captureFrameGeneration(commandBuffer, generated);
    }

    // Diagnostics: with OPENNOW_FG_CAPTURE_DIR set, saves the pair and every generated frame
    // as PNG 3 s after generation starts and then every 5 s, six times per session.
    void captureFrameGeneration(QRhiCommandBuffer *commandBuffer, int generated)
    {
        static const QString directory = qEnvironmentVariable("OPENNOW_FG_CAPTURE_DIR");
        if (directory.isEmpty() || m_captureCount >= 6 || !m_rhi) return;
        const auto now = clockNs();
        if (!m_captureDueNs) m_captureDueNs = now + 3'000'000'000;
        if (now < m_captureDueNs) return;
        m_captureDueNs = now + 5'000'000'000;
        const int capture = ++m_captureCount;
        m_captureResults.clear();
        std::vector<std::pair<QString, QRhiTexture *>> frames{
            {QStringLiteral("previous"), m_interpolator.previousTexture()},
            {QStringLiteral("current"), m_interpolator.currentTexture()}};
        for (int k = 1; k <= generated; ++k)
            frames.emplace_back(QStringLiteral("generated-%1").arg(k), m_interpolator.generatedTexture(k));
        auto *updates = m_rhi->nextResourceUpdateBatch();
        for (const auto &[name, texture] : frames) {
            if (!texture || texture->format() != QRhiTexture::RGBA8) continue;
            auto result = std::make_unique<QRhiReadbackResult>();
            const QString path = QStringLiteral("%1/capture-%2-%3.png").arg(directory).arg(capture).arg(name);
            auto *raw = result.get();
            raw->completed = [raw, path] {
                QImage(reinterpret_cast<const uchar *>(raw->data.constData()), raw->pixelSize.width(),
                       raw->pixelSize.height(), QImage::Format_RGBA8888).copy().save(path);
            };
            updates->readBackTexture(QRhiReadbackDescription(texture), raw);
            m_captureResults.push_back(std::move(result));
        }
        commandBuffer->resourceUpdate(updates);
        if (m_runtime)
            m_runtime->logRenderDiagnostic(QStringLiteral("frame-generation capture=%1 frames=%2 factor=%3")
                .arg(capture).arg(frames.size()).arg(m_pacer.factor()));
    }

    void setUpscalingTarget(const QSize &size) override
    {
        const auto target = size.width() > 0 && size.height() > 0
                && size.width() <= 16384 && size.height() <= 16384 ? size : QSize();
        if (m_upscalingTarget != target && m_rhi && m_rhi->backend() == QRhi::Metal)
            m_resetFrameGeneration = true;
        m_upscalingTarget = target;
    }

    void setFsrUpscaling(bool enabled) override
    {
        m_fsrUpscaling = enabled;
    }

    void setUpscalingEnhancement(int sharpness, int denoise) override
    {
        sharpness = qBound(0, sharpness, 15);
        denoise = qBound(0, denoise, 20);
        if (m_upscalingSharpness == sharpness && m_upscalingDenoise == denoise) return;
        m_upscalingSharpness = sharpness;
        m_upscalingDenoise = denoise;
        if (!m_upscalingTarget.isEmpty() && m_rhi && m_rhi->backend() == QRhi::Metal)
            m_resetFrameGeneration = true;
    }

    void setFrameGeneration(int factor, double refreshRate) override
    {
        factor = factor >= StreamFramePacer::MinimumFactor
            ? std::min(factor, StreamFramePacer::MaximumFactor) : 0;
        if (m_frameGenerationFactor != factor || m_refreshRate != refreshRate)
            m_resetFrameGeneration = true;
        m_frameGenerationFactor = factor;
        m_reportedFactor.store(factor);
        m_refreshRate = refreshRate;
        m_reportedRefreshRate.store(refreshRate);
    }

    bool needsFrame() const override
    {
        return m_needsFrame.load() && m_runtime && m_runtime->presentationAllowed();
    }

    QVariantMap swapStats() const override
    {
        const auto snapshot = m_swapTimings.snapshot();
        QVariantMap stats;
        stats.insert(QStringLiteral("available"), snapshot.available);
        if (snapshot.available) {
            stats.insert(QStringLiteral("p50Ms"), double(snapshot.submitToSwap.p50Ns) / 1.0e6);
            stats.insert(QStringLiteral("p95Ms"), double(snapshot.submitToSwap.p95Ns) / 1.0e6);
            stats.insert(QStringLiteral("maxMs"), double(snapshot.submitToSwap.maxNs) / 1.0e6);
        }
        stats.insert(QStringLiteral("windowSamples"), int(snapshot.windowSamples));
        stats.insert(QStringLiteral("swappedFramesTotal"),
                     qulonglong(snapshot.swappedFramesTotal));
        stats.insert(QStringLiteral("epoch"), qulonglong(snapshot.epoch));
        if (snapshot.hasLastSwap)
            stats.insert(QStringLiteral("sinceLastSwapMs"),
                         double(clockNs() - snapshot.lastSwapNs) / 1.0e6);
        return stats;
    }

    void setSwapGated(bool gated, const QString &) override
    {
        m_swapTimings.setGated(gated);
    }

    void frameSwapped() override
    {
        const int kind = m_submittedKind.exchange(0);
        if (kind) {
            ++m_outputCount;
            m_swapTimings.markSwap(clockNs());
        }
        if (kind == 2) m_midpointSwapped.store(true);
        const auto now = clockNs();
        const auto start = m_sampleStart.load();
        if (!start) {
            m_sampleStart.store(now);
            m_outputCount.store(0);
        } else if (now - start >= 1'000'000'000) {
            m_outputFps.store(double(m_outputCount.exchange(0)) * 1.0e9 / double(now - start));
            m_sampleStart.store(now);
        }
    }

    QVariantMap frameGenerationStats() const override
    {
        const QString states[] = {QStringLiteral("off"), QStringLiteral("warming-up"),
            QStringLiteral("active"), QStringLiteral("display-refresh"),
            QStringLiteral("overloaded"), QStringLiteral("unavailable"), QStringLiteral("discontinuity"),
            QStringLiteral("source-rate-limit"), QStringLiteral("hdr-unavailable")};
        const QString timing[] = {QStringLiteral("none"), QStringLiteral("source-timestamps"),
                                  QStringLiteral("arrival-cadence")};
        const QString rejections[] = {QStringLiteral("none"), QStringLiteral("sequence-gap"),
            QStringLiteral("timestamp-regression"), QStringLiteral("timestamp-jump"),
            QStringLiteral("arrival-gap"), QStringLiteral("cadence-unavailable")};
        return {{QStringLiteral("status"), states[int(m_frameGenerationStatus.load())]},
                {QStringLiteral("timingSource"), timing[int(m_timingSource.load())]},
                {QStringLiteral("rejectionReason"), rejections[int(m_rejection.load())]},
                {QStringLiteral("sourceIntervalMs"), double(m_sourceInterval.load()) / 1.0e6},
                {QStringLiteral("timestampDeltaMs"), double(m_timestampDelta.load()) / 1.0e6},
                {QStringLiteral("arrivalDeltaMs"), double(m_arrivalDelta.load()) / 1.0e6},
                {QStringLiteral("sequenceDelta"), qulonglong(m_sequenceDelta.load())},
                {QStringLiteral("refreshRateHz"), m_reportedRefreshRate.load()},
                {QStringLiteral("factor"), m_reportedFactor.load()},
                {QStringLiteral("effectiveFactor"), m_effectiveFactor.load()},
                {QStringLiteral("outputFps"), clockNs() - m_sampleStart.load() < 2'000'000'000
                    ? m_outputFps.load() : 0.0}};
    }

    void setComposition(const QMatrix4x4 &matrix, const QRectF &bounds,
                        const QRectF &videoRect, float opacity) override
    {
        m_textures.setComposition(matrix, bounds, videoRect, opacity);
    }

    void setClip(bool stencil, int reference) override
    {
        m_stencil = stencil;
        m_stencilReference = reference;
    }

    void recordFrame(QRhiCommandBuffer *commandBuffer, const QRect &) override
    {
        if (!m_runtime || !m_runtime->presentationAllowed()
                || m_presentationGeneration != m_runtime->presentationGeneration()) return;
        logFrameGenerationStatus();
        m_textures.render(commandBuffer, m_stencil, m_stencilReference);
        if (m_outputDirty) {
            m_submittedKind.store(m_outputKind);
            m_outputDirty = false;
            if (m_outputKind == 1) m_swapTimings.markSubmit(clockNs());
        }
        observeSwapProgress();
    }

    void finishFrame() override
    {
        if (m_preparedFrame && m_runtime)
            m_runtime->releaseFrame(std::exchange(m_preparedFrame, nullptr));
    }

    void releaseResources() override
    {
        tearDownResources(false, false, false);
    }

    void tearDownResources(bool ownRearm, bool deviceChanged, bool generationChanged)
    {
        m_swapStall.onResourcesReleased(ownRearm, deviceChanged, generationChanged);
        if (m_rhi && m_graphicsReady) m_rhi->finish();
        finishFrame();
        m_textures.release();
        m_interpolator.release();
        m_pacer.reset();
        m_swapTimings.reset();
        updateTimingStats();
        m_needsFrame.store(false);
        m_submittedKind.store(0);
        m_outputCount.store(0);
        m_outputFps.store(0);
        m_sampleStart.store(0);
        m_outputDirty = false;
        m_midpointSwapped.store(false);
        m_resetFrameGeneration = true;
        if (m_rhi && m_graphicsReady) m_rhi->finish();
        if (m_graphicsReady && m_runtime) m_runtime->sceneGraphShutdown();
        m_graphicsReady = false;
        m_reportedFailure = false;
        m_reportedColorFormat = 0;
        m_reportedColorSpace = 0;
        m_reportedOutputBits = 0;
        m_rhi = nullptr;
    }

    void observeSwapProgress()
    {
        const auto snapshot = m_swapTimings.snapshot();
        const auto now = clockNs();
        StreamSwapStallWatchdog::Observation observation;
        observation.gated = snapshot.gated;
        observation.gateEpoch = snapshot.gateEpoch;
        observation.hasPendingSubmit = snapshot.hasPendingSubmit;
        observation.hasLastSwap = snapshot.hasLastSwap;
        observation.lastSwapNs = snapshot.lastSwapNs;
        const auto progress = m_runtime
            ? m_runtime->upstreamProgress() : NativeStreamRuntime::UpstreamProgress{};
        observation.upstreamStalled = progress.stalled;
        observation.hasUpstreamSample = progress.hasDecodeTimings;
        observation.upstreamEpoch = progress.decodeEpoch;
        observation.upstreamOutputsTotal = progress.decodedOutputsTotal;
        const auto outcome = m_swapStall.observe(observation, now);
        if (outcome == StreamSwapStallWatchdog::Outcome::ResourceRearm) {
            m_resourceRearmPending = true;
        } else if (outcome == StreamSwapStallWatchdog::Outcome::Unrecovered) {
            reportFailure(QStringLiteral("The render thread stopped swapping decoded frames. The last presentation resource re-arm did not restore the stream, so the session must reconnect."));
        }
    }

private:
    NativeStreamRuntime *m_runtime;
    QRhi *m_rhi = nullptr;
    std::uint32_t m_reportedColorFormat = 0;
    std::uint32_t m_reportedColorSpace = 0;
    int m_reportedOutputBits = 0;
    StreamVideoTextureRenderer m_textures;
    StreamPresentTimings m_swapTimings;
    StreamSwapStallWatchdog m_swapStall;
    bool m_resourceRearmPending = false;
    int m_sourceColorSpace = OPENNOW_STREAMER_COLOR_SPACE_SDR709;
    StreamFrameInterpolator m_interpolator;
    StreamFramePacer m_pacer;
    QSize m_historySize;
    QRhiTexture::Format m_historyFormat = QRhiTexture::UnknownFormat;
    double m_refreshRate = 0;
    int m_frameGenerationFactor = 0;
    std::atomic_int m_reportedFactor = 0;
    std::atomic_int m_effectiveFactor = 0;
    std::vector<std::unique_ptr<QRhiReadbackResult>> m_captureResults;
    std::int64_t m_captureDueNs = 0;
    int m_captureCount = 0;
    FrameGenerationState m_loggedStatus = FrameGenerationState::Off;
    std::int64_t m_lastStatusLogNs = 0;
    int m_suppressedStatusChanges = 0;
    QSize m_upscalingTarget;
    bool m_fsrUpscaling = false;
    int m_upscalingSharpness = 10;
    int m_upscalingDenoise = 0;
    bool m_frameGenerationFailed = false;
    bool m_resetFrameGeneration = false;
    bool m_outputDirty = false;
    int m_outputKind = 0;
    std::atomic_bool m_needsFrame = false;
    std::atomic_bool m_midpointSwapped = false;
    std::atomic_int m_submittedKind = 0;
    std::atomic<FrameGenerationState> m_frameGenerationStatus = FrameGenerationState::Off;
    std::atomic_uint64_t m_outputCount = 0;
    std::atomic_int64_t m_sampleStart = 0;
    std::atomic<double> m_outputFps = 0;
    std::atomic<StreamFramePacer::TimingSource> m_timingSource = StreamFramePacer::TimingSource::None;
    std::atomic<StreamFramePacer::Rejection> m_rejection = StreamFramePacer::Rejection::None;
    std::atomic_uint64_t m_sourceInterval = 0;
    std::atomic_uint64_t m_timestampDelta = 0;
    std::atomic_uint64_t m_sequenceDelta = 0;
    std::atomic_int64_t m_arrivalDelta = 0;
    std::atomic<double> m_reportedRefreshRate = 0;
    OpenNowStreamerFrame *m_preparedFrame = nullptr;
    int m_stencilReference = 0;
    bool m_stencil = false;
    bool m_graphicsReady = false;
    bool m_reportedFailure = false;
    quint64 m_presentationGeneration = 0;
    static std::int64_t clockNs()
    {
        return streamMonotonicClockNs();
    }

    // Every frame-generation status change goes to qt-native.log with the pacer's numbers,
    // at most five lines a second; flickers in between are counted, not written.
    void logFrameGenerationStatus()
    {
        const auto status = m_frameGenerationStatus.load();
        if (status == m_loggedStatus) return;
        const auto now = clockNs();
        m_loggedStatus = status;
        if (m_lastStatusLogNs && now - m_lastStatusLogNs < 200'000'000) {
            ++m_suppressedStatusChanges;
            return;
        }
        if (!m_runtime) return;
        const auto stats = frameGenerationStats();
        m_runtime->logRenderDiagnostic(QStringLiteral(
            "frame-generation status=%1 reason=%2 timing=%3 factor=%4/%5 interval=%6ms stamp=%7ms arrival=%8ms "
            "seq-delta=%9 refresh=%10 output=%11fps suppressed=%12")
            .arg(stats.value(QStringLiteral("status")).toString(),
                 stats.value(QStringLiteral("rejectionReason")).toString(),
                 stats.value(QStringLiteral("timingSource")).toString())
            .arg(stats.value(QStringLiteral("effectiveFactor")).toInt())
            .arg(stats.value(QStringLiteral("factor")).toInt())
            .arg(stats.value(QStringLiteral("sourceIntervalMs")).toDouble(), 0, 'f', 2)
            .arg(stats.value(QStringLiteral("timestampDeltaMs")).toDouble(), 0, 'f', 2)
            .arg(stats.value(QStringLiteral("arrivalDeltaMs")).toDouble(), 0, 'f', 2)
            .arg(stats.value(QStringLiteral("sequenceDelta")).toULongLong())
            .arg(stats.value(QStringLiteral("refreshRateHz")).toDouble(), 0, 'f', 1)
            .arg(stats.value(QStringLiteral("outputFps")).toDouble(), 0, 'f', 0)
            .arg(m_suppressedStatusChanges));
        m_lastStatusLogNs = now;
        m_suppressedStatusChanges = 0;
    }

    void updateTimingStats()
    {
        m_timingSource.store(m_pacer.timingSource());
        m_rejection.store(m_pacer.rejection());
        m_sourceInterval.store(m_pacer.interval());
        m_timestampDelta.store(m_pacer.timestampDelta());
        m_sequenceDelta.store(m_pacer.sequenceDelta());
        m_arrivalDelta.store(m_pacer.arrivalDelta());
    }

    void resetFrameGeneration()
    {
        m_textures.clearExternalTextures();
        m_interpolator.release();
        m_pacer.reset();
        updateTimingStats();
        m_needsFrame.store(false);
        m_submittedKind.store(0);
        m_midpointSwapped.store(false);
        m_outputDirty = false;
        m_frameGenerationFailed = false;
        m_effectiveFactor.store(0);
        m_frameGenerationStatus.store(m_frameGenerationFactor >= 2 ? FrameGenerationState::WarmingUp : FrameGenerationState::Off);
    }

    void disableFrameGeneration()
    {
        m_textures.clearExternalTextures();
        m_interpolator.release();
        m_pacer.reset();
        updateTimingStats();
        m_needsFrame.store(false);
        m_frameGenerationFailed = true;
        m_frameGenerationStatus.store(FrameGenerationState::Unavailable);
    }

    // Advances the pair's presentation schedule: the next generated frame when its slot
    // arrives, then the original. Only moves on once the previously selected generated
    // frame has actually reached the display.
    void prepareNext()
    {
        if (m_frameGenerationFactor < 2 || !m_midpointSwapped.load()) return;
        const auto next = m_pacer.takeNext(clockNs(), m_refreshRate);
        if (next.kind == StreamFramePacer::Next::Kind::None) return;
        if (next.kind == StreamFramePacer::Next::Kind::Generated) {
            if (!m_textures.selectTexture(m_interpolator.generatedTexture(next.index))) {
                disableFrameGeneration();
                return;
            }
            m_midpointSwapped.store(false);
            m_needsFrame.store(true);
            m_outputDirty = true;
            m_outputKind = 2;
            return;
        }
        m_needsFrame.store(false);
        if (!m_textures.selectTexture(m_interpolator.currentTexture())) {
            disableFrameGeneration();
            return;
        }
        m_outputDirty = true;
        m_outputKind = 1;
    }
    void reportFailure(const QString &message,
                       NativeStreamRuntime::PresentationFailure failure
                       = NativeStreamRuntime::PresentationFailure::Recoverable)
    {
        if (m_reportedFailure || !m_runtime) return;
        m_reportedFailure = true;
        m_runtime->reportPresentationError(message, m_presentationGeneration, failure);
    }
};
}

std::shared_ptr<StreamVideoRenderCallback> createNativeStreamRenderCallback(
    NativeStreamRuntime *runtime)
{
    return std::make_shared<NativeStreamRenderCallback>(runtime);
}
