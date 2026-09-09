#include "input/platform/MacPointerCapture.h"

#include <QWindow>
#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>

namespace {
QString cgError(const char *operation, CGError error)
{
    return error == kCGErrorSuccess ? QString()
        : QStringLiteral("%1 failed with CoreGraphics error %2.")
              .arg(QString::fromLatin1(operation)).arg(int(error));
}

class CocoaPointerOperations final : public MacPointerCapture::NativeOperations
{
public:
    ~CocoaPointerOperations() override { stopMotion(); }

    QString associate(bool associated) override
    {
        return cgError("CGAssociateMouseAndMouseCursorPosition",
                       CGAssociateMouseAndMouseCursorPosition(associated));
    }

    QString center(QWindow *window, const QRect &windowRegion) override
    {
        NSView *view = reinterpret_cast<NSView *>(window->winId());
        NSWindow *nativeWindow = view.window;
        if (!view || !nativeWindow || !nativeWindow.isKeyWindow || !NSApp.isActive)
            return QStringLiteral("The Qt Cocoa window is not an active native window.");
        const QRect region = windowRegion.intersected(QRect(QPoint(), window->size()));
        if (region.isEmpty()) return QStringLiteral("The pointer capture region is empty.");
        const QPointF center = QRectF(region).center();
        const NSRect bounds = view.bounds;
        const NSPoint local = NSMakePoint(NSMinX(bounds) + center.x(),
            view.isFlipped ? NSMinY(bounds) + center.y() : NSMaxY(bounds) - center.y());
        const NSPoint screen = [nativeWindow convertPointToScreen:[view convertPoint:local toView:nil]];
        const CGPoint quartz = CGPointMake(screen.x, CGRectGetHeight(CGDisplayBounds(CGMainDisplayID())) - screen.y);
        return cgError("CGWarpMouseCursorPosition", CGWarpMouseCursorPosition(quartz));
    }

    QString setHidden(bool hidden) override
    {
        return hidden ? cgError("CGDisplayHideCursor", CGDisplayHideCursor(kCGDirectMainDisplay))
                      : cgError("CGDisplayShowCursor", CGDisplayShowCursor(kCGDirectMainDisplay));
    }

    QString startMotion(QWindow *window, std::function<void(QPointF)> callback) override
    {
        stopMotion();
        NSView *view = reinterpret_cast<NSView *>(window->winId());
        NSWindow *nativeWindow = view.window;
        if (!nativeWindow) return QStringLiteral("The Qt Cocoa window has no native surface.");
        const NSEventMask mask = NSEventMaskMouseMoved | NSEventMaskLeftMouseDragged
            | NSEventMaskRightMouseDragged | NSEventMaskOtherMouseDragged;
        monitor = [NSEvent addLocalMonitorForEventsMatchingMask:mask handler:^NSEvent *(NSEvent *event) {
            if (event.window != nativeWindow || !nativeWindow.isKeyWindow || !NSApp.isActive)
                return event;
            const auto deliver = callback;
            deliver(QPointF(event.deltaX, event.deltaY));
            return nil;
        }];
        return monitor ? QString() : QStringLiteral("AppKit could not install the local mouse event monitor.");
    }

    void stopMotion() override
    {
        if (monitor) [NSEvent removeMonitor:monitor];
        monitor = nil;
    }

private:
    id monitor = nil;
};
}

std::unique_ptr<MacPointerCapture::NativeOperations> makeMacPointerOperations()
{
    return std::make_unique<CocoaPointerOperations>();
}
