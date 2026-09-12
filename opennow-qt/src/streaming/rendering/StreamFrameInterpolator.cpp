#include "streaming/rendering/StreamFrameInterpolator.h"

#include <QFile>
#include <QSize>
#include <rhi/qrhi.h>
#include <rhi/qshader.h>
#include <algorithm>
#include <array>
#include <initializer_list>
#include <vector>

namespace {
QShader shader(const char *name)
{
    QFile file(QStringLiteral(":/opennow/shaders/") + QString::fromLatin1(name)
               + QStringLiteral(".qsb"));
    return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader{};
}

struct Surface {
    std::unique_ptr<QRhiTexture> texture;
    std::unique_ptr<QRhiRenderPassDescriptor> descriptor;
    std::unique_ptr<QRhiTextureRenderTarget> target;

    bool create(QRhi *rhi, QRhiTexture::Format format, QSize size, QRhiTexture::Flags flags = {})
    {
        texture.reset(rhi->newTexture(format, size, 1, QRhiTexture::RenderTarget | flags));
        if (!texture->create()) return false;
        target.reset(rhi->newTextureRenderTarget(QRhiTextureRenderTargetDescription(
            QRhiColorAttachment(texture.get()))));
        descriptor.reset(target->newCompatibleRenderPassDescriptor());
        target->setRenderPassDescriptor(descriptor.get());
        return target->create();
    }
};

struct Pass {
    std::unique_ptr<QRhiBuffer> uniform;
    std::unique_ptr<QRhiShaderResourceBindings> bindings;
    std::unique_ptr<QRhiGraphicsPipeline> pipeline;
    std::array<float, 4> parameters{};
    std::array<std::array<quint64, 2>, 5> boundTextures{};
    size_t boundTextureCount = 0;

    bool bind(QRhiSampler *sampler, std::initializer_list<QRhiTexture *> textures)
    {
        std::array<std::array<quint64, 2>, 5> identities{};
        size_t textureIndex = 0;
        for (auto *texture : textures)
            identities[textureIndex++] = {texture->globalResourceId(), texture->nativeTexture().object};
        if (boundTextureCount == textures.size() && boundTextures == identities)
            return true;
        std::vector<QRhiShaderResourceBinding> entries;
        entries.push_back(QRhiShaderResourceBinding::uniformBuffer(
            0, QRhiShaderResourceBinding::VertexStage | QRhiShaderResourceBinding::FragmentStage,
            uniform.get()));
        int index = 1;
        for (auto *texture : textures)
            entries.push_back(QRhiShaderResourceBinding::sampledTexture(
                index++, QRhiShaderResourceBinding::FragmentStage, texture, sampler));
        bindings->setBindings(entries.begin(), entries.end());
        if (!bindings->create()) {
            boundTextureCount = 0;
            return false;
        }
        boundTextures = identities;
        boundTextureCount = textures.size();
        return true;
    }

    bool create(QRhi *rhi, Surface &output, QRhiSampler *sampler, const QShader &vertex,
                const QShader &fragment, QSize resolution, float radius,
                std::initializer_list<QRhiTexture *> textures)
    {
        uniform.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 16));
        if (!uniform->create()) return false;
        bindings.reset(rhi->newShaderResourceBindings());
        if (!bind(sampler, textures)) return false;
        parameters = {1.0f / resolution.width(), 1.0f / resolution.height(),
                      rhi->isYUpInFramebuffer() == rhi->isYUpInNDC() ? 1.0f : -1.0f, radius};
        pipeline.reset(rhi->newGraphicsPipeline());
        pipeline->setTopology(QRhiGraphicsPipeline::Triangles);
        pipeline->setShaderStages({{QRhiShaderStage::Vertex, vertex},
                                   {QRhiShaderStage::Fragment, fragment}});
        pipeline->setShaderResourceBindings(bindings.get());
        pipeline->setRenderPassDescriptor(output.descriptor.get());
        return pipeline->create();
    }

    void draw(QRhi *rhi, QRhiCommandBuffer *cb, Surface &output)
    {
        auto *updates = rhi->nextResourceUpdateBatch();
        updates->updateDynamicBuffer(uniform.get(), 0, 16, parameters.data());
        cb->beginPass(output.target.get(), Qt::transparent, {1.0f, 0}, updates);
        cb->setGraphicsPipeline(pipeline.get());
        cb->setShaderResources(bindings.get());
        const QSize size = output.texture->pixelSize();
        cb->setViewport(QRhiViewport(0, 0, size.width(), size.height()));
        cb->draw(3);
        cb->endPass();
    }
};
}

struct StreamFrameInterpolator::State {
    QRhi *rhi = nullptr;
    std::unique_ptr<QRhiSampler> sampler;
    std::array<Surface, 2> history;
    std::array<std::array<Surface, 3>, 2> pyramid;
    std::array<std::array<Surface, 3>, 2> flow;
    Surface cut;
    Surface midpoint;
    std::array<Pass, 2> copy;
    std::array<std::array<Pass, 3>, 2> reduce;
    std::array<std::array<std::array<Pass, 3>, 2>, 2> motion;
    std::array<Pass, 2> detectCut;
    std::array<Pass, 2> synthesize;
    QSize size;
    QRhiTexture::Format format = QRhiTexture::UnknownFormat;
    int current = 0;
    int frames = 0;

    bool allocate(QRhiTexture *source)
    {
        size = source->pixelSize();
        format = source->format();
        const QShader vertex = shader("framegen.vert");
        const QShader copyShader = shader("framegen_copy.frag");
        const QShader reduceShader = shader("framegen_reduce.frag");
        const QShader motionShader = shader("framegen_motion.frag");
        const QShader cutShader = shader("framegen_cut.frag");
        const QShader synthesizeShader = shader("framegen_synthesize.frag");
        if (!vertex.isValid() || !copyShader.isValid() || !reduceShader.isValid()
            || !motionShader.isValid() || !cutShader.isValid() || !synthesizeShader.isValid())
            return false;
        sampler.reset(rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear,
                                      QRhiSampler::None, QRhiSampler::ClampToEdge,
                                      QRhiSampler::ClampToEdge));
        if (!sampler->create()) return false;
        const int divisor = std::max({8, (size.width() + 319) / 320,
                                     (size.height() + 179) / 180});
        std::array<QSize, 3> levels;
        for (int level = 0; level < 3; ++level) {
            const int scale = divisor << level;
            levels[level] = QSize(std::max(1, (size.width() + scale - 1) / scale),
                                  std::max(1, (size.height() + scale - 1) / scale));
        }
        for (int slot = 0; slot < 2; ++slot) {
            if (!history[slot].create(rhi, format, size, QRhiTexture::UsedAsTransferSource)) return false;
            for (int level = 0; level < 3; ++level) {
                if (!pyramid[slot][level].create(rhi, QRhiTexture::RGBA8, levels[level])
                    || !flow[slot][level].create(rhi, QRhiTexture::RGBA16F, levels[level]))
                    return false;
            }
        }
        if (!midpoint.create(rhi, format, size, QRhiTexture::UsedAsTransferSource)
            || !cut.create(rhi, QRhiTexture::RGBA8, {1, 1}))
            return false;
        for (int slot = 0; slot < 2; ++slot) {
            if (!copy[slot].create(rhi, history[slot], sampler.get(), vertex, copyShader,
                                   size, 0.0f, {source}))
                return false;
            for (int level = 0; level < 3; ++level) {
                auto *input = level == 0 ? history[slot].texture.get()
                                        : pyramid[slot][level - 1].texture.get();
                if (!reduce[slot][level].create(rhi, pyramid[slot][level], sampler.get(),
                                               vertex, reduceShader, levels[level], 0.0f, {input}))
                    return false;
            }
            for (int direction = 0; direction < 2; ++direction) {
                const int from = direction == 0 ? 1 - slot : slot;
                const int to = 1 - from;
                for (int level = 2; level >= 0; --level) {
                    auto *coarse = level == 2 ? pyramid[from][level].texture.get()
                                             : flow[direction][level + 1].texture.get();
                    if (!motion[slot][direction][level].create(
                            rhi, flow[direction][level], sampler.get(), vertex, motionShader,
                            levels[level], float(level),
                            {pyramid[from][level].texture.get(), pyramid[to][level].texture.get(), coarse}))
                        return false;
                }
            }
            if (!detectCut[slot].create(rhi, cut, sampler.get(), vertex, cutShader, {1, 1}, 0.0f,
                                       {pyramid[1 - slot][0].texture.get(), pyramid[slot][0].texture.get(),
                                        flow[0][0].texture.get(), flow[1][0].texture.get()}))
                return false;
            if (!synthesize[slot].create(rhi, midpoint, sampler.get(), vertex, synthesizeShader,
                                        levels[0], 0.0f,
                                        {history[1 - slot].texture.get(), history[slot].texture.get(),
                                         flow[0][0].texture.get(), flow[1][0].texture.get(), cut.texture.get()}))
                return false;
        }
        return true;
    }
};

StreamFrameInterpolator::StreamFrameInterpolator() = default;
StreamFrameInterpolator::~StreamFrameInterpolator() = default;

bool StreamFrameInterpolator::initialize(QRhi *rhi)
{
    if (!rhi) {
        release();
        return false;
    }
    if (m_state && m_state->rhi == rhi) return true;
    release();
    if (rhi->backend() == QRhi::Null
        || !rhi->isTextureFormatSupported(QRhiTexture::RGBA16F, QRhiTexture::RenderTarget)
        || !rhi->isTextureFormatSupported(QRhiTexture::RGBA8, QRhiTexture::RenderTarget))
        return false;
    m_state = std::make_unique<State>();
    m_state->rhi = rhi;
    return true;
}

bool StreamFrameInterpolator::ingest(QRhiCommandBuffer *cb, QRhiTexture *source)
{
    if (!m_state || !cb || !source) {
        reset();
        return false;
    }
    auto *rhi = m_state->rhi;
    const QSize size = source->pixelSize();
    const int maxDimension = std::min(4096, rhi->resourceLimit(QRhi::TextureSizeMax));
    if (source->rhi() != rhi || cb->rhi() != rhi
        || source->sampleCount() != 1 || source->pixelSize().isEmpty()
        || size.width() > maxDimension || size.height() > maxDimension
        || qint64(size.width()) * size.height() > 4096 * 2160
        || source->flags().testAnyFlags(QRhiTexture::CubeMap | QRhiTexture::TextureArray
                                       | QRhiTexture::ThreeDimensional)
        || (source->format() != QRhiTexture::RGBA8 && source->format() != QRhiTexture::RGB10A2)
        || !rhi->isTextureFormatSupported(source->format(), QRhiTexture::RenderTarget)) {
        reset();
        return false;
    }
    if (m_state->size != source->pixelSize() || m_state->format != source->format()) {
        m_state = std::make_unique<State>();
        m_state->rhi = rhi;
        if (!m_state->allocate(source)) {
            m_state = std::make_unique<State>();
            m_state->rhi = rhi;
            return false;
        }
    }
    auto &s = *m_state;
    const int next = s.frames == 0 ? 0 : 1 - s.current;
    if (!s.copy[next].bind(s.sampler.get(), {source})) {
        reset();
        return false;
    }
    s.copy[next].draw(rhi, cb, s.history[next]);
    for (int level = 0; level < 3; ++level)
        s.reduce[next][level].draw(rhi, cb, s.pyramid[next][level]);
    if (s.frames > 0) {
        for (int level = 2; level >= 0; --level)
            for (int direction = 0; direction < 2; ++direction)
                s.motion[next][direction][level].draw(rhi, cb, s.flow[direction][level]);
        s.detectCut[next].draw(rhi, cb, s.cut);
        s.synthesize[next].draw(rhi, cb, s.midpoint);
    }
    s.current = next;
    s.frames = std::min(2, s.frames + 1);
    return true;
}

QRhiTexture *StreamFrameInterpolator::currentTexture() const
{
    return m_state && m_state->frames > 0 ? m_state->history[m_state->current].texture.get() : nullptr;
}

QRhiTexture *StreamFrameInterpolator::midpointTexture() const
{
    return hasPair() ? m_state->midpoint.texture.get() : nullptr;
}

bool StreamFrameInterpolator::hasPair() const
{
    return m_state && m_state->frames == 2;
}

void StreamFrameInterpolator::reset()
{
    if (m_state) m_state->frames = 0;
}

void StreamFrameInterpolator::release()
{
    m_state.reset();
}
