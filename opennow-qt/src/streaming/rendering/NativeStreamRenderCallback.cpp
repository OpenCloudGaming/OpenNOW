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
#include "streaming/rendering/StreamSenderPacer.h"
#include "streaming/rendering/WindowsPresentationObserver.h"
#include "streaming/rendering/StreamFrameSnapshot.h"

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

    ~NativeStreamRenderCallback() override
    {
        releaseResources();
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
        m_renderStartNs = WindowsPresentationObserver::clockNs();
        prepareNativeFrame(commandBuffer);
        if (!m_runtime || !m_runtime->presentationAllowed()
            || m_presentationGeneration != m_runtime->presentationGeneration()
            || !m_graphicsReady || !m_rhi || !commandBuffer) return;
        if (!m_snapshotRequest) {
            m_snapshot.reset();
            const auto request = m_runtime->takeScreenshotRequest();
            if (request) {
                m_snapshot = std::make_unique<StreamFrameSnapshot>();
                m_snapshotRequest = request;
                if (!m_textures.importedTexture()
                    || !m_snapshot->submit(m_rhi, commandBuffer, m_textures.importedTexture(),
                        m_sourceColorSpace, [this, request](QImage image) {
                            if (m_snapshotRequest != request) return;
                            m_snapshotRequest = 0;
                            m_runtime->completeScreenshot(request, std::move(image));
                        })) {
                    m_snapshotRequest = 0;
                    m_runtime->completeScreenshot(request, {});
                }
            }
        }
        m_textures.prepareUpscaling(commandBuffer, m_upscalingTarget, m_fsrUpscaling,
            m_sourceColorSpace == OPENNOW_STREAMER_COLOR_SPACE_SDR709, m_upscalingSharpness);
        m_fsrResourceGeneration.store(m_textures.fsrResourceGeneration());
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
        const auto presentation = m_runtime->presentationConfig();
        if (m_senderResetRequested.exchange(false)
                || (m_pendingFrame && (!presentation.active
                    || presentation.generation != m_pendingGeneration
                    || presentation.scene_epoch != m_pendingSceneEpoch))) {
            clearPendingFrame();
            m_senderPacer.reset();
        }
        OpenNowStreamerFrameInfo info{};
        OpenNowStreamerRecordedFrame recorded{};
        OpenNowStreamerFrame *frame = nullptr;
        OpenNowStreamerStatus status = OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        auto senderNow = StreamSenderPacer::clockNs();
        if (m_pendingFrame) {
            if (senderNow < m_pendingDeadline) {
                prepareNext(commandBuffer);
                return;
            }
            frame = std::exchange(m_pendingFrame, nullptr);
            info = m_pendingInfo;
            m_nextFrameDeadline.store(0);
            status = OPENNOW_STREAMER_OK;
        } else {
            {
                ExternalCommandScope externalCommands(commandBuffer);
                status = m_runtime->acquireFrame(&info, &frame);
            }
            if (status == OPENNOW_STREAMER_OK && frame) {
                const bool recognized = presentation.active
                    && info.sender_generation == presentation.generation
                    && info.sender_scene_epoch == presentation.scene_epoch
                    && (info.sender_timing_flags & OPENNOW_STREAMER_FRAME_TIMING_CAPTURE);
                const StreamSenderTiming timing{info.sender_generation, info.sender_scene_epoch,
                    info.sender_frame_index, info.sender_capture_time_us,
                    (info.sender_timing_flags & OPENNOW_STREAMER_FRAME_TIMING_PRESENTATION)
                        ? info.sender_presentation_time_us : 0,
                    recognized,
                    recognized && presentation.sender_pacing == 2
                        && bool(info.sender_timing_flags & OPENNOW_STREAMER_FRAME_TIMING_PACED)
                        && info.sender_paced != 0};
                if (recognized) m_senderFramesObserved.fetch_add(1);
                if (timing.paced) m_senderFramesPaced.fetch_add(1);
                const int senderMode = !recognized ? 0 : timing.paced ? 2 : 1;
                const int previousSenderMode = m_senderMode.exchange(senderMode);
                if (timing.paced && previousSenderMode != 2 && m_frameGenerationFactor >= 2) {
                    resetFrameGeneration();
                    m_frameGenerationStatus.store(m_frameGenerationFactor >= 2
                        ? FrameGenerationState::Unavailable : FrameGenerationState::Off);
                }
                if (previousSenderMode != senderMode)
                    qInfo("Sender frame pacing: metadata=%s sender_paced=%d clock=%s; at most one unrecorded frame.",
                        recognized ? "correlated" : "unavailable", timing.paced,
                        !recognized ? "unavailable" : timing.captureTimeUs != 0
                            ? "capture-us" : timing.presentationTimeUs != 0 ? "extension-us" : "unavailable");
                senderNow = StreamSenderPacer::clockNs();
                const auto deadline = m_senderPacer.deadline(timing, senderNow);
                if (deadline > senderNow) {
                    m_pendingFrame = frame;
                    m_pendingInfo = info;
                    m_pendingGeneration = presentation.generation;
                    m_pendingSceneEpoch = presentation.scene_epoch;
                    m_pendingDeadline = deadline;
                    m_nextFrameDeadline.store(deadline);
                    m_senderFramesDeferred.fetch_add(1);
                    prepareNext(commandBuffer);
                    return;
                }
            }
        }
        if (status == OPENNOW_STREAMER_OK && frame) {
            const auto current = m_runtime->presentationConfig();
            if (presentation.active && (!current.active
                    || current.generation != presentation.generation
                    || current.scene_epoch != presentation.scene_epoch)) {
                m_runtime->releaseFrame(frame);
                m_senderPacer.reset();
                prepareNext(commandBuffer);
                return;
            }
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
                m_runtime->releaseFrame(frame);
                return;
            }
            if (!command.command_buffer) {
                m_runtime->releaseFrame(frame);
                return;
            }
            status = m_runtime->recordFrame(frame, command, &info, &recorded);
        }

        if (m_presentationGeneration != m_runtime->presentationGeneration()
                || !m_runtime->presentationAllowed()) {
            if (frame) m_runtime->releaseFrame(frame);
            m_textures.clearFrames();
            return;
        }
        if (status == OPENNOW_STREAMER_NO_FRAME || status == OPENNOW_STREAMER_STALE_FRAME) {
            if (frame) m_runtime->releaseFrame(frame);
            prepareNext(commandBuffer);
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
        m_sourceGeneration = presentation.active ? presentation.generation : 0;
        m_sourceSceneEpoch = presentation.scene_epoch;
        m_sourceTimestampNs = recorded.presentation_time_ns;
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
            if (m_runtime)
                m_runtime->logRenderDiagnostic(QStringLiteral("video texture import failed slot=%1 format=%2 %3x%4 step=%5")
                    .arg(command.frame_slot).arg(int(recorded.texture_format))
                    .arg(recorded.width).arg(recorded.height).arg(m_textures.importFailure()));
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
        const bool senderPaced = info.sender_generation == presentation.generation
            && info.sender_scene_epoch == presentation.scene_epoch
            && bool(info.sender_timing_flags & OPENNOW_STREAMER_FRAME_TIMING_PACED)
            && info.sender_paced != 0;
        auto *source = m_textures.importedTexture();
        if ((m_frameGenerationFactor >= 2) && !senderPaced && (m_historySize != source->pixelSize()
                                  || m_historyFormat != source->format())) {
            resetFrameGeneration();
            m_historySize = source->pixelSize();
            m_historyFormat = source->format();
        }
        m_outputDirty = true;
        m_outputKind = 1;
        if (senderPaced) {
            m_frameGenerationStatus.store(m_frameGenerationFactor >= 2
                ? FrameGenerationState::Unavailable : FrameGenerationState::Off);
            return;
        }
        if (m_sourceColorSpace != OPENNOW_STREAMER_COLOR_SPACE_SDR709) {
            m_frameGenerationStatus.store(m_frameGenerationFactor >= 2 ? FrameGenerationState::HdrUnsupported : FrameGenerationState::Off);
            return;
        }
        if (m_frameGenerationFactor < 2 || m_frameGenerationFailed) return;

        const auto now = clockNs();
        // The presenter is stalled when nothing new has reached the display for a while;
        // a source frame arriving early on its own is a burst, not overload. Generated frames
        // count: an engine that never reaches the original slot is late, not stalled.
        const auto lastPresent = m_lastPresentNs.load();
        const bool presenterStalled = lastPresent && now - lastPresent
            > std::max<std::int64_t>(100'000'000, 4 * std::int64_t(m_pacer.interval()));
        const bool late = m_pacer.pending();
        const auto decision = m_pacer.source(info.sequence, info.presentation_time_ns, now, m_refreshRate,
                                             (std::min)(m_frameGenerationFactor, m_factorCap), presenterStalled);
        updateTimingStats();
        if (decision == StreamFramePacer::Result::Interpolate) adaptFactorCap(late);
        if (decision == StreamFramePacer::Result::Duplicate) {
            m_outputDirty = false;
            if (m_pacer.pending())
                m_textures.selectTexture(m_interpolator.generatedTexture(m_pacer.pendingIndex()));
            else if (m_generatedOnly && selectLatestGenerated(commandBuffer)) {
            } else if (m_interpolator.hasPair())
                m_textures.selectTexture(m_interpolator.anchorTexture());
            prepareNext(commandBuffer);
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
        if (!m_interpolator.initialize(m_rhi)) {
            stopFrameGeneration(QStringLiteral("initialize"));
            return;
        }
        // Copy the frame that just arrived, then interpolate up to it. Computing first
        // shows the previous pair and holds this look for a whole source interval.
        if (!m_interpolator.ingest(commandBuffer, source, m_pacer.factor())) {
            stopFrameGeneration(QStringLiteral("ingest"));
            return;
        }
        if (!m_interpolator.computePending(commandBuffer)) {
            stopFrameGeneration(QStringLiteral("flyhigh compute failed"));
            return;
        }
        const double engineMs = m_interpolator.lastGpuMs();
        if (engineMs > 0.0)
            m_engineUs.store(qint64(engineMs * 1000.0));
        if (m_runtime && m_loggedEngine != m_interpolator.engine()) {
            m_loggedEngine = m_interpolator.engine();
            m_runtime->logRenderDiagnostic(QStringLiteral("frame-generation engine=%1").arg(m_loggedEngine));
        }
        if (!m_interpolator.hasPair()) {
            if (decision != StreamFramePacer::Result::Discontinuity)
                m_frameGenerationStatus.store(FrameGenerationState::WarmingUp);
            return;
        }
        // The engine may synthesise fewer frames than requested (the block-matching
        // engine knows only the midpoint); the schedule follows what exists.
        const int generated = (std::min)(m_interpolator.generatedCount(), m_pacer.factor() - 1);
        m_effectiveFactor.store(generated + 1);
        if (generated < 1 || !m_textures.selectTexture(m_interpolator.generatedTexture(1))) {
            stopFrameGeneration(QStringLiteral("no generated frame"));
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

    void setGeneratedFramesOnly(bool enabled) override
    {
        if (m_generatedOnly == enabled) return;
        m_generatedOnly = enabled;
        if (m_runtime)
            m_runtime->logRenderDiagnostic(QStringLiteral("generated-only=%1").arg(enabled ? 1 : 0));
    }

    void setFrameGeneration(int factor, double refreshRate) override
    {
        factor = factor >= StreamFramePacer::MinimumFactor
            ? (std::min)(factor, StreamFramePacer::MaximumFactor) : 0;
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

    std::uint64_t nextFrameDeadlineNs() const override
    {
        return m_nextFrameDeadline.load();
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
        stats.insert(QStringLiteral("senderTimingFrames"), qulonglong(m_senderFramesObserved.load()));
        stats.insert(QStringLiteral("senderPacedFrames"), qulonglong(m_senderFramesPaced.load()));
        stats.insert(QStringLiteral("senderDeferredFrames"), qulonglong(m_senderFramesDeferred.load()));
        stats.insert(QStringLiteral("senderFramePending"), m_nextFrameDeadline.load() != 0);
        stats.insert(QStringLiteral("fsrResourceGeneration"), qulonglong(m_fsrResourceGeneration.load()));
        if (snapshot.hasLastSwap)
            stats.insert(QStringLiteral("sinceLastSwapMs"),
                         double(clockNs() - snapshot.lastSwapNs) / 1.0e6);
        return stats;
    }

    void setSwapGated(bool gated, const QString &) override
    {
        m_swapTimings.setGated(gated);
        if (gated) {
            m_senderResetRequested.store(true);
            m_nextFrameDeadline.store(0);
        }
    }

    void beforeFrameBegin(QQuickWindow *window) override
    {
        m_presentationWindow = window;
        const auto config = m_runtime ? m_runtime->presentationConfig() : OpenNowStreamerPresentationConfig{};
        m_observationActive = config.active && config.feedback_mode == 1;
        if (m_observationActive) m_presentationObserver.beginFrame(window);
        else m_presentationObserver.reset();
    }

    void afterRendering() override
    {
        if (m_observationActive) m_presentationObserver.beforePresent();
    }

    void frameSwapped() override
    {
        if (const auto observation = m_observationActive
                ? m_presentationObserver.frameSwapped(m_presentationWindow) : std::nullopt;
                observation && m_runtime && m_runtime->presentationAllowed()
                && m_presentationGeneration == m_runtime->presentationGeneration()) {
            OpenNowStreamerPresentationSample sample{};
            sample.version = OPENNOW_STREAMER_PRESENTATION_VERSION;
            sample.struct_size = sizeof(sample);
            sample.generation = observation->source.generation;
            sample.scene_epoch = observation->source.sceneEpoch;
            sample.presentation_time_ns = observation->source.timestampNs;
            sample.render_duration_ns = observation->renderDurationNs;
            sample.render_present_to_complete_ns = observation->presentDurationNs;
            sample.completion_age_ns = observation->completionAgeNs;
            sample.vsync_interval_ns = observation->vsyncIntervalNs;
            sample.completion_kind = OPENNOW_STREAMER_PRESENTATION_PLATFORM_PRESENTED;
            sample.valid_fields = OPENNOW_STREAMER_PRESENTATION_REQUIRED_FIELDS;
            m_runtime->submitPresentationSample(sample);
        }
        const int kind = m_submittedKind.exchange(0);
        if (kind) {
            ++m_outputCount;
            m_swapTimings.markSwap(clockNs());
            m_lastPresentNs.store(clockNs());
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
                    ? m_outputFps.load() : 0.0},
                {QStringLiteral("engineMs"), double(m_engineUs.load()) / 1000.0}};
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
            if (m_outputKind == 1) {
                m_swapTimings.markSubmit(clockNs());
                m_presentationObserver.source(m_sourceGeneration, m_sourceSceneEpoch,
                                               m_sourceTimestampNs, m_renderStartNs);
            }
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
        if (m_snapshot) m_snapshot->cancel();
        if (const auto request = std::exchange(m_snapshotRequest, 0); request && m_runtime)
            m_runtime->completeScreenshot(request, {});
        if (m_rhi && m_graphicsReady) m_rhi->finish();
        m_snapshot.reset();
        finishFrame();
        clearPendingFrame();
        m_senderPacer.reset();
        m_senderFramesObserved.store(0);
        m_senderFramesPaced.store(0);
        m_senderFramesDeferred.store(0);
        m_senderMode.store(-1);
        m_textures.release();
        m_interpolator.release();
        m_pacer.reset();
        m_swapTimings.reset();
        m_presentationObserver.reset();
        m_sourceGeneration = 0;
        m_sourceSceneEpoch = 0;
        m_sourceTimestampNs = 0;
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
    void clearPendingFrame()
    {
        if (m_pendingFrame && m_runtime)
            m_runtime->releaseFrame(std::exchange(m_pendingFrame, nullptr));
        m_nextFrameDeadline.store(0);
        m_pendingInfo = {};
        m_pendingDeadline = 0;
    }

    NativeStreamRuntime *m_runtime;
    QRhi *m_rhi = nullptr;
    std::uint32_t m_reportedColorFormat = 0;
    std::uint32_t m_reportedColorSpace = 0;
    int m_reportedOutputBits = 0;
    StreamVideoTextureRenderer m_textures;
    std::unique_ptr<StreamFrameSnapshot> m_snapshot;
    quint64 m_snapshotRequest = 0;
    StreamPresentTimings m_swapTimings;
    WindowsPresentationObserver m_presentationObserver;
    bool m_observationActive = false;
    QQuickWindow *m_presentationWindow = nullptr;
    std::uint64_t m_renderStartNs = 0;
    std::uint64_t m_sourceGeneration = 0;
    std::uint64_t m_sourceSceneEpoch = 0;
    std::uint64_t m_sourceTimestampNs = 0;
    StreamSwapStallWatchdog m_swapStall;
    bool m_resourceRearmPending = false;
    int m_sourceColorSpace = OPENNOW_STREAMER_COLOR_SPACE_SDR709;
    StreamFrameInterpolator m_interpolator;
    StreamFramePacer m_pacer;
    StreamSenderPacer m_senderPacer;
    OpenNowStreamerFrame *m_pendingFrame = nullptr;
    OpenNowStreamerFrameInfo m_pendingInfo{};
    std::uint64_t m_pendingGeneration = 0;
    std::uint64_t m_pendingSceneEpoch = 0;
    std::uint64_t m_pendingDeadline = 0;
    std::atomic_uint64_t m_nextFrameDeadline = 0;
    std::atomic_bool m_senderResetRequested = false;
    std::atomic_uint64_t m_senderFramesObserved = 0;
    std::atomic_uint64_t m_senderFramesPaced = 0;
    std::atomic_uint64_t m_senderFramesDeferred = 0;
    std::atomic_uint64_t m_fsrResourceGeneration = 0;
    std::atomic_int m_senderMode = -1;
    QSize m_historySize;
    QRhiTexture::Format m_historyFormat = QRhiTexture::UnknownFormat;
    double m_refreshRate = 0;
    int m_frameGenerationFactor = 0;
    bool m_generatedOnly = false;
    QString m_loggedEngine;
    std::atomic_int m_reportedFactor = 0;
    std::atomic_int m_effectiveFactor = 0;
    std::atomic<qint64> m_engineUs = 0;
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
    std::atomic<std::int64_t> m_lastPresentNs = 0;
    int m_factorCap = StreamFramePacer::MaximumFactor;
    int m_lateStreak = 0;
    int m_onTimeStreak = 0;
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
        m_engineUs.store(0);
        m_factorCap = StreamFramePacer::MaximumFactor;
        m_lateStreak = 0;
        m_onTimeStreak = 0;
        m_frameGenerationStatus.store(m_frameGenerationFactor >= 2 ? FrameGenerationState::WarmingUp : FrameGenerationState::Off);
    }

    // A pair whose original frame has not been shown when the next source frame arrives
    // means the engine needs more than one source interval at this factor. Three in a row
    // lower the factor; a long on-time run tries the next one up again.
    void adaptFactorCap(bool late)
    {
        const int previous = m_factorCap;
        if (late) {
            m_onTimeStreak = 0;
            if (++m_lateStreak >= 3 && m_factorCap > StreamFramePacer::MinimumFactor) {
                --m_factorCap;
                m_lateStreak = 0;
            }
        } else {
            m_lateStreak = 0;
            if (++m_onTimeStreak >= 1200 && m_factorCap < StreamFramePacer::MaximumFactor) {
                ++m_factorCap;
                m_onTimeStreak = 0;
            }
        }
        if (m_factorCap != previous && m_runtime)
            m_runtime->logRenderDiagnostic(QStringLiteral("frame-generation factor cap %1 -> %2 (%3)")
                .arg(previous).arg(m_factorCap).arg(m_interpolator.engine()));
    }

    void stopFrameGeneration(const QString &reason)
    {
        logFrameGenerationEvent(QStringLiteral("stopped: %1").arg(reason));
        disableFrameGeneration();
    }

    void logFrameGenerationEvent(const QString &text)
    {
        if (m_runtime) m_runtime->logRenderDiagnostic(QStringLiteral("frame-generation %1").arg(text));
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
    void prepareNext(QRhiCommandBuffer *commandBuffer)
    {
        if (m_frameGenerationFactor < 2 || !m_midpointSwapped.load()) return;
        // No early lead here. At 240 Hz the pacer's 1.5-refresh lead makes the real frame
        // replace the generated one after a single tick, so the mix flickers and the
        // generated-only hold looks smoother. Each step keeps its full slot.
        const auto next = m_pacer.takeNext(clockNs(), 0);
        if (next.kind == StreamFramePacer::Next::Kind::None) {
            m_needsFrame.store(true);
            return;
        }
        if (next.kind == StreamFramePacer::Next::Kind::Generated) {
            if (m_interpolator.prepare(commandBuffer, next.index)
                && m_textures.selectTexture(m_interpolator.generatedTexture(next.index))) {
                m_midpointSwapped.store(false);
                m_needsFrame.store(true);
                m_outputDirty = true;
                m_outputKind = 2;
                return;
            }
            logFrameGenerationEvent(QStringLiteral("skipped generated frame %1").arg(next.index));
        }
        m_needsFrame.store(false);
        // Ctrl+Shift+F12 holds the last generated frame where the source frame would show.
        if (m_generatedOnly && selectLatestGenerated(commandBuffer)) {
            m_outputDirty = true;
            m_outputKind = 2;
            return;
        }
        if (m_textures.selectTexture(m_interpolator.anchorTexture())) {
            m_outputDirty = true;
            m_outputKind = 1;
            return;
        }
        logFrameGenerationEvent(QStringLiteral("anchor frame not shown"));
    }

    bool selectLatestGenerated(QRhiCommandBuffer *commandBuffer)
    {
        const int count = m_interpolator.generatedCount();
        return count > 0 && m_interpolator.prepare(commandBuffer, count)
            && m_textures.selectTexture(m_interpolator.generatedTexture(count));
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
