#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"
#include "streaming/StreamVideoItem.h"

#include <QGuiApplication>
#include <QJSValue>
#include <QPointer>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QTimer>

#include <cstdlib>
#include <memory>

using namespace Qt::StringLiterals;

namespace {
class StatsRenderCallback final : public StreamVideoRenderCallback
{
public:
    QVariantMap snapshot;
    void initialize(QRhi *, QRhiCommandBuffer *, QRhiRenderTarget *) override {}
    void prepareFrame(QRhiCommandBuffer *) override {}
    void recordFrame(QRhiCommandBuffer *, const QRect &) override {}
    void finishFrame() override {}
    void releaseResources() override {}
    QVariantMap frameGenerationStats() const override { return snapshot; }
};

QVariantMap statsFor(QObject *object)
{
    if (!object) return {};
    const auto value = object->property("frameGenerationStats");
    return value.canConvert<QJSValue>() ? value.value<QJSValue>().toVariant().toMap() : value.toMap();
}
}

int AcceptanceSession::startFrameGenerationStatsWorkload()
{
    auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
    if (!store) return EXIT_FAILURE;
    store->setProperty("settings", QVariantMap{{u"fps"_s, 60}, {u"frameGeneration"_s, u"2x"_s}});
    store->setProperty("streamer", QVariantMap{{u"status"_s, u"streaming"_s}, {u"framesPerSecond"_s, 60}});
    store->setProperty("streamState", u"streaming"_s);
    QTimer::singleShot(150, this, [this] {
        auto *window = m_engine.rootObjects().isEmpty() ? nullptr
            : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        QPointer<StreamVideoItem> surface;
        if (window) {
            for (auto *item : window->findChildren<StreamVideoItem *>(u"streamSurfaceHost"_s)) {
                if (item->isVisible()) surface = item;
            }
        }
        auto *host = window ? window->findChild<QQuickItem *>(u"desktopStreamOverlayHost"_s) : nullptr;
        auto *stats = host ? host->findChild<QQuickItem *>(u"desktopStreamStats"_s) : nullptr;
        auto *loader = window ? window->findChild<QObject *>(u"mainRouteLoader"_s) : nullptr;
        if (!surface || !surface->frameGeneration() || !host || !stats || !loader) {
            qCritical("Frame generation stats acceptance could not locate the production surface and overlay");
            m_application.exit(EXIT_FAILURE);
            return;
        }
        const QList<QVariantMap> snapshots{
            {{u"status"_s, u"warming-up"_s}, {u"outputFps"_s, 0}},
            {{u"status"_s, u"active"_s}, {u"outputFps"_s, 117}},
            {{u"status"_s, u"active"_s}, {u"outputFps"_s, 113}},
            {{u"status"_s, u"overloaded"_s}, {u"outputFps"_s, 59}},
            {{u"status"_s, u"unavailable"_s}, {u"outputFps"_s, 0}},
            {{u"status"_s, u"active"_s}, {u"outputFps"_s, 119}}
        };
        const auto callback = std::make_shared<StatsRenderCallback>();
        callback->snapshot = snapshots.first();
        surface->setRenderCallback(callback);
        const auto sampled = std::make_shared<bool>(false);
        connect(surface, &StreamVideoItem::frameGenerationStatsChanged, this,
                [sampled] { *sampled = true; });
        const auto phase = std::make_shared<qsizetype>(0);
        const QString prefix = window->property("desktopSurfaceActive").toBool()
            ? u"desktop-stream-stats"_s : u"stream-stats"_s;
        m_controller.showOverlay(prefix + u"-expanded"_s);
        auto *timer = new QTimer(this);
        timer->setInterval(50);
        connect(timer, &QTimer::timeout, this,
                [this, window, host, stats, loader, surface, callback, snapshots, sampled, phase, prefix, timer] {
            if (!*sampled) return;
            *sampled = false;
            const auto expected = snapshots.at(*phase);
            auto *route = loader->property("item").value<QObject *>();
            if (!surface || surface->renderCallback() != callback || !surface->isVisible()
                || statsFor(route) != expected || statsFor(host) != expected || statsFor(stats) != expected) {
                qCritical() << "Production frame-generation stats did not follow the render callback:"
                            << "route" << statsFor(route) << "host" << statsFor(host)
                            << "overlay" << statsFor(stats) << "expected" << expected;
                m_application.exit(EXIT_FAILURE);
                return;
            }
            QVariant report;
            const QString output = u"LOCAL OUTPUT FPS: %1 fps"_s.arg(expected.value(u"outputFps"_s).toInt());
            if (!QMetaObject::invokeMethod(host, "copyStatsSummary", Q_RETURN_ARG(QVariant, report))
                || !report.toString().contains(output)
                || !report.toString().contains(u"STREAM FPS: 60 fps"_s) || m_qmlWarningOccurred) {
                qCritical("Production frame-generation statistics report is stale or conflates source/output FPS");
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (*phase == 0) m_controller.showOverlay(prefix);
            else if (*phase == 1) m_controller.showOverlay({});
            else m_controller.showOverlay(prefix + u"-expanded"_s);
            if (*phase == 3) window->showFullScreen();
            else if (*phase == 4) window->showNormal();
            ++*phase;
            if (*phase < snapshots.size()) {
                callback->snapshot = snapshots.at(*phase);
                return;
            }
            timer->stop();
            const auto shot = m_arguments.indexOf(u"--screenshot"_s);
            if (shot >= 0 && (shot + 1 >= m_arguments.size()
                || !window->grabWindow().save(m_arguments.at(shot + 1)))) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            m_controller.showOverlay({});
            m_controller.navigate(u"home"_s);
            QTimer::singleShot(100, this, [this, host] {
                const bool passed = statsFor(host).isEmpty() && !m_qmlWarningOccurred;
                if (!passed) qCritical("Frame-generation stats survived leaving the stream route");
                m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
            });
        });
        timer->start();
    });
    QTimer::singleShot(10'000, this, [this] {
        qCritical("Production frame-generation stats acceptance timed out waiting for the surface sampler");
        m_application.exit(EXIT_FAILURE);
    });
    return EXIT_SUCCESS;
}
