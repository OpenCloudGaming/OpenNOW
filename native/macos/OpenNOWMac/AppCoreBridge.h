#pragma once

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

@interface ONAppCoreBridge : NSObject

@property(nonatomic, readonly) NSString *coreVersion;
@property(nonatomic, readonly) BOOL hasAuthService;
@property(nonatomic, readonly) BOOL hasCatalogService;
@property(nonatomic, readonly) BOOL hasSessionService;

- (instancetype)init;
- (NSData *)encodeHeartbeat;
- (NSString *)proofStreamStatus;
- (NSDictionary<NSString *, NSString *> *)defaultStreamPreferences;
- (NSDictionary<NSString *, NSString *> *)accountViewModel;
- (NSDictionary<NSString *, NSString *> *)nativeAuthStatus;
- (NSString *)beginNativeLogin;
- (NSString *)clearNativeAuthState;
- (NSDictionary<NSString *, NSString *> *)homeViewModel;
- (NSArray<NSDictionary<NSString *, NSString *> *> *)catalogViewModels;
- (NSArray<NSDictionary<NSString *, NSString *> *> *)libraryViewModels;
- (NSDictionary<NSString *, NSString *> *)settingsViewModel;
- (NSDictionary<NSString *, NSString *> *)queueViewModel;
- (NSDictionary<NSString *, NSString *> *)streamViewModel;
- (NSDictionary<NSString *, NSString *> *)statsViewModel;
- (NSArray<NSDictionary<NSString *, NSString *> *> *)statusLogViewModels;

@end

NS_ASSUME_NONNULL_END
