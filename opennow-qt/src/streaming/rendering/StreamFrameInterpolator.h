#pragma once

#include <QString>

#include <memory>

class FlyHighNetwork;
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
    // FlyHigh interpolates the two history copies. Call it after ingest, in the same
    // frame: the engine executes the copy before it samples it.
    bool computePending(QRhiCommandBuffer *cb);
    QRhiTexture *currentTexture() const;
    // The newer source frame of the current pair, shown after that pair's generated frames.
    QRhiTexture *anchorTexture() const;
    // The earlier frame of the current pair (diagnostics).
    QRhiTexture *previousTexture() const;
    QRhiTexture *midpointTexture() const;
    // Generated frames available for the current pair (1-based index between the two
    // source frames, in presentation order).
    int generatedCount() const;
    QRhiTexture *generatedTexture(int index) const;
    // FlyHigh computes generated frame `index` here, in the pass that shows it, so the work
    // of one pair is spread over its presentation slots. A no-op for block matching.
    bool prepare(QRhiCommandBuffer *cb, int index);
    bool hasPair() const;
    // GPU milliseconds of the last pair, measured by the engine. 0 for block matching.
    double lastGpuMs() const;
    // "flyhigh WxH" (network grid) or "motion".
    QString engine() const;
    void reset();
    void release();

private:
    struct State;
    std::unique_ptr<State> m_state;
    std::unique_ptr<FlyHighNetwork> m_flyHigh;
};
