#include "streaming/NativeStreamRuntime.h"
#include "streaming/rendering/NativeStreamRenderCallback.h"
#include "streaming/rendering/StreamVideoRenderCallback.h"

#include <QGuiApplication>
#include <QImage>
#include <QJsonDocument>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QTest>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>

#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <memory>
#include <optional>
#include <thread>

#if defined(Q_OS_WIN)
#include <d3d11.h>
#elif QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
#include <QVulkanFunctions>
#endif

#if defined(Q_OS_WIN) || (QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>))
namespace {
struct Producer {
#if !defined(Q_OS_WIN)
    struct Image {
        VkImage image = VK_NULL_HANDLE;
        VkDeviceMemory memory = VK_NULL_HANDLE;
        bool initialized = false;
    };
    QVulkanInstance *instance = nullptr;
    QVulkanDeviceFunctions *functions = nullptr;
    std::array<Image, 8> images{};
#endif
    struct Frame {
        Producer *producer;
        OpenNowStreamerFrameInfo info;
    };

    OpenNowStreamerConfig config{};
    OpenNowStreamerGraphicsContext context{};
    QRhiTexture *sourceTexture = nullptr;
    std::optional<OpenNowStreamerFrameInfo> next;
    OpenNowStreamerFrameInfo acquired{};
    OpenNowStreamerFrameInfo recorded{};
    OpenNowStreamerFrameInfo released{};
    int sourceCount = 0;
    int releaseCount = 0;
    int emptyCount = 0;
    int shutdownCount = 0;

    ~Producer() { clearImages(); }

    void clearImages()
    {
#if !defined(Q_OS_WIN)
        if (!functions) return;
        for (auto &image : images) {
            if (image.image) functions->vkDestroyImage(device(), image.image, nullptr);
            if (image.memory) functions->vkFreeMemory(device(), image.memory, nullptr);
            image = {};
        }
#endif
    }

#if !defined(Q_OS_WIN)
    VkDevice device() const { return reinterpret_cast<VkDevice>(context.device); }

    bool createImage(Image &image)
    {
        VkImageCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO;
        info.imageType = VK_IMAGE_TYPE_2D;
        info.format = VK_FORMAT_R8G8B8A8_UNORM;
        info.extent = {64, 64, 1};
        info.mipLevels = 1;
        info.arrayLayers = 1;
        info.samples = VK_SAMPLE_COUNT_1_BIT;
        info.tiling = VK_IMAGE_TILING_OPTIMAL;
        info.usage = VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_SAMPLED_BIT;
        info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
        if (functions->vkCreateImage(device(), &info, nullptr, &image.image) != VK_SUCCESS)
            return false;
        VkMemoryRequirements requirements{};
        functions->vkGetImageMemoryRequirements(device(), image.image, &requirements);
        VkPhysicalDeviceMemoryProperties properties{};
        instance->functions()->vkGetPhysicalDeviceMemoryProperties(
            reinterpret_cast<VkPhysicalDevice>(context.physical_device), &properties);
        for (uint32_t i = 0; i < properties.memoryTypeCount; ++i) {
            if (!(requirements.memoryTypeBits & (1u << i))) continue;
            VkMemoryAllocateInfo allocation{};
            allocation.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
            allocation.allocationSize = requirements.size;
            allocation.memoryTypeIndex = i;
            if (functions->vkAllocateMemory(device(), &allocation, nullptr, &image.memory)
                    != VK_SUCCESS) return false;
            return functions->vkBindImageMemory(device(), image.image, image.memory, 0) == VK_SUCCESS;
        }
        return false;
    }
#endif
};

Producer *producerToCreate = nullptr;

NativeStreamRuntime::Api producerApi()
{
    NativeStreamRuntime::Api api;
    api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
        producerToCreate->config = *config;
        *output = reinterpret_cast<OpenNowStreamer *>(producerToCreate);
        return OPENNOW_STREAMER_OK;
    };
    api.send = [](const OpenNowStreamer *handle, const uint8_t *bytes, size_t length) {
        auto *producer = reinterpret_cast<const Producer *>(handle);
        const auto command = QJsonDocument::fromJson(
            QByteArray(reinterpret_cast<const char *>(bytes), qsizetype(length))).object();
        const auto reply = QJsonDocument(QJsonObject{
            {QStringLiteral("id"), command.value(QStringLiteral("id"))},
            {QStringLiteral("type"), QStringLiteral("ok")}}).toJson(QJsonDocument::Compact);
        producer->config.response_callback(reinterpret_cast<const uint8_t *>(reply.constData()),
                                           size_t(reply.size()), producer->config.user_data);
        return OPENNOW_STREAMER_OK;
    };
    api.destroy = [](OpenNowStreamer *) { return OPENNOW_STREAMER_OK; };
    api.setGraphicsContext = [](const OpenNowStreamer *handle,
                                const OpenNowStreamerGraphicsContext *context) {
        auto *producer = reinterpret_cast<Producer *>(const_cast<OpenNowStreamer *>(handle));
#if defined(Q_OS_WIN)
        if (context->graphics_api != OPENNOW_STREAMER_GRAPHICS_API_D3D11
            || !context->device || !context->queue)
            return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        producer->context = *context;
#else
        if (context->graphics_api != OPENNOW_STREAMER_GRAPHICS_API_VULKAN)
            return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        producer->context = *context;
        producer->functions = producer->instance->deviceFunctions(producer->device());
#endif
        return OPENNOW_STREAMER_OK;
    };
    api.acquireLatestFrame = [](const OpenNowStreamer *handle, OpenNowStreamerFrame **output,
                                OpenNowStreamerFrameInfo *info) {
        auto *producer = reinterpret_cast<Producer *>(const_cast<OpenNowStreamer *>(handle));
        if (!producer->next) {
            ++producer->emptyCount;
            *output = nullptr;
            return OPENNOW_STREAMER_NO_FRAME;
        }
        *info = *producer->next;
        producer->acquired = *info;
        producer->next.reset();
        ++producer->sourceCount;
        *output = reinterpret_cast<OpenNowStreamerFrame *>(new Producer::Frame{producer, *info});
        return OPENNOW_STREAMER_OK;
    };
    api.recordFrame = [](const OpenNowStreamer *handle, const OpenNowStreamerFrame *token,
                         const OpenNowStreamerRecordCommand *command,
                         OpenNowStreamerRecordedFrame *output) {
        auto *producer = reinterpret_cast<Producer *>(const_cast<OpenNowStreamer *>(handle));
        const auto *frame = reinterpret_cast<const Producer::Frame *>(token);
#if defined(Q_OS_WIN)
        if (!producer->sourceTexture || !producer->context.device
            || command->command_buffer != producer->context.queue)
            return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        const auto native = producer->sourceTexture->nativeTexture();
        if (!native.object) return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        auto *texture = reinterpret_cast<ID3D11Texture2D *>(native.object);
        ID3D11Device *device = nullptr;
        texture->GetDevice(&device);
        const bool adoptedDevice = device == producer->context.device;
        if (device) device->Release();
        if (!adoptedDevice) return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        producer->recorded = frame->info;
        *output = {native.object, 0, OPENNOW_STREAMER_GRAPHICS_API_D3D11,
                   OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8, OPENNOW_STREAMER_COLOR_SPACE_SDR709,
                   frame->info.width, frame->info.height,
                   command->frame_slot, 1, frame->info.presentation_time_ns};
        return OPENNOW_STREAMER_OK;
#else
        if (!producer->functions || !command->command_buffer
            || command->frame_slot >= producer->images.size())
            return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        if (producer->sourceTexture) {
            const auto native = producer->sourceTexture->nativeTexture();
            VkImageMemoryBarrier barrier{};
            barrier.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER;
            barrier.srcAccessMask = VK_ACCESS_MEMORY_WRITE_BIT | VK_ACCESS_MEMORY_READ_BIT;
            barrier.dstAccessMask = VK_ACCESS_SHADER_READ_BIT;
            barrier.oldLayout = VkImageLayout(native.layout);
            barrier.newLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
            barrier.srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
            barrier.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
            barrier.image = VkImage(native.object);
            barrier.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
            producer->functions->vkCmdPipelineBarrier(
                reinterpret_cast<VkCommandBuffer>(command->command_buffer),
                VK_PIPELINE_STAGE_ALL_COMMANDS_BIT, VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT,
                0, 0, nullptr, 0, nullptr, 1, &barrier);
            producer->sourceTexture->setNativeLayout(VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL);
            producer->recorded = frame->info;
            *output = {native.object, 0, OPENNOW_STREAMER_GRAPHICS_API_VULKAN,
                       OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8, OPENNOW_STREAMER_COLOR_SPACE_SDR709,
                       frame->info.width, frame->info.height,
                       command->frame_slot, 1, frame->info.presentation_time_ns};
            return OPENNOW_STREAMER_OK;
        }
        auto &image = producer->images[command->frame_slot];
        if (!image.image && !producer->createImage(image))
            return OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE;
        auto *f = producer->functions;
        const auto cb = reinterpret_cast<VkCommandBuffer>(command->command_buffer);
        VkImageMemoryBarrier barrier{};
        barrier.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER;
        barrier.srcAccessMask = image.initialized ? VK_ACCESS_SHADER_READ_BIT : 0;
        barrier.dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
        barrier.oldLayout = image.initialized ? VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL
                                             : VK_IMAGE_LAYOUT_UNDEFINED;
        barrier.newLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
        barrier.srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        barrier.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        barrier.image = image.image;
        barrier.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
        f->vkCmdPipelineBarrier(cb, image.initialized ? VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT
                                                     : VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
                                VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, nullptr, 0, nullptr, 1, &barrier);
        const VkClearColorValue color{{0.25f, 0.5f, 0.75f, 1.0f}};
        f->vkCmdClearColorImage(cb, image.image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                               &color, 1, &barrier.subresourceRange);
        barrier.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
        barrier.dstAccessMask = VK_ACCESS_SHADER_READ_BIT;
        barrier.oldLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
        barrier.newLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
        f->vkCmdPipelineBarrier(cb, VK_PIPELINE_STAGE_TRANSFER_BIT,
                               VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 0, 0, nullptr,
                               0, nullptr, 1, &barrier);
        image.initialized = true;
        producer->recorded = frame->info;
        *output = {quint64(image.image), 0, OPENNOW_STREAMER_GRAPHICS_API_VULKAN,
                   OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8, OPENNOW_STREAMER_COLOR_SPACE_SDR709,
                   frame->info.width, frame->info.height,
                   command->frame_slot, 1, frame->info.presentation_time_ns};
        return OPENNOW_STREAMER_OK;
#endif
    };
    api.releaseFrame = [](OpenNowStreamerFrame *token) {
        std::unique_ptr<Producer::Frame> frame(reinterpret_cast<Producer::Frame *>(token));
        frame->producer->released = frame->info;
        ++frame->producer->releaseCount;
        return OPENNOW_STREAMER_OK;
    };
    api.sceneGraphShutdown = [](const OpenNowStreamer *handle) {
        auto *producer = reinterpret_cast<Producer *>(const_cast<OpenNowStreamer *>(handle));
        producer->clearImages();
        ++producer->shutdownCount;
        return OPENNOW_STREAMER_OK;
    };
    return api;
}
}
#endif

class NativeFrameGenerationTest final : public QObject
{
    Q_OBJECT
private:
#if !defined(Q_OS_WIN) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
    QVulkanInstance m_instance;
#endif
    std::unique_ptr<QRhi> m_rhi;
    std::atomic<int> m_validationErrors = 0;

    static QImage movingScene(QSize size, int shift)
    {
        QImage image(size, QImage::Format_RGBA8888);
        for (int y = 0; y < size.height(); ++y) {
            auto *row = image.scanLine(y);
            for (int x = 0; x < size.width(); ++x) {
                const double px = x - shift;
                row[x * 4] = uchar(128 + 52 * std::sin(px * 0.091 + y * 0.033)
                                      + 48 * std::cos(px * 0.047 - y * 0.077));
                row[x * 4 + 1] = uchar(128 + 53 * std::cos(px * 0.067 + y * 0.059)
                                          + 42 * std::sin(px * 0.029 - y * 0.113));
                row[x * 4 + 2] = uchar(128 + 51 * std::sin(px * 0.053 - y * 0.043)
                                          + 43 * std::cos(px * 0.107 + y * 0.023));
                row[x * 4 + 3] = 255;
            }
        }
        return image;
    }

    static double imageError(const QImage &actual, const QImage &expected)
    {
        double sum = 0;
        int count = 0;
        for (int y = 48; y < actual.height() - 48; ++y) {
            for (int x = 64; x < actual.width() - 64; ++x) {
                for (int c = 0; c < 3; ++c) {
                    const int delta = int(actual.constScanLine(y)[x * 4 + c])
                        - int(expected.constScanLine(y)[x * 4 + c]);
                    sum += delta * delta;
                    ++count;
                }
            }
        }
        return sum / count;
    }

private slots:
    void initTestCase()
    {
#if defined(Q_OS_WIN)
        QRhiD3D11InitParams params;
        params.enableDebugLayer = qEnvironmentVariableIsSet("OPENNOW_FRAMEGEN_VALIDATION");
        m_rhi.reset(QRhi::create(QRhi::D3D11, &params, QRhi::PreferSoftwareRenderer));
        QVERIFY2(m_rhi, "The Windows integration test requires a D3D11 software device");
        QVERIFY(m_rhi->isTextureFormatSupported(QRhiTexture::RGBA16F, QRhiTexture::RenderTarget));
#elif QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        m_instance.setApiVersion(QVersionNumber(1, 1));
        m_instance.setExtensions(QRhiVulkanInitParams::preferredInstanceExtensions());
        if (qEnvironmentVariableIsSet("OPENNOW_FRAMEGEN_VALIDATION")) {
            QVERIFY2(m_instance.supportedLayers().contains("VK_LAYER_KHRONOS_validation"),
                     "Vulkan validation requested but Khronos layer is unavailable");
            m_instance.setLayers({"VK_LAYER_KHRONOS_validation"});
            m_instance.installDebugOutputFilter(QVulkanInstance::DebugUtilsFilter(
                [this](QVulkanInstance::DebugMessageSeverityFlags severity,
                       QVulkanInstance::DebugMessageTypeFlags, const void *) {
                    if (severity.testFlag(QVulkanInstance::ErrorSeverity)) ++m_validationErrors;
                    return false;
                }));
        }
        if (!m_instance.create()) QSKIP("Vulkan instance unavailable");
        QRhiVulkanInitParams params;
        params.inst = &m_instance;
        m_rhi.reset(QRhi::create(QRhi::Vulkan, &params));
        if (!m_rhi) QSKIP("Vulkan QRhi unavailable");
        if (!m_rhi->isTextureFormatSupported(QRhiTexture::RGBA16F, QRhiTexture::RenderTarget))
            QSKIP("Renderable RGBA16F unavailable");
#else
        QSKIP("Native frame generation integration requires D3D11 or Vulkan");
#endif
    }

    void productionCallback_data()
    {
        QTest::addColumn<bool>("enabled");
        QTest::addColumn<int>("timestampMode");
        QTest::addColumn<bool>("moving");
        QTest::newRow("absent-pts") << true << 0 << false;
        QTest::newRow("repeated-pts") << true << 1 << false;
        QTest::newRow("grouped-pts") << true << 2 << false;
        QTest::newRow("off-control") << false << 0 << false;
        QTest::newRow("moving-repeated-pts") << true << 1 << true;
        QTest::newRow("moving-grouped-pts") << true << 2 << true;
        QTest::newRow("moving-off-control") << false << 1 << true;
    }

    void productionCallback()
    {
#if defined(Q_OS_WIN) || (QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>))
        QFETCH(bool, enabled);
        QFETCH(int, timestampMode);
        QFETCH(bool, moving);
        Producer producer;
#if !defined(Q_OS_WIN)
        producer.instance = &m_instance;
#endif
        producerToCreate = &producer;
        const auto resetProducer = qScopeGuard([] { producerToCreate = nullptr; });
        NativeStreamRuntime runtime(producerApi());
        QSignalSpy errors(&runtime, &NativeStreamRuntime::presentationError);
        QVERIFY(runtime.start());
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("integration-start")}}));
        QTRY_VERIFY(runtime.presentationAllowed());

        const QSize size = moving ? QSize(320, 224) : QSize(64, 64);
        std::array<QImage, 17> sources;
        std::array<QImage, 17> halfway;
        std::unique_ptr<QRhiTexture> sourceTexture;
        if (moving || m_rhi->backend() == QRhi::D3D11) {
            for (int i = 0; i < int(sources.size()); ++i) {
                if (moving) {
                    sources[i] = movingScene(size, i * 8);
                    halfway[i] = movingScene(size, i * 8 - 4);
                } else {
                    sources[i] = QImage(size, QImage::Format_RGBA8888);
                    sources[i].fill(QColor(64, 128, 191, 255));
                }
            }
            sourceTexture.reset(m_rhi->newTexture(QRhiTexture::RGBA8, size));
            QVERIFY(sourceTexture->create());
            producer.sourceTexture = sourceTexture.get();
        }
        auto texture = std::unique_ptr<QRhiTexture>(m_rhi->newTexture(
            QRhiTexture::RGBA8, size, 1, QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
        QVERIFY(texture->create());
        auto depth = std::unique_ptr<QRhiRenderBuffer>(m_rhi->newRenderBuffer(
            QRhiRenderBuffer::DepthStencil, size));
        QVERIFY(depth->create());
        QRhiTextureRenderTargetDescription description(QRhiColorAttachment(texture.get()));
        description.setDepthStencilBuffer(depth.get());
        auto target = std::unique_ptr<QRhiTextureRenderTarget>(m_rhi->newTextureRenderTarget(description));
        auto pass = std::unique_ptr<QRhiRenderPassDescriptor>(target->newCompatibleRenderPassDescriptor());
        target->setRenderPassDescriptor(pass.get());
        QVERIFY(target->create());
        auto callback = createNativeStreamRenderCallback(&runtime);
        const auto release = qScopeGuard([&] { callback->releaseResources(); });
        callback->setFrameGeneration(enabled, 165.0);
        QMatrix4x4 matrix = m_rhi->clipSpaceCorrMatrix();
        matrix.ortho(0, size.width(), size.height(), 0, -1, 1);
        const QRectF bounds(QPointF(0, 0), QSizeF(size));
        callback->setComposition(matrix, bounds, bounds, 1);

        QImage presented;
        const auto present = [&](bool swapped = true) {
            QRhiCommandBuffer *cb = nullptr;
            if (m_rhi->beginOffscreenFrame(&cb) != QRhi::FrameOpSuccess) return false;
            if (sourceTexture && producer.next) {
                auto *upload = m_rhi->nextResourceUpdateBatch();
                upload->uploadTexture(sourceTexture.get(), sources[producer.next->sequence]);
                cb->resourceUpdate(upload);
            }
            callback->initialize(m_rhi.get(), cb, target.get());
            callback->prepareFrame(cb);
            cb->beginPass(target.get(), Qt::black, {1.0f, 0});
            cb->setViewport(QRhiViewport(0, 0, size.width(), size.height()));
            cb->setScissor(QRhiScissor(0, 0, size.width(), size.height()));
            callback->recordFrame(cb, bounds.toRect());
            cb->endPass();
            QRhiReadbackResult readback;
            bool completed = false;
            readback.completed = [&] { completed = true; };
            auto *updates = m_rhi->nextResourceUpdateBatch();
            updates->readBackTexture(QRhiReadbackDescription(texture.get()), &readback);
            cb->resourceUpdate(updates);
            const auto ended = m_rhi->endOffscreenFrame();
            callback->finishFrame();
            if (swapped) callback->frameSwapped();
            if (ended != QRhi::FrameOpSuccess || !completed
                || readback.data.size() != size.width() * size.height() * 4)
                return false;
            presented = QImage(reinterpret_cast<const uchar *>(readback.data.constData()),
                               size.width(), size.height(), QImage::Format_RGBA8888).copy();
            if (moving) return true;
            const auto *pixel = reinterpret_cast<const unsigned char *>(readback.data.constData())
                + (32 * 64 + 32) * 4;
            return qAbs(int(pixel[0]) - 64) <= 2 && qAbs(int(pixel[1]) - 128) <= 2
                && qAbs(int(pixel[2]) - 191) <= 2 && pixel[3] == 255;
        };

        int generatedPairs = 0;
        quint64 timestamp = 0;
        quint64 previousTimestamp = 0;
        for (quint64 sequence = 1; sequence <= 12; ++sequence) {
            if (sequence > 1) std::this_thread::sleep_for(std::chrono::milliseconds(20));
            timestamp = timestampMode == 0 ? 0 : timestampMode == 1 ? 1'000'000'000
                : 1'000'000'000 + ((sequence - 1) / 3) * 60'000'000;
            producer.next = OpenNowStreamerFrameInfo{uint32_t(size.width()), uint32_t(size.height()),
                                                     sequence, timestamp};
            QVERIFY2(present(false), qPrintable(runtime.lastError()));
            QCOMPARE(producer.acquired.sequence, sequence);
            QCOMPARE(producer.acquired.presentation_time_ns, timestamp);
            QCOMPARE(producer.recorded.sequence, sequence);
            QCOMPARE(producer.recorded.presentation_time_ns, timestamp);
            QCOMPARE(producer.released.sequence, sequence);
            QCOMPARE(producer.released.presentation_time_ns, timestamp);
            QCOMPARE(producer.releaseCount, producer.sourceCount);
            const auto snapshot = callback->frameGenerationStats();
            const auto status = snapshot.value(QStringLiteral("status")).toString();
            const auto timestampDelta = sequence == 1 ? 0 : timestamp - previousTimestamp;
            previousTimestamp = timestamp;
            if (!enabled) {
                QCOMPARE(status, QStringLiteral("off"));
                QVERIFY(!callback->needsFrame());
                if (moving) QCOMPARE(presented, sources[sequence]);
                callback->frameSwapped();
                continue;
            }
            if (status != QStringLiteral("active")) {
                if (moving) QCOMPARE(presented, sources[sequence]);
                callback->frameSwapped();
                continue;
            }
            QVERIFY(callback->needsFrame());
            const QImage midpoint = presented;
            if (moving) {
                const double midpointError = imageError(midpoint, halfway[sequence]);
                const double currentError = imageError(sources[sequence], halfway[sequence]);
                QImage blend(size, QImage::Format_RGBA8888);
                for (int y = 0; y < size.height(); ++y)
                    for (int x = 0; x < size.width() * 4; ++x)
                        blend.scanLine(y)[x] = (int(sources[sequence - 1].constScanLine(y)[x])
                                                + int(sources[sequence].constScanLine(y)[x])) / 2;
                const double blendError = imageError(blend, halfway[sequence]);
                qInfo() << "Native callback midpoint/current halfway-motion error:"
                        << midpointError << currentError;
                QVERIFY(midpoint != sources[sequence]);
                QVERIFY2(midpointError < currentError * 0.45,
                         "Production callback output must move halfway, not repeat the current source");
                QVERIFY2(midpointError < blendError * 0.70,
                         "Production callback output must move texture features, not merely crossfade them");
            }
            QCOMPARE(snapshot.value(QStringLiteral("timingSource")).toString(),
                     QStringLiteral("arrival-cadence"));
            QCOMPARE(snapshot.value(QStringLiteral("sequenceDelta")).toULongLong(), 1);
            QCOMPARE(snapshot.value(QStringLiteral("timestampDeltaMs")).toDouble(),
                     double(timestampDelta) / 1.0e6);
            QCOMPARE(snapshot.value(QStringLiteral("refreshRateHz")).toDouble(), 165.0);
            const int sourceCount = producer.sourceCount;
            const int empty = producer.emptyCount;
            std::this_thread::sleep_for(std::chrono::milliseconds(3));
            QVERIFY(present(false));
            QVERIFY(callback->needsFrame());
            if (moving) QCOMPARE(presented, midpoint);
            callback->frameSwapped();
            std::this_thread::sleep_for(std::chrono::milliseconds(7));
            QVERIFY(present());
            QVERIFY(!callback->needsFrame());
            if (moving) QCOMPARE(presented, sources[sequence]);
            QCOMPARE(producer.sourceCount, sourceCount);
            QCOMPARE(producer.releaseCount, sourceCount);
            QCOMPARE(producer.emptyCount, empty + 2);
            QCOMPARE(producer.released.sequence, sequence);
            QCOMPARE(producer.released.presentation_time_ns, timestamp);
            const auto drained = callback->frameGenerationStats();
            for (const auto &key : {QStringLiteral("timingSource"), QStringLiteral("sequenceDelta"),
                                    QStringLiteral("timestampDeltaMs"), QStringLiteral("arrivalDeltaMs"),
                                    QStringLiteral("sourceIntervalMs")})
                QCOMPARE(drained.value(key), snapshot.value(key));
            ++generatedPairs;
        }
        if (enabled) {
            QVERIFY2(generatedPairs >= 3, "Production callback never sustained frame generation from native source arrivals");
            std::this_thread::sleep_for(std::chrono::milliseconds(20));
            producer.next = OpenNowStreamerFrameInfo{uint32_t(size.width()), uint32_t(size.height()),
                                                     14, timestamp};
            QVERIFY(present());
            QCOMPARE(callback->frameGenerationStats().value(QStringLiteral("status")).toString(),
                     QStringLiteral("discontinuity"));
            QVERIFY(!callback->needsFrame());
            QCOMPARE(callback->frameGenerationStats().value(QStringLiteral("rejectionReason")).toString(),
                     QStringLiteral("sequence-gap"));
            std::this_thread::sleep_for(std::chrono::milliseconds(20));
            producer.next = OpenNowStreamerFrameInfo{uint32_t(size.width()), uint32_t(size.height()),
                                                     15, 2'000'000'000};
            QVERIFY(present());
            if (callback->needsFrame()) {
                std::this_thread::sleep_for(std::chrono::milliseconds(10));
                QVERIFY(present());
                QVERIFY(!callback->needsFrame());
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(20));
            producer.next = OpenNowStreamerFrameInfo{uint32_t(size.width()), uint32_t(size.height()),
                                                     16, 1'999'999'999};
            QVERIFY(present());
            QCOMPARE(callback->frameGenerationStats().value(QStringLiteral("status")).toString(),
                     QStringLiteral("discontinuity"));
            QCOMPARE(callback->frameGenerationStats().value(QStringLiteral("rejectionReason")).toString(),
                     QStringLiteral("timestamp-regression"));
            QCOMPARE(producer.released.presentation_time_ns, 1'999'999'999);
            QVERIFY(!callback->needsFrame());
        }
        callback->releaseResources();
        QCOMPARE(producer.shutdownCount, 1);
        QVERIFY(runtime.shutdown());
        QCoreApplication::processEvents();
        QCOMPARE(errors.size(), 0);
        QCOMPARE(m_validationErrors.load(), 0);
#endif
    }

    void cleanupTestCase()
    {
        m_rhi.reset();
        QCOMPARE(m_validationErrors.load(), 0);
    }
};

QTEST_MAIN(NativeFrameGenerationTest)
#include "tst_nativeframegeneration.moc"
