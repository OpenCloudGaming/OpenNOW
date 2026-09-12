#include "streaming/rendering/WaylandHdrOutput.h"

#include <QGuiApplication>
#include <QMutex>
#include <QMutexLocker>
#include <QPlatformSurfaceEvent>
#include <QPointer>
#include <QScreen>
#include <QThread>
#include <QTimer>
#include <QWindow>
#include <cmath>
#include <vector>

#ifdef OPENNOW_WAYLAND_HDR
#include <QtGui/qguiapplication_platform.h>
#include <qpa/qplatformnativeinterface.h>
#include <unistd.h>
#include <wayland-client.h>
#include "color-management-v1-client-protocol.h"
#endif

struct WaylandHdrOutput::Private
{
    WaylandHdrOutput *q = nullptr;
    QPointer<QWindow> window;
    std::vector<QMetaObject::Connection> windowConnections;
    std::vector<QMetaObject::Connection> screenConnections;
    mutable QMutex mutex;
    State snapshot;
    quint64 generation = 0;
    bool queued = false;
    bool surfaceAlive = false;

    void publish(State value = {})
    {
        {
            QMutexLocker lock(&mutex);
            if (snapshot.supported == value.supported && snapshot.whiteNits == value.whiteNits)
                return;
            snapshot = value;
        }
        emit q->changed();
    }

    void invalidate()
    {
        ++generation;
        publish();
        schedule();
    }

    void schedule()
    {
        if (queued) return;
        queued = true;
        QTimer::singleShot(0, q, [this] {
            queued = false;
            refresh();
        });
    }

    QScreen *eligibleScreen() const
    {
        if (!window || !surfaceAlive || !window->isVisible()) return nullptr;
        auto *screen = window->screen();
        const QRect frame = window->frameGeometry();
        if (!screen || frame.isEmpty() || !screen->geometry().contains(frame)) return nullptr;
        const auto screens = QGuiApplication::screens();
        if (screens.size() != 1 || !screens.contains(screen)) return nullptr;
        return screen;
    }

    void watchScreens()
    {
        for (const auto &connection : screenConnections) QObject::disconnect(connection);
        screenConnections.clear();
        for (auto *screen : QGuiApplication::screens()) {
            screenConnections.push_back(QObject::connect(screen, &QScreen::geometryChanged,
                q, [this] { invalidate(); }));
            screenConnections.push_back(QObject::connect(screen, &QScreen::physicalDotsPerInchChanged,
                q, [this] { invalidate(); }));
        }
        invalidate();
    }

#ifdef OPENNOW_WAYLAND_HDR
    wl_registry *registry = nullptr;
    wl_callback *sync = nullptr;
    wp_color_manager_v1 *manager = nullptr;
    wp_color_management_output_v1 *output = nullptr;
    wl_output *nativeOutput = nullptr;
    QPointer<QScreen> outputScreen;
    uint32_t managerName = 0;
    bool registryReady = false;
    bool managerReady = false;
    wp_image_description_v1 *description = nullptr;
    wp_image_description_info_v1 *information = nullptr;
    QTimer requestTimeout;
    quint64 requestGeneration = 0;
    Description metadata;

    static wl_display *display()
    {
        auto *native = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>();
        return native ? native->display() : nullptr;
    }

    static wl_output *screenOutput(QScreen *screen)
    {
        auto *native = QGuiApplication::platformNativeInterface();
        return native && screen
            ? static_cast<wl_output *>(native->nativeResourceForScreen("output", screen)) : nullptr;
    }

    void clearOutput()
    {
        if (output) wp_color_management_output_v1_destroy(output);
        output = nullptr;
        nativeOutput = nullptr;
        outputScreen = nullptr;
    }

    void clearRequest()
    {
        requestTimeout.stop();
        if (information) wp_image_description_info_v1_destroy(information);
        if (description) wp_image_description_v1_destroy(description);
        information = nullptr;
        description = nullptr;
    }

    void finishRequest(bool complete)
    {
        const bool current = requestGeneration == generation;
        metadata.complete = complete;
        const bool validOutput = current && output && eligibleScreen() == outputScreen
            && screenOutput(outputScreen) == nativeOutput;
        const State result = validOutput ? stateForDescription(metadata) : State{};
        clearRequest();
        if (current) publish(result);
        else schedule();
    }

    static void informationDone(void *data, wp_image_description_info_v1 *)
    {
        static_cast<Private *>(data)->finishRequest(true);
    }

    static void descriptionFailed(void *data, wp_image_description_v1 *, uint32_t, const char *)
    {
        static_cast<Private *>(data)->finishRequest(false);
    }

    static void descriptionReady(void *data, wp_image_description_v1 *, uint32_t)
    {
        auto *d = static_cast<Private *>(data);
        if (d->requestGeneration != d->generation) {
            d->finishRequest(false);
            return;
        }
        d->metadata.ready = true;
        static const wp_image_description_info_v1_listener listener{
            informationDone,
            [](void *data, wp_image_description_info_v1 *, int32_t fd, uint32_t) {
                close(fd);
                static_cast<Private *>(data)->metadata.icc = true;
            },
            [](void *data, wp_image_description_info_v1 *, int32_t, int32_t, int32_t, int32_t,
               int32_t, int32_t, int32_t, int32_t) { static_cast<Private *>(data)->metadata.primaries = true; },
            [](void *, wp_image_description_info_v1 *, uint32_t) {},
            [](void *data, wp_image_description_info_v1 *, uint32_t) {
                static_cast<Private *>(data)->metadata.power = true;
            },
            [](void *data, wp_image_description_info_v1 *, uint32_t tf) {
                static_cast<Private *>(data)->metadata.pq = tf == WP_COLOR_MANAGER_V1_TRANSFER_FUNCTION_ST2084_PQ;
            },
            [](void *data, wp_image_description_info_v1 *, uint32_t min, uint32_t max, uint32_t ref) {
                auto *d = static_cast<Private *>(data);
                d->metadata.luminances = true;
                d->metadata.minimum = double(min) / 10000.0;
                d->metadata.maximum = max;
                d->metadata.white = ref;
            },
            [](void *, wp_image_description_info_v1 *, int32_t, int32_t, int32_t, int32_t,
               int32_t, int32_t, int32_t, int32_t) {},
            [](void *data, wp_image_description_info_v1 *, uint32_t min, uint32_t max) {
                auto *d = static_cast<Private *>(data);
                d->metadata.targetLuminance = true;
                d->metadata.targetMinimum = double(min) / 10000.0;
                d->metadata.targetMaximum = max;
            },
            [](void *, wp_image_description_info_v1 *, uint32_t) {},
            [](void *, wp_image_description_info_v1 *, uint32_t) {}
        };
        d->information = wp_image_description_v1_get_information(d->description);
        wp_image_description_info_v1_add_listener(d->information, &listener, d);
        wl_display_flush(display());
    }

    static void global(void *data, wl_registry *registry, uint32_t name,
                       const char *interface, uint32_t)
    {
        auto *d = static_cast<Private *>(data);
        if (QByteArrayView(interface) != wp_color_manager_v1_interface.name || d->manager) return;
        static const wp_color_manager_v1_listener listener{
            [](void *, wp_color_manager_v1 *, uint32_t) {},
            [](void *, wp_color_manager_v1 *, uint32_t) {},
            [](void *, wp_color_manager_v1 *, uint32_t) {},
            [](void *, wp_color_manager_v1 *, uint32_t) {},
            [](void *data, wp_color_manager_v1 *) {
                auto *d = static_cast<Private *>(data);
                d->managerReady = true;
                d->invalidate();
            }
        };
        d->managerName = name;
        d->manager = static_cast<wp_color_manager_v1 *>(
            wl_registry_bind(registry, name, &wp_color_manager_v1_interface, 1));
        wp_color_manager_v1_add_listener(d->manager, &listener, d);
        d->invalidate();
    }

    static void globalRemoved(void *data, wl_registry *, uint32_t name)
    {
        auto *d = static_cast<Private *>(data);
        d->invalidate();
        d->clearOutput();
        if (name == d->managerName && d->manager) {
            wp_color_manager_v1_destroy(d->manager);
            d->manager = nullptr;
            d->managerName = 0;
            d->managerReady = false;
        }
    }
#endif

    void refresh()
    {
#ifdef OPENNOW_WAYLAND_HDR
        if (!registryReady || !managerReady || !manager) {
            clearOutput();
            publish();
            return;
        }
        auto *screen = eligibleScreen();
        auto *native = screenOutput(screen);
        if (!native) {
            clearOutput();
            publish();
            return;
        }
        if (outputScreen != screen || nativeOutput != native) {
            clearOutput();
            outputScreen = screen;
            nativeOutput = native;
            static const wp_color_management_output_v1_listener listener{
                [](void *data, wp_color_management_output_v1 *) {
                    static_cast<Private *>(data)->invalidate();
                }
            };
            output = wp_color_manager_v1_get_output(manager, native);
            wp_color_management_output_v1_add_listener(output, &listener, this);
        }
        if (description) return;
        requestGeneration = generation;
        metadata = {};
        static const wp_image_description_v1_listener listener{descriptionFailed, descriptionReady};
        description = wp_color_management_output_v1_get_image_description(output);
        wp_image_description_v1_add_listener(description, &listener, this);
        requestTimeout.start();
        wl_display_flush(display());
#endif
    }
};

WaylandHdrOutput::WaylandHdrOutput(QObject *parent)
    : QObject(parent), d(std::make_unique<Private>())
{
    Q_ASSERT(QThread::currentThread() == qGuiApp->thread());
    d->q = this;
    connect(qGuiApp, &QGuiApplication::screenAdded, this, [this] { d->watchScreens(); });
    connect(qGuiApp, &QGuiApplication::screenRemoved, this, [this] { d->watchScreens(); });
    d->watchScreens();
#ifdef OPENNOW_WAYLAND_HDR
    if (!QGuiApplication::platformName().startsWith(QStringLiteral("wayland"))) return;
    d->requestTimeout.setSingleShot(true);
    d->requestTimeout.setInterval(2500);
    connect(&d->requestTimeout, &QTimer::timeout, this, [this] {
        d->finishRequest(false);
        d->schedule();
    });
    auto *display = Private::display();
    if (!display) return;
    static const wl_registry_listener registryListener{Private::global, Private::globalRemoved};
    static const wl_callback_listener syncListener{
        [](void *data, wl_callback *callback, uint32_t) {
            auto *d = static_cast<Private *>(data);
            wl_callback_destroy(callback);
            d->sync = nullptr;
            d->registryReady = true;
            d->schedule();
        }
    };
    d->registry = wl_display_get_registry(display);
    wl_registry_add_listener(d->registry, &registryListener, d.get());
    d->sync = wl_display_sync(display);
    wl_callback_add_listener(d->sync, &syncListener, d.get());
    wl_display_flush(display);
#endif
}

WaylandHdrOutput::~WaylandHdrOutput()
{
    Q_ASSERT(QThread::currentThread() == thread());
    if (d->window) d->window->removeEventFilter(this);
#ifdef OPENNOW_WAYLAND_HDR
    d->clearRequest();
    d->clearOutput();
    if (d->sync) wl_callback_destroy(d->sync);
    if (d->manager) wp_color_manager_v1_destroy(d->manager);
    if (d->registry) wl_registry_destroy(d->registry);
#endif
}

void WaylandHdrOutput::attach(QWindow *window)
{
    Q_ASSERT(QThread::currentThread() == thread());
    if (d->window == window) return;
    if (d->window) d->window->removeEventFilter(this);
    for (const auto &connection : d->windowConnections) disconnect(connection);
    d->windowConnections.clear();
    d->window = window;
    d->surfaceAlive = window && window->handle();
    if (window) {
        window->installEventFilter(this);
        d->windowConnections.push_back(connect(window, &QWindow::screenChanged,
            this, [this] { d->invalidate(); }));
        d->windowConnections.push_back(connect(window, &QWindow::visibleChanged,
            this, [this] { d->invalidate(); }));
        d->windowConnections.push_back(connect(window, &QObject::destroyed, this, [this] {
            d->surfaceAlive = false;
            d->invalidate();
        }));
    }
    d->invalidate();
}

WaylandHdrOutput::State WaylandHdrOutput::state() const
{
    QMutexLocker lock(&d->mutex);
    return d->snapshot;
}

WaylandHdrOutput::State WaylandHdrOutput::stateForDescription(const Description &value)
{
    if (!value.ready || !value.complete || !value.primaries || !value.pq || value.power || value.icc
        || !value.luminances || !value.targetLuminance || !std::isfinite(value.white)
        || value.white < 80.0 || value.white > 500.0 || !std::isfinite(value.minimum)
        || value.minimum < 0.0 || value.minimum >= value.white || !std::isfinite(value.maximum)
        || value.maximum < value.white || value.maximum > 10000.0
        || !std::isfinite(value.targetMinimum) || value.targetMinimum < 0.0
        || value.targetMinimum >= value.white || !std::isfinite(value.targetMaximum)
        || value.targetMaximum <= value.white || value.targetMaximum > 10000.0)
        return {};
    return {true, float(value.white)};
}

bool WaylandHdrOutput::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == d->window) {
        switch (event->type()) {
        case QEvent::PlatformSurface:
            d->surfaceAlive = static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType()
                == QPlatformSurfaceEvent::SurfaceCreated;
            d->invalidate();
            break;
        case QEvent::Move:
        case QEvent::Resize:
        case QEvent::Show:
        case QEvent::Hide:
        case QEvent::WindowStateChange:
        case QEvent::DevicePixelRatioChange:
            d->invalidate();
            break;
        default:
            break;
        }
    }
    return QObject::eventFilter(watched, event);
}
