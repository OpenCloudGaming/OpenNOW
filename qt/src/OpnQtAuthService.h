#pragma once

#include <QtCore/QDateTime>
#include <QtCore/QList>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QUrl>
#include <functional>

class QNetworkAccessManager;
class QNetworkReply;
class QTcpServer;

namespace OpnQt {

struct AuthSession {
    QString accessToken;
    QString idToken;
    QString refreshToken;
    QString userId;
    QString displayName;
    QString email;
    QString membershipTier;
    qint64 expiresAt = 0;
    bool isAuthenticated = false;
    QString clientToken;
    qint64 clientTokenExpiry = 0;
    qint64 clientTokenExpiryLength = 0;
    qint64 idTokenExpiry = 0;
    qint64 accessTokenExpiry = 0;

    static qint64 currentEpochMs();
    bool isClientTokenValid() const;
    bool isAccessTokenValid() const;
    QString identity() const;
};

using AuthCallback = std::function<void(bool success, const AuthSession &session, const QString &error)>;
using SimpleCallback = std::function<void(bool success, const QString &error)>;

class AuthService final : public QObject {
public:
    static AuthService &shared();

    void startOAuthLogin(AuthCallback completion);
    void refreshSession(AuthCallback completion, bool forceRefresh = false);
    void fetchUserInfo(const QString &accessToken, std::function<void(bool, const QVariantMap &, const QString &)> completion);
    void fetchClientToken(const QString &accessToken, std::function<void(bool, const QString &, const QString &)> completion);
    void serverLogout(const QString &idToken, const QString &locale, SimpleCallback completion);

    void saveSession(const AuthSession &session);
    AuthSession loadSavedSession() const;
    QList<AuthSession> loadSavedSessions() const;
    AuthSession loadSavedSessionForUserId(const QString &userId) const;
    void setActiveSessionUserId(const QString &userId);
    void removeSavedSession(const QString &userId);
    void clearSession();

    bool stayLoggedIn() const;
    void setStayLoggedIn(bool value);

    static AuthSession parseOAuthSession(const QVariantMap &json);
    static QString persistentDeviceUuid();

private:
    explicit AuthService(QObject *parent = nullptr);

    void exchangeOAuthCode(const QString &authCode, const QString &codeVerifier, const QString &redirectUri, AuthCallback completion);
    void ensureClientToken(AuthSession session, std::function<void(AuthSession)> completion);
    void completeRefreshWithSession(AuthSession session, AuthCallback completion);
    QNetworkReply *postTokenForm(const QString &body);
    QNetworkReply *getWithHeaders(const QUrl &url);
    QList<QVariantMap> loadAccountMaps(QString *activeUserId = nullptr) const;
    void saveAccountMaps(const QList<QVariantMap> &accounts, const QString &activeUserId);

    QNetworkAccessManager *m_network = nullptr;
    QTcpServer *m_oauthServer = nullptr;
    QTcpServer *m_oauthIpv6Server = nullptr;
    bool m_oauthCompleted = false;
};

} // namespace OpnQt
