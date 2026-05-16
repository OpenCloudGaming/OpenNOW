#include "OpnQtWebRtcStreamSession.h"

#include <QtCore/QDebug>

#if defined(OPNQT_HAVE_LIBWEBRTC)
#include <api/create_peerconnection_factory.h>
#include <api/peer_connection_interface.h>
#include <api/scoped_refptr.h>
#include <api/task_queue/default_task_queue_factory.h>
#include <rtc_base/ssl_adapter.h>
#include <rtc_base/thread.h>
#endif

namespace OpnQt {

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
    return QStringLiteral("native libwebrtc enabled");
#else
    return QStringLiteral("build without OPNQT_HAVE_LIBWEBRTC");
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
    Q_UNUSED(session);
    Q_UNUSED(offerSdp);
    const QString error = QStringLiteral("native libwebrtc peer connection implementation is not wired yet");
    qWarning() << "[WebRTC]" << error;
    if (m_onState) m_onState(false, error);
#else
    Q_UNUSED(session);
    Q_UNUSED(offerSdp);
    const QString error = availabilityDescription();
    qWarning() << "[WebRTC]" << error;
    if (m_onState) m_onState(false, error);
#endif
}

void WebRtcStreamSession::stop() {
    m_impl = nullptr;
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
