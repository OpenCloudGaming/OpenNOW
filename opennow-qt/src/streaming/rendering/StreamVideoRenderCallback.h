#pragma once

#include <QMatrix4x4>
#include <QRect>
#include <QVariantMap>

class QRhi;
class QRhiCommandBuffer;
class QRhiRenderTarget;

class StreamVideoRenderCallback
{
public:
    virtual ~StreamVideoRenderCallback() = default;

    virtual void initialize(QRhi *rhi,
                            QRhiCommandBuffer *commandBuffer,
                            QRhiRenderTarget *renderTarget) = 0;
    virtual void prepareFrame(QRhiCommandBuffer *commandBuffer) = 0;
    virtual void setComposition(const QMatrix4x4 &, const QRectF &, const QRectF &, float) {}
    virtual void setClip(bool, int) {}
    virtual void setFrameGeneration(bool, double) {}
    virtual void setUpscalingTarget(const QSize &) {}
    virtual void setUpscalingEnhancement(int, int) {}
    virtual bool needsFrame() const { return false; }
    virtual void frameSwapped() {}
    virtual QVariantMap frameGenerationStats() const { return {}; }
    virtual void recordFrame(QRhiCommandBuffer *commandBuffer,
                             const QRect &videoViewport) = 0;
    virtual void finishFrame() = 0;
    virtual void releaseResources() = 0;
};
