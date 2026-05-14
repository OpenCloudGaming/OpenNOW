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


    void StartOAuthLogin(AuthCallback completion);


    void RefreshSession(AuthCallback completion, bool forceRefresh = false);


    void FetchStarFleetUserInfo(const std::string &accessToken,
                                std::function<void(bool, NSDictionary *, const std::string &)> completion);
    void FetchClientToken(const std::string &accessToken,
                          std::function<void(bool, const std::string &, const std::string &)> completion);


    void ServerLogout(const std::string &idToken, const std::string &locale,
                      SimpleCallback completion);


    static std::string GetPersistentDeviceUUID();


    void SaveSession(const AuthSession &session);
    AuthSession LoadSavedSession();
    std::vector<AuthSession> LoadSavedSessions();
    AuthSession LoadSavedSessionForUserId(const std::string &userId);
    void SetActiveSessionUserId(const std::string &userId);
    void RemoveSavedSession(const std::string &userId);
    void ClearSession();


    bool GetStayLoggedIn();
    void SetStayLoggedIn(bool value);


    static constexpr const char *kOAuthAuthorizeURL = "https://login.nvidia.com/authorize";
    static constexpr const char *kOAuthTokenURL = "https://login.nvidia.com/token";
    static constexpr const char *kOAuthClientId = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
    static constexpr const char *kOAuthRedirectURI = "com.nvidia.geforcenow://oauth/callback";
    static constexpr const char *kOAuthScope = "openid consent email tk_client age";
    static constexpr const char *kDefaultIdpId = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
    static constexpr const char *kDefaultUserAgent = "NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
    static constexpr const char *kOAuthLogoutURL = "https://login.nvidia.com/logout";


    static AuthSession ParseOAuthSession(NSDictionary *json);
    static NSDictionary *parseQueryString(NSString *query);

private:
    AuthService();


    void doOAuthTokenExchange(NSString *authCode, NSString *codeVerifier,
                               NSString *redirectUri, AuthCallback completion);


    static int64_t getIdTokenExpiry(NSString *idToken);
};

}
