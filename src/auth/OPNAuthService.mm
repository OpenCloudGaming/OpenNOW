#include "OPNAuthService.h"

#include <CommonCrypto/CommonCrypto.h>
#include <AppKit/NSWorkspace.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <cstring>

namespace OPN {





std::string PersistentDeviceUUID::s_cachedUUID;

std::string PersistentDeviceUUID::GetUUID() {
    if (!s_cachedUUID.empty()) return s_cachedUUID;
    NSString *key = @"OPN_PersistentDeviceUUID";
    NSString *legacyKey = @"GFN_PersistentDeviceUUID";
    NSUserDefaults *defaults = [NSUserDefaults standardUserDefaults];
    NSString *stored = [defaults stringForKey:key];
    if (!stored || stored.length == 0) {
        stored = [defaults stringForKey:legacyKey];
        if (stored.length > 0) {
            [defaults setObject:stored forKey:key];
            [defaults synchronize];
        }
    }
    if (stored && stored.length > 0) {
        s_cachedUUID = [stored UTF8String];
        return s_cachedUUID;
    }
    NSString *newUUID = [[NSUUID UUID] UUIDString];
    [[NSUserDefaults standardUserDefaults] setObject:newUUID forKey:key];
    [[NSUserDefaults standardUserDefaults] synchronize];
    s_cachedUUID = [newUUID UTF8String];
    return s_cachedUUID;
}





struct OAuthState {
    std::string codeVerifier;
    std::string codeChallenge;
    std::string state;
    std::string nonce;
};

static std::string GenerateRandomString(size_t length) {
    static const char charset[] =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~";
    std::string result;
    result.reserve(length);
    for (size_t i = 0; i < length; i++) {
        result += charset[arc4random_uniform(sizeof(charset) - 1)];
    }
    return result;
}

static std::string ComputeSHA256Base64URL(const std::string &input) {
    unsigned char hash[CC_SHA256_DIGEST_LENGTH];
    CC_SHA256(input.c_str(), static_cast<CC_LONG>(input.length()), hash);
    NSData *hashData = [NSData dataWithBytes:hash length:CC_SHA256_DIGEST_LENGTH];
    NSString *base64 = [hashData base64EncodedStringWithOptions:0];
    NSString *base64URL = [base64 stringByReplacingOccurrencesOfString:@"+" withString:@"-"];
    base64URL = [base64URL stringByReplacingOccurrencesOfString:@"/" withString:@"_"];
    base64URL = [base64URL stringByReplacingOccurrencesOfString:@"=" withString:@""];
    return std::string([base64URL UTF8String]);
}

static std::string URLEncode(const std::string &value) {
    NSString *str = [NSString stringWithUTF8String:value.c_str()];
    NSCharacterSet *allowed = [NSCharacterSet URLQueryAllowedCharacterSet];
    NSString *encoded = [str stringByAddingPercentEncodingWithAllowedCharacters:allowed];
    return encoded ? std::string([encoded UTF8String]) : value;
}

static std::string FormURLEncode(const std::string &value) {
    NSString *str = [NSString stringWithUTF8String:value.c_str()];
    NSMutableCharacterSet *allowed = [NSMutableCharacterSet alphanumericCharacterSet];
    [allowed addCharactersInString:@"-._~"];
    NSString *encoded = [str stringByAddingPercentEncodingWithAllowedCharacters:allowed];
    return encoded ? std::string([encoded UTF8String]) : value;
}

static OAuthState GeneratePKCEState() {
    OAuthState s;
    s.codeVerifier = GenerateRandomString(64);
    s.codeChallenge = ComputeSHA256Base64URL(s.codeVerifier);
    s.state = GenerateRandomString(32);
    s.nonce = GenerateRandomString(32);
    return s;
}





AuthService &AuthService::Shared() {
    static AuthService instance;
    return instance;
}

AuthService::AuthService() {}

int64_t AuthService::getIdTokenExpiry(NSString *idToken) {
    if (!idToken || idToken.length == 0) return 0;
    NSArray *parts = [idToken componentsSeparatedByString:@"."];
    if (parts.count < 2) return 0;
    NSString *payload = parts[1];
    payload = [payload stringByReplacingOccurrencesOfString:@"-" withString:@"+"];
    payload = [payload stringByReplacingOccurrencesOfString:@"_" withString:@"/"];
    while (payload.length % 4) payload = [payload stringByAppendingString:@"="];
    NSData *payloadData = [[NSData alloc] initWithBase64EncodedString:payload options:0];
    if (!payloadData) return 0;
    NSDictionary *claims = [NSJSONSerialization JSONObjectWithData:payloadData options:0 error:nil];
    if (!claims) return 0;
    NSNumber *exp = claims[@"exp"];
    return exp ? ([exp longLongValue] * 1000) : 0;
}

std::string AuthService::GetPersistentDeviceUUID() {
    return PersistentDeviceUUID::GetUUID();
}





void AuthService::FetchStarFleetUserInfo(const std::string &accessToken,
                                          std::function<void(bool, NSDictionary *, const std::string &)> completion) {
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:
        [NSURL URLWithString:@"https://login.nvidia.com/userinfo"]];
    req.HTTPMethod = @"GET";
    req.timeoutInterval = 10.0;
    [req setValue:[NSString stringWithFormat:@"Bearer %s", accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:@"https://nvfile" forHTTPHeaderField:@"Origin"];
    [req setValue:@"application/json" forHTTPHeaderField:@"Accept"];
    [req setValue:[NSString stringWithUTF8String:kDefaultUserAgent] forHTTPHeaderField:@"User-Agent"];

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (error) {
                    completion(false, nil, [[error localizedDescription] UTF8String]);
                    return;
                }
                NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                if (http.statusCode != 200 || !data) {
                    completion(false, nil, [[NSString stringWithFormat:@"HTTP %ld", (long)http.statusCode] UTF8String]);
                    return;
                }
                NSDictionary *info = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
                completion(true, info, "");
            });
        }];
    [task resume];
}





void AuthService::FetchClientToken(const std::string &accessToken,
    std::function<void(bool, const std::string &, const std::string &)> completion) {
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:
        [NSURL URLWithString:@"https://login.nvidia.com/client_token"]];
    req.HTTPMethod = @"GET";
    req.timeoutInterval = 10.0;
    NSLog(@"[OpenNOW] FetchClientToken: accessToken length=%zu", accessToken.size());
    [req setValue:[NSString stringWithFormat:@"Bearer %s", accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:@"https://nvfile" forHTTPHeaderField:@"Origin"];
    [req setValue:@"application/json, text/plain, */*" forHTTPHeaderField:@"Accept"];
    [req setValue:[NSString stringWithUTF8String:kDefaultUserAgent] forHTTPHeaderField:@"User-Agent"];

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (error) {
                    completion(false, "", [[error localizedDescription] UTF8String]);
                    return;
                }
                NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                if (http.statusCode != 200 || !data) {
                    completion(false, "", [[NSString stringWithFormat:@"HTTP %ld", (long)http.statusCode] UTF8String]);
                    return;
                }
                NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
                NSString *ct = json[@"client_token"];
                if (!ct || ct.length == 0) {
                    completion(false, "", "No client_token in response");
                    return;
                }
                NSString *expiresIn = json[@"expires_in"] ? [json[@"expires_in"] stringValue] : @"";
                completion(true, [ct UTF8String], [expiresIn UTF8String]);
            });
        }];
    [task resume];
}





static constexpr int64_t kClientTokenRefreshWindowMs = 5 * 60 * 1000;
static constexpr int64_t kClientTokenRefreshWindowPercent = 20;

static bool ShouldRefreshClientToken(const AuthSession &session) {
    if (session.clientToken.empty() || session.clientTokenExpiry == 0) return true;
    int64_t remainingMs = session.clientTokenExpiry - AuthSession::CurrentEpochMs();
    if (session.clientTokenExpiryLength > 0) {
        return remainingMs < (session.clientTokenExpiryLength * kClientTokenRefreshWindowPercent) / 100;
    }
    return remainingMs < kClientTokenRefreshWindowMs;
}

static AuthSession MergeRefreshedSession(const AuthSession &saved, const AuthSession &refreshed) {
    AuthSession merged = refreshed;
    if (merged.refreshToken.empty()) merged.refreshToken = saved.refreshToken;
    if (merged.clientToken.empty()) {
        merged.clientToken = saved.clientToken;
        merged.clientTokenExpiry = saved.clientTokenExpiry;
        merged.clientTokenExpiryLength = saved.clientTokenExpiryLength;
    }
    if (merged.email.empty()) merged.email = saved.email;
    if (merged.displayName.empty()) merged.displayName = saved.displayName;
    if (merged.membershipTier.empty()) merged.membershipTier = saved.membershipTier;
    if (merged.userId.empty()) merged.userId = saved.userId;
    return merged;
}

static NSMutableURLRequest *CreateTokenRequest(NSString *body) {
    NSString *tokenURLStr = [NSString stringWithUTF8String:AuthService::kOAuthTokenURL];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:tokenURLStr]];
    req.HTTPMethod = @"POST";
    req.timeoutInterval = 15.0;
    [req setValue:@"application/x-www-form-urlencoded; charset=UTF-8" forHTTPHeaderField:@"Content-Type"];
    [req setValue:@"application/json, text/plain, */*" forHTTPHeaderField:@"Accept"];
    [req setValue:@"https://nvfile" forHTTPHeaderField:@"Origin"];
    [req setValue:@"https://nvfile/" forHTTPHeaderField:@"Referer"];
    [req setValue:[NSString stringWithUTF8String:AuthService::kDefaultUserAgent] forHTTPHeaderField:@"User-Agent"];
    req.HTTPBody = [body dataUsingEncoding:NSUTF8StringEncoding];
    return req;
}

static NSString *TokenRefreshErrorMessage(NSHTTPURLResponse *http, NSDictionary *json, NSString *fallback) {
    if (json) {
        NSString *description = [json[@"error_description"] isKindOfClass:NSString.class] ? json[@"error_description"] : nil;
        if (description.length > 0) return description;
        NSString *message = [json[@"message"] isKindOfClass:NSString.class] ? json[@"message"] : nil;
        if (message.length > 0) return message;
        NSString *error = [json[@"error"] isKindOfClass:NSString.class] ? json[@"error"] : nil;
        if (error.length > 0) return error;
    }
    if (http) return [NSString stringWithFormat:@"%@ (HTTP %ld)", fallback, (long)http.statusCode];
    return fallback;
}

static void EnsureClientToken(AuthSession session, std::function<void(AuthSession)> completion) {
    if (!session.isAuthenticated || !session.IsAccessTokenValid() || !ShouldRefreshClientToken(session)) {
        completion(session);
        return;
    }

    AuthService::Shared().FetchClientToken(session.accessToken,
        [session, completion](bool success, const std::string &clientToken, const std::string &expiresInText) mutable {
            if (success && !clientToken.empty()) {
                session.clientToken = clientToken;
                int64_t expiresIn = 86400;
                if (!expiresInText.empty()) {
                    NSString *rawExpires = [NSString stringWithUTF8String:expiresInText.c_str()];
                    int64_t parsedExpires = [rawExpires longLongValue];
                    if (parsedExpires > 0) expiresIn = parsedExpires;
                }
                session.clientTokenExpiry = AuthSession::CurrentEpochMs() + (expiresIn * 1000);
                session.clientTokenExpiryLength = expiresIn * 1000;
            }
            completion(session);
        });
}

static void CompleteRefreshWithSession(AuthSession session, AuthCallback completion) {
    EnsureClientToken(session, [completion](AuthSession enriched) {
        AuthService::Shared().SaveSession(enriched);
        completion(true, enriched, "");
    });
}

void AuthService::RefreshSession(AuthCallback completion, bool forceRefresh) {
    AuthSession session = LoadSavedSession();
    if (!session.isAuthenticated) {
        completion(false, AuthSession{}, "No saved session available");
        return;
    }

    if (!forceRefresh && session.IsAccessTokenValid() && ShouldRefreshClientToken(session)) {
        CompleteRefreshWithSession(session, completion);
        return;
    }

    AuthCallback completionCopy = completion;
    AuthSession savedSession = session;

    auto refreshWithOAuthToken = [completionCopy, savedSession]() {
        if (savedSession.refreshToken.empty()) {
            if (savedSession.IsAccessTokenValid()) {
                CompleteRefreshWithSession(savedSession, completionCopy);
            } else {
                completionCopy(false, savedSession, "No refresh mechanism available");
            }
            return;
        }

        NSString *bodyStr = [NSString stringWithFormat:
            @"grant_type=refresh_token"
            @"&refresh_token=%s"
            @"&client_id=%s",
            FormURLEncode(savedSession.refreshToken).c_str(),
            FormURLEncode(kOAuthClientId).c_str()];
        NSMutableURLRequest *req = CreateTokenRequest(bodyStr);
        NSURLSessionDataTask *task = [[NSURLSession sharedSession]
            dataTaskWithRequest:req
            completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    if (error) {
                        completionCopy(false, savedSession, [[error localizedDescription] UTF8String]);
                        return;
                    }
                    NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                    NSDictionary *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
                    if (http.statusCode != 200 || !json) {
                        NSString *msg = TokenRefreshErrorMessage(http, json, @"Token refresh failed");
                        completionCopy(false, savedSession, [msg UTF8String]);
                        return;
                    }

                    AuthSession refreshed = MergeRefreshedSession(savedSession, AuthService::ParseOAuthSession(json));
                    CompleteRefreshWithSession(refreshed, completionCopy);
                });
            }];
        [task resume];
    };

    if (session.clientToken.empty()) {
        refreshWithOAuthToken();
        return;
    }

    NSString *bodyStr = [NSString stringWithFormat:
        @"grant_type=urn%%3Aietf%%3Aparams%%3Aoauth%%3Agrant-type%%3Aclient_token"
        @"&client_token=%s"
        @"&client_id=%s"
        @"&sub=%s",
        FormURLEncode(session.clientToken).c_str(),
        FormURLEncode(kOAuthClientId).c_str(),
        FormURLEncode(session.userId).c_str()];
    NSMutableURLRequest *req = CreateTokenRequest(bodyStr);
    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                NSDictionary *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
                if (error || http.statusCode != 200 || !json) {
                    refreshWithOAuthToken();
                    return;
                }

                AuthSession refreshed = MergeRefreshedSession(savedSession, AuthService::ParseOAuthSession(json));
                CompleteRefreshWithSession(refreshed, completionCopy);
            });
        }];
    [task resume];
}





void AuthService::ServerLogout(const std::string &idToken,
                                const std::string &locale,
                                SimpleCallback completion) {
    if (idToken.empty()) {
        ClearSession();
        completion(true, "");
        return;
    }

    std::string loc = locale.empty() ? "en_US" : locale;
    NSString *urlStr = [NSString stringWithFormat:@"%s?id_token_hint=%s&ui_locales=%s",
                        kOAuthLogoutURL,
                        URLEncode(idToken).c_str(),
                        URLEncode(loc).c_str()];

    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:urlStr]];
    req.timeoutInterval = 10.0;

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *, NSURLResponse *, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                ClearSession();
                if (error) {
                    completion(false, [[error localizedDescription] UTF8String]);
                    return;
                }
                completion(true, "");
            });
        }];
    [task resume];
}





static int FindAvailablePort() {
    static const int candidatePorts[] = {2259, 6460, 7119, 8870, 9096};
    for (int port : candidatePorts) {
        int sock = socket(AF_INET, SOCK_STREAM, 0);
        if (sock < 0) continue;
        struct sockaddr_in addr = {};
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = INADDR_ANY;
        addr.sin_port = htons(port);
        int reuse = 1;
        setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
        if (bind(sock, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
            close(sock);
            return port;
        }
        close(sock);
    }
    return 0;
}

static std::string GenerateOpenNOWDeviceId() {
    char hostname[256] = {0};
    gethostname(hostname, sizeof(hostname));
    const char *user = getenv("USER");
    if (!user) user = "unknown";
    std::string input = std::string(hostname) + ":" + std::string(user) + ":opennow-stable";
    unsigned char hash[CC_SHA256_DIGEST_LENGTH];
    CC_SHA256(input.c_str(), static_cast<CC_LONG>(input.length()), hash);
    NSMutableString *hex = [NSMutableString stringWithCapacity:CC_SHA256_DIGEST_LENGTH * 2];
    for (int i = 0; i < CC_SHA256_DIGEST_LENGTH; i++)
        [hex appendFormat:@"%02x", hash[i]];
    return std::string([hex UTF8String]);
}

void AuthService::StartOAuthLogin(AuthCallback completion) {
    int port = FindAvailablePort();
    if (port == 0) {
        completion(false, AuthSession{}, "No available port for OAuth callback");
        return;
    }

    OAuthState pkce = GeneratePKCEState();
    std::string deviceId = GenerateOpenNOWDeviceId();
    std::string nonceHex = GenerateRandomString(32);
    std::string redirectUri = "http://localhost:" + std::to_string(port);

    NSString *authorizeURLStr = [NSString stringWithFormat:
        @"%s?response_type=code"
        @"&device_id=%s"
        @"&scope=%s"
        @"&client_id=%s"
        @"&redirect_uri=%s"
        @"&ui_locales=en_US"
        @"&nonce=%s"
        @"&prompt=select_account"
        @"&code_challenge=%s"
        @"&code_challenge_method=S256"
        @"&idp_id=%s"
        @"&state=%s",
        kOAuthAuthorizeURL,
        URLEncode(deviceId).c_str(),
        URLEncode(kOAuthScope).c_str(),
        kOAuthClientId,
        URLEncode(redirectUri).c_str(),
        URLEncode(nonceHex).c_str(),
        pkce.codeChallenge.c_str(),
        kDefaultIdpId,
        pkce.state.c_str()];

    int serverSock = socket(AF_INET, SOCK_STREAM, 0);
    int reuse = 1;
    setsockopt(serverSock, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
    struct sockaddr_in addr = {};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = INADDR_ANY;
    addr.sin_port = htons(port);
    bind(serverSock, (struct sockaddr *)&addr, sizeof(addr));
    listen(serverSock, 1);

    __block bool completed = NO;
    __block int blockSock = serverSock;
    __block OAuthState blockPkce = pkce;

    dispatch_async(dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT, 0), ^{
        int clientSock = accept(blockSock, NULL, NULL);
        close(blockSock);
        if (clientSock < 0) {
            if (!completed) {
                completed = YES;
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, "Failed to accept OAuth callback");
                });
            }
            return;
        }

        char buf[4096] = {0};
        ssize_t n = recv(clientSock, buf, sizeof(buf) - 1, 0);
        if (n <= 0) {
            close(clientSock);
            if (!completed) {
                completed = YES;
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, "Empty OAuth callback request");
                });
            }
            return;
        }

        const char *response =
            "HTTP/1.1 200 OK\r\n"
            "Content-Type: text/html; charset=utf-8\r\n"
            "Connection: close\r\n"
            "\r\n"
            R"HTML(<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OpenNOW Sign In Complete</title>
<style>
:root {
  color-scheme: dark;
  --bg: #050807;
  --panel: rgba(11, 20, 16, 0.78);
  --panel-line: rgba(173, 255, 202, 0.24);
  --text: #f1fff7;
  --muted: #a8c3b4;
  --green: #79f2a7;
  --green-deep: #1dd46e;
  --cyan: #79e9ff;
}

* { box-sizing: border-box; }

html,
body {
  width: 100%;
  min-height: 100%;
  margin: 0;
}

body {
  display: grid;
  place-items: center;
  overflow: hidden;
  padding: 32px;
  color: var(--text);
  background:
    radial-gradient(circle at 22% 18%, rgba(121, 242, 167, 0.22), transparent 28%),
    radial-gradient(circle at 78% 80%, rgba(121, 233, 255, 0.16), transparent 30%),
    linear-gradient(135deg, #030504 0%, #07110d 48%, #020403 100%);
  font-family: "Avenir Next", ui-rounded, "SF Pro Rounded", "Segoe UI", sans-serif;
}

body::before {
  position: fixed;
  inset: -20%;
  content: "";
  background-image:
    linear-gradient(rgba(121, 242, 167, 0.055) 1px, transparent 1px),
    linear-gradient(90deg, rgba(121, 242, 167, 0.055) 1px, transparent 1px);
  background-size: 42px 42px;
  mask-image: radial-gradient(circle at center, black 0%, transparent 68%);
  transform: perspective(700px) rotateX(58deg) translateY(12%);
}

.card {
  position: relative;
  width: min(520px, 100%);
  padding: 38px;
  overflow: hidden;
  background: var(--panel);
  border: 1px solid var(--panel-line);
  border-radius: 30px;
  box-shadow:
    0 36px 110px rgba(0, 0, 0, 0.56),
    inset 0 1px 0 rgba(255, 255, 255, 0.08);
  backdrop-filter: blur(24px) saturate(1.28);
}

.card::before {
  position: absolute;
  inset: 0;
  content: "";
  background: linear-gradient(115deg, rgba(255, 255, 255, 0.16), transparent 28%, transparent 62%, rgba(121, 242, 167, 0.12));
  pointer-events: none;
}

.mark {
  position: relative;
  display: grid;
  width: 76px;
  height: 76px;
  place-items: center;
  margin-bottom: 28px;
  border-radius: 24px;
  background: linear-gradient(145deg, var(--green), var(--green-deep));
  box-shadow: 0 0 0 10px rgba(121, 242, 167, 0.08), 0 22px 48px rgba(29, 212, 110, 0.32);
}

.mark svg {
  width: 42px;
  height: 42px;
  fill: none;
  stroke: #031008;
  stroke-width: 4;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.eyebrow {
  position: relative;
  margin: 0 0 12px;
  color: var(--green);
  font-size: 12px;
  font-weight: 800;
  letter-spacing: 0.28em;
  text-transform: uppercase;
}

h1 {
  position: relative;
  max-width: 10ch;
  margin: 0;
  font-size: clamp(42px, 8vw, 68px);
  line-height: 0.92;
  letter-spacing: -0.065em;
}

.copy {
  position: relative;
  max-width: 34rem;
  margin: 22px 0 0;
  color: var(--muted);
  font-size: 17px;
  line-height: 1.58;
}

.status {
  position: relative;
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 30px;
  padding: 14px 16px;
  color: #d9ffe8;
  background: rgba(121, 242, 167, 0.10);
  border: 1px solid rgba(121, 242, 167, 0.18);
  border-radius: 16px;
  font-size: 14px;
  font-weight: 700;
}

.pulse {
  width: 10px;
  height: 10px;
  flex: 0 0 auto;
  border-radius: 999px;
  background: var(--green);
  box-shadow: 0 0 0 0 rgba(121, 242, 167, 0.6);
  animation: pulse 1.45s ease-out infinite;
}

.actions {
  position: relative;
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 24px;
}

button {
  min-height: 44px;
  padding: 0 18px;
  color: #031008;
  background: var(--green);
  border: 0;
  border-radius: 999px;
  font: inherit;
  font-size: 14px;
  font-weight: 800;
  cursor: pointer;
}

.hint {
  align-self: center;
  color: rgba(168, 195, 180, 0.78);
  font-size: 13px;
}

@keyframes pulse {
  70% { box-shadow: 0 0 0 14px rgba(121, 242, 167, 0); }
  100% { box-shadow: 0 0 0 0 rgba(121, 242, 167, 0); }
}

@media (prefers-reduced-motion: reduce) {
  .pulse { animation: none; }
}

@media (max-width: 520px) {
  body { padding: 18px; }
  .card { padding: 28px; border-radius: 24px; }
  .mark { width: 64px; height: 64px; border-radius: 20px; }
}
</style>
</head>
<body>
<main class="card" aria-labelledby="title">
  <div class="mark" aria-hidden="true">
    <svg viewBox="0 0 48 48"><path d="M12 25.5 20.5 34 37 15"/></svg>
  </div>
  <p class="eyebrow">OpenNOW</p>
  <h1 id="title">Sign in complete</h1>
  <p class="copy">Your NVIDIA account is connected. OpenNOW is receiving the secure token now, so you can return to the app and continue to your cloud gaming library.</p>
  <div class="status" role="status" aria-live="polite"><span class="pulse" aria-hidden="true"></span><span id="statusText">Returning you to OpenNOW...</span></div>
  <div class="actions">
    <button type="button" onclick="window.close()">Close this window</button>
    <span class="hint">If it stays open, switch back to OpenNOW.</span>
  </div>
  <noscript><p class="copy">JavaScript is disabled. You can safely close this window and return to OpenNOW.</p></noscript>
</main>
<script>
setTimeout(function () {
  var statusText = document.getElementById('statusText');
  if (statusText) statusText.textContent = 'You can safely close this window.';
  window.close();
}, 1200);
</script>
</body>
</html>)HTML";
        send(clientSock, response, strlen(response), 0);
        close(clientSock);

        std::string request(buf, n);
        size_t pathStart = request.find("GET ");
        if (pathStart == std::string::npos) {
            if (!completed) {
                completed = YES;
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, "Invalid OAuth callback request");
                });
            }
            return;
        }
        pathStart += 4;
        size_t pathEnd = request.find(" ", pathStart);
        if (pathEnd == std::string::npos) {
            if (!completed) {
                completed = YES;
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, "Malformed OAuth callback request");
                });
            }
            return;
        }
        std::string path = request.substr(pathStart, pathEnd - pathStart);
        NSString *query = nil;
        if (path.find("?") != std::string::npos)
            query = [NSString stringWithUTF8String:path.substr(path.find("?") + 1).c_str()];

        NSDictionary *params = AuthService::parseQueryString(query);
        NSString *code = params[@"code"];
        NSString *state = params[@"state"];

        if (!code) {
            if (!completed) {
                completed = YES;
                NSString *errorMsg = params[@"error_description"]
                    ? params[@"error_description"]
                    : (params[@"error"] ? params[@"error"] : @"Unknown error");
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, [errorMsg UTF8String]);
                });
            }
            return;
        }

        NSString *expectedState = [NSString stringWithUTF8String:blockPkce.state.c_str()];
        if (![state isEqualToString:expectedState]) {
            if (!completed) {
                completed = YES;
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, "State mismatch — possible CSRF");
                });
            }
            return;
        }

        if (!completed) {
            completed = YES;
            NSString *codeVerifier = [NSString stringWithUTF8String:blockPkce.codeVerifier.c_str()];
            NSString *redirectStr = [NSString stringWithUTF8String:redirectUri.c_str()];
            doOAuthTokenExchange(code, codeVerifier, redirectStr, completion);
        }
    });

    dispatch_async(dispatch_get_main_queue(), ^{
        NSURL *authURL = [NSURL URLWithString:authorizeURLStr];
        [[NSWorkspace sharedWorkspace] openURL:authURL];
    });
}

void AuthService::doOAuthTokenExchange(NSString *authCode, NSString *codeVerifier,
                                        NSString *redirectUri, AuthCallback completion) {
    NSString *tokenURLStr = [NSString stringWithUTF8String:kOAuthTokenURL];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:tokenURLStr]];
    req.HTTPMethod = @"POST";
    req.timeoutInterval = 15.0;
    [req setValue:@"application/x-www-form-urlencoded; charset=UTF-8" forHTTPHeaderField:@"Content-Type"];
    [req setValue:@"application/json, text/plain, */*" forHTTPHeaderField:@"Accept"];
    [req setValue:@"https://nvfile" forHTTPHeaderField:@"Origin"];
    [req setValue:@"https://nvfile/" forHTTPHeaderField:@"Referer"];
    [req setValue:[NSString stringWithUTF8String:kDefaultUserAgent] forHTTPHeaderField:@"User-Agent"];

    NSString *bodyStr = [NSString stringWithFormat:
        @"grant_type=authorization_code"
        @"&code=%@"
        @"&redirect_uri=%@"
        @"&code_verifier=%@",
        authCode,
        redirectUri,
        codeVerifier];
    req.HTTPBody = [bodyStr dataUsingEncoding:NSUTF8StringEncoding];

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            if (error) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, [[error localizedDescription] UTF8String]);
                });
                return;
            }
            NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
            NSDictionary *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;

            if (http.statusCode != 200 || !json) {
                NSString *msg = json ? (json[@"error_description"] ? json[@"error_description"]
                    : (json[@"message"] ? json[@"message"] : @"Token exchange failed"))
                    : [NSString stringWithFormat:@"Token exchange failed (%ld)", (long)http.statusCode];
                dispatch_async(dispatch_get_main_queue(), ^{
                    completion(false, AuthSession{}, [msg UTF8String]);
                });
                return;
            }

            dispatch_async(dispatch_get_main_queue(), ^{
                AuthSession session = ParseOAuthSession(json);
                EnsureClientToken(session, [completion](AuthSession enriched) {
                    completion(true, enriched, "");
                });
            });
        }];
    [task resume];
}

NSDictionary *AuthService::parseQueryString(NSString *query) {
    NSMutableDictionary *params = [NSMutableDictionary dictionary];
    if (!query || query.length == 0) return params;

    NSArray *pairs = [query componentsSeparatedByString:@"&"];
    for (NSString *pair in pairs) {
        NSArray *components = [pair componentsSeparatedByString:@"="];
        if (components.count == 2) {
            NSString *key = [components[0] stringByRemovingPercentEncoding];
            NSString *value = [components[1] stringByRemovingPercentEncoding];
            if (key) params[key] = value ? value : @"";
        }
    }
    return params;
}





static NSString *SessionStorageDirectory() {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSArray<NSString *> *paths = NSSearchPathForDirectoriesInDomains(
        NSApplicationSupportDirectory, NSUserDomainMask, YES);
    if (paths.count == 0) return nil;
    NSString *basePath = paths[0];
    NSString *dir = [basePath stringByAppendingPathComponent:@"OpenNOW"];
    if (![fm fileExistsAtPath:dir]) {
        NSError *error = nil;
        NSDictionary *attrs = @{NSFilePosixPermissions: @(0700)};
        [fm createDirectoryAtPath:dir
      withIntermediateDirectories:YES
                       attributes:attrs
                            error:&error];
        if (error) return nil;
    }
    return dir;
}

static NSString *LegacySessionFilePath() {
    NSArray<NSString *> *paths = NSSearchPathForDirectoriesInDomains(
        NSApplicationSupportDirectory, NSUserDomainMask, YES);
    if (paths.count == 0) return nil;
    NSString *legacyDir = [paths[0] stringByAppendingPathComponent:@"com.nvidia.geforcenow"];
    return [legacyDir stringByAppendingPathComponent:@"session.plist"];
}

static NSString *SessionFilePath() {
    NSString *dir = SessionStorageDirectory();
    if (!dir) return nil;
    return [dir stringByAppendingPathComponent:@"session.plist"];
}

static NSString *AccountsFilePath() {
    NSString *dir = SessionStorageDirectory();
    if (!dir) return nil;
    return [dir stringByAppendingPathComponent:@"accounts.plist"];
}

static NSString *SessionFilePathForRead() {
    NSString *path = SessionFilePath();
    if (path && [[NSFileManager defaultManager] fileExistsAtPath:path]) {
        return path;
    }
    NSString *legacyPath = LegacySessionFilePath();
    if (legacyPath && [[NSFileManager defaultManager] fileExistsAtPath:legacyPath]) {
        return legacyPath;
    }
    return path;
}

static void SetFileOwnerOnly(NSString *path) {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSDictionary *attrs = @{NSFilePosixPermissions: @(0600)};
    [fm setAttributes:attrs ofItemAtPath:path error:nil];
}

static NSString *SessionIdentityFromValues(NSString *userId, NSString *email, NSString *displayName, NSString *accessToken) {
    if (userId.length > 0) return userId;
    if (email.length > 0) return email;
    if (displayName.length > 0) return displayName;
    if (accessToken.length > 0) return accessToken;
    return nil;
}

static NSString *SessionIdentityFromDictionary(NSDictionary *dict) {
    if (![dict isKindOfClass:[NSDictionary class]]) return nil;
    return SessionIdentityFromValues(dict[@"user_id"], dict[@"email"], dict[@"display_name"], dict[@"access_token"]);
}

static NSString *SessionIdentityFromSession(const AuthSession &session) {
    NSString *userId = session.userId.empty() ? nil : [NSString stringWithUTF8String:session.userId.c_str()];
    NSString *email = session.email.empty() ? nil : [NSString stringWithUTF8String:session.email.c_str()];
    NSString *displayName = session.displayName.empty() ? nil : [NSString stringWithUTF8String:session.displayName.c_str()];
    NSString *accessToken = session.accessToken.empty() ? nil : [NSString stringWithUTF8String:session.accessToken.c_str()];
    return SessionIdentityFromValues(userId, email, displayName, accessToken);
}

static NSMutableDictionary *DictionaryFromSession(const AuthSession &session) {
    NSMutableDictionary *dict = [NSMutableDictionary dictionary];
    auto putStr = [&](NSString *key, const std::string &val) {
        if (!val.empty()) dict[key] = [NSString stringWithUTF8String:val.c_str()];
    };
    putStr(@"access_token",  session.accessToken);
    putStr(@"id_token",      session.idToken);
    putStr(@"refresh_token", session.refreshToken);
    putStr(@"client_token",  session.clientToken);
    putStr(@"user_id",       session.userId);
    putStr(@"display_name",  session.displayName);
    putStr(@"email",         session.email);
    putStr(@"membership_tier", session.membershipTier);
    dict[@"expires_at"]                 = @(session.expiresAt);
    dict[@"access_token_expiry"]        = @(session.accessTokenExpiry);
    dict[@"client_token_expiry"]        = @(session.clientTokenExpiry);
    dict[@"client_token_expiry_length"] = @(session.clientTokenExpiryLength);
    dict[@"id_token_expiry"]            = @(session.idTokenExpiry);
    return dict;
}

static AuthSession SessionFromDictionary(NSDictionary *dict) {
    AuthSession session;
    if (![dict isKindOfClass:[NSDictionary class]]) return session;
    NSString *accessToken = [dict[@"access_token"] isKindOfClass:NSString.class] ? dict[@"access_token"] : nil;
    if (accessToken.length == 0) return session;

    session.accessToken = [accessToken UTF8String];
    auto getStr = [&](NSString *key) -> std::string {
        NSString *v = [dict[key] isKindOfClass:NSString.class] ? dict[key] : nil;
        return v ? std::string([v UTF8String]) : std::string();
    };
    session.idToken               = getStr(@"id_token");
    session.refreshToken          = getStr(@"refresh_token");
    session.clientToken           = getStr(@"client_token");
    session.userId                = getStr(@"user_id");
    session.displayName           = getStr(@"display_name");
    session.email                 = getStr(@"email");
    session.membershipTier        = getStr(@"membership_tier").empty()
                                        ? "Free" : getStr(@"membership_tier");

    auto getInt64 = [&](NSString *key) -> int64_t {
        NSNumber *n = [dict[key] isKindOfClass:NSNumber.class] ? dict[key] : nil;
        return n ? [n longLongValue] : 0;
    };
    session.expiresAt               = getInt64(@"expires_at");
    session.accessTokenExpiry       = getInt64(@"access_token_expiry");
    session.clientTokenExpiry       = getInt64(@"client_token_expiry");
    session.clientTokenExpiryLength = getInt64(@"client_token_expiry_length");
    session.idTokenExpiry           = getInt64(@"id_token_expiry");
    session.isAuthenticated = true;
    return session;
}

static NSDictionary *LoadPropertyListDictionary(NSString *path) {
    if (!path || ![[NSFileManager defaultManager] fileExistsAtPath:path]) return nil;
    NSData *plistData = [NSData dataWithContentsOfFile:path];
    if (!plistData) return nil;
    NSPropertyListFormat format = NSPropertyListXMLFormat_v1_0;
    id plist = [NSPropertyListSerialization propertyListWithData:plistData
                                                         options:NSPropertyListImmutable
                                                          format:&format
                                                           error:nil];
    return [plist isKindOfClass:NSDictionary.class] ? (NSDictionary *)plist : nil;
}

static AuthSession LoadLegacySingleSession() {
    NSString *path = SessionFilePathForRead();
    NSDictionary *dict = LoadPropertyListDictionary(path);
    return SessionFromDictionary(dict);
}

static NSArray *LoadAccountDictionaries(NSString **activeUserId) {
    NSDictionary *store = LoadPropertyListDictionary(AccountsFilePath());
    NSString *active = [store[@"active_user_id"] isKindOfClass:NSString.class] ? store[@"active_user_id"] : nil;
    NSArray *accounts = [store[@"accounts"] isKindOfClass:NSArray.class] ? store[@"accounts"] : nil;
    if (activeUserId) *activeUserId = active;
    return accounts ?: @[];
}

static void SaveAccountDictionaries(NSArray *accounts, NSString *activeUserId) {
    NSString *path = AccountsFilePath();
    if (!path) return;
    NSMutableDictionary *store = [NSMutableDictionary dictionary];
    store[@"accounts"] = accounts ?: @[];
    if (activeUserId.length > 0) store[@"active_user_id"] = activeUserId;

    NSData *plistData = [NSPropertyListSerialization dataWithPropertyList:store
                                                                    format:NSPropertyListXMLFormat_v1_0
                                                                   options:0
                                                                     error:nil];
    if (!plistData) return;
    [plistData writeToFile:path options:NSDataWritingAtomic error:nil];
    SetFileOwnerOnly(path);
}

void AuthService::SaveSession(const AuthSession &session) {
    if (!session.isAuthenticated || session.accessToken.empty()) return;
    NSString *identity = SessionIdentityFromSession(session);
    if (identity.length == 0) return;

    NSString *activeUserId = nil;
    NSArray *existing = LoadAccountDictionaries(&activeUserId);
    NSMutableArray *accounts = [NSMutableArray array];
    for (NSDictionary *account in existing) {
        NSString *accountIdentity = SessionIdentityFromDictionary(account);
        if (accountIdentity.length > 0 && [accountIdentity isEqualToString:identity]) continue;
        [accounts addObject:account];
    }
    [accounts insertObject:DictionaryFromSession(session) atIndex:0];
    SaveAccountDictionaries(accounts, identity);

    [[NSUserDefaults standardUserDefaults] setBool:YES forKey:@"OPN_HasSavedSession"];
    [[NSUserDefaults standardUserDefaults] setObject:identity forKey:@"OPN_ActiveUserId"];
    [[NSUserDefaults standardUserDefaults] synchronize];
}

AuthSession AuthService::LoadSavedSession() {
    NSUserDefaults *defaults = [NSUserDefaults standardUserDefaults];
    NSString *storeActiveUserId = nil;
    NSArray *accounts = LoadAccountDictionaries(&storeActiveUserId);
    NSString *preferredUserId = [defaults stringForKey:@"OPN_ActiveUserId"] ?: storeActiveUserId;
    NSDictionary *fallback = nil;
    for (NSDictionary *account in accounts) {
        if (![account isKindOfClass:NSDictionary.class]) continue;
        if (!fallback) fallback = account;
        NSString *identity = SessionIdentityFromDictionary(account);
        if (preferredUserId.length > 0 && [identity isEqualToString:preferredUserId]) {
            AuthSession session = SessionFromDictionary(account);
            if (session.isAuthenticated) return session;
        }
    }
    if (fallback) {
        AuthSession session = SessionFromDictionary(fallback);
        NSString *identity = SessionIdentityFromDictionary(fallback);
        if (identity.length > 0) [defaults setObject:identity forKey:@"OPN_ActiveUserId"];
        [defaults setBool:YES forKey:@"OPN_HasSavedSession"];
        [defaults synchronize];
        return session;
    }

    if (![defaults boolForKey:@"OPN_HasSavedSession"] && ![defaults boolForKey:@"GFN_HasSavedSession"])
        return AuthSession{};

    AuthSession legacy = LoadLegacySingleSession();
    if (legacy.isAuthenticated) SaveSession(legacy);
    return legacy;
}

std::vector<AuthSession> AuthService::LoadSavedSessions() {
    std::vector<AuthSession> sessions;
    NSArray *accounts = LoadAccountDictionaries(nullptr);
    for (NSDictionary *account in accounts) {
        AuthSession session = SessionFromDictionary(account);
        if (session.isAuthenticated) sessions.push_back(session);
    }
    if (sessions.empty()) {
        AuthSession legacy = LoadLegacySingleSession();
        if (legacy.isAuthenticated) sessions.push_back(legacy);
    }
    return sessions;
}

AuthSession AuthService::LoadSavedSessionForUserId(const std::string &userId) {
    NSString *target = userId.empty() ? nil : [NSString stringWithUTF8String:userId.c_str()];
    if (target.length == 0) return AuthSession{};
    NSArray *accounts = LoadAccountDictionaries(nullptr);
    for (NSDictionary *account in accounts) {
        NSString *identity = SessionIdentityFromDictionary(account);
        if ([identity isEqualToString:target]) return SessionFromDictionary(account);
    }
    return AuthSession{};
}

void AuthService::SetActiveSessionUserId(const std::string &userId) {
    NSString *identity = userId.empty() ? nil : [NSString stringWithUTF8String:userId.c_str()];
    if (identity.length == 0) return;
    NSString *activeUserId = nil;
    NSArray *accounts = LoadAccountDictionaries(&activeUserId);
    BOOL found = NO;
    for (NSDictionary *account in accounts) {
        if ([SessionIdentityFromDictionary(account) isEqualToString:identity]) {
            found = YES;
            break;
        }
    }
    if (!found) return;
    SaveAccountDictionaries(accounts, identity);
    [NSUserDefaults.standardUserDefaults setObject:identity forKey:@"OPN_ActiveUserId"];
    [NSUserDefaults.standardUserDefaults setBool:YES forKey:@"OPN_HasSavedSession"];
    [NSUserDefaults.standardUserDefaults synchronize];
}

void AuthService::RemoveSavedSession(const std::string &userId) {
    NSString *identity = userId.empty() ? nil : [NSString stringWithUTF8String:userId.c_str()];
    if (identity.length == 0) return;
    NSString *activeUserId = nil;
    NSArray *existing = LoadAccountDictionaries(&activeUserId);
    NSMutableArray *accounts = [NSMutableArray array];
    for (NSDictionary *account in existing) {
        if ([SessionIdentityFromDictionary(account) isEqualToString:identity]) continue;
        [accounts addObject:account];
    }
    NSString *newActive = [activeUserId isEqualToString:identity] ? nil : activeUserId;
    if (newActive.length == 0 && accounts.count > 0) {
        newActive = SessionIdentityFromDictionary(accounts[0]);
    }
    SaveAccountDictionaries(accounts, newActive);
    if (newActive.length > 0) {
        [NSUserDefaults.standardUserDefaults setObject:newActive forKey:@"OPN_ActiveUserId"];
        [NSUserDefaults.standardUserDefaults setBool:YES forKey:@"OPN_HasSavedSession"];
    } else {
        [NSUserDefaults.standardUserDefaults removeObjectForKey:@"OPN_ActiveUserId"];
        [NSUserDefaults.standardUserDefaults removeObjectForKey:@"OPN_HasSavedSession"];
    }
    [NSUserDefaults.standardUserDefaults synchronize];
}

void AuthService::ClearSession() {
    NSString *activeUserId = [NSUserDefaults.standardUserDefaults stringForKey:@"OPN_ActiveUserId"];
    if (activeUserId.length > 0) {
        RemoveSavedSession([activeUserId UTF8String]);
        return;
    }
    NSString *accountsPath = AccountsFilePath();
    if (accountsPath) {
        [[NSFileManager defaultManager] removeItemAtPath:accountsPath error:nil];
    }
    NSString *path = SessionFilePath();
    if (path) {
        [[NSFileManager defaultManager] removeItemAtPath:path error:nil];
    }
    NSString *legacyPath = LegacySessionFilePath();
    if (legacyPath) {
        [[NSFileManager defaultManager] removeItemAtPath:legacyPath error:nil];
    }
    [[NSUserDefaults standardUserDefaults] removeObjectForKey:@"OPN_HasSavedSession"];
    [[NSUserDefaults standardUserDefaults] removeObjectForKey:@"GFN_HasSavedSession"];
    [[NSUserDefaults standardUserDefaults] removeObjectForKey:@"OPN_ActiveUserId"];
    [[NSUserDefaults standardUserDefaults] synchronize];
}

bool AuthService::GetStayLoggedIn() {
    NSUserDefaults *d = [NSUserDefaults standardUserDefaults];
    if ([d objectForKey:@"OPN_StayLoggedIn"]) return [d boolForKey:@"OPN_StayLoggedIn"];
    if ([d objectForKey:@"GFN_StayLoggedIn"]) return [d boolForKey:@"GFN_StayLoggedIn"];
    return true;
}

void AuthService::SetStayLoggedIn(bool value) {
    [[NSUserDefaults standardUserDefaults] setBool:value forKey:@"OPN_StayLoggedIn"];
    [[NSUserDefaults standardUserDefaults] synchronize];
}





AuthSession AuthService::ParseOAuthSession(NSDictionary *json) {
    AuthSession s;
    {
        NSString *v = json[@"access_token"];
        s.accessToken = v ? std::string([v UTF8String]) : std::string();
    }
    {
        NSString *v = json[@"id_token"];
        s.idToken = v ? std::string([v UTF8String]) : std::string();
    }
    {
        NSString *v = json[@"refresh_token"];
        s.refreshToken = v ? std::string([v UTF8String]) : std::string();
    }
    {
        NSString *v = json[@"client_token"];
        s.clientToken = v ? std::string([v UTF8String]) : std::string();
    }
    {
        NSString *v = json[@"expires_in"];
        int64_t expiresIn = v ? [v longLongValue] : 86400;
        int64_t nowMs = AuthSession::CurrentEpochMs();
        s.accessTokenExpiry = nowMs + (expiresIn * 1000);
        s.expiresAt = static_cast<int64_t>([[NSDate date] timeIntervalSince1970]) + expiresIn;
    }

    {
        NSString *v = json[@"client_token_expires_in"];
        int64_t ctExpiresIn = v ? [v longLongValue] : 0;
        if (ctExpiresIn > 0 && !s.clientToken.empty()) {
            s.clientTokenExpiry = AuthSession::CurrentEpochMs() + (ctExpiresIn * 1000);
            s.clientTokenExpiryLength = ctExpiresIn * 1000;
        }
    }

    NSString *idToken = json[@"id_token"];
    if (idToken) {
        s.idTokenExpiry = getIdTokenExpiry(idToken);
        NSArray *parts = [idToken componentsSeparatedByString:@"."];
        if (parts.count >= 2) {
            NSString *payload = parts[1];
            payload = [payload stringByReplacingOccurrencesOfString:@"-" withString:@"+"];
            payload = [payload stringByReplacingOccurrencesOfString:@"_" withString:@"/"];
            while (payload.length % 4) payload = [payload stringByAppendingString:@"="];
            NSData *payloadData = [[NSData alloc] initWithBase64EncodedString:payload options:0];
            if (payloadData) {
                NSDictionary *claims = [NSJSONSerialization JSONObjectWithData:payloadData options:0 error:nil];
                {
                    NSString *v = claims[@"sub"];
                    s.userId = v ? std::string([v UTF8String]) : std::string();
                }
                {
                    NSString *v = claims[@"name"];
                    s.displayName = v ? std::string([v UTF8String]) : std::string();
                }
                {
                    NSString *v = claims[@"email"];
                    s.email = v ? std::string([v UTF8String]) : std::string();
                }
                {
                    NSString *v = claims[@"membership_tier"];
                    s.membershipTier = v ? std::string([v UTF8String]) : "Free";
                }
            }
        }
    }
    if (s.expiresAt == 0) {
        s.expiresAt = static_cast<int64_t>([[NSDate date] timeIntervalSince1970]) + 86400;
        s.accessTokenExpiry = AuthSession::CurrentEpochMs() + 86400000;
    }
    s.isAuthenticated = !s.accessToken.empty();
    return s;
}

}
