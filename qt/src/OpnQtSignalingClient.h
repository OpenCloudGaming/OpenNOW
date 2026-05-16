#pragma once

#include "OpnQtStreamTypes.h"

#include <QtCore/QObject>
#include <QtCore/QTimer>
#include <functional>

class QWebSocket;

namespace OpnQt {

using SignalingOfferCallback = std::function<void(const QString &sdp)>;
using SignalingIceCallback = std::function<void(const IceCandidatePayload &candidate)>;
using SignalingConnectCallback = std::function<void(bool success, const QString &error)>;

class SignalingClient final : public QObject {
public:
    SignalingClient(const QString &signalingServer,
                    const QString &sessionId,
                    const QString &signalingUrl,
                    QObject *parent = nullptr);
    ~SignalingClient() override;

    void connectToServer(SignalingConnectCallback onConnect);
    void disconnectFromServer();
    void sendAnswer(const SendAnswerRequest &answer);
    void sendIceCandidate(const IceCandidatePayload &candidate);
    void setPeerResolution(const QString &resolution);
    void onOffer(SignalingOfferCallback callback);
    void onIceCandidate(SignalingIceCallback callback);
    bool isConnected() const;

private:
    QUrl buildSignInUrl() const;
    void sendJson(const QJsonObject &json);
    void sendPeerInfo();
    void handleMessage(const QString &text);
    void setupHeartbeat();

    QString m_signalingServer;
    QString m_sessionId;
    QString m_signalingUrl;
    int m_peerId = 0;
    int m_remotePeerId = 1;
    int m_ackCounter = 0;
    QString m_peerName;
    QString m_peerResolution = QStringLiteral("1920x1080");
    SignalingOfferCallback m_onOffer;
    SignalingIceCallback m_onIceCandidate;
    QWebSocket *m_socket = nullptr;
    QTimer m_heartbeat;
};

} // namespace OpnQt
