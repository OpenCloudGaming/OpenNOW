#include "streaming/StreamVideoItem.h"
#include "streaming/rendering/LinuxVulkanGraphics.h"
#include "streaming/NativeStreamRuntime.h"
#include "streaming/rendering/StreamVideoTextureRenderer.h"
#include "input/platform/WaylandPointerCapture.h"
#include "input/platform/MacPointerCapture.h"

#include <QGuiApplication>
#include <QBuffer>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QCursor>
#include <QPixmap>
#include <QScopeGuard>
#include <QtQml/qqml.h>
#include <QQuickWindow>
#include <QSGSimpleRectNode>
#include <QSignalSpy>
#include <QTest>

#include <atomic>
#include <memory>

#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
#include <vulkan/vulkan.h>
#endif

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

class TestRenderCallback final : public StreamVideoRenderCallback
{
public:
    void initialize(QRhi *rhi,
                    QRhiCommandBuffer *commandBuffer,
                    QRhiRenderTarget *renderTarget) override
    {
        validContext.store(rhi && commandBuffer && renderTarget);
        ++initializeCount;
    }

    void recordFrame(QRhiCommandBuffer *commandBuffer, const QRect &viewport) override
    {
        validContext.store(validContext.load() && commandBuffer);
        viewportWidth.store(viewport.width());
        viewportHeight.store(viewport.height());
        ++frameCount;
    }

    void prepareFrame(QRhiCommandBuffer *commandBuffer) override
    {
        validContext.store(validContext.load() && commandBuffer);
        ++prepareCount;
    }

    void setUpscalingTarget(const QSize &target) override
    {
        upscaleWidth.store(target.width());
        upscaleHeight.store(target.height());
    }

    void finishFrame() override
    {
        ++finishCount;
    }

    void setUpscalingEnhancement(int sharpness, int denoise) override
    {
        upscaleSharpness.store(sharpness);
        upscaleDenoise.store(denoise);
    }

    void releaseResources() override
    {
        ++releaseCount;
    }

    std::atomic_bool validContext = false;
    std::atomic_int initializeCount = 0;
    std::atomic_int frameCount = 0;
    std::atomic_int prepareCount = 0;
    std::atomic_int finishCount = 0;
    std::atomic_int releaseCount = 0;
    std::atomic_int viewportWidth = 0;
    std::atomic_int viewportHeight = 0;
    std::atomic_int upscaleWidth = 0;
    std::atomic_int upscaleHeight = 0;
    std::atomic_int upscaleSharpness = 0;
    std::atomic_int upscaleDenoise = 0;
};

// Exercise the production import/material with a GPU texture, without a remote
// account or a synthetic alternate presenter. Readbacks are test-only.
class TextureRenderCallback final : public StreamVideoRenderCallback
{
public:
    explicit TextureRenderCallback(bool externalTexture = false) : m_externalTexture(externalTexture) {}
    void initialize(QRhi *rhi, QRhiCommandBuffer *, QRhiRenderTarget *target) override
    {
        if (m_rhi != rhi) releaseResources();
        m_rhi = rhi;
        renderer.initialize(rhi, target);
        directTarget.store(target->resourceType() == QRhiResource::SwapChainRenderTarget);
    }
    void setComposition(const QMatrix4x4 &matrix, const QRectF &bounds,
                        const QRectF &viewport, float opacity) override
    {
        renderer.setComposition(matrix, bounds, viewport, opacity);
    }
    void prepareFrame(QRhiCommandBuffer *cb) override
    {
        if (!showVideo.load()) {
            renderer.clearFrames();
            imported.store(false);
            return;
        }
        if (!renderer.prepare(cb)) return;
        if (!texture) {
            texture.reset(m_rhi->newTexture(QRhiTexture::RGBA8, QSize(4, 4)));
            if (!texture->create()) return;
            QImage image(4, 4, QImage::Format_RGBA8888);
            for (int y = 0; y < 4; ++y)
                for (int x = 0; x < 4; ++x)
                    image.setPixelColor(x, y, y < 2 ? Qt::red : Qt::green);
            auto *updates = m_rhi->nextResourceUpdateBatch();
            updates->uploadTexture(texture.get(), image);
            cb->resourceUpdate(updates);
        }
#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        if (textureWasSampled && m_rhi->backend() == QRhi::Vulkan)
            texture->setNativeLayout(VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL);
#endif
        imported.store(renderer.importFrame(m_rhi->currentFrameSlot(), texture->nativeTexture(),
                                            QRhiTexture::RGBA8, QSize(4, 4)));
        importedSlots.store(int(renderer.importedSlotCount()));
        if (m_externalTexture)
            imported.store(imported.load() && renderer.selectTexture(texture.get()));
    }
    void setClip(bool enabled, int reference) override { stencil = enabled; stencilReference = reference; }
    void recordFrame(QRhiCommandBuffer *cb, const QRect &) override
    {
        renderer.render(cb, stencil, stencilReference);
        if (imported.load()) textureWasSampled = true;
        ++frames;
    }
    void finishFrame() override {}
    void releaseResources() override
    {
        renderer.release();
        texture.reset();
        textureWasSampled = false;
        m_rhi = nullptr;
        ++releases;
    }
    std::atomic_bool imported = false;
    std::atomic_bool directTarget = false;
    std::atomic_int frames = 0;
    std::atomic_int importedSlots = 0;
    std::atomic_bool showVideo = true;
    std::atomic_int releases = 0;
private:
    bool m_externalTexture = false;
    QRhi *m_rhi = nullptr;
    StreamVideoTextureRenderer renderer;
    std::unique_ptr<QRhiTexture> texture;
    bool textureWasSampled = false;
    bool stencil = false;
    int stencilReference = 0;
};

class WhiteOverlay final : public QQuickItem
{
public:
    explicit WhiteOverlay(QQuickItem *parent) : QQuickItem(parent) { setFlag(ItemHasContents); }
protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override
    {
        auto *node = static_cast<QSGSimpleRectNode *>(old);
        if (!node) node = new QSGSimpleRectNode;
        node->setRect(boundingRect());
        node->setColor(Qt::white);
        return node;
    }
};

class StreamVideoItemTest final : public QObject
{
    Q_OBJECT

    struct CursorSession {
        inline static OpenNowStreamerConfig callbacks;

        static NativeStreamRuntime::Api api()
        {
            NativeStreamRuntime::Api api{};
            api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
                callbacks = *config;
                *output = reinterpret_cast<OpenNowStreamer *>(new int(1));
                return OPENNOW_STREAMER_OK;
            };
            api.destroy = [](OpenNowStreamer *handle) {
                delete reinterpret_cast<int *>(handle);
                return OPENNOW_STREAMER_OK;
            };
            api.send = [](const OpenNowStreamer *, const std::uint8_t *, std::size_t) {
                return OPENNOW_STREAMER_OK;
            };
            api.setCaptureActive = [](const OpenNowStreamer *, bool, bool, std::uintptr_t, bool *raw) {
                *raw = false;
                return OPENNOW_STREAMER_OK;
            };
            return api;
        }

        bool start()
        {
            if (!runtime.start()) return false;
            StreamVideoItem::setNativeStreamRuntime(&runtime);
            if (!runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                               {QStringLiteral("id"), QStringLiteral("cursor-test")}})) return false;
            const QByteArray ready = R"({"id":"cursor-test","type":"ok"})";
            callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(ready.constData()),
                                        ready.size(), callbacks.user_data);
            window.resize(640, 480);
            window.show();
            window.requestActivate();
            return true;
        }

        void composition(bool composited)
        {
            const auto bytes = QJsonDocument(QJsonObject{
                {QStringLiteral("type"), QStringLiteral("cursor-capture")},
                {QStringLiteral("startId"), startId},
                {QStringLiteral("composited"), composited}}).toJson(QJsonDocument::Compact);
            callbacks.event_callback(reinterpret_cast<const std::uint8_t *>(bytes.constData()),
                                     bytes.size(), callbacks.user_data);
        }

        ~CursorSession() { StreamVideoItem::setNativeStreamRuntime(nullptr); }

        NativeStreamRuntime runtime{api()};
        QQuickWindow window;
        QString startId = QStringLiteral("cursor-test");
    };

private slots:
    void macPointerCaptureOwnsMotionAndReleasesAcrossTransitions()
    {
        struct PointerState {
            bool associated = true;
            bool hidden = false;
            bool failCapture = false;
            std::function<void(QPointF)> motion;
        } pointer;
        class Operations final : public MacPointerCapture::NativeOperations {
        public:
            explicit Operations(PointerState &state) : state(state) {}
            QString associate(bool value) override {
                if (!value && state.failCapture) return QStringLiteral("capture unavailable");
                state.associated = value;
                return {};
            }
            QString center(QWindow *, const QRect &) override { return {}; }
            QString setHidden(bool value) override { state.hidden = value; return {}; }
            QString startMotion(QWindow *, std::function<void(QPointF)> callback) override
            { state.motion = std::move(callback); return {}; }
            void stopMotion() override { state.motion = {}; }
            PointerState &state;
        };
        static OpenNowStreamerConfig callbacks;
        static QList<QPoint> motions;
        motions.clear();
        NativeStreamRuntime::Api api{};
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbacks = *config;
            *output = reinterpret_cast<OpenNowStreamer *>(new int(1));
            return OPENNOW_STREAMER_OK;
        };
        api.destroy = [](OpenNowStreamer *handle) {
            delete reinterpret_cast<int *>(handle);
            return OPENNOW_STREAMER_OK;
        };
        api.send = [](const OpenNowStreamer *, const std::uint8_t *, std::size_t) {
            return OPENNOW_STREAMER_OK;
        };
        api.setCaptureActive = [](const OpenNowStreamer *, bool, bool, std::uintptr_t, bool *raw) {
            *raw = false;
            return OPENNOW_STREAMER_OK;
        };
        api.submitMouseRelative = [](const OpenNowStreamer *, std::int16_t x, std::int16_t y) {
            motions.append(QPoint(x, y));
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("mac-capture")}}));
        const QByteArray ready = R"({"id":"mac-capture","type":"ok"})";
        callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(ready.constData()),
                                    ready.size(), callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(
            std::make_unique<MacPointerCapture>(std::make_unique<Operations>(pointer)),
            true, window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        item->setVideoSize(QSize(1920, 1080));
        auto *overlay = new QQuickItem(window.contentItem());
        overlay->setVisible(false);
        for (const bool fullscreen : {false, true}) {
            if (fullscreen) window.showFullScreen();
            else window.showNormal();
            window.requestActivate();
            QTRY_VERIFY(window.isActive());
            item->setSize(window.size());
            item->forceActiveFocus();
            item->setRelativeMouse(true);
            QTRY_VERIFY(item->captureActive());
            QVERIFY(item->m_macPointer->locked());
            QVERIFY(!pointer.associated);
            QVERIFY(pointer.hidden);
            QVERIFY(pointer.motion);
            const auto move = pointer.motion;
            motions.clear();
            move(QPointF(5, -3));
            QCOMPARE(motions, QList<QPoint>{QPoint(5, -3)});
            QMouseEvent synthetic(QEvent::MouseMove, QPointF(300, 200), QPointF(300, 200),
                                  Qt::NoButton, Qt::NoButton, Qt::NoModifier);
            item->mouseMoveEvent(&synthetic);
            QCOMPARE(motions.size(), 1);
            item->togglePointerLock();
            QVERIFY(pointer.associated);
            QVERIFY(!pointer.hidden);
            QVERIFY(!pointer.motion);
            item->applyRemoteCursor(QByteArray::fromHex("0000"));
            QVERIFY(!item->relativeMouse());
            QVERIFY(!item->m_macPointer->locked());
            item->togglePointerLock();
            QVERIFY(item->m_macPointer->locked());
            item->applyRemoteCursor(QByteArray::fromHex("0001"));
            QVERIFY(item->m_macPointer->locked());
            overlay->setVisible(true);
            overlay->forceActiveFocus();
            QTRY_VERIFY(!item->captureActive());
            QVERIFY(pointer.associated);
            QVERIFY(!pointer.hidden);
            QCOMPARE(item->cursor().shape(), Qt::ArrowCursor);
            overlay->setVisible(false);
            item->forceActiveFocus();
            QTRY_VERIFY(item->m_macPointer->locked());
            item->setInputEnabled(false);
            QVERIFY(pointer.associated);
            QVERIFY(!pointer.hidden);
            item->setInputEnabled(true);
            QTRY_VERIFY(item->m_macPointer->locked());
        }
        item->togglePointerLock();
        pointer.failCapture = true;
        item->togglePointerLock();
        QVERIFY(!item->captureActive());
        QVERIFY(!item->m_macPointer->locked());
        QVERIFY(pointer.associated);
        QVERIFY(!pointer.hidden);
        QVERIFY(!item->inputCaptureError().isEmpty());
        item->setShortcutBindings({{QStringLiteral("toggle-pointer-lock"), QStringLiteral("F8")}});
        connect(item, &StreamVideoItem::localShortcutRequested, item, [item](const QString &action) {
            if (action == QStringLiteral("toggle-pointer-lock")) item->togglePointerLock();
        });
        QKeyEvent recovery(QEvent::KeyPress, Qt::Key_F8, Qt::NoModifier);
        item->keyPressEvent(&recovery);
        QVERIFY(recovery.isAccepted());
        QVERIFY(!item->relativeMouse());
        pointer.failCapture = false;
        item->togglePointerLock();
        QVERIFY(item->m_macPointer->locked());
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("stop")}}));
        QTRY_VERIFY(!item->captureActive());
        QVERIFY(pointer.associated);
        QVERIFY(!pointer.hidden);
        QVERIFY(!item->m_manualRelativeMouse.has_value());
        delete item;
        QVERIFY(pointer.associated);
        QVERIFY(!pointer.hidden);
    }

    void manualPointerLockOutranksServerCursorMessages()
    {
        StreamVideoItem item;
        const auto hidden = QByteArray::fromHex("0000");
        const auto visible = QByteArray::fromHex("0001");
        item.applyRemoteCursor(hidden);
        QVERIFY(item.relativeMouse());
        item.togglePointerLock();
        QVERIFY(!item.relativeMouse());
        item.applyRemoteCursor(visible);
        item.applyRemoteCursor(hidden);
        QVERIFY(!item.relativeMouse());
        item.releaseInput();
        item.applyRemoteCursor(hidden);
        QVERIFY(!item.relativeMouse());
        item.togglePointerLock();
        QVERIFY(item.relativeMouse());
        item.applyRemoteCursor(visible);
        QVERIFY(item.relativeMouse());
        item.setVisible(false);
        QVERIFY(!item.m_manualRelativeMouse.has_value());
        item.setVisible(true);
        item.applyRemoteCursor(visible);
        QVERIFY(!item.relativeMouse());
    }

    void manualPointerUnlockClearsHeldInputAndDeferredMode()
    {
        StreamVideoItem item;
        item.setRelativeMouse(true);
        item.m_pressedKeys.insert(1, {0x57, 0});
        item.m_pressedMouseButtons.insert(1);
        item.applyRemoteCursor(QByteArray::fromHex("0001"));
        QVERIFY(item.m_pendingRelativeMouse.has_value());
        item.togglePointerLock();
        QVERIFY(!item.relativeMouse());
        QVERIFY(item.m_pressedKeys.isEmpty());
        QVERIFY(item.m_pressedMouseButtons.isEmpty());
        QVERIFY(!item.m_pendingRelativeMouse.has_value());
        item.applyRemoteCursor(QByteArray::fromHex("0000"));
        QVERIFY(!item.relativeMouse());
    }

    void macCursorVisibilityTracksCaptureAndServerHandoff()
    {
        StreamVideoItem item;
        item.m_usesMacPointerCapture = true;
        item.m_captureActive = true;
        item.updateLocalCursor();
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        item.m_manualRelativeMouse = false;
        item.updateLocalCursor();
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
        item.m_manualRelativeMouse.reset();
        item.m_serverCursorComposited = false;
        item.m_remoteCursorKnown = true;
        item.setRemoteCursorShape(QCursor(Qt::CrossCursor));
        QCOMPARE(item.cursor().shape(), Qt::CrossCursor);
        item.m_relativeMouse = true;
        item.updateLocalCursor();
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        item.releaseInput();
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
        item.m_captureActive = false;
        item.updateLocalCursor();
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
    }

    void cursorOwnershipSurvivesOverlaysFullscreenAndRestart()
    {
        CursorSession session;
        QVERIFY(session.start());
        StreamVideoItem item(session.window.contentItem());
        item.m_usesMacPointerCapture = false;
        item.setRenderCallback({});
        item.setSize(session.window.size());
        item.forceActiveFocus();
        QTRY_VERIFY(session.runtime.inputAllowed());
        QTRY_VERIFY(session.window.isActive());
        QTRY_VERIFY(item.hasActiveFocus());
        QVERIFY(item.isVisible());
        QTRY_VERIFY(item.captureActive());
        QVERIFY(!item.m_remoteCursorKnown);
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        session.composition(false);
        QTRY_COMPARE(item.cursor().shape(), Qt::ArrowCursor);
        QVERIFY(!item.m_remoteCursorKnown);

        QQuickItem overlay(session.window.contentItem());
        overlay.setVisible(false);
        for (const bool fullscreen : {false, true}) {
            if (fullscreen) session.window.showFullScreen();
            else session.window.showNormal();
            session.window.requestActivate();
            QTRY_VERIFY(session.window.isActive());
            item.setSize(session.window.size());
            item.forceActiveFocus();
            QTRY_VERIFY(item.captureActive());
            item.applyRemoteCursor(QByteArray::fromHex("0002"));
            QCOMPARE(item.cursor().shape(), Qt::IBeamCursor);
            session.composition(true);
            QTRY_COMPARE(item.cursor().shape(), Qt::BlankCursor);
            session.composition(false);
            QTRY_COMPARE(item.cursor().shape(), Qt::IBeamCursor);
            item.applyRemoteCursor(QByteArray::fromHex("0000"));
            QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
            overlay.setVisible(true);
            overlay.forceActiveFocus();
            QTRY_VERIFY(!item.captureActive());
            QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
            item.applyRemoteCursor(QByteArray::fromHex("000c"));
            QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
            item.applyRemoteCursor(QByteArray::fromHex("0000"));
            QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
            overlay.setVisible(false);
            item.forceActiveFocus();
            QTRY_VERIFY(item.captureActive());
            QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
            item.setInputEnabled(false);
            QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
            item.setInputEnabled(true);
            QTRY_VERIFY(item.captureActive());
            QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
            item.applyRemoteCursor(QByteArray::fromHex("000c"));
            QCOMPARE(item.cursor().shape(), Qt::PointingHandCursor);
        }

        QVERIFY(session.runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                                      {QStringLiteral("id"), QStringLiteral("replacement")}}));
        session.startId = QStringLiteral("replacement");
        QVERIFY(!item.captureActive());
        QVERIFY(!item.m_remoteCursorKnown);
        QVERIFY(item.m_serverCursorComposited);
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
        const QByteArray ready = R"({"id":"replacement","type":"ok"})";
        CursorSession::callbacks.response_callback(
            reinterpret_cast<const std::uint8_t *>(ready.constData()), ready.size(),
            CursorSession::callbacks.user_data);
        QTRY_VERIFY(item.captureActive());
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        session.composition(false);
        QTRY_COMPARE(item.cursor().shape(), Qt::ArrowCursor);
        item.applyRemoteCursor(QByteArray::fromHex("0000"));
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        QVERIFY(session.runtime.send({{QStringLiteral("type"), QStringLiteral("stop")}}));
        QVERIFY(!item.captureActive());
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
    }

    void cursorVisibilityPolicyIsTheSameAcrossPlatforms()
    {
        StreamVideoItem item;
        for (const bool mac : {false, true}) {
            item.m_usesMacPointerCapture = mac;
            for (const bool capture : {false, true}) {
                item.m_captureActive = capture;
                for (const bool composited : {false, true}) {
                    item.m_serverCursorComposited = composited;
                    for (const bool relative : {false, true}) {
                        item.m_relativeMouse = relative;
                        item.m_remoteCursor = QCursor(Qt::CrossCursor);
                        item.updateLocalCursor();
                        QCOMPARE(item.cursor().shape(), !capture ? Qt::ArrowCursor
                            : relative || composited ? Qt::BlankCursor : Qt::CrossCursor);
                    }
                }
            }
        }
        item.releaseInput();
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
    }

    void initTestCase()
    {
        registerStreamVideoItemQmlType();
    }

    void linuxDmabufRequiresEnabledExtensionsAndVulkanPrerequisites()
    {
#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        using namespace LinuxVulkanGraphics;
        const auto required = deviceExtensions();
        QVERIFY(hasDmabufImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                        required, required));
        QVERIFY(!hasDmabufImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                         {}, required));
        QVERIFY(!hasDmabufImportContract(QVersionNumber(1, 0), VK_API_VERSION_1_1,
                                         required, required));
        QVERIFY(!hasDmabufImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_0,
                                         required, required));
        const QByteArrayList mandatory = {"VK_KHR_external_memory_fd", "VK_EXT_external_memory_dma_buf",
                                         "VK_EXT_image_drm_format_modifier", "VK_KHR_image_format_list"};
        for (const auto &extension : mandatory) {
            auto missing = required;
            missing.removeAll(extension);
            QVERIFY(!hasDmabufImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                             missing, required));
            QVERIFY(!hasDmabufImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                             required, missing));
        }
        auto promoted = mandatory;
        promoted.removeAll("VK_KHR_image_format_list");
        QVERIFY(hasDmabufImportContract(QVersionNumber(1, 2), VK_API_VERSION_1_2,
                                        promoted, promoted));
        QVERIFY(!hasDmabufImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_2,
                                         promoted, promoted));
        QCOMPARE(enabledImportCapabilities(nullptr, VK_NULL_HANDLE), uint32_t(0));
#else
        QSKIP("Linux Vulkan capability contract");
#endif
    }

    void linuxSandRequiresExplicitForeignBufferImportSupport()
    {
#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
        using namespace LinuxVulkanGraphics;
        const QByteArrayList required = {"VK_KHR_external_memory_fd", "VK_EXT_external_memory_dma_buf",
                                        "VK_EXT_queue_family_foreign"};
        QVERIFY(hasDmabufBufferImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                              required, required));
        QVERIFY(!hasDmabufImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                         required, required));
        for (const auto &extension : required) {
            auto missing = required;
            missing.removeAll(extension);
            QVERIFY(!hasDmabufBufferImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                                   missing, required));
            QVERIFY(!hasDmabufBufferImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_1,
                                                   required, missing));
        }
        QVERIFY(!hasDmabufBufferImportContract(QVersionNumber(1, 0), VK_API_VERSION_1_1,
                                               required, required));
        QVERIFY(!hasDmabufBufferImportContract(QVersionNumber(1, 1), VK_API_VERSION_1_0,
                                               required, required));
#else
        QSKIP("Linux Vulkan capability contract");
#endif
    }

    void calculatesCenteredAspectFitViewport()
    {
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1920, 1080), QSize(1000, 1000)),
                 QRect(0, 219, 1000, 562));
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1000, 1000), QSize(1920, 1080)),
                 QRect(420, 0, 1080, 1080));
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1280, 720), QSize(2560, 1440)),
                 QRect(0, 0, 2560, 1440));
    }

    void handlesUnknownAndInvalidSizesPredictably()
    {
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(), QSize(640, 360)),
                 QRect(0, 0, 640, 360));
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1920, 1080), QSize()), QRect());
    }

    void mapsScaledStreamBoundsIntoNativeClientCoordinates()
    {
        QCOMPARE(StreamVideoItem::scaledCaptureRect(
                     QRectF(80, 0, 1840, 1080), QSizeF(2000, 1080),
                     QRect(100, 50, 2500, 1350)),
                 QRect(200, 50, 2300, 1350));
        QCOMPARE(StreamVideoItem::scaledCaptureRect(
                     QRectF(), QSizeF(1920, 1080), QRect(0, 0, 1920, 1080)),
                 QRect());
    }

    void mapsAbsoluteMouseAgainstTheRenderedViewport()
    {
        QCOMPARE(StreamVideoItem::absoluteMouseCoordinates(
                     QPointF(500, 500), QSize(1920, 1080), QSizeF(1000, 1000)),
                 QRect(500, 281, 1000, 562));
        QCOMPARE(StreamVideoItem::absoluteMouseCoordinates(
                     QPointF(1920, 1080), QSize(1920, 1080), QSizeF(2560, 1440)),
                 QRect(1920, 1080, 2560, 1440));
        QCOMPARE(StreamVideoItem::absoluteMouseCoordinates(
                     QPointF(-50, 2000), QSize(1920, 1080), QSizeF(2560, 1440)),
                 QRect(0, 1439, 2560, 1440));
    }

    void parsesAndMapsRemoteCursorMetadata()
    {
        QByteArray systemCursor;
        systemCursor.append(char(0));
        systemCursor.append(char(12));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0x00));
        systemCursor.append(char(0x80));
        systemCursor.append(char(0xff));
        systemCursor.append(char(0xff));
        const auto system = StreamVideoItem::remoteCursorMetadata(systemCursor);
        QCOMPARE(system.imageOffset, qsizetype(7));
        QCOMPARE(system.imageLength, qsizetype(0));
        QVERIFY(system.normalizedPosition.has_value());
        QCOMPARE(*system.normalizedPosition, QPoint(32768, 65535));
        QCOMPARE(system.scale, 1.0);
        QCOMPARE(StreamVideoItem::mapRemoteCursorPosition(
                     *system.normalizedPosition, QSize(1920, 1080), QSizeF(2560, 1440)),
                 QPoint(1280, 1439));

        QByteArray scaledCursor = systemCursor;
        scaledCursor[0] = char(1);
        scaledCursor.append(char(200));
        scaledCursor.append(char(0));
        const auto scaled = StreamVideoItem::remoteCursorMetadata(scaledCursor);
        QCOMPARE(scaled.scale, 2.0);

        const auto malformed = StreamVideoItem::remoteCursorMetadata(QByteArray::fromHex("01000000000400"));
        QCOMPARE(malformed.imageOffset, qsizetype(-1));
        QVERIFY(!malformed.normalizedPosition.has_value());
    }

    void mapsQtKeyboardStateToTypedGfnInputFields()
    {
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_W), quint16(0x57));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_Escape), quint16(0x1b));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_F24), quint16(0x87));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_unknown), quint16(0));
        QCOMPARE(StreamVideoItem::inputModifiers(
                     Qt::ShiftModifier | Qt::ControlModifier, Qt::Key_W), quint16(0x03));
        QCOMPARE(StreamVideoItem::inputModifiers(Qt::ShiftModifier, Qt::Key_Shift), quint16(0));
    }

    void tabDoesNotStealGameplayFocus_data()
    {
        QTest::addColumn<int>("key");
        QTest::addColumn<bool>("fullscreen");
        for (const bool fullscreen : {false, true}) {
            const auto mode = fullscreen ? "fullscreen" : "windowed";
            QTest::newRow(qPrintable(QStringLiteral("tab-%1").arg(mode)))
                << int(Qt::Key_Tab) << fullscreen;
            QTest::newRow(qPrintable(QStringLiteral("backtab-%1").arg(mode)))
                << int(Qt::Key_Backtab) << fullscreen;
        }
    }

    void tabDoesNotStealGameplayFocus()
    {
        QFETCH(int, key);
        QFETCH(bool, fullscreen);
        static OpenNowStreamerConfig callbacks;
        static QList<QList<quint16>> inputCalls;
        inputCalls.clear();
        NativeStreamRuntime::Api api{};
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbacks = *config;
            *output = reinterpret_cast<OpenNowStreamer *>(new int(1));
            return OPENNOW_STREAMER_OK;
        };
        api.destroy = [](OpenNowStreamer *handle) {
            delete reinterpret_cast<int *>(handle);
            return OPENNOW_STREAMER_OK;
        };
        api.send = [](const OpenNowStreamer *, const std::uint8_t *, std::size_t) {
            return OPENNOW_STREAMER_OK;
        };
        api.setCaptureActive = [](const OpenNowStreamer *, bool, bool, std::uintptr_t, bool *raw) {
            *raw = false;
            return OPENNOW_STREAMER_OK;
        };
        api.submitKey = [](const OpenNowStreamer *, std::uint16_t vk,
                           std::uint16_t modifiers, bool pressed) {
            inputCalls.append(QList<quint16>{vk, modifiers, quint16(pressed)});
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("keyboard-focus")}}));
        const QByteArray ready = R"({"id":"keyboard-focus","type":"ok"})";
        callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(ready.constData()),
                                    ready.size(), callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        auto *other = new QQuickItem(window.contentItem());
        other->setActiveFocusOnTab(true);
        if (fullscreen) window.showFullScreen();
        else window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        const auto modifiers = key == Qt::Key_Backtab ? Qt::ShiftModifier : Qt::NoModifier;
        QKeyEvent press(QEvent::KeyPress, key, modifiers, 23, 0, 0);
        QCoreApplication::sendEvent(&window, &press);
        QCOMPARE(window.activeFocusItem(), item);
        for (int i = 0; i < 3; ++i) {
            QKeyEvent repeat(QEvent::KeyPress, key, modifiers, 23, 0, 0, {}, true);
            QCoreApplication::sendEvent(&window, &repeat);
            QCOMPARE(window.activeFocusItem(), item);
            QVERIFY(item->captureActive());
        }
        QKeyEvent release(QEvent::KeyRelease, key, modifiers, 23, 0, 0);
        QCoreApplication::sendEvent(&window, &release);
        const auto wireModifiers = quint16(key == Qt::Key_Backtab ? 1 : 0);
        QCOMPARE(inputCalls, (QList<QList<quint16>>{
            {0x09, wireModifiers, 1}, {0x09, wireModifiers, 0}}));
        for (const auto movement : {Qt::Key_W, Qt::Key_A, Qt::Key_S, Qt::Key_D}) {
            inputCalls.clear();
            QTest::keyClick(&window, movement);
            QCOMPARE(inputCalls, (QList<QList<quint16>>{
                {quint16(movement), 0, 1}, {quint16(movement), 0, 0}}));
        }
        item->setInputEnabled(false);
        other->forceActiveFocus();
        inputCalls.clear();
        QTest::keyClick(&window, Qt::Key_W);
        QVERIFY(inputCalls.isEmpty());
        item->setInputEnabled(true);
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        QTest::keyClick(&window, Qt::Key_W);
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{0x57, 0, 1}, {0x57, 0, 0}}));
    }

    void forwardsShiftedPunctuation_data()
    {
        QTest::addColumn<int>("key");
        QTest::addColumn<int>("baseKey");
        QTest::addColumn<quint16>("virtualKey");
        const struct {
            const char *name;
            Qt::Key key;
            Qt::Key baseKey;
            quint16 virtualKey;
        } cases[] = {
            {"!", Qt::Key_Exclam, Qt::Key_1, 0x31},
            {"@", Qt::Key_At, Qt::Key_2, 0x32},
            {"#", Qt::Key_NumberSign, Qt::Key_3, 0x33},
            {"$", Qt::Key_Dollar, Qt::Key_4, 0x34},
            {"%", Qt::Key_Percent, Qt::Key_5, 0x35},
            {"^", Qt::Key_AsciiCircum, Qt::Key_6, 0x36},
            {"&", Qt::Key_Ampersand, Qt::Key_7, 0x37},
            {"*", Qt::Key_Asterisk, Qt::Key_8, 0x38},
            {"(", Qt::Key_ParenLeft, Qt::Key_9, 0x39},
            {")", Qt::Key_ParenRight, Qt::Key_0, 0x30},
            {"_", Qt::Key_Underscore, Qt::Key_Minus, 0xbd},
            {"+", Qt::Key_Plus, Qt::Key_Equal, 0xbb},
            {"{", Qt::Key_BraceLeft, Qt::Key_BracketLeft, 0xdb},
            {"}", Qt::Key_BraceRight, Qt::Key_BracketRight, 0xdd},
            {"|", Qt::Key_Bar, Qt::Key_Backslash, 0xdc},
            {":", Qt::Key_Colon, Qt::Key_Semicolon, 0xba},
            {"\"", Qt::Key_QuoteDbl, Qt::Key_Apostrophe, 0xde},
            {"<", Qt::Key_Less, Qt::Key_Comma, 0xbc},
            {">", Qt::Key_Greater, Qt::Key_Period, 0xbe},
            {"?", Qt::Key_Question, Qt::Key_Slash, 0xbf},
            {"~", Qt::Key_AsciiTilde, Qt::Key_QuoteLeft, 0xc0},
        };
        for (const auto &entry : cases)
            QTest::newRow(entry.name) << int(entry.key) << int(entry.baseKey) << entry.virtualKey;
    }

    void forwardsShiftedPunctuation()
    {
        QFETCH(int, key);
        QFETCH(int, baseKey);
        QFETCH(quint16, virtualKey);
        QCOMPARE(StreamVideoItem::windowsVirtualKey(key, Qt::ShiftModifier), virtualKey);
        QCOMPARE(StreamVideoItem::windowsVirtualKey(baseKey), virtualKey);

        static OpenNowStreamerConfig callbacks;
        static QList<QList<quint16>> inputCalls;
        inputCalls.clear();
        NativeStreamRuntime::Api api{};
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbacks = *config;
            *output = reinterpret_cast<OpenNowStreamer *>(new int(1));
            return OPENNOW_STREAMER_OK;
        };
        api.destroy = [](OpenNowStreamer *handle) {
            delete reinterpret_cast<int *>(handle);
            return OPENNOW_STREAMER_OK;
        };
        api.send = [](const OpenNowStreamer *, const std::uint8_t *, std::size_t) {
            return OPENNOW_STREAMER_OK;
        };
        api.setCaptureActive = [](const OpenNowStreamer *, bool, bool, std::uintptr_t, bool *raw) {
            *raw = false;
            return OPENNOW_STREAMER_OK;
        };
        api.submitKey = [](const OpenNowStreamer *, std::uint16_t vk,
                           std::uint16_t modifiers, bool pressed) {
            inputCalls.append(QList<quint16>{vk, modifiers, quint16(pressed)});
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("punctuation")}}));
        const QByteArray ready = R"({"id":"punctuation","type":"ok"})";
        callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(ready.constData()),
                                    ready.size(), callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        auto *overlay = new QQuickItem(window.contentItem());
        overlay->setVisible(false);
        for (const bool fullscreen : {false, true}) {
            if (fullscreen) window.showFullScreen();
            else window.showNormal();
            window.requestActivate();
            QTRY_VERIFY(window.isActive());
            item->forceActiveFocus();
            QTRY_VERIFY(item->captureActive());
            for (const quint32 scanCode : {0u, 39u}) {
                for (const bool shiftReleasedFirst : {false, true}) {
                    inputCalls.clear();
                    QKeyEvent press(QEvent::KeyPress, key, Qt::ShiftModifier, scanCode, 0, 0);
                    QCoreApplication::sendEvent(&window, &press);
                    QVERIFY(press.isAccepted());
                    QCOMPARE(inputCalls, (QList<QList<quint16>>{{virtualKey, 1, 1}}));
                    QKeyEvent repeat(QEvent::KeyPress, key, Qt::ShiftModifier,
                                     scanCode, 0, 0, {}, true);
                    QCoreApplication::sendEvent(&window, &repeat);
                    QCOMPARE(inputCalls.size(), 1);
                    const auto modifiers = shiftReleasedFirst ? Qt::NoModifier : Qt::ShiftModifier;
                    QKeyEvent release(QEvent::KeyRelease, shiftReleasedFirst ? baseKey : key,
                                      modifiers, scanCode, 0, 0);
                    QCoreApplication::sendEvent(&window, &release);
                    QVERIFY(release.isAccepted());
                    QCOMPARE(inputCalls, (QList<QList<quint16>>{
                        {virtualKey, 1, 1}, {virtualKey, quint16(shiftReleasedFirst ? 0 : 1), 0}}));
                    QVERIFY(item->m_pressedKeys.isEmpty());
                }
            }
            inputCalls.clear();
            QKeyEvent held(QEvent::KeyPress, key, Qt::ShiftModifier);
            QCoreApplication::sendEvent(&window, &held);
            overlay->setVisible(true);
            overlay->forceActiveFocus();
            QTRY_VERIFY(!item->captureActive());
            QCOMPARE(inputCalls, (QList<QList<quint16>>{{virtualKey, 1, 1}, {virtualKey, 0, 0}}));
            QVERIFY(item->m_pressedKeys.isEmpty());
            QKeyEvent blocked(QEvent::KeyPress, key, Qt::ShiftModifier);
            QCoreApplication::sendEvent(&window, &blocked);
            QCOMPARE(inputCalls.size(), 2);
            overlay->setVisible(false);
            item->forceActiveFocus();
            QTRY_VERIFY(item->captureActive());
            QKeyEvent resumed(QEvent::KeyPress, key, Qt::ShiftModifier);
            QCoreApplication::sendEvent(&window, &resumed);
            QCOMPARE(inputCalls.last(), (QList<quint16>{virtualKey, 1, 1}));
            item->releaseInput();
            QCOMPARE(inputCalls.size(), 4);
        }
    }

    void keepsKeypadOperatorsSeparateFromShiftedNumberRow()
    {
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_Plus, Qt::KeypadModifier), quint16(0x6b));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_Asterisk, Qt::KeypadModifier), quint16(0x6a));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(
                     Qt::Key_Plus, Qt::KeypadModifier | Qt::ShiftModifier), quint16(0x6b));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(
                     Qt::Key_Asterisk, Qt::KeypadModifier | Qt::ShiftModifier), quint16(0x6a));
    }

    void inputEnablementIsExplicitAndObservable()
    {
        StreamVideoItem item;
        QSignalSpy changes(&item, &StreamVideoItem::inputEnabledChanged);
        QVERIFY(item.inputEnabled());
        item.setInputEnabled(false);
        QVERIFY(!item.inputEnabled());
        QCOMPARE(changes.size(), 1);
        item.setInputEnabled(false);
        QCOMPARE(changes.size(), 1);
    }

    void matchesShellShortcutsWithExactModifiersAndAliases()
    {
        const QVariantMap bindings{
            {QStringLiteral("guide"), QVariantList{QStringLiteral("Ctrl+G")}},
            {QStringLiteral("request-exit"), QVariantList{QStringLiteral("Escape")}},
            {QStringLiteral("toggle-fullscreen"), QVariantList{QStringLiteral("F11")}},
            {QStringLiteral("toggle-stats"),
             QVariantList{QStringLiteral("F3"), QStringLiteral("Ctrl+N")}},
        };
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_F3, Qt::NoModifier), QStringLiteral("toggle-stats"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_N, Qt::ControlModifier), QStringLiteral("toggle-stats"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_F11, Qt::NoModifier), QStringLiteral("toggle-fullscreen"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_G, Qt::ControlModifier), QStringLiteral("guide"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_Escape, Qt::NoModifier), QStringLiteral("request-exit"));
        QVERIFY(StreamVideoItem::shortcutActionForInput(
                    bindings, Qt::Key_F3, Qt::ShiftModifier).isEmpty());
        QVERIFY(StreamVideoItem::shortcutActionForInput(
                    bindings, Qt::Key_G, Qt::NoModifier).isEmpty());
    }

    void microphoneShortcutRequiresExactModifiersAndAnEnabledBinding()
    {
        const QVariantMap bindings{
            {QStringLiteral("toggle-microphone"), QVariantList{QStringLiteral("Ctrl+Shift+M")}},
        };
        QCOMPARE(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_M,
                     Qt::ControlModifier | Qt::ShiftModifier), QStringLiteral("toggle-microphone"));
        QVERIFY(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_M,
                    Qt::ControlModifier).isEmpty());
        QVERIFY(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_M,
                    Qt::ControlModifier | Qt::ShiftModifier | Qt::AltModifier).isEmpty());
        QVERIFY(StreamVideoItem::shortcutActionForInput({}, Qt::Key_M,
                    Qt::ControlModifier | Qt::ShiftModifier).isEmpty());
    }

    void shortcutBindingsAreExplicitAndObservable()
    {
        StreamVideoItem item;
        QSignalSpy changes(&item, &StreamVideoItem::shortcutBindingsChanged);
        const QVariantMap bindings{
            {QStringLiteral("toggle-pointer-lock"), QVariantList{QStringLiteral("F8")}},
        };
        item.setShortcutBindings(bindings);
        QCOMPARE(item.shortcutBindings(), bindings);
        QCOMPARE(changes.size(), 1);
        item.setShortcutBindings(bindings);
        QCOMPARE(changes.size(), 1);
    }

    void recordingAndClipShortcutsRemainDistinct()
    {
        const QVariantMap bindings{
            {QStringLiteral("toggle-recording"), QVariantList{QStringLiteral("F12")}},
            {QStringLiteral("save-clip"), QVariantList{QStringLiteral("Ctrl+F12")}},
        };
        QCOMPARE(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_F12,
                     Qt::NoModifier), QStringLiteral("toggle-recording"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_F12,
                     Qt::ControlModifier), QStringLiteral("save-clip"));
        QVERIFY(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_F12,
                    Qt::ControlModifier | Qt::ShiftModifier).isEmpty());
        const QVariantMap custom{
            {QStringLiteral("save-clip"), QVariantList{QStringLiteral("Alt+F9")}},
        };
        QCOMPARE(StreamVideoItem::shortcutActionForInput(custom, Qt::Key_F9,
                     Qt::AltModifier), QStringLiteral("save-clip"));
        QVERIFY(StreamVideoItem::shortcutActionForInput(custom, Qt::Key_F12,
                    Qt::ControlModifier).isEmpty());
    }

    void normalizesVideoSizeAndTracksCallbackAvailability()
    {
        StreamVideoItem item;
        QSignalSpy sizeChanges(&item, &StreamVideoItem::videoSizeChanged);
        QSignalSpy callbackChanges(&item, &StreamVideoItem::renderCallbackAvailableChanged);

        item.setVideoSize(QSize(-1, 1080));
        QCOMPARE(item.videoSize(), QSize());
        QCOMPARE(sizeChanges.size(), 0);

        item.setVideoSize(QSize(1920, 1080));
        QCOMPARE(item.videoSize(), QSize(1920, 1080));
        QCOMPARE(sizeChanges.size(), 1);

        const auto callback = std::make_shared<TestRenderCallback>();
        item.setRenderCallback(callback);
        QVERIFY(item.renderCallbackAvailable());
        QVERIFY(item.renderCallback() == callback);
        QCOMPARE(callbackChanges.size(), 1);

        item.setRenderCallback(callback);
        QCOMPARE(callbackChanges.size(), 1);
        item.setRenderCallback({});
        QVERIFY(!item.renderCallbackAvailable());
        QCOMPARE(callbackChanges.size(), 2);
    }

    void registersConcreteQmlSceneGraphType()
    {
        QVERIFY(qmlTypeId("OpenNOW", 1, 0, "StreamVideoItem") >= 0);
        StreamVideoItem item;
        QVERIFY(qobject_cast<QQuickItem *>(&item));
        QVERIFY(item.flags().testFlag(QQuickItem::ItemHasContents));
        // Direct scene-graph rendering must not reintroduce an offscreen color target.
        QCOMPARE(item.metaObject()->indexOfProperty("colorBufferFormat"), -1);
    }

    void metalFxUpscalingIsMacOnlyAndDoesNotReplaceThePresenter()
    {
        StreamVideoItem item;
        const auto callback = std::make_shared<TestRenderCallback>();
        item.setRenderCallback(callback);
        item.setVideoSize(QSize(1920, 1080));
        QSignalSpy changes(&item, &StreamVideoItem::metalFxUpscalingChanged);
        QVERIFY(!item.metalFxUpscaling());
        item.setMetalFxUpscaling(true);
#if defined(Q_OS_MACOS)
        QVERIFY(item.metalFxUpscaling());
        QCOMPARE(changes.size(), 1);
        item.setMetalFxUpscaling(true);
        QCOMPARE(changes.size(), 1);
        item.setMetalFxUpscaling(false);
        QVERIFY(!item.metalFxUpscaling());
        QCOMPARE(changes.size(), 2);
#else
        QVERIFY(!item.metalFxUpscaling());
        QCOMPARE(changes.size(), 0);
#endif
        QCOMPARE(item.renderCallback(), callback);
        QCOMPARE(item.videoSize(), QSize(1920, 1080));
        QVERIFY(!item.frameGeneration());
    }

    void upscalingEnhancementIsBoundedAndPreservesPresenter()
    {
        StreamVideoItem item;
        const auto callback = std::make_shared<TestRenderCallback>();
        item.setRenderCallback(callback);
        QSignalSpy sharpnessChanges(&item, &StreamVideoItem::upscalingSharpnessChanged);
        QSignalSpy denoiseChanges(&item, &StreamVideoItem::upscalingDenoiseChanged);
        QCOMPARE(item.upscalingSharpness(), 10);
        QCOMPARE(item.upscalingDenoise(), 0);
        item.setUpscalingSharpness(10);
        item.setUpscalingDenoise(0);
        QCOMPARE(sharpnessChanges.size(), 0);
        QCOMPARE(denoiseChanges.size(), 0);
        item.setUpscalingSharpness(100);
        item.setUpscalingDenoise(100);
        QCOMPARE(item.upscalingSharpness(), 15);
        QCOMPARE(item.upscalingDenoise(), 20);
        item.setUpscalingSharpness(-1);
        item.setUpscalingDenoise(-1);
        QCOMPARE(item.upscalingSharpness(), 0);
        QCOMPARE(item.upscalingDenoise(), 0);
        QCOMPARE(sharpnessChanges.size(), 2);
        QCOMPARE(denoiseChanges.size(), 2);
        QVERIFY(!item.metalFxUpscaling());
        QCOMPARE(item.renderCallback(), callback);
    }

    void frameGenerationIsOptInAndDoesNotReplaceThePresenter()
    {
        StreamVideoItem item;
        const auto callback = std::make_shared<TestRenderCallback>();
        item.setRenderCallback(callback);
        QSignalSpy changes(&item, &StreamVideoItem::frameGenerationChanged);
        QSignalSpy stats(&item, &StreamVideoItem::frameGenerationStatsChanged);
        QVERIFY(!item.frameGeneration());
        QVERIFY(!item.m_frameStatsTimer.isActive());
        item.setFrameGeneration(true);
        QVERIFY(item.frameGeneration());
        QVERIFY(item.m_frameStatsTimer.isActive());
        QCOMPARE(changes.size(), 1);
        QCOMPARE(stats.size(), 1);
        item.setFrameGeneration(true);
        QCOMPARE(changes.size(), 1);
        QCOMPARE(item.renderCallback(), callback);
        item.setFrameGeneration(false);
        QVERIFY(!item.m_frameStatsTimer.isActive());
        QCOMPARE(changes.size(), 2);
        QCOMPARE(item.renderCallback(), callback);
    }

    void generatedTextureBindingsPreserveFullscreenAndOverlays()
    {
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("The offscreen platform plugin does not create a QRhi.");
        const auto callback = std::make_shared<TextureRenderCallback>(true);
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback(callback);
        auto *overlay = new WhiteOverlay(window.contentItem());
        overlay->setSize(QSizeF(40, 40));
        overlay->setZ(10);
        for (const bool fullscreen : {false, true}) {
            if (fullscreen) window.showFullScreen();
            else window.showNormal();
            QTRY_VERIFY(window.isExposed());
            item->setSize(window.size());
            item->setVideoSize(window.size());
            for (const bool overlayVisible : {false, true}) {
                overlay->setVisible(overlayVisible);
                item->requestFrame();
                QTRY_VERIFY(callback->imported.load());
                const auto image = window.grabWindow();
                QVERIFY(!image.isNull());
                QCOMPARE(image.pixelColor(image.width() / 2, image.height() / 4), QColor(Qt::red));
                QCOMPARE(image.pixelColor(image.width() / 2, image.height() * 3 / 4), QColor(Qt::green));
                QCOMPARE(image.pixelColor(10, 10), QColor(overlayVisible ? Qt::white : Qt::red));
            }
        }
    }

    void createsRenderCallbackFromTheSharedNativeRuntime()
    {
        NativeStreamRuntime runtime;
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        {
            StreamVideoItem item;
            QCOMPARE(StreamVideoItem::nativeStreamRuntime(), &runtime);
            QVERIFY(item.renderCallbackAvailable());
        }
        StreamVideoItem::setNativeStreamRuntime(nullptr);
    }

    void upscalingTargetTracksViewportAcrossOverlaysAndWindowChanges()
    {
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("The offscreen platform plugin does not create a QRhi.");
        const auto callback = std::make_shared<TestRenderCallback>();
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setVideoSize(QSize(320, 180));
        item->m_metalFxUpscaling = true;
        item->setRenderCallback(callback);
        QQuickItem overlay(window.contentItem());
        overlay.setZ(10);
        for (const bool fullscreen : {false, true, false}) {
            if (fullscreen) window.showFullScreen();
            else {
                window.showNormal();
                window.resize(800, 600);
            }
            QTest::qWait(100);
            item->setSize(window.size());
            overlay.setSize(window.size());
            for (const bool visible : {false, true}) {
                overlay.setVisible(visible);
                const int frames = callback->frameCount.load();
                item->requestFrame();
                QTRY_VERIFY_WITH_TIMEOUT(callback->frameCount.load() > frames, 5'000);
                const auto viewport = StreamVideoItem::aspectFitRect(item->videoSize(), window.size());
                const auto target = (QSizeF(viewport.size()) * window.effectiveDevicePixelRatio()).toSize();
                QCOMPARE(callback->upscaleWidth.load(), target.width());
                QCOMPARE(callback->upscaleHeight.load(), target.height());
                item->setUpscalingSharpness(visible ? 15 : 0);
                item->setUpscalingDenoise(visible ? 20 : 0);
                item->requestFrame();
                QTRY_COMPARE(callback->upscaleSharpness.load(), visible ? 15 : 0);
                QTRY_COMPARE(callback->upscaleDenoise.load(), visible ? 20 : 0);
                QCOMPARE(item->renderCallback(), callback);
                QCOMPARE(item->videoSize(), QSize(320, 180));
            }
        }
        item->setMetalFxUpscaling(false);
        item->requestFrame();
        QTRY_COMPARE(callback->upscaleWidth.load(), -1);
        QTRY_COMPARE(callback->upscaleHeight.load(), -1);
    }

    void drivesCallbackThroughRhiSceneGraph()
    {
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("The offscreen platform plugin does not create a QRhi.");

        const auto callback = std::make_shared<TestRenderCallback>();
        {
            QQuickWindow window;
            window.resize(640, 480);
            auto *item = new StreamVideoItem(window.contentItem());
            item->setSize(QSizeF(640, 480));
            item->setVideoSize(QSize(1920, 1080));
            item->setRenderCallback(callback);
            window.show();
            item->requestFrame();

            QTRY_VERIFY_WITH_TIMEOUT(callback->initializeCount.load() > 0, 5'000);
            QTRY_VERIFY_WITH_TIMEOUT(callback->frameCount.load() > 0, 5'000);
            QVERIFY(callback->prepareCount.load() > 0);
            QTRY_VERIFY_WITH_TIMEOUT(callback->finishCount.load() > 0, 5'000);
            QVERIFY(callback->validContext.load());
            QCOMPARE(callback->viewportWidth.load(), 640);
            QCOMPARE(callback->viewportHeight.load(), 360);
        }
        QTRY_VERIFY_WITH_TIMEOUT(callback->releaseCount.load() > 0, 5'000);
    }

    void rawPointerLockPinsTheCursorWithoutRestrictingAbsoluteInput()
    {
        for (const QRect viewport : {QRect(100, 80, 960, 540), QRect(0, 0, 1920, 1080),
                                    QRect(-2560, 120, 2560, 1440)}) {
            QCOMPARE(StreamVideoItem::cursorConfinementRect(viewport, true),
                     QRect(viewport.center(), QSize(1, 1)));
            QCOMPARE(StreamVideoItem::cursorConfinementRect(viewport, false), viewport);
        }
        QVERIFY(StreamVideoItem::cursorConfinementRect({}, true).isEmpty());
    }

    void nativePointerLockStaysFixedAndReleasesForOverlays()
    {
#if defined(Q_OS_WIN)
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("Requires native Windows cursor confinement.");
        const auto originalPosition = QCursor::pos();
        const auto restore = qScopeGuard([&] { ClipCursor(nullptr); QCursor::setPos(originalPosition); });
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        for (const bool fullscreen : {false, true}) {
            if (fullscreen) window.showFullScreen();
            else window.showNormal();
            window.requestActivate();
            QTRY_VERIFY(window.isActive());
            item->setSize(window.size());
            item->setInputEnabled(true);
            // Emulate an established native Raw Input capture without a live game.
            item->m_relativeMouse = true;
            item->m_rawInputActive = true;
            item->m_captureActive = true;
            item->updateCursorConfinement();
            RECT clipped{};
            QVERIFY(GetClipCursor(&clipped));
            QCOMPARE(clipped.right - clipped.left, LONG(1));
            QCOMPARE(clipped.bottom - clipped.top, LONG(1));
            QCursor::setPos(clipped.left + 200, clipped.top + 100);
            QCOMPARE(QCursor::pos(), QPoint(clipped.left, clipped.top));
            // Blocking overlays/focus loss disable input; confinement must go too.
            item->setInputEnabled(false);
            QVERIFY(!item->captureActive());
            QVERIFY(GetClipCursor(&clipped));
            QVERIFY(clipped.right - clipped.left > 1);
            QVERIFY(clipped.bottom - clipped.top > 1);
        }
#else
        QSKIP("Windows cursor confinement test.");
#endif
    }

    void waylandAbsoluteToPendingRelativeLockSurvivesUntilCompositorAcknowledges()
    {
        if (!WaylandPointerCapture::isWayland()
                || !qEnvironmentVariableIsSet("OPENNOW_TEST_WAYLAND_CAPTURE"))
            QSKIP("Requires an interactive Wayland compositor");
        NativeStreamRuntime::Api api{};
        static OpenNowStreamerConfig callbacks;
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbacks = *config;
            *output = reinterpret_cast<OpenNowStreamer *>(new int(1));
            return OPENNOW_STREAMER_OK;
        };
        api.destroy = [](OpenNowStreamer *handle) {
            delete reinterpret_cast<int *>(handle);
            return OPENNOW_STREAMER_OK;
        };
        api.send = [](const OpenNowStreamer *, const std::uint8_t *, std::size_t) {
            return OPENNOW_STREAMER_OK;
        };
        api.setCaptureActive = [](const OpenNowStreamer *, bool, bool,
                                  std::uintptr_t window, bool *raw) {
            Q_ASSERT(window == 0);
            *raw = false;
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        item->setVideoSize(QSize(1920, 1080));
        connect(&window, &QWindow::widthChanged, item, [&window, item] { item->setSize(window.size()); });
        connect(&window, &QWindow::heightChanged, item, [&window, item] { item->setSize(window.size()); });
        window.show();
        QTRY_VERIFY_WITH_TIMEOUT(window.isActive(), 10000);
        item->forceActiveFocus();
        QVERIFY(!item->captureActive());
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("wayland-start")}}));
        const QByteArray ready = R"({"id":"wayland-start","type":"ok"})";
        callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(ready.constData()),
                                    ready.size(), callbacks.user_data);
        QTRY_VERIFY(item->captureActive());
        item->setRelativeMouse(true);
        QVERIFY(!item->captureActive());
        QTRY_VERIFY_WITH_TIMEOUT(item->m_waylandPointer->locked(), 10000);
        QTRY_VERIFY(item->captureActive());
        for (const bool fullscreen : {false, true}) {
            if (fullscreen) window.showFullScreen();
            else window.showNormal();
            item->setSize(window.size());
            item->setInputEnabled(false);
            QVERIFY(!item->m_waylandPointer->locked());
            QVERIFY(!item->captureActive());
            item->setInputEnabled(true);
            QTRY_VERIFY_WITH_TIMEOUT(item->m_waylandPointer->locked(), 10000);
            QTRY_VERIFY(item->captureActive());
        }
        for (const bool failure : {false, true}) {
            if (failure) {
                const QByteArray error = R"({"type":"status","status":"error"})";
                callbacks.event_callback(reinterpret_cast<const std::uint8_t *>(error.constData()),
                                         error.size(), callbacks.user_data);
            } else {
                QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("stop")}}));
            }
            QTRY_VERIFY(!runtime.inputAllowed());
            QVERIFY(item->isVisible());
            QVERIFY(!item->m_waylandPointer->locked());
            QVERIFY(!item->captureActive());
            QMetaObject::invokeMethod(item->m_waylandPointer.get(), "stateChanged", Qt::QueuedConnection);
            QTest::qWait(150);
            QVERIFY(!item->m_waylandPointer->locked());
            QVERIFY(!item->captureActive());
            QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                                  {QStringLiteral("id"), QStringLiteral("wayland-start")}}));
            callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(ready.constData()),
                                        ready.size(), callbacks.user_data);
            QTRY_VERIFY_WITH_TIMEOUT(item->m_waylandPointer->locked(), 10000);
            QTRY_VERIFY(item->captureActive());
        }
        window.hide();
        QTRY_VERIFY(!item->m_waylandPointer->locked());
        QTRY_VERIFY(!item->captureActive());
    }

    void sessionAuthorizationPreventsVisibleCaptureFromReopeningAfterReset()
    {
        static OpenNowStreamerConfig callbacks;
        static QStringList inputCalls;
        static OpenNowStreamerStatus commandStatus;
        inputCalls.clear();
        commandStatus = OPENNOW_STREAMER_OK;
        NativeStreamRuntime::Api api{};
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbacks = *config;
            *output = reinterpret_cast<OpenNowStreamer *>(new int(1));
            return OPENNOW_STREAMER_OK;
        };
        api.destroy = [](OpenNowStreamer *handle) {
            delete reinterpret_cast<int *>(handle);
            return OPENNOW_STREAMER_OK;
        };
        api.send = [](const OpenNowStreamer *, const std::uint8_t *, std::size_t) {
            return commandStatus;
        };
        api.setCaptureActive = [](const OpenNowStreamer *, bool active, bool, std::uintptr_t, bool *raw) {
            inputCalls.append(active ? QStringLiteral("open") : QStringLiteral("close"));
            *raw = false;
            return OPENNOW_STREAMER_OK;
        };
        api.submitKey = [](const OpenNowStreamer *, std::uint16_t, std::uint16_t, bool pressed) {
            inputCalls.append(pressed ? QStringLiteral("key-down") : QStringLiteral("key-up"));
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QVERIFY(!runtime.presentationAllowed());
        QVERIFY(!item->captureActive());
        QVERIFY(!inputCalls.contains(QStringLiteral("open")));
        const auto reply = [](const QString &id) {
            const auto bytes = QJsonDocument(QJsonObject{{QStringLiteral("id"), id},
                {QStringLiteral("type"), QStringLiteral("ok")}}).toJson(QJsonDocument::Compact);
            callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(bytes.constData()),
                                        bytes.size(), callbacks.user_data);
        };
        const auto delayedCaptureCallback = [item] {
            QMetaObject::invokeMethod(item->m_waylandPointer.get(), "stateChanged", Qt::QueuedConnection);
            QCoreApplication::sendPostedEvents();
            QCoreApplication::processEvents();
            item->resynchronizeInput();
        };
        for (const auto reason : {QStringLiteral("stop"), QStringLiteral("presentation-error"),
                                  QStringLiteral("terminal-error"), QStringLiteral("rejected-stop")}) {
            QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                                  {QStringLiteral("id"), reason}}));
            QVERIFY(!item->captureActive());
            delayedCaptureCallback();
            QVERIFY(!item->captureActive());
            reply(reason);
            QTRY_VERIFY(runtime.inputAllowed());
            QTRY_VERIFY(item->captureActive());
            QKeyEvent press(QEvent::KeyPress, Qt::Key_W, Qt::NoModifier);
            item->keyPressEvent(&press);
            QVERIFY(inputCalls.contains(QStringLiteral("key-down")));
            inputCalls.clear();
            if (reason == QStringLiteral("presentation-error")) {
                runtime.reportPresentationError(QStringLiteral("fixture presentation failure"));
            } else if (reason == QStringLiteral("terminal-error")) {
                const QByteArray bytes = R"({"type":"status","status":"error"})";
                callbacks.event_callback(reinterpret_cast<const std::uint8_t *>(bytes.constData()),
                                         bytes.size(), callbacks.user_data);
            } else {
                commandStatus = reason == QStringLiteral("rejected-stop")
                    ? OPENNOW_STREAMER_QUEUE_FULL : OPENNOW_STREAMER_OK;
                QCOMPARE(runtime.send({{QStringLiteral("type"), QStringLiteral("stop")}}),
                         commandStatus == OPENNOW_STREAMER_OK);
                commandStatus = OPENNOW_STREAMER_OK;
            }
            QTRY_COMPARE(runtime.inputAllowed(), reason == QStringLiteral("rejected-stop"));
            QVERIFY(window.isVisible());
            QVERIFY(item->isVisible());
            QCOMPARE(item->captureActive(), reason == QStringLiteral("rejected-stop"));
            QVERIFY(inputCalls.indexOf(QStringLiteral("key-up")) >= 0);
            QVERIFY(inputCalls.indexOf(QStringLiteral("key-up")) < inputCalls.indexOf(QStringLiteral("close")));
            inputCalls.clear();
            reply(reason);
            delayedCaptureCallback();
            QCOMPARE(runtime.inputAllowed(), reason == QStringLiteral("rejected-stop"));
            QCOMPARE(item->captureActive(), reason == QStringLiteral("rejected-stop"));
            QCOMPARE(inputCalls.contains(QStringLiteral("open")), reason == QStringLiteral("rejected-stop"));
        }
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("pending-start")}}));
        commandStatus = OPENNOW_STREAMER_QUEUE_FULL;
        QVERIFY(!runtime.send({{QStringLiteral("type"), QStringLiteral("stop")}}));
        commandStatus = OPENNOW_STREAMER_OK;
        delayedCaptureCallback();
        QVERIFY(!runtime.inputAllowed());
        QVERIFY(!item->captureActive());
        reply(QStringLiteral("pending-start"));
        QTRY_VERIFY(runtime.presentationAllowed());
        delayedCaptureCallback();
        QVERIFY(runtime.inputAllowed());
        QVERIFY(item->captureActive());
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("fresh-start")}}));
        reply(QStringLiteral("fresh-start"));
        QTRY_VERIFY(item->captureActive());
    }

    void cursorModeChangesDoNotReleaseAnActiveDrag()
    {
        StreamVideoItem item;
        QVERIFY(item.keepMouseGrab());
        item.m_pressedMouseButtons.insert(1);
        item.setRelativeMouse(true);
        QVERIFY(!item.relativeMouse());
        QVERIFY(item.m_pressedMouseButtons.contains(1));
        QCOMPARE(item.m_pendingRelativeMouse, std::optional<bool>(true));
        item.setRelativeMouse(false);
        QCOMPARE(item.m_pendingRelativeMouse, std::optional<bool>(false));
        item.releaseInput();
        QVERIFY(item.m_pressedMouseButtons.isEmpty());
        QVERIFY(!item.m_pendingRelativeMouse.has_value());
        item.setRelativeMouse(true);
        QVERIFY(item.relativeMouse());
        item.m_pressedMouseButtons.insert(1);
        item.setRelativeMouse(false);
        QVERIFY(item.relativeMouse());
        QVERIFY(item.m_pressedMouseButtons.contains(1));
        item.m_pressedMouseButtons.clear();
        item.setRelativeMouse(false);
        QVERIFY(!item.relativeMouse());
        item.m_pressedMouseButtons.insert(1);
        item.setRelativeMouse(true);
        item.releaseInput();
        QVERIFY(item.relativeMouse());
        QVERIFY(item.m_pressedMouseButtons.isEmpty());
        QVERIFY(!item.m_pendingRelativeMouse.has_value());
    }

    void visibleCursorUpdatesStayHiddenUntilRelativeButtonRelease()
    {
        CursorSession session;
        QVERIFY(session.start());
        StreamVideoItem item(session.window.contentItem());
        item.m_usesMacPointerCapture = false;
        item.setRenderCallback({});
        item.setSize(session.window.size());
        item.forceActiveFocus();
        QTRY_VERIFY(item.captureActive());
        session.composition(false);
        QTRY_VERIFY(!item.m_serverCursorComposited);
        item.applyRemoteCursor(QByteArray::fromHex("0000"));
        QVERIFY(item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        item.m_pressedMouseButtons.insert(1);
        item.m_pressedMouseButtons.insert(3);
        for (const auto &cursor : {"000c", "0002", "0000", "000c"}) {
            item.applyRemoteCursor(QByteArray::fromHex(cursor));
            QVERIFY(item.relativeMouse());
            QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
            QCOMPARE(item.m_pressedMouseButtons.size(), 2);
        }
        item.m_captureActive = true;
        item.m_rawInputActive = true;
        QMouseEvent leftRelease(QEvent::MouseButtonRelease, QPointF(), QPointF(),
                                Qt::LeftButton, Qt::RightButton, Qt::NoModifier);
        item.mouseReleaseEvent(&leftRelease);
        QVERIFY(item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        QMouseEvent rightRelease(QEvent::MouseButtonRelease, QPointF(), QPointF(),
                                 Qt::RightButton, Qt::NoButton, Qt::NoModifier);
        item.mouseReleaseEvent(&rightRelease);
        QVERIFY(!item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::PointingHandCursor);
        QVERIFY(item.m_pressedMouseButtons.isEmpty());
        QVERIFY(!item.m_pendingRelativeMouse.has_value());
    }

    void deferredCursorUpdatesSurviveInputRelease()
    {
        CursorSession session;
        QVERIFY(session.start());
        StreamVideoItem item(session.window.contentItem());
        item.m_usesMacPointerCapture = false;
        item.setRenderCallback({});
        item.setSize(session.window.size());
        item.forceActiveFocus();
        QTRY_VERIFY(item.captureActive());
        session.composition(false);
        QTRY_VERIFY(!item.m_serverCursorComposited);
        item.applyRemoteCursor(QByteArray::fromHex("0002"));
        QCOMPARE(item.cursor().shape(), Qt::IBeamCursor);
        item.m_pressedMouseButtons.insert(1);
        item.applyRemoteCursor(QByteArray::fromHex("0000"));
        QVERIFY(!item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::IBeamCursor);
        item.releaseInput();
        QVERIFY(item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
        item.syncCaptureState();
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        item.m_pressedMouseButtons.insert(1);
        item.applyRemoteCursor(QByteArray::fromHex("000c"));
        item.applyRemoteCursor(QByteArray::fromHex("0002"));
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        item.releaseInput();
        QVERIFY(!item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
        item.syncCaptureState();
        QCOMPARE(item.cursor().shape(), Qt::IBeamCursor);
        QVERIFY(item.m_pressedMouseButtons.isEmpty());
        QVERIFY(!item.m_pendingRelativeMouse.has_value());
    }

    void deferredBitmapCursorRetainsItsShapeAndHotspot()
    {
        QPixmap image(8, 8);
        image.fill(Qt::red);
        QByteArray png;
        QBuffer buffer(&png);
        QVERIFY(buffer.open(QIODevice::WriteOnly));
        QVERIFY(image.save(&buffer, "PNG"));
        const auto encoded = png.toBase64();
        auto message = QByteArray::fromHex("0100020300");
        message.append(static_cast<char>(encoded.size() & 0xff));
        message.append(static_cast<char>((encoded.size() >> 8) & 0xff));
        message.append(encoded);

        CursorSession session;
        QVERIFY(session.start());
        StreamVideoItem item(session.window.contentItem());
        item.m_usesMacPointerCapture = false;
        item.setRenderCallback({});
        item.setSize(session.window.size());
        item.forceActiveFocus();
        QTRY_VERIFY(item.captureActive());
        session.composition(false);
        QTRY_VERIFY(!item.m_serverCursorComposited);
        item.applyRemoteCursor(QByteArray::fromHex("0000"));
        item.m_pressedMouseButtons.insert(1);
        item.applyRemoteCursor(message);
        QVERIFY(item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::BlankCursor);
        item.releaseInput();
        QVERIFY(!item.relativeMouse());
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
        item.syncCaptureState();
        QCOMPARE(item.cursor().shape(), Qt::BitmapCursor);
        QCOMPARE(item.cursor().hotSpot(), QPoint(2, 3));
        QCOMPARE(item.cursor().pixmap().toImage(), image.toImage());
        item.setVisible(false);
        item.setVisible(true);
        item.setRelativeMouse(true);
        item.setRelativeMouse(false);
        QCOMPARE(item.cursor().shape(), Qt::ArrowCursor);
    }

    void directVideoPreservesPixelsClippingOpacityAndOverlays()
    {
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("Requires a native QRhi window.");
        QQuickWindow window;
        window.setColor(Qt::blue);
        window.resize(320, 260);
        auto *clip = new QQuickItem(window.contentItem());
        clip->setPosition(QPointF(20, 20));
        clip->setSize(QSizeF(200, 200));
        clip->setClip(true);
        auto *video = new StreamVideoItem(clip);
        video->setInputEnabled(false);
        video->setSize(QSizeF(200, 200));
        video->setVideoSize(QSize(200, 100));
        const auto callback = std::make_shared<TextureRenderCallback>();
        video->setRenderCallback(callback);
        auto *overlay = new WhiteOverlay(window.contentItem());
        overlay->setPosition(QPointF(80, 80));
        overlay->setSize(QSizeF(20, 20));
        overlay->setZ(10);
        window.show();
        QTRY_VERIFY_WITH_TIMEOUT(callback->imported.load(), 5'000);
        QVERIFY(callback->directTarget.load());
        const auto pixel = [&window](const QImage &image, int x, int y) {
            return image.pixelColor(x * image.width() / window.width(),
                                    y * image.height() / window.height());
        };
        auto image = window.grabWindow();
        QVERIFY(!image.isNull());
        QCOMPARE(pixel(image, 40, 40), QColor(Qt::black)); // letterbox
        QCOMPARE(pixel(image, 40, 80), QColor(Qt::red));
        QCOMPARE(pixel(image, 40, 160), QColor(Qt::green));
        QCOMPARE(pixel(image, 85, 85), QColor(Qt::white)); // overlay remains above video

        callback->showVideo.store(false);
        video->requestFrame();
        QTRY_COMPARE_WITH_TIMEOUT(pixel(window.grabWindow(), 40, 80), QColor(Qt::blue), 5'000);
        QCOMPARE(pixel(window.grabWindow(), 85, 85), QColor(Qt::white));
        callback->showVideo.store(true);
        video->requestFrame();
        QTRY_COMPARE_WITH_TIMEOUT(pixel(window.grabWindow(), 40, 80), QColor(Qt::red), 5'000);

        clip->setWidth(100);
        video->setOpacity(0.5);
        image = window.grabWindow();
        QCOMPARE(pixel(image, 160, 80), QColor(Qt::blue));
        const auto blended = pixel(image, 40, 80);
        QVERIFY(qAbs(blended.red() - 128) <= 2);
        QVERIFY(qAbs(blended.blue() - 127) <= 2);
        QCOMPARE(pixel(image, 85, 85), QColor(Qt::white));

        // Rotated rectangular clips use stencil rather than a simple scissor.
        video->setOpacity(1.0);
        clip->setRotation(15);
        image = window.grabWindow();
        const auto inside = clip->mapToScene(QPointF(40, 70));
        const auto outside = clip->mapToScene(QPointF(150, 70));
        QCOMPARE(pixel(image, int(inside.x()), int(inside.y())), QColor(Qt::red));
        QCOMPARE(pixel(image, int(outside.x()), int(outside.y())), QColor(Qt::blue));

        clip->setRotation(0);
        clip->setWidth(200);
        window.showFullScreen();
        QTRY_VERIFY(window.visibility() == QWindow::FullScreen);
        // Visibility changes synchronously; native resize/swapchain recreation does not.
        QTRY_COMPARE_WITH_TIMEOUT(pixel(window.grabWindow(), 40, 80), QColor(Qt::red), 5'000);
        overlay->setVisible(false);
        window.showNormal();
        window.resize(400, 300);
        QTRY_COMPARE_WITH_TIMEOUT(pixel(window.grabWindow(), 40, 80), QColor(Qt::red), 5'000);
        QVERIFY(callback->importedSlots.load() <= 8);
        const auto releases = callback->releases.load();
        video->setRenderCallback(nullptr);
        window.grabWindow();
        QTRY_VERIFY(callback->releases.load() > releases);
    }
};

QTEST_MAIN(StreamVideoItemTest)
#include "tst_streamvideoitem.moc"
