#include "OPNAuthService.h"

#include <CommonCrypto/CommonCrypto.h>
#include <AppKit/NSWorkspace.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <cstring>

namespace OPN {

// ═══════════════════════════════════════════════════════════════
// Persistent Device UUID
// ═══════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════
// OAuth PKCE Helpers
// ═══════════════════════════════════════════════════════════════

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

static OAuthState GeneratePKCEState() {
    OAuthState s;
    s.codeVerifier = GenerateRandomString(64);
    s.codeChallenge = ComputeSHA256Base64URL(s.codeVerifier);
    s.state = GenerateRandomString(32);
    s.nonce = GenerateRandomString(32);
    return s;
}

// ═══════════════════════════════════════════════════════════════
// AuthService Singleton
// ═══════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════
// StarFleet User Info (login.nvidia.com/userinfo)
// ═══════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════
// Client Token Fetch (login.nvidia.com/client_token)
// ═══════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════
// Token Refresh
// ═══════════════════════════════════════════════════════════════

void AuthService::RefreshSession(AuthCallback completion) {
    AuthSession session = LoadSavedSession();
    if (!session.isAuthenticated || session.refreshToken.empty()) {
        completion(false, AuthSession{}, "No refresh token available");
        return;
    }

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
        @"grant_type=refresh_token"
        @"&refresh_token=%@"
        @"&client_id=%s",
        [NSString stringWithUTF8String:session.refreshToken.c_str()],
        kOAuthClientId];
    req.HTTPBody = [bodyStr dataUsingEncoding:NSUTF8StringEncoding];

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (error) {
                    completion(false, session, [[error localizedDescription] UTF8String]);
                    return;
                }
                NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                NSDictionary *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
                if (http.statusCode != 200 || !json) {
                    NSString *msg = json ? (json[@"error_description"] ? json[@"error_description"] : @"Token refresh failed")
                                         : @"Token refresh failed";
                    completion(false, session, [msg UTF8String]);
                    return;
                }

                AuthSession refreshed = ParseOAuthSession(json);
                if (refreshed.refreshToken.empty()) refreshed.refreshToken = session.refreshToken;
                refreshed.email = session.email;
                refreshed.displayName = session.displayName;
                refreshed.membershipTier = session.membershipTier;
                refreshed.userId = session.userId;
                if (GetStayLoggedIn()) SaveSession(refreshed);
                completion(true, refreshed, "");
            });
        }];
    [task resume];
}

// ═══════════════════════════════════════════════════════════════
// Server Logout
// ═══════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════
// OAuth 2.0 PKCE Flow (local HTTP server callback)
// ═══════════════════════════════════════════════════════════════

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
            "<html><body><h1>Sign in complete</h1><p>Return to the app.</p>"
            "<script>setTimeout(function(){window.close()}, 500)</script></body></html>";
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

            AuthSession session = ParseOAuthSession(json);
            dispatch_async(dispatch_get_main_queue(), ^{
                completion(true, session, "");
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

// ═══════════════════════════════════════════════════════════════
// Token / Session Persistence (File-Based Plist)
// ═══════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════
// OAuth Session Parser
// ═══════════════════════════════════════════════════════════════

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

} // namespace OPN
