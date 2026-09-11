#include "streaming/rendering/HdrChromeEffect.h"
#include "streaming/rendering/HdrOutput.h"

#include <QSGGeometryNode>
#include <QSGMaterial>
#include <QSGMaterialShader>
#include <QSGTextureProvider>
#include <cstring>

namespace {
class ChromeMaterial;

class ChromeShader final : public QSGMaterialShader
{
public:
    ChromeShader()
    {
        setShaderFileName(VertexStage, QStringLiteral(":/opennow/shaders/hdrchrome.vert.qsb"));
        setShaderFileName(FragmentStage, QStringLiteral(":/opennow/shaders/hdrchrome.frag.qsb"));
    }
    bool updateUniformData(RenderState &state, QSGMaterial *, QSGMaterial *) override
    {
        auto *data = state.uniformData();
        const auto output = HdrOutput::renderState();
        const float parameters[] = {state.opacity(), float(output.mode), output.whiteNits, 0.0f};
        std::memcpy(data->data(), state.combinedMatrix().constData(), 64);
        std::memcpy(data->data() + 64, parameters, sizeof(parameters));
        return true;
    }
    void updateSampledImage(RenderState &, int binding, QSGTexture **texture,
                            QSGMaterial *material, QSGMaterial *) override;
};

class ChromeMaterial final : public QSGMaterial
{
public:
    ChromeMaterial() { setFlag(Blending); }
    QSGMaterialType *type() const override { static QSGMaterialType type; return &type; }
    QSGMaterialShader *createShader(QSGRendererInterface::RenderMode) const override
    {
        return new ChromeShader;
    }
    int compare(const QSGMaterial *other) const override
    {
        if (this == other) return 0;
        return std::less<const QSGMaterial *>{}(this, other) ? -1 : 1;
    }
    QPointer<QSGTextureProvider> provider;
};

void ChromeShader::updateSampledImage(RenderState &state, int binding, QSGTexture **texture,
                                     QSGMaterial *material, QSGMaterial *)
{
    if (binding != 1) return;
    const auto *chrome = static_cast<ChromeMaterial *>(material);
    *texture = chrome->provider ? chrome->provider->texture() : nullptr;
    if (*texture) {
        (*texture)->setFiltering(QSGTexture::Linear);
        (*texture)->commitTextureOperations(state.rhi(), state.resourceUpdateBatch());
    }
}

class ChromeNode final : public QSGGeometryNode
{
public:
    ChromeNode()
    {
        setGeometry(new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 4));
        setFlag(OwnsGeometry);
        setMaterial(new ChromeMaterial);
        setFlag(OwnsMaterial);
        setFlag(UsePreprocess);
    }
    void preprocess() override
    {
        const auto *chrome = static_cast<ChromeMaterial *>(material());
        if (chrome->provider) {
            if (auto *texture = qobject_cast<QSGDynamicTexture *>(chrome->provider->texture()))
                texture->updateTexture();
        }
        markDirty(DirtyMaterial);
    }
};
}

HdrChromeEffect::HdrChromeEffect(QQuickItem *parent) : QQuickItem(parent)
{
    setFlag(ItemHasContents);
}

void HdrChromeEffect::setSource(QQuickItem *source)
{
    if (m_source == source) return;
    m_source = source;
    emit sourceChanged();
    update();
}

QSGNode *HdrChromeEffect::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    auto *provider = m_source && m_source->isTextureProvider() ? m_source->textureProvider() : nullptr;
    if (!provider) { delete oldNode; return nullptr; }
    auto *node = static_cast<ChromeNode *>(oldNode);
    if (!node) node = new ChromeNode;
    static_cast<ChromeMaterial *>(node->material())->provider = provider;
    QSGGeometry::updateTexturedRectGeometry(node->geometry(), boundingRect(), QRectF(0, 0, 1, 1));
    node->markDirty(QSGNode::DirtyGeometry | QSGNode::DirtyMaterial);
    return node;
}
