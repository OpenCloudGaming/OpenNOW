#include "OpnQtWebRtcStreamSession.h"

#include <QtCore/QDebug>

#if defined(OPNQT_HAVE_LIBWEBRTC)
#include <QtCore/QMetaObject>
#include <QtCore/QStringList>

#include <api/create_peerconnection_factory.h>
#include <api/audio_codecs/builtin_audio_decoder_factory.h>
#include <api/audio_codecs/builtin_audio_encoder_factory.h>
#include <api/data_channel_interface.h>
#include <api/jsep.h>
#include <api/make_ref_counted.h>
#include <api/media_stream_interface.h>
#include <api/peer_connection_interface.h>
#include <api/scoped_refptr.h>
#include <api/task_queue/default_task_queue_factory.h>
#include <api/video/video_frame.h>
#include <api/video/video_frame_buffer.h>
#include <api/video/video_sink_interface.h>
#include <api/video_codecs/video_decoder_factory_template.h>
#include <api/video_codecs/video_decoder_factory_template_dav1d_adapter.h>
#include <api/video_codecs/video_decoder_factory_template_libvpx_vp8_adapter.h>
#include <api/video_codecs/video_decoder_factory_template_libvpx_vp9_adapter.h>
#include <api/video_codecs/video_decoder_factory_template_open_h264_adapter.h>
#include <api/video_codecs/video_encoder_factory_template.h>
#include <api/video_codecs/video_encoder_factory_template_libaom_av1_adapter.h>
#include <api/video_codecs/video_encoder_factory_template_libvpx_vp8_adapter.h>
#include <api/video_codecs/video_encoder_factory_template_libvpx_vp9_adapter.h>
#include <api/video_codecs/video_encoder_factory_template_open_h264_adapter.h>
#include <media/base/media_engine.h>
#include <modules/audio_device/include/audio_device.h>
#include <pc/video_track_source.h>
#include <rtc_base/logging.h>
#include <rtc_base/ssl_adapter.h>
#include <rtc_base/thread.h>

#include <memory>
#include <string>
#include <utility>
#include <vector>
#include <chrono>
#endif

namespace OpnQt {

#if defined(OPNQT_HAVE_LIBWEBRTC)
namespace {

std::string toStdString(const QString &value) {
    return value.toStdString();
}

QString fromStdString(const std::string &value) {
    return QString::fromStdString(value);
}

QString rtcErrorMessage(const webrtc::RTCError &error) {
    const std::string message = error.message();
    return message.empty() ? QStringLiteral("unknown WebRTC error") : fromStdString(message);
}

constexpr quint32 kInputHeartbeat = 2;
constexpr quint32 kInputKeyDown = 3;
constexpr quint32 kInputKeyUp = 4;
constexpr quint32 kInputMouseRel = 7;
constexpr quint32 kInputMouseButtonDown = 8;
constexpr quint32 kInputMouseButtonUp = 9;
constexpr quint32 kInputMouseWheel = 10;
constexpr int kPartialReliableInputLifetimeMs = 8;

quint64 timestampUs() {
    static const auto start = std::chrono::steady_clock::now();
    return static_cast<quint64>(std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now() - start).count());
}

void writeU16BE(std::vector<uint8_t> &out, size_t offset, quint16 value) {
    out[offset] = static_cast<uint8_t>((value >> 8) & 0xffu);
    out[offset + 1] = static_cast<uint8_t>(value & 0xffu);
}

void writeI16BE(std::vector<uint8_t> &out, size_t offset, qint16 value) {
    writeU16BE(out, offset, static_cast<quint16>(value));
}

void writeU32LE(std::vector<uint8_t> &out, size_t offset, quint32 value) {
    out[offset] = static_cast<uint8_t>(value & 0xffu);
    out[offset + 1] = static_cast<uint8_t>((value >> 8) & 0xffu);
    out[offset + 2] = static_cast<uint8_t>((value >> 16) & 0xffu);
    out[offset + 3] = static_cast<uint8_t>((value >> 24) & 0xffu);
}

void writeU64BE(std::vector<uint8_t> &out, size_t offset, quint64 value) {
    for (int i = 7; i >= 0; --i) out[offset + static_cast<size_t>(7 - i)] = static_cast<uint8_t>((value >> (i * 8)) & 0xffu);
}

std::vector<uint8_t> encodeHeartbeat() {
    std::vector<uint8_t> bytes(4, 0);
    writeU32LE(bytes, 0, kInputHeartbeat);
    return bytes;
}

std::vector<uint8_t> wrapSingleEvent(std::vector<uint8_t> payload, quint16 protocolVersion) {
    if (protocolVersion <= 2) return payload;
    std::vector<uint8_t> wrapped(10 + payload.size(), 0);
    wrapped[0] = 0x23;
    writeU64BE(wrapped, 1, timestampUs());
    wrapped[9] = 0x22;
    std::copy(payload.begin(), payload.end(), wrapped.begin() + 10);
    return wrapped;
}

std::vector<uint8_t> wrapMouseMoveEvent(std::vector<uint8_t> payload, quint16 protocolVersion) {
    if (protocolVersion <= 2) return payload;
    std::vector<uint8_t> wrapped(12 + payload.size(), 0);
    wrapped[0] = 0x23;
    writeU64BE(wrapped, 1, timestampUs());
    wrapped[9] = 0x21;
    writeU16BE(wrapped, 10, static_cast<quint16>(payload.size()));
    std::copy(payload.begin(), payload.end(), wrapped.begin() + 12);
    return wrapped;
}

std::vector<uint8_t> encodeKey(quint32 type, quint16 keycode, quint16 scancode, quint16 modifiers, quint16 protocolVersion) {
    std::vector<uint8_t> bytes(18, 0);
    writeU32LE(bytes, 0, type);
    writeU16BE(bytes, 4, keycode);
    writeU16BE(bytes, 6, modifiers);
    writeU16BE(bytes, 8, scancode);
    writeU64BE(bytes, 10, timestampUs());
    return wrapSingleEvent(std::move(bytes), protocolVersion);
}

std::vector<uint8_t> encodeMouseMove(qint16 dx, qint16 dy, quint16 protocolVersion) {
    std::vector<uint8_t> bytes(22, 0);
    writeU32LE(bytes, 0, kInputMouseRel);
    writeI16BE(bytes, 4, dx);
    writeI16BE(bytes, 6, dy);
    writeU64BE(bytes, 14, timestampUs());
    return wrapMouseMoveEvent(std::move(bytes), protocolVersion);
}

std::vector<uint8_t> encodeMouseButton(quint32 type, quint8 button, quint16 protocolVersion) {
    std::vector<uint8_t> bytes(18, 0);
    writeU32LE(bytes, 0, type);
    bytes[4] = button;
    writeU64BE(bytes, 10, timestampUs());
    return wrapSingleEvent(std::move(bytes), protocolVersion);
}

std::vector<uint8_t> encodeMouseWheel(qint16 delta, quint16 protocolVersion) {
    std::vector<uint8_t> bytes(22, 0);
    writeU32LE(bytes, 0, kInputMouseWheel);
    writeI16BE(bytes, 6, delta);
    writeU64BE(bytes, 14, timestampUs());
    return wrapSingleEvent(std::move(bytes), protocolVersion);
}

struct IceCredentials {
    QString ufrag;
    QString pwd;
    QString fingerprint;
};

bool startsWith(const QString &value, QStringView prefix) {
    return value.startsWith(prefix);
}

IceCredentials extractIceCredentials(const QString &sdp) {
    IceCredentials credentials;
    const QStringList lines = sdp.split(QLatin1Char('\n'));
    for (QString line : lines) {
        if (line.endsWith(QLatin1Char('\r'))) line.chop(1);
        if (startsWith(line, u"a=ice-ufrag:")) {
            credentials.ufrag = line.mid(12);
        } else if (startsWith(line, u"a=ice-pwd:")) {
            credentials.pwd = line.mid(10);
        } else if (startsWith(line, u"a=fingerprint:")) {
            credentials.fingerprint = line.mid(14);
        }
    }
    return credentials;
}

QStringList splitResolution(const QString &resolution) {
    const int separator = resolution.indexOf(QLatin1Char('x'));
    if (separator <= 0 || separator + 1 >= resolution.size()) return {QStringLiteral("1920"), QStringLiteral("1080")};
    return {resolution.left(separator), resolution.mid(separator + 1)};
}

int stringToPositiveInt(const QString &value, int fallback) {
    bool ok = false;
    const int parsed = value.toInt(&ok);
    return ok && parsed > 0 ? parsed : fallback;
}

QString normalizeCodec(QString codec) {
    codec = codec.toUpper();
    if (codec == QLatin1String("AUTO")) return QStringLiteral("H264");
    if (codec == QLatin1String("HEVC")) return QStringLiteral("H265");
    return codec;
}

QString buildNvstSdp(const StreamSettings &settings, const IceCredentials &credentials) {
    static constexpr int partialReliableInputLifetimeMs = 8;

    const QStringList resolution = splitResolution(settings.resolution);
    const int width = stringToPositiveInt(resolution.value(0), 1920);
    const int height = stringToPositiveInt(resolution.value(1), 1080);
    const int maxBitrateKbps = std::max(1000, settings.maxBitrateMbps * 1000);
    const int minBitrateKbps = std::max(5000, maxBitrateKbps * 35 / 100);
    const int initialBitrateKbps = std::max(minBitrateKbps, maxBitrateKbps * 70 / 100);
    const int bitDepth = settings.colorQuality.startsWith(QStringLiteral("10bit")) ? 10 : 8;
    const QString codec = normalizeCodec(settings.codec);
    const bool isAv1 = codec == QLatin1String("AV1");
    const bool isHighFps = settings.fps >= 90;
    const bool is120Fps = settings.fps == 120;
    const bool is240Fps = settings.fps >= 240;

    QStringList lines = {
        QStringLiteral("v=0"),
        QStringLiteral("o=SdpTest test_id_13 14 IN IPv4 127.0.0.1"),
        QStringLiteral("s=-"),
        QStringLiteral("t=0 0"),
        QStringLiteral("a=general.icePassword:%1").arg(credentials.pwd),
        QStringLiteral("a=general.iceUserNameFragment:%1").arg(credentials.ufrag),
        QStringLiteral("a=general.dtlsFingerprint:%1").arg(credentials.fingerprint),
        QStringLiteral("m=video 0 RTP/AVP"),
        QStringLiteral("a=msid:fbc-video-0"),
        QStringLiteral("a=vqos.fec.rateDropWindow:10"),
        QStringLiteral("a=vqos.fec.minRequiredFecPackets:2"),
        QStringLiteral("a=vqos.fec.repairMinPercent:5"),
        QStringLiteral("a=vqos.fec.repairPercent:5"),
        QStringLiteral("a=vqos.fec.repairMaxPercent:35"),
        QStringLiteral("a=vqos.dynamicStreamingMode:0"),
        QStringLiteral("a=vqos.drc.enable:0"),
        QStringLiteral("a=vqos.dfc.enable:0"),
        QStringLiteral("a=vqos.dfc.adjustResAndFps:0"),
        QStringLiteral("a=video.dx9EnableNv12:1"),
        QStringLiteral("a=video.dx9EnableHdr:1"),
        QStringLiteral("a=vqos.qpg.enable:1"),
        QStringLiteral("a=vqos.resControl.qp.qpg.featureSetting:7"),
        QStringLiteral("a=bwe.useOwdCongestionControl:1"),
        QStringLiteral("a=video.enableRtpNack:1"),
        QStringLiteral("a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200"),
        QStringLiteral("a=vqos.drc.bitrateIirFilterFactor:18"),
        QStringLiteral("a=video.packetSize:1140"),
        QStringLiteral("a=packetPacing.minNumPacketsPerGroup:15"),
    };

    if (isHighFps) {
        lines.append({
            QStringLiteral("a=bwe.iirFilterFactor:8"),
            QStringLiteral("a=video.encoderFeatureSetting:47"),
            QStringLiteral("a=video.encoderPreset:6"),
            QStringLiteral("a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600"),
            QStringLiteral("a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9"),
            QStringLiteral("a=video.fbcDynamicFpsGrabTimeoutMs:%1").arg(is120Fps ? 6 : 18),
            QStringLiteral("a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:%1").arg(is120Fps ? 6000 : 12000),
        });
    }

    if (is240Fps) {
        lines.append({
            QStringLiteral("a=video.enableNextCaptureMode:1"),
            QStringLiteral("a=vqos.maxStreamFpsEstimate:240"),
            QStringLiteral("a=video.videoSplitEncodeStripsPerFrame:3"),
            QStringLiteral("a=video.updateSplitEncodeStateDynamically:1"),
        });
    }

    lines.append({
        QStringLiteral("a=vqos.adjustStreamingFpsDuringOutOfFocus:1"),
        QStringLiteral("a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1"),
        QStringLiteral("a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1"),
        QStringLiteral("a=vqos.resControl.cpmRtc.featureMask:0"),
        QStringLiteral("a=vqos.resControl.cpmRtc.enable:0"),
        QStringLiteral("a=vqos.resControl.cpmRtc.minResolutionPercent:100"),
        QStringLiteral("a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999"),
        QStringLiteral("a=packetPacing.numGroups:%1").arg(is120Fps ? 3 : 5),
        QStringLiteral("a=packetPacing.maxDelayUs:1000"),
        QStringLiteral("a=packetPacing.minNumPacketsFrame:10"),
        QStringLiteral("a=video.rtpNackQueueLength:1024"),
        QStringLiteral("a=video.rtpNackQueueMaxPackets:512"),
        QStringLiteral("a=video.rtpNackMaxPacketCount:25"),
        QStringLiteral("a=vqos.drc.qpMaxResThresholdAdj:4"),
        QStringLiteral("a=vqos.grc.qpMaxResThresholdAdj:4"),
        QStringLiteral("a=vqos.drc.iirFilterFactor:100"),
    });

    if (isAv1) {
        lines.append({
            QStringLiteral("a=vqos.drc.minQpHeadroom:20"),
            QStringLiteral("a=vqos.drc.lowerQpThreshold:100"),
            QStringLiteral("a=vqos.drc.upperQpThreshold:200"),
            QStringLiteral("a=vqos.drc.minAdaptiveQpThreshold:180"),
            QStringLiteral("a=vqos.drc.qpCodecThresholdAdj:0"),
            QStringLiteral("a=vqos.drc.qpMaxResThresholdAdj:20"),
            QStringLiteral("a=vqos.dfc.minQpHeadroom:20"),
            QStringLiteral("a=vqos.dfc.qpLowerLimit:100"),
            QStringLiteral("a=vqos.dfc.qpMaxUpperLimit:200"),
            QStringLiteral("a=vqos.dfc.qpMinUpperLimit:180"),
            QStringLiteral("a=vqos.dfc.qpMaxResThresholdAdj:20"),
            QStringLiteral("a=vqos.dfc.qpCodecThresholdAdj:0"),
            QStringLiteral("a=vqos.grc.minQpHeadroom:20"),
            QStringLiteral("a=vqos.grc.lowerQpThreshold:100"),
            QStringLiteral("a=vqos.grc.upperQpThreshold:200"),
            QStringLiteral("a=vqos.grc.minAdaptiveQpThreshold:180"),
            QStringLiteral("a=vqos.grc.qpMaxResThresholdAdj:20"),
            QStringLiteral("a=vqos.grc.qpCodecThresholdAdj:0"),
            QStringLiteral("a=video.minQp:25"),
            QStringLiteral("a=video.enableAv1RcPrecisionFactor:1"),
        });
    }

    lines.append({
        QStringLiteral("a=video.clientViewportWd:%1").arg(width),
        QStringLiteral("a=video.clientViewportHt:%1").arg(height),
        QStringLiteral("a=video.maxFPS:%1").arg(settings.fps),
        QStringLiteral("a=video.initialBitrateKbps:%1").arg(initialBitrateKbps),
        QStringLiteral("a=video.initialPeakBitrateKbps:%1").arg(maxBitrateKbps),
        QStringLiteral("a=vqos.bw.maximumBitrateKbps:%1").arg(maxBitrateKbps),
        QStringLiteral("a=vqos.bw.minimumBitrateKbps:%1").arg(minBitrateKbps),
        QStringLiteral("a=vqos.bw.peakBitrateKbps:%1").arg(maxBitrateKbps),
        QStringLiteral("a=vqos.bw.serverPeakBitrateKbps:%1").arg(maxBitrateKbps),
        QStringLiteral("a=vqos.bw.enableBandwidthEstimation:1"),
        QStringLiteral("a=vqos.bw.disableBitrateLimit:0"),
        QStringLiteral("a=vqos.grc.maximumBitrateKbps:%1").arg(maxBitrateKbps),
        QStringLiteral("a=vqos.grc.enable:0"),
        QStringLiteral("a=video.maxNumReferenceFrames:4"),
        QStringLiteral("a=video.mapRtpTimestampsToFrames:1"),
        QStringLiteral("a=video.encoderCscMode:3"),
        QStringLiteral("a=video.dynamicRangeMode:0"),
        QStringLiteral("a=video.bitDepth:%1").arg(bitDepth),
        QStringLiteral("a=video.scalingFeature1:%1").arg(isAv1 ? 1 : 0),
        QStringLiteral("a=video.prefilterParams.prefilterModel:0"),
        QStringLiteral("m=audio 0 RTP/AVP"),
        QStringLiteral("a=msid:audio"),
        QStringLiteral("m=mic 0 RTP/AVP"),
        QStringLiteral("a=msid:mic"),
        QStringLiteral("a=rtpmap:0 PCMU/8000"),
        QStringLiteral("m=application 0 RTP/AVP"),
        QStringLiteral("a=msid:input_1"),
        QStringLiteral("a=ri.partialReliableThresholdMs:%1").arg(partialReliableInputLifetimeMs),
        QStringLiteral("a=ri.hidDeviceMask:4294967295"),
        QStringLiteral("a=ri.enablePartiallyReliableTransferGamepad:15"),
        QStringLiteral("a=ri.enablePartiallyReliableTransferHid:4294967295"),
        QString(),
    });

    QString result;
    for (const QString &line : lines) {
        result += line;
        result += QLatin1Char('\n');
    }
    return result;
}

int clampColor(int value) {
    return std::clamp(value, 0, 255);
}

QImage i420ToImage(const webrtc::I420BufferInterface &buffer) {
    const int width = buffer.width();
    const int height = buffer.height();
    if (width <= 0 || height <= 0) return {};

    QImage image(width, height, QImage::Format_RGB888);
    if (image.isNull()) return {};

    const uint8_t *yPlane = buffer.DataY();
    const uint8_t *uPlane = buffer.DataU();
    const uint8_t *vPlane = buffer.DataV();
    const int yStride = buffer.StrideY();
    const int uStride = buffer.StrideU();
    const int vStride = buffer.StrideV();

    for (int y = 0; y < height; ++y) {
        uchar *dst = image.scanLine(y);
        const uint8_t *yRow = yPlane + y * yStride;
        const uint8_t *uRow = uPlane + (y / 2) * uStride;
        const uint8_t *vRow = vPlane + (y / 2) * vStride;
        for (int x = 0; x < width; ++x) {
            const int c = static_cast<int>(yRow[x]) - 16;
            const int d = static_cast<int>(uRow[x / 2]) - 128;
            const int e = static_cast<int>(vRow[x / 2]) - 128;
            dst[x * 3 + 0] = static_cast<uchar>(clampColor((298 * c + 409 * e + 128) >> 8));
            dst[x * 3 + 1] = static_cast<uchar>(clampColor((298 * c - 100 * d - 208 * e + 128) >> 8));
            dst[x * 3 + 2] = static_cast<uchar>(clampColor((298 * c + 516 * d + 128) >> 8));
        }
    }
    return image;
}

class RemoteVideoSink final : public webrtc::VideoSinkInterface<webrtc::VideoFrame> {
public:
    explicit RemoteVideoSink(WebRtcStreamSession *owner) : m_owner(owner) {}

    void setOwner(WebRtcStreamSession *owner) { m_owner = owner; }

    void OnFrame(const webrtc::VideoFrame &frame) override {
        WebRtcStreamSession *owner = m_owner;
        if (!owner) return;
        webrtc::scoped_refptr<webrtc::I420BufferInterface> i420 = frame.video_frame_buffer() ? frame.video_frame_buffer()->ToI420() : nullptr;
        if (!i420) return;
        QImage image = i420ToImage(*i420);
        if (image.isNull()) return;
        QMetaObject::invokeMethod(owner, [owner, image = std::move(image)]() {
            owner->deliverVideoFrame(image);
        }, Qt::QueuedConnection);
    }

private:
    WebRtcStreamSession *m_owner = nullptr;
};

class InputDataChannelObserver final : public webrtc::DataChannelObserver {
public:
    InputDataChannelObserver(WebRtcStreamSession *owner, QString label) : m_owner(owner), m_label(std::move(label)) {}

    void setOwner(WebRtcStreamSession *owner) { m_owner = owner; }
    void OnStateChange() override;
    void OnMessage(const webrtc::DataBuffer &buffer) override;

private:
    WebRtcStreamSession *m_owner = nullptr;
    QString m_label;
};

class SetDescriptionObserver : public webrtc::SetSessionDescriptionObserver {
public:
    using Callback = std::function<void(const QString &)>;

    explicit SetDescriptionObserver(Callback callback) : m_callback(std::move(callback)) {}

    void OnSuccess() override {
        if (m_callback) m_callback(QString());
    }

    void OnFailure(webrtc::RTCError error) override {
        if (m_callback) m_callback(rtcErrorMessage(error));
    }

private:
    Callback m_callback;
};

class CreateDescriptionObserver : public webrtc::CreateSessionDescriptionObserver {
public:
    using Callback = std::function<void(std::unique_ptr<webrtc::SessionDescriptionInterface>, const QString &)>;

    explicit CreateDescriptionObserver(Callback callback) : m_callback(std::move(callback)) {}

    void OnSuccess(webrtc::SessionDescriptionInterface *description) override {
        if (m_callback) m_callback(std::unique_ptr<webrtc::SessionDescriptionInterface>(description), QString());
    }

    void OnFailure(webrtc::RTCError error) override {
        if (m_callback) m_callback(nullptr, rtcErrorMessage(error));
    }

private:
    Callback m_callback;
};

} // namespace

struct WebRtcStreamSession::Impl final : public webrtc::PeerConnectionObserver {
    explicit Impl(WebRtcStreamSession *owner) : owner(owner) {}

    void OnSignalingChange(webrtc::PeerConnectionInterface::SignalingState) override {}
    void OnDataChannel(webrtc::scoped_refptr<webrtc::DataChannelInterface>) override {}
    void OnRenegotiationNeeded() override {}
    void OnIceGatheringChange(webrtc::PeerConnectionInterface::IceGatheringState) override {}
    void OnIceConnectionReceivingChange(bool) override {}
    void OnAddStream(webrtc::scoped_refptr<webrtc::MediaStreamInterface>) override {}
    void OnRemoveStream(webrtc::scoped_refptr<webrtc::MediaStreamInterface>) override {}
    void OnTrack(webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) override {
        if (!owner || !transceiver || !transceiver->receiver()) return;
        webrtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track = transceiver->receiver()->track();
        if (!track || track->kind() != webrtc::MediaStreamTrackInterface::kVideoKind) return;
        remoteVideoTrack = webrtc::scoped_refptr<webrtc::VideoTrackInterface>(static_cast<webrtc::VideoTrackInterface *>(track.get()));
        if (!remoteVideoSink) remoteVideoSink = std::make_unique<RemoteVideoSink>(owner);
        webrtc::VideoSinkWants wants;
        remoteVideoTrack->AddOrUpdateSink(remoteVideoSink.get(), wants);
        qInfo() << "[WebRTC] Remote video track attached";
    }
    void OnRemoveTrack(webrtc::scoped_refptr<webrtc::RtpReceiverInterface>) override {}
    void OnAddTrack(webrtc::scoped_refptr<webrtc::RtpReceiverInterface>, const std::vector<webrtc::scoped_refptr<webrtc::MediaStreamInterface>> &) override {}

    void OnIceConnectionChange(webrtc::PeerConnectionInterface::IceConnectionState state) override {
        if (!owner || !owner->m_onState) return;
        if (state == webrtc::PeerConnectionInterface::kIceConnectionConnected || state == webrtc::PeerConnectionInterface::kIceConnectionCompleted) {
            owner->m_onState(true, QString());
        } else if (state == webrtc::PeerConnectionInterface::kIceConnectionFailed) {
            owner->m_onState(false, QStringLiteral("ICE connection failed"));
        }
    }

    void OnIceCandidate(const webrtc::IceCandidateInterface *candidate) override {
        if (!owner || !owner->m_onIceCandidate || !candidate) return;
        std::string sdp;
        if (!candidate->ToString(&sdp)) return;
        IceCandidatePayload payload;
        payload.candidate = fromStdString(sdp);
        payload.sdpMid = fromStdString(candidate->sdp_mid());
        payload.sdpMLineIndex = candidate->sdp_mline_index();
        owner->m_onIceCandidate(payload);
    }

    bool reliableOpen() const {
        return reliableInputChannel && reliableInputChannel->state() == webrtc::DataChannelInterface::kOpen;
    }

    bool partialOpen() const {
        return partialInputChannel && partialInputChannel->state() == webrtc::DataChannelInterface::kOpen;
    }

    void updateInputReady() {
        if (!reliableOpen() || !partialOpen()) inputReady = false;
        qInfo() << "[WebRTC] Input channels" << "reliable" << reliableOpen() << "partial" << partialOpen() << "ready" << inputReady;
    }

    void createInputChannels() {
        if (!peerConnection || reliableInputChannel || partialInputChannel) return;

        webrtc::DataChannelInit reliableConfig;
        reliableConfig.ordered = true;
        auto reliableResult = peerConnection->CreateDataChannelOrError("input_channel_v1", &reliableConfig);
        if (!reliableResult.ok()) qWarning() << "[WebRTC] reliable input data channel failed" << rtcErrorMessage(reliableResult.error());
        reliableInputChannel = reliableResult.ok() ? reliableResult.MoveValue() : nullptr;
        if (reliableInputChannel) {
            reliableObserver = std::make_unique<InputDataChannelObserver>(owner, QStringLiteral("input_channel_v1"));
            reliableInputChannel->RegisterObserver(reliableObserver.get());
        }

        webrtc::DataChannelInit partialConfig;
        partialConfig.ordered = false;
        partialConfig.maxRetransmits = absl::nullopt;
        partialConfig.maxRetransmitTime = kPartialReliableInputLifetimeMs;
        auto partialResult = peerConnection->CreateDataChannelOrError("input_channel_partially_reliable", &partialConfig);
        if (!partialResult.ok()) qWarning() << "[WebRTC] partial input data channel failed" << rtcErrorMessage(partialResult.error());
        partialInputChannel = partialResult.ok() ? partialResult.MoveValue() : nullptr;
        if (partialInputChannel) {
            partialObserver = std::make_unique<InputDataChannelObserver>(owner, QStringLiteral("input_channel_partially_reliable"));
            partialInputChannel->RegisterObserver(partialObserver.get());
        }
    }

    void sendInput(const std::vector<uint8_t> &payload, bool partial) {
        webrtc::scoped_refptr<webrtc::DataChannelInterface> channel = partial ? partialInputChannel : reliableInputChannel;
        if (!channel || channel->state() != webrtc::DataChannelInterface::kOpen || payload.empty()) return;
        webrtc::CopyOnWriteBuffer buffer(payload.data(), payload.size());
        channel->Send(webrtc::DataBuffer(buffer, true));
    }

    WebRtcStreamSession *owner = nullptr;
    std::unique_ptr<webrtc::Thread> networkThread;
    std::unique_ptr<webrtc::Thread> workerThread;
    std::unique_ptr<webrtc::Thread> signalingThread;
    webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> factory;
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peerConnection;
    webrtc::scoped_refptr<webrtc::VideoTrackInterface> remoteVideoTrack;
    webrtc::scoped_refptr<webrtc::DataChannelInterface> reliableInputChannel;
    webrtc::scoped_refptr<webrtc::DataChannelInterface> partialInputChannel;
    std::unique_ptr<RemoteVideoSink> remoteVideoSink;
    std::unique_ptr<InputDataChannelObserver> reliableObserver;
    std::unique_ptr<InputDataChannelObserver> partialObserver;
    bool inputReady = false;
};

void InputDataChannelObserver::OnStateChange() {
    Q_UNUSED(m_label);
    if (m_owner) QMetaObject::invokeMethod(m_owner, [owner = m_owner]() { owner->handleInputDataChannelStateChanged(); }, Qt::QueuedConnection);
}

void InputDataChannelObserver::OnMessage(const webrtc::DataBuffer &buffer) {
    if (!m_owner) return;
    QByteArray data(reinterpret_cast<const char *>(buffer.data.data()), static_cast<qsizetype>(buffer.data.size()));
    QMetaObject::invokeMethod(m_owner, [owner = m_owner, label = m_label, data = std::move(data)]() { owner->handleInputDataChannelMessage(label, data); }, Qt::QueuedConnection);
}
#else
struct WebRtcStreamSession::Impl final {
};
#endif

WebRtcStreamSession::WebRtcStreamSession(QObject *parent) : QObject(parent) {
    m_inputHeartbeatTimer.setInterval(2000);
    m_inputHeartbeatTimer.setTimerType(Qt::CoarseTimer);
    connect(&m_inputHeartbeatTimer, &QTimer::timeout, this, [this]() {
        if (m_impl && m_impl->inputReady) m_impl->sendInput(encodeHeartbeat(), false);
    });
}

WebRtcStreamSession::~WebRtcStreamSession() {
    stop();
}

bool WebRtcStreamSession::isAvailable() {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    return true;
#else
    return false;
#endif
}

QString WebRtcStreamSession::availabilityDescription() {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    return QStringLiteral("native C++ libwebrtc enabled");
#else
    return QStringLiteral("build without OPNQT_HAVE_LIBWEBRTC; install native C++ SDK under third_party/libwebrtc and configure with OPNQT_ENABLE_LIBWEBRTC=ON");
#endif
}

void WebRtcStreamSession::start(const SessionInfo &session,
                                const QString &offerSdp,
                                const StreamSettings &settings,
                                StreamStateCallback onState) {
    stop();
    m_settings = settings;
    m_onState = std::move(onState);

#if defined(OPNQT_HAVE_LIBWEBRTC)
    m_impl = std::make_unique<Impl>(this);
    webrtc::InitializeSSL();

    m_impl->networkThread = webrtc::Thread::CreateWithSocketServer();
    m_impl->workerThread = webrtc::Thread::Create();
    m_impl->signalingThread = webrtc::Thread::Create();
    if (!m_impl->networkThread || !m_impl->workerThread || !m_impl->signalingThread) {
        if (m_onState) m_onState(false, QStringLiteral("failed to create libwebrtc threads"));
        stop();
        return;
    }
    m_impl->networkThread->Start();
    m_impl->workerThread->Start();
    m_impl->signalingThread->Start();

    m_impl->factory = webrtc::CreatePeerConnectionFactory(
        m_impl->networkThread.get(),
        m_impl->workerThread.get(),
        m_impl->signalingThread.get(),
        nullptr,
        webrtc::CreateBuiltinAudioEncoderFactory(),
        webrtc::CreateBuiltinAudioDecoderFactory(),
        std::make_unique<webrtc::VideoEncoderFactoryTemplate<webrtc::LibvpxVp8EncoderTemplateAdapter,
                                                             webrtc::LibvpxVp9EncoderTemplateAdapter,
                                                             webrtc::OpenH264EncoderTemplateAdapter,
                                                             webrtc::LibaomAv1EncoderTemplateAdapter>>(),
        std::make_unique<webrtc::VideoDecoderFactoryTemplate<webrtc::LibvpxVp8DecoderTemplateAdapter,
                                                             webrtc::LibvpxVp9DecoderTemplateAdapter,
                                                             webrtc::OpenH264DecoderTemplateAdapter,
                                                             webrtc::Dav1dDecoderTemplateAdapter>>(),
        nullptr,
        nullptr);
    if (!m_impl->factory) {
        if (m_onState) m_onState(false, QStringLiteral("failed to create libwebrtc peer connection factory"));
        stop();
        return;
    }

    webrtc::PeerConnectionInterface::RTCConfiguration configuration;
    configuration.sdp_semantics = webrtc::SdpSemantics::kUnifiedPlan;
    configuration.bundle_policy = webrtc::PeerConnectionInterface::kBundlePolicyMaxBundle;
    configuration.rtcp_mux_policy = webrtc::PeerConnectionInterface::kRtcpMuxPolicyRequire;
    configuration.tcp_candidate_policy = webrtc::PeerConnectionInterface::kTcpCandidatePolicyDisabled;
    configuration.continual_gathering_policy = webrtc::PeerConnectionInterface::GATHER_ONCE;
    for (const IceServer &server : session.iceServers) {
        webrtc::PeerConnectionInterface::IceServer iceServer;
        for (const QString &url : server.urls) iceServer.urls.push_back(toStdString(url));
        if (iceServer.urls.empty()) continue;
        iceServer.username = toStdString(server.username);
        iceServer.password = toStdString(server.credential);
        configuration.servers.push_back(iceServer);
    }

    auto peerResult = m_impl->factory->CreatePeerConnectionOrError(configuration, webrtc::PeerConnectionDependencies(m_impl.get()));
    if (!peerResult.ok()) {
        if (m_onState) m_onState(false, rtcErrorMessage(peerResult.error()));
        stop();
        return;
    }
    m_impl->peerConnection = peerResult.MoveValue();
    m_impl->createInputChannels();

    webrtc::SdpParseError parseError;
    std::unique_ptr<webrtc::SessionDescriptionInterface> offer = webrtc::CreateSessionDescription(webrtc::SdpType::kOffer, toStdString(offerSdp), &parseError);
    if (!offer) {
        if (m_onState) m_onState(false, QStringLiteral("failed to parse offer SDP: %1").arg(fromStdString(parseError.description)));
        stop();
        return;
    }

    m_impl->peerConnection->SetRemoteDescription(webrtc::make_ref_counted<SetDescriptionObserver>([this](const QString &remoteError) {
        if (!remoteError.isEmpty()) {
            if (m_onState) m_onState(false, QStringLiteral("setRemoteDescription failed: %1").arg(remoteError));
            return;
        }
        webrtc::PeerConnectionInterface::RTCOfferAnswerOptions options;
        m_impl->peerConnection->CreateAnswer(webrtc::make_ref_counted<CreateDescriptionObserver>([this](std::unique_ptr<webrtc::SessionDescriptionInterface> answer, const QString &answerError) mutable {
            if (!answer) {
                if (m_onState) m_onState(false, QStringLiteral("createAnswer failed: %1").arg(answerError));
                return;
            }
            std::string sdp;
            if (!answer->ToString(&sdp)) {
                if (m_onState) m_onState(false, QStringLiteral("createAnswer produced invalid SDP"));
                return;
            }
            const QString answerSdp = fromStdString(sdp);
            m_impl->peerConnection->SetLocalDescription(webrtc::make_ref_counted<SetDescriptionObserver>([this, answerSdp](const QString &localError) {
                if (!localError.isEmpty()) {
                    if (m_onState) m_onState(false, QStringLiteral("setLocalDescription failed: %1").arg(localError));
                    return;
                }
                SendAnswerRequest request;
                request.sdp = answerSdp;
                request.nvstSdp = buildNvstSdp(m_settings, extractIceCredentials(answerSdp));
                if (m_onAnswer) m_onAnswer(request);
            }).get(), answer.release());
        }).get(), options);
    }).get(), offer.release());
#else
    Q_UNUSED(session);
    Q_UNUSED(offerSdp);
    const QString error = availabilityDescription();
    qWarning() << "[WebRTC]" << error;
    if (m_onState) m_onState(false, error);
#endif
}

void WebRtcStreamSession::stop() {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (m_impl && m_impl->remoteVideoTrack && m_impl->remoteVideoSink) {
        m_impl->remoteVideoTrack->RemoveSink(m_impl->remoteVideoSink.get());
        m_impl->remoteVideoSink->setOwner(nullptr);
    }
    if (m_impl && m_impl->reliableInputChannel) m_impl->reliableInputChannel->UnregisterObserver();
    if (m_impl && m_impl->partialInputChannel) m_impl->partialInputChannel->UnregisterObserver();
    if (m_impl && m_impl->reliableObserver) m_impl->reliableObserver->setOwner(nullptr);
    if (m_impl && m_impl->partialObserver) m_impl->partialObserver->setOwner(nullptr);
    m_inputHeartbeatTimer.stop();
    m_inputProtocolVersion = 2;
    if (m_impl && m_impl->peerConnection) {
        m_impl->peerConnection->Close();
    }
    if (m_impl) m_impl->owner = nullptr;
#endif
    m_impl.reset();
}

void WebRtcStreamSession::addRemoteIceCandidate(const IceCandidatePayload &candidate) {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (!m_impl || !m_impl->peerConnection || candidate.candidate.isEmpty()) return;
    webrtc::SdpParseError error;
    std::unique_ptr<webrtc::IceCandidateInterface> iceCandidate(webrtc::CreateIceCandidate(toStdString(candidate.sdpMid), candidate.sdpMLineIndex, toStdString(candidate.candidate), &error));
    if (!iceCandidate) {
        qWarning() << "[WebRTC] failed to parse remote ICE" << fromStdString(error.description);
        return;
    }
    m_impl->peerConnection->AddIceCandidate(std::move(iceCandidate), [](webrtc::RTCError addError) {
        if (!addError.ok()) qWarning() << "[WebRTC] addIceCandidate failed" << rtcErrorMessage(addError);
    });
#else
    Q_UNUSED(candidate);
#endif
}

bool WebRtcStreamSession::inputReady() const {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    return m_impl && m_impl->inputReady;
#else
    return false;
#endif
}

void WebRtcStreamSession::sendKeyEvent(quint16 keycode, quint16 scancode, quint16 modifiers, bool down) {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (!m_impl || !m_impl->inputReady) return;
    m_impl->sendInput(encodeKey(down ? kInputKeyDown : kInputKeyUp, keycode, scancode, modifiers, m_inputProtocolVersion), false);
#else
    Q_UNUSED(keycode);
    Q_UNUSED(scancode);
    Q_UNUSED(modifiers);
    Q_UNUSED(down);
#endif
}

void WebRtcStreamSession::sendMouseMove(qint16 dx, qint16 dy) {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (!m_impl || !m_impl->inputReady) return;
    m_impl->sendInput(encodeMouseMove(dx, dy, m_inputProtocolVersion), true);
#else
    Q_UNUSED(dx);
    Q_UNUSED(dy);
#endif
}

void WebRtcStreamSession::sendMouseButton(quint8 button, bool down) {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (!m_impl || !m_impl->inputReady) return;
    m_impl->sendInput(encodeMouseButton(down ? kInputMouseButtonDown : kInputMouseButtonUp, button, m_inputProtocolVersion), false);
#else
    Q_UNUSED(button);
    Q_UNUSED(down);
#endif
}

void WebRtcStreamSession::sendMouseWheel(qint16 delta) {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (!m_impl || !m_impl->inputReady) return;
    m_impl->sendInput(encodeMouseWheel(delta, m_inputProtocolVersion), false);
#else
    Q_UNUSED(delta);
#endif
}

void WebRtcStreamSession::onAnswerReady(StreamAnswerCallback callback) {
    m_onAnswer = std::move(callback);
}

void WebRtcStreamSession::onIceCandidateReady(StreamLocalIceCallback callback) {
    m_onIceCandidate = std::move(callback);
}

void WebRtcStreamSession::onVideoFrameReady(StreamVideoFrameCallback callback) {
    m_onVideoFrame = std::move(callback);
}

void WebRtcStreamSession::deliverVideoFrame(const QImage &frame) {
    if (m_onVideoFrame) m_onVideoFrame(frame);
}

void WebRtcStreamSession::handleInputDataChannelStateChanged() {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (m_impl) m_impl->updateInputReady();
#endif
}

void WebRtcStreamSession::handleInputDataChannelMessage(const QString &label, const QByteArray &data) {
#if defined(OPNQT_HAVE_LIBWEBRTC)
    if (!m_impl || label != QLatin1String("input_channel_v1") || data.size() < 2 || m_impl->inputReady) return;
    const quint16 firstWord = static_cast<quint8>(data[0]) | (static_cast<quint16>(static_cast<quint8>(data[1])) << 8);
    quint16 version = 2;
    if (firstWord == 526) {
        if (data.size() >= 4) version = static_cast<quint8>(data[2]) | (static_cast<quint16>(static_cast<quint8>(data[3])) << 8);
    } else if (static_cast<quint8>(data[0]) == 0x0e) {
        version = firstWord;
    } else {
        qInfo() << "[WebRTC] input channel message before handshake" << "len" << data.size() << "firstWord" << firstWord;
        return;
    }
    m_inputProtocolVersion = version == 0 ? 2 : version;
    m_impl->inputReady = m_impl->reliableOpen() && m_impl->partialOpen();
    std::vector<uint8_t> echo(static_cast<size_t>(data.size()));
    std::copy(data.begin(), data.end(), echo.begin());
    m_impl->sendInput(echo, false);
    if (m_impl->inputReady && !m_inputHeartbeatTimer.isActive()) m_inputHeartbeatTimer.start();
    m_impl->updateInputReady();
    qInfo() << "[WebRTC] input handshake complete" << "protocol" << m_inputProtocolVersion << "ready" << m_impl->inputReady;
#else
    Q_UNUSED(label);
    Q_UNUSED(data);
#endif
}

} // namespace OpnQt
