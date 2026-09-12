#pragma once

#include <QFile>
#include <rhi/qrhi.h>
#include <memory>

class HdrOutputPass
{
public:
    bool matches(QRhi *rhi, QRhiSwapChain *sc) const
    {
        return matches(rhi, sc->currentFrameRenderTarget());
    }

    bool initialize(QRhi *rhi, QRhiSwapChain *sc)
    {
        return initialize(rhi, sc->currentFrameRenderTarget());
    }

    bool initialize(QRhi *rhi, QRhiRenderTarget *output)
    {
        if (matches(rhi, output)) return true;
        m_ready = false;
        const auto size = output->pixelSize();
        if (size.isEmpty() || qint64(size.width()) * size.height() > 7680LL * 4320
                || !rhi->isTextureFormatSupported(QRhiTexture::RGBA16F, QRhiTexture::RenderTarget))
            return false;
        m_rhi = rhi;
        m_samples = output->sampleCount();
        m_outputFormat = output->renderPassDescriptor()->serializedFormat();
        m_pipeline.reset();
        m_bindings.reset();
        m_target.reset();
        m_pass.reset();
        m_texture.reset(rhi->newTexture(QRhiTexture::RGBA16F, size, 1, QRhiTexture::RenderTarget));
        if (!m_texture->create()) return false;
        QRhiColorAttachment color(m_texture.get());
        if (m_samples > 1) {
            m_multisample.reset(rhi->newTexture(QRhiTexture::RGBA16F, size, m_samples, QRhiTexture::RenderTarget));
            if (!m_multisample->create()) return false;
            color.setTexture(m_multisample.get());
            color.setResolveTexture(m_texture.get());
        }
        m_depth.reset(rhi->newRenderBuffer(QRhiRenderBuffer::DepthStencil, size, m_samples));
        if (!m_depth->create()) return false;
        QRhiTextureRenderTargetDescription description(color);
        description.setDepthStencilBuffer(m_depth.get());
        m_target.reset(rhi->newTextureRenderTarget(description));
        m_pass.reset(m_target->newCompatibleRenderPassDescriptor());
        m_target->setRenderPassDescriptor(m_pass.get());
        if (!m_target->create()) return false;
        m_sampler.reset(rhi->newSampler(QRhiSampler::Nearest, QRhiSampler::Nearest, QRhiSampler::None,
            QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
        if (!m_sampler->create()) return false;
        m_uniforms.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 16));
        if (!m_uniforms->create()) return false;
        m_bindings.reset(rhi->newShaderResourceBindings());
        m_bindings->setBindings({
            QRhiShaderResourceBinding::uniformBuffer(0, QRhiShaderResourceBinding::VertexStage, m_uniforms.get()),
            QRhiShaderResourceBinding::sampledTexture(1, QRhiShaderResourceBinding::FragmentStage, m_texture.get(), m_sampler.get())});
        if (!m_bindings->create()) return false;
        m_pipeline.reset(rhi->newGraphicsPipeline());
        m_pipeline->setSampleCount(m_samples);
        m_pipeline->setShaderStages({{QRhiShaderStage::Vertex, shader(":/opennow/shaders/framegen.vert.qsb")},
                                    {QRhiShaderStage::Fragment, shader(":/opennow/shaders/hdroutput.frag.qsb")}});
        m_pipeline->setShaderResourceBindings(m_bindings.get());
        m_pipeline->setRenderPassDescriptor(output->renderPassDescriptor());
        m_ready = m_pipeline->create();
        return m_ready;
    }

    QRhiRenderTarget *target() const { return m_target.get(); }

    void record(QRhiSwapChain *sc)
    {
        record(sc->currentFrameCommandBuffer(), sc->currentFrameRenderTarget());
    }

    void record(QRhiCommandBuffer *cb, QRhiRenderTarget *output)
    {
        if (!m_ready) return;
        const float params[] = {0, 0, m_rhi->isYUpInFramebuffer() == m_rhi->isYUpInNDC() ? 1.0f : -1.0f, 0};
        auto *updates = m_rhi->nextResourceUpdateBatch();
        updates->updateDynamicBuffer(m_uniforms.get(), 0, sizeof(params), params);
        cb->beginPass(output, Qt::black, {1.0f, 0}, updates);
        cb->setGraphicsPipeline(m_pipeline.get());
        const auto size = output->pixelSize();
        cb->setViewport({0, 0, float(size.width()), float(size.height())});
        cb->setShaderResources();
        cb->draw(3);
        cb->endPass();
    }

private:
    bool matches(QRhi *rhi, QRhiRenderTarget *output) const
    {
        return m_ready && m_rhi == rhi && m_texture && m_texture->pixelSize() == output->pixelSize()
            && m_samples == output->sampleCount()
            && m_outputFormat == output->renderPassDescriptor()->serializedFormat();
    }
    static QShader shader(const char *path)
    {
        QFile file(QString::fromLatin1(path));
        if (!file.open(QIODevice::ReadOnly)) return {};
        return QShader::fromSerialized(file.readAll());
    }
    QRhi *m_rhi = nullptr;
    bool m_ready = false;
    int m_samples = 1;
    QVector<quint32> m_outputFormat;
    std::unique_ptr<QRhiTexture> m_texture;
    std::unique_ptr<QRhiTexture> m_multisample;
    std::unique_ptr<QRhiRenderBuffer> m_depth;
    std::unique_ptr<QRhiRenderPassDescriptor> m_pass;
    std::unique_ptr<QRhiTextureRenderTarget> m_target;
    std::unique_ptr<QRhiSampler> m_sampler;
    std::unique_ptr<QRhiBuffer> m_uniforms;
    std::unique_ptr<QRhiShaderResourceBindings> m_bindings;
    std::unique_ptr<QRhiGraphicsPipeline> m_pipeline;
};
