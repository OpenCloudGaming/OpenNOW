#include "OpnQtWebRtcStreamSession.h"

#include <QtCore/QDebug>

#if defined(OPNQT_HAVE_LIBWEBRTC)
#include <QtCore/QStringList>

#include <api/create_peerconnection_factory.h>
#include <api/audio_codecs/builtin_audio_decoder_factory.h>
#include <api/audio_codecs/builtin_audio_encoder_factory.h>
#include <api/jsep.h>
#include <api/make_ref_counted.h>
#include <api/peer_connection_interface.h>
#include <api/scoped_refptr.h>
#include <api/task_queue/default_task_queue_factory.h>
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
    void OnTrack(webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>) override {}
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

    WebRtcStreamSession *owner = nullptr;
    std::unique_ptr<webrtc::Thread> networkThread;
    std::unique_ptr<webrtc::Thread> workerThread;
    std::unique_ptr<webrtc::Thread> signalingThread;
    webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> factory;
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peerConnection;
};
#else
struct WebRtcStreamSession::Impl final {
};
#endif

WebRtcStreamSession::WebRtcStreamSession(QObject *parent) : QObject(parent) {}

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

void WebRtcStreamSession::onAnswerReady(StreamAnswerCallback callback) {
    m_onAnswer = std::move(callback);
}

void WebRtcStreamSession::onIceCandidateReady(StreamLocalIceCallback callback) {
    m_onIceCandidate = std::move(callback);
}

} // namespace OpnQt
