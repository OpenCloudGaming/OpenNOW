#include "streaming/rendering/MetalHdrOutput.h"

#include <QTest>
#include <rhi/qrhi.h>

#import <QuartzCore/CAMetalLayer.h>

void verifyMetalSdrLayerRecovery()
{
    @autoreleasepool {
        auto *layer = [CAMetalLayer layer];
        layer.pixelFormat = MTLPixelFormatRGBA16Float;
        QRhiSwapChainProxyData proxy;
        proxy.reserved[0] = layer;
        const auto hdrSpace = CGColorSpaceCreateWithName(kCGColorSpaceExtendedLinearSRGB);
        const auto sdrSpace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        QVERIFY(hdrSpace);
        QVERIFY(sdrSpace);
        for (int recreation = 0; recreation < 3; ++recreation) {
            layer.colorspace = hdrSpace;
            layer.wantsExtendedDynamicRangeContent = YES;
            QVERIFY(layer.wantsExtendedDynamicRangeContent);
            QVERIFY(CFEqual(layer.colorspace, hdrSpace));
            QVERIFY(resetMetalSdrOutput(nullptr, proxy));
            QVERIFY(!layer.wantsExtendedDynamicRangeContent);
            QVERIFY(CFEqual(layer.colorspace, sdrSpace));
        }
        CGColorSpaceRelease(hdrSpace);
        CGColorSpaceRelease(sdrSpace);
        QVERIFY(!resetMetalSdrOutput(nullptr, {}));
    }
}
