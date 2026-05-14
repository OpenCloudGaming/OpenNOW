#import "OPNStreamViewController.h"
#import "OPNStreamView.h"
#import "../views/OPNLoadingView.h"
#include "OPNSignalingClient.h"
#include "OPNStreamSession.h"
#include "OPNStreamBackend.h"
#include "OPNStreamPreferences.h"
#include "../common/OPNLogCapture.h"
#include "OPNSessionManager.h"
#include "../games/OPNGameService.h"
#import <QuartzCore/QuartzCore.h>
#import <CoreGraphics/CoreGraphics.h>
#import <AVFoundation/AVFoundation.h>
#import <os/signpost.h>
#include <algorithm>
#include <cmath>
#include <limits>
#include <sstream>
#include <vector>
#include <cctype>
#include <cstring>
#include <cstdlib>

static constexpr NSInteger OPNMaxAutomaticRecoveryAttempts = 2;
static constexpr NSTimeInterval OPNStableRecoveryResetInterval = 15.0;
static constexpr NSTimeInterval OPNSignalingRemoteIceGraceInterval = 5.0;

@interface OPNQuitGameOverlayView : NSView
@property (nonatomic, copy) void (^onCancel)(void);
@property (nonatomic, copy) void (^onQuit)(void);
@end

@interface OPNShortcutLegendView : NSView
@end

@interface OPNStatsOverlayView : NSView
- (void)updateLatencyMs:(NSInteger)latencyMs
               jitterMs:(NSInteger)jitterMs
            bitrateMbps:(double)bitrateMbps
            packetsLost:(int64_t)packetsLost
       packetLossPercent:(double)packetLossPercent
                     gpu:(NSString *)gpu
              resolution:(NSString *)resolution
                     fps:(NSInteger)fps
               renderFps:(double)renderFps
                   codec:(NSString *)codec
                    zone:(NSString *)zone
             decoder:(NSString *)decoder
        decodeTimeMs:(double)decodeTimeMs
                 sink:(NSString *)sink
        pipelineMode:(NSString *)pipelineMode
        webrtcBackend:(NSString *)webrtcBackend
       framesReceived:(uint64_t)framesReceived
        framesDecoded:(uint64_t)framesDecoded
        framesDropped:(uint64_t)framesDropped;
@end

@interface OPNStreamViewController ()
@property (nonatomic, strong) OPNStreamView *streamView;
@property (nonatomic, strong) NSTextField *statusLabel;
@property (nonatomic, strong) OPNLoadingView *loadingView;
@property (nonatomic, strong) OPNQuitGameOverlayView *quitOverlay;
@property (nonatomic, strong) OPNStatsOverlayView *statsOverlay;
@property (nonatomic, strong) OPNShortcutLegendView *shortcutLegendOverlay;
@property (nonatomic, strong) NSTimer *statsRefreshTimer;
@property (nonatomic, assign) std::string gameTitle;
@property (nonatomic, assign) std::string appId;
@property (nonatomic, assign) std::string apiToken;
@property (nonatomic, assign) std::string selectedStore;
@property (nonatomic, assign) std::string resumeSessionId;
@property (nonatomic, assign) std::string resumeServer;
@property (nonatomic, assign) BOOL accountLinked;
@property (nonatomic, assign) BOOL resumeExistingSession;
@property (nonatomic, assign) BOOL streamStarted;
@property (nonatomic, assign) BOOL launchSignpostActive;
@property (nonatomic, assign) CFTimeInterval launchStartTime;
@property (nonatomic, assign) os_signpost_id_t launchSignpostId;
- (void)finishLaunchMeasurementWithSuccess:(BOOL)success reason:(NSString *)reason;
- (void)connectWithSessionInfo:(const OPN::SessionInfo &)sessionInfo
                       settings:(const OPN::StreamSettings &)settings
               launchGeneration:(NSUInteger)launchGeneration;
@end

static os_log_t OPNStreamPerformanceLog() {
    static os_log_t log = os_log_create("com.opennow.stream", "performance");
    return log;
}

static bool IsDottedIp(const std::string &value) {
    int dots = 0;
    int digits = 0;
    if (value.empty()) return false;
    for (char c : value) {
        if (c == '.') {
            if (digits == 0) return false;
            dots++;
            digits = 0;
        } else if (std::isdigit((unsigned char)c)) {
            digits++;
            if (digits > 3) return false;
        } else {
            return false;
        }
    }
    return dots == 3 && digits > 0;
}

static std::string ExtractPublicIp(const std::string &hostOrIp) {
    if (hostOrIp.empty()) return "";
    if (IsDottedIp(hostOrIp)) return hostOrIp;

    std::string firstLabel = hostOrIp.substr(0, hostOrIp.find('.'));
    std::vector<std::string> parts;
    std::stringstream ss(firstLabel);
    std::string part;
    while (std::getline(ss, part, '-')) {
        if (part.empty()) return "";
        for (char c : part) {
            if (!std::isdigit((unsigned char)c)) return "";
        }
        parts.push_back(part);
    }

    if (parts.size() != 4) return "";
    return parts[0] + "." + parts[1] + "." + parts[2] + "." + parts[3];
}

struct OPNDisplayStreamProfile {
    int displayWidth = 1920;
    int displayHeight = 1080;
    int streamWidth = 1920;
    int streamHeight = 1080;
    std::string resolution = "1920x1080";
};

static NSSize CurrentDisplayPixelSize(NSWindow *window) {
    NSScreen *screen = window.screen ?: NSScreen.mainScreen;
    if (!screen) {
        return NSMakeSize(1920.0, 1080.0);
    }

    NSNumber *screenNumber = screen.deviceDescription[@"NSScreenNumber"];
    if ([screenNumber isKindOfClass:[NSNumber class]]) {
        CGDirectDisplayID displayId = (CGDirectDisplayID)screenNumber.unsignedIntValue;
        size_t pixelWidth = CGDisplayPixelsWide(displayId);
        size_t pixelHeight = CGDisplayPixelsHigh(displayId);
        if (pixelWidth > 0 && pixelHeight > 0) {
            return NSMakeSize((CGFloat)pixelWidth, (CGFloat)pixelHeight);
        }
    }

    CGFloat scale = screen.backingScaleFactor > 0 ? screen.backingScaleFactor : 1.0;
    return NSMakeSize(NSWidth(screen.frame) * scale, NSHeight(screen.frame) * scale);
}

static OPNDisplayStreamProfile ResolveDisplayStreamProfile(NSWindow *window) {
    NSSize displayPixels = CurrentDisplayPixelSize(window);
    int displayWidth = std::max(640, (int)std::llround(displayPixels.width));
    int displayHeight = std::max(360, (int)std::llround(displayPixels.height));

    struct Candidate {
        int width;
        int height;
    };
    static const Candidate candidates[] = {
        {1280, 720}, {1280, 800}, {1366, 768}, {1440, 900},
        {1600, 900}, {1680, 1050}, {1920, 1080}, {1920, 1200},
        {2560, 1080}, {2560, 1440}, {2560, 1600}, {2880, 1800},
        {3440, 1440}, {3456, 2160}, {3840, 1600}, {3840, 2160},
    };

    double targetAspect = (double)displayWidth / (double)displayHeight;
    double targetPixels = (double)displayWidth * (double)displayHeight;
    const Candidate *best = &candidates[0];
    double bestScore = std::numeric_limits<double>::max();

    for (const Candidate &candidate : candidates) {
        double candidateAspect = (double)candidate.width / (double)candidate.height;
        double candidatePixels = (double)candidate.width * (double)candidate.height;
        double aspectScore = std::fabs(std::log(candidateAspect / targetAspect));
        double pixelScore = std::fabs(candidatePixels - targetPixels) / targetPixels;
        double oversizePenalty = candidatePixels > targetPixels ? 0.05 : 0.0;
        double score = (aspectScore * 20.0) + pixelScore + oversizePenalty;
        if (score < bestScore) {
            bestScore = score;
            best = &candidate;
        }
    }

    OPNDisplayStreamProfile profile;
    profile.displayWidth = displayWidth;
    profile.displayHeight = displayHeight;
    profile.streamWidth = best->width;
    profile.streamHeight = best->height;
    profile.resolution = std::to_string(best->width) + "x" + std::to_string(best->height);
    return profile;
}

static OPN::StreamResolutionOption OPNPowerSaverResolution(const OPN::StreamPreferenceProfile &profile) {
    double aspect = profile.AspectRatio();
    if (aspect <= 0.1 || !std::isfinite(aspect)) aspect = 16.0 / 9.0;

    int width = std::min(profile.resolution.width, 1280);
    int height = (int)std::llround((double)width / aspect);
    if (height > 800) {
        height = 800;
        width = (int)std::llround((double)height * aspect);
    }
    width = std::max(640, width - (width % 2));
    height = std::max(360, height - (height % 2));
    return {width, height};
}

static int OPNEffectiveMaxBitrateMbps(const OPN::StreamPreferenceProfile &profile) {
    if (profile.enablePowerSaver) return std::min(profile.maxBitrateMbps, 15);
    return profile.maxBitrateMbps;
}

static std::string OPNEffectiveStreamCodec(const OPN::StreamPreferenceProfile &profile, OPN::StreamWebRTCBackend backend) {
    std::string requested = profile.codec.value.empty() ? "H264" : profile.codec.value;
    std::transform(requested.begin(), requested.end(), requested.begin(), [](unsigned char c) { return (char)std::toupper(c); });
    if (requested != "AUTO") return requested;
    return backend == OPN::StreamWebRTCBackend::LibWebRTC ? "H264" : "H264";
}

static OPN::StreamSettings OPNSettingsWithNegotiatedProfile(OPN::StreamSettings settings, const OPN::SessionInfo &sessionInfo) {
    const OPN::NegotiatedStreamProfile &profile = sessionInfo.negotiatedStreamProfile;
    if (!profile.resolution.empty()) settings.resolution = profile.resolution;
    if (profile.fps > 0) settings.fps = profile.fps;
    if (!profile.codec.empty()) settings.codec = profile.codec;
    if (!profile.colorQuality.empty()) settings.colorQuality = profile.colorQuality;
    return settings;
}

static std::string ExtractIceUfragFromOffer(const std::string &sdp) {
    std::stringstream ss(sdp);
    std::string line;
    while (std::getline(ss, line)) {
        if (!line.empty() && line.back() == '\r') line.pop_back();
        const char *prefix = "a=ice-ufrag:";
        if (line.rfind(prefix, 0) == 0) {
            return line.substr(strlen(prefix));
        }
    }
    return "";
}

static void InjectManualIceCandidate(OPN::IStreamSession *session, const OPN::SessionInfo &sessionInfo, const std::string &serverIceUfrag) {
    if (!session) return;
    const char *enabled = getenv("OPN_INJECT_MANUAL_ICE");
    if (!enabled || strcmp(enabled, "1") != 0) {
        NSLog(@"[StreamVC] Manual ICE candidate injection disabled; relying on offer and signaled remote candidates");
        return;
    }

    std::string ip = ExtractPublicIp(sessionInfo.mediaConnectionInfo.ip);
    int port = sessionInfo.mediaConnectionInfo.port;
    if (ip.empty() || port <= 0) {
        NSLog(@"[StreamVC] No valid mediaConnectionInfo for manual ICE candidate (ip=%s, port=%d)",
              sessionInfo.mediaConnectionInfo.ip.c_str(), port);
        return;
    }

    std::string candidate = "candidate:1 1 udp 2130706431 " + ip + " " + std::to_string(port) + " typ host";
    OPN::IceCandidatePayload payload;
    payload.candidate = candidate;
    payload.sdpMid = "0";
    payload.sdpMLineIndex = 0;
    payload.usernameFragment = serverIceUfrag;
    NSLog(@"[StreamVC] Injecting manual ICE candidate: %s:%d (sdpMid=0 ufrag=%s)",
          ip.c_str(),
          port,
          serverIceUfrag.empty() ? "(none)" : serverIceUfrag.c_str());
    session->AddRemoteIceCandidate(payload);
}

static BOOL OPNIsCommandQEvent(NSEvent *event) {
    if (!event || event.type != NSEventTypeKeyDown) return NO;
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    return (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"q"];
}

static BOOL OPNIsCommandNEvent(NSEvent *event) {
    if (!event || event.type != NSEventTypeKeyDown) return NO;
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    return (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"n"];
}

static BOOL OPNIsCommandMEvent(NSEvent *event) {
    if (!event || event.type != NSEventTypeKeyDown) return NO;
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    return (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"m"];
}

static BOOL OPNIsCommandGEvent(NSEvent *event) {
    if (!event || event.type != NSEventTypeKeyDown) return NO;
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    return (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"g"];
}

static BOOL OPNIsCommandREvent(NSEvent *event) {
    if (!event || event.type != NSEventTypeKeyDown) return NO;
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    return (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"r"];
}

static BOOL OPNIsCommandHEvent(NSEvent *event) {
    if (!event || event.type != NSEventTypeKeyDown) return NO;
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    return (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"h"];
}

static BOOL OPNIsCommandLEvent(NSEvent *event) {
    if (!event || event.type != NSEventTypeKeyDown) return NO;
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    return (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"l"];
}

static std::string OPNLowercaseCopy(std::string value) {
    std::transform(value.begin(), value.end(), value.begin(), [](unsigned char c) { return (char)std::tolower(c); });
    return value;
}

static BOOL OPNStreamErrorIsRecoverable(const std::string &error) {
    if (error.empty()) return NO;
    std::string lower = OPNLowercaseCopy(error);
    if (lower.find("invalid game id") != std::string::npos) return NO;
    if (lower.find("terminal error state") != std::string::npos) return NO;
    if (lower.find("401") != std::string::npos || lower.find("unauthorized") != std::string::npos) return NO;

    return lower.find("connection") != std::string::npos ||
           lower.find("webrtc") != std::string::npos ||
           lower.find("ice") != std::string::npos ||
           lower.find("signaling") != std::string::npos ||
           lower.find("timeout") != std::string::npos ||
           lower.find("stream connection lost") != std::string::npos;
}

static NSTimeInterval OPNRecoveryDelayForAttempt(NSInteger attempt) {
    static const NSTimeInterval delays[] = {0.0, 3.0};
    if (attempt <= 0) return delays[0];
    NSInteger index = MIN(attempt - 1, (NSInteger)(sizeof(delays) / sizeof(delays[0])) - 1);
    return delays[index];
}

static NSColor *OPNQuitColor(CGFloat r, CGFloat g, CGFloat b, CGFloat a) {
    return [NSColor colorWithCalibratedRed:r green:g blue:b alpha:a];
}

static NSTextField *OPNQuitLabel(NSString *text, CGFloat size, NSFontWeight weight, NSColor *color, NSTextAlignment alignment) {
    NSTextField *label = [[NSTextField alloc] initWithFrame:NSZeroRect];
    label.stringValue = text;
    label.font = [NSFont systemFontOfSize:size weight:weight];
    label.textColor = color;
    label.alignment = alignment;
    label.drawsBackground = NO;
    label.bordered = NO;
    label.editable = NO;
    label.selectable = NO;
    return label;
}

static void OPNStyleQuitButton(NSButton *button, NSColor *background, NSColor *textColor) {
    button.bezelStyle = NSBezelStyleRegularSquare;
    button.bordered = NO;
    button.wantsLayer = YES;
    button.layer.cornerRadius = 10.0;
    button.layer.backgroundColor = background.CGColor;
    button.attributedTitle = [[NSAttributedString alloc] initWithString:button.title attributes:@{
        NSFontAttributeName: [NSFont systemFontOfSize:13.0 weight:NSFontWeightSemibold],
        NSForegroundColorAttributeName: textColor,
    }];
}

@implementation OPNQuitGameOverlayView {
    NSRect _cardFrame;
    NSTextField *_brandLabel;
    NSTextField *_eyebrowLabel;
    NSTextField *_titleLabel;
    NSTextField *_messageLabel;
    NSButton *_cancelButton;
    NSButton *_quitButton;
}

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;

        _brandLabel = OPNQuitLabel(@"OpenNOW", 13.0, NSFontWeightSemibold,
                                   OPNQuitColor(0.72, 0.74, 0.78, 1.0), NSTextAlignmentLeft);
        _eyebrowLabel = OPNQuitLabel(@"Command-Q", 12.0, NSFontWeightMedium,
                                     OPNQuitColor(0.48, 0.50, 0.56, 1.0), NSTextAlignmentLeft);
        _titleLabel = OPNQuitLabel(@"End stream?", 24.0, NSFontWeightSemibold,
                                   OPNQuitColor(0.96, 0.96, 0.98, 1.0), NSTextAlignmentLeft);
        _messageLabel = OPNQuitLabel(@"Your stream will close and you will return to the library. Unsaved in-game progress may be lost.",
                                     13.0, NSFontWeightRegular,
                                     OPNQuitColor(0.66, 0.68, 0.73, 1.0), NSTextAlignmentLeft);
        _messageLabel.maximumNumberOfLines = 2;

        _cancelButton = [NSButton buttonWithTitle:@"Cancel" target:self action:@selector(cancelPressed:)];
        OPNStyleQuitButton(_cancelButton,
                           OPNQuitColor(0.20, 0.21, 0.24, 0.95),
                           OPNQuitColor(0.88, 0.89, 0.92, 1.0));
        _cancelButton.layer.borderWidth = 1.0;
        _cancelButton.layer.borderColor = OPNQuitColor(1.0, 1.0, 1.0, 0.10).CGColor;

        _quitButton = [NSButton buttonWithTitle:@"End Stream" target:self action:@selector(quitPressed:)];
        OPNStyleQuitButton(_quitButton,
                           OPNQuitColor(0.00, 0.48, 1.0, 1.0),
                           [NSColor whiteColor]);
        _quitButton.layer.shadowColor = OPNQuitColor(0.0, 0.0, 0.0, 1.0).CGColor;
        _quitButton.layer.shadowOpacity = 0.20;
        _quitButton.layer.shadowRadius = 12.0;
        _quitButton.layer.shadowOffset = CGSizeMake(0, -3);

        [self addSubview:_brandLabel];
        [self addSubview:_eyebrowLabel];
        [self addSubview:_titleLabel];
        [self addSubview:_messageLabel];
        [self addSubview:_cancelButton];
        [self addSubview:_quitButton];
    }
    return self;
}

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    [self.window makeFirstResponder:self];
}

- (void)layout {
    [super layout];
    CGFloat cardW = MIN(460.0, MAX(320.0, NSWidth(self.bounds) - 80.0));
    CGFloat cardH = 236.0;
    _cardFrame = NSMakeRect(floor((NSWidth(self.bounds) - cardW) / 2.0),
                            floor((NSHeight(self.bounds) - cardH) / 2.0),
                            cardW,
                            cardH);
    CGFloat x = NSMinX(_cardFrame);
    CGFloat top = NSMaxY(_cardFrame);
    CGFloat padding = 28.0;
    _brandLabel.frame = NSMakeRect(x + padding, top - 43.0, 140.0, 18.0);
    _eyebrowLabel.frame = NSMakeRect(NSMaxX(_cardFrame) - padding - 82.0, top - 43.0, 82.0, 18.0);
    _titleLabel.frame = NSMakeRect(x + padding, top - 88.0, cardW - padding * 2.0, 30.0);
    _messageLabel.frame = NSMakeRect(x + padding, top - 134.0, cardW - padding * 2.0, 42.0);

    CGFloat buttonW = 124.0;
    CGFloat buttonY = NSMinY(_cardFrame) + 26.0;
    _quitButton.frame = NSMakeRect(NSMaxX(_cardFrame) - padding - buttonW, buttonY, buttonW, 42.0);
    _cancelButton.frame = NSMakeRect(NSMinX(_quitButton.frame) - 12.0 - buttonW, buttonY, buttonW, 42.0);
    [self setNeedsDisplay:YES];
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    [[NSColor colorWithCalibratedWhite:0.0 alpha:0.52] setFill];
    NSRectFill(self.bounds);

    NSGradient *veil = [[NSGradient alloc] initWithStartingColor:OPNQuitColor(0.12, 0.13, 0.16, 0.58)
                                                    endingColor:OPNQuitColor(0.04, 0.05, 0.06, 0.84)];
    [veil drawInRect:self.bounds angle:90.0];

    NSBezierPath *outer = [NSBezierPath bezierPathWithRoundedRect:_cardFrame xRadius:22.0 yRadius:22.0];
    [OPNQuitColor(1.0, 1.0, 1.0, 0.12) setFill];
    [outer fill];

    NSRect innerRect = NSInsetRect(_cardFrame, 1.0, 1.0);
    NSBezierPath *inner = [NSBezierPath bezierPathWithRoundedRect:innerRect xRadius:21.0 yRadius:21.0];
    NSGradient *card = [[NSGradient alloc] initWithStartingColor:OPNQuitColor(0.15, 0.16, 0.18, 0.96)
                                                    endingColor:OPNQuitColor(0.10, 0.11, 0.13, 0.96)];
    [card drawInBezierPath:inner angle:90.0];

    NSRect divider = NSMakeRect(NSMinX(_cardFrame) + 28.0, NSMinY(_cardFrame) + 78.0,
                                NSWidth(_cardFrame) - 56.0, 1.0);
    [OPNQuitColor(1.0, 1.0, 1.0, 0.08) setFill];
    NSRectFill(divider);
}

- (void)cancelPressed:(id)sender {
    (void)sender;
    if (self.onCancel) self.onCancel();
}

- (void)quitPressed:(id)sender {
    (void)sender;
    if (self.onQuit) self.onQuit();
}

- (void)keyDown:(NSEvent *)event {
    NSString *key = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    BOOL commandQ = (event.modifierFlags & NSEventModifierFlagCommand) && [key isEqualToString:@"q"];
    if (event.keyCode == 53) {
        if (self.onCancel) self.onCancel();
        return;
    }
    if (event.keyCode == 36 || commandQ) {
        if (self.onQuit) self.onQuit();
        return;
    }
    [super keyDown:event];
}

@end

@implementation OPNShortcutLegendView {
    NSTextField *_titleLabel;
    NSArray<NSTextField *> *_shortcutLabels;
    NSArray<NSTextField *> *_descriptionLabels;
}

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.layer.cornerRadius = 18.0;
        self.layer.backgroundColor = OPNQuitColor(0.03, 0.035, 0.045, 0.90).CGColor;
        self.layer.borderWidth = 1.0;
        self.layer.borderColor = OPNQuitColor(1.0, 1.0, 1.0, 0.12).CGColor;

        _titleLabel = OPNQuitLabel(@"Shortcuts", 18.0, NSFontWeightSemibold, OPNQuitColor(0.96, 0.97, 0.99, 1.0), NSTextAlignmentLeft);
        [self addSubview:_titleLabel];

        NSArray<NSString *> *shortcuts = @[@"Command-H", @"Command-G", @"Command-R", @"Command-N", @"Command-M", @"Command-L", @"Command-Q", @"Hold Esc"];
        NSArray<NSString *> *descriptions = @[@"Toggle this legend", @"Audio HUD", @"Record stream", @"Stats HUD", @"Toggle microphone", @"Copy logs", @"Quit stream", @"Release pointer"];
        NSMutableArray<NSTextField *> *shortcutLabels = [NSMutableArray arrayWithCapacity:shortcuts.count];
        NSMutableArray<NSTextField *> *descriptionLabels = [NSMutableArray arrayWithCapacity:descriptions.count];
        for (NSUInteger i = 0; i < shortcuts.count; i++) {
            NSTextField *shortcut = OPNQuitLabel(shortcuts[i], 12.0, NSFontWeightSemibold, OPNQuitColor(0.75, 0.92, 0.86, 1.0), NSTextAlignmentLeft);
            NSTextField *description = OPNQuitLabel(descriptions[i], 12.0, NSFontWeightRegular, OPNQuitColor(0.74, 0.76, 0.80, 1.0), NSTextAlignmentRight);
            [shortcutLabels addObject:shortcut];
            [descriptionLabels addObject:description];
            [self addSubview:shortcut];
            [self addSubview:description];
        }
        _shortcutLabels = shortcutLabels;
        _descriptionLabels = descriptionLabels;
    }
    return self;
}

- (void)layout {
    [super layout];
    CGFloat padding = 20.0;
    CGFloat width = NSWidth(self.bounds);
    CGFloat top = NSHeight(self.bounds);
    _titleLabel.frame = NSMakeRect(padding, top - 42.0, width - padding * 2.0, 22.0);
    for (NSUInteger i = 0; i < _shortcutLabels.count; i++) {
        CGFloat y = top - 78.0 - (CGFloat)i * 28.0;
        _shortcutLabels[i].frame = NSMakeRect(padding, y, 112.0, 18.0);
        _descriptionLabels[i].frame = NSMakeRect(132.0, y, width - 132.0 - padding, 18.0);
    }
}

@end

@implementation OPNStatsOverlayView {
    CALayer *_textTintLayer;
    NSTextField *_statsLineLabel;
}

static NSTextField *OPNStatsText(NSString *text, CGFloat size, NSFontWeight weight, NSColor *color, NSTextAlignment alignment) {
    NSTextField *label = [[NSTextField alloc] initWithFrame:NSZeroRect];
    label.stringValue = text ?: @"";
    label.font = [NSFont systemFontOfSize:size weight:weight];
    label.textColor = color;
    label.alignment = alignment;
    label.drawsBackground = NO;
    label.bordered = NO;
    label.editable = NO;
    label.selectable = NO;
    label.lineBreakMode = NSLineBreakByTruncatingTail;
    return label;
}

static NSAttributedString *OPNStatsOutlinedLine(NSString *text) {
    NSMutableParagraphStyle *style = [[NSMutableParagraphStyle alloc] init];
    style.alignment = NSTextAlignmentCenter;
    style.lineBreakMode = NSLineBreakByTruncatingMiddle;
    return [[NSAttributedString alloc] initWithString:text ?: @""
                                           attributes:@{
        NSFontAttributeName: [NSFont monospacedSystemFontOfSize:12.0 weight:NSFontWeightSemibold],
        NSForegroundColorAttributeName: OPNQuitColor(1.0, 1.0, 1.0, 1.0),
        NSStrokeColorAttributeName: OPNQuitColor(0.0, 0.0, 0.0, 0.95),
        NSStrokeWidthAttributeName: @-3.0,
        NSParagraphStyleAttributeName: style,
    }];
}

static NSString *OPNStatsZoneName(NSString *zone) {
    NSString *trimmed = [zone stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    if (trimmed.length == 0) return @"pending";

    NSString *host = [NSURLComponents componentsWithString:trimmed].host;
    if (host.length == 0) {
        host = trimmed;
        NSRange schemeRange = [host rangeOfString:@"://"];
        if (schemeRange.location != NSNotFound) {
            host = [host substringFromIndex:NSMaxRange(schemeRange)];
        }
        NSRange pathRange = [host rangeOfString:@"/"];
        if (pathRange.location != NSNotFound) {
            host = [host substringToIndex:pathRange.location];
        }
    }

    NSRange portRange = [host rangeOfString:@":"];
    if (portRange.location != NSNotFound) {
        host = [host substringToIndex:portRange.location];
    }
    NSArray<NSString *> *labels = [host componentsSeparatedByString:@"."];
    NSString *zoneName = labels.count > 0 ? labels.firstObject : host;
    return zoneName.length > 0 ? zoneName : @"pending";
}

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.autoresizingMask = NSViewWidthSizable | NSViewMinYMargin;
        _textTintLayer = [CALayer layer];
        _textTintLayer.backgroundColor = OPNQuitColor(0.0, 0.0, 0.0, 0.42).CGColor;
        _textTintLayer.cornerRadius = 8.0;
        [self.layer addSublayer:_textTintLayer];

        _statsLineLabel = OPNStatsText(@"", 12.0, NSFontWeightSemibold, NSColor.clearColor, NSTextAlignmentCenter);
        _statsLineLabel.lineBreakMode = NSLineBreakByTruncatingMiddle;
        _statsLineLabel.maximumNumberOfLines = 1;
        _statsLineLabel.attributedStringValue = OPNStatsOutlinedLine(@"Stats: measuring");
        [self addSubview:_statsLineLabel];
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (NSView *)hitTest:(NSPoint)point {
    (void)point;
    return nil;
}

- (void)layout {
    [super layout];
    _textTintLayer.frame = NSInsetRect(self.bounds, 4.0, 1.0);
    _statsLineLabel.frame = NSInsetRect(self.bounds, 10.0, 2.0);
}

- (void)updateLatencyMs:(NSInteger)latencyMs
               jitterMs:(NSInteger)jitterMs
            bitrateMbps:(double)bitrateMbps
            packetsLost:(int64_t)packetsLost
       packetLossPercent:(double)packetLossPercent
                     gpu:(NSString *)gpu
              resolution:(NSString *)resolution
                     fps:(NSInteger)fps
               renderFps:(double)renderFps
                   codec:(NSString *)codec
                    zone:(NSString *)zone
                 decoder:(NSString *)decoder
            decodeTimeMs:(double)decodeTimeMs
                     sink:(NSString *)sink
            pipelineMode:(NSString *)pipelineMode
            webrtcBackend:(NSString *)webrtcBackend
           framesReceived:(uint64_t)framesReceived
             framesDecoded:(uint64_t)framesDecoded
             framesDropped:(uint64_t)framesDropped {
    NSString *latencyText = latencyMs >= 0 ? [NSString stringWithFormat:@"%ld ms", (long)latencyMs] : @"measuring";
    NSString *jitterText = jitterMs >= 0 ? [NSString stringWithFormat:@"%ld ms", (long)jitterMs] : @"--";
    NSString *bitrateText = bitrateMbps >= 0.0 ? [NSString stringWithFormat:@"%.1f Mbps", bitrateMbps] : @"--";
    NSString *lossText = @"--";
    if (packetLossPercent >= 0.0 && packetsLost >= 0) {
        lossText = [NSString stringWithFormat:@"%.2f%%/%lld", packetLossPercent, (long long)packetsLost];
    } else if (packetsLost >= 0) {
        lossText = [NSString stringWithFormat:@"%lld", (long long)packetsLost];
    }
    (void)gpu;

    NSString *streamText = @"--";
    if (resolution.length > 0 && fps > 0) {
        streamText = [NSString stringWithFormat:@"%@@%ld", resolution, (long)fps];
    } else if (resolution.length > 0) {
        streamText = resolution;
    }
    if (codec.length > 0) {
        streamText = [NSString stringWithFormat:@"%@/%@", streamText, codec];
    }
    NSString *serverText = OPNStatsZoneName(zone);
    NSString *decodeText = @"pending";
    if (decoder.length > 0 && decodeTimeMs >= 0.0) {
        decodeText = [NSString stringWithFormat:@"%@ %.1f ms", decoder, decodeTimeMs];
    } else if (decoder.length > 0) {
        decodeText = decoder;
    }
    NSString *renderText = @"pending";
    if (framesReceived > 0 || framesDecoded > 0 || framesDropped > 0) {
        NSString *fpsText = renderFps >= 0.0 ? [NSString stringWithFormat:@"%.0f fps ", renderFps] : @"";
        NSString *sinkText = sink.length > 0 ? [NSString stringWithFormat:@" %@", sink] : @"";
        renderText = [NSString stringWithFormat:@"%@%llu drop%@",
                      fpsText,
                      (unsigned long long)framesDropped,
                      sinkText];
    } else if (sink.length > 0 && pipelineMode.length > 0) {
        renderText = [NSString stringWithFormat:@"%@/%@", sink, pipelineMode];
    }
    NSArray<NSString *> *parts = @[
        [NSString stringWithFormat:@"Latency %@", latencyText],
        [NSString stringWithFormat:@"Jitter %@", jitterText],
        [NSString stringWithFormat:@"Bitrate %@", bitrateText],
        [NSString stringWithFormat:@"Loss %@", lossText],
        [NSString stringWithFormat:@"Stream %@", streamText],
        [NSString stringWithFormat:@"Server %@", serverText],
        [NSString stringWithFormat:@"Decode %@", decodeText],
        [NSString stringWithFormat:@"Frames %@", renderText],
    ];
    _statsLineLabel.attributedStringValue = OPNStatsOutlinedLine([parts componentsJoinedByString:@"  |  "]);
}

@end

static void OPNReleaseSignalingClientAfterCallbacks(OPN::SignalingClient *client) {
    if (!client) return;
    client->Disconnect();
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(2.0 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        delete client;
    });
}

static void OPNReleaseStreamSessionAfterCallbacks(OPN::IStreamSession *session) {
    if (!session) return;
    session->Stop();
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(2.0 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        delete session;
    });
}

@implementation OPNStreamViewController {
    OPN::SignalingClient *_signaling;
    OPN::IStreamSession *_session;
    NSRect _initialViewFrame;
    id _quitKeyMonitor;
    BOOL _streamEnded;
    BOOL _remoteIceReceived;
    BOOL _connectedOnce;
    BOOL _recovering;
    NSInteger _recoveryAttempt;
    NSUInteger _launchGeneration;
    NSUInteger _stableResetGeneration;
    void *_remoteIceGraceTimer;
    std::string _webRTCBackendName;
    OPN::SessionInfo _activeSessionInfo;
    BOOL _hasActiveSessionInfo;
    BOOL _remoteStopRequested;
    NSView *_connectedToast;
}

- (instancetype)initWithGameTitle:(const std::string &)title
                             appId:(const std::string &)appId
                          apiToken:(const std::string &)token
                     accountLinked:(bool)accountLinked
                      selectedStore:(const std::string &)selectedStore {
    return [self initWithGameTitle:title
                             appId:appId
                          apiToken:token
                     accountLinked:accountLinked
                     selectedStore:selectedStore
                   resumeSessionId:""
                       resumeServer:""];
}

- (instancetype)initWithGameTitle:(const std::string &)title
                             appId:(const std::string &)appId
                          apiToken:(const std::string &)token
                     accountLinked:(bool)accountLinked
                     selectedStore:(const std::string &)selectedStore
                   resumeSessionId:(const std::string &)resumeSessionId
                       resumeServer:(const std::string &)resumeServer {
    self = [super init];
    if (self) {
        _gameTitle = title;
        _appId = appId;
        _apiToken = token;
        _accountLinked = accountLinked;
        _selectedStore = selectedStore;
        _resumeSessionId = resumeSessionId;
        _resumeServer = resumeServer;
        _resumeExistingSession = !_resumeSessionId.empty() && !_resumeServer.empty();
        OPN::StreamWebRTCBackend backend = OPN::ResolveStreamWebRTCBackend();
        _session = OPN::CreateStreamSession(backend).release();
        _webRTCBackendName = OPN::StreamWebRTCBackendName(backend);
        NSLog(@"[StreamVC] WebRTC backend selected: %s", OPN::StreamWebRTCBackendName(backend).c_str());
        _initialViewFrame = NSMakeRect(0, 0, 1, 1);
        _signaling = nullptr;
        _streamStarted = NO;
        _streamEnded = NO;
        _remoteIceReceived = NO;
        _connectedOnce = NO;
        _recovering = NO;
        _hasActiveSessionInfo = NO;
        _remoteStopRequested = NO;
        _recoveryAttempt = 0;
        _launchGeneration = 0;
        _stableResetGeneration = 0;
        _remoteIceGraceTimer = nullptr;
        _connectedToast = nil;
    }
    return self;
}

- (void)startStreamIfNeeded {
    if (_streamStarted || _streamEnded) return;
    _streamStarted = YES;
    [self startStreamLaunchFlow];
}

- (void)setInitialViewFrame:(NSRect)frame {
    if (NSWidth(frame) <= 0 || NSHeight(frame) <= 0) return;
    _initialViewFrame = frame;
    if (self.isViewLoaded) {
        self.view.frame = frame;
    }
}

- (void)loadView {
    OPNStreamView *view = [[OPNStreamView alloc] initWithFrame:_initialViewFrame];
    view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    self.view = view;
    self.streamView = view;
    __weak __typeof__(self) weakSelf = self;
    view.onGuideButtonPressed = ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf || strongSelf->_streamEnded) return;
        if (strongSelf.onControllerLibraryRequested) strongSelf.onControllerLibraryRequested();
    };
    [self.streamView setStreamSession:_session];
    [self.streamView setRecordingGameTitle:[NSString stringWithUTF8String:_gameTitle.c_str()]];
    NSLog(@"[StreamVC] loadView called, view=%p", (__bridge void *)view);
}

- (NSView *)streamPictureInPictureView {
    return self.view;
}

- (void)prepareForLibraryPictureInPicture {
    [self removeQuitShortcutMonitor];
    [self dismissQuitGameOverlayAndRefocus:NO];
    [self.streamView setInputSuspendedForLibraryOverlay:YES];
    self.view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
}

- (void)restoreFromLibraryPictureInPicture {
    self.view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [self.streamView setInputSuspendedForLibraryOverlay:NO];
    [self installQuitShortcutMonitor];
    [self.streamView takeFocus];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    NSLog(@"[StreamVC] viewDidLoad called");

    NSRect bounds = self.view.bounds;
    NSLog(@"[StreamVC] view bounds: %@", NSStringFromRect(bounds));

    self.loadingView = [[OPNLoadingView alloc] initWithFrame:[self loadingViewFrame]
                                                     message:@"Starting session..."];
    self.loadingView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [self.loadingView setSteps:@[@"Allocate cloud session",
                                 @"Connect to game server",
                                 @"Receive stream offer",
                                 @"Negotiate WebRTC",
                                 @"Start video pipeline",
                                 @"Connected"]
                   currentStepIndex:-1];
    [self.view addSubview:self.loadingView];
    self.statusLabel = self.loadingView.messageLabel;
    [self configureLoadingViewAdHandler];
}

- (void)viewDidLayout {
    [super viewDidLayout];
    if (self.statsOverlay) {
        self.statsOverlay.frame = [self statsOverlayFrame];
    }
    if (self.shortcutLegendOverlay) {
        self.shortcutLegendOverlay.frame = [self shortcutLegendFrame];
    }
    if (self.loadingView) {
        self.loadingView.frame = [self loadingViewFrame];
    }
}

- (void)viewWillAppear {
    [super viewWillAppear];
    NSLog(@"[StreamVC] viewWillAppear called");
}

- (void)viewDidAppear {
    [super viewDidAppear];
    NSLog(@"[StreamVC] viewDidAppear called, streamStarted=%d, streamEnded=%d", _streamStarted, _streamEnded);
    [self installQuitShortcutMonitor];
    if (!_streamStarted && !_streamEnded) {
        NSLog(@"[StreamVC] Triggering startStreamLaunchFlow");
        [self startStreamIfNeeded];
    }
}

- (void)viewWillDisappear {
    [super viewWillDisappear];
    [self removeQuitShortcutMonitor];
}

- (void)setStatus:(NSString *)msg {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (self.loadingView) {
            self.loadingView.message = msg;
        }
        if (self.statusLabel) {
            self.statusLabel.stringValue = msg;
        }
    });
}

- (NSRect)loadingViewFrame {
    return self.view.bounds;
}

- (void)setLaunchStep:(NSInteger)stepIndex message:(NSString *)message {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (self.loadingView) {
            if (stepIndex >= self.loadingView.currentStepIndex) {
                [self.loadingView advanceToStep:stepIndex message:message];
            }
        }
        if (self.statusLabel) {
            self.statusLabel.stringValue = message;
        }
    });
}

- (void)configureLoadingViewAdHandler {
    if (!self.loadingView) return;
    __weak __typeof__(self) weakSelf = self;
    self.loadingView.adPlaybackEventHandler = ^(NSString *adId, NSString *action, NSInteger watchedTimeInMs, NSInteger pausedTimeInMs, NSString *cancelReason) {
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf || strongSelf->_streamEnded || !strongSelf->_hasActiveSessionInfo) return;
        std::string adIdString = adId.length > 0 ? [adId UTF8String] : "";
        std::string actionString = action.length > 0 ? [action UTF8String] : "";
        std::string cancelReasonString = cancelReason.length > 0 ? [cancelReason UTF8String] : "";
        OPN::SessionInfo sessionInfo = strongSelf->_activeSessionInfo;
        OPN::SessionManager::Shared().ReportSessionAd(sessionInfo,
                                                       adIdString,
                                                       actionString,
                                                       (int)watchedTimeInMs,
                                                       (int)pausedTimeInMs,
                                                       cancelReasonString,
                                                       [weakSelf](bool ok, const OPN::SessionInfo &updatedInfo, const std::string &error) {
            __typeof__(self) callbackSelf = weakSelf;
            if (!callbackSelf || callbackSelf->_streamEnded) return;
            if (!ok) {
                NSLog(@"[StreamVC] Ad report failed: %s", error.c_str());
                return;
            }
            OPN::SessionInfo updatedInfoCopy = updatedInfo;
            dispatch_async(dispatch_get_main_queue(), ^{
                if (!callbackSelf || callbackSelf->_streamEnded || !callbackSelf.loadingView) return;
                callbackSelf->_activeSessionInfo = updatedInfoCopy;
                callbackSelf->_hasActiveSessionInfo = YES;
                [callbackSelf.loadingView updateAdState:updatedInfoCopy.adState];
            });
        });
    };
}

- (void)updateLaunchAdState:(const OPN::SessionInfo &)sessionInfo {
    OPN::SessionInfo sessionInfoCopy = sessionInfo;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!self.loadingView || self->_streamEnded) return;
        if (!sessionInfoCopy.sessionId.empty()) {
            self->_activeSessionInfo = sessionInfoCopy;
            self->_hasActiveSessionInfo = YES;
        }
        [self.loadingView updateQueuePosition:sessionInfoCopy.queuePosition];
        if (sessionInfoCopy.adState.isAdsRequired) {
            [self.loadingView updateAdState:sessionInfoCopy.adState];
        } else {
            [self.loadingView clearAdPresentation];
        }
    });
}

- (void)finishLaunchMeasurementWithSuccess:(BOOL)success reason:(NSString *)reason {
    if (!self.launchSignpostActive) return;
    self.launchSignpostActive = NO;
    CFTimeInterval elapsedMs = (CACurrentMediaTime() - self.launchStartTime) * 1000.0;
    NSString *safeReason = reason.length > 0 ? reason : @"unknown";
    os_signpost_interval_end(OPNStreamPerformanceLog(),
                             self.launchSignpostId,
                             "StreamLaunch",
                             "success=%{public}s elapsed_ms=%.1f reason=%{public}@",
                             success ? "yes" : "no",
                             elapsedMs,
                             safeReason);
    NSLog(@"[StreamVC] Stream launch measurement success=%d elapsed=%.1fms reason=%@", success, elapsedMs, safeReason);
}

- (void)installQuitShortcutMonitor {
    if (_quitKeyMonitor) return;
    __weak __typeof__(self) weakSelf = self;
	_quitKeyMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown
	                                                            handler:^NSEvent *(NSEvent *event) {
	    __typeof__(self) strongSelf = weakSelf;
	    if (!strongSelf || strongSelf->_streamEnded) return event;
	    if (!strongSelf.view.window || event.window != strongSelf.view.window) return event;
	    if (OPNIsCommandQEvent(event)) {
	        [strongSelf requestQuitGameConfirmation];
	        return (NSEvent *)nil;
	    }
	    if (OPNIsCommandNEvent(event)) {
	        [strongSelf toggleStatsOverlay];
	        return (NSEvent *)nil;
	    }
	    if (OPNIsCommandMEvent(event)) {
	        [strongSelf.streamView toggleMicrophoneEnabledShortcut];
	        return (NSEvent *)nil;
	    }
	    if (OPNIsCommandGEvent(event)) {
	        [strongSelf.streamView toggleSidebarHUD];
	        return (NSEvent *)nil;
	    }
	    if (OPNIsCommandREvent(event)) {
	        [strongSelf.streamView toggleRecordingShortcut];
	        return (NSEvent *)nil;
	    }
	    if (OPNIsCommandHEvent(event)) {
	        [strongSelf toggleShortcutLegendOverlay];
	        return (NSEvent *)nil;
	    }
	    if (OPNIsCommandLEvent(event)) {
	        [strongSelf copyCurrentLogToClipboardFromShortcut];
	        return (NSEvent *)nil;
	    }
	    return event;
	}];
}

- (NSRect)statsOverlayFrame {
    CGFloat width = MAX(0.0, NSWidth(self.view.bounds) - 32.0);
    CGFloat height = 26.0;
    return NSMakeRect(16.0,
                      floor(NSHeight(self.view.bounds) - height - 10.0),
                      width,
                      height);
}

- (void)showConnectedToastWithResolution:(const std::string &)resolution
                                      fps:(int)fps
                                  bitrate:(int)bitrateMbps
                                    codec:(const std::string &)codec {
    [_connectedToast removeFromSuperview];

    CGFloat width = MIN(360.0, MAX(286.0, NSWidth(self.view.bounds) - 36.0));
    NSView *toast = [[NSView alloc] initWithFrame:NSMakeRect(floor((NSWidth(self.view.bounds) - width) / 2.0),
                                                             24.0,
                                                             width,
                                                             76.0)];
    toast.wantsLayer = YES;
    toast.layer.cornerRadius = 18.0;
    toast.layer.backgroundColor = OPNQuitColor(0.05, 0.07, 0.06, 0.86).CGColor;
    toast.layer.borderWidth = 1.0;
    toast.layer.borderColor = OPNQuitColor(0.35, 0.95, 0.56, 0.24).CGColor;
    toast.alphaValue = 0.0;

    NSTextField *title = OPNStatsText(@"Stream Connected", 14.0, NSFontWeightSemibold, OPNQuitColor(0.92, 1.0, 0.94, 1.0), NSTextAlignmentCenter);
    title.frame = NSMakeRect(18.0, 14.0, width - 36.0, 20.0);
    [toast addSubview:title];

    NSString *detail = [NSString stringWithFormat:@"%s • %s • %d fps • %d Mbps",
                        _webRTCBackendName.c_str(),
                        codec.c_str(),
                        fps,
                        bitrateMbps];
    if (!resolution.empty()) {
        detail = [NSString stringWithFormat:@"%@ • %s", detail, resolution.c_str()];
    }
    NSTextField *subtitle = OPNStatsText(detail, 11.0, NSFontWeightMedium, OPNQuitColor(0.78, 0.84, 0.78, 1.0), NSTextAlignmentCenter);
    subtitle.frame = NSMakeRect(18.0, 40.0, width - 36.0, 18.0);
    [toast addSubview:subtitle];

    _connectedToast = toast;
    [self.view addSubview:toast positioned:NSWindowAbove relativeTo:nil];
    [NSAnimationContext runAnimationGroup:^(NSAnimationContext *context) {
        context.duration = 0.18;
        toast.animator.alphaValue = 1.0;
    } completionHandler:^{
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2200 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
            if (self->_connectedToast != toast) return;
            [NSAnimationContext runAnimationGroup:^(NSAnimationContext *context) {
                context.duration = 0.25;
                toast.animator.alphaValue = 0.0;
            } completionHandler:^{
                if (self->_connectedToast == toast) self->_connectedToast = nil;
                [toast removeFromSuperview];
            }];
        });
    }];
}

- (NSRect)shortcutLegendFrame {
    CGFloat width = MIN(328.0, MAX(276.0, NSWidth(self.view.bounds) - 36.0));
    CGFloat height = 282.0;
    return NSMakeRect(floor(NSWidth(self.view.bounds) - width - 18.0),
                      floor((NSHeight(self.view.bounds) - height) / 2.0),
                      width,
                      height);
}

- (void)showLogCopiedToast {
    [_connectedToast removeFromSuperview];

    CGFloat width = MIN(340.0, MAX(276.0, NSWidth(self.view.bounds) - 36.0));
    NSView *toast = [[NSView alloc] initWithFrame:NSMakeRect(floor((NSWidth(self.view.bounds) - width) / 2.0),
                                                             24.0,
                                                             width,
                                                             64.0)];
    toast.wantsLayer = YES;
    toast.layer.cornerRadius = 16.0;
    toast.layer.backgroundColor = OPNQuitColor(0.05, 0.07, 0.06, 0.88).CGColor;
    toast.layer.borderWidth = 1.0;
    toast.layer.borderColor = OPNQuitColor(0.55, 0.76, 1.0, 0.30).CGColor;
    toast.alphaValue = 0.0;

    NSTextField *title = OPNStatsText(@"Log Copied", 14.0, NSFontWeightSemibold, OPNQuitColor(0.92, 0.96, 1.0, 1.0), NSTextAlignmentCenter);
    title.frame = NSMakeRect(18.0, 11.0, width - 36.0, 20.0);
    [toast addSubview:title];

    NSTextField *subtitle = OPNStatsText(@"Current OpenNOW log is on the clipboard", 11.0, NSFontWeightMedium, OPNQuitColor(0.78, 0.84, 0.88, 1.0), NSTextAlignmentCenter);
    subtitle.frame = NSMakeRect(18.0, 34.0, width - 36.0, 18.0);
    [toast addSubview:subtitle];

    _connectedToast = toast;
    [self.view addSubview:toast positioned:NSWindowAbove relativeTo:nil];
    [NSAnimationContext runAnimationGroup:^(NSAnimationContext *context) {
        context.duration = 0.16;
        toast.animator.alphaValue = 1.0;
    } completionHandler:^{
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 1800 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
            if (self->_connectedToast != toast) return;
            [NSAnimationContext runAnimationGroup:^(NSAnimationContext *context) {
                context.duration = 0.22;
                toast.animator.alphaValue = 0.0;
            } completionHandler:^{
                if (self->_connectedToast == toast) self->_connectedToast = nil;
                [toast removeFromSuperview];
            }];
        });
    }];
}

- (void)copyCurrentLogToClipboardFromShortcut {
    if (_streamEnded) return;
    OPN::AppendLogEvent(@"[StreamVC] Command-L requested log copy");
    OPN::CopyCapturedLogToClipboard(@"Command-L log copy");
    [self showLogCopiedToast];
    [self.streamView takeFocus];
    NSLog(@"[StreamVC] Current log copied to clipboard via CMD+L");
}

- (void)updateStatsOverlay {
    if (!self.statsOverlay) return;
    OPN::StreamStats stats;
    if (_session) {
        _session->RequestStats();
        stats = _session->GetLatestStats();
    }

    NSInteger latencyMs = stats.latencyMs >= 0 ? (NSInteger)std::llround(stats.latencyMs) : -1;
    NSInteger jitterMs = stats.jitterMs >= 0 ? (NSInteger)std::llround(stats.jitterMs) : -1;
    int64_t packetsLost = stats.available ? stats.packetsLost : -1;
    NSString *gpu = [NSString stringWithUTF8String:stats.gpuType.c_str()];
    NSString *resolution = [NSString stringWithUTF8String:stats.resolution.c_str()];
    NSString *codec = [NSString stringWithUTF8String:stats.codec.c_str()];
    NSString *zone = [NSString stringWithUTF8String:stats.zone.c_str()];
    NSString *decoder = [NSString stringWithUTF8String:stats.videoDecoder.c_str()];
    NSString *sink = [NSString stringWithUTF8String:stats.videoSink.c_str()];
    NSString *pipelineMode = [NSString stringWithUTF8String:stats.videoPipelineMode.c_str()];
    NSString *webrtcBackend = [NSString stringWithUTF8String:_webRTCBackendName.c_str()];
    [self.statsOverlay updateLatencyMs:latencyMs
                               jitterMs:jitterMs
                            bitrateMbps:stats.inboundBitrateMbps
                           packetsLost:packetsLost
                      packetLossPercent:stats.packetLossPercent
                                     gpu:gpu
                              resolution:resolution
                                      fps:stats.fps
                                renderFps:stats.renderFps
                                    codec:codec
                                     zone:zone
                                  decoder:decoder
                             decodeTimeMs:stats.decodeTimeMs
                                      sink:sink
                              pipelineMode:pipelineMode
                              webrtcBackend:webrtcBackend
                            framesReceived:stats.framesReceived
                             framesDecoded:stats.framesDecoded
                             framesDropped:stats.framesDropped];
}

- (void)startStatsRefreshTimer {
    if (self.statsRefreshTimer) return;
    self.statsRefreshTimer = [NSTimer scheduledTimerWithTimeInterval:0.5
                                                              target:self
                                                            selector:@selector(refreshStatsOverlayTimerFired:)
                                                            userInfo:nil
                                                             repeats:YES];
}

- (void)stopStatsRefreshTimer {
    [self.statsRefreshTimer invalidate];
    self.statsRefreshTimer = nil;
}

- (void)refreshStatsOverlayTimerFired:(NSTimer *)timer {
    (void)timer;
    [self updateStatsOverlay];
}

- (void)toggleStatsOverlay {
    if (_streamEnded) return;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (self->_streamEnded) return;
        if (self.statsOverlay) {
            [self.statsOverlay removeFromSuperview];
            self.statsOverlay = nil;
            [self stopStatsRefreshTimer];
            [self.streamView takeFocus];
            NSLog(@"[StreamVC] Stats overlay hidden via CMD+N");
            return;
        }

        OPNStatsOverlayView *overlay = [[OPNStatsOverlayView alloc] initWithFrame:[self statsOverlayFrame]];
        self.statsOverlay = overlay;
        [self.view addSubview:overlay positioned:NSWindowAbove relativeTo:nil];
        [self updateStatsOverlay];
        [self startStatsRefreshTimer];
        [self.streamView takeFocus];
        NSLog(@"[StreamVC] Stats overlay shown via CMD+N");
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 250 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
            [self updateStatsOverlay];
        });
    });
}

- (void)toggleShortcutLegendOverlay {
    if (_streamEnded) return;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (self->_streamEnded) return;
        if (self.shortcutLegendOverlay) {
            [self.shortcutLegendOverlay removeFromSuperview];
            self.shortcutLegendOverlay = nil;
            [self.streamView takeFocus];
            NSLog(@"[StreamVC] Shortcut legend hidden via CMD+H");
            return;
        }

        OPNShortcutLegendView *overlay = [[OPNShortcutLegendView alloc] initWithFrame:[self shortcutLegendFrame]];
        self.shortcutLegendOverlay = overlay;
        [self.view addSubview:overlay positioned:NSWindowAbove relativeTo:nil];
        [self.streamView takeFocus];
        NSLog(@"[StreamVC] Shortcut legend shown via CMD+H");
    });
}

- (void)removeQuitShortcutMonitor {
    if (!_quitKeyMonitor) return;
    [NSEvent removeMonitor:_quitKeyMonitor];
    _quitKeyMonitor = nil;
}

- (void)dismissQuitGameOverlayAndRefocus:(BOOL)refocus {
    if (!self.quitOverlay) return;
    [self.quitOverlay removeFromSuperview];
    self.quitOverlay = nil;
    if (refocus && !_streamEnded) {
        [self.streamView takeFocus];
    }
}

- (void)requestQuitGameConfirmation {
    if (_streamEnded) return;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (self->_streamEnded) return;
        if (self.quitOverlay) {
            [self.quitOverlay removeFromSuperview];
            [self.view addSubview:self.quitOverlay positioned:NSWindowAbove relativeTo:nil];
            [self.view.window makeFirstResponder:self.quitOverlay];
            return;
        }

        [self.streamView releasePointerLock];

        OPNQuitGameOverlayView *overlay = [[OPNQuitGameOverlayView alloc] initWithFrame:self.view.bounds];
        overlay.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        __weak __typeof__(self) weakSelf = self;
        overlay.onCancel = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf dismissQuitGameOverlayAndRefocus:YES];
        };
        overlay.onQuit = ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf dismissQuitGameOverlayAndRefocus:NO];
            [strongSelf endStreamFromUserQuit];
        };
        self.quitOverlay = overlay;
        [self.view addSubview:overlay positioned:NSWindowAbove relativeTo:nil];
        [self.view.window makeFirstResponder:overlay];
    });
}

- (void)ensureLoadingViewWithMessage:(NSString *)message {
    if (self.loadingView) {
        [self.loadingView startAnimating];
        self.loadingView.message = message;
        self.statusLabel = self.loadingView.messageLabel;
        [self configureLoadingViewAdHandler];
        [self.view addSubview:self.loadingView positioned:NSWindowAbove relativeTo:nil];
        return;
    }

    self.loadingView = [[OPNLoadingView alloc] initWithFrame:[self loadingViewFrame]
                                                     message:message];
    self.loadingView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [self.loadingView setSteps:@[@"Allocate cloud session",
                                 @"Connect to game server",
                                 @"Receive stream offer",
                                 @"Negotiate WebRTC",
                                 @"Start video pipeline",
                                 @"Connected"]
                   currentStepIndex:-1];
    self.statusLabel = self.loadingView.messageLabel;
    [self configureLoadingViewAdHandler];
    [self.view addSubview:self.loadingView positioned:NSWindowAbove relativeTo:nil];
}

- (void)resetTransportForRecovery {
    [self cancelRemoteIceGraceTimer];
    if (self.streamView) {
        [self.streamView setStreamSession:nullptr];
        [self.streamView detachFromPipeline];
    }
    if (_signaling) {
        OPNReleaseSignalingClientAfterCallbacks(_signaling);
        _signaling = nullptr;
    }
    if (_session) {
        OPNReleaseStreamSessionAfterCallbacks(_session);
        _session = nullptr;
    }
    OPN::StreamWebRTCBackend backend = OPN::ResolveStreamWebRTCBackend();
    _session = OPN::CreateStreamSession(backend).release();
    _webRTCBackendName = OPN::StreamWebRTCBackendName(backend);
    _remoteIceReceived = NO;
    if (self.streamView) {
        [self.streamView setStreamSession:_session];
    }
}

- (void)requestRemoteStopForActiveSession {
    if (_remoteStopRequested) return;
    if (!_hasActiveSessionInfo) {
        NSLog(@"[StreamVC] No active session info available for remote stop");
        return;
    }

    OPN::SessionInfo sessionInfo = _activeSessionInfo;
    if (sessionInfo.sessionId.empty() || sessionInfo.serverIp.empty()) {
        NSLog(@"[StreamVC] Cannot stop remote session; sessionId=%s serverIp=%s",
              sessionInfo.sessionId.empty() ? "(empty)" : sessionInfo.sessionId.c_str(),
              sessionInfo.serverIp.empty() ? "(empty)" : sessionInfo.serverIp.c_str());
        return;
    }

    _remoteStopRequested = YES;
    NSLog(@"[StreamVC] Requesting remote session stop sessionId=%s serverIp=%s",
          sessionInfo.sessionId.c_str(),
          sessionInfo.serverIp.c_str());
    OPN::SessionManager::Shared().StopSession(sessionInfo.sessionId,
                                              sessionInfo.serverIp,
                                              [sessionInfo](bool ok, const std::string &error) {
        if (ok) {
            NSLog(@"[StreamVC] Remote session stop succeeded sessionId=%s", sessionInfo.sessionId.c_str());
        } else {
            NSLog(@"[StreamVC] Remote session stop failed sessionId=%s error=%s",
                  sessionInfo.sessionId.c_str(),
                  error.empty() ? "unknown" : error.c_str());
        }
    });
}

- (void)endStreamFromUserQuit {
    if (_streamEnded) return;
    [self requestRemoteStopForActiveSession];
    [self endStreamWithSuccess:YES errorMessage:""];
}

- (void)shutdownForApplicationTermination {
    if (![NSThread isMainThread]) {
        __weak OPNStreamViewController *weakSelf = self;
        dispatch_async(dispatch_get_main_queue(), ^{
            OPNStreamViewController *strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf shutdownForApplicationTermination];
        });
        return;
    }

    if (_streamEnded) return;
    _streamEnded = YES;
    [self requestRemoteStopForActiveSession];
    [self finishLaunchMeasurementWithSuccess:YES reason:@"application terminating"];
    [self cleanup];
}

- (void)cancelRemoteIceGraceTimer {
    if (!_remoteIceGraceTimer) return;
    dispatch_source_t timer = (__bridge_transfer dispatch_source_t)_remoteIceGraceTimer;
    _remoteIceGraceTimer = nullptr;
    dispatch_source_cancel(timer);
}

- (void)startRemoteIceGraceTimerForLaunchGeneration:(NSUInteger)launchGeneration {
    [self cancelRemoteIceGraceTimer];
    if (!_recovering || !_connectedOnce || _streamEnded) return;

    dispatch_source_t timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    if (!timer) return;

    void *timerToken = (__bridge_retained void *)timer;
    _remoteIceGraceTimer = timerToken;

    __weak __typeof__(self) weakSelf = self;
    dispatch_source_set_timer(timer,
                              dispatch_time(DISPATCH_TIME_NOW, (int64_t)(OPNSignalingRemoteIceGraceInterval * NSEC_PER_SEC)),
                              DISPATCH_TIME_FOREVER,
                              0);
    dispatch_source_set_event_handler(timer, ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf) return;
        if (strongSelf->_remoteIceGraceTimer != timerToken) return;

        dispatch_source_t firedTimer = (__bridge_transfer dispatch_source_t)strongSelf->_remoteIceGraceTimer;
        strongSelf->_remoteIceGraceTimer = nullptr;
        dispatch_source_cancel(firedTimer);

        if (strongSelf->_streamEnded || strongSelf->_launchGeneration != launchGeneration || strongSelf->_remoteIceReceived) return;

        std::string error = "No remote ICE received after offer";
        NSLog(@"[StreamVC] No remote ICE received within %.0fms after offer; forcing targeted recovery",
              OPNSignalingRemoteIceGraceInterval * 1000.0);
        if ([strongSelf beginAutomaticRecoveryForError:error]) return;
        if (strongSelf->_connectedOnce) {
            [strongSelf endStreamWithSuccess:NO errorMessage:error];
        }
    });
    dispatch_resume(timer);
}

- (void)scheduleRecoveryAttemptResetForLaunchGeneration:(NSUInteger)launchGeneration {
    NSUInteger resetGeneration = ++_stableResetGeneration;
    __weak __typeof__(self) weakSelf = self;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(OPNStableRecoveryResetInterval * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf || strongSelf->_streamEnded) return;
        if (strongSelf->_stableResetGeneration != resetGeneration) return;
        if (strongSelf->_launchGeneration != launchGeneration) return;
        if (strongSelf->_recovering || !strongSelf->_connectedOnce) return;
        if (strongSelf->_recoveryAttempt > 0) {
            NSLog(@"[StreamVC] Automatic recovery budget reset after stable connection");
        }
        strongSelf->_recoveryAttempt = 0;
    });
}

- (BOOL)beginAutomaticRecoveryForError:(const std::string &)error {
    if (_streamEnded || !OPNStreamErrorIsRecoverable(error)) return NO;
    if (!_connectedOnce) {
        NSLog(@"[StreamVC] Skipping automatic recovery before confirmed stream connection: %s", error.c_str());
        return NO;
    }
    if (_recoveryAttempt >= OPNMaxAutomaticRecoveryAttempts) {
        NSLog(@"[StreamVC] Automatic recovery exhausted after %ld attempts for error: %s",
              (long)_recoveryAttempt, error.c_str());
        return NO;
    }

    _recoveryAttempt++;
    _recovering = YES;
    _stableResetGeneration++;
    NSUInteger recoveryGeneration = ++_launchGeneration;
    NSTimeInterval delay = OPNRecoveryDelayForAttempt(_recoveryAttempt);
    NSString *message = [NSString stringWithFormat:@"Connection interrupted. Reconnecting (%ld/%ld)...",
                         (long)_recoveryAttempt,
                         (long)OPNMaxAutomaticRecoveryAttempts];
    NSLog(@"[StreamVC] Starting automatic recovery attempt %ld/%ld in %.2fs after error: %s",
          (long)_recoveryAttempt,
          (long)OPNMaxAutomaticRecoveryAttempts,
          delay,
          error.c_str());

    [self finishLaunchMeasurementWithSuccess:NO reason:@"recovering"];
    [self ensureLoadingViewWithMessage:message];
    [self.streamView stopRecordingIfNeeded];
    [self resetTransportForRecovery];
    [self setStatus:message];

    __weak __typeof__(self) weakSelf = self;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delay * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        __typeof__(self) strongSelf = weakSelf;
        if (!strongSelf || strongSelf->_streamEnded) return;
        if (strongSelf->_launchGeneration != recoveryGeneration || !strongSelf->_recovering) return;
        [strongSelf startStreamLaunchFlow];
    });
    return YES;
}

- (void)connectWithSessionInfo:(const OPN::SessionInfo &)sessionInfo
                       settings:(const OPN::StreamSettings &)settings
               launchGeneration:(NSUInteger)launchGeneration {
    if (![NSThread isMainThread]) {
        OPN::SessionInfo sessionInfoCopy = sessionInfo;
        OPN::StreamSettings settingsCopy = settings;
        __weak __typeof__(self) weakSelf = self;
        dispatch_async(dispatch_get_main_queue(), ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf connectWithSessionInfo:sessionInfoCopy settings:settingsCopy launchGeneration:launchGeneration];
        });
        return;
    }

    if (_streamEnded || _launchGeneration != launchGeneration) return;

    [self setLaunchStep:1 message:@"Connecting to game server..."];

    OPN::SessionInfo activeSessionInfo = sessionInfo;
    OPN::StreamSettings negotiatedSettings = OPNSettingsWithNegotiatedProfile(settings, activeSessionInfo);
    if (negotiatedSettings.resolution != settings.resolution
        || negotiatedSettings.fps != settings.fps
        || negotiatedSettings.codec != settings.codec
        || negotiatedSettings.colorQuality != settings.colorQuality) {
        NSLog(@"[StreamVC] Using finalized stream profile %s@%dfps codec=%s color=%s (requested %s@%dfps codec=%s color=%s)",
              negotiatedSettings.resolution.c_str(),
              negotiatedSettings.fps,
              negotiatedSettings.codec.c_str(),
              negotiatedSettings.colorQuality.c_str(),
              settings.resolution.c_str(),
              settings.fps,
              settings.codec.c_str(),
              settings.colorQuality.c_str());
    }
    _activeSessionInfo = activeSessionInfo;
    _hasActiveSessionInfo = YES;

    _signaling = new OPN::SignalingClient(
        activeSessionInfo.signalingServer, activeSessionInfo.sessionId, activeSessionInfo.signalingUrl
    );
    _signaling->SetPeerResolution(negotiatedSettings.resolution);

    NSLog(@"[StreamVC] Signaling client created, server=%s, url=%s", activeSessionInfo.signalingServer.c_str(), activeSessionInfo.signalingUrl.c_str());

    __weak __typeof__(self) weakSelf = self;
    _signaling->OnOffer([weakSelf, activeSessionInfo, negotiatedSettings, launchGeneration](const std::string &sdp) {
        __typeof__(self) s = weakSelf;
        if (!s || s->_streamEnded || s->_launchGeneration != launchGeneration) return;
        if (!s->_session) {
            [s endStreamWithSuccess:NO errorMessage:"libwebrtc stream session is unavailable"];
            return;
        }
        std::string serverIceUfrag = ExtractIceUfragFromOffer(sdp);
        s->_remoteIceReceived = NO;
        [s startRemoteIceGraceTimerForLaunchGeneration:launchGeneration];

        [s setLaunchStep:3 message:@"Negotiating stream..."];
        s->_session->SetNativeWindow((__bridge void *)[s.streamView nativeVideoView]);

        s->_session->OnAnswerReady([weakSelf, activeSessionInfo, serverIceUfrag, launchGeneration](const OPN::SendAnswerRequest &answer) {
            __typeof__(self) s2 = weakSelf;
            if (!s2 || !s2->_signaling || s2->_streamEnded || s2->_launchGeneration != launchGeneration) return;
            NSLog(@"[StreamVC] Sending WebRTC answer (sdp=%zu, nvstSdp=%zu)",
                  answer.sdp.size(), answer.nvstSdp.size());
            s2->_signaling->SendAnswer(answer);
            dispatch_async(dispatch_get_main_queue(), ^{
                __typeof__(self) s3 = weakSelf;
                if (!s3 || !s3->_session || s3->_streamEnded || s3->_launchGeneration != launchGeneration) return;
                InjectManualIceCandidate(s3->_session, activeSessionInfo, serverIceUfrag);
            });
        });

        s->_session->OnIceCandidateReady([weakSelf, launchGeneration](const OPN::IceCandidatePayload &candidate) {
            __typeof__(self) s2 = weakSelf;
            if (!s2 || !s2->_signaling || s2->_streamEnded || s2->_launchGeneration != launchGeneration) return;
            s2->_signaling->SendIceCandidate(candidate);
        });

        [s setLaunchStep:4 message:@"Starting video pipeline..."];
        s->_session->Start(activeSessionInfo, sdp, negotiatedSettings, [weakSelf, activeSessionInfo, negotiatedSettings, launchGeneration](bool connected, const std::string &streamError) {
            __typeof__(self) s2 = weakSelf;
            if (!s2 || s2->_streamEnded || s2->_launchGeneration != launchGeneration) return;
            std::string streamErrorCopy = streamError;
            dispatch_async(dispatch_get_main_queue(), ^{
                if (!s2 || s2->_streamEnded || s2->_launchGeneration != launchGeneration) return;
                if (connected) {
                    [s2 cancelRemoteIceGraceTimer];
                    s2->_connectedOnce = YES;
                    s2->_recovering = NO;
                    [s2 setLaunchStep:5 message:@"Connected!"];
                    [s2 finishLaunchMeasurementWithSuccess:YES reason:@"connected"];
                    [s2.streamView setStreamSession:s2->_session];
                    [s2.streamView takeFocus];
                    [s2 showConnectedToastWithResolution:negotiatedSettings.resolution fps:negotiatedSettings.fps bitrate:negotiatedSettings.maxBitrateMbps codec:negotiatedSettings.codec];
                    [s2 updateStatsOverlay];
                    [s2 scheduleRecoveryAttemptResetForLaunchGeneration:launchGeneration];
                    __weak __typeof__(s2) weakConnectedSelf = s2;
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 700 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
                        __typeof__(s2) connectedSelf = weakConnectedSelf;
                        if (!connectedSelf || connectedSelf->_streamEnded || connectedSelf->_launchGeneration != launchGeneration) return;
                        [connectedSelf.loadingView clearAdPresentation];
                        [connectedSelf.loadingView stopAnimating];
                        [connectedSelf.loadingView removeFromSuperview];
                        connectedSelf.loadingView = nil;
                        connectedSelf.statusLabel = nil;
                    });
                } else {
                    if (streamErrorCopy.empty()) {
                        [s2 endStreamWithSuccess:YES errorMessage:""];
                    } else {
                        [s2 setStatus:[NSString stringWithFormat:@"Stream error: %s", streamErrorCopy.c_str()]];
                        if ([s2 beginAutomaticRecoveryForError:streamErrorCopy]) return;
                        [s2 endStreamWithSuccess:NO errorMessage:streamErrorCopy];
                    }
                }
            });
        });

        s->_signaling->OnIceCandidate([weakSelf, launchGeneration](const OPN::IceCandidatePayload &candidate) {
            __typeof__(self) s2 = weakSelf;
            if (!s2 || !s2->_session || s2->_streamEnded || s2->_launchGeneration != launchGeneration) return;
            s2->_remoteIceReceived = YES;
            [s2 cancelRemoteIceGraceTimer];
            s2->_session->AddRemoteIceCandidate(candidate);
        });
    });

    _signaling->Connect([weakSelf, launchGeneration](bool ok, const std::string &err) {
        __typeof__(self) s = weakSelf;
        if (!s) {
            NSLog(@"[StreamVC] Signaling Connect callback: self is nil");
            return;
        }
        if (s->_launchGeneration != launchGeneration) {
            NSLog(@"[StreamVC] Signaling Connect callback ignored for stale generation %lu", (unsigned long)launchGeneration);
            return;
        }
        NSLog(@"[StreamVC] Signaling Connect result: ok=%d, error=%s", ok, err.c_str());
        if (ok) {
            [s setLaunchStep:2 message:@"Waiting for stream offer..."];
        }
        if (!ok) {
            std::string errCopy = err;
            dispatch_async(dispatch_get_main_queue(), ^{
                if (!s || s->_streamEnded || s->_launchGeneration != launchGeneration) return;
                [s setStatus:[NSString stringWithFormat:@"Signaling error: %s", errCopy.c_str()]];
                if ([s beginAutomaticRecoveryForError:errCopy]) return;
                [s endStreamWithSuccess:NO errorMessage:errCopy];
            });
        }
    });
}

- (void)startStreamLaunchFlow {
    NSUInteger launchGeneration = ++_launchGeneration;
    BOOL recoveringLaunch = _recovering;
    NSLog(@"[StreamVC] Starting stream launch flow for game: %s (appId=%s, recovery=%d attempt=%ld/%ld)",
          _gameTitle.c_str(),
          _appId.c_str(),
          recoveringLaunch,
          (long)_recoveryAttempt,
          (long)OPNMaxAutomaticRecoveryAttempts);
    NSString *launchMessage = recoveringLaunch ? @"Reconnecting to session..." : @"Allocating session...";
    [self ensureLoadingViewWithMessage:launchMessage];
    [self setLaunchStep:0 message:launchMessage];

    if (!_session) {
        std::string error = "libwebrtc stream session is unavailable";
        [self setStatus:@"libwebrtc is unavailable in this build."];
        [self endStreamWithSuccess:NO errorMessage:error];
        return;
    }

    if (_appId.empty()) {
        NSLog(@"[StreamVC] ERROR: appId is empty!");
        [self endStreamWithSuccess:NO errorMessage:"Invalid game ID"];
        return;
    }

    OPNDisplayStreamProfile displayProfile = ResolveDisplayStreamProfile(self.view.window);
    OPN::StreamPreferenceProfile streamProfile = OPN::LoadStreamPreferenceProfile();
    OPN::StreamResolutionOption effectiveResolution = streamProfile.enablePowerSaver
        ? OPNPowerSaverResolution(streamProfile)
        : streamProfile.resolution;
    OPN::StreamSettings settings;
    settings.resolution = effectiveResolution.Value();
    settings.fps = streamProfile.enablePowerSaver ? std::min(streamProfile.fps, 30) : streamProfile.fps;
    settings.codec = OPNEffectiveStreamCodec(streamProfile, OPN::ResolveStreamWebRTCBackend());
    settings.colorQuality = streamProfile.colorQuality.value.empty() ? "8bit_420" : streamProfile.colorQuality.value;
    settings.maxBitrateMbps = OPNEffectiveMaxBitrateMbps(streamProfile);
    settings.enableL4S = streamProfile.enableL4S;
    settings.microphoneMode = streamProfile.microphoneMode;
    settings.microphoneDeviceId = streamProfile.microphoneDeviceId;
    settings.microphonePushToTalkKeyCode = streamProfile.microphonePushToTalkKeyCode;
    settings.microphonePushToTalkModifierMask = streamProfile.microphonePushToTalkModifierMask;
    settings.gameVolume = streamProfile.gameVolume;
    settings.microphoneVolume = streamProfile.microphoneVolume;
    settings.accountLinked = _accountLinked;
    settings.selectedStore = _selectedStore;
    [self.streamView setVideoAspectRatio:(CGFloat)streamProfile.AspectRatio()];
    [self.streamView setSuppressInputWhenWindowInactive:streamProfile.suppressInputWhenInactive ? YES : NO];
    [self.streamView setMaxBitrateMbps:settings.maxBitrateMbps];
    [self.streamView setMicrophoneMode:settings.microphoneMode
                     pushToTalkKeyCode:(uint16_t)settings.microphonePushToTalkKeyCode
                          modifierMask:(uint16_t)settings.microphonePushToTalkModifierMask];

    if (settings.microphoneMode != "disabled") {
        AVAuthorizationStatus microphoneStatus = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
        if (microphoneStatus == AVAuthorizationStatusDenied || microphoneStatus == AVAuthorizationStatusRestricted) {
            NSLog(@"[StreamVC] Microphone permission denied or restricted; cannot start mic-enabled stream");
            [self setStatus:@"Microphone permission is disabled. Enable it in macOS Settings > Privacy & Security > Microphone."];
            [self endStreamWithSuccess:NO errorMessage:"Microphone permission denied"];
            return;
        }
        if (microphoneStatus == AVAuthorizationStatusNotDetermined) {
            NSUInteger permissionGeneration = launchGeneration;
            [self setStatus:@"Requesting microphone permission..."];
            NSLog(@"[StreamVC] Requesting macOS microphone permission");
            __weak __typeof__(self) permissionWeakSelf = self;
            [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    __typeof__(self) strongSelf = permissionWeakSelf;
                    if (!strongSelf || strongSelf->_streamEnded) return;
                    if (strongSelf->_launchGeneration != permissionGeneration) return;
                    if (!granted) {
                        NSLog(@"[StreamVC] macOS microphone permission request denied");
                        [strongSelf setStatus:@"Microphone permission was denied. Enable it in macOS Settings > Privacy & Security > Microphone."];
                        [strongSelf endStreamWithSuccess:NO errorMessage:"Microphone permission denied"];
                        return;
                    }
                    NSLog(@"[StreamVC] macOS microphone permission granted; restarting launch flow");
                    [strongSelf startStreamLaunchFlow];
                });
            }];
            return;
        }
    }

    self.launchStartTime = CACurrentMediaTime();
    self.launchSignpostId = os_signpost_id_generate(OPNStreamPerformanceLog());
    self.launchSignpostActive = YES;
    os_signpost_interval_begin(OPNStreamPerformanceLog(),
                               self.launchSignpostId,
                               "StreamLaunch",
                               "appId=%{public}s resolution=%{public}s fps=%d bitrate=%d codec=%{public}s powerSaver=%{public}s",
                               _appId.c_str(),
                               settings.resolution.c_str(),
                               settings.fps,
                               settings.maxBitrateMbps,
                               settings.codec.c_str(),
                               streamProfile.enablePowerSaver ? "on" : "off");

    NSLog(@"[StreamVC] Selected stream profile display=%dx%d stream=%s fps=%d bitrate=%dMbps codec=%s aspect=%s %.4f l4s=%s powerSaver=%s requested=%s@%dfps/%dMbps/%s",
          displayProfile.displayWidth,
          displayProfile.displayHeight,
          settings.resolution.c_str(),
          settings.fps,
          settings.maxBitrateMbps,
          settings.codec.c_str(),
          streamProfile.aspect.label.c_str(),
          streamProfile.AspectRatio(),
          settings.enableL4S ? "on" : "off",
          streamProfile.enablePowerSaver ? "on" : "off",
          streamProfile.resolution.Value().c_str(),
          streamProfile.fps,
          streamProfile.maxBitrateMbps,
          streamProfile.codec.value.c_str());

    __weak __typeof__(self) weakSelf = self;

    if (_resumeExistingSession) {
        NSLog(@"[StreamVC] Claiming active session for silent resume: sessionId=%s", _resumeSessionId.c_str());
        OPN::SessionManager::Shared().SetAccessToken(_apiToken);
        OPN::SessionManager::Shared().SetStreamingBaseUrl(OPN::LoadSelectedStreamingBaseUrl());
        [self setLaunchStep:0 message:@"Resuming active session..."];
        OPN::SessionManager::Shared().ClaimSession(_resumeSessionId, _resumeServer, _appId, settings, recoveringLaunch,
            [weakSelf, settings, launchGeneration](bool success, const OPN::SessionInfo &info, const std::string &error) {
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf || strongSelf->_streamEnded || strongSelf->_launchGeneration != launchGeneration) return;
                NSLog(@"[StreamVC] Active session claim result: success=%d", success);
                if (!success) {
                    NSString *errMsg = [NSString stringWithFormat:@"Resume failed: %s", error.c_str()];
                    [strongSelf setStatus:errMsg];
                    if ([strongSelf beginAutomaticRecoveryForError:error]) return;
                    [strongSelf endStreamWithSuccess:NO errorMessage:error];
                    return;
                }
                [strongSelf connectWithSessionInfo:info settings:settings launchGeneration:launchGeneration];
            });
        return;
    }

    NSLog(@"[StreamVC] Calling GameService::LaunchGame...");
    OPN::GameService::Shared().SetAccessToken(_apiToken);
    OPN::GameService::Shared().SetStreamingBaseUrl(OPN::LoadSelectedStreamingBaseUrl());

    OPN::GameService::Shared().LaunchGame(_appId, _gameTitle, settings, recoveringLaunch,
        [weakSelf, launchGeneration](const std::string &message, const OPN::SessionInfo &progressSession) {
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || strongSelf->_streamEnded || strongSelf->_launchGeneration != launchGeneration) return;
            NSString *statusMessage = [NSString stringWithUTF8String:message.c_str()];
            [strongSelf setLaunchStep:0 message:statusMessage ?: @"Waiting for session cleanup..."];
            if (!progressSession.sessionId.empty()) {
                [strongSelf updateLaunchAdState:progressSession];
            }
        },
        [weakSelf, settings, launchGeneration](bool success, const OPN::SessionInfo &info, const std::string &, const std::string &error) {
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) {
                NSLog(@"[StreamVC] LaunchGame callback: self is nil");
                return;
            }
            if (strongSelf->_launchGeneration != launchGeneration) {
                NSLog(@"[StreamVC] LaunchGame callback ignored for stale generation %lu", (unsigned long)launchGeneration);
                return;
            }
            if (strongSelf->_streamEnded) {
                NSLog(@"[StreamVC] LaunchGame callback: stream ended");
                return;
            }

            NSLog(@"[StreamVC] LaunchGame result: success=%d", success);
            if (!success) {
                NSString *errMsg = [NSString stringWithFormat:@"Session failed: %s", error.c_str()];
                NSLog(@"[StreamVC] %@", errMsg);
                [strongSelf setStatus:errMsg];
                if ([strongSelf beginAutomaticRecoveryForError:error]) return;
                [strongSelf endStreamWithSuccess:NO errorMessage:error];
                return;
            }
            [strongSelf connectWithSessionInfo:info settings:settings launchGeneration:launchGeneration];
        });
}

- (void)endStreamWithSuccess:(BOOL)success errorMessage:(const std::string &)errorMessage {
    if (![NSThread isMainThread]) {
        std::string errorMessageCopy = errorMessage;
        __weak OPNStreamViewController *weakSelf = self;
        dispatch_async(dispatch_get_main_queue(), ^{
            OPNStreamViewController *strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf endStreamWithSuccess:success errorMessage:errorMessageCopy];
        });
        return;
    }

    if (_streamEnded) return;
    _streamEnded = YES;
    NSString *reason = success
        ? @"ended"
        : (errorMessage.empty() ? @"failed" : [NSString stringWithUTF8String:errorMessage.c_str()]);
    [self finishLaunchMeasurementWithSuccess:success reason:reason];
    if (!success) {
        NSString *message = reason.length > 0 ? reason : @"Stream failed";
        OPN::AppendLogEvent([NSString stringWithFormat:@"[StreamVC] Stream ending with error: %@", message]);
    }
    [self cleanup];

    if (self.onStreamEnd) {
        self.onStreamEnd(success, errorMessage);
    }
}

- (void)cleanup {
    if (![NSThread isMainThread]) {
        __weak OPNStreamViewController *weakSelf = self;
        dispatch_async(dispatch_get_main_queue(), ^{
            OPNStreamViewController *strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf cleanup];
        });
        return;
    }

    _launchGeneration++;
    _stableResetGeneration++;
    _recovering = NO;
    [self cancelRemoteIceGraceTimer];
    [self removeQuitShortcutMonitor];
    [self dismissQuitGameOverlayAndRefocus:NO];
    [self stopStatsRefreshTimer];
    [self.streamView stopRecordingIfNeeded];
    if (self.statsOverlay) {
        [self.statsOverlay removeFromSuperview];
        self.statsOverlay = nil;
    }
    if (self.shortcutLegendOverlay) {
        [self.shortcutLegendOverlay removeFromSuperview];
        self.shortcutLegendOverlay = nil;
    }
    [_connectedToast removeFromSuperview];
    _connectedToast = nil;
    if (self.streamView) {
        [self.streamView setStreamSession:nullptr];
        [self.streamView detachFromPipeline];
    }
    if (self.loadingView) {
        [self.loadingView stopAnimating];
        [self.loadingView removeFromSuperview];
        self.loadingView = nil;
        self.statusLabel = nil;
    }
    if (_signaling) {
        OPNReleaseSignalingClientAfterCallbacks(_signaling);
        _signaling = nullptr;
    }
    if (_session) {
        OPNReleaseStreamSessionAfterCallbacks(_session);
        _session = nullptr;
    }
}

- (void)dealloc {
    [self cleanup];
}

@end
