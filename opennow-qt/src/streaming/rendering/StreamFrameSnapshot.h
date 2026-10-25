#pragma once

#include "streaming/rendering/StreamVideoTextureRenderer.h"

#include <QImage>
#include <functional>
#include <utility>

class StreamFrameSnapshot final
{
public:
    ~StreamFrameSnapshot()
    {
        cancel();
    }

    void cancel()
    {
        m_completion.reset();
    }

    bool submit(QRhi *rhi, QRhiCommandBuffer *cb, QRhiTexture *source, int colorSpace,
                std::function<void(QImage)> complete)
    {
        if (!source || m_texture) return false;
        auto size = source->pixelSize();
        if (size.isEmpty()) return false;
        if (size.width() > 3840 || size.height() > 2160)
            size.scale(3840, 2160, Qt::KeepAspectRatio);
        m_texture.reset(rhi->newTexture(QRhiTexture::RGBA8, size, 1,
            QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
        if (!m_texture->create()) return false;
        m_target.reset(rhi->newTextureRenderTarget(
            QRhiTextureRenderTargetDescription(QRhiColorAttachment(m_texture.get()))));
        m_pass.reset(m_target->newCompatibleRenderPassDescriptor());
        m_target->setRenderPassDescriptor(m_pass.get());
        if (!m_target->create()) return false;
        m_renderer.initialize(rhi, m_target.get());
        auto matrix = rhi->clipSpaceCorrMatrix();
        matrix.ortho(0, size.width(), size.height(), 0, -1, 1);
        const QRect rect(QPoint(), size);
        m_renderer.setComposition(matrix, rect, rect, 1.0f);
        m_renderer.setColorSpace(colorSpace, 0, 203.0f, false);
        if (!m_renderer.prepare(cb)
            || !m_renderer.importFrame(0, source->nativeTexture(), source->format(),
                                       source->pixelSize())) return false;
        cb->beginPass(m_target.get(), Qt::black, {1.0f, 0});
        cb->setViewport(QRhiViewport(0, 0, size.width(), size.height()));
        cb->setScissor({0, 0, size.width(), size.height()});
        m_renderer.render(cb, false, 0);
        cb->endPass();
        m_completion = std::make_shared<std::function<void(QImage)>>(std::move(complete));
        auto *result = new QRhiReadbackResult;
        result->completed = [result, completion = std::weak_ptr(m_completion),
                             flip = rhi->isYUpInFramebuffer()] {
            const std::unique_ptr<QRhiReadbackResult> ownedResult(result);
            const auto state = completion.lock();
            if (!state || !*state) return;
            auto complete = std::exchange(*state, {});
            QImage image;
            const auto size = result->pixelSize;
            if (result->format == QRhiTexture::RGBA8 && size.width() > 0
                && size.height() > 0 && size.width() <= 3840 && size.height() <= 2160
                && result->data.size() == qsizetype(size.width()) * size.height() * 4) {
                const QImage view(reinterpret_cast<const uchar *>(result->data.constData()),
                    size.width(), size.height(), size.width() * 4, QImage::Format_RGBA8888);
                image = flip ? view.mirrored() : view.copy();
            }
            complete(std::move(image));
        };
        auto *updates = rhi->nextResourceUpdateBatch();
        updates->readBackTexture(QRhiReadbackDescription(m_texture.get()), result);
        cb->resourceUpdate(updates);
        return true;
    }

private:
    std::unique_ptr<QRhiTexture> m_texture;
    std::unique_ptr<QRhiRenderPassDescriptor> m_pass;
    std::unique_ptr<QRhiTextureRenderTarget> m_target;
    StreamVideoTextureRenderer m_renderer;
    std::shared_ptr<std::function<void(QImage)>> m_completion;
};
