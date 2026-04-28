#import "AppCoreBridge.h"

#import <AppKit/AppKit.h>
#include <memory>
#include <vector>
#import <Security/Security.h>

#include "opennow/open_now_core.hpp"
#include "opennow/service_builders.hpp"
#include "opennow/settings.hpp"
#include "opennow/stream_client.hpp"

@interface ONAppCoreBridge ()
- (NSString *)serviceSummary;
@end

@implementation ONAppCoreBridge {
  std::unique_ptr<opennow::OpenNowCore> _core;
}

static NSString *const ONKeychainService = @"OpenNOWNativeAuth";
static NSString *const ONKeychainAccount = @"pending-login";

static NSData *ONKeychainData(NSString *value) {
  return [value dataUsingEncoding:NSUTF8StringEncoding];
}

static NSString *ONStringFromKeychainData(NSData *data) {
  if (!data) {
    return nil;
  }
  return [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
}

static BOOL ONStorePendingAuthState(NSString *state, NSError **error) {
  NSDictionary *query = @{
    (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
    (__bridge id)kSecAttrService: ONKeychainService,
    (__bridge id)kSecAttrAccount: ONKeychainAccount
  };
  SecItemDelete((__bridge CFDictionaryRef)query);

  NSMutableDictionary *item = [query mutableCopy];
  item[(__bridge id)kSecValueData] = ONKeychainData(state);
  OSStatus status = SecItemAdd((__bridge CFDictionaryRef)item, nil);
  if (status != errSecSuccess && error) {
    *error = [NSError errorWithDomain:NSOSStatusErrorDomain code:status userInfo:nil];
  }
  return status == errSecSuccess;
}

static NSString *ONLoadPendingAuthState(void) {
  NSDictionary *query = @{
    (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
    (__bridge id)kSecAttrService: ONKeychainService,
    (__bridge id)kSecAttrAccount: ONKeychainAccount,
    (__bridge id)kSecReturnData: @YES,
    (__bridge id)kSecMatchLimit: (__bridge id)kSecMatchLimitOne
  };

  CFTypeRef result = nil;
  OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &result);
  if (status != errSecSuccess || result == nil) {
    return nil;
  }

  NSData *data = CFBridgingRelease(result);
  return ONStringFromKeychainData(data);
}

static BOOL ONClearPendingAuthState(void) {
  NSDictionary *query = @{
    (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
    (__bridge id)kSecAttrService: ONKeychainService,
    (__bridge id)kSecAttrAccount: ONKeychainAccount
  };
  OSStatus status = SecItemDelete((__bridge CFDictionaryRef)query);
  return status == errSecSuccess || status == errSecItemNotFound;
}

- (instancetype)init {
  self = [super init];
  if (self) {
    _core = std::make_unique<opennow::OpenNowCore>(opennow::CoreDependencies{});
  }
  return self;
}

- (NSString *)coreVersion {
  const auto version = opennow::native_core_version();
  return [NSString stringWithUTF8String:version.c_str()];
}

- (BOOL)hasAuthService {
  return _core->has_auth_service();
}

- (BOOL)hasCatalogService {
  return _core->has_catalog_service();
}

- (BOOL)hasSessionService {
  return _core->has_session_service();
}

- (NSData *)encodeHeartbeat {
  const std::vector<std::uint8_t> bytes = _core->input_encoder().encode_heartbeat();
  return [NSData dataWithBytes:bytes.data() length:bytes.size()];
}

- (NSString *)proofStreamStatus {
  auto backend = std::make_unique<opennow::ProofWebRtcBackend>();
  opennow::StreamClient client(std::move(backend));
  opennow::StreamClientConfig config;
  config.nvst_sdp.width = 1920;
  config.nvst_sdp.height = 1080;
  config.nvst_sdp.codec = opennow::VideoCodec::H264;
  config.nvst_sdp.color_quality = opennow::ColorQuality::EightBit420;
  config.nvst_sdp.credentials = {"bridge-ufrag", "bridge-pwd", "AA:BB"};
  client.start(config);
  const auto diagnostics = client.diagnostics();
  client.stop();
  return [NSString stringWithUTF8String:diagnostics.connection_state.c_str()];
}

- (NSDictionary<NSString *, NSString *> *)defaultStreamPreferences {
  const auto prefs = opennow::normalize_stream_preferences(
      opennow::VideoCodec::H264,
      opennow::ColorQuality::TenBit420);
  return @{
    @"codec": [NSString stringWithUTF8String:opennow::to_string(prefs.codec).c_str()],
    @"colorQuality": [NSString stringWithUTF8String:opennow::to_string(prefs.color_quality).c_str()],
    @"keyboardLayout": [NSString stringWithUTF8String:opennow::resolve_gfn_keyboard_layout("en-US", "darwin").c_str()]
  };
}

- (NSDictionary<NSString *, NSString *> *)accountViewModel {
  NSDictionary<NSString *, NSString *> *authStatus = [self nativeAuthStatus];
  return @{
    @"title": @"Sign in to OpenNOW",
    @"subtitle": authStatus[@"detail"] ?: @"Native authentication will connect through the C++ auth service.",
    @"provider": _core->has_auth_service() ? @"NVIDIA account" : @"Provider bridge pending",
    @"status": authStatus[@"state"] ?: (_core->has_auth_service() ? @"Ready" : @"Offline scaffold"),
    @"cta": @"Continue with browser sign-in"
  };
}

- (NSDictionary<NSString *, NSString *> *)nativeAuthStatus {
  NSString *pending = ONLoadPendingAuthState();
  if (pending.length > 0) {
    return @{
      @"state": @"Browser login pending",
      @"detail": pending,
      @"canClear": @"true"
    };
  }

  return @{
    @"state": @"Not signed in",
    @"detail": @"Click Continue to open NVIDIA sign-in. The callback exchange is the next native auth step.",
    @"canClear": @"false"
  };
}

- (NSString *)beginNativeLogin {
  NSString *nonce = [NSUUID UUID].UUIDString;
  NSString *deviceId = [NSUUID UUID].UUIDString;
  NSString *challenge = [NSUUID UUID].UUIDString;
  opennow::LoginProvider provider{
      "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg",
      "NVIDIA",
      "NVIDIA",
      "https://prod.cloudmatchbeta.nvidiagrid.net/",
      0,
  };
  const auto authUrl = opennow::build_auth_url({
      provider,
      std::string([challenge UTF8String]),
      2259,
      std::string([nonce UTF8String]),
      std::string([deviceId UTF8String]),
  });
  NSString *urlString = [NSString stringWithUTF8String:authUrl.c_str()];
  NSString *pendingState = [NSString stringWithFormat:@"Opened NVIDIA sign-in. Nonce %@ is stored in Keychain until callback exchange is implemented.", nonce];
  NSError *error = nil;
  if (!ONStorePendingAuthState(pendingState, &error)) {
    return [NSString stringWithFormat:@"Failed to write auth state to Keychain: %@", error.localizedDescription ?: @"unknown error"];
  }

  NSURL *url = [NSURL URLWithString:urlString];
  if (!url || ![[NSWorkspace sharedWorkspace] openURL:url]) {
    return @"Generated auth URL but failed to open the browser.";
  }

  return @"Opened NVIDIA sign-in in your browser.";
}

- (NSString *)clearNativeAuthState {
  return ONClearPendingAuthState() ? @"Cleared native auth state." : @"Failed to clear native auth state.";
}

- (NSDictionary<NSString *, NSString *> *)homeViewModel {
  return @{
    @"headline": @"Ready for native parity",
    @"subtitle": @"Catalog, library, queue, and stream surfaces are wired for bridge data.",
    @"featuredTitle": @"Cyberpunk 2077",
    @"featuredMeta": @"RTX ready - 1440p - Controller supported",
    @"recentTitle": @"No recent native sessions",
    @"serviceState": [self serviceSummary]
  };
}

- (NSArray<NSDictionary<NSString *, NSString *> *> *)catalogViewModels {
  return @[
    @{
      @"title": @"Cyberpunk 2077",
      @"subtitle": @"Action RPG",
      @"meta": @"RTX ON - Steam - Controller",
      @"status": @"Catalog placeholder"
    },
    @{
      @"title": @"Fortnite",
      @"subtitle": @"Battle royale",
      @"meta": @"Epic Games - Keyboard and mouse",
      @"status": @"Available when catalog service is connected"
    },
    @{
      @"title": @"Baldur's Gate 3",
      @"subtitle": @"Role-playing",
      @"meta": @"Steam - Controller",
      @"status": @"Sample browse result"
    }
  ];
}

- (NSArray<NSDictionary<NSString *, NSString *> *> *)libraryViewModels {
  return @[
    @{
      @"title": @"Alan Wake 2",
      @"subtitle": @"Library placeholder",
      @"meta": @"Epic Games - Synced ownership pending",
      @"status": @"Not installed locally"
    },
    @{
      @"title": @"Forza Horizon 5",
      @"subtitle": @"Recently played",
      @"meta": @"Xbox - 1080p60 target",
      @"status": @"Ready after session bridge"
    }
  ];
}

- (NSDictionary<NSString *, NSString *> *)settingsViewModel {
  const auto prefs = opennow::normalize_stream_preferences(
      opennow::VideoCodec::H264,
      opennow::ColorQuality::TenBit420);
  return @{
    @"resolution": @"1920 x 1080",
    @"frameRate": @"60 FPS",
    @"bitrate": @"15 Mbps",
    @"codec": [NSString stringWithUTF8String:opennow::to_string(prefs.codec).c_str()],
    @"colorQuality": [NSString stringWithUTF8String:opennow::to_string(prefs.color_quality).c_str()],
    @"keyboardLayout": [NSString stringWithUTF8String:opennow::resolve_gfn_keyboard_layout("en-US", "darwin").c_str()],
    @"microphone": @"Ask every session"
  };
}

- (NSDictionary<NSString *, NSString *> *)queueViewModel {
  return @{
    @"title": @"Waiting for a rig",
    @"position": @"Position 3",
    @"estimate": @"About 2 minutes",
    @"phase": @"Session allocation",
    @"detail": @"This state is static until native session polling is connected."
  };
}

- (NSDictionary<NSString *, NSString *> *)streamViewModel {
  NSString *proofStatus = [self proofStreamStatus];
  return @{
    @"title": @"Stream Preview",
    @"state": proofStatus,
    @"resolution": @"1920 x 1080",
    @"latency": @"-- ms",
    @"input": @"Heartbeat encoder ready",
    @"transport": @"Native WebRTC proof backend"
  };
}

- (NSDictionary<NSString *, NSString *> *)statsViewModel {
  return @{
    @"fps": @"60",
    @"bitrate": @"15.0 Mbps",
    @"packetLoss": @"0.0%",
    @"rtt": @"-- ms",
    @"decoder": @"VideoToolbox pending",
    @"codec": @"H.264"
  };
}

- (NSArray<NSDictionary<NSString *, NSString *> *> *)statusLogViewModels {
  return @[
    @{
      @"level": @"info",
      @"message": @"macOS SwiftUI parity scaffold loaded",
      @"detail": @"Bridge returned static view models"
    },
    @{
      @"level": _core->has_auth_service() ? @"ok" : @"warn",
      @"message": @"Auth service",
      @"detail": _core->has_auth_service() ? @"configured" : @"pending"
    },
    @{
      @"level": _core->has_catalog_service() ? @"ok" : @"warn",
      @"message": @"Catalog service",
      @"detail": _core->has_catalog_service() ? @"configured" : @"pending"
    },
    @{
      @"level": _core->has_session_service() ? @"ok" : @"warn",
      @"message": @"Session service",
      @"detail": _core->has_session_service() ? @"configured" : @"pending"
    }
  ];
}

- (NSString *)serviceSummary {
  return [NSString stringWithFormat:@"Auth %@ - Catalog %@ - Sessions %@",
                                    _core->has_auth_service() ? @"ready" : @"pending",
                                    _core->has_catalog_service() ? @"ready" : @"pending",
                                    _core->has_session_service() ? @"ready" : @"pending"];
}

@end
