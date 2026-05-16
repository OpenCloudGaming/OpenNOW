#include "OpnQtWebRtcStreamSession.h"

#include <QtCore/QDebug>

#if defined(OPNQT_HAVE_LIBWEBRTC)
#include <api/create_peerconnection_factory.h>
#include <api/jsep.h>
#include <api/peer_connection_interface.h>
#include <api/scoped_refptr.h>
#include <api/task_queue/default_task_queue_factory.h>
#include <media/base/media_engine.h>
#include <modules/audio_device/include/audio_device.h>
#include <pc/video_track_source.h>
#include <rtc_base/logging.h>
#include <rtc_base/ssl_adapter.h>
#include <rtc_base/thread.h>

#include <memory>
#include <string>
#endif

namespace OpnQt {

#if defined(OPNQT_HAVE_LIBWEBRTC)
struct WebRtcStreamSession::Impl final {
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
    Q_UNUSED(session);
    Q_UNUSED(offerSdp);

#if defined(OPNQT_HAVE_LIBWEBRTC)
    const QString error = QStringLiteral("native C++ libwebrtc peer connection wiring is next");
    qWarning() << "[WebRTC]" << error;
    if (m_onState) m_onState(false, error);
#else
    const QString error = availabilityDescription();
    qWarning() << "[WebRTC]" << error;
    if (m_onState) m_onState(false, error);
#endif
}

void WebRtcStreamSession::stop() {
    m_impl.reset();
}

void WebRtcStreamSession::addRemoteIceCandidate(const IceCandidatePayload &candidate) {
    Q_UNUSED(candidate);
}

void WebRtcStreamSession::onAnswerReady(StreamAnswerCallback callback) {
    m_onAnswer = std::move(callback);
}

void WebRtcStreamSession::onIceCandidateReady(StreamLocalIceCallback callback) {
    m_onIceCandidate = std::move(callback);
}

} // namespace OpnQt
