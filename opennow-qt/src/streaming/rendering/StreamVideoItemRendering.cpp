#include "streaming/StreamVideoItem.h"

#include <QQuickWindow>
#include <QSGRenderNode>
#include <QScreen>
#include <rhi/qrhi.h>

namespace {
// Native conversion is recorded in prepare(), before Qt begins its scene pass.
class StreamVideoNode final : public QSGRenderNode
{
public:
    explicit StreamVideoNode(QQuickWindow *window) : m_window(window) {}
    ~StreamVideoNode() override { releaseResources(); }

    void synchronize(StreamVideoItem *item)
    {
        const auto callback = item->renderCallback();
        if (m_callback != callback) {
            releaseResources();
            m_callback = callback;
        }
        m_bounds = item->boundingRect();
        if (m_callback)
            m_callback->setFrameGeneration(item->frameGeneration() && item->isVisible(),
                m_window->screen() ? m_window->screen()->refreshRate() : 0.0);
        m_viewport = StreamVideoItem::aspectFitRect(item->videoSize(), m_bounds.size().toSize());
        m_metalFxUpscaling = item->metalFxUpscaling() && item->isVisible();
        m_fsrUpscaling = item->fsrUpscaling() && item->isVisible();
        if (m_callback) m_callback->setFsrUpscaling(m_fsrUpscaling);
        if (m_callback)
            m_callback->setUpscalingEnhancement(item->upscalingSharpness(), item->upscalingDenoise());
        markDirty(QSGNode::DirtyGeometry | QSGNode::DirtyMaterial);
    }

    void prepare() override
    {
        auto *rhi = m_window->rhi();
        if (!m_callback || !rhi || !renderTarget() || !commandBuffer()) return;
        m_callback->initialize(rhi, commandBuffer(), renderTarget());
        m_initialized = true;
        m_callback->setComposition(*projectionMatrix() * *matrix(), m_bounds, m_viewport,
                                   float(inheritedOpacity()));
        m_callback->setUpscalingTarget(m_metalFxUpscaling || m_fsrUpscaling
            ? (QSizeF(m_viewport.size()) * m_window->effectiveDevicePixelRatio()).toSize() : QSize());
        m_callback->prepareFrame(commandBuffer());
    }

    void render(const RenderState *state) override
    {
        if (!m_initialized || !commandBuffer() || !renderTarget()) return;
        auto *cb = commandBuffer();
        const auto size = renderTarget()->pixelSize();
        cb->setViewport(QRhiViewport(0, 0, size.width(), size.height()));
        const auto scissor = state->scissorEnabled()
            ? state->scissorRect() : QRect(QPoint(), size);
        cb->setScissor(QRhiScissor(scissor.x(), scissor.y(), scissor.width(), scissor.height()));
        m_callback->setClip(state->stencilEnabled(), state->stencilValue());
        m_callback->recordFrame(cb, m_viewport);
        m_callback->finishFrame();
    }

    void releaseResources() override
    {
        if (m_initialized && m_callback) m_callback->releaseResources();
        m_initialized = false;
    }

    RenderingFlags flags() const override
    {
        // Conversion brackets its own native commands in prepare(). render()
        // is entirely QRhi: Qt must not open an external/native render section
        // around these draw calls (especially with Vulkan secondary buffers).
        return BoundedRectRendering | DepthAwareRendering | NoExternalRendering;
    }
    StateFlags changedStates() const override { return ViewportState | ScissorState; }
    QRectF rect() const override { return m_bounds; }

private:
    QQuickWindow *m_window;
    std::shared_ptr<StreamVideoRenderCallback> m_callback;
    QRectF m_bounds;
    QRect m_viewport;
    bool m_initialized = false;
    bool m_metalFxUpscaling = false;
    bool m_fsrUpscaling = false;
};
}

QSGNode *StreamVideoItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    auto *node = static_cast<StreamVideoNode *>(oldNode);
    if (!node) node = new StreamVideoNode(window());
    node->synchronize(this);
    return node;
}
