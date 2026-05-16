#include "OpnQtAuthService.h"

#include <QtCore/QCryptographicHash>
#include <QtCore/QFileInfo>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QRandomGenerator>
#include <QtCore/QSettings>
#include <QtCore/QSysInfo>
#include <QtCore/QTimer>
#include <QtCore/QUrlQuery>
#include <QtCore/QUuid>
#include <QtCore/QVariantMap>
#include <QtCore/QDebug>
#include <QtGui/QDesktopServices>
#include <QtNetwork/QHostAddress>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>
#include <QtNetwork/QTcpServer>
#include <QtNetwork/QTcpSocket>

#include <algorithm>

namespace OpnQt {
namespace {

constexpr const char *kOAuthAuthorizeUrl = "https://login.nvidia.com/authorize";
constexpr const char *kOAuthTokenUrl = "https://login.nvidia.com/token";
constexpr const char *kOAuthClientId = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
constexpr const char *kOAuthScope = "openid consent email tk_client age";
constexpr const char *kDefaultIdpId = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
constexpr const char *kDefaultUserAgent = "NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
constexpr const char *kOAuthLogoutUrl = "https://login.nvidia.com/logout";
constexpr qint64 kClientTokenRefreshWindowMs = 5 * 60 * 1000;
constexpr qint64 kClientTokenRefreshWindowPercent = 20;

QString base64Url(QByteArray bytes) {
    return QString::fromLatin1(bytes.toBase64(QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals));
}

QString randomString(int length) {
    static constexpr char charset[] = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~";
    QString result;
    result.reserve(length);
    for (int i = 0; i < length; ++i) {
        const quint32 index = QRandomGenerator::global()->bounded(static_cast<quint32>(sizeof(charset) - 1));
        result.append(QLatin1Char(charset[index]));
    }
    return result;
}

QString codeChallengeForVerifier(const QString &verifier) {
    return base64Url(QCryptographicHash::hash(verifier.toUtf8(), QCryptographicHash::Sha256));
}

QString generateDeviceId() {
    const QString input = QSysInfo::machineHostName() + QStringLiteral(":") + qEnvironmentVariable("USER", "unknown") + QStringLiteral(":opennow-stable");
    return QString::fromLatin1(QCryptographicHash::hash(input.toUtf8(), QCryptographicHash::Sha256).toHex());
}

QVariantMap jsonMapFromBytes(const QByteArray &data) {
    QJsonParseError error;
    const QJsonDocument document = QJsonDocument::fromJson(data, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) {
        return {};
    }
    return document.object().toVariantMap();
}

QString jsonErrorMessage(const QVariantMap &json, const QString &fallback) {
    const QString description = json.value(QStringLiteral("error_description")).toString();
    if (!description.isEmpty()) return description;
    const QString message = json.value(QStringLiteral("message")).toString();
    if (!message.isEmpty()) return message;
    const QString error = json.value(QStringLiteral("error")).toString();
    if (!error.isEmpty()) return error;
    return fallback;
}

QString compactResponseBody(const QByteArray &data) {
    QString text = QString::fromUtf8(data).simplified();
    if (text.size() > 600) text = text.left(600) + QStringLiteral("...");
    return text;
}

QVariantMap sessionToMap(const AuthSession &session) {
    QVariantMap map;
    map.insert(QStringLiteral("accessToken"), session.accessToken);
    map.insert(QStringLiteral("idToken"), session.idToken);
    map.insert(QStringLiteral("refreshToken"), session.refreshToken);
    map.insert(QStringLiteral("userId"), session.userId);
    map.insert(QStringLiteral("displayName"), session.displayName);
    map.insert(QStringLiteral("email"), session.email);
    map.insert(QStringLiteral("membershipTier"), session.membershipTier);
    map.insert(QStringLiteral("expiresAt"), session.expiresAt);
    map.insert(QStringLiteral("isAuthenticated"), session.isAuthenticated);
    map.insert(QStringLiteral("clientToken"), session.clientToken);
    map.insert(QStringLiteral("clientTokenExpiry"), session.clientTokenExpiry);
    map.insert(QStringLiteral("clientTokenExpiryLength"), session.clientTokenExpiryLength);
    map.insert(QStringLiteral("idTokenExpiry"), session.idTokenExpiry);
    map.insert(QStringLiteral("accessTokenExpiry"), session.accessTokenExpiry);
    return map;
}

AuthSession sessionFromMap(const QVariantMap &map) {
    AuthSession session;
    session.accessToken = map.value(QStringLiteral("accessToken")).toString();
    session.idToken = map.value(QStringLiteral("idToken")).toString();
    session.refreshToken = map.value(QStringLiteral("refreshToken")).toString();
    session.userId = map.value(QStringLiteral("userId")).toString();
    session.displayName = map.value(QStringLiteral("displayName")).toString();
    session.email = map.value(QStringLiteral("email")).toString();
    session.membershipTier = map.value(QStringLiteral("membershipTier")).toString();
    session.expiresAt = map.value(QStringLiteral("expiresAt")).toLongLong();
    session.isAuthenticated = map.value(QStringLiteral("isAuthenticated")).toBool() || !session.accessToken.isEmpty();
    session.clientToken = map.value(QStringLiteral("clientToken")).toString();
    session.clientTokenExpiry = map.value(QStringLiteral("clientTokenExpiry")).toLongLong();
    session.clientTokenExpiryLength = map.value(QStringLiteral("clientTokenExpiryLength")).toLongLong();
    session.idTokenExpiry = map.value(QStringLiteral("idTokenExpiry")).toLongLong();
    session.accessTokenExpiry = map.value(QStringLiteral("accessTokenExpiry")).toLongLong();
    return session;
}

QString sessionIdentityFromMap(const QVariantMap &map) {
    AuthSession session = sessionFromMap(map);
    return session.identity();
}

bool shouldRefreshClientToken(const AuthSession &session) {
    if (session.clientToken.isEmpty() || session.clientTokenExpiry == 0) return true;
    const qint64 remainingMs = session.clientTokenExpiry - AuthSession::currentEpochMs();
    if (session.clientTokenExpiryLength > 0) {
        return remainingMs < (session.clientTokenExpiryLength * kClientTokenRefreshWindowPercent) / 100;
    }
    return remainingMs < kClientTokenRefreshWindowMs;
}

AuthSession mergeRefreshedSession(const AuthSession &saved, const AuthSession &refreshed) {
    AuthSession merged = refreshed;
    if (merged.refreshToken.isEmpty()) merged.refreshToken = saved.refreshToken;
    if (merged.clientToken.isEmpty()) {
        merged.clientToken = saved.clientToken;
        merged.clientTokenExpiry = saved.clientTokenExpiry;
        merged.clientTokenExpiryLength = saved.clientTokenExpiryLength;
    }
    if (merged.email.isEmpty()) merged.email = saved.email;
    if (merged.displayName.isEmpty()) merged.displayName = saved.displayName;
    if (merged.membershipTier.isEmpty()) merged.membershipTier = saved.membershipTier;
    if (merged.userId.isEmpty()) merged.userId = saved.userId;
    return merged;
}

QList<quint16> candidatePorts() {
    return {2259, 6460, 7119, 8870, 9096};
}

void closeAndDeleteServer(QTcpServer *&server) {
    if (!server) return;
    server->close();
    server->deleteLater();
    server = nullptr;
}

QString callbackHtml() {
    return QStringLiteral(R"HTML(<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>OpenNOW Sign In Complete</title><style>:root{color-scheme:dark}body{display:grid;place-items:center;min-height:100vh;margin:0;background:radial-gradient(circle at 25% 20%,rgba(52,199,89,.22),transparent 30%),linear-gradient(135deg,#050807,#111814);color:#f1fff7;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}.card{width:min(520px,calc(100vw - 40px));padding:34px;border:1px solid rgba(173,255,202,.24);border-radius:28px;background:rgba(11,20,16,.82);box-shadow:0 34px 90px rgba(0,0,0,.54)}h1{margin:0 0 14px;font-size:42px;line-height:.96;letter-spacing:-.04em}.copy{color:#a8c3b4;font-size:16px;line-height:1.55}.status{margin-top:24px;padding:14px 16px;border-radius:16px;background:rgba(121,242,167,.10);color:#d9ffe8;font-weight:700}</style></head><body><main class="card"><h1>Sign in complete</h1><p class="copy">Your NVIDIA account is connected. Return to OpenNOW to continue to your cloud gaming library.</p><div class="status">You can safely close this window.</div><script>setTimeout(function(){window.close();},1200);</script></main></body></html>)HTML");
}

} // namespace

qint64 AuthSession::currentEpochMs() {
    return QDateTime::currentMSecsSinceEpoch();
}

bool AuthSession::isClientTokenValid() const {
    return !clientToken.isEmpty() && clientTokenExpiry > currentEpochMs();
}

bool AuthSession::isAccessTokenValid() const {
    return !accessToken.isEmpty() && accessTokenExpiry > currentEpochMs();
}

QString AuthSession::identity() const {
    if (!userId.isEmpty()) return userId;
    if (!email.isEmpty()) return email;
    if (!displayName.isEmpty()) return displayName;
    return accessToken;
}

AuthService &AuthService::shared() {
    static AuthService service;
    return service;
}

AuthService::AuthService(QObject *parent)
    : QObject(parent), m_network(new QNetworkAccessManager(this)) {}

QString AuthService::persistentDeviceUuid() {
    QSettings settings;
    QString stored = settings.value(QStringLiteral("OPN_PersistentDeviceUUID")).toString();
    if (stored.isEmpty()) {
        stored = settings.value(QStringLiteral("GFN_PersistentDeviceUUID")).toString();
        if (!stored.isEmpty()) settings.setValue(QStringLiteral("OPN_PersistentDeviceUUID"), stored);
    }
    if (stored.isEmpty()) {
        stored = QUuid::createUuid().toString(QUuid::WithoutBraces).toUpper();
        settings.setValue(QStringLiteral("OPN_PersistentDeviceUUID"), stored);
    }
    return stored;
}

void AuthService::startOAuthLogin(AuthCallback completion) {
    closeAndDeleteServer(m_oauthServer);
    closeAndDeleteServer(m_oauthIpv6Server);

    m_oauthCompleted = false;
    auto *server = new QTcpServer(this);
    auto *ipv6Server = new QTcpServer(this);
    quint16 port = 0;
    for (quint16 candidate : candidatePorts()) {
        if (server->listen(QHostAddress::LocalHost, candidate)) {
            port = candidate;
            ipv6Server->listen(QHostAddress::LocalHostIPv6, candidate);
            break;
        }
    }
    if (port == 0) {
        server->deleteLater();
        ipv6Server->deleteLater();
        completion(false, AuthSession{}, QStringLiteral("No available port for OAuth callback"));
        return;
    }
    m_oauthServer = server;
    m_oauthIpv6Server = ipv6Server->isListening() ? ipv6Server : nullptr;
    if (!m_oauthIpv6Server) ipv6Server->deleteLater();
    qInfo() << "[Auth] OAuth callback listening"
            << "ipv4" << server->serverAddress().toString() << server->serverPort()
            << "ipv6" << (m_oauthIpv6Server ? m_oauthIpv6Server->serverAddress().toString() : QStringLiteral("disabled"));

    const QString codeVerifier = randomString(64);
    const QString codeChallenge = codeChallengeForVerifier(codeVerifier);
    const QString state = randomString(32);
    const QString nonce = randomString(32);
    const QString redirectUri = QStringLiteral("http://localhost:%1").arg(port);

    const auto handleConnection = [this, completion, state, codeVerifier, redirectUri](QTcpServer *sourceServer) {
        QTcpSocket *socket = sourceServer->nextPendingConnection();
        if (!socket) return;
        connect(socket, &QTcpSocket::readyRead, this, [this, socket, completion, state, codeVerifier, redirectUri]() {
            const QByteArray requestBytes = socket->readAll();
            const QByteArray responseBody = callbackHtml().toUtf8();
            socket->write("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\nContent-Length: ");
            socket->write(QByteArray::number(responseBody.size()));
            socket->write("\r\n\r\n");
            socket->write(responseBody);
            socket->disconnectFromHost();
            closeAndDeleteServer(m_oauthServer);
            closeAndDeleteServer(m_oauthIpv6Server);

            if (m_oauthCompleted) return;
            m_oauthCompleted = true;

            const QString request = QString::fromUtf8(requestBytes);
            const int getStart = request.indexOf(QStringLiteral("GET "));
            const int pathStart = getStart >= 0 ? getStart + 4 : -1;
            const int pathEnd = pathStart >= 0 ? request.indexOf(QLatin1Char(' '), pathStart) : -1;
            if (pathStart < 0 || pathEnd < 0) {
                completion(false, AuthSession{}, QStringLiteral("Invalid OAuth callback request"));
                return;
            }

            const QString path = request.mid(pathStart, pathEnd - pathStart);
            const QUrl callbackUrl(QStringLiteral("http://localhost") + path);
            const QUrlQuery query(callbackUrl);
            const QString code = query.queryItemValue(QStringLiteral("code"));
            const QString returnedState = query.queryItemValue(QStringLiteral("state"));
            if (code.isEmpty()) {
                QString error = query.queryItemValue(QStringLiteral("error_description"));
                if (error.isEmpty()) error = query.queryItemValue(QStringLiteral("error"));
                completion(false, AuthSession{}, error.isEmpty() ? QStringLiteral("Unknown OAuth error") : error);
                return;
            }
            if (returnedState != state) {
                completion(false, AuthSession{}, QStringLiteral("State mismatch - possible CSRF"));
                return;
            }
            exchangeOAuthCode(code, codeVerifier, redirectUri, completion);
        });
    };
    connect(server, &QTcpServer::newConnection, this, [handleConnection, server]() { handleConnection(server); });
    if (m_oauthIpv6Server) {
        QTcpServer *ipv6Source = m_oauthIpv6Server;
        connect(ipv6Source, &QTcpServer::newConnection, this, [handleConnection, ipv6Source]() { handleConnection(ipv6Source); });
    }

    auto *probe = new QTcpSocket(this);
    connect(probe, &QTcpSocket::connected, this, [probe]() {
        qInfo() << "[Auth] OAuth callback preflight connected";
        probe->disconnectFromHost();
        probe->deleteLater();
    });
    connect(probe, &QTcpSocket::errorOccurred, this, [probe](QAbstractSocket::SocketError) {
        qWarning() << "[Auth] OAuth callback preflight failed" << probe->errorString();
        probe->deleteLater();
    });
    probe->connectToHost(QHostAddress::LocalHost, port);

    QUrl authUrl(QString::fromLatin1(kOAuthAuthorizeUrl));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("response_type"), QStringLiteral("code"));
    query.addQueryItem(QStringLiteral("device_id"), generateDeviceId());
    query.addQueryItem(QStringLiteral("scope"), QString::fromLatin1(kOAuthScope));
    query.addQueryItem(QStringLiteral("client_id"), QString::fromLatin1(kOAuthClientId));
    query.addQueryItem(QStringLiteral("redirect_uri"), redirectUri);
    query.addQueryItem(QStringLiteral("ui_locales"), QStringLiteral("en_US"));
    query.addQueryItem(QStringLiteral("nonce"), nonce);
    query.addQueryItem(QStringLiteral("prompt"), QStringLiteral("select_account"));
    query.addQueryItem(QStringLiteral("code_challenge"), codeChallenge);
    query.addQueryItem(QStringLiteral("code_challenge_method"), QStringLiteral("S256"));
    query.addQueryItem(QStringLiteral("idp_id"), QString::fromLatin1(kDefaultIdpId));
    query.addQueryItem(QStringLiteral("state"), state);
    authUrl.setQuery(query);
    if (!QDesktopServices::openUrl(authUrl)) {
        m_oauthCompleted = true;
        closeAndDeleteServer(m_oauthServer);
        closeAndDeleteServer(m_oauthIpv6Server);
        completion(false, AuthSession{}, QStringLiteral("Failed to open browser for OAuth sign in"));
    }
}

void AuthService::exchangeOAuthCode(const QString &authCode, const QString &codeVerifier, const QString &redirectUri, AuthCallback completion) {
    const QString body = QStringLiteral("grant_type=authorization_code&code=%1&redirect_uri=%2&code_verifier=%3")
        .arg(authCode, redirectUri, codeVerifier);
    QNetworkReply *reply = postTokenForm(body);
    connect(reply, &QNetworkReply::finished, this, [this, reply, completion]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QVariantMap json = jsonMapFromBytes(data);
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        reply->deleteLater();
        qInfo() << "[Auth] OAuth token exchange response" << "status" << status << "networkError" << networkError;
        if (!networkError.isEmpty() || status != 200 || json.isEmpty()) {
            qWarning() << "[Auth] OAuth token exchange failed"
                       << "status" << status
                       << "networkError" << networkError
                       << "body" << compactResponseBody(data);
            completion(false, AuthSession{}, !networkError.isEmpty() ? networkError : jsonErrorMessage(json, QStringLiteral("Token exchange failed (%1)").arg(status)));
            return;
        }
        AuthSession session = parseOAuthSession(json);
        ensureClientToken(session, [completion](AuthSession enriched) {
            completion(true, enriched, QString());
        });
    });
}

void AuthService::refreshSession(AuthCallback completion, bool forceRefresh) {
    const AuthSession saved = loadSavedSession();
    if (!saved.isAuthenticated) {
        completion(false, AuthSession{}, QStringLiteral("No saved session available"));
        return;
    }
    if (!forceRefresh && saved.isAccessTokenValid() && shouldRefreshClientToken(saved)) {
        completeRefreshWithSession(saved, completion);
        return;
    }

    const auto refreshWithOAuthToken = [this, saved, completion]() {
        if (saved.refreshToken.isEmpty()) {
            if (saved.isAccessTokenValid()) completeRefreshWithSession(saved, completion);
            else completion(false, saved, QStringLiteral("No refresh mechanism available"));
            return;
        }
        QUrlQuery body;
        body.addQueryItem(QStringLiteral("grant_type"), QStringLiteral("refresh_token"));
        body.addQueryItem(QStringLiteral("refresh_token"), saved.refreshToken);
        body.addQueryItem(QStringLiteral("client_id"), QString::fromLatin1(kOAuthClientId));
        QNetworkReply *reply = postTokenForm(body.query(QUrl::FullyEncoded));
        connect(reply, &QNetworkReply::finished, this, [this, reply, saved, completion]() {
            const QByteArray data = reply->readAll();
            const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            const QVariantMap json = jsonMapFromBytes(data);
            const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
            reply->deleteLater();
            if (!networkError.isEmpty() || status != 200 || json.isEmpty()) {
                completion(false, saved, !networkError.isEmpty() ? networkError : jsonErrorMessage(json, QStringLiteral("Token refresh failed (%1)").arg(status)));
                return;
            }
            completeRefreshWithSession(mergeRefreshedSession(saved, parseOAuthSession(json)), completion);
        });
    };

    if (saved.clientToken.isEmpty()) {
        refreshWithOAuthToken();
        return;
    }

    QUrlQuery body;
    body.addQueryItem(QStringLiteral("grant_type"), QStringLiteral("urn:ietf:params:oauth:grant-type:client_token"));
    body.addQueryItem(QStringLiteral("client_token"), saved.clientToken);
    body.addQueryItem(QStringLiteral("client_id"), QString::fromLatin1(kOAuthClientId));
    body.addQueryItem(QStringLiteral("sub"), saved.userId);
    QNetworkReply *reply = postTokenForm(body.query(QUrl::FullyEncoded));
    connect(reply, &QNetworkReply::finished, this, [this, reply, saved, completion, refreshWithOAuthToken]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QVariantMap json = jsonMapFromBytes(data);
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        reply->deleteLater();
        if (!networkError.isEmpty() || status != 200 || json.isEmpty()) {
            refreshWithOAuthToken();
            return;
        }
        completeRefreshWithSession(mergeRefreshedSession(saved, parseOAuthSession(json)), completion);
    });
}

void AuthService::ensureClientToken(AuthSession session, std::function<void(AuthSession)> completion) {
    if (!session.isAuthenticated || !session.isAccessTokenValid() || !shouldRefreshClientToken(session)) {
        completion(session);
        return;
    }
    fetchClientToken(session.accessToken, [session, completion](bool success, const QString &clientToken, const QString &expiresInText) mutable {
        if (success && !clientToken.isEmpty()) {
            session.clientToken = clientToken;
            qint64 expiresIn = expiresInText.toLongLong();
            if (expiresIn <= 0) expiresIn = 86400;
            session.clientTokenExpiry = AuthSession::currentEpochMs() + (expiresIn * 1000);
            session.clientTokenExpiryLength = expiresIn * 1000;
        }
        completion(session);
    });
}

void AuthService::completeRefreshWithSession(AuthSession session, AuthCallback completion) {
    ensureClientToken(session, [this, completion](AuthSession enriched) {
        saveSession(enriched);
        completion(true, enriched, QString());
    });
}

void AuthService::fetchUserInfo(const QString &accessToken, std::function<void(bool, const QVariantMap &, const QString &)> completion) {
    QNetworkRequest request(QUrl(QStringLiteral("https://login.nvidia.com/userinfo")));
    request.setAttribute(QNetworkRequest::Http2AllowedAttribute, false);
    request.setRawHeader("Authorization", (QStringLiteral("Bearer ") + accessToken).toUtf8());
    request.setRawHeader("Origin", "https://nvfile");
    request.setRawHeader("Accept", "application/json");
    request.setRawHeader("User-Agent", kDefaultUserAgent);
    QNetworkReply *reply = m_network->get(request);
    connect(reply, &QNetworkReply::finished, this, [reply, completion]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        const QVariantMap json = jsonMapFromBytes(data);
        reply->deleteLater();
        qInfo() << "[Auth] Userinfo response" << "status" << status << "networkError" << networkError;
        if (!networkError.isEmpty() || status != 200 || json.isEmpty()) {
            qWarning() << "[Auth] Client token fetch failed"
                       << "status" << status
                       << "networkError" << networkError
                       << "body" << compactResponseBody(data);
            completion(false, {}, !networkError.isEmpty() ? networkError : QStringLiteral("HTTP %1").arg(status));
            return;
        }
        completion(true, json, QString());
    });
}

void AuthService::fetchClientToken(const QString &accessToken, std::function<void(bool, const QString &, const QString &)> completion) {
    QNetworkRequest request(QUrl(QStringLiteral("https://login.nvidia.com/client_token")));
    request.setAttribute(QNetworkRequest::Http2AllowedAttribute, false);
    request.setRawHeader("Authorization", (QStringLiteral("Bearer ") + accessToken).toUtf8());
    request.setRawHeader("Origin", "https://nvfile");
    request.setRawHeader("Accept", "application/json, text/plain, */*");
    request.setRawHeader("User-Agent", kDefaultUserAgent);
    QNetworkReply *reply = m_network->get(request);
    connect(reply, &QNetworkReply::finished, this, [reply, completion]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        const QVariantMap json = jsonMapFromBytes(data);
        reply->deleteLater();
        qInfo() << "[Auth] Client token response" << "status" << status << "networkError" << networkError;
        if (!networkError.isEmpty() || status != 200 || json.isEmpty()) {
            completion(false, {}, !networkError.isEmpty() ? networkError : QStringLiteral("HTTP %1").arg(status));
            return;
        }
        const QString token = json.value(QStringLiteral("client_token")).toString();
        if (token.isEmpty()) {
            completion(false, {}, QStringLiteral("No client_token in response"));
            return;
        }
        completion(true, token, json.value(QStringLiteral("expires_in")).toString());
    });
}

void AuthService::serverLogout(const QString &idToken, const QString &locale, SimpleCallback completion) {
    if (idToken.isEmpty()) {
        clearSession();
        completion(true, QString());
        return;
    }
    QUrl url(QString::fromLatin1(kOAuthLogoutUrl));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("id_token_hint"), idToken);
    query.addQueryItem(QStringLiteral("ui_locales"), locale.isEmpty() ? QStringLiteral("en_US") : locale);
    url.setQuery(query);
    QNetworkReply *reply = m_network->get(QNetworkRequest(url));
    connect(reply, &QNetworkReply::finished, this, [this, reply, completion]() {
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        reply->deleteLater();
        clearSession();
        completion(networkError.isEmpty(), networkError);
    });
}

QNetworkReply *AuthService::postTokenForm(const QString &body) {
    QNetworkRequest request(QUrl(QString::fromLatin1(kOAuthTokenUrl)));
    request.setAttribute(QNetworkRequest::Http2AllowedAttribute, false);
    request.setHeader(QNetworkRequest::ContentTypeHeader, QStringLiteral("application/x-www-form-urlencoded; charset=UTF-8"));
    request.setRawHeader("Accept", "application/json, text/plain, */*");
    request.setRawHeader("Origin", "https://nvfile");
    request.setRawHeader("Referer", "https://nvfile/");
    request.setRawHeader("User-Agent", kDefaultUserAgent);
    qInfo() << "[Auth] Token POST" << body.left(body.indexOf(QStringLiteral("&code=")) > 0 ? body.indexOf(QStringLiteral("&code=")) : body.size());
    return m_network->post(request, body.toUtf8());
}

QNetworkReply *AuthService::getWithHeaders(const QUrl &url) {
    QNetworkRequest request(url);
    request.setRawHeader("User-Agent", kDefaultUserAgent);
    return m_network->get(request);
}

void AuthService::saveSession(const AuthSession &session) {
    if (!session.isAuthenticated || session.accessToken.isEmpty()) return;
    const QString identity = session.identity();
    if (identity.isEmpty()) return;
    QString activeUserId;
    QList<QVariantMap> existing = loadAccountMaps(&activeUserId);
    QList<QVariantMap> accounts;
    for (const QVariantMap &account : existing) {
        if (sessionIdentityFromMap(account) == identity) continue;
        accounts.append(account);
    }
    accounts.prepend(sessionToMap(session));
    saveAccountMaps(accounts, identity);
    QSettings settings;
    settings.setValue(QStringLiteral("OPN_HasSavedSession"), true);
    settings.setValue(QStringLiteral("OPN_ActiveUserId"), identity);
}

AuthSession AuthService::loadSavedSession() const {
    QSettings settings;
    QString activeUserId;
    const QList<QVariantMap> accounts = loadAccountMaps(&activeUserId);
    const QString preferred = settings.value(QStringLiteral("OPN_ActiveUserId"), activeUserId).toString();
    QVariantMap fallback;
    for (const QVariantMap &account : accounts) {
        if (fallback.isEmpty()) fallback = account;
        if (!preferred.isEmpty() && sessionIdentityFromMap(account) == preferred) {
            AuthSession session = sessionFromMap(account);
            if (session.isAuthenticated) return session;
        }
    }
    if (!fallback.isEmpty()) return sessionFromMap(fallback);
    return {};
}

QList<AuthSession> AuthService::loadSavedSessions() const {
    QList<AuthSession> sessions;
    const QList<QVariantMap> accounts = loadAccountMaps(nullptr);
    for (const QVariantMap &account : accounts) {
        AuthSession session = sessionFromMap(account);
        if (session.isAuthenticated) sessions.append(session);
    }
    return sessions;
}

AuthSession AuthService::loadSavedSessionForUserId(const QString &userId) const {
    if (userId.isEmpty()) return {};
    for (const QVariantMap &account : loadAccountMaps(nullptr)) {
        if (sessionIdentityFromMap(account) == userId) return sessionFromMap(account);
    }
    return {};
}

void AuthService::setActiveSessionUserId(const QString &userId) {
    if (userId.isEmpty()) return;
    QString activeUserId;
    const QList<QVariantMap> accounts = loadAccountMaps(&activeUserId);
    const bool found = std::any_of(accounts.begin(), accounts.end(), [&userId](const QVariantMap &account) {
        return sessionIdentityFromMap(account) == userId;
    });
    if (!found) return;
    saveAccountMaps(accounts, userId);
    QSettings settings;
    settings.setValue(QStringLiteral("OPN_ActiveUserId"), userId);
    settings.setValue(QStringLiteral("OPN_HasSavedSession"), true);
}

void AuthService::removeSavedSession(const QString &userId) {
    if (userId.isEmpty()) return;
    QString activeUserId;
    const QList<QVariantMap> existing = loadAccountMaps(&activeUserId);
    QList<QVariantMap> accounts;
    for (const QVariantMap &account : existing) {
        if (sessionIdentityFromMap(account) == userId) continue;
        accounts.append(account);
    }
    QString newActive = activeUserId == userId ? QString() : activeUserId;
    if (newActive.isEmpty() && !accounts.isEmpty()) newActive = sessionIdentityFromMap(accounts.first());
    saveAccountMaps(accounts, newActive);
    QSettings settings;
    if (newActive.isEmpty()) {
        settings.remove(QStringLiteral("OPN_ActiveUserId"));
        settings.remove(QStringLiteral("OPN_HasSavedSession"));
    } else {
        settings.setValue(QStringLiteral("OPN_ActiveUserId"), newActive);
        settings.setValue(QStringLiteral("OPN_HasSavedSession"), true);
    }
}

void AuthService::clearSession() {
    QSettings settings;
    const QString activeUserId = settings.value(QStringLiteral("OPN_ActiveUserId")).toString();
    if (!activeUserId.isEmpty()) {
        removeSavedSession(activeUserId);
        return;
    }
    settings.remove(QStringLiteral("auth/accounts"));
    settings.remove(QStringLiteral("auth/activeUserId"));
    settings.remove(QStringLiteral("OPN_HasSavedSession"));
    settings.remove(QStringLiteral("GFN_HasSavedSession"));
    settings.remove(QStringLiteral("OPN_ActiveUserId"));
}

bool AuthService::stayLoggedIn() const {
    QSettings settings;
    if (settings.contains(QStringLiteral("OPN_StayLoggedIn"))) return settings.value(QStringLiteral("OPN_StayLoggedIn")).toBool();
    if (settings.contains(QStringLiteral("GFN_StayLoggedIn"))) return settings.value(QStringLiteral("GFN_StayLoggedIn")).toBool();
    return true;
}

void AuthService::setStayLoggedIn(bool value) {
    QSettings settings;
    settings.setValue(QStringLiteral("OPN_StayLoggedIn"), value);
}

QList<QVariantMap> AuthService::loadAccountMaps(QString *activeUserId) const {
    QSettings settings;
    if (activeUserId) *activeUserId = settings.value(QStringLiteral("auth/activeUserId")).toString();
    QList<QVariantMap> accounts;
    const QVariantList stored = settings.value(QStringLiteral("auth/accounts")).toList();
    for (const QVariant &item : stored) {
        const QVariantMap account = item.toMap();
        if (!account.isEmpty()) accounts.append(account);
    }
    return accounts;
}

void AuthService::saveAccountMaps(const QList<QVariantMap> &accounts, const QString &activeUserId) {
    QVariantList stored;
    for (const QVariantMap &account : accounts) stored.append(account);
    QSettings settings;
    settings.setValue(QStringLiteral("auth/accounts"), stored);
    if (activeUserId.isEmpty()) settings.remove(QStringLiteral("auth/activeUserId"));
    else settings.setValue(QStringLiteral("auth/activeUserId"), activeUserId);
}

AuthSession AuthService::parseOAuthSession(const QVariantMap &json) {
    AuthSession session;
    session.accessToken = json.value(QStringLiteral("access_token")).toString();
    session.idToken = json.value(QStringLiteral("id_token")).toString();
    session.refreshToken = json.value(QStringLiteral("refresh_token")).toString();
    session.clientToken = json.value(QStringLiteral("client_token")).toString();
    qint64 expiresIn = json.value(QStringLiteral("expires_in")).toLongLong();
    if (expiresIn <= 0) expiresIn = 86400;
    session.accessTokenExpiry = AuthSession::currentEpochMs() + (expiresIn * 1000);
    session.expiresAt = QDateTime::currentSecsSinceEpoch() + expiresIn;

    const qint64 clientTokenExpiresIn = json.value(QStringLiteral("client_token_expires_in")).toLongLong();
    if (clientTokenExpiresIn > 0 && !session.clientToken.isEmpty()) {
        session.clientTokenExpiry = AuthSession::currentEpochMs() + (clientTokenExpiresIn * 1000);
        session.clientTokenExpiryLength = clientTokenExpiresIn * 1000;
    }

    if (!session.idToken.isEmpty()) {
        const QStringList parts = session.idToken.split(QLatin1Char('.'));
        if (parts.size() >= 2) {
            QByteArray payload = parts[1].toUtf8();
            const int padding = (4 - (payload.size() % 4)) % 4;
            payload.append(QByteArray(padding, '='));
            const QByteArray decoded = QByteArray::fromBase64(payload, QByteArray::Base64UrlEncoding);
            const QVariantMap claims = jsonMapFromBytes(decoded);
            session.userId = claims.value(QStringLiteral("sub")).toString();
            session.displayName = claims.value(QStringLiteral("name")).toString();
            session.email = claims.value(QStringLiteral("email")).toString();
            session.membershipTier = claims.value(QStringLiteral("membership_tier"), QStringLiteral("Free")).toString();
            const qint64 exp = claims.value(QStringLiteral("exp")).toLongLong();
            if (exp > 0) session.idTokenExpiry = exp * 1000;
        }
    }
    session.isAuthenticated = !session.accessToken.isEmpty();
    return session;
}

} // namespace OpnQt
