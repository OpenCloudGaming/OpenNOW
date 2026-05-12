#import "OPNAppDelegate.h"
#import "auth/OPNAuthService.h"
#import "games/OPNGameService.h"
#import "streaming/OPNStreamViewController.h"
#include "streaming/OPNStreamPreferences.h"
#import "views/OPNBackdropView.h"
#import "views/OPNEmailEntryView.h"
#import "views/OPNAuthenticatingView.h"
#import "views/OPNErrorView.h"
#import "views/OPNGameCatalogView.h"
#import "views/OPNSettingsView.h"
#import "views/OPNStoreView.h"
#import "common/OPNColorTokens.h"
#import "common/OPNUIHelpers.h"
#include "common/OPNLogCapture.h"
#import "common/OPNAuthTypes.h"
#import "common/OPNGameTypes.h"
#import <CommonCrypto/CommonDigest.h>
#include <algorithm>
#include <cstring>

@interface AppDelegate ()
@property (nonatomic, strong) OPNBackdropView *rootView;
@property (nonatomic, strong) OPNGameCatalogView *catalogView;
@property (nonatomic, strong) OPNSettingsView *settingsView;
@property (nonatomic, strong) OPNStoreView *storeView;
@property (nonatomic, strong) OPNStreamViewController *streamingController;
@property (nonatomic, copy) NSString *currentStreamTitle;
@property (nonatomic, assign) OPN::AuthScreen activeStreamReturnScreen;
@property (nonatomic, assign) BOOL streamLibraryOverlayActive;
@property (nonatomic, strong) NSTimer *gameLibraryRefreshTimer;
@property (nonatomic, assign) std::vector<OPN::GameInfo> cachedGameLibrary;
@property (nonatomic, assign) std::string cachedGameLibraryFingerprint;
@property (nonatomic, assign) std::string cachedGameLibraryAccountIdentifier;
@property (nonatomic, assign) BOOL hasCachedGameLibrary;
@property (nonatomic, assign) BOOL gameLibraryRefreshInFlight;
@property (nonatomic, assign) NSInteger catalogBrowseGeneration;
- (void)configureContentContainerForScreen:(OPN::AuthScreen)screen;
- (void)refreshAccountSummary;
- (void)refreshAccountAvatar;
- (void)refreshStreamRegions;
- (void)refreshAccountMenu;
- (void)addAccount;
- (void)switchToAccountIdentifier:(NSString *)identifier;
- (void)restoreSavedWindowPresentation;
- (void)saveWindowPresentation;
- (void)startGameLibraryRefreshTimer;
- (void)stopGameLibraryRefreshTimer;
- (void)launchGame:(const OPN::GameInfo &)game variantIndex:(int)variantIndex returnScreen:(OPN::AuthScreen)returnScreen;
- (void)showLibraryOverlayForActiveStream;
- (void)returnToActiveStreamFromLibraryOverlay;
- (void)loadStorePanelsWithRetry:(BOOL)canRetry;
- (void)refreshGameLibraryInBackground;
- (void)fetchGameLibraryWithRetry:(BOOL)canRetry
                        completion:(void (^)(BOOL success, const std::vector<OPN::GameInfo> &games))completion;
- (void)browseCatalogWithSearch:(NSString *)searchQuery
                          sortId:(NSString *)sortId
                       filterIds:(const std::vector<std::string> &)filterIds
                        canRetry:(BOOL)canRetry;
@end

@implementation AppDelegate

static NSString *const OPNMainWindowFrameAutosaveName = @"OpenNOW.MainWindowFrame";
static NSString *const OPNMainWindowWasFullScreenKey = @"OpenNOW.MainWindowWasFullScreen";

static BOOL OPNWindowIsFullScreen(NSWindow *window) {
    return window && ((window.styleMask & NSWindowStyleMaskFullScreen) == NSWindowStyleMaskFullScreen);
}

static NSSize OPNResizableWindowMaxSize() {
    return NSMakeSize(16000.0, 16000.0);
}

static void OPNConfigureResizableWindow(NSWindow *window, NSSize minSize, NSSize maxSize) {
    window.styleMask = window.styleMask | NSWindowStyleMaskResizable;
    window.collectionBehavior = window.collectionBehavior | NSWindowCollectionBehaviorFullScreenPrimary;
    NSRect minFrame = [window frameRectForContentRect:NSMakeRect(0, 0, minSize.width, minSize.height)];
    window.minSize = minFrame.size;
    window.maxSize = maxSize;
    window.contentMinSize = minSize;
    window.contentMaxSize = maxSize;
    window.resizeIncrements = NSMakeSize(1.0, 1.0);
    window.contentResizeIncrements = NSMakeSize(1.0, 1.0);
}

static void OPNConfigureLibraryWindow(NSWindow *window) {
    OPNConfigureResizableWindow(window,
                                NSMakeSize(720.0, 480.0),
                                OPNResizableWindowMaxSize());
    window.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
    window.backgroundColor = OpnColor(OPN::kBackground);
}

static void OPNConfigureStreamWindow(NSWindow *window) {
    OPNConfigureResizableWindow(window,
                                NSMakeSize(640.0, 360.0),
                                OPNResizableWindowMaxSize());
}

static NSString *OPNDisplayTier(const std::string &tier) {
    NSString *raw = tier.empty() ? @"Free" : [NSString stringWithUTF8String:tier.c_str()];
    NSString *upper = raw.uppercaseString;
    if ([upper isEqualToString:@"ULTIMATE"]) return @"Ultimate";
    if ([upper isEqualToString:@"PRIORITY"] || [upper isEqualToString:@"PERFORMANCE"]) return @"Priority";
    if ([upper isEqualToString:@"FREE"]) return @"Free";
    return raw.capitalizedString;
}

static NSString *OPNFormatHours(double hours) {
    if (!std::isfinite(hours) || hours < 0) hours = 0;
    NSInteger totalMinutes = MAX(0, (NSInteger)llround(hours * 60.0));
    NSInteger wholeHours = totalMinutes / 60;
    NSInteger minutes = totalMinutes % 60;
    return [NSString stringWithFormat:@"%ldh %02ldm", (long)wholeHours, (long)minutes];
}

static NSString *OPNFormatRemainingPlayTime(const OPN::SubscriptionInfo &subscription) {
    if (subscription.isUnlimited) return @"Unlimited";
    return [NSString stringWithFormat:@"%@ left", OPNFormatHours(subscription.remainingHours)];
}

static std::string OPNAuthSessionIdentifier(const OPN::AuthSession &session) {
    if (!session.userId.empty()) return session.userId;
    if (!session.email.empty()) return session.email;
    if (!session.displayName.empty()) return session.displayName;
    return session.accessToken;
}

static NSString *OPNAuthSessionDisplayName(const OPN::AuthSession &session) {
    if (!session.displayName.empty()) return [NSString stringWithUTF8String:session.displayName.c_str()];
    if (!session.email.empty()) return [NSString stringWithUTF8String:session.email.c_str()];
    if (!session.userId.empty()) return [NSString stringWithUTF8String:session.userId.c_str()];
    return @"Account";
}

static NSString *OPNGravatarURLStringForEmail(const std::string &email) {
    if (email.empty()) return nil;
    NSString *rawEmail = [NSString stringWithUTF8String:email.c_str()];
    NSString *normalized = [[rawEmail stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] lowercaseString];
    if (normalized.length == 0) return nil;

    const char *utf8 = normalized.UTF8String;
    unsigned char digest[CC_MD5_DIGEST_LENGTH];
    CC_MD5(utf8, (CC_LONG)strlen(utf8), digest);
    NSMutableString *hash = [NSMutableString stringWithCapacity:CC_MD5_DIGEST_LENGTH * 2];
    for (NSUInteger i = 0; i < CC_MD5_DIGEST_LENGTH; i++) {
        [hash appendFormat:@"%02x", digest[i]];
    }
    return [NSString stringWithFormat:@"https://www.gravatar.com/avatar/%@?s=96&d=identicon", hash];
}

static bool OPNIsOwnedLibraryStatus(const std::string &status) {
    return status == "MANUAL" || status == "PLATFORM_SYNC" || status == "IN_LIBRARY";
}

static bool OPNChooseAccountLinked(const OPN::GameInfo &game, const OPN::GameVariant *selectedVariant) {
    if (game.playType == "INSTALL_TO_PLAY") return false;
    if (selectedVariant && OPNIsOwnedLibraryStatus(selectedVariant->serviceStatus)) return true;
    if (game.isInLibrary) return true;
    for (const auto &variant : game.variants) {
        if (OPNIsOwnedLibraryStatus(variant.serviceStatus)) return true;
    }
    return false;
}

static void OPNAppendFingerprintField(std::string &target, const std::string &value) {
    target += std::to_string(value.size());
    target += ':';
    target += value;
    target += '|';
}

static void OPNAppendFingerprintList(std::string &target, std::vector<std::string> values) {
    std::sort(values.begin(), values.end());
    target += '[';
    for (const std::string &value : values) {
        OPNAppendFingerprintField(target, value);
    }
    target += ']';
}

static std::string OPNGameLibraryFingerprint(const std::vector<OPN::GameInfo> &games) {
    std::vector<std::string> entries;
    entries.reserve(games.size());
    for (const OPN::GameInfo &game : games) {
        std::string entry;
        OPNAppendFingerprintField(entry, game.id);
        OPNAppendFingerprintField(entry, game.uuid);
        OPNAppendFingerprintField(entry, game.launchAppId);
        OPNAppendFingerprintField(entry, game.title);
        OPNAppendFingerprintField(entry, game.shortName);
        OPNAppendFingerprintField(entry, game.playabilityState);
        OPNAppendFingerprintField(entry, game.imageUrl);
        OPNAppendFingerprintList(entry, game.availableStores);
        OPNAppendFingerprintList(entry, game.genres);
        std::vector<std::string> variants;
        variants.reserve(game.variants.size());
        for (const OPN::GameVariant &variant : game.variants) {
            std::string variantEntry;
            OPNAppendFingerprintField(variantEntry, variant.id);
            OPNAppendFingerprintField(variantEntry, variant.appStore);
            OPNAppendFingerprintField(variantEntry, variant.serviceStatus);
            OPNAppendFingerprintField(variantEntry, variant.librarySelected ? "1" : "0");
            OPNAppendFingerprintField(variantEntry, variant.inLibrary ? "1" : "0");
            variants.push_back(variantEntry);
        }
        OPNAppendFingerprintList(entry, variants);
        entries.push_back(entry);
    }
    std::sort(entries.begin(), entries.end());
    std::string fingerprint;
    fingerprint.reserve(entries.size() * 128);
    for (const std::string &entry : entries) {
        OPNAppendFingerprintField(fingerprint, entry);
    }
    return fingerprint;
}

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    using namespace OPN;

    OPN::StartLogCapture();
    NSApp.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];

    NSRect frame = NSMakeRect(0, 0, kWindowWidth, kWindowHeight);
    self.window = [[NSWindow alloc] initWithContentRect:frame
                                              styleMask:NSWindowStyleMaskTitled |
                                                        NSWindowStyleMaskClosable |
                                                        NSWindowStyleMaskMiniaturizable |
                                                        NSWindowStyleMaskResizable
                                                backing:NSBackingStoreBuffered
                                                  defer:NO];
    self.window.title = @"OpenNOW";
    OPNConfigureLibraryWindow(self.window);
    if (![self.window setFrameUsingName:OPNMainWindowFrameAutosaveName]) {
        [self.window center];
    }
    self.window.frameAutosaveName = OPNMainWindowFrameAutosaveName;
    [self installLibraryRootIfNeeded];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(windowFullScreenStateChanged:)
                                                 name:NSWindowDidEnterFullScreenNotification
                                               object:self.window];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(windowFullScreenStateChanged:)
                                                 name:NSWindowDidExitFullScreenNotification
                                               object:self.window];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(interfacePreferencesChanged:)
                                                 name:OPNInterfacePreferencesDidChangeNotification
                                               object:nil];
    {
        OPN::AuthCredentials creds = self.pendingCredentials;
        creds.stayLoggedIn = AuthService::Shared().GetStayLoggedIn();
        self.pendingCredentials = creds;
    }

    AuthSession saved = AuthService::Shared().LoadSavedSession();
    BOOL shouldAutoSignIn = saved.isAuthenticated && AuthService::Shared().GetStayLoggedIn();
    if (shouldAutoSignIn && saved.IsAccessTokenValid()) {
        self.currentSession = saved;
        [self transitionToScreen:AuthScreen::Catalog];
    } else if (shouldAutoSignIn && !saved.refreshToken.empty()) {
        [self showAuthenticatingWithMessage:@"Refreshing session..."];
        __weak __typeof__(self) weakSelf = self;
        AuthService::Shared().RefreshSession(^(bool success, const AuthSession &fresh,
                                                const std::string &) {
            __typeof__(self) s = weakSelf;
            if (!s) return;
            if (success) {
                s.currentSession = fresh;
                AuthService::Shared().SaveSession(fresh);
                [s refreshAccountMenu];
                [s transitionToScreen:AuthScreen::Catalog];
            } else {
                AuthService::Shared().ClearSession();
                OPN::AuthSession fallback = AuthService::Shared().LoadSavedSession();
                if (fallback.isAuthenticated && fallback.IsAccessTokenValid()) {
                    s.currentSession = fallback;
                    [s transitionToScreen:AuthScreen::Catalog];
                } else {
                    [s transitionToScreen:AuthScreen::EmailEntry];
                }
            }
        });
    } else {
        [self transitionToScreen:AuthScreen::EmailEntry];
    }

    [self.window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
    [self restoreSavedWindowPresentation];
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    (void)notification;
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [self.window saveFrameUsingName:OPNMainWindowFrameAutosaveName];
    [self saveWindowPresentation];
    [self stopGameLibraryRefreshTimer];
    if (self.streamingController) {
        [self.streamingController shutdownForApplicationTermination];
        self.streamingController = nil;
    }
}

- (void)restoreSavedWindowPresentation {
    if (![NSUserDefaults.standardUserDefaults boolForKey:OPNMainWindowWasFullScreenKey]) return;
    __weak __typeof__(self) weakSelf = self;
    dispatch_async(dispatch_get_main_queue(), ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf || !strongSelf.window || OPNWindowIsFullScreen(strongSelf.window)) return;
        [strongSelf.window toggleFullScreen:nil];
    });
}

- (void)saveWindowPresentation {
    [NSUserDefaults.standardUserDefaults setBool:OPNWindowIsFullScreen(self.window)
                                          forKey:OPNMainWindowWasFullScreenKey];
    [NSUserDefaults.standardUserDefaults synchronize];
}

- (void)windowFullScreenStateChanged:(NSNotification *)notification {
    if (notification.object != self.window) return;
    [self saveWindowPresentation];
}

- (void)interfacePreferencesChanged:(NSNotification *)notification {
    (void)notification;
    if (!self.rootView || OpnDerivedAccentColorsEnabled()) return;
    self.rootView.controllerAccentRGB = OpnCurrentAccentRGB();
}

- (void)launchGame:(const OPN::GameInfo &)game variantIndex:(int)variantIndex returnScreen:(OPN::AuthScreen)returnScreen {
    using namespace OPN;

    if (self.streamingController) {
        NSLog(@"[AppDelegate] Ignoring game launch while stream is active: title=%s, id=%s", game.title.c_str(), game.id.c_str());
        return;
    }

    self.catalogView = nil;
    self.storeView = nil;
    self.settingsView = nil;

    NSLog(@"[AppDelegate] Game selected: title=%s, id=%s, uuid=%s, variantIndex=%d", game.title.c_str(), game.id.c_str(), game.uuid.c_str(), variantIndex);

    std::string apiToken = self.currentSession.idToken.empty()
        ? self.currentSession.accessToken : self.currentSession.idToken;

    std::string effectiveAppId;
    std::string selectedStore;
    bool accountLinked = OPNChooseAccountLinked(game, nullptr);
    if (variantIndex >= 0 && variantIndex < (int)game.variants.size()) {
        const GameVariant &variant = game.variants[(size_t)variantIndex];
        effectiveAppId = variant.id;
        selectedStore = variant.appStore;
        accountLinked = OPNChooseAccountLinked(game, &variant);
        NSLog(@"[AppDelegate] Variant: id=%s, store=%s, status=%s, accountLinked=%d",
              variant.id.c_str(), variant.appStore.c_str(), variant.serviceStatus.c_str(), accountLinked);
    }
    if (effectiveAppId.empty()) {
        effectiveAppId = game.launchAppId.empty() ? game.id : game.launchAppId;
    }
    NSLog(@"[AppDelegate] Using appId=%s, store=%s, accountLinked=%d",
          effectiveAppId.c_str(), selectedStore.c_str(), accountLinked);

    OPNStreamViewController *streamVC =
        [[OPNStreamViewController alloc] initWithGameTitle:game.title
                                                     appId:effectiveAppId
                                                  apiToken:apiToken
                                             accountLinked:accountLinked
                                                      selectedStore:selectedStore];
    self.currentStreamTitle = game.title.empty() ? @"Current Stream" : [NSString stringWithUTF8String:game.title.c_str()];
    self.activeStreamReturnScreen = returnScreen;
    self.streamLibraryOverlayActive = NO;

    __weak __typeof__(self) weakSelf = self;
    streamVC.onStreamEnd = ^(BOOL success, const std::string &error) {
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf) return;
        std::string errorCopy = error;
        dispatch_async(dispatch_get_main_queue(), ^{
            NSLog(@"[AppDelegate] Stream ended, restoring previous screen. Success=%d", success);
            strongSelf.streamLibraryOverlayActive = NO;
            [strongSelf transitionToScreen:returnScreen];
            strongSelf.streamingController = nil;
            strongSelf.currentStreamTitle = nil;
            if (!success && !errorCopy.empty()) {
                [strongSelf showError:errorCopy canRetry:YES];
            }
        });
    };
    streamVC.onControllerLibraryRequested = ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf) return;
        dispatch_async(dispatch_get_main_queue(), ^{
            [strongSelf showLibraryOverlayForActiveStream];
        });
    };

    NSRect preservedFrame = self.window.frame;
    BOOL preserveFrame = !OPNWindowIsFullScreen(self.window);
    [streamVC setInitialViewFrame:self.window.contentView.bounds];
    streamVC.view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    OPNConfigureStreamWindow(self.window);
    self.window.contentViewController = streamVC;
    OpnDisableFocusHighlights(streamVC.view);
    if (preserveFrame) {
        [self.window setFrame:preservedFrame display:YES animate:NO];
    }
    self.streamingController = streamVC;
    [self.window makeKeyAndOrderFront:nil];
    if (OpnAutoFullScreenEnabled() && !OPNWindowIsFullScreen(self.window)) {
        dispatch_async(dispatch_get_main_queue(), ^{
            if (!OPNWindowIsFullScreen(self.window)) {
                [self.window toggleFullScreen:nil];
            }
        });
    }
    NSLog(@"[AppDelegate] Window setup complete");
}

- (void)showLibraryOverlayForActiveStream {
    if (!self.streamingController || self.streamLibraryOverlayActive) return;
    self.streamLibraryOverlayActive = YES;
    [self.streamingController prepareForLibraryPictureInPicture];
    [self transitionToScreen:OPN::AuthScreen::Catalog];
}

- (void)returnToActiveStreamFromLibraryOverlay {
    if (!self.streamingController) return;
    NSRect preservedFrame = self.window.frame;
    BOOL preserveFrame = !OPNWindowIsFullScreen(self.window);
    NSView *streamView = [self.streamingController streamPictureInPictureView];
    [streamView removeFromSuperview];
    [self.streamingController setInitialViewFrame:self.window.contentView.bounds];
    self.streamingController.view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    OPNConfigureStreamWindow(self.window);
    self.window.contentViewController = self.streamingController;
    OpnDisableFocusHighlights(self.streamingController.view);
    if (preserveFrame) {
        [self.window setFrame:preservedFrame display:YES animate:NO];
    }
    self.streamLibraryOverlayActive = NO;
    [self.streamingController restoreFromLibraryPictureInPicture];
    [self.window makeKeyAndOrderFront:nil];
}

#pragma mark - Screen Transitions

- (void)installLibraryRootIfNeeded {
    using namespace OPN;

    BOOL needsRoot = !self.rootView || self.window.contentView != self.rootView;
    if (needsRoot) {
        self.window.contentViewController = nil;
        self.rootView = [[OPNBackdropView alloc] initWithFrame:self.window.contentView.bounds];
        self.rootView.wantsLayer = YES;
        self.rootView.layer.opaque = NO;
        self.rootView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        __weak __typeof__(self) weakSelf = self;
        self.rootView.onStoreSelected = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || strongSelf.currentScreen == OPN::AuthScreen::Store) return;
            [strongSelf transitionToScreen:OPN::AuthScreen::Store];
        };
        self.rootView.onLibrarySelected = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || strongSelf.currentScreen == OPN::AuthScreen::Catalog) return;
            [strongSelf transitionToScreen:OPN::AuthScreen::Catalog];
        };
        self.rootView.onSettingsSelected = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || strongSelf.currentScreen == OPN::AuthScreen::Settings) return;
            [strongSelf transitionToScreen:OPN::AuthScreen::Settings];
        };
        self.rootView.onAccountSelected = ^(NSString *accountIdentifier) {
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf switchToAccountIdentifier:accountIdentifier];
        };
        self.rootView.onAddAccountSelected = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf addAccount];
        };
        self.rootView.onSignOutSelected = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf performServerLogout];
        };
        self.rootView.onExitSelected = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            [NSApp terminate:strongSelf];
        };
        self.window.contentView = self.rootView;
        OpnDisableFocusHighlights(self.rootView);
    }

    if (!self.contentContainer || self.contentContainer.superview != self.rootView) {
        self.contentContainer = [[NSView alloc] initWithFrame:self.rootView.bounds];
        self.contentContainer.wantsLayer = YES;
        self.contentContainer.layer.opaque = NO;
        self.contentContainer.layer.backgroundColor = NSColor.clearColor.CGColor;
        self.contentContainer.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        [self.rootView addSubview:self.contentContainer];
    }
}

- (void)configureContentContainerForScreen:(OPN::AuthScreen)screen {
    if (self.rootView) {
        if (screen == OPN::AuthScreen::Store) {
            self.rootView.mode = OPNBackdropModeStore;
        } else if (screen == OPN::AuthScreen::Catalog) {
            self.rootView.mode = OPNBackdropModeLibrary;
        } else if (screen == OPN::AuthScreen::Settings) {
            self.rootView.mode = OPNBackdropModeSettings;
        } else {
            self.rootView.mode = OPNBackdropModeAuth;
        }
    }
    self.contentContainer.frame = self.rootView.bounds;
    self.contentContainer.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
}

- (void)transitionToScreen:(OPN::AuthScreen)screen {
    using namespace OPN;

    [self installLibraryRootIfNeeded];
    [self configureContentContainerForScreen:screen];

    for (NSView *subview in [self.contentContainer.subviews copy]) {
        [subview removeFromSuperview];
    }

    self.currentScreen = screen;
    NSRect bounds = self.contentContainer.bounds;

    switch (screen) {
        case AuthScreen::EmailEntry: {
            OPNEmailEntryView *view = [[OPNEmailEntryView alloc] initWithFrame:bounds];
            view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
            __weak __typeof__(self) weakSelf = self;

            view.onSignInWithBrowser = ^{
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf) return;
                OPN::AuthCredentials creds = strongSelf.pendingCredentials;
                creds.stayLoggedIn = OPN::AuthService::Shared().GetStayLoggedIn();
                strongSelf.pendingCredentials = creds;
                [strongSelf transitionToScreen:OPN::AuthScreen::OAuthBrowser];
            };

            [self.contentContainer addSubview:view];
            OpnDisableFocusHighlights(view);
            self.window.title = @"OpenNOW";
            break;
        }

        case AuthScreen::OAuthBrowser: {
            __weak __typeof__(self) weakSelf = self;
            [self showAuthenticatingWithMessage:@"Opening browser for sign in..."];
            OPN::AuthService::Shared().StartOAuthLogin(
                ^(bool success, const OPN::AuthSession &session, const std::string &error) {
                    __typeof__(self) strongSelf = weakSelf;
                    if (!strongSelf) return;
                    if (success) {
                        strongSelf.currentSession = session;
                        if (strongSelf.pendingCredentials.stayLoggedIn)
                            OPN::AuthService::Shared().SaveSession(session);
                        [strongSelf refreshAccountMenu];
                        [strongSelf transitionToScreen:AuthScreen::Catalog];
                    } else {
                        [strongSelf showError:error canRetry:YES];
                    }
                });
            break;
        }

        case AuthScreen::Store: {
            OPNConfigureLibraryWindow(self.window);
            self.catalogView = nil;
            self.settingsView = nil;

            NSString *displayName = [NSString stringWithUTF8String:self.currentSession.displayName.c_str()];
            self.rootView.accountName = displayName.length > 0 ? displayName : @"User";
            self.rootView.accountStatus = OPNDisplayTier(self.currentSession.membershipTier);
            self.rootView.remainingPlayTime = @"--";
            self.rootView.gameCountText = @"";
            [self refreshAccountAvatar];
            [self refreshAccountMenu];
            [self refreshAccountSummary];
            [self refreshStreamRegions];

            OPNStoreView *store = [[OPNStoreView alloc] initWithFrame:bounds];
            store.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
            self.storeView = store;

            std::string storeAccountIdentifier = OPNAuthSessionIdentifier(self.currentSession);
            if (self.hasCachedGameLibrary && self.cachedGameLibraryAccountIdentifier == storeAccountIdentifier) {
                [store setLibraryGames:self.cachedGameLibrary];
            } else {
                __weak __typeof__(self) weakSelfForLibrary = self;
                [self fetchGameLibraryWithRetry:YES completion:^(BOOL success, const std::vector<GameInfo> &games) {
                    __typeof__(self) strongSelf = weakSelfForLibrary;
                    if (!strongSelf || !success || storeAccountIdentifier != OPNAuthSessionIdentifier(strongSelf.currentSession)) return;
                    strongSelf.cachedGameLibrary = games;
                    strongSelf.cachedGameLibraryFingerprint = OPNGameLibraryFingerprint(games);
                    strongSelf.cachedGameLibraryAccountIdentifier = storeAccountIdentifier;
                    strongSelf.hasCachedGameLibrary = YES;
                    if (strongSelf.currentScreen == AuthScreen::Store && strongSelf.storeView) {
                        [strongSelf.storeView setLibraryGames:games];
                    }
                }];
            }

            __weak __typeof__(self) weakSelf = self;
            store.onSelectGame = ^(const GameInfo &game, int variantIndex) {
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf) return;
                [strongSelf launchGame:game variantIndex:variantIndex returnScreen:AuthScreen::Store];
            };

            [self.contentContainer addSubview:store];
            OpnDisableFocusHighlights(store);
            self.window.title = @"OpenNOW - Store";
            [self loadStorePanelsWithRetry:YES];
            break;
        }

        case AuthScreen::Catalog: {
            OPNConfigureLibraryWindow(self.window);
            self.storeView = nil;
            self.settingsView = nil;

            OPNGameCatalogView *catalog = [[OPNGameCatalogView alloc] initWithFrame:bounds];
            catalog.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
            self.catalogView = catalog;

            // Set display name (may be empty for OAuth — will be filled below)
            NSString *displayName = [NSString stringWithUTF8String:
                self.currentSession.displayName.c_str()];
            if (displayName.length > 0) {
                [catalog setUserName:displayName];
                self.rootView.accountName = displayName;
            } else {
                [catalog setUserName:@"User"];
                self.rootView.accountName = @"User";
            }
            self.rootView.accountStatus = OPNDisplayTier(self.currentSession.membershipTier);
            self.rootView.remainingPlayTime = @"--";
            self.rootView.gameCountText = @"";
            [self refreshAccountAvatar];
            [self refreshAccountMenu];
            [self refreshAccountSummary];
            [self refreshStreamRegions];

            __weak __typeof__(self) weakSelf = self;
            catalog.onSignOut = ^{
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf) return;
                [strongSelf performServerLogout];
            };

            catalog.onGameCountChanged = ^(NSInteger count) {
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf || !strongSelf.rootView) return;
                strongSelf.rootView.gameCountText = [NSString stringWithFormat:@"%ld %@", (long)count, count == 1 ? @"game" : @"games"];
            };

            catalog.onFocusedArtworkAccentChanged = ^(unsigned accentRGB) {
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf || !strongSelf.rootView) return;
                strongSelf.rootView.controllerAccentRGB = OpnDerivedAccentColorsEnabled() ? accentRGB : OpnCurrentAccentRGB();
            };

            catalog.onSelectGame = ^(const GameInfo &game, int variantIndex) {
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf) return;
                [strongSelf launchGame:game variantIndex:variantIndex returnScreen:AuthScreen::Catalog];
            };

            catalog.onCatalogBrowseRequested = ^(NSString *searchQuery, NSString *sortId, const std::vector<std::string> &filterIds) {
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf) return;
                [strongSelf browseCatalogWithSearch:searchQuery sortId:sortId filterIds:filterIds canRetry:YES];
            };

            if (self.streamingController && self.streamLibraryOverlayActive) {
                [catalog setStreamPictureInPictureView:[self.streamingController streamPictureInPictureView]
                                                title:self.currentStreamTitle ?: @"Current Stream"];
                catalog.onStreamPictureInPictureSelected = ^{
                    __typeof__(self) strongSelf = weakSelf;
                    if (!strongSelf) return;
                    [strongSelf returnToActiveStreamFromLibraryOverlay];
                };
            }

            [self.contentContainer addSubview:catalog];
            OpnDisableFocusHighlights(catalog);
            self.window.title = @"OpenNOW";

            // Fetch user info if displayName not already set (OAuth flow)
            if (displayName.length == 0 && !self.currentSession.accessToken.empty()) {
                [catalog setLoading:YES];
                AuthService::Shared().FetchStarFleetUserInfo(
                    self.currentSession.accessToken,
                    ^(bool uiSuccess, NSDictionary *info, const std::string &) {
                        __typeof__(self) s = weakSelf;
                        if (!s) return;
                        dispatch_async(dispatch_get_main_queue(), ^{
                            if (uiSuccess && info) {
                                NSString *pref = info[@"preferred_username"];
                                NSString *email = info[@"email"];
                                NSString *name = pref ? pref : (email ? [email componentsSeparatedByString:@"@"][0] : nil);
                                if (name) {
                                    s.currentSession.displayName = [name UTF8String];
                                    if (email.length > 0) s.currentSession.email = [email UTF8String];
                                    if (s.pendingCredentials.stayLoggedIn)
                                        AuthService::Shared().SaveSession(s.currentSession);
                                    [s.catalogView setUserName:name];
                                    s.rootView.accountName = name;
                                    [s refreshAccountAvatar];
                                    [s refreshAccountMenu];
                                    [s refreshAccountSummary];
                                }
                            }
                            [s loadGamesIntoCatalog];
                        });
                    });
            } else {
                [self loadGamesIntoCatalog];
            }
            break;
        }

        case AuthScreen::Settings: {
            OPNConfigureLibraryWindow(self.window);
            self.storeView = nil;
            self.catalogView = nil;
            NSString *displayName = [NSString stringWithUTF8String:self.currentSession.displayName.c_str()];
            self.rootView.accountName = displayName.length > 0 ? displayName : @"User";
            self.rootView.accountStatus = OPNDisplayTier(self.currentSession.membershipTier);
            self.rootView.remainingPlayTime = @"--";
            self.rootView.gameCountText = @"";
            [self refreshAccountAvatar];
            [self refreshAccountMenu];
            [self refreshAccountSummary];
            [self refreshStreamRegions];
            OPNSettingsView *settings = [[OPNSettingsView alloc] initWithFrame:bounds];
            settings.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
            self.settingsView = settings;
            [self.contentContainer addSubview:settings];
            OpnDisableFocusHighlights(settings);
            self.window.title = @"OpenNOW - Settings";
            break;
        }

        case AuthScreen::Error: {
            break;
        }

        default:
            break;
    }
}

- (void)refreshAccountSummary {
    using namespace OPN;
    if (!self.rootView || self.currentSession.accessToken.empty()) {
        return;
    }
    self.rootView.accountStatus = OPNDisplayTier(self.currentSession.membershipTier);
    GameService::Shared().SetAccessToken(self.currentSession.idToken.empty()
        ? self.currentSession.accessToken
        : self.currentSession.idToken);
    std::string userId = self.currentSession.userId;
    __weak __typeof__(self) weakSelf = self;
    GameService::Shared().FetchSubscriptionInfo(userId, [weakSelf](bool success, const SubscriptionInfo &subscription, const std::string &error) {
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf || !strongSelf.rootView) return;
        if (!success) {
            NSLog(@"[AppDelegate] Subscription fetch failed: %s", error.c_str());
            return;
        }
        strongSelf.rootView.accountStatus = OPNDisplayTier(subscription.membershipTier);
        strongSelf.rootView.remainingPlayTime = OPNFormatRemainingPlayTime(subscription);
        strongSelf.currentSession.membershipTier = subscription.membershipTier;
        if (AuthService::Shared().GetStayLoggedIn()) {
            AuthService::Shared().SaveSession(strongSelf.currentSession);
        }
    });
}

- (void)refreshAccountAvatar {
    if (!self.rootView) return;
    NSString *email = self.currentSession.email.empty() ? @"" : [NSString stringWithUTF8String:self.currentSession.email.c_str()];
    NSString *avatarURLString = OPNGravatarURLStringForEmail(self.currentSession.email);
    self.rootView.accountAvatarImage = nil;
    if (avatarURLString.length == 0) return;

    NSURL *url = [NSURL URLWithString:avatarURLString];
    if (!url) return;
    __weak __typeof__(self) weakSelf = self;
    [[[NSURLSession sharedSession] dataTaskWithURL:url completionHandler:^(NSData *data, NSURLResponse *, NSError *error) {
        if (error || !data) return;
        NSImage *image = [[NSImage alloc] initWithData:data];
        if (!image) return;
        dispatch_async(dispatch_get_main_queue(), ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || !strongSelf.rootView) return;
            NSString *currentEmail = strongSelf.currentSession.email.empty() ? @"" : [NSString stringWithUTF8String:strongSelf.currentSession.email.c_str()];
            if (![currentEmail isEqualToString:email]) return;
            strongSelf.rootView.accountAvatarImage = image;
        });
    }] resume];
}

- (void)refreshStreamRegions {
    using namespace OPN;
    if (self.currentSession.accessToken.empty()) {
        return;
    }
    std::string token = self.currentSession.idToken.empty()
        ? self.currentSession.accessToken
        : self.currentSession.idToken;
    GameService::Shared().SetStreamingBaseUrl(LoadSelectedStreamingBaseUrl());
    FetchStreamRegions(token, DefaultStreamingBaseUrl(), [](const std::vector<StreamRegionOption> &) {
        GameService::Shared().SetStreamingBaseUrl(LoadSelectedStreamingBaseUrl());
        [[NSNotificationCenter defaultCenter] postNotificationName:@"OpenNOW.StreamRegionsUpdated" object:nil];
    });
}

- (void)refreshAccountMenu {
    using namespace OPN;
    if (!self.rootView) return;
    NSMutableArray<NSDictionary<NSString *, NSString *> *> *items = [NSMutableArray array];
    for (const AuthSession &session : AuthService::Shared().LoadSavedSessions()) {
        std::string identifier = OPNAuthSessionIdentifier(session);
        if (identifier.empty()) continue;
        NSString *identifierString = [NSString stringWithUTF8String:identifier.c_str()];
        NSString *label = OPNAuthSessionDisplayName(session);
        [items addObject:@{@"identifier": identifierString, @"label": label}];
    }
    self.rootView.accountMenuItems = items;
    std::string currentIdentifier = OPNAuthSessionIdentifier(self.currentSession);
    self.rootView.currentAccountIdentifier = currentIdentifier.empty()
        ? @""
        : [NSString stringWithUTF8String:currentIdentifier.c_str()];
}

- (void)addAccount {
    OPN::AuthCredentials creds = self.pendingCredentials;
    creds.stayLoggedIn = true;
    self.pendingCredentials = creds;
    [self transitionToScreen:OPN::AuthScreen::OAuthBrowser];
}

- (void)switchToAccountIdentifier:(NSString *)identifier {
    using namespace OPN;
    if (identifier.length == 0) return;
    std::string accountId = [identifier UTF8String];
    if (accountId == OPNAuthSessionIdentifier(self.currentSession)) return;

    AuthService::Shared().SetActiveSessionUserId(accountId);
    AuthSession selected = AuthService::Shared().LoadSavedSessionForUserId(accountId);
    if (!selected.isAuthenticated) return;
    self.currentSession = selected;
    if (selected.IsAccessTokenValid()) {
        [self transitionToScreen:AuthScreen::Catalog];
        return;
    }

    [self showAuthenticatingWithMessage:@"Refreshing session..."];
    __weak __typeof__(self) weakSelf = self;
    AuthService::Shared().RefreshSession(^(bool success, const AuthSession &fresh, const std::string &) {
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf) return;
        if (success) {
            strongSelf.currentSession = fresh;
            AuthService::Shared().SaveSession(fresh);
            [strongSelf refreshAccountMenu];
            [strongSelf transitionToScreen:AuthScreen::Catalog];
            return;
        }

        AuthService::Shared().RemoveSavedSession(accountId);
        AuthSession fallback = AuthService::Shared().LoadSavedSession();
        if (fallback.isAuthenticated && fallback.IsAccessTokenValid()) {
            strongSelf.currentSession = fallback;
            [strongSelf transitionToScreen:AuthScreen::Catalog];
        } else {
            strongSelf.currentSession.Clear();
            [strongSelf transitionToScreen:AuthScreen::EmailEntry];
        }
    });
}

#pragma mark - Store Loading

- (void)loadStorePanelsWithRetry:(BOOL)canRetry {
    using namespace OPN;

    if (!self.storeView) return;
    [self.storeView setLoading:YES];

    std::string accountIdentifier = OPNAuthSessionIdentifier(self.currentSession);
    std::string apiToken = self.currentSession.idToken.empty()
        ? self.currentSession.accessToken : self.currentSession.idToken;
    GameService::Shared().SetAccessToken(apiToken);
    GameService::Shared().SetVpcId("GFN-PC");

    __weak __typeof__(self) weakSelf = self;
    GameService::Shared().FetchMainPanels(
        [weakSelf, accountIdentifier, canRetry](bool success, const std::vector<PanelResult> &panels, const std::string &error) {
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            if (accountIdentifier != OPNAuthSessionIdentifier(strongSelf.currentSession)) return;

            if (!success && canRetry && error.find("401") != std::string::npos) {
                AuthService::Shared().RefreshSession(^(bool refreshSuccess, const AuthSession &fresh, const std::string &) {
                    __typeof__(self) retrySelf = weakSelf;
                    if (!retrySelf) return;
                    if (refreshSuccess) {
                        retrySelf.currentSession = fresh;
                        if (retrySelf.pendingCredentials.stayLoggedIn) {
                            AuthService::Shared().SaveSession(fresh);
                        }
                        [retrySelf refreshAccountMenu];
                        [retrySelf loadStorePanelsWithRetry:NO];
                        return;
                    }

                    AuthService::Shared().ClearSession();
                    AuthSession fallback = AuthService::Shared().LoadSavedSession();
                    if (fallback.isAuthenticated && fallback.IsAccessTokenValid()) {
                        retrySelf.currentSession = fallback;
                        [retrySelf transitionToScreen:AuthScreen::Store];
                    } else {
                        [retrySelf transitionToScreen:AuthScreen::EmailEntry];
                    }
                });
                return;
            }

            if (!strongSelf.storeView || strongSelf.currentScreen != AuthScreen::Store) return;
            if (!success) {
                NSString *message = error.empty()
                    ? @"Unable to load Store collections."
                    : [NSString stringWithUTF8String:error.c_str()];
                [strongSelf.storeView setError:message];
                return;
            }

            [strongSelf.storeView setPanels:panels];
            [strongSelf.storeView setLoading:NO];
        });
}

#pragma mark - Catalog Loading

- (void)startGameLibraryRefreshTimer {
    if (self.gameLibraryRefreshTimer) return;
    self.gameLibraryRefreshTimer = [NSTimer scheduledTimerWithTimeInterval:30.0 * 60.0
                                                                    target:self
                                                                  selector:@selector(gameLibraryRefreshTimerFired:)
                                                                  userInfo:nil
                                                                   repeats:YES];
}

- (void)stopGameLibraryRefreshTimer {
    [self.gameLibraryRefreshTimer invalidate];
    self.gameLibraryRefreshTimer = nil;
}

- (void)gameLibraryRefreshTimerFired:(NSTimer *)timer {
    (void)timer;
    [self refreshGameLibraryInBackground];
}

- (void)refreshGameLibraryInBackground {
    using namespace OPN;
    if (!self.currentSession.isAuthenticated || self.currentSession.accessToken.empty() || self.gameLibraryRefreshInFlight) {
        return;
    }
    self.gameLibraryRefreshInFlight = YES;
    std::string accountIdentifier = OPNAuthSessionIdentifier(self.currentSession);
    __weak __typeof__(self) weakSelf = self;
    [self fetchGameLibraryWithRetry:YES completion:^(BOOL success, const std::vector<GameInfo> &games) {
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf) return;
        strongSelf.gameLibraryRefreshInFlight = NO;
        if (!success || accountIdentifier != OPNAuthSessionIdentifier(strongSelf.currentSession)) return;

        std::string fingerprint = OPNGameLibraryFingerprint(games);
        BOOL changed = !strongSelf.hasCachedGameLibrary
            || strongSelf.cachedGameLibraryAccountIdentifier != accountIdentifier
            || strongSelf.cachedGameLibraryFingerprint != fingerprint;
        if (!changed) return;

        strongSelf.cachedGameLibrary = games;
        strongSelf.cachedGameLibraryFingerprint = fingerprint;
        strongSelf.cachedGameLibraryAccountIdentifier = accountIdentifier;
        strongSelf.hasCachedGameLibrary = YES;
        if (strongSelf.currentScreen == AuthScreen::Catalog && strongSelf.catalogView) {
            [strongSelf.catalogView setGames:games];
        } else if (strongSelf.currentScreen == AuthScreen::Store && strongSelf.storeView) {
            [strongSelf.storeView setLibraryGames:games];
        }
    }];
}

- (void)loadGamesIntoCatalog {
    [self loadGamesIntoCatalogWithRetry:YES];
}

- (void)loadGamesIntoCatalogWithRetry:(BOOL)canRetry {
    using namespace OPN;
    if (!self.catalogView) {
        return;
    }
    [self browseCatalogWithSearch:@"" sortId:@"last_played" filterIds:std::vector<std::string>() canRetry:canRetry];
}

- (void)browseCatalogWithSearch:(NSString *)searchQuery
                          sortId:(NSString *)sortId
                       filterIds:(const std::vector<std::string> &)filterIds
                        canRetry:(BOOL)canRetry {
    using namespace OPN;
    if (!self.catalogView) return;

    NSInteger requestGeneration = ++self.catalogBrowseGeneration;
    std::string accountIdentifier = OPNAuthSessionIdentifier(self.currentSession);
    std::string apiToken = self.currentSession.idToken.empty()
        ? self.currentSession.accessToken : self.currentSession.idToken;
    GameService::Shared().SetAccessToken(apiToken);
    GameService::Shared().SetStreamingBaseUrl(LoadSelectedStreamingBaseUrl());
    GameService::Shared().SetVpcId("GFN-PC");
    [self.catalogView setLoading:YES];

    std::string search = searchQuery.length > 0 ? [searchQuery UTF8String] : "";
    std::string selectedSort = sortId.length > 0 ? [sortId UTF8String] : "last_played";
    __weak __typeof__(self) weakSelf = self;
    GameService::Shared().BrowseCatalogGames(search, selectedSort, filterIds, 96,
        [weakSelf, accountIdentifier, canRetry, requestGeneration, searchQuery, sortId, filterIds]
        (bool success, const CatalogBrowseResult &result, const std::string &error) {
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || requestGeneration != strongSelf.catalogBrowseGeneration) return;
            if (accountIdentifier != OPNAuthSessionIdentifier(strongSelf.currentSession)) return;

            if (!success && canRetry && error.find("401") != std::string::npos) {
                AuthService::Shared().RefreshSession(^(bool refreshSuccess, const AuthSession &fresh, const std::string &) {
                    __typeof__(self) retrySelf = weakSelf;
                    if (!retrySelf) return;
                    if (refreshSuccess) {
                        retrySelf.currentSession = fresh;
                        if (retrySelf.pendingCredentials.stayLoggedIn) AuthService::Shared().SaveSession(fresh);
                        [retrySelf refreshAccountMenu];
                        [retrySelf browseCatalogWithSearch:searchQuery sortId:sortId filterIds:filterIds canRetry:NO];
                        return;
                    }
                    [retrySelf.catalogView setLoading:NO];
                    AuthService::Shared().ClearSession();
                    [retrySelf transitionToScreen:AuthScreen::EmailEntry];
                });
                return;
            }

            [strongSelf.catalogView setLoading:NO];
            if (!success) {
                NSString *message = error.empty() ? @"Unable to browse catalog." : [NSString stringWithUTF8String:error.c_str()];
                [strongSelf.catalogView setError:message];
                return;
            }
            strongSelf.cachedGameLibrary = result.games;
            strongSelf.cachedGameLibraryFingerprint = OPNGameLibraryFingerprint(result.games);
            strongSelf.cachedGameLibraryAccountIdentifier = accountIdentifier;
            strongSelf.hasCachedGameLibrary = YES;
            [strongSelf.catalogView setCatalogBrowseResult:result];
            [strongSelf startGameLibraryRefreshTimer];
        });
}

- (void)fetchGameLibraryWithRetry:(BOOL)canRetry
                       completion:(void (^)(BOOL success, const std::vector<OPN::GameInfo> &games))completion {
    using namespace OPN;

    std::string apiToken = self.currentSession.idToken.empty()
        ? self.currentSession.accessToken : self.currentSession.idToken;
    GameService::Shared().SetAccessToken(apiToken);
    GameService::Shared().SetStreamingBaseUrl(LoadSelectedStreamingBaseUrl());
    GameService::Shared().SetVpcId("GFN-PC");

    __block int outstanding = 2;
    __block std::vector<GameInfo> allGames;
    __block BOOL retryStarted = NO;
    __block BOOL completed = NO;
    __weak __typeof__(self) weakSelf = self;

    auto finalize = ^(std::vector<GameInfo> toAdd) {
        __typeof__(self) s = weakSelf;
        if (!s || retryStarted || completed) return;
        for (auto &g : toAdd) allGames.push_back(g);
        outstanding--;
        if (outstanding == 0) {
            if (allGames.empty()) {
                GameService::Shared().FetchPublicGames(
                    ^(bool, const std::vector<GameInfo> &publicGames, const std::string &) {
                        if (completed) return;
                        completed = YES;
                        completion(true, publicGames);
                    });
            } else {
                completed = YES;
                completion(true, allGames);
            }
        }
    };

    // Load library games
    GameService::Shared().FetchLibraryGames(
        ^(bool ok, const std::vector<OPN::GameInfo> &games, const std::string &err) {
            if (!ok && canRetry && err.find("401") != std::string::npos) {
                retryStarted = YES;
                AuthService::Shared().RefreshSession(^(bool ok2, const AuthSession &fresh, const std::string &) {
                    __typeof__(self) s2 = weakSelf;
                    if (!s2 || completed) return;
                    if (ok2) {
                        s2.currentSession = fresh;
                        if (s2.pendingCredentials.stayLoggedIn)
                            AuthService::Shared().SaveSession(fresh);
                        [s2 refreshAccountMenu];
                        [s2 fetchGameLibraryWithRetry:NO completion:completion];
                    } else {
                        completed = YES;
                        AuthService::Shared().ClearSession();
                        AuthSession fallback = AuthService::Shared().LoadSavedSession();
                        if (fallback.isAuthenticated && fallback.IsAccessTokenValid()) {
                            s2.currentSession = fallback;
                            [s2 transitionToScreen:AuthScreen::Catalog];
                        } else {
                            [s2 transitionToScreen:AuthScreen::EmailEntry];
                        }
                        completion(false, std::vector<GameInfo>());
                    }
                });
                return;
            }
            finalize(games);
        });

    // Load catalog games
    GameService::Shared().FetchCatalogGames(
        ^(bool ok, const std::vector<GameInfo> &games, const std::string &) {
            finalize(ok ? games : std::vector<GameInfo>());
        });
}

#pragma mark - Server Logout

- (void)performServerLogout {
    using namespace OPN;
    __weak __typeof__(self) weakSelf = self;
    std::string idToken = self.currentSession.idToken;

    [self showAuthenticatingWithMessage:@"Signing out..."];

    AuthService::Shared().ServerLogout(idToken, "en_US",
        ^(bool, const std::string &) {
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;

            OPNConfigureLibraryWindow(strongSelf.window);
            AuthSession next = AuthService::Shared().LoadSavedSession();
            if (next.isAuthenticated && next.IsAccessTokenValid()) {
                strongSelf.currentSession = next;
                [strongSelf transitionToScreen:AuthScreen::Catalog];
                return;
            }

            strongSelf.currentSession.Clear();
            strongSelf.cachedGameLibrary.clear();
            strongSelf.cachedGameLibraryFingerprint.clear();
            strongSelf.cachedGameLibraryAccountIdentifier.clear();
            strongSelf.hasCachedGameLibrary = NO;
            strongSelf.pendingCredentials = AuthCredentials{};
            AuthCredentials creds = strongSelf.pendingCredentials;
            creds.stayLoggedIn = true;
            strongSelf.pendingCredentials = creds;
            [strongSelf refreshAccountMenu];
            [strongSelf transitionToScreen:AuthScreen::EmailEntry];
        });
}

#pragma mark - Overlay Screens

- (void)showAuthenticatingWithMessage:(NSString *)message {
    self.rootView.mode = OPNBackdropModeAuth;
    for (NSView *subview in [self.contentContainer.subviews copy]) {
        [subview removeFromSuperview];
    }
    OPNAuthenticatingView *overlay = [[OPNAuthenticatingView alloc]
        initWithFrame:self.contentContainer.bounds message:message];
    [self.contentContainer addSubview:overlay];
    self.currentScreen = OPN::AuthScreen::Authenticating;
}

- (void)showError:(const std::string &)errorMessage canRetry:(BOOL)canRetry {
    OPN::AuthScreen retryScreen = (self.currentScreen == OPN::AuthScreen::Store ||
                                   self.currentScreen == OPN::AuthScreen::Catalog ||
                                   self.currentScreen == OPN::AuthScreen::Settings)
        ? self.currentScreen
        : OPN::AuthScreen::EmailEntry;
    self.rootView.mode = OPNBackdropModeAuth;
    for (NSView *subview in [self.contentContainer.subviews copy]) {
        [subview removeFromSuperview];
    }
    NSString *msg = [NSString stringWithUTF8String:errorMessage.c_str()];
    if (!msg || msg.length == 0) {
        msg = @"An unknown error occurred.";
    }
    OPN::AppendLogEvent([NSString stringWithFormat:@"[AppDelegate] Presenting error: %@", msg]);
    OPN::CopyCapturedLogToClipboard(msg);
    msg = [msg stringByAppendingString:@"\n\nFull log copied to clipboard."];
    OPNErrorView *view = [[OPNErrorView alloc] initWithFrame:self.contentContainer.bounds
                                                      message:msg
                                                    canRetry:canRetry];
    __weak __typeof__(self) weakSelf = self;

    view.onRetry = ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf) return;
        [strongSelf transitionToScreen:retryScreen];
    };

    view.onBackToEmail = ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf) return;
        {
            OPN::AuthCredentials _c = OPN::AuthCredentials{};
            _c.stayLoggedIn = OPN::AuthService::Shared().GetStayLoggedIn();
            strongSelf.pendingCredentials = _c;
        }
        [strongSelf transitionToScreen:OPN::AuthScreen::EmailEntry];
    };

    [self.contentContainer addSubview:view];
    self.currentScreen = OPN::AuthScreen::Error;
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    (void)sender;
    return YES;
}

- (NSApplicationTerminateReply)applicationShouldTerminate:(NSApplication *)sender {
    (void)sender;
    if (self.streamingController) {
        [self.streamingController shutdownForApplicationTermination];
        self.streamingController = nil;
    }
    return NSTerminateNow;
}

@end
