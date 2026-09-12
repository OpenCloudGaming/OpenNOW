#include "input/platform/WaylandPointerCapture.h"

#include <QGuiApplication>
#include <QPlatformSurfaceEvent>
#include <QPointer>
#include <QTimer>
#include <QWindow>
#include <algorithm>
#include <cmath>
#include <utility>

#ifdef OPENNOW_WAYLAND_INPUT
#include <QtGui/qguiapplication_platform.h>
#include <qpa/qplatformnativeinterface.h>
#include <wayland-client.h>
#include "pointer-constraints-unstable-v1-client-protocol.h"
#include "relative-pointer-unstable-v1-client-protocol.h"
#endif

struct WaylandPointerCapture::Private
{
    WaylandPointerCapture *q;
    QPointer<QWindow> window;
    QRect region;
    QPointF remainder;
    QString error;
    QTimer refresh;
    bool locked = false;
    bool ready = false;
#ifdef OPENNOW_WAYLAND_INPUT
    wl_registry *registry = nullptr;
    wl_callback *sync = nullptr;
    zwp_relative_pointer_manager_v1 *relativeManager = nullptr;
    zwp_pointer_constraints_v1 *constraints = nullptr;
    zwp_relative_pointer_v1 *relative = nullptr;
    zwp_locked_pointer_v1 *lock = nullptr;
    wl_surface *surface = nullptr;
    wl_pointer *pointer = nullptr;
    uint32_t relativeName = 0;
    uint32_t constraintsName = 0;

    static QNativeInterface::QWaylandApplication *native()
    {
        return qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>();
    }

    static void global(void *data, wl_registry *registry, uint32_t name,
                       const char *interface, uint32_t)
    {
        auto *d = static_cast<Private *>(data);
        if (QByteArrayView(interface) == zwp_relative_pointer_manager_v1_interface.name
                && !d->relativeManager) {
            d->relativeName = name;
            d->relativeManager = static_cast<zwp_relative_pointer_manager_v1 *>(
                wl_registry_bind(registry, name, &zwp_relative_pointer_manager_v1_interface, 1));
        } else if (QByteArrayView(interface) == zwp_pointer_constraints_v1_interface.name
                   && !d->constraints) {
            d->constraintsName = name;
            d->constraints = static_cast<zwp_pointer_constraints_v1 *>(
                wl_registry_bind(registry, name, &zwp_pointer_constraints_v1_interface, 1));
        }
        if (d->ready) emit d->q->stateChanged();
    }

    static void globalRemoved(void *data, wl_registry *, uint32_t name)
    {
        auto *d = static_cast<Private *>(data);
        if (name != d->relativeName && name != d->constraintsName) return;
        d->clearLock();
        if (name == d->relativeName && d->relativeManager) {
            zwp_relative_pointer_manager_v1_destroy(d->relativeManager);
            d->relativeManager = nullptr;
        }
        if (name == d->constraintsName && d->constraints) {
            zwp_pointer_constraints_v1_destroy(d->constraints);
            d->constraints = nullptr;
        }
        d->setError(QStringLiteral("The Wayland compositor withdrew relative pointer or pointer lock support."));
    }

    static void synchronized(void *data, wl_callback *callback, uint32_t)
    {
        auto *d = static_cast<Private *>(data);
        wl_callback_destroy(callback);
        d->sync = nullptr;
        d->ready = true;
        emit d->q->stateChanged();
    }

    static void pointerLocked(void *data, zwp_locked_pointer_v1 *)
    {
        auto *d = static_cast<Private *>(data);
        d->locked = true;
        d->remainder = {};
        emit d->q->stateChanged();
    }

    static void pointerUnlocked(void *data, zwp_locked_pointer_v1 *)
    {
        auto *d = static_cast<Private *>(data);
        d->locked = false;
        d->remainder = {};
        emit d->q->stateChanged();
    }

    static void motion(void *data, zwp_relative_pointer_v1 *, uint32_t, uint32_t,
                       wl_fixed_t, wl_fixed_t, wl_fixed_t dx, wl_fixed_t dy)
    {
        auto *d = static_cast<Private *>(data);
        if (!d->locked || !d->window || !d->window->isActive()) return;
        const auto delta = boundedDelta(d->remainder,
            QPointF(wl_fixed_to_double(dx), wl_fixed_to_double(dy)));
        if (!delta.isNull()) emit d->q->relativeMotion(qint16(delta.x()), qint16(delta.y()));
    }
#endif

    void clearLock()
    {
#ifdef OPENNOW_WAYLAND_INPUT
        const bool hadCapture = lock || relative;
        if (lock) zwp_locked_pointer_v1_destroy(lock);
        if (relative) zwp_relative_pointer_v1_destroy(relative);
        lock = nullptr;
        relative = nullptr;
        surface = nullptr;
        pointer = nullptr;
        if (hadCapture) {
            if (auto *application = native(); application && application->display())
                wl_display_flush(application->display());
        }
#endif
        remainder = {};
        if (std::exchange(locked, false)) emit q->stateChanged();
    }

    void setError(const QString &value)
    {
        if (error == value) return;
        error = value;
        if (!error.isEmpty()) qWarning("Wayland input capture: %s", qPrintable(error));
        emit q->stateChanged();
    }
};

WaylandPointerCapture::WaylandPointerCapture(QObject *parent)
    : QObject(parent), d(std::make_unique<Private>())
{
    d->q = this;
    d->refresh.setInterval(100);
    connect(&d->refresh, &QTimer::timeout, this, [this] {
        if (!d->window || !d->window->isActive()) {
            release();
            return;
        }
        setCapture(d->window, true, d->region);
    });
#ifdef OPENNOW_WAYLAND_INPUT
    if (!isWayland()) return;
    auto *native = Private::native();
    if (!native || !native->display()) {
        d->setError(QStringLiteral("Qt did not expose its native Wayland display."));
        return;
    }
    static const wl_registry_listener registryListener{Private::global, Private::globalRemoved};
    static const wl_callback_listener syncListener{Private::synchronized};
    d->registry = wl_display_get_registry(native->display());
    wl_registry_add_listener(d->registry, &registryListener, d.get());
    d->sync = wl_display_sync(native->display());
    wl_callback_add_listener(d->sync, &syncListener, d.get());
    wl_display_flush(native->display());
#endif
}

WaylandPointerCapture::~WaylandPointerCapture()
{
    release();
#ifdef OPENNOW_WAYLAND_INPUT
    if (d->sync) wl_callback_destroy(d->sync);
    if (d->constraints) zwp_pointer_constraints_v1_destroy(d->constraints);
    if (d->relativeManager) zwp_relative_pointer_manager_v1_destroy(d->relativeManager);
    if (d->registry) wl_registry_destroy(d->registry);
#endif
}

bool WaylandPointerCapture::isWayland()
{
    return QGuiApplication::platformName().startsWith(QStringLiteral("wayland"));
}

bool WaylandPointerCapture::locked() const { return d->locked; }
QString WaylandPointerCapture::error() const { return d->error; }

QPoint WaylandPointerCapture::boundedDelta(QPointF &remainder, const QPointF &delta)
{
    if (!std::isfinite(delta.x()) || !std::isfinite(delta.y())) {
        remainder = {};
        return {};
    }
    const auto sum = remainder + delta;
    const QPoint result(int(std::clamp(sum.x(), -32768.0, 32767.0)),
                        int(std::clamp(sum.y(), -32768.0, 32767.0)));
    remainder = QPointF(std::abs(sum.x()) < 32767 ? sum.x() - result.x() : 0,
                        std::abs(sum.y()) < 32767 ? sum.y() - result.y() : 0);
    return result;
}

void WaylandPointerCapture::setCapture(QWindow *window, bool enabled, const QRect &surfaceRegion)
{
    if (!enabled || !window || !window->isVisible() || !window->isActive() || surfaceRegion.isEmpty()) {
        release();
        return;
    }
    if (!isWayland()) return;
    if (d->window != window) {
        release();
        d->window = window;
        window->installEventFilter(this);
    }
    const bool regionChanged = d->region != surfaceRegion;
    d->region = surfaceRegion;
    if (!d->refresh.isActive()) d->refresh.start();
#ifdef OPENNOW_WAYLAND_INPUT
    if (!d->ready) return;
    if (!d->relativeManager || !d->constraints) {
        d->clearLock();
        d->refresh.stop();
        d->setError(QStringLiteral("Relative mouse input requires Wayland relative-pointer-v1 and pointer-constraints-v1; the compositor does not provide both protocols."));
        return;
    }
    auto *native = Private::native();
    auto *surface = static_cast<wl_surface *>(QGuiApplication::platformNativeInterface()
        ->nativeResourceForWindow("surface", window));
    auto *pointer = native ? native->pointer() : nullptr;
    if (!surface || !pointer || !native->compositor()) {
        d->clearLock();
        d->setError(QStringLiteral("The Qt Wayland window has no live surface or pointer device."));
        return;
    }
    if (d->surface != surface || d->pointer != pointer) d->clearLock();
    if (d->lock && !regionChanged) return;
    d->setError({});
    auto *region = wl_compositor_create_region(native->compositor());
    wl_region_add(region, surfaceRegion.x(), surfaceRegion.y(),
                  surfaceRegion.width(), surfaceRegion.height());
    if (!d->lock) {
        d->surface = surface;
        d->pointer = pointer;
        static const zwp_relative_pointer_v1_listener motionListener{Private::motion};
        static const zwp_locked_pointer_v1_listener lockListener{
            Private::pointerLocked, Private::pointerUnlocked};
        d->relative = zwp_relative_pointer_manager_v1_get_relative_pointer(d->relativeManager, pointer);
        zwp_relative_pointer_v1_add_listener(d->relative, &motionListener, d.get());
        d->lock = zwp_pointer_constraints_v1_lock_pointer(d->constraints, surface, pointer,
            region, ZWP_POINTER_CONSTRAINTS_V1_LIFETIME_PERSISTENT);
        zwp_locked_pointer_v1_add_listener(d->lock, &lockListener, d.get());
    } else {
        zwp_locked_pointer_v1_set_region(d->lock, region);
    }
    wl_region_destroy(region);
    window->requestUpdate();
    wl_display_flush(native->display());
#else
    d->refresh.stop();
    d->setError(QStringLiteral("This build has no native Wayland relative mouse backend."));
#endif
}

void WaylandPointerCapture::release()
{
    d->refresh.stop();
    d->clearLock();
    if (d->window) d->window->removeEventFilter(this);
    d->window.clear();
    d->region = {};
}

bool WaylandPointerCapture::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == d->window) {
        if (event->type() == QEvent::FocusOut || event->type() == QEvent::Hide
                || event->type() == QEvent::Close)
            release();
        else if (event->type() == QEvent::PlatformSurface
                 && static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType()
                     == QPlatformSurfaceEvent::SurfaceAboutToBeDestroyed)
            d->clearLock();
    }
    return QObject::eventFilter(watched, event);
}
