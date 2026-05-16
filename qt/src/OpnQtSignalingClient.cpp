#include "OpnQtSignalingClient.h"

#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QRandomGenerator>
#include <QtCore/QUrl>
#include <QtCore/QUrlQuery>
#include <QtNetwork/QNetworkRequest>
#include <QtWebSockets/QWebSocket>

namespace OpnQt {

SignalingClient::SignalingClient(const QString &signalingServer,
                                 const QString &sessionId,
                                 const QString &signalingUrl,
                                 QObject *parent)
    : QObject(parent)
    , m_signalingServer(signalingServer)
    , m_sessionId(sessionId)
    , m_signalingUrl(signalingUrl)
    , m_socket(new QWebSocket(QString(), QWebSocketProtocol::VersionLatest, this)) {
    m_heartbeat.setInterval(5000);
    connect(&m_heartbeat, &QTimer::timeout, this, [this]() {
        sendJson(QJsonObject{{QStringLiteral("hb"), 1}});
    });
    connect(m_socket, &QWebSocket::textMessageReceived, this, [this](const QString &message) {
        handleMessage(message);
    });
}

SignalingClient::~SignalingClient() {
    disconnectFromServer();
}

QUrl SignalingClient::buildSignInUrl() const {
    QUrl url = m_signalingUrl.isEmpty()
        ? QUrl(QStringLiteral("wss://%1/nvst/").arg(m_signalingServer.contains(QLatin1Char(':')) ? m_signalingServer : m_signalingServer + QStringLiteral(":443")))
        : QUrl(m_signalingUrl);
    url.setScheme(QStringLiteral("wss"));
    QString path = url.path().isEmpty() ? QStringLiteral("/nvst/") : url.path();
    if (!path.endsWith(QLatin1Char('/'))) path += QLatin1Char('/');
    path += QStringLiteral("sign_in");
    url.setPath(path);
    QUrlQuery query(url);
    query.addQueryItem(QStringLiteral("peer_id"), m_peerName);
    query.addQueryItem(QStringLiteral("version"), QStringLiteral("2"));
    query.addQueryItem(QStringLiteral("peer_role"), QStringLiteral("1"));
    query.addQueryItem(QStringLiteral("pairing_id"), m_sessionId);
    url.setQuery(query);
    return url;
}

void SignalingClient::connectToServer(SignalingConnectCallback onConnect) {
    if (isConnected()) {
        onConnect(true, QString());
        return;
    }
    m_peerName = QStringLiteral("peer-%1").arg(QRandomGenerator::global()->bounded(1000000000));
    QNetworkRequest request(buildSignInUrl());
    request.setRawHeader("Sec-WebSocket-Protocol", QStringLiteral("x-nv-sessionid.%1").arg(m_sessionId).toUtf8());
    request.setRawHeader("Origin", "https://play.geforcenow.com");
    request.setRawHeader("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36");

    auto openedConnection = std::make_shared<QMetaObject::Connection>();
    auto errorConnection = std::make_shared<QMetaObject::Connection>();
    *openedConnection = connect(m_socket, &QWebSocket::connected, this, [this, onConnect, openedConnection, errorConnection]() {
        disconnect(*openedConnection);
        disconnect(*errorConnection);
        sendPeerInfo();
        setupHeartbeat();
        onConnect(true, QString());
    });
    *errorConnection = connect(m_socket, &QWebSocket::errorOccurred, this, [this, onConnect, openedConnection, errorConnection](QAbstractSocket::SocketError) {
        disconnect(*openedConnection);
        disconnect(*errorConnection);
        onConnect(false, m_socket->errorString());
    });
    m_socket->open(request);
}

void SignalingClient::disconnectFromServer() {
    m_heartbeat.stop();
    if (m_socket) m_socket->close(QWebSocketProtocol::CloseCodeNormal);
}

void SignalingClient::setupHeartbeat() {
    m_heartbeat.start();
}

void SignalingClient::sendJson(const QJsonObject &json) {
    if (!isConnected()) return;
    m_socket->sendTextMessage(QString::fromUtf8(QJsonDocument(json).toJson(QJsonDocument::Compact)));
}

void SignalingClient::sendPeerInfo() {
    sendJson(QJsonObject{
        {QStringLiteral("ackid"), ++m_ackCounter},
        {QStringLiteral("peer_info"), QJsonObject{
            {QStringLiteral("browser"), QStringLiteral("Chrome")},
            {QStringLiteral("browserVersion"), QStringLiteral("131")},
            {QStringLiteral("connected"), true},
            {QStringLiteral("id"), m_peerId},
            {QStringLiteral("name"), m_peerName},
            {QStringLiteral("peerRole"), 0},
            {QStringLiteral("resolution"), m_peerResolution},
            {QStringLiteral("version"), 2},
        }},
    });
}

void SignalingClient::handleMessage(const QString &text) {
    const QJsonDocument document = QJsonDocument::fromJson(text.toUtf8());
    if (!document.isObject()) return;
    const QJsonObject json = document.object();
    const QJsonObject peerInfo = json.value(QStringLiteral("peer_info")).toObject();
    if (!peerInfo.isEmpty() && peerInfo.value(QStringLiteral("name")).toString() == m_peerName) {
        m_peerId = peerInfo.value(QStringLiteral("id")).toInt(m_peerId);
    }
    if (json.contains(QStringLiteral("ackid"))) {
        const int peerId = peerInfo.value(QStringLiteral("id")).toInt(-1);
        if (peerId != m_peerId) sendJson(QJsonObject{{QStringLiteral("ack"), json.value(QStringLiteral("ackid")).toInt()}});
    }
    if (json.contains(QStringLiteral("ack"))) return;
    if (json.contains(QStringLiteral("hb"))) {
        sendJson(QJsonObject{{QStringLiteral("hb"), 1}});
        return;
    }

    const QJsonObject peerMessage = json.value(QStringLiteral("peer_msg")).toObject();
    const QString messageText = peerMessage.value(QStringLiteral("msg")).toString();
    if (messageText.isEmpty()) return;
    m_remotePeerId = peerMessage.value(QStringLiteral("from")).toInt(m_remotePeerId);
    const QJsonDocument payloadDocument = QJsonDocument::fromJson(messageText.toUtf8());
    if (!payloadDocument.isObject()) return;
    const QJsonObject payload = payloadDocument.object();
    if (payload.value(QStringLiteral("type")).toString() == QStringLiteral("offer")) {
        if (m_onOffer) m_onOffer(payload.value(QStringLiteral("sdp")).toString());
        return;
    }
    const QString candidateText = payload.value(QStringLiteral("candidate")).toString();
    if (!candidateText.isEmpty() && m_onIceCandidate) {
        IceCandidatePayload candidate;
        candidate.candidate = candidateText;
        candidate.sdpMid = payload.value(QStringLiteral("sdpMid")).toString();
        candidate.sdpMLineIndex = payload.value(QStringLiteral("sdpMLineIndex")).toInt();
        candidate.usernameFragment = payload.value(QStringLiteral("usernameFragment")).toString(payload.value(QStringLiteral("ufrag")).toString());
        m_onIceCandidate(candidate);
    }
}

void SignalingClient::sendAnswer(const SendAnswerRequest &answer) {
    QJsonObject payload{{QStringLiteral("type"), QStringLiteral("answer")}, {QStringLiteral("sdp"), answer.sdp}};
    if (!answer.nvstSdp.isEmpty()) payload.insert(QStringLiteral("nvstSdp"), answer.nvstSdp);
    sendJson(QJsonObject{
        {QStringLiteral("peer_msg"), QJsonObject{{QStringLiteral("from"), m_peerId}, {QStringLiteral("to"), m_remotePeerId}, {QStringLiteral("msg"), QString::fromUtf8(QJsonDocument(payload).toJson(QJsonDocument::Compact))}}},
        {QStringLiteral("ackid"), ++m_ackCounter},
    });
}

void SignalingClient::sendIceCandidate(const IceCandidatePayload &candidate) {
    QJsonObject payload{{QStringLiteral("candidate"), candidate.candidate}, {QStringLiteral("sdpMid"), candidate.sdpMid}, {QStringLiteral("sdpMLineIndex"), candidate.sdpMLineIndex}};
    if (!candidate.usernameFragment.isEmpty()) payload.insert(QStringLiteral("usernameFragment"), candidate.usernameFragment);
    sendJson(QJsonObject{
        {QStringLiteral("peer_msg"), QJsonObject{{QStringLiteral("from"), m_peerId}, {QStringLiteral("to"), m_remotePeerId}, {QStringLiteral("msg"), QString::fromUtf8(QJsonDocument(payload).toJson(QJsonDocument::Compact))}}},
        {QStringLiteral("ackid"), ++m_ackCounter},
    });
}

void SignalingClient::setPeerResolution(const QString &resolution) {
    if (!resolution.isEmpty()) m_peerResolution = resolution;
}

void SignalingClient::onOffer(SignalingOfferCallback callback) {
    m_onOffer = callback;
}

void SignalingClient::onIceCandidate(SignalingIceCallback callback) {
    m_onIceCandidate = callback;
}

bool SignalingClient::isConnected() const {
    return m_socket && m_socket->state() == QAbstractSocket::ConnectedState;
}

} // namespace OpnQt
