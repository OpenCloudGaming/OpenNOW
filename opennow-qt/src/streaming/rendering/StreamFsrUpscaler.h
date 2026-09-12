#pragma once

#include <QFile>
#include <QDebug>
#include <QSize>
#include <rhi/qrhi.h>
#include <rhi/qshader.h>
#include <algorithm>
#include <array>
#include <memory>

class StreamFsrUpscaler
{
public:
    bool matchesConfiguration(QRhi *rhi, QRhiTexture *source, const QSize &target) const
    {
        return m_state && source && m_state->rhi == rhi
            && m_state->sourceSize == source->pixelSize() && m_state->targetSize == target
            && m_state->sourceFormat == source->format();
    }

    void forgetSource(QRhiTexture *source)
    {
        if (!m_state || !source) return;
        for (auto &entry : m_state->easu.bindings)
            if (entry.id == source->globalResourceId()) entry = Pass::Binding{};
    }

    QRhiTexture *render(QRhi *rhi, QRhiCommandBuffer *cb, QRhiTexture *source,
                        const QSize &target, bool enabled, bool sdr, int sharpness)
    {
        if (!rhi || !cb || !source || !enabled || !sdr
            || (rhi->backend() != QRhi::D3D11 && rhi->backend() != QRhi::Vulkan)
            || source->pixelSize().width() < 2 || source->pixelSize().height() < 2
            || target.width() <= source->pixelSize().width()
            || target.height() <= source->pixelSize().height()
            || target.width() > rhi->resourceLimit(QRhi::TextureSizeMax)
            || target.height() > rhi->resourceLimit(QRhi::TextureSizeMax)
            || qint64(target.width()) * target.height() > 16 * 1024 * 1024) {
            release();
            return source;
        }
        const auto format = source->format() == QRhiTexture::BGRA8
            ? QRhiTexture::RGBA8 : source->format();
        if ((format != QRhiTexture::RGBA8 && format != QRhiTexture::RGB10A2
             && format != QRhiTexture::RGBA16F)
            || !rhi->isTextureFormatSupported(format, QRhiTexture::RenderTarget)) {
            release();
            return source;
        }
        if (!matchesConfiguration(rhi, source, target)) {
            release();
            m_state = std::make_unique<State>();
            m_state->rhi = rhi;
            m_state->sourceSize = source->pixelSize();
            m_state->targetSize = target;
            m_state->format = format;
            m_state->sourceFormat = source->format();
            m_state->ready = m_state->create(source);
            if (!m_state->ready)
                qWarning("FSR1 EASU resources unavailable; retaining normal video scaling");
        }
        if (!m_state->ready) return source;
        auto &state = *m_state;
        const float stops = 2.0f * (15 - std::clamp(sharpness, 1, 15)) / 14.0f;
        const std::array<float, 8> parameters = {
            0, 0, rhi->isYUpInFramebuffer() == rhi->isYUpInNDC() ? 1.0f : -1.0f, stops,
            float(state.sourceSize.width()), float(state.sourceSize.height()),
            float(target.width()), float(target.height())};
        if (!state.easu.bind(rhi, state.sampler.get(), source)) return source;
        state.easu.draw(rhi, cb, parameters);
        if (sharpness <= 0) return state.easu.texture.get();
        if (!state.rcas.texture && !state.rcasFailed) {
            state.rcasFailed = !state.rcas.create(rhi, format, target, state.sampler.get(),
                state.easu.texture.get(), shader("fsr_rcas.frag"));
            if (state.rcasFailed) {
                state.rcas = Pass{};
                qWarning("FSR1 RCAS resources unavailable; retaining EASU without sharpening");
            }
        }
        if (state.rcasFailed) return state.easu.texture.get();
        state.rcas.draw(rhi, cb, parameters);
        return state.rcas.texture.get();
    }

    void release() { m_state.reset(); }

private:
    static QShader shader(const char *name)
    {
        QFile file(QStringLiteral(":/opennow/shaders/") + QString::fromLatin1(name)
                   + QStringLiteral(".qsb"));
        return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader{};
    }

    struct Pass {
        std::unique_ptr<QRhiTexture> texture;
        std::unique_ptr<QRhiRenderPassDescriptor> descriptor;
        std::unique_ptr<QRhiTextureRenderTarget> target;
        std::unique_ptr<QRhiBuffer> uniform;
        struct Binding {
            quint64 id = 0;
            quint64 native = 0;
            std::unique_ptr<QRhiShaderResourceBindings> resource;
        };
        std::array<Binding, 10> bindings;
        std::unique_ptr<QRhiGraphicsPipeline> pipeline;
        size_t current = 0;
        size_t replacement = 0;

        bool bind(QRhi *rhi, QRhiSampler *sampler, QRhiTexture *source)
        {
            for (size_t i = 0; i < bindings.size(); ++i) {
                if (bindings[i].id == source->globalResourceId()
                    && bindings[i].native == source->nativeTexture().object
                    && bindings[i].resource) {
                    current = i;
                    return true;
                }
            }
            current = replacement++ % bindings.size();
            auto &entry = bindings[current];
            entry = Binding{};
            entry.resource.reset(rhi->newShaderResourceBindings());
            entry.resource->setBindings({
                QRhiShaderResourceBinding::uniformBuffer(0,
                    QRhiShaderResourceBinding::VertexStage | QRhiShaderResourceBinding::FragmentStage,
                    uniform.get()),
                QRhiShaderResourceBinding::sampledTexture(1,
                    QRhiShaderResourceBinding::FragmentStage, source, sampler)});
            if (!entry.resource->create()) { entry.resource.reset(); return false; }
            entry.id = source->globalResourceId();
            entry.native = source->nativeTexture().object;
            return true;
        }

        bool create(QRhi *rhi, QRhiTexture::Format format, QSize size,
                    QRhiSampler *sampler, QRhiTexture *source, const QShader &fragment)
        {
            const auto vertex = shader("framegen.vert");
            if (!vertex.isValid() || !fragment.isValid()) return false;
            texture.reset(rhi->newTexture(format, size, 1,
                QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
            if (!texture->create()) return false;
            target.reset(rhi->newTextureRenderTarget(QRhiTextureRenderTargetDescription(
                QRhiColorAttachment(texture.get()))));
            descriptor.reset(target->newCompatibleRenderPassDescriptor());
            target->setRenderPassDescriptor(descriptor.get());
            if (!target->create()) return false;
            uniform.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 32));
            if (!uniform->create() || !bind(rhi, sampler, source)) return false;
            pipeline.reset(rhi->newGraphicsPipeline());
            pipeline->setTopology(QRhiGraphicsPipeline::Triangles);
            pipeline->setShaderStages({{QRhiShaderStage::Vertex, vertex},
                                       {QRhiShaderStage::Fragment, fragment}});
            pipeline->setShaderResourceBindings(bindings[current].resource.get());
            pipeline->setRenderPassDescriptor(descriptor.get());
            return pipeline->create();
        }

        void draw(QRhi *rhi, QRhiCommandBuffer *cb, const std::array<float, 8> &parameters)
        {
            auto *updates = rhi->nextResourceUpdateBatch();
            updates->updateDynamicBuffer(uniform.get(), 0, 32, parameters.data());
            cb->beginPass(target.get(), Qt::transparent, {1.0f, 0}, updates);
            cb->setGraphicsPipeline(pipeline.get());
            cb->setShaderResources(bindings[current].resource.get());
            cb->setViewport(QRhiViewport(0, 0, texture->pixelSize().width(), texture->pixelSize().height()));
            cb->draw(3);
            cb->endPass();
        }
    };

    struct State {
        QRhi *rhi = nullptr;
        QSize sourceSize;
        QSize targetSize;
        QRhiTexture::Format format = QRhiTexture::UnknownFormat;
        QRhiTexture::Format sourceFormat = QRhiTexture::UnknownFormat;
        std::unique_ptr<QRhiSampler> sampler;
        Pass easu;
        Pass rcas;
        bool ready = false;
        bool rcasFailed = false;

        bool create(QRhiTexture *source)
        {
            sampler.reset(rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear,
                QRhiSampler::None, QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
            if (sampler->create() && easu.create(rhi, format, targetSize, sampler.get(), source,
                                               shader("fsr_easu.frag"))) return true;
            easu = Pass{};
            sampler.reset();
            return false;
        }
    };
    std::unique_ptr<State> m_state;
};
