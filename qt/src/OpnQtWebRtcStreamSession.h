#pragma once

#include "OpnQtStreamTypes.h"

#include <QtCore/QObject>
#include <functional>

namespace OpnQt {

using StreamStateCallback = std::function<void(bool connected, const QString &error)>;
using StreamAnswerCallback = std::function<void(const SendAnswerRequest &answer)>;
using StreamLocalIceCallback = std::function<void(const IceCandidatePayload &candidate)>;

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
    void onAnswerReady(StreamAnswerCallback callback);
    void onIceCandidateReady(StreamLocalIceCallback callback);

private:
    void *m_impl = nullptr;
    StreamSettings m_settings;
    StreamAnswerCallback m_onAnswer;
    StreamLocalIceCallback m_onIceCandidate;
    StreamStateCallback m_onState;
};

} // namespace OpnQt
