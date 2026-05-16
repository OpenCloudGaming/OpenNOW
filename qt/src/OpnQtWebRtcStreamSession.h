#pragma once

#include "OpnQtStreamTypes.h"

#include <QtGui/QImage>
#include <QtCore/QObject>
#include <QtCore/QTimer>
#include <functional>
#include <memory>

namespace OpnQt {

using StreamStateCallback = std::function<void(bool connected, const QString &error)>;
using StreamAnswerCallback = std::function<void(const SendAnswerRequest &answer)>;
using StreamLocalIceCallback = std::function<void(const IceCandidatePayload &candidate)>;
using StreamVideoFrameCallback = std::function<void(const QImage &frame)>;

class WebRtcStreamSession final : public QObject {
public:
    explicit WebRtcStreamSession(QObject *parent = nullptr);
    ~WebRtcStreamSession() override;

    static bool isAvailable();
    static QString availabilityDescription();

    void start(const SessionInfo &session,
               const QString &offerSdp,
               const StreamSettings &settings,
               StreamStateCallback onState);
    void stop();
    void addRemoteIceCandidate(const IceCandidatePayload &candidate);
    bool inputReady() const;
    void sendKeyEvent(quint16 keycode, quint16 scancode, quint16 modifiers, bool down);
    void sendMouseMove(qint16 dx, qint16 dy);
    void sendMouseButton(quint8 button, bool down);
    void sendMouseWheel(qint16 delta);
    void onAnswerReady(StreamAnswerCallback callback);
    void onIceCandidateReady(StreamLocalIceCallback callback);
    void onVideoFrameReady(StreamVideoFrameCallback callback);
    void deliverVideoFrame(const QImage &frame);
    void handleInputDataChannelStateChanged();
    void handleInputDataChannelMessage(const QString &label, const QByteArray &data);

private:
    struct Impl;
    std::unique_ptr<Impl> m_impl;
    StreamSettings m_settings;
    StreamAnswerCallback m_onAnswer;
    StreamLocalIceCallback m_onIceCandidate;
    StreamVideoFrameCallback m_onVideoFrame;
    StreamStateCallback m_onState;
    QTimer m_inputHeartbeatTimer;
    quint16 m_inputProtocolVersion = 2;
};

} // namespace OpnQt
