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

    // Generated frames per pair: N - 1 frames at t = k / N for a factor N up to this.
    static constexpr int MaximumFactor = 4;

    bool initialize(QRhi *rhi);
    bool ingest(QRhiCommandBuffer *cb, QRhiTexture *source, int factor = 2);
    QRhiTexture *currentTexture() const;
    // The earlier frame of the current pair (diagnostics).
    QRhiTexture *previousTexture() const;
    QRhiTexture *midpointTexture() const;
    // Generated frames available for the current pair (1-based index between the two
    // source frames, in presentation order).
    int generatedCount() const;
    QRhiTexture *generatedTexture(int index) const;
    bool hasPair() const;
    void reset();
    void release();

private:
    struct State;
    std::unique_ptr<State> m_state;
};
