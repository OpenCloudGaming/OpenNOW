#include "streaming/rendering/MetalHdrOutput.h"

#include <QThread>
#include <QWindow>
#include <rhi/qrhi.h>

#import <AppKit/NSView.h>
#import <QuartzCore/CAMetalLayer.h>

bool resetMetalSdrOutput(QWindow *window, const QRhiSwapChainProxyData &proxy)
{
    auto *layer = static_cast<CAMetalLayer *>(proxy.reserved[0]);
    if (!layer && window && QThread::currentThread() == window->thread()) {
        auto *view = reinterpret_cast<NSView *>(window->winId());
        layer = static_cast<CAMetalLayer *>(view.layer);
    }
    if (![layer isKindOfClass:[CAMetalLayer class]]) return false;
    const auto colorSpace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    if (!colorSpace) return false;
    layer.colorspace = colorSpace;
    layer.wantsExtendedDynamicRangeContent = NO;
    CGColorSpaceRelease(colorSpace);
    return true;
}
