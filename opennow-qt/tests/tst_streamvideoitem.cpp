#include "streaming/StreamVideoItem.h"
#include "streaming/PhysicalKeyMapData.h"
#include "streaming/rendering/LinuxVulkanGraphics.h"
#include "streaming/NativeStreamRuntime.h"
#include "streaming/rendering/StreamVideoTextureRenderer.h"
#include "input/platform/WaylandPointerCapture.h"
#include "input/platform/MacPointerCapture.h"

#include <QGuiApplication>
#include <QClipboard>
#include <QKeySequence>
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
#include <qpa/qwindowsysteminterface.h>

#include <atomic>
#include <algorithm>
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

    void setFsrUpscaling(bool enabled) override
    {
        fsrUpscaling.store(enabled);
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

    void setSwapGated(bool gated, const QString &source) override
    {
        ++gateSetCount;
        swapGated.store(gated);
        gateSource = source;
    }

    std::atomic_bool validContext = false;
    std::atomic_bool swapGated = false;
    std::atomic_int gateSetCount = 0;
    QString gateSource;
    std::atomic_bool fsrUpscaling = false;
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
    void clipboardPasteRouting_data()
    {
        QTest::addColumn<bool>("fullscreen");
        QTest::newRow("windowed") << false;
        QTest::newRow("fullscreen") << true;
    }

    void clipboardPasteRouting()
    {
        QFETCH(bool, fullscreen);
        static QList<QList<quint16>> keys;
        static QList<QByteArray> texts;
        static OpenNowStreamerStatus textStatus;
        keys.clear();
        texts.clear();
        textStatus = OPENNOW_STREAMER_OK;
        auto api = CursorSession::api();
        api.submitKey = [](const OpenNowStreamer *, std::uint16_t vk,
                           std::uint16_t modifiers, bool pressed) {
            keys.append({vk, modifiers, quint16(pressed)});
            return OPENNOW_STREAMER_OK;
        };
        api.submitText = [](const OpenNowStreamer *, const std::uint8_t *text, std::size_t size) {
            if (textStatus == OPENNOW_STREAMER_OK)
                texts.append(QByteArray(reinterpret_cast<const char *>(text), qsizetype(size)));
            return textStatus;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("paste-test")}}));
        const QByteArray ready = R"({"id":"paste-test","type":"ok"})";
        CursorSession::callbacks.response_callback(
            reinterpret_cast<const std::uint8_t *>(ready.constData()), ready.size(),
            CursorSession::callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        if (fullscreen) window.showFullScreen();
        else window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        const auto paste = QKeySequence::keyBindings(QKeySequence::Paste).first()[0];
        const auto sendPaste = [&] {
            QTest::keyClick(&window, paste.key(), paste.keyboardModifiers());
        };
        auto *clipboard = QGuiApplication::clipboard();
        const auto previousText = clipboard->text();
        const auto restoreClipboard = qScopeGuard([&] { clipboard->setText(previousText); });
        const auto text = QString::fromUtf8("Hello café 世界 🎮\nsecond\tline");
        clipboard->setText(text);
        QVERIFY(!item->clipboardPaste());
        sendPaste();
        QVERIFY(texts.isEmpty());
        QVERIFY(std::any_of(keys.cbegin(), keys.cend(), [](const auto &key) {
            return key[0] == 0x56 && key[2] == 1;
        }));
        keys.clear();
        QSignalSpy changes(item, &StreamVideoItem::clipboardPasteChanged);
        QSignalSpy failures(item, &StreamVideoItem::clipboardPasteFailed);
        item->setClipboardPaste(true);
        item->setClipboardPaste(true);
        QCOMPARE(changes.size(), 1);
        sendPaste();
        QCOMPARE(texts, QList<QByteArray>{text.toUtf8()});
        QVERIFY(std::none_of(keys.cbegin(), keys.cend(), [](const auto &key) {
            return key[0] == 0x56;
        }));
        QVERIFY(item->m_pressedKeys.isEmpty());
        QVERIFY(item->m_pressedShortcuts.isEmpty());
        QKeyEvent repeat(QEvent::KeyPress, paste.key(), paste.keyboardModifiers(), {}, true);
        item->keyPressEvent(&repeat);
        QCOMPARE(texts.size(), 1);
        QCOMPARE(failures.size(), 0);
        for (const auto &invalid : {QString{}, QString(65'537, u'x'),
                                   QString(32'769, QChar(0x00e9)),
                                   QString(QChar::Null)}) {
            clipboard->setText(invalid);
            sendPaste();
        }
        QCOMPARE(failures.size(), 4);
        QCOMPARE(texts.size(), 1);
        clipboard->setText(QString(65'536, u'x'));
        sendPaste();
        QCOMPARE(texts.last().size(), 65'536);
        textStatus = OPENNOW_STREAMER_QUEUE_FULL;
        sendPaste();
        QCOMPARE(failures.size(), 5);
        QCOMPARE(texts.size(), 2);
        textStatus = OPENNOW_STREAMER_OK;
        QSignalSpy shortcuts(item, &StreamVideoItem::localShortcutRequested);
        item->setShortcutBindings({{QStringLiteral("test"),
            QKeySequence(paste).toString(QKeySequence::PortableText)}});
        sendPaste();
        QCOMPARE(shortcuts.size(), 1);
        QCOMPARE(texts.size(), 2);
        item->setShortcutBindings({});
        item->setInputEnabled(false);
        auto *overlay = new QQuickItem(window.contentItem());
        overlay->forceActiveFocus();
        sendPaste();
        QCOMPARE(texts.size(), 2);
        item->setInputEnabled(true);
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        sendPaste();
        QCOMPARE(texts.size(), 3);
        item->setFocus(false);
        sendPaste();
        QCOMPARE(texts.size(), 3);
        QCOMPARE(runtime.submitText(QByteArray(65'537, 'x')), OPENNOW_STREAMER_MESSAGE_TOO_LARGE);
        QCOMPARE(runtime.submitText(QByteArray(1, char(0xff))), OPENNOW_STREAMER_INVALID_CONFIG);
        QCOMPARE(runtime.submitText(QByteArray(1, '\0')), OPENNOW_STREAMER_INVALID_CONFIG);
        QCOMPARE(runtime.submitText({}), OPENNOW_STREAMER_INVALID_CONFIG);
    }

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

    void hiddenWindowSynchronizesANewlyAttachedCallbackGate()
    {
        QQuickWindow window;
        window.resize(320, 240);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setWidth(320);
        item->setHeight(240);
        window.show();
        QTRY_VERIFY(window.isVisible());
        window.hide();
        QTRY_VERIFY(!window.isVisible());
        auto callback = std::make_shared<TestRenderCallback>();
        item->setRenderCallback(callback);
        QCOMPARE(callback->gateSetCount.load(), 1);
        QVERIFY(callback->swapGated.load());
        QCOMPARE(callback->gateSource, QStringLiteral("hidden"));
        QCOMPARE(item->swapStats().value(QStringLiteral("gated")).toBool(), true);
        QCOMPARE(item->swapStats().value(QStringLiteral("gateSource")).toString(),
                 QStringLiteral("hidden"));
        window.show();
        QTRY_VERIFY(window.isVisible());
        QTRY_COMPARE(callback->gateSetCount.load(), 2);
        QVERIFY(!callback->swapGated.load());
        QCOMPARE(item->swapStats().value(QStringLiteral("gated")).toBool(), false);
        QVERIFY(!item->swapStats().contains(QStringLiteral("gateSource")));
    }

    void hidingAWindowGatesTheAttachedCallbackAndShowingReleasesIt()
    {
        QQuickWindow window;
        window.resize(320, 240);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setWidth(320);
        item->setHeight(240);
        window.show();
        QTRY_VERIFY(window.isVisible());
        auto callback = std::make_shared<TestRenderCallback>();
        item->setRenderCallback(callback);
        QCOMPARE(callback->gateSetCount.load(), 1);
        QVERIFY(!callback->swapGated.load());
        window.hide();
        QTRY_VERIFY(!window.isVisible());
        QTRY_COMPARE(callback->gateSetCount.load(), 2);
        QVERIFY(callback->swapGated.load());
        QCOMPARE(callback->gateSource, QStringLiteral("hidden"));
        item->setRenderCallback(nullptr);
        window.show();
        QTRY_VERIFY(window.isVisible());
        QCOMPARE(callback->gateSetCount.load(), 2);
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
                                         "VK_EXT_image_drm_format_modifier", "VK_KHR_image_format_list",
                                         "VK_EXT_queue_family_foreign"};
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
        // VK_EXT_queue_family_foreign is never promoted, so it stays required.
        QVERIFY(promoted.contains("VK_EXT_queue_family_foreign"));
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

    void preservesWindowsVirtualKeysAcrossLayouts_data()
    {
        QTest::addColumn<int>("key");
        QTest::addColumn<quint32>("nativeKey");
        QTest::addColumn<quint16>("expected");
        QTest::newRow("cyrillic-w") << 0x0426 << quint32(0x57) << quint16(0x57);
        QTest::newRow("cyrillic-a") << 0x0424 << quint32(0x41) << quint16(0x41);
        QTest::newRow("cyrillic-s") << 0x042b << quint32(0x53) << quint16(0x53);
        QTest::newRow("cyrillic-d") << 0x0412 << quint32(0x44) << quint16(0x44);
        QTest::newRow("french-number-row") << int(Qt::Key_Eacute) << quint32(0x32) << quint16(0x32);
        QTest::newRow("numpad-one") << int(Qt::Key_1) << quint32(0x61) << quint16(0x61);
        QTest::newRow("right-shift") << int(Qt::Key_Shift) << quint32(0xa1) << quint16(0xa1);
        QTest::newRow("generic-control") << int(Qt::Key_Control) << quint32(0x11) << quint16(0xa2);
        QTest::newRow("synthetic-w") << int(Qt::Key_W) << quint32(0) << quint16(0x57);
        QTest::newRow("invalid-native") << int(Qt::Key_W) << quint32(0x10057) << quint16(0x57);
    }

    void preservesWindowsVirtualKeysAcrossLayouts()
    {
        QFETCH(int, key);
        QFETCH(quint32, nativeKey);
        QFETCH(quint16, expected);
        QCOMPARE(StreamVideoItem::windowsVirtualKey(key, Qt::NoModifier, nativeKey), expected);
    }

    void mapsGermanUmlautAndMinusAwayFromLayoutVirtualKeys()
    {
        const auto gameplay = [](int key, quint32 scanCode, quint32 layoutVirtualKey) {
            return StreamVideoItem::windowsGameplayVirtualKey(
                key, Qt::NoModifier, scanCode, layoutVirtualKey);
        };
        // German Windows reports ü as VK_OEM_1 and the hyphen key as VK_OEM_MINUS.
        // Those are the physical positions of ö and ß. GFN wants the US position.
        const auto uUmlaut = gameplay(Qt::Key_Udiaeresis, 0x1a, 0xba);
        const auto oUmlaut = gameplay(Qt::Key_Odiaeresis, 0x27, 0xc0);
        const auto hyphen = gameplay(Qt::Key_Minus, 0x35, 0xbd);
        const auto eszett = gameplay(Qt::Key_ssharp, 0x0c, 0xdb);
        QCOMPARE(uUmlaut, quint16(0xdb));
        QCOMPARE(oUmlaut, quint16(0xba));
        QVERIFY(uUmlaut != oUmlaut);
        QCOMPARE(hyphen, quint16(0xbf));
        QCOMPARE(eszett, quint16(0xbd));
        QVERIFY(hyphen != eszett);
        QCOMPARE(gameplay(Qt::Key_Slash, 0x35, 0x6f), quint16(0x6f));
        QCOMPARE(gameplay(Qt::Key_Y, 0x2c, 0x59), quint16(0x5a));
        QCOMPARE(gameplay(Qt::Key_Z, 0x15, 0x5a), quint16(0x59));
        QCOMPARE(gameplay(0x0426, 0, 0x57), quint16(0x57));

        // French AZERTY assigns VK_Z to the physical W key, VK_Q to physical A,
        // and VK_M to the semicolon key. The set-1 scan code keeps the US position.
        const auto azertyZ = gameplay(Qt::Key_Z, 0x11, 0x5a);
        const auto azertyQ = gameplay(Qt::Key_Q, 0x1e, 0x51);
        const auto azertyW = gameplay(Qt::Key_W, 0x2c, 0x57);
        const auto azertyA = gameplay(Qt::Key_A, 0x10, 0x41);
        const auto azertyM = gameplay(Qt::Key_M, 0x27, 0x4d);
        QCOMPARE(azertyZ, quint16(0x57));
        QVERIFY(azertyZ != quint16(0x5a));
        QCOMPARE(azertyQ, quint16(0x41));
        QVERIFY(azertyQ != quint16(0x51));
        QCOMPARE(azertyW, quint16(0x5a));
        QVERIFY(azertyW != quint16(0x57));
        QCOMPARE(azertyA, quint16(0x51));
        QVERIFY(azertyA != quint16(0x41));
        QCOMPARE(azertyM, quint16(0xba));
        QVERIFY(azertyM != quint16(0x4d));
        QCOMPARE(gameplay(Qt::Key_Eacute, 0x03, 0x32), quint16(0x32));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_Eacute), quint16(0));
        // Cyrillic ц sits on US W. Neither the character nor a substituted VK moves it.
        QCOMPARE(gameplay(0x0446, 0x11, 0x5a), quint16(0x57));

        const auto mac = [](int key, quint32 keyCode) {
            return StreamVideoItem::macGameplayVirtualKey(key, Qt::NoModifier, keyCode);
        };
        const auto macU = mac(Qt::Key_Udiaeresis, 0x21);
        const auto macO = mac(Qt::Key_Odiaeresis, 0x29);
        const auto macHyphen = mac(Qt::Key_Minus, 0x2c);
        const auto macEszett = mac(Qt::Key_ssharp, 0x1b);
        QCOMPARE(macU, quint16(0xdb));
        QCOMPARE(macO, quint16(0xba));
        QVERIFY(macU != macO);
        QCOMPARE(macHyphen, quint16(0xbf));
        QCOMPARE(macEszett, quint16(0xbd));
        QVERIFY(macHyphen != macEszett);
        QCOMPARE(mac(Qt::Key_Y, 0x06), quint16(0x5a));
        QCOMPARE(mac(Qt::Key_Z, 0x10), quint16(0x59));
        QCOMPARE(mac(Qt::Key_Z, 0x0d), quint16(0x57));
        QVERIFY(mac(Qt::Key_Z, 0x0d) != quint16(0x5a));
        QCOMPARE(mac(Qt::Key_W, 0x06), quint16(0x5a));
        QCOMPARE(mac(Qt::Key_A, 0x0c), quint16(0x51));
        QCOMPARE(mac(Qt::Key_M, 0x29), quint16(0xba));
        QVERIFY(mac(Qt::Key_M, 0x29) != quint16(0x4d));
        QCOMPARE(mac(Qt::Key_Eacute, 0x13), quint16(0x32));
        QCOMPARE(mac(0x0446, 0x0d), quint16(0x57));
        // Key code 0 is both ANSI A and a Qt event with no native key.
        QCOMPARE(mac(Qt::Key_A, 0), quint16(0x41));
        QCOMPARE(mac(Qt::Key_W, 0), quint16(0x57));
        QCOMPARE(mac(Qt::Key_Minus, 0), quint16(0xbd));

        StreamVideoItem keyboard;
#if defined(Q_OS_LINUX)
        QKeyEvent uPress(QEvent::KeyPress, Qt::Key_Udiaeresis, Qt::NoModifier, 34, 0xba, 0);
        QKeyEvent oPress(QEvent::KeyPress, Qt::Key_Odiaeresis, Qt::NoModifier, 47, 0xc0, 0);
        QKeyEvent hyphenPress(QEvent::KeyPress, Qt::Key_Minus, Qt::NoModifier, 61, 0xbd, 0);
        QKeyEvent eszettPress(QEvent::KeyPress, Qt::Key_ssharp, Qt::NoModifier, 20, 0xdb, 0);
        QKeyEvent azertyPress(QEvent::KeyPress, Qt::Key_Z, Qt::NoModifier, 25, 0x5a, 0);
        QKeyEvent cyrillicPress(QEvent::KeyPress, 0x0446, Qt::NoModifier, 25, 0x5a, 0);
#elif defined(Q_OS_WIN)
        QKeyEvent uPress(QEvent::KeyPress, Qt::Key_Udiaeresis, Qt::NoModifier, 0x1a, 0xba, 0);
        QKeyEvent oPress(QEvent::KeyPress, Qt::Key_Odiaeresis, Qt::NoModifier, 0x27, 0xc0, 0);
        QKeyEvent hyphenPress(QEvent::KeyPress, Qt::Key_Minus, Qt::NoModifier, 0x35, 0xbd, 0);
        QKeyEvent eszettPress(QEvent::KeyPress, Qt::Key_ssharp, Qt::NoModifier, 0x0c, 0xdb, 0);
        QKeyEvent azertyPress(QEvent::KeyPress, Qt::Key_Z, Qt::NoModifier, 0x11, 0x5a, 0);
        QKeyEvent cyrillicPress(QEvent::KeyPress, 0x0446, Qt::NoModifier, 0x11, 0x5a, 0);
#elif defined(Q_OS_MACOS)
        QKeyEvent uPress(QEvent::KeyPress, Qt::Key_Udiaeresis, Qt::NoModifier, 0, 0x21, 0);
        QKeyEvent oPress(QEvent::KeyPress, Qt::Key_Odiaeresis, Qt::NoModifier, 0, 0x29, 0);
        QKeyEvent hyphenPress(QEvent::KeyPress, Qt::Key_Minus, Qt::NoModifier, 0, 0x2c, 0);
        QKeyEvent eszettPress(QEvent::KeyPress, Qt::Key_ssharp, Qt::NoModifier, 0, 0x1b, 0);
        QKeyEvent azertyPress(QEvent::KeyPress, Qt::Key_Z, Qt::NoModifier, 0, 0x0d, 0);
        QKeyEvent cyrillicPress(QEvent::KeyPress, 0x0446, Qt::NoModifier, 0, 0x0d, 0);
#endif
#if defined(Q_OS_LINUX) || defined(Q_OS_WIN) || defined(Q_OS_MACOS)
        QCOMPARE(keyboard.eventVirtualKey(&uPress), quint16(0xdb));
        QCOMPARE(keyboard.eventVirtualKey(&oPress), quint16(0xba));
        QCOMPARE(keyboard.eventVirtualKey(&hyphenPress), quint16(0xbf));
        QCOMPARE(keyboard.eventVirtualKey(&eszettPress), quint16(0xbd));
        QVERIFY(keyboard.eventVirtualKey(&uPress) != quint16(0xba));
        QVERIFY(keyboard.eventVirtualKey(&hyphenPress) != quint16(0xbd));
        QCOMPARE(keyboard.eventVirtualKey(&azertyPress), quint16(0x57));
        QVERIFY(keyboard.eventVirtualKey(&azertyPress) != quint16(0x5a));
        QCOMPARE(keyboard.eventVirtualKey(&cyrillicPress), quint16(0x57));
#endif
    }

    void nativeKeyboardEventsPreserveGameplayKeys_data()
    {
        QTest::addColumn<bool>("fullscreen");
        QTest::newRow("windowed") << false;
        QTest::newRow("fullscreen") << true;
    }

    void nativeKeyboardEventsPreserveGameplayKeys()
    {
        QFETCH(bool, fullscreen);
        static QList<QList<quint16>> inputCalls;
        static QList<QByteArray> textCalls;
        inputCalls.clear();
        textCalls.clear();
        auto api = CursorSession::api();
        api.submitKey = [](const OpenNowStreamer *, std::uint16_t vk,
                           std::uint16_t modifiers, bool pressed) {
            inputCalls.append(QList<quint16>{vk, modifiers, quint16(pressed)});
            return OPENNOW_STREAMER_OK;
        };
        api.submitText = [](const OpenNowStreamer *, const std::uint8_t *text, std::size_t size) {
            textCalls.append(QByteArray(reinterpret_cast<const char *>(text), qsizetype(size)));
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("native-keyboard")}}));
        const QByteArray ready = R"({"id":"native-keyboard","type":"ok"})";
        CursorSession::callbacks.response_callback(
            reinterpret_cast<const std::uint8_t *>(ready.constData()), ready.size(),
            CursorSession::callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        auto *overlay = new QQuickItem(window.contentItem());
        if (fullscreen) window.showFullScreen();
        else window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        const struct {
            int key;
            quint32 scanCode;
            quint32 nativeKey;
            quint16 expected;
        } keys[] = {
#if defined(Q_OS_WIN)
            {0x0426, 0x11, 0x57, 0x57},
            {0x0424, 0x1e, 0x41, 0x41},
            {0x042b, 0x1f, 0x53, 0x53},
            {0x0412, 0x20, 0x44, 0x44},
            {Qt::Key_Udiaeresis, 0x1a, 0xba, 0xdb},
            {Qt::Key_Odiaeresis, 0x27, 0xc0, 0xba},
            {Qt::Key_Minus, 0x35, 0xbd, 0xbf},
            {Qt::Key_ssharp, 0x0c, 0xdb, 0xbd},
            {Qt::Key_Z, 0x11, 0x5a, 0x57},
            {Qt::Key_Q, 0x1e, 0x51, 0x41},
            {Qt::Key_M, 0x27, 0x4d, 0xba},
            {0x0446, 0x11, 0x5a, 0x57},
#elif defined(Q_OS_MACOS)
            {Qt::Key_W, 1, 0x0d, 0x57},
            {Qt::Key_S, 3, 0x01, 0x53},
            {Qt::Key_D, 4, 0x02, 0x44},
            {Qt::Key_Udiaeresis, 5, 0x21, 0xdb},
            {Qt::Key_Odiaeresis, 6, 0x29, 0xba},
            {Qt::Key_Minus, 7, 0x2c, 0xbf},
            {Qt::Key_ssharp, 8, 0x1b, 0xbd},
            {Qt::Key_Z, 9, 0x0d, 0x57},
            {Qt::Key_M, 10, 0x29, 0xba},
            {0x0446, 11, 0x0d, 0x57},
#else
            {Qt::Key_W, 25, 0x77, 0x57},
            {Qt::Key_A, 38, 0x61, 0x41},
            {Qt::Key_S, 39, 0x73, 0x53},
            {Qt::Key_D, 40, 0x64, 0x44},
#endif
        };
        for (const auto &key : keys) {
            inputCalls.clear();
            QKeyEvent press(QEvent::KeyPress, key.key, Qt::NoModifier,
                            key.scanCode, key.nativeKey, 0);
            QCoreApplication::sendEvent(&window, &press);
            QCOMPARE(inputCalls, (QList<QList<quint16>>{{key.expected, 0, 1}}));
            QKeyEvent release(QEvent::KeyRelease, Qt::Key_unknown, Qt::NoModifier,
                              key.scanCode, 0, 0);
            QCoreApplication::sendEvent(&window, &release);
            QCOMPARE(inputCalls, (QList<QList<quint16>>{
                {key.expected, 0, 1}, {key.expected, 0, 0}}));
            QCoreApplication::sendEvent(&window, &press);
            item->setInputEnabled(false);
            overlay->forceActiveFocus();
            QCOMPARE(inputCalls.last(), (QList<quint16>{key.expected, 0, 0}));
            inputCalls.clear();
            QCoreApplication::sendEvent(&window, &press);
            QCoreApplication::sendEvent(&window, &release);
            QVERIFY(inputCalls.isEmpty());
            item->setInputEnabled(true);
            item->forceActiveFocus();
            QTRY_VERIFY(item->captureActive());
            QCoreApplication::sendEvent(&window, &press);
            QCoreApplication::sendEvent(&window, &release);
            QCOMPARE(inputCalls, (QList<QList<quint16>>{
                {key.expected, 0, 1}, {key.expected, 0, 0}}));
        }
#if defined(Q_OS_WIN)
        item->setShortcutBindings({{QStringLiteral("menu"), QStringLiteral("Ctrl+G")}});
        QSignalSpy shortcuts(item, &StreamVideoItem::localShortcutRequested);
        inputCalls.clear();
        QKeyEvent shortcutPress(QEvent::KeyPress, 0x041f, Qt::ControlModifier, 0x22, 0x47, 0);
        QKeyEvent shortcutRelease(QEvent::KeyRelease, 0x041f, Qt::ControlModifier, 0x22, 0x47, 0);
        QCoreApplication::sendEvent(&window, &shortcutPress);
        QCoreApplication::sendEvent(&window, &shortcutRelease);
        QCOMPARE(shortcuts.size(), 1);
        QCOMPARE(shortcuts.first().first().toString(), QStringLiteral("menu"));
        QVERIFY(inputCalls.isEmpty());
        auto *clipboard = QGuiApplication::clipboard();
        const auto previousText = clipboard->text();
        const auto restoreClipboard = qScopeGuard([&] { clipboard->setText(previousText); });
        clipboard->setText(QStringLiteral("native keyboard paste"));
        item->setClipboardPaste(true);
        QKeyEvent controlPress(QEvent::KeyPress, Qt::Key_Control, Qt::ControlModifier,
                               0x11d, 0xa3, 0);
        QCoreApplication::sendEvent(&window, &controlPress);
        QKeyEvent pastePress(QEvent::KeyPress, 0x041c, Qt::ControlModifier, 0x2f, 0x56, 0);
        QKeyEvent pasteRelease(QEvent::KeyRelease, 0x041c, Qt::ControlModifier, 0x2f, 0x56, 0);
        QCoreApplication::sendEvent(&window, &pastePress);
        QCoreApplication::sendEvent(&window, &pasteRelease);
        QCOMPARE(textCalls, (QList<QByteArray>{"native keyboard paste"}));
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{0xa3, 0, 1}, {0xa3, 0, 0}}));
        QVERIFY(item->m_pressedKeys.isEmpty());
#elif defined(Q_OS_MACOS)
        item->setShortcutBindings({{QStringLiteral("menu"), QStringLiteral("Ctrl+G")}});
        QSignalSpy shortcuts(item, &StreamVideoItem::localShortcutRequested);
        inputCalls.clear();
        QKeyEvent shortcutPress(QEvent::KeyPress, Qt::Key_G, Qt::ControlModifier, 20, 0x05, 0);
        QKeyEvent shortcutRelease(QEvent::KeyRelease, Qt::Key_G, Qt::ControlModifier, 20, 0x05, 0);
        QCoreApplication::sendEvent(&window, &shortcutPress);
        QCoreApplication::sendEvent(&window, &shortcutRelease);
        QCOMPARE(shortcuts.size(), 1);
        QCOMPARE(shortcuts.first().first().toString(), QStringLiteral("menu"));
        QVERIFY(inputCalls.isEmpty());
#endif
    }

    void mapsLinuxScanCodesToPhysicalVirtualKeys_data()
    {
        QTest::addColumn<quint32>("scanCode");
        QTest::addColumn<quint16>("virtualKey");
        const struct {
            const char *name;
            quint32 scanCode;
            quint16 virtualKey;
        } cases[] = {
            {"tlde", 49, 0xc0},
            {"ae01", 10, 0x31},
            {"ae02", 11, 0x32},
            {"ae03", 12, 0x33},
            {"ae04", 13, 0x34},
            {"ae05", 14, 0x35},
            {"ae06", 15, 0x36},
            {"ae07", 16, 0x37},
            {"ae08", 17, 0x38},
            {"ae09", 18, 0x39},
            {"ae10", 19, 0x30},
            {"ae11", 20, 0xbd},
            {"ae12", 21, 0xbb},
            {"ad01", 24, 0x51},
            {"ad02", 25, 0x57},
            {"ad03", 26, 0x45},
            {"ad04", 27, 0x52},
            {"ad05", 28, 0x54},
            {"ad06", 29, 0x59},
            {"ad07", 30, 0x55},
            {"ad08", 31, 0x49},
            {"ad09", 32, 0x4f},
            {"ad10", 33, 0x50},
            {"ad11", 34, 0xdb},
            {"ad12", 35, 0xdd},
            {"ac01", 38, 0x41},
            {"ac02", 39, 0x53},
            {"ac03", 40, 0x44},
            {"ac04", 41, 0x46},
            {"ac05", 42, 0x47},
            {"ac06", 43, 0x48},
            {"ac07", 44, 0x4a},
            {"ac08", 45, 0x4b},
            {"ac09", 46, 0x4c},
            {"ac10", 47, 0xba},
            {"ac11", 48, 0xde},
            {"bksl", 51, 0xdc},
            {"ab01", 52, 0x5a},
            {"ab02", 53, 0x58},
            {"ab03", 54, 0x43},
            {"ab04", 55, 0x56},
            {"ab05", 56, 0x42},
            {"ab06", 57, 0x4e},
            {"ab07", 58, 0x4d},
            {"ab08", 59, 0xbc},
            {"ab09", 60, 0xbe},
            {"ab10", 61, 0xbf},
            {"lsgt", 94, 0xe2},
            {"lfsh", 50, 0xa0},
            {"rtsh", 62, 0xa1},
            {"lctl", 37, 0xa2},
            {"rctl", 105, 0xa3},
            {"lalt", 64, 0xa4},
            {"ralt", 108, 0xa5},
            {"lwin", 133, 0x5b},
            {"rwin", 134, 0x5c},
            {"escape", 9, 0},
            {"backspace", 22, 0},
            {"tab", 23, 0},
            {"return", 36, 0},
            {"caps-lock", 66, 0},
            {"space", 65, 0},
            {"f1", 67, 0},
            {"num-lock", 77, 0},
            {"scroll-lock", 78, 0},
            {"kp0", 90, 0x60},
            {"kp-enter", 104, 0},
            {"print", 107, 0},
            {"home", 110, 0},
            {"up", 111, 0},
            {"insert", 118, 0},
            {"delete", 119, 0},
            {"pause", 127, 0},
            {"unspecified", 0, 0},
            {"out-of-range", 240, 0},
        };
        for (const auto &entry : cases)
            QTest::newRow(entry.name) << entry.scanCode << entry.virtualKey;
    }

    void mapsLinuxScanCodesToPhysicalVirtualKeys()
    {
        QFETCH(quint32, scanCode);
        QFETCH(quint16, virtualKey);
        QCOMPARE(StreamVideoItem::linuxPhysicalVirtualKey(scanCode), virtualKey);
    }

    void preservesPhysicalGameplayKeysAcrossLayouts_data()
    {
        QTest::addColumn<bool>("fullscreen");
        QTest::addColumn<int>("key");
        QTest::addColumn<quint32>("scanCode");
        QTest::addColumn<quint16>("virtualKey");
        const struct {
            const char *name;
            int key;
            quint32 scanCode;
            quint16 virtualKey;
        } layouts[] = {
            {"us-w", Qt::Key_W, 25, 0x57},
            {"us-a", Qt::Key_A, 38, 0x41},
            {"us-s", Qt::Key_S, 39, 0x53},
            {"us-d", Qt::Key_D, 40, 0x44},
            {"qwertz-z-at-us-y", Qt::Key_Z, 29, 0x59},
            {"qwertz-y-at-us-z", Qt::Key_Y, 52, 0x5a},
            {"qwertz-u-umlaut", Qt::Key_Udiaeresis, 34, 0xdb},
            {"qwertz-o-umlaut", Qt::Key_Odiaeresis, 47, 0xba},
            {"qwertz-hyphen", Qt::Key_Minus, 61, 0xbf},
            {"qwertz-eszett", Qt::Key_ssharp, 20, 0xbd},
            {"azerty-z-at-us-w", Qt::Key_Z, 25, 0x57},
            {"azerty-q-at-us-a", Qt::Key_Q, 38, 0x41},
            {"azerty-w-at-us-z", Qt::Key_W, 52, 0x5a},
            {"azerty-m-at-us-semicolon", Qt::Key_M, 47, 0xba},
            {"azerty-eacute-at-us-2", Qt::Key_Eacute, 11, 0x32},
            {"ru-tse-at-us-w", 0x0446, 25, 0x57},
            {"ru-ef-at-us-a", 0x0444, 38, 0x41},
            {"ru-yeru-at-us-s", 0x044b, 39, 0x53},
            {"ru-ve-at-us-d", 0x0432, 40, 0x44},
            {"ru-softsign-at-us-m", 0x044c, 58, 0x4d},
            {"ru-ya-at-us-z", 0x044f, 52, 0x5a},
            {"ru-io-at-us-tilde", 0x0451, 49, 0xc0},
            {"dead-acute-at-us-apostrophe", Qt::Key_Dead_Acute, 48, 0xde},
            {"fallback-backslash-at-xkb-97", Qt::Key_Backslash, 97, 0xdc},
            {"altgr-at-at-us-q", Qt::Key_At, 24, 0x51},
        };
        for (const bool fullscreen : {false, true}) {
            const auto mode = fullscreen ? QStringLiteral("fullscreen") : QStringLiteral("windowed");
            for (const auto &layout : layouts) {
                QTest::newRow(qPrintable(QStringLiteral("%1-%2").arg(layout.name, mode)))
                    << fullscreen << layout.key << layout.scanCode << layout.virtualKey;
            }
        }
    }

    void preservesPhysicalGameplayKeysAcrossLayouts()
    {
        QFETCH(bool, fullscreen);
        QFETCH(int, key);
        QFETCH(quint32, scanCode);
        QFETCH(quint16, virtualKey);
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
                              {QStringLiteral("id"), QStringLiteral("physical-layout")}}));
        const QByteArray ready = R"({"id":"physical-layout","type":"ok"})";
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
        if (fullscreen) window.showFullScreen();
        else window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
#if defined(Q_OS_LINUX)
        const quint32 layoutVirtualKey = virtualKey == 0xba ? quint32(0xdb) : quint32(0xba);
        QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier, scanCode, layoutVirtualKey, 0);
        QCoreApplication::sendEvent(&window, &press);
        QVERIFY(press.isAccepted());
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{virtualKey, 0, 1}}));
        QKeyEvent release(QEvent::KeyRelease, Qt::Key_unknown, Qt::NoModifier, scanCode, 0, 0);
        QCoreApplication::sendEvent(&window, &release);
        QVERIFY(release.isAccepted());
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{virtualKey, 0, 1}, {virtualKey, 0, 0}}));
        QVERIFY(item->m_pressedKeys.isEmpty());
        inputCalls.clear();
        QKeyEvent held(QEvent::KeyPress, key, Qt::NoModifier, scanCode, layoutVirtualKey, 0);
        QCoreApplication::sendEvent(&window, &held);
        overlay->setVisible(true);
        overlay->forceActiveFocus();
        QTRY_VERIFY(!item->captureActive());
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{virtualKey, 0, 1}, {virtualKey, 0, 0}}));
        QVERIFY(item->m_pressedKeys.isEmpty());
        QKeyEvent blocked(QEvent::KeyPress, key, Qt::NoModifier, scanCode, layoutVirtualKey, 0);
        QCoreApplication::sendEvent(&window, &blocked);
        QCOMPARE(inputCalls.size(), 2);
        overlay->setVisible(false);
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        QKeyEvent resumed(QEvent::KeyPress, key, Qt::NoModifier, scanCode, layoutVirtualKey, 0);
        QCoreApplication::sendEvent(&window, &resumed);
        QCOMPARE(inputCalls.last(), (QList<quint16>{virtualKey, 0, 1}));
        item->releaseInput();
        QCOMPARE(inputCalls, (QList<QList<quint16>>{
            {virtualKey, 0, 1}, {virtualKey, 0, 0}, {virtualKey, 0, 1}, {virtualKey, 0, 0}}));
#else
        QSKIP("Physical key codes come from the XKB layout on Linux.");
#endif
    }

    void keepsAltGrDeadKeysAndModifierSidesPhysical()
    {
        static OpenNowStreamerConfig callbacks;
        static QList<QList<quint16>> inputCalls;
        static QList<QByteArray> textCalls;
        inputCalls.clear();
        textCalls.clear();
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
        api.submitText = [](const OpenNowStreamer *, const std::uint8_t *text, std::size_t size) {
            textCalls.append(QByteArray(reinterpret_cast<const char *>(text), qsizetype(size)));
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("altgr")}}));
        const QByteArray ready = R"({"id":"altgr","type":"ok"})";
        callbacks.response_callback(reinterpret_cast<const std::uint8_t *>(ready.constData()),
                                    ready.size(), callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
#if defined(Q_OS_LINUX)
        const auto send = [&](QEvent::Type type, int key, Qt::KeyboardModifiers modifiers,
                              quint32 scanCode) {
            QKeyEvent event(type, key, modifiers, scanCode, 0, 0);
            QCoreApplication::sendEvent(&window, &event);
            return event.isAccepted();
        };
        QVERIFY(send(QEvent::KeyPress, Qt::Key_AltGr,
                     Qt::ControlModifier | Qt::AltModifier, 108));
        QVERIFY(send(QEvent::KeyPress, Qt::Key_At,
                     Qt::ControlModifier | Qt::AltModifier, 24));
        QVERIFY(send(QEvent::KeyRelease, Qt::Key_At,
                     Qt::ControlModifier | Qt::AltModifier, 24));
        QVERIFY(send(QEvent::KeyRelease, Qt::Key_AltGr, Qt::NoModifier, 108));
        QCOMPARE(inputCalls, (QList<QList<quint16>>{
            {0xa5, 0x06, 1}, {0x51, 0x06, 1}, {0x51, 0x06, 0}, {0xa5, 0x00, 0}}));
        QVERIFY(item->m_pressedKeys.isEmpty());
        inputCalls.clear();
        QVERIFY(send(QEvent::KeyPress, Qt::Key_Dead_Acute, Qt::NoModifier, 48));
        QVERIFY(send(QEvent::KeyRelease, Qt::Key_Dead_Acute, Qt::NoModifier, 48));
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{0xde, 0, 1}, {0xde, 0, 0}}));
        inputCalls.clear();
        const struct {
            int key;
            quint32 scanCode;
            quint16 virtualKey;
        } sides[] = {
            {Qt::Key_Shift, 62, 0xa1},
            {Qt::Key_Control, 105, 0xa3},
            {Qt::Key_Alt, 108, 0xa5},
            {Qt::Key_Meta, 134, 0x5c},
        };
        for (const auto &side : sides) {
            QKeyEvent press(QEvent::KeyPress, side.key, Qt::NoModifier, side.scanCode, 0, 0);
            QCoreApplication::sendEvent(&window, &press);
            QVERIFY(press.isAccepted());
            QCOMPARE(inputCalls, (QList<QList<quint16>>{{side.virtualKey, 0, 1}}));
            QKeyEvent release(QEvent::KeyRelease, side.key, Qt::NoModifier, side.scanCode, 0, 0);
            QCoreApplication::sendEvent(&window, &release);
            QVERIFY(release.isAccepted());
            QCOMPARE(inputCalls, (QList<QList<quint16>>{
                {side.virtualKey, 0, 1}, {side.virtualKey, 0, 0}}));
            QVERIFY(item->m_pressedKeys.isEmpty());
            inputCalls.clear();
        }
        item->setShortcutBindings({{QStringLiteral("guide"), QStringLiteral("Ctrl+G")}});
        QSignalSpy shortcuts(item, &StreamVideoItem::localShortcutRequested);
        QKeyEvent shortcutPress(QEvent::KeyPress, 0x043f, Qt::ControlModifier, 42, 0, 0);
        QCoreApplication::sendEvent(&window, &shortcutPress);
        QVERIFY(shortcutPress.isAccepted());
        QCOMPARE(shortcuts.size(), 1);
        QCOMPARE(shortcuts.first().first().toString(), QStringLiteral("guide"));
        QKeyEvent shortcutRelease(QEvent::KeyRelease, 0x043f, Qt::ControlModifier, 42, 0, 0);
        QCoreApplication::sendEvent(&window, &shortcutRelease);
        QCOMPARE(inputCalls.size(), 0);
        QVERIFY(item->m_pressedKeys.isEmpty());
        QVERIFY(item->m_pressedShortcuts.isEmpty());
        item->setShortcutBindings({});
        auto *clipboard = QGuiApplication::clipboard();
        const auto previousText = clipboard->text();
        const auto restoreClipboard = qScopeGuard([&] { clipboard->setText(previousText); });
        clipboard->setText(QStringLiteral("physical layout paste"));
        item->setClipboardPaste(true);
        QKeyEvent controlPress(QEvent::KeyPress, Qt::Key_Control, Qt::ControlModifier, 37, 0, 0);
        QCoreApplication::sendEvent(&window, &controlPress);
        QVERIFY(send(QEvent::KeyPress, 0x043c, Qt::ControlModifier, 55));
        QVERIFY(send(QEvent::KeyRelease, 0x043c, Qt::ControlModifier, 55));
        QKeyEvent controlRelease(QEvent::KeyRelease, Qt::Key_Control, Qt::NoModifier, 37, 0, 0);
        QCoreApplication::sendEvent(&window, &controlRelease);
        QCOMPARE(textCalls, QList<QByteArray>{QByteArray("physical layout paste")});
        QVERIFY(std::none_of(inputCalls.cbegin(), inputCalls.cend(), [](const auto &call) {
            return call[0] == 0x56;
        }));
        QVERIFY(item->m_pressedKeys.isEmpty());
        item->setClipboardPaste(false);
#else
        QSKIP("Physical key codes come from the XKB layout on Linux.");
#endif
    }

#if defined(Q_OS_LINUX)
    void linuxRegionalKeysUsePhysicalPositions_data()
    {
        QTest::addColumn<QString>("layout");
        QTest::addColumn<int>("key");
        QTest::addColumn<int>("modifiers");
        QTest::addColumn<quint32>("xkbKeycode");
        QTest::addColumn<quint16>("expectedKey");
        QTest::addColumn<quint16>("expectedModifiers");
        const auto row = [](const char *name, const char *layout, int key, int modifiers,
                            quint32 evdevCode, quint16 expectedKey, quint16 expectedModifiers = 0) {
            QTest::newRow(name) << QString::fromLatin1(layout) << key << modifiers
                               << quint32(evdevCode ? evdevCode + 8 : 0)
                               << expectedKey << expectedModifiers;
        };
        row("us-semicolon", "en-US", Qt::Key_Colon, Qt::ShiftModifier, 39, 0xba, 1);
        row("uk-hash", "en-GB", Qt::Key_NumberSign, 0, 43, 0xde);
        row("uk-backslash", "en-GB", Qt::Key_Backslash, 0, 86, 0xdc);
        row("turkish-dotless-i", "tr-TR", 0x0131, 0, 23, 0x49);
        row("turkish-dotted-i", "tr-TR", 0x0130, 0, 40, 0xde);
        row("german-y", "de-DE", Qt::Key_Y, 0, 44, 0x59);
        row("german-z", "de-DE", Qt::Key_Z, 0, 21, 0x5a);
        row("german-umlaut", "de-DE", Qt::Key_Udiaeresis, 0, 26, 0xba);
        row("french-a", "fr-FR", Qt::Key_A, 0, 16, 0x41);
        row("french-q", "fr-FR", Qt::Key_Q, 0, 30, 0x51);
        row("french-m", "fr-FR", Qt::Key_M, 0, 39, 0x4d);
        row("french-e-acute", "fr-FR", Qt::Key_Eacute, 0, 3, 0x32);
        row("spanish-enye", "es-ES", Qt::Key_Ntilde, 0, 39, 0xc0);
        row("latin-american-dead-acute", "es-MX", Qt::Key_Dead_Acute, 0, 26, 0xba);
        row("italian-e-grave", "it-IT", Qt::Key_Egrave, 0, 26, 0xba);
        row("portuguese-cedilla", "pt-PT", Qt::Key_Ccedilla, 0, 39, 0xc0);
        row("brazilian-cedilla", "pt-BR", Qt::Key_Ccedilla, 0, 39, 0xba);
        row("brazilian-extra-slash", "pt-BR", Qt::Key_Slash, 0, 89, 0xc1);
        row("brazilian-keypad-decimal", "pt-BR", Qt::Key_Period, Qt::KeypadModifier, 121, 0xc2);
        row("polish-altgr-l", "pl-PL", 0x0141, Qt::GroupSwitchModifier, 38, 0x4c, 6);
        row("danish-ae", "da-DK", Qt::Key_AE, 0, 39, 0xc0);
        row("norwegian-oslash", "nb-NO", Qt::Key_Ooblique, 0, 39, 0xc0);
        row("swedish-altgr-2", "sv-SE", Qt::Key_At, Qt::GroupSwitchModifier, 3, 0x32, 6);
        row("swedish-altgr-e", "sv-SE", 0x20ac, Qt::GroupSwitchModifier, 18, 0x45, 6);
        row("finnish-dead-diaeresis", "fi-FI", Qt::Key_Dead_Diaeresis, 0, 27, 0xba);
        row("russian-cyrillic", "ru-RU", 0x0416, 0, 39, 0xba);
        row("ukrainian-cyrillic", "uk-UA", 0x0406, 0, 31, 0x53);
        row("japanese-at", "ja-JP", Qt::Key_At, 0, 26, 0xc0);
        row("japanese-canonical-id", "ja-106", Qt::Key_At, 0, 26, 0xc0);
        row("japanese-legacy-id", "Japanese106", Qt::Key_At, 0, 26, 0xc0);
        row("spanish-canonical-id", "es-ES_tradnl", Qt::Key_Ntilde, 0, 39, 0xc0);
        row("japanese-yen", "ja-JP", Qt::Key_yen, 0, 124, 0xdc);
        row("japanese-ro", "ja-JP", Qt::Key_Backslash, 0, 89, 0xe2);
        row("japanese-convert", "ja-JP", Qt::Key_Henkan, 0, 92, 0x1c);
        row("japanese-nonconvert", "ja-JP", Qt::Key_Muhenkan, 0, 94, 0x1d);
        row("korean-letter", "ko-KR", 0x3142, 0, 16, 0x51);
        row("korean-hangul", "ko-KR", Qt::Key_Hangul, 0, 122, 0x15);
        row("korean-hanja", "ko-KR", Qt::Key_Hangul_Hanja, 0, 123, 0x19);
        row("chinese-simplified", "zh-CN", Qt::Key_A, 0, 30, 0x41);
        row("chinese-traditional", "zh-TW", Qt::Key_Semicolon, 0, 39, 0xba);
        row("right-alt", "sv-SE", Qt::Key_AltGr, Qt::GroupSwitchModifier, 100, 0xa5);
        row("right-control", "en-US", Qt::Key_Control, Qt::ControlModifier, 97, 0xa3);
        row("right-shift", "en-US", Qt::Key_Shift, Qt::ShiftModifier, 54, 0xa1);
        row("keypad-digit", "fr-FR", Qt::Key_1, Qt::KeypadModifier, 79, 0x61);
        row("keypad-navigation", "fr-FR", Qt::Key_End, Qt::KeypadModifier, 79, 0x61);
        row("keypad-comma", "de-DE", Qt::Key_Comma, Qt::KeypadModifier, 83, 0x6e);
        row("keypad-minus", "en-US", Qt::Key_Minus, Qt::KeypadModifier, 74, 0x6d);
        row("keypad-divide", "en-US", Qt::Key_Slash, Qt::KeypadModifier, 98, 0x6f);
        row("unknown-layout-fallback", "xx-YY", Qt::Key_Y, 0, 21, 0x59);
    }

    void linuxRegionalKeysUsePhysicalPositions()
    {
        linuxPhysicalKeysFollowTheRequestedKeyboardLayout();
    }

    void regionalLayoutLookupIsBoundedAndNormalizesLocaleNames()
    {
        const auto *swedish = PhysicalKeyMap::layoutFor("sv-SE");
        QVERIFY(swedish);
        QCOMPARE(PhysicalKeyMap::layoutFor("SV_se"), swedish);
        QCOMPARE(PhysicalKeyMap::layoutFor("sv"), swedish);
        QCOMPARE(PhysicalKeyMap::layoutFor("nn-NO"), PhysicalKeyMap::layoutFor("nb-NO"));
        QCOMPARE(PhysicalKeyMap::layoutFor("no-NO"), PhysicalKeyMap::layoutFor("nb-NO"));
        QCOMPARE(PhysicalKeyMap::layoutFor("ja-106"), PhysicalKeyMap::layoutFor("ja-JP"));
        QCOMPARE(PhysicalKeyMap::layoutFor("Japanese106"), PhysicalKeyMap::layoutFor("ja-JP"));
        QCOMPARE(PhysicalKeyMap::layoutFor("JA_106"), PhysicalKeyMap::layoutFor("ja-JP"));
        QCOMPARE(PhysicalKeyMap::layoutFor("es-ES_tradnl"), PhysicalKeyMap::layoutFor("es-ES"));
        QVERIFY(!PhysicalKeyMap::layoutFor("unknown"));
        QCOMPARE(PhysicalKeyMap::virtualKey(nullptr, 26), quint16(0));
        QCOMPARE(PhysicalKeyMap::virtualKey(swedish, 128), quint16(0));
        QCOMPARE(PhysicalKeyMap::virtualKey(swedish, 0xffffffff), quint16(0));
        QCOMPARE(PhysicalKeyMap::evdevCodeFromNativeScanCode(0), quint32(0));
        QCOMPARE(PhysicalKeyMap::evdevCodeFromNativeScanCode(7), quint32(0));
    }

    void internationalExtraKeyIsLimitedToBrazilianAndJapaneseLayouts()
    {
        for (const auto &layout : PhysicalKeyMap::layouts) {
            const quint16 expected = layout.locale == "pt-BR" ? 0xc1
                : layout.locale == "ja-JP" ? 0xe2 : 0;
            QCOMPARE(PhysicalKeyMap::virtualKey(PhysicalKeyMap::layoutFor(layout.locale), 89),
                     expected);
        }
    }

    void linuxAltGrAndLayoutChangesPreserveKeyLifetimes_data()
    {
        QTest::addColumn<bool>("fullscreen");
        QTest::newRow("windowed") << false;
        QTest::newRow("fullscreen") << true;
    }

    void linuxAltGrAndLayoutChangesPreserveKeyLifetimes()
    {
        QFETCH(bool, fullscreen);
        static QList<QList<quint16>> inputCalls;
        inputCalls.clear();
        auto api = CursorSession::api();
        api.submitKey = [](const OpenNowStreamer *, std::uint16_t key,
                           std::uint16_t modifiers, bool pressed) {
            inputCalls.append({key, modifiers, quint16(pressed)});
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("regional-lifetimes")}}));
        const QByteArray ready = R"({"id":"regional-lifetimes","type":"ok"})";
        CursorSession::callbacks.response_callback(
            reinterpret_cast<const std::uint8_t *>(ready.constData()), ready.size(),
            CursorSession::callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        item->setKeyboardLayout(QStringLiteral("sv-SE"));
        item->setShortcutBindings({{QStringLiteral("local-e"), QStringLiteral("E")}});
        QSignalSpy shortcuts(item, &StreamVideoItem::localShortcutRequested);
        if (fullscreen) window.showFullScreen();
        else window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        const auto send = [&](QEvent::Type type, int key, quint32 scan) {
            QKeyEvent event(type, key, Qt::NoModifier, scan, 0, 0);
            QCoreApplication::sendEvent(&window, &event);
        };

        send(QEvent::KeyPress, Qt::Key_AltGr, 108);
        send(QEvent::KeyPress, Qt::Key_E, 26);
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{0xa5, 0, 1}, {0x45, 6, 1}}));
        QVERIFY(shortcuts.isEmpty());
        send(QEvent::KeyRelease, Qt::Key_E, 26);
        QCOMPARE(inputCalls.last(), (QList<quint16>{0x45, 6, 0}));
        send(QEvent::KeyPress, Qt::Key_E, 26);
        item->setInputEnabled(false);
        QVERIFY(item->m_pressedKeys.isEmpty());
        QVERIFY(inputCalls.contains({0xa5, 0, 0}));
        QVERIFY(inputCalls.contains({0x45, 0, 0}));
        send(QEvent::KeyRelease, Qt::Key_unknown, 108);
        send(QEvent::KeyRelease, Qt::Key_unknown, 26);
        item->setInputEnabled(true);
        QTRY_VERIFY(item->captureActive());
        send(QEvent::KeyPress, Qt::Key_E, 26);
        QCOMPARE(shortcuts.size(), 1);
        send(QEvent::KeyRelease, Qt::Key_E, 26);

        item->setKeyboardLayout(QStringLiteral("fr-FR"));
        send(QEvent::KeyPress, Qt::Key_A, 24);
        QCOMPARE(inputCalls.last(), (QList<quint16>{0x41, 0, 1}));
        item->setKeyboardLayout(QStringLiteral("en-US"));
        send(QEvent::KeyRelease, Qt::Key_unknown, 24);
        QCOMPARE(inputCalls.last(), (QList<quint16>{0x41, 0, 0}));
        send(QEvent::KeyPress, Qt::Key_Q, 0);
        send(QEvent::KeyRelease, Qt::Key_Q, 0);
        QCOMPARE(inputCalls.last(), (QList<quint16>{0x51, 0, 0}));
        QVERIFY(item->m_pressedKeys.isEmpty());
    }

    void altGrPreservesWireModifiersWithoutTriggeringLocalShortcuts()
    {
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_AltGr), quint16(0xa5));
        QCOMPARE(StreamVideoItem::inputModifiers(Qt::GroupSwitchModifier, Qt::Key_E), quint16(6));
        QCOMPARE(StreamVideoItem::inputModifiers(Qt::GroupSwitchModifier | Qt::ShiftModifier,
                                                Qt::Key_E), quint16(7));
        QCOMPARE(StreamVideoItem::inputModifiers(Qt::GroupSwitchModifier, Qt::Key_AltGr), quint16(0));
        const QVariantMap bindings{{QStringLiteral("plain"), QStringLiteral("E")},
                                  {QStringLiteral("modified"), QStringLiteral("Ctrl+Alt+E")}};
        QVERIFY(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_E,
                    Qt::GroupSwitchModifier).isEmpty());
        QVERIFY(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_E,
                    Qt::GroupSwitchModifier | Qt::ControlModifier | Qt::AltModifier).isEmpty());
        QCOMPARE(StreamVideoItem::shortcutActionForInput(bindings, Qt::Key_E,
                    Qt::ControlModifier | Qt::AltModifier), QStringLiteral("modified"));
    }

    void linuxPhysicalKeysFollowTheRequestedKeyboardLayout_data()
    {
        QTest::addColumn<QString>("layout");
        QTest::addColumn<int>("key");
        QTest::addColumn<int>("modifiers");
        QTest::addColumn<quint32>("xkbKeycode");
        QTest::addColumn<quint16>("expectedKey");
        QTest::addColumn<quint16>("expectedModifiers");
        const auto row = [](const char *name, const char *layout, int key, int modifiers,
                            quint32 xkbKeycode, quint16 expectedKey, quint16 expectedModifiers) {
            QTest::newRow(name) << QString::fromLatin1(layout) << key << modifiers << xkbKeycode
                                << expectedKey << expectedModifiers;
        };
        // Swedish keys that previously produced no input at all.
        row("sv-aring", "sv-SE", Qt::Key_Aring, 0, 34, 0xdd, 0);
        row("sv-odiaeresis", "sv-SE", Qt::Key_Odiaeresis, 0, 47, 0xc0, 0);
        row("sv-adiaeresis", "sv-SE", Qt::Key_Adiaeresis, 0, 48, 0xde, 0);
        row("sv-section", "sv-SE", Qt::Key_section, 0, 49, 0xdc, 0);
        row("sv-dead-acute", "sv-SE", Qt::Key_Dead_Acute, 0, 21, 0xdb, 0);
        row("sv-dead-diaeresis", "sv-SE", Qt::Key_Dead_Diaeresis, 0, 35, 0xba, 0);
        row("sv-apostrophe", "sv-SE", Qt::Key_Apostrophe, 0, 51, 0xbf, 0);
        row("sv-angle-bracket", "sv-SE", Qt::Key_Less, 0, 94, 0xe2, 0);
        row("nb-aring", "nb-NO", Qt::Key_Aring, 0, 34, 0xdd, 0);
        row("da-aring", "da-DK", Qt::Key_Aring, 0, 34, 0xdd, 0);
        // The digit row is VK_0..VK_9 whatever punctuation shift produces.
        row("sv-shift-2", "sv-SE", Qt::Key_QuoteDbl, Qt::ShiftModifier, 11, 0x32, 0x01);
        row("sv-shift-7", "sv-SE", Qt::Key_Slash, Qt::ShiftModifier, 16, 0x37, 0x01);
        row("sv-shift-0", "sv-SE", Qt::Key_Equal, Qt::ShiftModifier, 19, 0x30, 0x01);
        row("us-shift-2", "en-US", Qt::Key_At, Qt::ShiftModifier, 11, 0x32, 0x01);
        row("fr-unshifted-1", "fr-FR", Qt::Key_Ampersand, 0, 10, 0x31, 0);
        // Layouts without a physical table keep the logical-key mapping.
        row("us-bracket-left", "en-US", Qt::Key_BracketLeft, 0, 34, 0xdb, 0);
        row("us-letter", "en-US", Qt::Key_W, 0, 25, 0x57, 0);
        row("sv-letter", "sv-SE", Qt::Key_W, 0, 25, 0x57, 0);
    }

    void linuxPhysicalKeysFollowTheRequestedKeyboardLayout()
    {
        QFETCH(QString, layout);
        QFETCH(int, key);
        QFETCH(int, modifiers);
        QFETCH(quint32, xkbKeycode);
        QFETCH(quint16, expectedKey);
        QFETCH(quint16, expectedModifiers);
        static QList<QList<quint16>> inputCalls;
        inputCalls.clear();
        auto api = CursorSession::api();
        api.submitKey = [](const OpenNowStreamer *, std::uint16_t vk,
                           std::uint16_t wireModifiers, bool pressed) {
            inputCalls.append(QList<quint16>{vk, wireModifiers, quint16(pressed)});
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        const auto reset = qScopeGuard([] { StreamVideoItem::setNativeStreamRuntime(nullptr); });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("physical-keyboard")}}));
        const QByteArray ready = R"({"id":"physical-keyboard","type":"ok"})";
        CursorSession::callbacks.response_callback(
            reinterpret_cast<const std::uint8_t *>(ready.constData()), ready.size(),
            CursorSession::callbacks.user_data);
        QTRY_VERIFY(runtime.inputAllowed());
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setRenderCallback({});
        item->setSize(window.size());
        item->setKeyboardLayout(layout);
        window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());

        QKeyEvent press(QEvent::KeyPress, key, Qt::KeyboardModifiers(modifiers), xkbKeycode, 0, 0);
        QCoreApplication::sendEvent(&window, &press);
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{expectedKey, expectedModifiers, 1}}));
        QKeyEvent release(QEvent::KeyRelease, Qt::Key_unknown, Qt::NoModifier, xkbKeycode, 0, 0);
        QCoreApplication::sendEvent(&window, &release);
        QCOMPARE(inputCalls.last().at(0), expectedKey);
        QCOMPARE(inputCalls.last().at(2), quint16(0));
    }
#endif

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
        const auto movementClick = [&window](Qt::Key key) {
#if defined(Q_OS_MACOS)
            const quint32 nativeKey = key == Qt::Key_W ? 0x0d
                : key == Qt::Key_S ? 0x01 : key == Qt::Key_D ? 0x02 : 0x00;
            for (const auto type : {QEvent::KeyPress, QEvent::KeyRelease}) {
                QWindowSystemInterface::handleExtendedKeyEvent(
                    &window, 1, type, key, Qt::NoModifier, 0, nativeKey, 0);
                QWindowSystemInterface::flushWindowSystemEvents();
            }
#else
            QTest::keyClick(&window, key);
#endif
        };
        for (const auto movement : {Qt::Key_W, Qt::Key_A, Qt::Key_S, Qt::Key_D}) {
            inputCalls.clear();
            movementClick(movement);
            QCOMPARE(inputCalls, (QList<QList<quint16>>{
                {quint16(movement), 0, 1}, {quint16(movement), 0, 0}}));
        }
        item->setInputEnabled(false);
        other->forceActiveFocus();
        inputCalls.clear();
        movementClick(Qt::Key_W);
        QVERIFY(inputCalls.isEmpty());
        item->setInputEnabled(true);
        item->forceActiveFocus();
        QTRY_VERIFY(item->captureActive());
        movementClick(Qt::Key_W);
        QCOMPARE(inputCalls, (QList<QList<quint16>>{{0x57, 0, 1}, {0x57, 0, 0}}));
    }

    void forwardsShiftedPunctuation_data()
    {
        QTest::addColumn<int>("key");
        QTest::addColumn<int>("baseKey");
        QTest::addColumn<quint16>("virtualKey");
        QTest::addColumn<quint32>("scanCode");
        const struct {
            const char *name;
            Qt::Key key;
            Qt::Key baseKey;
            quint16 virtualKey;
            quint32 scanCode;
        } cases[] = {
            {"!", Qt::Key_Exclam, Qt::Key_1, 0x31, 10},
            {"@", Qt::Key_At, Qt::Key_2, 0x32, 11},
            {"#", Qt::Key_NumberSign, Qt::Key_3, 0x33, 12},
            {"$", Qt::Key_Dollar, Qt::Key_4, 0x34, 13},
            {"%", Qt::Key_Percent, Qt::Key_5, 0x35, 14},
            {"^", Qt::Key_AsciiCircum, Qt::Key_6, 0x36, 15},
            {"&", Qt::Key_Ampersand, Qt::Key_7, 0x37, 16},
            {"*", Qt::Key_Asterisk, Qt::Key_8, 0x38, 17},
            {"(", Qt::Key_ParenLeft, Qt::Key_9, 0x39, 18},
            {")", Qt::Key_ParenRight, Qt::Key_0, 0x30, 19},
            {"_", Qt::Key_Underscore, Qt::Key_Minus, 0xbd, 20},
            {"+", Qt::Key_Plus, Qt::Key_Equal, 0xbb, 21},
            {"{", Qt::Key_BraceLeft, Qt::Key_BracketLeft, 0xdb, 34},
            {"}", Qt::Key_BraceRight, Qt::Key_BracketRight, 0xdd, 35},
            {"|", Qt::Key_Bar, Qt::Key_Backslash, 0xdc, 51},
            {":", Qt::Key_Colon, Qt::Key_Semicolon, 0xba, 47},
            {"\"", Qt::Key_QuoteDbl, Qt::Key_Apostrophe, 0xde, 48},
            {"<", Qt::Key_Less, Qt::Key_Comma, 0xbc, 59},
            {">", Qt::Key_Greater, Qt::Key_Period, 0xbe, 60},
            {"?", Qt::Key_Question, Qt::Key_Slash, 0xbf, 61},
            {"~", Qt::Key_AsciiTilde, Qt::Key_QuoteLeft, 0xc0, 49},
        };
        for (const auto &entry : cases) {
            QTest::newRow(entry.name) << int(entry.key) << int(entry.baseKey)
                                      << entry.virtualKey << entry.scanCode;
        }
    }

    void forwardsShiftedPunctuation()
    {
        QFETCH(int, key);
        QFETCH(int, baseKey);
        QFETCH(quint16, virtualKey);
        QFETCH(quint32, scanCode);
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
            for (const quint32 candidate : {0u, scanCode}) {
                for (const bool shiftReleasedFirst : {false, true}) {
                    inputCalls.clear();
                    QKeyEvent press(QEvent::KeyPress, key, Qt::ShiftModifier, candidate, 0, 0);
                    QCoreApplication::sendEvent(&window, &press);
                    QVERIFY(press.isAccepted());
                    QCOMPARE(inputCalls, (QList<QList<quint16>>{{virtualKey, 1, 1}}));
                    QKeyEvent repeat(QEvent::KeyPress, key, Qt::ShiftModifier,
                                     candidate, 0, 0, {}, true);
                    QCoreApplication::sendEvent(&window, &repeat);
                    QCOMPARE(inputCalls.size(), 1);
                    const auto modifiers = shiftReleasedFirst ? Qt::NoModifier : Qt::ShiftModifier;
                    QKeyEvent release(QEvent::KeyRelease, shiftReleasedFirst ? baseKey : key,
                                      modifiers, candidate, 0, 0);
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

    void mapsNativeMacZeroKeycodeWithoutChangingSyntheticFallback()
    {
        for (const int key : {int(Qt::Key_Q), 0x0424, int(Qt::Key_A)}) {
            QCOMPARE(StreamVideoItem::macGameplayVirtualKey(key, Qt::NoModifier, 0, true),
                     quint16(0x41));
        }
        QCOMPARE(StreamVideoItem::macGameplayVirtualKey(Qt::Key_Q, Qt::NoModifier, 0, false),
                 quint16(0x51));
        QCOMPARE(StreamVideoItem::macGameplayVirtualKey(Qt::Key_W, Qt::NoModifier, 0, false),
                 quint16(0x57));
        QCOMPARE(StreamVideoItem::macGameplayVirtualKey(0x0424, Qt::NoModifier, 0, false),
                 quint16(0));
        QCOMPARE(StreamVideoItem::macGameplayVirtualKey(Qt::Key_A, Qt::NoModifier, 0x0c, true),
                 quint16(0x51));
    }

    void preservesNativeMacZeroKeycodeThroughQuickWindowDelivery()
    {
        class KeyProbe final : public StreamVideoItem
        {
        public:
            explicit KeyProbe(QQuickItem *parent)
                : StreamVideoItem(std::make_unique<MacPointerCapture>(), true, parent) {}
            QList<quint16> virtualKeys;
            QList<bool> spontaneous;
            QList<QEvent::Type> types;
            bool forwardToGameplay = false;
            void keyPressEvent(QKeyEvent *event) override
            {
                virtualKeys.append(macGameplayVirtualKey(event));
                spontaneous.append(event->spontaneous());
                types.append(event->type());
                if (forwardToGameplay) {
                    if (event->type() == QEvent::KeyPress)
                        StreamVideoItem::keyPressEvent(event);
                    else
                        StreamVideoItem::keyReleaseEvent(event);
                    return;
                }
                event->accept();
            }
            void keyReleaseEvent(QKeyEvent *event) override { keyPressEvent(event); }
        };
        QQuickWindow window;
        window.resize(320, 240);
        auto *item = new KeyProbe(window.contentItem());
        item->setSize(QSizeF(320, 240));
        window.show();
        window.requestActivate();
        QVERIFY(QTest::qWaitForWindowActive(&window));
        item->forceActiveFocus();
        QVERIFY(item->hasActiveFocus());
        for (const int key : {int(Qt::Key_Q), 0x0424}) {
            for (const auto type : {QEvent::KeyPress, QEvent::KeyRelease}) {
                QWindowSystemInterface::handleExtendedKeyEvent(
                    &window, 1, type, key, Qt::NoModifier, 0, 0, 0);
                QWindowSystemInterface::flushWindowSystemEvents();
            }
        }
        QCOMPARE(item->virtualKeys, (QList<quint16>{0x41, 0x41, 0x41, 0x41}));
        QCOMPARE(item->spontaneous, (QList<bool>{false, false, false, false}));
        QCOMPARE(item->types, (QList<QEvent::Type>{QEvent::KeyPress, QEvent::KeyRelease,
                                                 QEvent::KeyPress, QEvent::KeyRelease}));
        for (const int key : {int(Qt::Key_Q), int(Qt::Key_W)}) {
            QKeyEvent event(QEvent::KeyPress, key, Qt::NoModifier);
            QCoreApplication::sendEvent(&window, &event);
            QVERIFY(event.isAccepted());
        }
        QCOMPARE(item->virtualKeys, (QList<quint16>{0x41, 0x41, 0x41, 0x41, 0x51, 0x57}));
        item->setInputEnabled(false);
        QWindowSystemInterface::handleExtendedKeyEvent(
            &window, 1, QEvent::KeyPress, Qt::Key_Q, Qt::NoModifier, 0, 0, 0);
        QWindowSystemInterface::flushWindowSystemEvents();
        QCOMPARE(item->virtualKeys.last(), quint16(0x51));
        item->setInputEnabled(true);
        item->m_captureActive = true;
        item->forwardToGameplay = true;
        item->setShortcutBindings({{QStringLiteral("test-action"), QStringLiteral("Q")}});
        QSignalSpy shortcuts(item, &StreamVideoItem::localShortcutRequested);
        for (const auto type : {QEvent::KeyPress, QEvent::KeyRelease}) {
            QWindowSystemInterface::handleExtendedKeyEvent(
                &window, 1, type, Qt::Key_Q, Qt::NoModifier, 0, 0, 0);
            QWindowSystemInterface::flushWindowSystemEvents();
        }
        QCOMPARE(shortcuts.count(), 1);
        QVERIFY(item->m_pressedKeys.isEmpty());
        QVERIFY(item->m_pressedShortcuts.isEmpty());

        class ShellKeyProbe final : public QQuickItem
        {
        public:
            using QQuickItem::QQuickItem;
            quint16 virtualKey = 0;
            void keyPressEvent(QKeyEvent *event) override
            {
                virtualKey = StreamVideoItem::macGameplayVirtualKey(event);
                event->accept();
            }
        };
        auto *shell = new ShellKeyProbe(window.contentItem());
        shell->forceActiveFocus();
        QVERIFY(shell->hasActiveFocus());
        const auto deliveredToStream = item->virtualKeys.size();
        QWindowSystemInterface::handleExtendedKeyEvent(
            &window, 1, QEvent::KeyPress, Qt::Key_Q, Qt::NoModifier, 0, 0, 0);
        QWindowSystemInterface::flushWindowSystemEvents();
        QCOMPARE(shell->virtualKey, quint16(0x51));
        QCOMPARE(item->virtualKeys.size(), deliveredToStream);
    }

    void mapsLinuxKeypadPhysicalPositionsWithAndWithoutNumLock()
    {
        StreamVideoItem keyboard;
        const quint32 scanCodes[] = {90, 87, 88, 89, 83, 84, 85, 79, 80, 81};
        const int unlockedKeys[] = {Qt::Key_Insert, Qt::Key_End, Qt::Key_Down,
            Qt::Key_PageDown, Qt::Key_Left, Qt::Key_Clear, Qt::Key_Right,
            Qt::Key_Home, Qt::Key_Up, Qt::Key_PageUp};
        for (int digit = 0; digit < 10; ++digit) {
            const auto expected = quint16(0x60 + digit);
            QCOMPARE(StreamVideoItem::linuxPhysicalVirtualKey(scanCodes[digit]), expected);
            QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_0 + digit, Qt::KeypadModifier),
                     expected);
#if defined(Q_OS_LINUX)
            for (int key : {Qt::Key_0 + digit, unlockedKeys[digit]}) {
                QKeyEvent event(QEvent::KeyPress, key, Qt::KeypadModifier,
                                scanCodes[digit], 0, 0);
                QCOMPARE(keyboard.eventVirtualKey(&event), expected);
            }
#else
            Q_UNUSED(unlockedKeys);
#endif
        }
        struct Operator { int key; quint32 scanCode; quint16 expected; };
        const Operator operators[] = {{Qt::Key_Minus, 82, 0x6d},
            {Qt::Key_Period, 91, 0x6e}, {Qt::Key_Comma, 91, 0x6e},
            {Qt::Key_Slash, 106, 0x6f}};
        for (const auto &entry : operators) {
            QCOMPARE(StreamVideoItem::linuxPhysicalVirtualKey(entry.scanCode), entry.expected);
            QCOMPARE(StreamVideoItem::windowsVirtualKey(entry.key, Qt::KeypadModifier),
                     entry.expected);
#if defined(Q_OS_LINUX)
            QKeyEvent event(QEvent::KeyPress, entry.key, Qt::KeypadModifier,
                            entry.scanCode, 0, 0);
            QCOMPARE(keyboard.eventVirtualKey(&event), entry.expected);
#endif
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

    void clearedShortcutsDoNotConsumeGameplayKeys()
    {
        const QVariantMap bindings{
            {QStringLiteral("guide"), QVariantList{QStringLiteral("Ctrl+G")}},
            {QStringLiteral("toggle-recording"), QVariantList{QString{}}},
            {QStringLiteral("toggle-stats"), QVariantList{QString{}}},
        };
        QVERIFY(StreamVideoItem::shortcutActionForInput(
                    bindings, Qt::Key_F3, Qt::NoModifier).isEmpty());
        QVERIFY(StreamVideoItem::shortcutActionForInput(
                    bindings, Qt::Key_F12, Qt::NoModifier).isEmpty());
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_G, Qt::ControlModifier), QStringLiteral("guide"));
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

    void fsrUpscalingPreservesPresenterAndSourceSettings()
    {
        StreamVideoItem item;
        const auto callback = std::make_shared<TestRenderCallback>();
        item.setRenderCallback(callback);
        item.setVideoSize(QSize(1920, 1080));
        QSignalSpy changes(&item, &StreamVideoItem::fsrUpscalingChanged);
        QVERIFY(!item.fsrUpscaling());
        item.setFsrUpscaling(true);
        QVERIFY(item.fsrUpscaling());
        QCOMPARE(changes.size(), 1);
        item.setFsrUpscaling(true);
        QCOMPARE(changes.size(), 1);
        item.setFsrUpscaling(false);
        QVERIFY(!item.fsrUpscaling());
        QCOMPARE(changes.size(), 2);
        QCOMPARE(item.renderCallback(), callback);
        QCOMPARE(item.videoSize(), QSize(1920, 1080));
        QVERIFY(!item.frameGeneration());
        QVERIFY(!item.metalFxUpscaling());
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

    void upscalingTargetTracksViewportAcrossOverlaysAndWindowChanges_data()
    {
        QTest::addColumn<bool>("fsr");
        QTest::newRow("MetalFX") << false;
        QTest::newRow("FSR1") << true;
    }

    void upscalingTargetTracksViewportAcrossOverlaysAndWindowChanges()
    {
        QFETCH(bool, fsr);
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("The offscreen platform plugin does not create a QRhi.");
        const auto callback = std::make_shared<TestRenderCallback>();
        QQuickWindow window;
        window.resize(640, 480);
        auto *item = new StreamVideoItem(window.contentItem());
        item->setVideoSize(QSize(320, 180));
        item->m_metalFxUpscaling = !fsr;
        item->setFsrUpscaling(fsr);
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
                QCOMPARE(callback->fsrUpscaling.load(), fsr);
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
        item->setFsrUpscaling(false);
        item->requestFrame();
        QTRY_COMPARE(callback->upscaleWidth.load(), -1);
        QTRY_COMPARE(callback->upscaleHeight.load(), -1);
        QTRY_VERIFY(!callback->fsrUpscaling.load());
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
