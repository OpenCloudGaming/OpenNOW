#pragma once

#include <string>
#include <functional>
#include <vector>
#include <Foundation/Foundation.h>

#import "../common/OPNAuthTypes.h"

namespace OPN {

using AuthCallback = std::function<void(bool success, const AuthSession &session,
                                        const std::string &error)>;
using SimpleCallback = std::function<void(bool success, const std::string &error)>;

class AuthService {
public:
    static AuthService &Shared();

    // ── OAuth 2.0 PKCE Flow ──
    void StartOAuthLogin(AuthCallback completion);

    // ── Token Management ──
    void RefreshSession(AuthCallback completion, bool forceRefresh = false);

    // ── User Info ──
    void FetchStarFleetUserInfo(const std::string &accessToken,
                                std::function<void(bool, NSDictionary *, const std::string &)> completion);
    void FetchClientToken(const std::string &accessToken,
                          std::function<void(bool, const std::string &, const std::string &)> completion);

    // ── Logout ──
    void ServerLogout(const std::string &idToken, const std::string &locale,
                      SimpleCallback completion);

    // ── Persistent Device ID ──
    static std::string GetPersistentDeviceUUID();

    // ── Session Persistence (File-Based Plist) ──
    void SaveSession(const AuthSession &session);
    AuthSession LoadSavedSession();
    std::vector<AuthSession> LoadSavedSessions();
    AuthSession LoadSavedSessionForUserId(const std::string &userId);
    void SetActiveSessionUserId(const std::string &userId);
    void RemoveSavedSession(const std::string &userId);
    void ClearSession();

    // ── Stay Logged In Preference ──
    bool GetStayLoggedIn();
    void SetStayLoggedIn(bool value);

    // ── OAuth Constants ──
    static constexpr const char *kOAuthAuthorizeURL = "https://login.nvidia.com/authorize";
    static constexpr const char *kOAuthTokenURL = "https://login.nvidia.com/token";
    static constexpr const char *kOAuthClientId = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
    static constexpr const char *kOAuthRedirectURI = "com.nvidia.geforcenow://oauth/callback";
    static constexpr const char *kOAuthScope = "openid consent email tk_client age";
    static constexpr const char *kDefaultIdpId = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
    static constexpr const char *kDefaultUserAgent = "NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
    static constexpr const char *kOAuthLogoutURL = "https://login.nvidia.com/logout";

    // ── Utility Parsers ──
    static AuthSession ParseOAuthSession(NSDictionary *json);
    static NSDictionary *parseQueryString(NSString *query);

private:
    AuthService();

    // OAuth helpers
    void doOAuthTokenExchange(NSString *authCode, NSString *codeVerifier,
                               NSString *redirectUri, AuthCallback completion);

    // Internal helpers
    static int64_t getIdTokenExpiry(NSString *idToken);
};

} // namespace OPN
