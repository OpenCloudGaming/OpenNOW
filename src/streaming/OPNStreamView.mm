#include "OPNStreamView.h"
#include "OPNStreamSession.h"
#include "OPNInputProtocol.h"
#include "OPNStreamPreferences.h"
#include "../common/OPNUIHelpers.h"
#import "OPNStreamRecordingManager.h"

#import <GameController/GameController.h>
#import <ApplicationServices/ApplicationServices.h>
#import <QuartzCore/QuartzCore.h>

#include <algorithm>
#include <climits>
#include <cmath>
#include <cstring>
#include <string>

using OPN::Input::GAMEPAD_A;
using OPN::Input::GAMEPAD_B;
using OPN::Input::GAMEPAD_BACK;
using OPN::Input::GAMEPAD_DPAD_DOWN;
using OPN::Input::GAMEPAD_DPAD_LEFT;
using OPN::Input::GAMEPAD_DPAD_RIGHT;
using OPN::Input::GAMEPAD_DPAD_UP;
using OPN::Input::GAMEPAD_GUIDE;
using OPN::Input::GAMEPAD_LB;
using OPN::Input::GAMEPAD_LS;
using OPN::Input::GAMEPAD_MAX_CONTROLLERS;
using OPN::Input::GAMEPAD_RB;
using OPN::Input::GAMEPAD_RS;
using OPN::Input::GAMEPAD_START;
using OPN::Input::GAMEPAD_X;
using OPN::Input::GAMEPAD_Y;

struct OPNPadSnapshot {
    bool known = false;
    OPN::Input::GamepadState state;
};

static constexpr NSTimeInterval OPNMouseCoalescingIntervalSeconds = 0.004;
static constexpr CFTimeInterval OPNGuideButtonDebounceSeconds = 0.35;

static uint16_t OPNPushToTalkModifierFlags(NSEvent *event);

@interface OPNVideoSurfaceView : NSView
@end

@implementation OPNVideoSurfaceView

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.layer.backgroundColor = [NSColor blackColor].CGColor;
    }
    return self;
}

- (NSView *)hitTest:(NSPoint)point {
    (void)point;
    return nil;
}
@end

@interface OPNStreamView () {
    void *_attachedPipeline;
    OPN::IStreamSession *_streamSession;
    NSTimer *_gamepadTimer;
    NSTimer *_mouseFlushTimer;
    dispatch_source_t _escapeHoldTimer;
    BOOL _cursorCaptured;
    uint8_t _mouseButtonsDown;
    uint16_t _gamepadBitmap;
    BOOL _modifierDown[128];
    std::string _microphoneMode;
    uint16_t _pushToTalkKeyCode;
    uint16_t _pushToTalkModifierMask;
    BOOL _pushToTalkPrimaryKeyDown;
    BOOL _pushToTalkMicEnabled;
    BOOL _microphoneShortcutEnabled;
    BOOL _suppressInputWhenWindowInactive;
    BOOL _sidebarOpen;
    BOOL _inputSuspendedForLibraryOverlay;
    double _gameVolume;
    double _microphoneVolumeLevel;
    double _microphoneLevel;
    double _pendingMouseDx;
    double _pendingMouseDy;
    int _maxBitrateMbps;
    OPNPadSnapshot _previousPads[GAMEPAD_MAX_CONTROLLERS];
    CFTimeInterval _lastGamepadSend[GAMEPAD_MAX_CONTROLLERS];
    CFTimeInterval _lastGuideButtonDispatch;
}
@property (nonatomic, strong) OPNVideoSurfaceView *videoSurface;
@property (nonatomic, strong) NSView *microphoneActiveOverlay;
@property (nonatomic, strong) NSView *sidebarHUD;
@property (nonatomic, strong) NSTextField *sidebarMicStatusValue;
@property (nonatomic, strong) NSTextField *sidebarBitrateValue;
@property (nonatomic, strong) NSTextField *sidebarRecordingStatusValue;
@property (nonatomic, strong) NSSlider *bitrateSlider;
@property (nonatomic, strong) NSSlider *gameVolumeSlider;
@property (nonatomic, strong) NSSlider *microphoneVolumeSlider;
@property (nonatomic, strong) NSView *microphoneMeterTrack;
@property (nonatomic, strong) CALayer *microphoneMeterFill;
@property (nonatomic, strong) NSButton *recordingButton;
@property (nonatomic, strong) NSView *recentRecordingsContainer;
@property (nonatomic, strong) OPNStreamRecordingManager *recordingManager;
@property (nonatomic, copy) NSString *recordingGameTitle;
@property (nonatomic, assign) CGFloat videoAspectRatio;
@end

@implementation OPNStreamView

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        _attachedPipeline = nullptr;
        _streamSession = nullptr;
        _gamepadTimer = nil;
        _mouseFlushTimer = nil;
        _escapeHoldTimer = nil;
        _cursorCaptured = NO;
        _mouseButtonsDown = 0;
        _gamepadBitmap = 0;
        std::memset(_modifierDown, 0, sizeof(_modifierDown));
        _microphoneMode = "disabled";
        _pushToTalkKeyCode = 9;
        _pushToTalkModifierMask = 0;
        _pushToTalkPrimaryKeyDown = NO;
        _pushToTalkMicEnabled = NO;
        _microphoneShortcutEnabled = YES;
        _suppressInputWhenWindowInactive = YES;
        _sidebarOpen = NO;
        _inputSuspendedForLibraryOverlay = NO;
        OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
        _gameVolume = profile.gameVolume;
        _microphoneVolumeLevel = profile.microphoneVolume;
        _maxBitrateMbps = profile.maxBitrateMbps;
        _microphoneLevel = 0.0;
        _pendingMouseDx = 0;
        _pendingMouseDy = 0;
        _videoAspectRatio = 16.0 / 9.0;
        _recordingGameTitle = @"Stream";
        _recordingManager = [[OPNStreamRecordingManager alloc] init];
        __weak OPNStreamView *weakSelf = self;
        _recordingManager.onStateChanged = ^{
            OPNStreamView *strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf updateRecordingControls];
        };
        self.wantsLayer = YES;
        self.layer.backgroundColor = [NSColor blackColor].CGColor;
        _videoSurface = [[OPNVideoSurfaceView alloc] initWithFrame:self.bounds];
        _videoSurface.wantsLayer = YES;
        _videoSurface.layer.backgroundColor = [NSColor blackColor].CGColor;
        [self addSubview:_videoSurface];
        [self createMicrophoneActiveOverlay];
        [self createSidebarHUDWithProfile:profile];
        [self registerForControllerNotifications];
        [self registerGuideButtonHandlersForConnectedControllers];
    }
    return self;
}

static NSTextField *OPNSidebarLabel(NSString *text, CGFloat size, NSFontWeight weight, NSColor *color, NSTextAlignment alignment) {
    NSTextField *label = [[NSTextField alloc] initWithFrame:NSZeroRect];
    label.stringValue = text ?: @"";
    label.font = [NSFont systemFontOfSize:size weight:weight];
    label.textColor = color ?: NSColor.whiteColor;
    label.alignment = alignment;
    label.drawsBackground = NO;
    label.bordered = NO;
    label.editable = NO;
    label.selectable = NO;
    label.lineBreakMode = NSLineBreakByTruncatingTail;
    return label;
}

static NSColor *OPNSidebarColor(CGFloat white, CGFloat alpha) {
    return [NSColor colorWithCalibratedWhite:white alpha:alpha];
}

- (void)addSidebarRowTo:(NSView *)panel title:(NSString *)title value:(NSTextField *)value y:(CGFloat)y {
    NSTextField *label = OPNSidebarLabel(title, 11.0, NSFontWeightMedium, OPNSidebarColor(0.72, 1.0), NSTextAlignmentLeft);
    label.frame = NSMakeRect(20.0, y, 120.0, 18.0);
    value.frame = NSMakeRect(128.0, y, NSWidth(panel.frame) - 148.0, 18.0);
    [panel addSubview:label];
    [panel addSubview:value];
}

- (NSSlider *)sidebarSliderWithValue:(double)value action:(SEL)action y:(CGFloat)y panel:(NSView *)panel {
    NSSlider *slider = [[NSSlider alloc] initWithFrame:NSMakeRect(20.0, y, NSWidth(panel.frame) - 40.0, 22.0)];
    slider.minValue = 0.0;
    slider.maxValue = 100.0;
    slider.doubleValue = std::max(0.0, std::min(value, 1.0)) * 100.0;
    slider.target = self;
    slider.action = action;
    slider.continuous = YES;
    [panel addSubview:slider];
    return slider;
}

- (void)createSidebarHUDWithProfile:(const OPN::StreamPreferenceProfile &)profile {
    NSView *panel = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 332.0, 650.0)];
    panel.wantsLayer = YES;
    panel.layer.cornerRadius = 18.0;
    panel.layer.backgroundColor = [NSColor colorWithCalibratedWhite:0.03 alpha:0.88].CGColor;
    panel.layer.borderWidth = 1.0;
    panel.layer.borderColor = [NSColor colorWithCalibratedWhite:1.0 alpha:0.12].CGColor;
    panel.hidden = YES;

    NSTextField *title = OPNSidebarLabel(@"Stream Controls", 18.0, NSFontWeightSemibold, NSColor.whiteColor, NSTextAlignmentLeft);
    title.frame = NSMakeRect(20.0, 18.0, 180.0, 24.0);
    [panel addSubview:title];

    NSButton *close = [[NSButton alloc] initWithFrame:NSMakeRect(NSWidth(panel.frame) - 48.0, 14.0, 30.0, 30.0)];
    close.title = @"x";
    close.bordered = NO;
    close.target = self;
    close.action = @selector(closeSidebarHUDClicked:);
    close.contentTintColor = NSColor.whiteColor;
    [panel addSubview:close];

    self.sidebarMicStatusValue = OPNSidebarLabel(@"--", 12.0, NSFontWeightSemibold, NSColor.whiteColor, NSTextAlignmentRight);
    [self addSidebarRowTo:panel title:@"Mic" value:self.sidebarMicStatusValue y:66.0];

    self.sidebarBitrateValue = OPNSidebarLabel(@"-- Mbps", 12.0, NSFontWeightSemibold, NSColor.whiteColor, NSTextAlignmentRight);
    [self addSidebarRowTo:panel title:@"Bitrate" value:self.sidebarBitrateValue y:104.0];
    [panel addSubview:OPNSidebarLabel(@"Stream Bitrate", 12.0, NSFontWeightMedium, OPNSidebarColor(0.82, 1.0), NSTextAlignmentLeft)];
    panel.subviews.lastObject.frame = NSMakeRect(20.0, 142.0, 180.0, 18.0);
    self.bitrateSlider = [[NSSlider alloc] initWithFrame:NSMakeRect(20.0, 166.0, NSWidth(panel.frame) - 40.0, 22.0)];
    self.bitrateSlider.minValue = 15.0;
    self.bitrateSlider.maxValue = 100.0;
    self.bitrateSlider.doubleValue = profile.maxBitrateMbps;
    self.bitrateSlider.target = self;
    self.bitrateSlider.action = @selector(bitrateSliderChanged:);
    self.bitrateSlider.continuous = YES;
    [panel addSubview:self.bitrateSlider];

    NSTextField *audioTitle = OPNSidebarLabel(@"Audio", 14.0, NSFontWeightSemibold, NSColor.whiteColor, NSTextAlignmentLeft);
    audioTitle.frame = NSMakeRect(20.0, 218.0, 180.0, 20.0);
    [panel addSubview:audioTitle];
    [panel addSubview:OPNSidebarLabel(@"Game Volume", 12.0, NSFontWeightMedium, OPNSidebarColor(0.82, 1.0), NSTextAlignmentLeft)];
    panel.subviews.lastObject.frame = NSMakeRect(20.0, 254.0, 180.0, 18.0);
    self.gameVolumeSlider = [self sidebarSliderWithValue:profile.gameVolume action:@selector(gameVolumeSliderChanged:) y:278.0 panel:panel];
    [panel addSubview:OPNSidebarLabel(@"Mic Volume", 12.0, NSFontWeightMedium, OPNSidebarColor(0.82, 1.0), NSTextAlignmentLeft)];
    panel.subviews.lastObject.frame = NSMakeRect(20.0, 316.0, 180.0, 18.0);
    self.microphoneVolumeSlider = [self sidebarSliderWithValue:profile.microphoneVolume action:@selector(microphoneVolumeSliderChanged:) y:340.0 panel:panel];

    [panel addSubview:OPNSidebarLabel(@"Mic Meter", 12.0, NSFontWeightMedium, OPNSidebarColor(0.82, 1.0), NSTextAlignmentLeft)];
    panel.subviews.lastObject.frame = NSMakeRect(20.0, 394.0, 180.0, 18.0);
    NSView *meterTrack = [[NSView alloc] initWithFrame:NSMakeRect(20.0, 424.0, NSWidth(panel.frame) - 40.0, 14.0)];
    meterTrack.wantsLayer = YES;
    meterTrack.layer.cornerRadius = 7.0;
    meterTrack.layer.backgroundColor = [NSColor colorWithCalibratedWhite:1.0 alpha:0.12].CGColor;
    CALayer *meterFill = [CALayer layer];
    meterFill.frame = NSMakeRect(0.0, 0.0, 0.0, 14.0);
    meterFill.cornerRadius = 7.0;
    meterFill.backgroundColor = [NSColor colorWithCalibratedRed:0.28 green:0.88 blue:0.54 alpha:1.0].CGColor;
    [meterTrack.layer addSublayer:meterFill];
    self.microphoneMeterTrack = meterTrack;
    self.microphoneMeterFill = meterFill;
    [panel addSubview:meterTrack];

    NSTextField *recordingTitle = OPNSidebarLabel(@"Recording", 14.0, NSFontWeightSemibold, NSColor.whiteColor, NSTextAlignmentLeft);
    recordingTitle.frame = NSMakeRect(20.0, 466.0, 180.0, 20.0);
    [panel addSubview:recordingTitle];

    self.sidebarRecordingStatusValue = OPNSidebarLabel(@"Ready", 12.0, NSFontWeightMedium, OPNSidebarColor(0.82, 1.0), NSTextAlignmentLeft);
    self.sidebarRecordingStatusValue.frame = NSMakeRect(20.0, 496.0, NSWidth(panel.frame) - 40.0, 18.0);
    [panel addSubview:self.sidebarRecordingStatusValue];

    NSButton *recordingButton = [NSButton buttonWithTitle:@"Start Recording" target:self action:@selector(recordingButtonClicked:)];
    recordingButton.frame = NSMakeRect(20.0, 526.0, NSWidth(panel.frame) - 40.0, 38.0);
    recordingButton.bezelStyle = NSBezelStyleRegularSquare;
    recordingButton.bordered = NO;
    recordingButton.wantsLayer = YES;
    recordingButton.layer.cornerRadius = 12.0;
    recordingButton.layer.backgroundColor = [NSColor colorWithCalibratedRed:0.0 green:0.48 blue:1.0 alpha:1.0].CGColor;
    [panel addSubview:recordingButton];
    self.recordingButton = recordingButton;

    NSTextField *recentTitle = OPNSidebarLabel(@"Recent", 12.0, NSFontWeightMedium, OPNSidebarColor(0.82, 1.0), NSTextAlignmentLeft);
    recentTitle.frame = NSMakeRect(20.0, 584.0, 180.0, 18.0);
    [panel addSubview:recentTitle];
    self.recentRecordingsContainer = [[NSView alloc] initWithFrame:NSMakeRect(20.0, 610.0, NSWidth(panel.frame) - 40.0, 30.0)];
    [panel addSubview:self.recentRecordingsContainer];

    self.sidebarHUD = panel;
    [self addSubview:panel positioned:NSWindowAbove relativeTo:self.microphoneActiveOverlay];
    [self updateSidebarMicStatus];
    [self updateSidebarBitrateStatus];
    [self updateRecordingControls];
}

- (void)createMicrophoneActiveOverlay {
    NSView *overlay = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 46.0, 46.0)];
    overlay.wantsLayer = YES;
    overlay.layer.cornerRadius = 15.0;
    overlay.layer.backgroundColor = [NSColor colorWithCalibratedWhite:0.0 alpha:0.68].CGColor;
    overlay.alphaValue = 0.5;
    overlay.hidden = YES;

    NSImage *image = [NSImage imageWithSystemSymbolName:@"mic.fill" accessibilityDescription:@"Microphone active"];
    if (image) {
        NSImageView *icon = [[NSImageView alloc] initWithFrame:NSMakeRect(11.0, 10.0, 24.0, 26.0)];
        icon.image = image;
        icon.contentTintColor = NSColor.whiteColor;
        icon.imageScaling = NSImageScaleProportionallyUpOrDown;
        [overlay addSubview:icon];
    } else {
        NSTextField *label = [[NSTextField alloc] initWithFrame:NSMakeRect(6.0, 13.0, 34.0, 18.0)];
        label.stringValue = @"MIC";
        label.font = [NSFont systemFontOfSize:11.0 weight:NSFontWeightBold];
        label.textColor = NSColor.whiteColor;
        label.alignment = NSTextAlignmentCenter;
        label.drawsBackground = NO;
        label.bordered = NO;
        label.editable = NO;
        label.selectable = NO;
        [overlay addSubview:label];
    }

    self.microphoneActiveOverlay = overlay;
    [self addSubview:overlay positioned:NSWindowAbove relativeTo:self.videoSurface];
}

- (void)dealloc {
    [self stopRecordingIfNeeded];
    [self stopGamepadPolling];
    [self cancelEscapeHoldTimer];
    [self releaseCursorCapture];
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)setStreamSession:(OPN::IStreamSession *)session {
    OPN::IStreamSession *previousSession = _streamSession;
    if (previousSession && previousSession != session) {
        previousSession->OnVideoFrame(OPN::VideoFrameCallback{});
    }
    _streamSession = session;
    if (session) {
        session->SetGameVolume(_gameVolume);
        session->SetMicrophoneVolume(_microphoneVolumeLevel);
        session->SetMaxBitrateMbps(_maxBitrateMbps);
        __weak OPNStreamView *weakSelf = self;
        session->OnMicrophoneLevel([weakSelf](double level) {
            dispatch_async(dispatch_get_main_queue(), ^{
                OPNStreamView *strongSelf = weakSelf;
                if (!strongSelf) return;
                [strongSelf setMicrophoneLevel:level];
            });
        });
        session->OnVideoFrame([weakSelf](void *frame) {
            OPNStreamView *strongSelf = weakSelf;
            if (!strongSelf) return;
            [strongSelf.recordingManager appendWebRTCVideoFrame:frame];
        });
        [self startGamepadPolling];
        [self applyMicrophoneShortcutState];
    } else {
        [self stopGamepadPolling];
        [self stopMouseFlushTimer];
        _pendingMouseDx = 0;
        _pendingMouseDy = 0;
        _pushToTalkPrimaryKeyDown = NO;
        _pushToTalkMicEnabled = NO;
        [self setMicrophoneLevel:0.0];
        [self setMicrophoneActive:NO];
        [self cancelEscapeHoldTimer];
        [self releaseCursorCapture];
    }
}

- (void)setMaxBitrateMbps:(NSInteger)mbps {
    int clampedMbps = std::max(1, std::min((int)mbps, 250));
    _maxBitrateMbps = clampedMbps;
    if (self.bitrateSlider) self.bitrateSlider.doubleValue = clampedMbps;
    if (_streamSession) _streamSession->SetMaxBitrateMbps(clampedMbps);
    [self updateSidebarBitrateStatus];
}

- (void)setMicrophoneMode:(const std::string &)mode pushToTalkKeyCode:(uint16_t)keyCode modifierMask:(uint16_t)modifierMask {
    _microphoneMode = mode;
    _pushToTalkKeyCode = keyCode;
    _pushToTalkModifierMask = OPNPushToTalkNormalizedModifierMask(keyCode, modifierMask);
    _pushToTalkPrimaryKeyDown = NO;
    _pushToTalkMicEnabled = NO;
    _microphoneShortcutEnabled = YES;
    [self applyMicrophoneShortcutState];
}

- (void)applyMicrophoneShortcutState {
    if (_microphoneMode == "disabled") {
        _pushToTalkMicEnabled = NO;
        [self setMicrophoneActive:NO];
        [self updateSidebarMicStatus];
        return;
    }
    if (_microphoneMode == "push-to-talk") {
        [self updatePushToTalkMicWithModifierMask:OPNPushToTalkModifierFlags(NSApp.currentEvent)];
        return;
    }
    [self setMicrophoneActive:_microphoneShortcutEnabled];
}

- (void)setMicrophoneActive:(BOOL)active {
    self.microphoneActiveOverlay.hidden = !active;
    if (_streamSession) _streamSession->SetMicrophoneEnabled(active ? true : false);
    if (!active) [self setMicrophoneLevel:0.0];
    [self updateSidebarMicStatus];
}

- (BOOL)toggleMicrophoneEnabledShortcut {
    if (_microphoneMode == "disabled") {
        NSLog(@"[StreamView] Command-M ignored because microphone is disabled in settings");
        return NO;
    }
    _microphoneShortcutEnabled = !_microphoneShortcutEnabled;
    if (!_microphoneShortcutEnabled) {
        _pushToTalkMicEnabled = NO;
        [self setMicrophoneActive:NO];
    } else {
        [self applyMicrophoneShortcutState];
    }
    NSLog(@"[StreamView] Microphone shortcut toggled %s", _microphoneShortcutEnabled ? "on" : "off");
    return YES;
}

- (void)setRecordingGameTitle:(NSString *)gameTitle {
    _recordingGameTitle = [gameTitle.length > 0 ? gameTitle : @"Stream" copy];
}

- (BOOL)toggleRecordingShortcut {
    [self.recordingManager toggleRecordingForGameTitle:_recordingGameTitle window:self.window];
    [self updateRecordingControls];
    return YES;
}

- (void)stopRecordingIfNeeded {
    [self.recordingManager stopRecording];
}

- (void)attachToPipeline:(void *)pipeline {
    _attachedPipeline = pipeline;
}

- (void)detachFromPipeline {
    _attachedPipeline = nullptr;
    [self setStreamSession:nullptr];
}

- (NSView *)nativeVideoView {
    return self.videoSurface ?: self;
}

- (void)setVideoAspectRatio:(CGFloat)aspectRatio {
    if (aspectRatio <= 0.1 || !std::isfinite((double)aspectRatio)) {
        aspectRatio = 16.0 / 9.0;
    }
    _videoAspectRatio = aspectRatio;
    [self setNeedsLayout:YES];
}

- (void)layout {
    [super layout];
    CGFloat width = NSWidth(self.bounds);
    CGFloat height = NSHeight(self.bounds);
    if (width <= 0 || height <= 0) return;

    CGFloat targetAspect = self.videoAspectRatio > 0.1 ? self.videoAspectRatio : (16.0 / 9.0);
    CGFloat fittedWidth = width;
    CGFloat fittedHeight = floor(width / targetAspect);
    if (fittedHeight > height) {
        fittedHeight = height;
        fittedWidth = floor(height * targetAspect);
    }
    CGFloat x = floor((width - fittedWidth) / 2.0);
    CGFloat y = floor((height - fittedHeight) / 2.0);
    self.videoSurface.frame = NSMakeRect(x, y, fittedWidth, fittedHeight);
    CGFloat overlaySize = 46.0;
    self.microphoneActiveOverlay.frame = NSMakeRect(NSMaxX(self.videoSurface.frame) - overlaySize - 18.0,
                                                   NSMinY(self.videoSurface.frame) + 18.0,
                                                   overlaySize,
                                                   overlaySize);
    if (self.sidebarHUD) {
        CGFloat panelWidth = NSWidth(self.sidebarHUD.frame);
        CGFloat panelHeight = MIN(650.0, MAX(450.0, height - 36.0));
        self.sidebarHUD.frame = NSMakeRect(18.0, floor((height - panelHeight) / 2.0), panelWidth, panelHeight);
    }
}

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (BOOL)canBecomeKeyView {
    return YES;
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    [[self window] setAcceptsMouseMovedEvents:YES];
}

- (void)setSuppressInputWhenWindowInactive:(BOOL)suppress {
    _suppressInputWhenWindowInactive = suppress;
}

- (void)setInputSuspendedForLibraryOverlay:(BOOL)suspended {
    if (_inputSuspendedForLibraryOverlay == suspended) return;
    _inputSuspendedForLibraryOverlay = suspended;
    if (suspended) {
        [self resetInputStateAfterSuppression];
        [self releaseCursorCapture];
    } else {
        [self takeFocus];
    }
}

- (BOOL)streamWindowAcceptsInput {
    if (_inputSuspendedForLibraryOverlay) return NO;
    if (_sidebarOpen) return NO;
    if (!_suppressInputWhenWindowInactive) return YES;
    NSWindow *window = self.window;
    return NSApp.isActive && window && (window.isKeyWindow || window.isMainWindow);
}

- (void)toggleSidebarHUD {
    _sidebarOpen = !_sidebarOpen;
    self.sidebarHUD.hidden = !_sidebarOpen;
    if (_sidebarOpen) {
        [self resetInputStateAfterSuppression];
        [self releaseCursorCapture];
        [self updateSidebarMicStatus];
        [self.window makeFirstResponder:self.sidebarHUD];
    } else {
        [self takeFocus];
    }
    [self setNeedsLayout:YES];
}

- (void)closeSidebarHUDClicked:(id)sender {
    (void)sender;
    if (!_sidebarOpen) return;
    [self toggleSidebarHUD];
}

- (void)recordingButtonClicked:(id)sender {
    (void)sender;
    [self toggleRecordingShortcut];
}

- (void)gameVolumeSliderChanged:(NSSlider *)slider {
    _gameVolume = std::max(0.0, std::min(slider.doubleValue / 100.0, 1.0));
    if (_streamSession) _streamSession->SetGameVolume(_gameVolume);
    OPN::SaveStreamGameVolume(_gameVolume);
}

- (void)microphoneVolumeSliderChanged:(NSSlider *)slider {
    _microphoneVolumeLevel = std::max(0.0, std::min(slider.doubleValue / 100.0, 1.0));
    if (_streamSession) _streamSession->SetMicrophoneVolume(_microphoneVolumeLevel);
    OPN::SaveStreamMicrophoneVolume(_microphoneVolumeLevel);
}

- (void)bitrateSliderChanged:(NSSlider *)slider {
    int mbps = (int)std::lround(slider.doubleValue);
    mbps = std::max(15, std::min(mbps, 100));
    slider.doubleValue = mbps;
    [self setMaxBitrateMbps:mbps];

    const std::vector<OPN::StreamBitrateOption> &options = OPN::StreamBitrateOptions();
    int nearestIndex = 0;
    int nearestDistance = INT_MAX;
    for (size_t i = 0; i < options.size(); i++) {
        int distance = std::abs(options[i].mbps - mbps);
        if (distance < nearestDistance) {
            nearestDistance = distance;
            nearestIndex = (int)i;
        }
    }
    OPN::SaveStreamBitrateIndex(nearestIndex);
}

- (void)updateSidebarBitrateStatus {
    self.sidebarBitrateValue.stringValue = [NSString stringWithFormat:@"%d Mbps", _maxBitrateMbps];
}

- (void)updateSidebarMicStatus {
    NSString *mode = @"Disabled";
    if (_microphoneMode == "push-to-talk") {
        mode = self.microphoneActiveOverlay.hidden ? @"PTT muted" : @"PTT live";
    } else if (_microphoneMode == "voice-activity") {
        mode = self.microphoneActiveOverlay.hidden ? @"Open mic muted" : @"Open mic live";
    }
    self.sidebarMicStatusValue.stringValue = mode;
}

- (void)updateRecordingControls {
    NSString *title = @"Start Recording";
    NSColor *buttonColor = [NSColor colorWithCalibratedRed:0.0 green:0.48 blue:1.0 alpha:1.0];
    if (self.recordingManager.isRecording) {
        title = @"Stop Recording";
        buttonColor = [NSColor colorWithCalibratedRed:0.92 green:0.18 blue:0.22 alpha:1.0];
    } else if (self.recordingManager.isStarting) {
        title = @"Starting...";
        buttonColor = [NSColor colorWithCalibratedRed:0.56 green:0.42 blue:0.12 alpha:1.0];
    }
    self.recordingButton.title = title;
    self.recordingButton.layer.backgroundColor = buttonColor.CGColor;
    self.sidebarRecordingStatusValue.stringValue = self.recordingManager.statusText ?: @"Ready";
    [self rebuildRecentRecordingThumbnails];
}

- (void)rebuildRecentRecordingThumbnails {
    if (!self.recentRecordingsContainer) return;
    for (NSView *view in self.recentRecordingsContainer.subviews.copy) {
        [view removeFromSuperview];
    }
    NSArray<NSURL *> *urls = self.recordingManager.recentRecordingURLs;
    if (urls.count == 0) {
        NSTextField *empty = OPNSidebarLabel(@"No recordings yet", 11.0, NSFontWeightRegular, OPNSidebarColor(0.58, 1.0), NSTextAlignmentLeft);
        empty.frame = self.recentRecordingsContainer.bounds;
        [self.recentRecordingsContainer addSubview:empty];
        return;
    }
    CGFloat thumbWidth = 66.0;
    CGFloat gap = 8.0;
    NSUInteger count = MIN((NSUInteger)4, urls.count);
    for (NSUInteger i = 0; i < count; i++) {
        NSRect frame = NSMakeRect((thumbWidth + gap) * (CGFloat)i, 0.0, thumbWidth, 30.0);
        NSImageView *imageView = [[NSImageView alloc] initWithFrame:frame];
        imageView.wantsLayer = YES;
        imageView.layer.cornerRadius = 7.0;
        imageView.layer.masksToBounds = YES;
        imageView.layer.backgroundColor = [NSColor colorWithCalibratedWhite:1.0 alpha:0.10].CGColor;
        imageView.imageScaling = NSImageScaleAxesIndependently;
        imageView.image = [self.recordingManager thumbnailForRecordingURL:urls[i] size:frame.size];
        [self.recentRecordingsContainer addSubview:imageView];
    }
}

- (void)setMicrophoneLevel:(double)level {
    _microphoneLevel = std::max(0.0, std::min(level, 1.0));
    if (!self.microphoneMeterTrack || !self.microphoneMeterFill) return;
    CGFloat width = NSWidth(self.microphoneMeterTrack.bounds) * (CGFloat)_microphoneLevel;
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    self.microphoneMeterFill.frame = NSMakeRect(0.0, 0.0, width, NSHeight(self.microphoneMeterTrack.bounds));
    if (_microphoneLevel > 0.72) {
        self.microphoneMeterFill.backgroundColor = [NSColor colorWithCalibratedRed:1.0 green:0.48 blue:0.24 alpha:1.0].CGColor;
    } else if (_microphoneLevel > 0.45) {
        self.microphoneMeterFill.backgroundColor = [NSColor colorWithCalibratedRed:0.95 green:0.78 blue:0.28 alpha:1.0].CGColor;
    } else {
        self.microphoneMeterFill.backgroundColor = [NSColor colorWithCalibratedRed:0.28 green:0.88 blue:0.54 alpha:1.0].CGColor;
    }
    [CATransaction commit];
}

- (void)resetInputStateAfterSuppression {
    [self cancelEscapeHoldTimer];
    [self stopMouseFlushTimer];
    _pendingMouseDx = 0;
    _pendingMouseDy = 0;
    _pushToTalkPrimaryKeyDown = NO;
    if (_pushToTalkMicEnabled && _streamSession && _microphoneMode == "push-to-talk") {
        _pushToTalkMicEnabled = NO;
        [self setMicrophoneActive:NO];
    } else {
        _pushToTalkMicEnabled = NO;
    }
}

- (void)takeFocus {
    [[self window] makeFirstResponder:self];
    [[self window] setAcceptsMouseMovedEvents:YES];
}

static uint16_t OPNModifierFlags(NSEvent *event) {
    NSEventModifierFlags flags = event.modifierFlags;
    uint16_t out = 0;
    if (flags & NSEventModifierFlagShift) out |= 0x01;
    if (flags & NSEventModifierFlagControl) out |= 0x02;
    if (flags & NSEventModifierFlagOption) out |= 0x04;
    if (flags & NSEventModifierFlagCommand) out |= 0x08;
    if (flags & NSEventModifierFlagCapsLock) out |= 0x10;
    if (flags & NSEventModifierFlagNumericPad) out |= 0x20;
    return out;
}

static uint16_t OPNPushToTalkModifierFlags(NSEvent *event) {
    NSEventModifierFlags flags = event.modifierFlags;
    uint16_t out = 0;
    if (flags & NSEventModifierFlagShift) out |= 0x01;
    if (flags & NSEventModifierFlagControl) out |= 0x02;
    if (flags & NSEventModifierFlagOption) out |= 0x04;
    if (flags & NSEventModifierFlagCommand) out |= 0x08;
    if (flags & NSEventModifierFlagCapsLock) out |= 0x10;
    return out;
}

static uint16_t OPNPushToTalkModifierBitForKeyCode(uint16_t keyCode) {
    switch (keyCode) {
        case 55: return 0x08;
        case 56:
        case 60: return 0x01;
        case 57: return 0x10;
        case 58:
        case 61: return 0x04;
        case 59:
        case 62: return 0x02;
        default: return 0;
    }
}

static uint16_t OPNPushToTalkNormalizedModifierMask(uint16_t keyCode, uint16_t modifierMask) {
    uint16_t normalized = modifierMask & 0x1f;
    uint16_t keyModifierBit = OPNPushToTalkModifierBitForKeyCode(keyCode);
    if (keyModifierBit != 0) normalized |= keyModifierBit;
    return normalized;
}

static int16_t OPNClampI16(double value) {
    value = std::max(-32768.0, std::min(32767.0, std::round(value)));
    return (int16_t)value;
}

static uint8_t OPNMouseButtonForEvent(NSEvent *event) {
    switch (event.type) {
        case NSEventTypeLeftMouseDown:
        case NSEventTypeLeftMouseUp:
        case NSEventTypeLeftMouseDragged:
            return OPN::Input::MOUSE_LEFT;
        case NSEventTypeRightMouseDown:
        case NSEventTypeRightMouseUp:
        case NSEventTypeRightMouseDragged:
            return OPN::Input::MOUSE_RIGHT;
        case NSEventTypeOtherMouseDown:
        case NSEventTypeOtherMouseUp:
        case NSEventTypeOtherMouseDragged:
            if (event.buttonNumber == 2) return OPN::Input::MOUSE_MIDDLE;
            if (event.buttonNumber == 3) return OPN::Input::MOUSE_BACK;
            if (event.buttonNumber == 4) return OPN::Input::MOUSE_FORWARD;
            return (uint8_t)std::min<NSInteger>(5, std::max<NSInteger>(1, event.buttonNumber + 1));
        default:
            return 0;
    }
}

static uint8_t OPNMouseButtonMask(uint8_t button) {
    if (button == 0 || button > 7) return 0;
    return (uint8_t)(1u << (button - 1));
}

- (void)updatePushToTalkMicWithModifierMask:(uint16_t)modifierMask {
    if (!_streamSession || _microphoneMode != "push-to-talk") return;
    BOOL shouldEnable = _microphoneShortcutEnabled && _pushToTalkPrimaryKeyDown && ((modifierMask & 0x1f) == _pushToTalkModifierMask);
    if (_pushToTalkMicEnabled == shouldEnable) return;
    _pushToTalkMicEnabled = shouldEnable;
    [self setMicrophoneActive:shouldEnable];
}

- (BOOL)handlePushToTalkKeyEvent:(NSEvent *)event down:(BOOL)down {
    if (_microphoneMode != "push-to-talk" || event.keyCode != _pushToTalkKeyCode) return NO;
    if (down && event.isARepeat) return YES;

    _pushToTalkPrimaryKeyDown = down ? YES : NO;
    [self updatePushToTalkMicWithModifierMask:OPNPushToTalkModifierFlags(event)];
    return YES;
}

- (BOOL)handlePushToTalkFlagsChanged:(NSEvent *)event {
    if (_microphoneMode != "push-to-talk") return NO;

    uint16_t changedModifier = OPNPushToTalkModifierBitForKeyCode((uint16_t)event.keyCode);
    if (changedModifier == 0) return NO;

    uint16_t currentModifiers = OPNPushToTalkModifierFlags(event);
    BOOL isPrimaryKey = event.keyCode == _pushToTalkKeyCode;
    BOOL isConfiguredModifier = (_pushToTalkModifierMask & changedModifier) != 0;
    if (!isPrimaryKey && !isConfiguredModifier && !_pushToTalkMicEnabled) return NO;

    if (isPrimaryKey) {
        _pushToTalkPrimaryKeyDown = (currentModifiers & changedModifier) != 0 ? YES : NO;
    }
    [self updatePushToTalkMicWithModifierMask:currentModifiers];
    return isPrimaryKey || isConfiguredModifier || _pushToTalkMicEnabled;
}

- (void)handleKeyEvent:(NSEvent *)event {
    if (!_streamSession) return;
    if (![self streamWindowAcceptsInput]) {
        [self resetInputStateAfterSuppression];
        return;
    }
    bool down = event.type == NSEventTypeKeyDown;
    if ([self handlePushToTalkKeyEvent:event down:down]) {
        return;
    }

    if (!_streamSession->InputReady()) return;

    auto mapping = OPN::Input::MapMacKeyCode((uint16_t)event.keyCode);
    if (!mapping) {
        NSLog(@"[StreamView] No OPN key mapping for mac keyCode=%hu", (unsigned short)event.keyCode);
        return;
    }

    if (event.keyCode == 53) {
        if (down && !event.isARepeat) {
            [self startEscapeHoldTimer];
        } else if (!down) {
            [self cancelEscapeHoldTimer];
        }
    }
    _streamSession->SendKeyEvent(mapping->vk, mapping->scancode, OPNModifierFlags(event), down);
}

- (void)handleMouseEvent:(NSEvent *)event {
    if (!_streamSession || !_streamSession->InputReady()) return;
    if (![self streamWindowAcceptsInput]) {
        [self resetInputStateAfterSuppression];
        return;
    }

    switch (event.type) {
        case NSEventTypeMouseMoved:
        case NSEventTypeLeftMouseDragged:
        case NSEventTypeRightMouseDragged:
        case NSEventTypeOtherMouseDragged: {
            if (!_cursorCaptured) {
                break;
            }
            [self accumulateMouseDx:event.deltaX dy:event.deltaY];
            [self startMouseFlushTimer];
            break;
        }
        case NSEventTypeLeftMouseDown:
        case NSEventTypeRightMouseDown:
        case NSEventTypeOtherMouseDown: {
            [self takeFocus];
            if (!_cursorCaptured) {
                [self captureCursorIfNeeded];
                break;
            }
            uint8_t button = OPNMouseButtonForEvent(event);
            uint8_t mask = OPNMouseButtonMask(button);
            if (mask) _mouseButtonsDown |= mask;
            [self flushPendingMouseMove];
            _streamSession->SendMouseButton(button, true);
            break;
        }
        case NSEventTypeLeftMouseUp:
        case NSEventTypeRightMouseUp:
        case NSEventTypeOtherMouseUp: {
            if (!_cursorCaptured) {
                break;
            }
            uint8_t button = OPNMouseButtonForEvent(event);
            uint8_t mask = OPNMouseButtonMask(button);
            if (mask) _mouseButtonsDown &= (uint8_t)~mask;
            [self flushPendingMouseMove];
            _streamSession->SendMouseButton(button, false);
            break;
        }
        case NSEventTypeScrollWheel: {
            if (!_cursorCaptured) {
                break;
            }
            [self flushPendingMouseMove];
            double precise = event.hasPreciseScrollingDeltas ? event.scrollingDeltaY : event.deltaY * 120.0;
            _streamSession->SendMouseWheel(OPNClampI16(-precise));
            break;
        }
        default:
            break;
    }
}

- (void)keyDown:(NSEvent *)event {
    [self handleKeyEvent:event];
}

- (void)keyUp:(NSEvent *)event {
    [self handleKeyEvent:event];
}

- (void)mouseMoved:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)mouseDown:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)mouseUp:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)rightMouseDown:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)rightMouseUp:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)otherMouseDown:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)otherMouseUp:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)mouseDragged:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)rightMouseDragged:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)otherMouseDragged:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)scrollWheel:(NSEvent *)event {
    [self handleMouseEvent:event];
}

- (void)flagsChanged:(NSEvent *)event {
    if (!_streamSession) return;
    if (![self streamWindowAcceptsInput]) {
        [self resetInputStateAfterSuppression];
        return;
    }
    auto mapping = OPN::Input::MapMacKeyCode((uint16_t)event.keyCode);
    if (!mapping || event.keyCode >= 128) return;

    NSEventModifierFlags flags = event.modifierFlags;
    BOOL down = NO;
    switch (event.keyCode) {
        case 55:
            down = (flags & NSEventModifierFlagCommand) != 0;
            break;
        case 56:
        case 60:
            down = (flags & NSEventModifierFlagShift) != 0;
            break;
        case 57:
            down = (flags & NSEventModifierFlagCapsLock) != 0;
            break;
        case 58:
        case 61:
            down = (flags & NSEventModifierFlagOption) != 0;
            break;
        case 59:
        case 62:
            down = (flags & NSEventModifierFlagControl) != 0;
            break;
        default:
            return;
    }

    if (_modifierDown[event.keyCode] == down) return;
    _modifierDown[event.keyCode] = down;
    if ([self handlePushToTalkFlagsChanged:event]) {
        return;
    }
    if (!_streamSession->InputReady()) return;
    _streamSession->SendKeyEvent(mapping->vk, mapping->scancode, OPNModifierFlags(event), down);
}

- (void)captureCursorIfNeeded {
    if (_cursorCaptured) return;
    CGAssociateMouseAndMouseCursorPosition(false);
    CGDisplayHideCursor(kCGDirectMainDisplay);
    _cursorCaptured = YES;
    NSLog(@"[StreamView] Stream pointer locker active");
}

- (void)releasePressedMouseButtons {
    if (!_mouseButtonsDown) return;
    static const uint8_t buttons[] = {
        OPN::Input::MOUSE_LEFT,
        OPN::Input::MOUSE_MIDDLE,
        OPN::Input::MOUSE_RIGHT,
        OPN::Input::MOUSE_BACK,
        OPN::Input::MOUSE_FORWARD,
    };
    if (_streamSession && _streamSession->InputReady()) {
        for (uint8_t button : buttons) {
            uint8_t mask = OPNMouseButtonMask(button);
            if (mask && (_mouseButtonsDown & mask)) {
                _streamSession->SendMouseButton(button, false);
            }
        }
    }
    _mouseButtonsDown = 0;
}

- (void)releaseCursorCapture {
    if (!_cursorCaptured) return;
    [self releasePressedMouseButtons];
    [self stopMouseFlushTimer];
    _pendingMouseDx = 0;
    _pendingMouseDy = 0;
    CGAssociateMouseAndMouseCursorPosition(true);
    CGDisplayShowCursor(kCGDirectMainDisplay);
    _cursorCaptured = NO;
    NSLog(@"[StreamView] Stream pointer locker armed");
}

- (void)releasePointerLock {
    [self releaseCursorCapture];
}

- (void)startEscapeHoldTimer {
    if (_escapeHoldTimer) return;
    _escapeHoldTimer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    if (!_escapeHoldTimer) return;
    dispatch_source_set_timer(_escapeHoldTimer,
                              dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC),
                              DISPATCH_TIME_FOREVER,
                              50 * NSEC_PER_MSEC);
    __weak OPNStreamView *weakSelf = self;
    dispatch_source_set_event_handler(_escapeHoldTimer, ^{
        OPNStreamView *strongSelf = weakSelf;
        if (!strongSelf) return;
        [strongSelf releaseCursorCapture];
        [strongSelf cancelEscapeHoldTimer];
        NSLog(@"[StreamView] ESC held for 3s; pointer capture released");
    });
    dispatch_resume(_escapeHoldTimer);
}

- (void)cancelEscapeHoldTimer {
    if (!_escapeHoldTimer) return;
    dispatch_source_cancel(_escapeHoldTimer);
    _escapeHoldTimer = nil;
}

- (void)accumulateMouseDx:(double)dx dy:(double)dy {
    _pendingMouseDx += dx;
    _pendingMouseDy += dy;
}

- (void)startMouseFlushTimer {
    if (_mouseFlushTimer) return;
    _mouseFlushTimer = [NSTimer scheduledTimerWithTimeInterval:OPNMouseCoalescingIntervalSeconds
                                                        target:self
                                                      selector:@selector(mouseFlushTimerFired:)
                                                      userInfo:nil
                                                       repeats:NO];
}

- (void)stopMouseFlushTimer {
    [_mouseFlushTimer invalidate];
    _mouseFlushTimer = nil;
}

- (void)mouseFlushTimerFired:(NSTimer *)timer {
    if (timer != _mouseFlushTimer) return;
    _mouseFlushTimer = nil;
    [self flushPendingMouseMove];
}

- (void)flushPendingMouseMove {
    [self stopMouseFlushTimer];
    if (!_streamSession || !_streamSession->InputReady() || ![self streamWindowAcceptsInput]) {
        _pendingMouseDx = 0;
        _pendingMouseDy = 0;
        return;
    }
    if (std::fabs(_pendingMouseDx) < 0.5 && std::fabs(_pendingMouseDy) < 0.5) {
        return;
    }

    double sendDx = std::round(_pendingMouseDx);
    double sendDy = std::round(_pendingMouseDy);
    if (sendDx == 0 && sendDy == 0) {
        return;
    }
    _pendingMouseDx -= sendDx;
    _pendingMouseDy -= sendDy;
    _streamSession->SendMouseMove(OPNClampI16(sendDx), OPNClampI16(sendDy));
}

- (void)registerForControllerNotifications {
    NSNotificationCenter *center = [NSNotificationCenter defaultCenter];
    [center addObserver:self selector:@selector(controllerDidConnect:) name:GCControllerDidConnectNotification object:nil];
    [center addObserver:self selector:@selector(controllerDidDisconnect:) name:GCControllerDidDisconnectNotification object:nil];
}

- (void)controllerDidConnect:(NSNotification *)notification {
    GCController *controller = [notification.object isKindOfClass:GCController.class] ? notification.object : nil;
    if (controller) {
        [self registerGuideButtonHandlersForController:controller];
    } else {
        [self registerGuideButtonHandlersForConnectedControllers];
    }
    NSLog(@"[StreamView] GameController connected");
    [self startGamepadPolling];
}

- (void)controllerDidDisconnect:(NSNotification *)notification {
    (void)notification;
    NSLog(@"[StreamView] GameController disconnected");
    [self pollGamepads];
    if (GCController.controllers.count == 0) {
        [self stopGamepadPolling];
    }
}

- (void)startGamepadPolling {
    if (_gamepadTimer) return;
    if (!_streamSession || GCController.controllers.count == 0) return;
    _gamepadTimer = [NSTimer scheduledTimerWithTimeInterval:(1.0 / 60.0)
                                                      target:self
                                                   selector:@selector(pollGamepads)
                                                   userInfo:nil
                                                    repeats:YES];
}

- (void)stopGamepadPolling {
    [_gamepadTimer invalidate];
    _gamepadTimer = nil;
}

- (void)registerGuideButtonHandlersForConnectedControllers {
    for (GCController *controller in GCController.controllers) {
        [self registerGuideButtonHandlersForController:controller];
    }
}

- (void)registerGuideButtonHandlersForController:(GCController *)controller {
    if (!controller.extendedGamepad) return;

    GCExtendedGamepad *pad = controller.extendedGamepad;
    __weak OPNStreamView *weakSelf = self;
    if (@available(macOS 11.0, *)) {
        GCControllerButtonInput *homeButton = pad.buttonHome;
        homeButton.pressedChangedHandler = ^(GCControllerButtonInput *button, float value, BOOL pressed) {
            (void)button;
            (void)value;
            if (!pressed) return;
            dispatch_async(dispatch_get_main_queue(), ^{
                OPNStreamView *strongSelf = weakSelf;
                if (!strongSelf) return;
                [strongSelf dispatchGuideButtonPressIfNeeded];
            });
        };
    }

    if (@available(macOS 13.0, *)) {
        pad.valueDidChangeHandler = ^(GCPhysicalInputProfile *profile, GCControllerElement *element) {
            if (![element.aliases containsObject:GCInputButtonHome]) return;
            GCControllerButtonInput *homeButton = profile.buttons[GCInputButtonHome];
            BOOL pressed = homeButton ? homeButton.isPressed : NO;
            if (!pressed && [element isKindOfClass:GCControllerButtonInput.class]) {
                pressed = ((GCControllerButtonInput *)element).isPressed;
            }
            if (!pressed) return;
            dispatch_async(dispatch_get_main_queue(), ^{
                OPNStreamView *strongSelf = weakSelf;
                if (!strongSelf) return;
                [strongSelf dispatchGuideButtonPressIfNeeded];
            });
        };
    }
}

- (BOOL)dispatchGuideButtonPressIfNeeded {
    if (!_streamSession || !self.onGuideButtonPressed || _inputSuspendedForLibraryOverlay) return NO;

    CFTimeInterval now = CACurrentMediaTime();
    if (now - _lastGuideButtonDispatch < OPNGuideButtonDebounceSeconds) return YES;
    _lastGuideButtonDispatch = now;

    if (_sidebarOpen) {
        _sidebarOpen = NO;
        self.sidebarHUD.hidden = YES;
    }
    [self resetInputStateAfterSuppression];
    [self releaseCursorCapture];
    NSLog(@"[StreamView] GameController Home/Guide requested library overlay");
    self.onGuideButtonPressed();
    return YES;
}

static bool OPNStateEquals(const OPN::Input::GamepadState &a, const OPN::Input::GamepadState &b) {
    return a.connected == b.connected
        && a.buttons == b.buttons
        && a.leftTrigger == b.leftTrigger
        && a.rightTrigger == b.rightTrigger
        && a.leftStickX == b.leftStickX
        && a.leftStickY == b.leftStickY
        && a.rightStickX == b.rightStickX
        && a.rightStickY == b.rightStickY;
}

- (void)pollGamepads {
    if (!_streamSession || !_streamSession->InputReady()) return;
    if (![self streamWindowAcceptsInput]) return;

    NSArray<GCController *> *controllers = [GCController controllers];
    if (controllers.count == 0) {
        [self stopGamepadPolling];
    }
    BOOL seen[GAMEPAD_MAX_CONTROLLERS] = {NO, NO, NO, NO};

    NSUInteger count = MIN((NSUInteger)GAMEPAD_MAX_CONTROLLERS, controllers.count);
    for (NSUInteger i = 0; i < count; i++) {
        GCController *controller = controllers[i];
        GCExtendedGamepad *pad = controller.extendedGamepad;
        if (!pad) continue;
        seen[i] = YES;

        _gamepadBitmap |= (uint16_t)(1u << i);
        _gamepadBitmap |= (uint16_t)(1u << (i + 8));

        double lx = pad.leftThumbstick.xAxis.value;
        double ly = pad.leftThumbstick.yAxis.value;
        double rx = pad.rightThumbstick.xAxis.value;
        double ry = pad.rightThumbstick.yAxis.value;
        double dlx = 0, dly = 0, drx = 0, dry = 0;
        OPN::Input::ApplyRadialDeadzone(lx, ly, dlx, dly);
        OPN::Input::ApplyRadialDeadzone(rx, ry, drx, dry);

        uint16_t buttons = 0;
        if (pad.buttonA.value > 0) buttons |= GAMEPAD_A;
        if (pad.buttonB.value > 0) buttons |= GAMEPAD_B;
        if (pad.buttonX.value > 0) buttons |= GAMEPAD_X;
        if (pad.buttonY.value > 0) buttons |= GAMEPAD_Y;
        if (pad.leftShoulder.value > 0) buttons |= GAMEPAD_LB;
        if (pad.rightShoulder.value > 0) buttons |= GAMEPAD_RB;
        if (pad.dpad.up.value > 0) buttons |= GAMEPAD_DPAD_UP;
        if (pad.dpad.down.value > 0) buttons |= GAMEPAD_DPAD_DOWN;
        if (pad.dpad.left.value > 0) buttons |= GAMEPAD_DPAD_LEFT;
        if (pad.dpad.right.value > 0) buttons |= GAMEPAD_DPAD_RIGHT;
        if (@available(macOS 10.15, *)) {
            if (pad.buttonOptions.value > 0) buttons |= GAMEPAD_BACK;
            if (pad.buttonMenu.value > 0) buttons |= GAMEPAD_START;
            if (pad.leftThumbstickButton.value > 0) buttons |= GAMEPAD_LS;
            if (pad.rightThumbstickButton.value > 0) buttons |= GAMEPAD_RS;
        }
        if (@available(macOS 11.0, *)) {
            if (pad.buttonHome.value > 0) buttons |= GAMEPAD_GUIDE;
        }

        OPN::Input::GamepadState state;
        state.controllerId = (uint16_t)i;
        state.connected = true;
        state.buttons = buttons;
        state.leftTrigger = OPN::Input::NormalizeTriggerToUint8(pad.leftTrigger.value);
        state.rightTrigger = OPN::Input::NormalizeTriggerToUint8(pad.rightTrigger.value);
        state.leftStickX = OPN::Input::NormalizeAxisToInt16(dlx);
        state.leftStickY = OPN::Input::NormalizeAxisToInt16(dly);
        state.rightStickX = OPN::Input::NormalizeAxisToInt16(drx);
        state.rightStickY = OPN::Input::NormalizeAxisToInt16(dry);
        state.timestampUs = OPN::Input::TimestampUs();

        CFTimeInterval now = CACurrentMediaTime();
        uint16_t libraryShortcutMask = OpnControllerLibraryShortcutMask();
        BOOL guidePressed = (buttons & GAMEPAD_GUIDE) && (!_previousPads[i].known || !(_previousPads[i].state.buttons & GAMEPAD_GUIDE));
        BOOL shortcutPressed = libraryShortcutMask != 0
            && ((buttons & libraryShortcutMask) == libraryShortcutMask)
            && (!_previousPads[i].known || ((_previousPads[i].state.buttons & libraryShortcutMask) != libraryShortcutMask));
        if ((guidePressed || shortcutPressed) && self.onGuideButtonPressed) {
            _previousPads[i].known = true;
            _previousPads[i].state = state;
            _lastGamepadSend[i] = now;
            if ([self dispatchGuideButtonPressIfNeeded]) return;
        }
        BOOL changed = !_previousPads[i].known || !OPNStateEquals(_previousPads[i].state, state);
        BOOL keepalive = (now - _lastGamepadSend[i]) >= 1.0;
        if (changed || keepalive) {
            _streamSession->SendGamepadState(state, _gamepadBitmap);
            _previousPads[i].known = true;
            _previousPads[i].state = state;
            _lastGamepadSend[i] = now;
        }
    }

    for (NSUInteger i = 0; i < (NSUInteger)GAMEPAD_MAX_CONTROLLERS; i++) {
        if (seen[i] || !_previousPads[i].known || !_previousPads[i].state.connected) continue;
        _gamepadBitmap &= (uint16_t)~(1u << i);
        _gamepadBitmap &= (uint16_t)~(1u << (i + 8));

        OPN::Input::GamepadState state;
        state.controllerId = (uint16_t)i;
        state.connected = false;
        state.timestampUs = OPN::Input::TimestampUs();
        _streamSession->SendGamepadState(state, _gamepadBitmap);
        _previousPads[i].state = state;
        _lastGamepadSend[i] = CACurrentMediaTime();
    }
}

@end
