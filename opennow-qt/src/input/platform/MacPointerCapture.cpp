#include "input/platform/MacPointerCapture.h"

#include <QGuiApplication>
#include <QPlatformSurfaceEvent>
#include <QPointer>
#include <QScreen>
#include <QSignalBlocker>
#include <QThread>
#include <QWindow>
#include <algorithm>
#include <cmath>

#ifdef Q_OS_MACOS
std::unique_ptr<MacPointerCapture::NativeOperations> makeMacPointerOperations();
#endif

namespace {
QPointer<MacPointerCapture> captureOwner;
}

struct MacPointerCapture::Private
{
    std::unique_ptr<NativeOperations> operations;
    QPointer<QWindow> window;
    QPointer<QWindow> failedWindow;
    QPointer<QScreen> failedScreen;
    QRect failedRegion;
    QRect failedGeometry;
    qreal failedDpr = 0;
    QMetaObject::Connection destroyed;
    QRect region;
    QPointF remainder;
    QString error;
    bool locked = false;
    bool disassociated = false;
    bool hidden = false;
    quint64 generation = 0;
};

MacPointerCapture::MacPointerCapture(QObject *parent)
    : MacPointerCapture(
#ifdef Q_OS_MACOS
          isSupported() ? makeMacPointerOperations() : nullptr,
#else
          nullptr,
#endif
          parent)
{
}

MacPointerCapture::MacPointerCapture(std::unique_ptr<NativeOperations> operations, QObject *parent)
    : QObject(parent), d(std::make_unique<Private>())
{
    Q_ASSERT(qApp && QThread::currentThread() == qApp->thread());
    d->operations = std::move(operations);
    connect(qApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
        if (state != Qt::ApplicationActive) release();
    });
    connect(qApp, &QCoreApplication::aboutToQuit, this, &MacPointerCapture::release);
}

MacPointerCapture::~MacPointerCapture()
{
    const QSignalBlocker blocker(this);
    release();
    if (d->disassociated || d->hidden) release();
}

bool MacPointerCapture::isSupported()
{
#ifdef Q_OS_MACOS
    return QGuiApplication::platformName() == QStringLiteral("cocoa");
#else
    return false;
#endif
}

bool MacPointerCapture::locked() const { return d->locked; }
QString MacPointerCapture::error() const { return d->error; }

void MacPointerCapture::fail(const QString &error)
{
    const auto previousError = d->error;
    const bool wasLocked = d->locked;
    const QPointer<QWindow> window = d->window;
    const QRect region = d->region;
    {
        const QSignalBlocker blocker(this);
        d->error.clear();
        release();
    }
    if (window) {
        d->failedWindow = window;
        d->failedRegion = region;
        d->failedGeometry = window->geometry();
        d->failedScreen = window->screen();
        d->failedDpr = window->devicePixelRatio();
    }
    d->error = d->error.isEmpty() ? error : error + QStringLiteral("; ") + d->error;
    if (d->error != previousError)
        qWarning("macOS pointer capture: %s", qPrintable(d->error));
    if (wasLocked || d->error != previousError) emit stateChanged();
}

void MacPointerCapture::setCapture(QWindow *window, bool enabled, const QRect &windowRegion)
{
    Q_ASSERT(QThread::currentThread() == qApp->thread());
    if (!enabled || !window || !window->isVisible() || !window->isActive()
        || windowRegion.intersected(QRect(QPoint(), window->size())).isEmpty()) {
        release();
        return;
    }
    if (!d->operations) return;
    if (d->failedWindow == window && d->failedRegion == windowRegion
        && d->failedGeometry == window->geometry() && d->failedScreen == window->screen()
        && d->failedDpr == window->devicePixelRatio()) return;
    if (captureOwner && captureOwner != this) {
        d->window = window;
        d->region = windowRegion;
        fail(QStringLiteral("Another macOS pointer capture owner is active."));
        return;
    }
    if (d->locked && d->window == window) {
        if (d->region == windowRegion) return;
        d->region = windowRegion;
        const auto error = d->operations->center(window, windowRegion);
        if (!error.isEmpty()) fail(error);
        return;
    }
    const QPointer<MacPointerCapture> guard(this);
    release();
    if (!guard) return;
    if (d->disassociated || d->hidden) return;
    captureOwner = this;
    d->window = window;
    d->region = windowRegion;
    window->installEventFilter(this);
    d->destroyed = connect(window, &QObject::destroyed, this, &MacPointerCapture::release);
    auto error = d->operations->associate(false);
    if (!error.isEmpty()) {
        fail(error);
        return;
    }
    d->disassociated = true;
    error = d->operations->center(window, windowRegion);
    if (!error.isEmpty()) {
        fail(error);
        return;
    }
    error = d->operations->setHidden(true);
    if (!error.isEmpty()) {
        fail(error);
        return;
    }
    d->hidden = true;
    error = d->operations->startMotion(window, [this](QPointF delta) { motion(delta); });
    if (!error.isEmpty()) {
        fail(error);
        return;
    }
    d->error.clear();
    d->locked = true;
    emit stateChanged();
}

void MacPointerCapture::release()
{
    Q_ASSERT(QThread::currentThread() == qApp->thread());
    const bool wasLocked = d->locked;
    const auto previousError = d->error;
    ++d->generation;
    d->failedWindow.clear();
    d->locked = false;
    if (d->operations) d->operations->stopMotion();
    QStringList errors;
    if (d->disassociated) {
        const auto error = d->operations->associate(true);
        if (error.isEmpty()) d->disassociated = false;
        else errors.append(error);
    }
    if (d->hidden) {
        const auto error = d->operations->setHidden(false);
        if (error.isEmpty()) d->hidden = false;
        else errors.append(error);
    }
    if (!d->disassociated && !d->hidden && captureOwner == this) captureOwner.clear();
    if (d->window) d->window->removeEventFilter(this);
    disconnect(d->destroyed);
    d->window.clear();
    d->region = {};
    d->remainder = {};
    if (!errors.isEmpty()) {
        d->error = errors.join(QStringLiteral("; "));
        qWarning("macOS pointer capture restore: %s", qPrintable(d->error));
    }
    if (wasLocked || d->error != previousError) emit stateChanged();
}

void MacPointerCapture::motion(const QPointF &delta)
{
    Q_ASSERT(QThread::currentThread() == qApp->thread());
    if (!d->locked) return;
    constexpr double maximumDelta = 1048576;
    if (!std::isfinite(delta.x()) || !std::isfinite(delta.y())
        || std::abs(delta.x()) > maximumDelta || std::abs(delta.y()) > maximumDelta) {
        fail(QStringLiteral("The native mouse event contained an invalid relative delta."));
        return;
    }
    const QPointF sum = d->remainder + delta;
    int x = int(sum.x());
    int y = int(sum.y());
    d->remainder = sum - QPointF(x, y);
    const QPointer<MacPointerCapture> guard(this);
    const auto generation = d->generation;
    while (x != 0 || y != 0) {
        const int partX = std::clamp(x, -32768, 32767);
        const int partY = std::clamp(y, -32768, 32767);
        x -= partX;
        y -= partY;
        emit relativeMotion(qint16(partX), qint16(partY));
        if (!guard || !d->locked || d->generation != generation) return;
    }
}

bool MacPointerCapture::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == d->window) {
        switch (event->type()) {
        case QEvent::FocusOut:
        case QEvent::WindowDeactivate:
        case QEvent::Hide:
        case QEvent::Close:
            release();
            break;
        case QEvent::PlatformSurface:
            if (static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType()
                == QPlatformSurfaceEvent::SurfaceAboutToBeDestroyed) release();
            break;
        case QEvent::Move:
        case QEvent::Resize:
        case QEvent::ScreenChangeInternal:
        case QEvent::DevicePixelRatioChange:
            if (d->locked) {
                const auto error = d->operations->center(d->window, d->region);
                if (!error.isEmpty()) fail(error);
            }
            break;
        default:
            break;
        }
    }
    return false;
}
