#pragma once

#include "OPNStreamTypes.h"
#include <string>
#include <functional>

namespace OPN {

using SignalingOfferCallback = std::function<void(const std::string &sdp)>;
using SignalingIceCallback = std::function<void(const IceCandidatePayload &candidate)>;
using SignalingConnectCallback = std::function<void(bool success, const std::string &error)>;

class SignalingClient {
public:
    SignalingClient(const std::string &signalingServer,
                    const std::string &sessionId,
                    const std::string &signalingUrl);
    ~SignalingClient();

    void Connect(SignalingConnectCallback onConnect);
    void Disconnect();
    void SendAnswer(const SendAnswerRequest &answer);
    void SendIceCandidate(const IceCandidatePayload &candidate);
    void SetPeerResolution(const std::string &resolution);

    void OnOffer(SignalingOfferCallback cb);
    void OnIceCandidate(SignalingIceCallback cb);

    bool IsConnected() const;

private:
    std::string m_signalingServer;
    std::string m_sessionId;
    std::string m_signalingUrl;
    int m_peerId = 0;
    int m_remotePeerId = 1;
    int m_ackCounter = 0;
    std::string m_peerName;
    std::string m_peerResolution = "1920x1080";

    SignalingOfferCallback m_onOffer;
    SignalingIceCallback m_onIceCandidate;

    void *m_webSocketTask = nullptr;
    void *m_urlSession = nullptr;
    void *m_delegate = nullptr;
    void *m_heartbeatSource = nullptr;
    int m_connectionGeneration = 0;
    bool m_didOpen = false;

    void SetupHeartbeat();
    void ClearHeartbeat();
    void HandleMessage(const std::string &text);
    void SendJson(const std::string &json);
    void SendPeerInfo();
    void RearmReceiveHandler();

    bool IsCurrentGeneration(int generation) const;
};

}
