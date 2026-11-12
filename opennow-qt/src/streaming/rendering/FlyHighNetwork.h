#pragma once

#include <QSize>
#include <QString>

#include <memory>

class QRhi;
class QRhiCommandBuffer;
class QRhiTexture;

// FlyHigh Engine (https://github.com/getflyhigh/flyhigh-engine) on its own Vulkan device.
// Source frames are copied into D3D11 shared textures and imported; generated frames are
// copied back into D3D11 textures Qt samples. The engine owns the network, the static
// protection and the repair pass.
class FlyHighNetwork
{
public:
    static constexpr int MaximumFrames = 3;

    FlyHighNetwork();
    ~FlyHighNetwork();
    FlyHighNetwork(const FlyHighNetwork &) = delete;
    FlyHighNetwork &operator=(const FlyHighNetwork &) = delete;

    // Explicit engine selection, not a claim of bridge/bundle availability.
    // Current releases reject FlyHigh initialization until its GPU completion
    // and resource-lifetime contract can be verified.
    static bool enabled();
    static QString unavailableReason();
    static QString weightsPath();

    bool initialize(QRhi *rhi);
    // Runs the pair and every generated frame at t = k / (count + 1). `frame` is then a no-op.
    bool pair(QRhiCommandBuffer *cb, QRhiTexture *previous, QRhiTexture *current, int count);
    bool frame(QRhiCommandBuffer *cb, int index);
    QRhiTexture *output(int index) const;
    int count() const;
    QSize networkSize() const;
    double lastGpuMs() const;
    QString error() const;
    void release();

private:
    struct State;
    std::unique_ptr<State> m_state;
    QString m_error;
};
