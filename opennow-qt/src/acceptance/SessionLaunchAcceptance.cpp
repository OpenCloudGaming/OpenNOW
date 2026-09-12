#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"
#include "streaming/StreamVideoItem.h"
#include "streaming/rendering/StreamVideoTextureRenderer.h"

#include <QGuiApplication>
#include <QKeyEvent>
#include <QPointer>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QVariantMap>

#include <cstdlib>
#include <atomic>
#include <memory>

using namespace Qt::StringLiterals;

namespace {
class LaunchVideoFixture final : public StreamVideoRenderCallback
{
public:
    void initialize(QRhi *rhi, QRhiCommandBuffer *, QRhiRenderTarget *target) override
    {
        if (m_rhi != rhi) releaseResources();
        m_rhi = rhi;
        m_renderer.initialize(rhi, target);
    }
    ~LaunchVideoFixture() override { releaseResources(); }
    void setComposition(const QMatrix4x4 &matrix, const QRectF &bounds,
                        const QRectF &viewport, float opacity) override
    {
        m_renderer.setComposition(matrix, bounds, viewport, opacity);
    }
    void prepareFrame(QRhiCommandBuffer *cb) override
    {
        if (!m_renderer.prepare(cb)) return;
        if (!m_texture) {
            m_texture.reset(m_rhi->newTexture(QRhiTexture::RGBA8, QSize(4, 4)));
            if (!m_texture->create()) return;
            QImage image(4, 4, QImage::Format_RGBA8888);
            image.fill(QColor(24, 112, 160));
            auto *updates = m_rhi->nextResourceUpdateBatch();
            updates->uploadTexture(m_texture.get(), image);
            cb->resourceUpdate(updates);
        }
        if (m_renderer.importFrame(m_rhi->currentFrameSlot(), m_texture->nativeTexture(),
                QRhiTexture::RGBA8, QSize(4, 4)))
            m_renderer.selectTexture(m_texture.get());
    }
    void recordFrame(QRhiCommandBuffer *cb, const QRect &) override
    {
        m_renderer.render(cb, false, 0);
        rendered.store(true);
    }
    void finishFrame() override {}
    void releaseResources() override
    {
        m_renderer.release();
        m_texture.reset();
        m_rhi = nullptr;
    }
    std::atomic_bool rendered = false;
private:
    QRhi *m_rhi = nullptr;
    StreamVideoTextureRenderer m_renderer;
    std::unique_ptr<QRhiTexture> m_texture;
};
}

int AcceptanceSession::startSessionLaunchWorkload()
{
    auto *window = m_engine.rootObjects().isEmpty() ? nullptr
        : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
    auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
    if (!window || !store) return EXIT_FAILURE;
    const bool fullscreen = m_arguments.contains(u"--smoke-motion-fullscreen"_s);
    if (fullscreen) window->showFullScreen();
    window->requestActivate();
    store->setProperty("selectedGame", QVariantMap{{u"title"_s, u"Smoke Test Game"_s},
        {u"heroImageUrl"_s, u"qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"_s}});
    const QVariantMap session{{u"sessionId"_s, u"launch-fixture"_s}, {u"queuePosition"_s, 12}};
    store->setProperty("activeSession", session);
    store->setProperty("streamState", u"checking"_s);
    const QVariantMap starting{{u"sessionId"_s, u"launch-fixture"_s}, {u"status"_s, u"starting"_s}};
    store->setProperty("streamer", starting);
    m_controller.navigate(u"library"_s);
    const auto video = m_arguments.contains(u"--smoke-session-launch-video"_s)
        ? std::make_shared<LaunchVideoFixture>() : nullptr;
    if (video) {
        auto *surface = window->findChild<StreamVideoItem *>(u"streamSurfaceHost"_s);
        if (!surface) return EXIT_FAILURE;
        surface->setRenderCallback(video);
    }

    struct State {
        int tick = 0;
        bool entered = false;
        bool exited = false;
        QPointer<QQuickItem> surface;
        QPointer<QObject> page;
    };
    const auto state = std::make_shared<State>();
    auto *timer = new QTimer(this);
    timer->setInterval(30);
    connect(timer, &QTimer::timeout, this, [this, window, store, fullscreen, session, starting, video, state, timer] {
        const auto require = [this, window, state, timer](bool ok, const char *message) {
            if (!ok) {
                qCritical("Session launch tick %d: %s", state->tick, message);
                qCritical() << "route" << m_controller.route() << "overlay" << m_controller.overlay()
                            << "focus" << window->activeFocusItem();
                auto *cover = window->findChild<QQuickItem *>(u"desktopSessionStarting"_s);
                auto *motion = window->findChild<QObject *>(u"sessionLaunchMotion"_s);
                auto *surface = window->findChild<QQuickItem *>(u"streamSurfaceHost"_s);
                qCritical() << "cover" << (cover && cover->isVisible())
                            << "progress" << (motion ? motion->property("progress") : QVariant{})
                            << "status" << (cover ? cover->property("statusText") : QVariant{})
                            << "surface enabled" << (surface && surface->isEnabled());
                timer->stop();
                m_application.exit(EXIT_FAILURE);
            }
            return ok;
        };
        const auto event = [store](const QVariantMap &value) {
            return QMetaObject::invokeMethod(store, "acceptNativeEvent", Q_ARG(QVariant, value));
        };
        const auto key = [window](Qt::Key value) {
            QKeyEvent press(QEvent::KeyPress, value, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &press);
            QKeyEvent release(QEvent::KeyRelease, value, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &release);
        };
        const int tick = ++state->tick;
        auto *cover = window->findChild<QQuickItem *>(u"desktopSessionStarting"_s);
        auto *motion = window->findChild<QObject *>(u"sessionLaunchMotion"_s);
        auto *surface = window->findChild<QQuickItem *>(u"streamSurfaceHost"_s);
        auto *pageLoader = window->findChild<QObject *>(u"desktopPageLoader"_s);
        if (!require(!m_qmlWarningOccurred && cover && motion && surface && pageLoader,
                     "missing launch fixture or QML warning")) return;
        if (!require((window->visibility() == QWindow::FullScreen) == fullscreen,
                     "launch changed the window mode")) return;
        const double progress = motion->property("progress").toDouble();
        if (tick == 1) {
            state->surface = surface;
            state->page = pageLoader->property("item").value<QObject *>();
            m_controller.navigate(u"inserting"_s);
        }
        if (!require(state->surface == surface, "launch recreated the video surface")) return;
        if (tick > 1 && tick < 25) state->entered |= progress > 0 && progress < 1;
        if (tick > 70 && tick < 100) state->exited |= progress > 0 && progress < 1;
        if (tick > 1 && tick < 45
            && !require(state->page == pageLoader->property("item").value<QObject *>(),
                        "launch replaced the page beneath its fade")) return;
        if (tick == 25) {
            if (!require(cover->isVisible() && progress == 1 && !surface->isEnabled()
                    && cover->property("statusText").toString() == u"Queue position 12"_s,
                    "queue did not show its actual position")) return;
            const auto shot = m_arguments.indexOf(u"--screenshot"_s);
            if (shot >= 0 && !require(shot + 1 < m_arguments.size()
                    && window->grabWindow().save(m_arguments.at(shot + 1)),
                    "could not save launch screenshot")) return;
            key(Qt::Key_Escape);
        }
        if (tick == 35) {
            if (!require(m_controller.overlay() == u"desktop-stream-exit-confirm"_s
                    && cover->isVisible() && m_controller.route() == u"inserting"_s,
                    "queue cancellation bypassed confirmation")) return;
            key(Qt::Key_Escape);
            auto preparing = session;
            preparing.insert(u"queuePosition"_s, 0);
            store->setProperty("activeSession", preparing);
            store->setProperty("streamState", u"preparing"_s);
        }
        if (tick == 45) {
            if (!require(cover->property("statusText").toString() == u"Preparing your game"_s
                    && m_controller.overlay().isEmpty(), "queue did not advance to preparation")) return;
            store->setProperty("streamState", u"ready"_s);
            m_controller.navigate(u"stream"_s);
            if (!require(event({{u"type"_s, u"status"_s}, {u"status"_s, u"streaming"_s}}),
                         "receiver status event failed")) return;
        }
        if (tick == 65) {
            if (!require(progress == 1 && cover->isVisible() && surface->isVisible()
                    && !surface->isEnabled() && !surface->hasActiveFocus()
                    && cover->property("statusText").toString() == u"Connecting to your game"_s,
                    "receiver readiness exposed video before its first frame")) return;
            if (video && !require(window->grabWindow().pixelColor(window->width() / 2,
                    window->height() / 4).blue() < 100, "video showed through the waiting cover")) return;
        }
        if (tick == 70 || tick == 160) {
            if (!require(event({{u"type"_s, u"status"_s}, {u"status"_s, u"streaming"_s},
                    {u"event"_s, u"first-frame"_s}}), "first-frame event failed")) return;
        }
        if (tick == 105 || tick == 195) {
            if (!require(!cover->isVisible() && progress == 0 && surface->isVisible()
                    && surface->isEnabled(), "first-frame handoff did not finish")) return;
            if (!m_controller.reducedMotion()
                && !require(state->entered && state->exited, "missing intermediate fade frames")) return;
            if (video && tick == 105
                && !require(video->rendered.load() && window->grabWindow().pixelColor(
                    window->width() / 2, window->height() / 4) == QColor(24, 112, 160),
                    "handoff did not reveal the GPU video fixture")) return;
        }
        if (tick == 110) m_controller.showOverlay(u"desktop-stream-stats"_s);
        if (tick == 120) {
            if (!require(!cover->isVisible() && surface->isEnabled(),
                         "statistics reopened the launch screen")) return;
            m_controller.showOverlay(u"desktop-stream-menu"_s);
        }
        if (tick == 130) {
            if (!require(!cover->isVisible() && !surface->property("inputEnabled").toBool(),
                         "session menu failed to retain video and block input")) return;
            m_controller.showOverlay({});
            store->setProperty("streamer", starting);
            store->setProperty("streamState", u"reconnecting"_s);
        }
        if (tick == 150) {
            if (!require(cover->isVisible() && progress == 1 && !surface->isEnabled()
                    && cover->property("statusText").toString() == u"Reconnecting to your session"_s,
                    "reconnect did not restore the launch cover")) return;
            m_controller.showOverlay(u"desktop-stream-exit-confirm"_s);
        }
        if (tick == 195) {
            if (!require(m_controller.overlay() == u"desktop-stream-exit-confirm"_s
                    && !surface->property("inputEnabled").toBool(),
                    "first frame dismissed confirmation or enabled gameplay behind it")) return;
            key(Qt::Key_Escape);
            store->setProperty("streamer", QVariantMap{{u"status"_s, u"error"_s},
                {u"message"_s, u"Connection interrupted"_s}});
            store->setProperty("streamState", u"error"_s);
        }
        if (tick == 220) {
            if (!require(cover->isVisible() && cover->property("failed").toBool()
                    && cover->property("detailText").toString() == u"Connection interrupted"_s,
                    "failure lost its actionable error")) return;
            m_controller.navigate(u"home"_s);
        }
        if (tick == 250) {
            if (!require(!cover->isVisible() && progress == 0,
                         "leaving the session retained the cover")) return;
            store->setProperty("streamer", starting);
            store->setProperty("streamState", u"requesting"_s);
            m_controller.navigate(u"inserting"_s);
        }
        if (tick == 255) m_controller.navigate(u"home"_s);
        if (tick == 258) m_controller.navigate(u"inserting"_s);
        if (tick == 285) {
            if (!require(cover->isVisible() && progress == 1,
                         "interrupted launch fade did not recover")) return;
            timer->stop();
            m_application.exit(EXIT_SUCCESS);
        }
    });
    timer->start();
    return EXIT_SUCCESS;
}
