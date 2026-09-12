#pragma once

#include <memory>

class QRhi;
class QRhiCommandBuffer;
class QRhiTexture;

class StreamFrameInterpolator
{
public:
    StreamFrameInterpolator();
    ~StreamFrameInterpolator();
    StreamFrameInterpolator(const StreamFrameInterpolator &) = delete;
    StreamFrameInterpolator &operator=(const StreamFrameInterpolator &) = delete;

    bool initialize(QRhi *rhi);
    bool ingest(QRhiCommandBuffer *cb, QRhiTexture *source);
    QRhiTexture *currentTexture() const;
    QRhiTexture *midpointTexture() const;
    bool hasPair() const;
    void reset();
    void release();

private:
    struct State;
    std::unique_ptr<State> m_state;
};
