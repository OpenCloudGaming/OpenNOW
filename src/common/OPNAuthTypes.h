#pragma once

#include <string>
#include <cstdint>
#include <chrono>

namespace OPN {

enum class AuthScreen {
    EmailEntry,
    Authenticating,
    Store,
    Catalog,
    Settings,
    Error,
    OAuthBrowser
};

struct AuthCredentials {
    std::string email;
    bool stayLoggedIn = true;
};

struct AuthSession {
    std::string accessToken;
    std::string idToken;
    std::string refreshToken;
    std::string userId;
    std::string displayName;
    std::string email;
    std::string membershipTier;
    int64_t expiresAt = 0;
    bool isAuthenticated = false;

    std::string clientToken;
    int64_t clientTokenExpiry = 0;
    int64_t clientTokenExpiryLength = 0;
    int64_t idTokenExpiry = 0;
    int64_t accessTokenExpiry = 0;

    static int64_t CurrentEpochMs() {
        using namespace std::chrono;
        return duration_cast<milliseconds>(system_clock::now().time_since_epoch()).count();
    }

    bool IsClientTokenValid() const {
        return !clientToken.empty() && clientTokenExpiry > CurrentEpochMs();
    }
    bool IsAccessTokenValid() const {
        return !accessToken.empty() && accessTokenExpiry > CurrentEpochMs();
    }
    bool HasAccessToken() const {
        return !accessToken.empty();
    }

    void Clear() {
        accessToken.clear();
        idToken.clear();
        refreshToken.clear();
        userId.clear();
        displayName.clear();
        email.clear();
        membershipTier.clear();
        expiresAt = 0;
        isAuthenticated = false;
        clientToken.clear();
        clientTokenExpiry = 0;
        clientTokenExpiryLength = 0;
        idTokenExpiry = 0;
        accessTokenExpiry = 0;
    }
};

struct SubscriptionInfo {
    std::string membershipTier = "Free";
    std::string subscriptionType;
    std::string subscriptionSubType;
    double allottedHours = 0;
    double purchasedHours = 0;
    double rolledOverHours = 0;
    double usedHours = 0;
    double remainingHours = 0;
    double totalHours = 0;
    bool isUnlimited = false;
    bool isGamePlayAllowed = true;
};

class PersistentDeviceUUID {
public:
    static std::string GetUUID();
private:
    static std::string s_cachedUUID;
};

}
