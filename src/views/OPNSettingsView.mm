#import "OPNSettingsView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#include "../games/OPNGameService.h"
#include "../streaming/OPNInputProtocol.h"
#include "../streaming/OPNLibWebRTCStreamSession.h"
#include "../streaming/OPNStreamBackend.h"
#include "../streaming/OPNStreamPreferences.h"
#import <GameController/GameController.h>
#include <QuartzCore/QuartzCore.h>
#include <CoreAudio/CoreAudio.h>
#include <cmath>

static const CGFloat kSettingsNavHeight = 64.0;
static const CGFloat kSettingsControllerNavHeight = 136.0;
static const CGFloat kSettingsTopInset = 72.0;
static const CGFloat kSettingsSidebarWidth = 300.0;
static const CGFloat kSettingsColumnGap = 28.0;

static NSString *OPNSettingsDisplayName(NSString *section) {
    if ([section isEqualToString:@"Stream"]) return @"Network";
    return section;
}

static NSDictionary<NSString *, NSString *> *OPNWebRTCBackendRuntimeInfo(void) {
    OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
    BOOL libWebRTCAvailable = OPN::LibWebRTCStreamSession::IsAvailable() ? YES : NO;
    NSString *active = [NSString stringWithUTF8String:OPN::StreamWebRTCBackendName(OPN::ResolveStreamWebRTCBackend()).c_str()];
    NSString *libDescription = [NSString stringWithUTF8String:OPN::LibWebRTCStreamSession::AvailabilityDescription().c_str()];
    NSString *requestedCodec = [NSString stringWithUTF8String:(profile.codec.value.empty() ? std::string("H264") : profile.codec.value).c_str()];
    NSString *effectiveCodec = [requestedCodec caseInsensitiveCompare:@"auto"] == NSOrderedSame ? @"H264" : requestedCodec;
    NSString *status = libWebRTCAvailable ? @"Using libwebrtc" : @"libwebrtc unavailable";

    return @{
        @"status": status,
        @"effective": active,
        @"codec": [NSString stringWithFormat:@"%@ effective (%@ requested)", effectiveCodec, requestedCodec],
        @"libwebrtc": [NSString stringWithFormat:@"%@ (%@)", libWebRTCAvailable ? @"Available" : @"Unavailable", libDescription],
    };
}

static NSInteger OPNSelectedPerformanceProfile(const OPN::StreamPreferenceProfile &profile) {
    if (!profile.enableL4S && !profile.enablePowerSaver && profile.codecIndex == 0 && profile.fpsIndex == 1 && profile.bitrateIndex == 2) return 0;
    if (!profile.enableL4S && !profile.enablePowerSaver && profile.codecIndex == 1 && profile.fpsIndex == 1 && profile.bitrateIndex == 4) return 1;
    return -1;
}

@interface OPNSettingsFlippedView : NSView
@end

@implementation OPNSettingsFlippedView
- (BOOL)isFlipped { return YES; }
@end

static uint16_t OPNShortcutModifierMaskFromFlags(NSEventModifierFlags flags) {
    uint16_t out = 0;
    if (flags & NSEventModifierFlagShift) out |= 0x01;
    if (flags & NSEventModifierFlagControl) out |= 0x02;
    if (flags & NSEventModifierFlagOption) out |= 0x04;
    if (flags & NSEventModifierFlagCommand) out |= 0x08;
    if (flags & NSEventModifierFlagCapsLock) out |= 0x10;
    return out;
}

static uint16_t OPNShortcutModifierBitForKeyCode(uint16_t keyCode) {
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

static NSString *OPNControllerShortcutNameForButton(uint16_t button) {
    using namespace OPN::Input;
    switch (button) {
        case GAMEPAD_DPAD_UP: return @"D-Pad Up";
        case GAMEPAD_DPAD_DOWN: return @"D-Pad Down";
        case GAMEPAD_DPAD_LEFT: return @"D-Pad Left";
        case GAMEPAD_DPAD_RIGHT: return @"D-Pad Right";
        case GAMEPAD_START: return @"Menu";
        case GAMEPAD_BACK: return @"View";
        case GAMEPAD_LS: return @"L3";
        case GAMEPAD_RS: return @"R3";
        case GAMEPAD_LB: return @"LB";
        case GAMEPAD_RB: return @"RB";
        case GAMEPAD_GUIDE: return @"Home";
        case GAMEPAD_A: return @"A / Cross";
        case GAMEPAD_B: return @"B / Circle";
        case GAMEPAD_X: return @"X / Square";
        case GAMEPAD_Y: return @"Y / Triangle";
        default: return nil;
    }
}

static NSString *OPNControllerShortcutLabel(uint16_t mask) {
    using namespace OPN::Input;
    if (mask == 0) return @"Disabled";
    static const uint16_t order[] = {
        GAMEPAD_BACK, GAMEPAD_START, GAMEPAD_LB, GAMEPAD_RB, GAMEPAD_LS, GAMEPAD_RS,
        GAMEPAD_DPAD_UP, GAMEPAD_DPAD_DOWN, GAMEPAD_DPAD_LEFT, GAMEPAD_DPAD_RIGHT,
        GAMEPAD_A, GAMEPAD_B, GAMEPAD_X, GAMEPAD_Y, GAMEPAD_GUIDE,
    };
    NSMutableArray<NSString *> *parts = [NSMutableArray array];
    uint16_t remaining = mask;
    for (uint16_t button : order) {
        if ((mask & button) == 0) continue;
        NSString *name = OPNControllerShortcutNameForButton(button);
        if (name.length > 0) [parts addObject:name];
        remaining &= (uint16_t)~button;
    }
    if (remaining != 0) {
        [parts addObject:[NSString stringWithFormat:@"0x%04X", remaining]];
    }
    return parts.count > 0 ? [parts componentsJoinedByString:@" + "] : @"Disabled";
}

static uint16_t OPNPressedControllerShortcutMask(void) {
    using namespace OPN::Input;
    uint16_t buttons = 0;
    for (GCController *controller in GCController.controllers) {
        GCExtendedGamepad *pad = controller.extendedGamepad;
        if (!pad) continue;
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
        if (buttons != 0) break;
    }
    return buttons;
}

static uint16_t OPNSettingsGamepadButtons(void) {
    NSArray<GCController *> *controllers = [GCController controllers];
    if (controllers.count == 0) return 0;
    GCExtendedGamepad *pad = controllers.firstObject.extendedGamepad;
    if (!pad) return 0;
    uint16_t buttons = 0;
    if (pad.buttonA.value > 0.5) buttons |= 1u << 0;
    if (pad.buttonB.value > 0.5) buttons |= 1u << 1;
    if (pad.dpad.up.value > 0.5 || pad.leftThumbstick.yAxis.value > 0.65) buttons |= 1u << 2;
    if (pad.dpad.down.value > 0.5 || pad.leftThumbstick.yAxis.value < -0.65) buttons |= 1u << 3;
    if (pad.dpad.left.value > 0.5 || pad.leftThumbstick.xAxis.value < -0.65) buttons |= 1u << 4;
    if (pad.dpad.right.value > 0.5 || pad.leftThumbstick.xAxis.value > 0.65) buttons |= 1u << 5;
    return buttons;
}

static BOOL OPNSettingsGamepadNavigationActive(NSView *view) {
    NSWindow *window = view.window;
    if (!window || window.contentViewController != nil) return NO;
    return window.contentView == view || [view isDescendantOf:window.contentView];
}

@interface OPNPushToTalkShortcutField : NSTextField
@property (nonatomic, assign) uint16_t shortcutKeyCode;
@property (nonatomic, assign) uint16_t shortcutModifierMask;
@property (nonatomic, copy) void (^onShortcutChanged)(uint16_t keyCode, uint16_t modifierMask);
- (void)configureWithKeyCode:(uint16_t)keyCode modifierMask:(uint16_t)modifierMask;
@end

@implementation OPNPushToTalkShortcutField

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.editable = NO;
        self.selectable = NO;
        self.bordered = NO;
        self.drawsBackground = NO;
        self.focusRingType = NSFocusRingTypeNone;
        self.font = [NSFont systemFontOfSize:14.0 weight:NSFontWeightMedium];
        self.textColor = OpnColor(OPN::kTextPrimary);
        self.alignment = NSTextAlignmentLeft;
        self.wantsLayer = YES;
        self.layer.backgroundColor = OpnColor(0x090A0C, 0.72).CGColor;
        self.layer.cornerRadius = 11.0;
        self.layer.borderWidth = 1.0;
        self.layer.borderColor = OpnColor(OPN::kPanelBorder, 0.78).CGColor;
    }
    return self;
}

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (BOOL)becomeFirstResponder {
    BOOL result = [super becomeFirstResponder];
    self.layer.borderColor = OpnColor(OPN::kBrandGreen, 0.70).CGColor;
    return result;
}

- (BOOL)resignFirstResponder {
    BOOL result = [super resignFirstResponder];
    self.layer.borderColor = OpnColor(OPN::kPanelBorder, 0.78).CGColor;
    return result;
}

- (void)mouseDown:(NSEvent *)event {
    (void)event;
    [self.window makeFirstResponder:self];
}

- (void)configureWithKeyCode:(uint16_t)keyCode modifierMask:(uint16_t)modifierMask {
    self.shortcutKeyCode = keyCode;
    self.shortcutModifierMask = modifierMask & 0x1f;
    std::string label = OPN::StreamMicrophonePushToTalkComboLabel(self.shortcutKeyCode, self.shortcutModifierMask);
    self.stringValue = [NSString stringWithUTF8String:label.c_str()];
}

- (void)captureShortcutFromEvent:(NSEvent *)event {
    if (event.keyCode > 127) return;
    uint16_t keyCode = (uint16_t)event.keyCode;
    uint16_t modifierMask = OPNShortcutModifierMaskFromFlags(event.modifierFlags);
    [self configureWithKeyCode:keyCode modifierMask:modifierMask];
    if (self.onShortcutChanged) self.onShortcutChanged(keyCode, modifierMask);
}

- (void)keyDown:(NSEvent *)event {
    if (event.isARepeat) return;
    [self captureShortcutFromEvent:event];
}

- (void)flagsChanged:(NSEvent *)event {
    if (event.keyCode > 127) return;
    uint16_t modifierBit = OPNShortcutModifierBitForKeyCode((uint16_t)event.keyCode);
    if (modifierBit == 0) return;
    if ((OPNShortcutModifierMaskFromFlags(event.modifierFlags) & modifierBit) == 0) return;
    [self captureShortcutFromEvent:event];
}

@end

@interface OPNSettingsView ()
@property (nonatomic, strong) NSTextField *titleLabel;
@property (nonatomic, strong) NSView *shellView;
@property (nonatomic, strong) NSView *sidebarView;
@property (nonatomic, strong) NSScrollView *scrollView;
@property (nonatomic, strong) NSView *documentView;
@property (nonatomic, strong) NSMutableArray<NSButton *> *sidebarButtons;
@property (nonatomic, strong) NSArray<NSString *> *sectionNames;
@property (nonatomic, assign) NSInteger selectedSection;
@property (nonatomic, assign) NSInteger selectedAspect;
@property (nonatomic, assign) NSInteger selectedResolution;
@property (nonatomic, assign) NSInteger selectedFps;
@property (nonatomic, assign) NSInteger selectedRegion;
@property (nonatomic, assign) NSInteger selectedCodec;
@property (nonatomic, assign) NSInteger selectedBitrate;
@property (nonatomic, assign) NSInteger selectedColorDepth;
@property (nonatomic, assign) NSInteger selectedRendererPacing;
@property (nonatomic, assign) NSInteger selectedMicrophoneMode;
@property (nonatomic, assign) NSInteger selectedMicrophoneDevice;
@property (nonatomic, assign) BOOL enableL4S;
@property (nonatomic, assign) BOOL suppressInputWhenInactive;
@property (nonatomic, strong) NSTextField *posterSizeValueLabel;
@property (nonatomic, strong) NSTextField *controllerGridSizeValueLabel;
@property (nonatomic, strong) NSTextField *backgroundTintValueLabel;
@property (nonatomic, strong) NSTextField *accentRedValueLabel;
@property (nonatomic, strong) NSTextField *accentGreenValueLabel;
@property (nonatomic, strong) NSTextField *accentBlueValueLabel;
@property (nonatomic, strong) NSButton *controllerShortcutCaptureButton;
@property (nonatomic, strong) NSTextField *controllerShortcutStatusLabel;
@property (nonatomic, strong) NSTimer *controllerShortcutCaptureTimer;
@property (nonatomic, assign) CFTimeInterval controllerShortcutCaptureDeadline;
@property (nonatomic, assign) CFTimeInterval controllerShortcutPendingSince;
@property (nonatomic, assign) uint16_t controllerShortcutPendingMask;
@property (nonatomic, assign) BOOL audioDeviceListenerInstalled;
@property (nonatomic, assign) CGFloat contentAreaWidth;
@property (nonatomic, strong) NSMutableArray<NSControl *> *controllerFocusableControls;
@property (nonatomic, assign) NSInteger controllerFocusedControlIndex;
@property (nonatomic, strong) NSTimer *gamepadNavigationTimer;
@property (nonatomic, assign) uint16_t previousGamepadButtons;
@property (nonatomic, assign) CFTimeInterval lastGamepadMoveTime;
- (void)applyPerformanceProfile:(NSInteger)index;
- (void)audioDevicesChanged;
- (void)startGamepadNavigationIfNeeded;
- (void)stopGamepadNavigation;
- (void)pollGamepadNavigation;
@end

static OSStatus OPNSettingsAudioDevicesChanged(AudioObjectID, UInt32, const AudioObjectPropertyAddress *, void *clientData) {
    OPNSettingsView *view = (__bridge OPNSettingsView *)clientData;
    dispatch_async(dispatch_get_main_queue(), ^{
        [view audioDevicesChanged];
    });
    return noErr;
}

@implementation OPNSettingsView

using namespace OPN;

- (instancetype)initWithFrame:(NSRect)frame {
    return [self initWithFrame:frame selectedSectionName:nil];
}

- (instancetype)initWithFrame:(NSRect)frame selectedSectionName:(NSString *)selectedSectionName {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.layer.backgroundColor = [NSColor clearColor].CGColor;
        _selectedSection = 0;
        OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
        _selectedAspect = profile.aspectIndex;
        _selectedResolution = profile.resolutionIndex;
        _selectedFps = profile.fpsIndex;
        _selectedRegion = 0;
        _selectedCodec = profile.codecIndex;
        _selectedBitrate = profile.bitrateIndex;
        _selectedColorDepth = profile.colorQualityIndex;
        _selectedRendererPacing = profile.rendererPacingIndex;
        _selectedMicrophoneMode = profile.microphoneMode == "push-to-talk" ? 1 : (profile.microphoneMode == "voice-activity" ? 2 : 0);
        _selectedMicrophoneDevice = 0;
        _enableL4S = profile.enableL4S;
        _suppressInputWhenInactive = profile.suppressInputWhenInactive;
        _sectionNames = @[@"Stream", @"Video", @"Audio", @"Input", @"Interface", @"About", @"Thanks"];
        _sidebarButtons = [NSMutableArray array];
        _controllerFocusableControls = [NSMutableArray array];
        _controllerFocusedControlIndex = -1;
        NSUInteger requestedSection = selectedSectionName.length > 0 ? [_sectionNames indexOfObject:selectedSectionName] : NSNotFound;
        if (requestedSection != NSNotFound) _selectedSection = (NSInteger)requestedSection;

        _titleLabel = OpnLabel(@"Settings", NSZeroRect, 28.0, OpnColor(kTextPrimary), NSFontWeightSemibold);
        [self addSubview:_titleLabel];

        _shellView = [[OPNSettingsFlippedView alloc] initWithFrame:NSZeroRect];
        _shellView.wantsLayer = YES;
        _shellView.layer.backgroundColor = OpnColor(0x0F1013, 0.58).CGColor;
        _shellView.layer.cornerRadius = 18.0;
        _shellView.layer.borderWidth = 1.0;
        _shellView.layer.borderColor = OpnColor(0xFFFFFF, 0.08).CGColor;
        [self addSubview:_shellView];

        _sidebarView = [[OPNSettingsFlippedView alloc] initWithFrame:NSZeroRect];
        _sidebarView.wantsLayer = YES;
        _sidebarView.layer.backgroundColor = OpnColor(0x08090B, 0.62).CGColor;
        [_shellView addSubview:_sidebarView];
        self.titleLabel.textColor = OpnColor(kTextPrimary);

        [self buildSidebarButtons];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(streamRegionsUpdated:)
                                                     name:@"OpenNOW.StreamRegionsUpdated"
                                                   object:nil];
        [self startAudioDeviceMonitoring];

        _scrollView = [[NSScrollView alloc] initWithFrame:NSZeroRect];
        _scrollView.hasVerticalScroller = YES;
        _scrollView.hasHorizontalScroller = NO;
        _scrollView.autohidesScrollers = YES;
        _scrollView.drawsBackground = NO;
        _scrollView.borderType = NSNoBorder;
        [_shellView addSubview:_scrollView];

        _documentView = [[OPNSettingsFlippedView alloc] initWithFrame:NSZeroRect];
        _documentView.wantsLayer = YES;
        _scrollView.documentView = _documentView;
        [self rebuildContent];
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (BOOL)acceptsFirstResponder { return YES; }

- (void)dealloc {
    [self.controllerShortcutCaptureTimer invalidate];
    [self stopGamepadNavigation];
    [self stopAudioDeviceMonitoring];
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    if (self.window) {
        [self startGamepadNavigationIfNeeded];
    } else {
        [self stopGamepadNavigation];
    }
}

- (void)startAudioDeviceMonitoring {
    if (self.audioDeviceListenerInstalled) return;
    AudioObjectPropertyAddress devicesAddress = {
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    AudioObjectPropertyAddress defaultInputAddress = {
        kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    OSStatus devicesStatus = AudioObjectAddPropertyListener(kAudioObjectSystemObject, &devicesAddress, OPNSettingsAudioDevicesChanged, (__bridge void *)self);
    OSStatus inputStatus = AudioObjectAddPropertyListener(kAudioObjectSystemObject, &defaultInputAddress, OPNSettingsAudioDevicesChanged, (__bridge void *)self);
    self.audioDeviceListenerInstalled = devicesStatus == noErr || inputStatus == noErr;
}

- (void)stopAudioDeviceMonitoring {
    if (!self.audioDeviceListenerInstalled) return;
    AudioObjectPropertyAddress devicesAddress = {
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    AudioObjectPropertyAddress defaultInputAddress = {
        kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    AudioObjectRemovePropertyListener(kAudioObjectSystemObject, &devicesAddress, OPNSettingsAudioDevicesChanged, (__bridge void *)self);
    AudioObjectRemovePropertyListener(kAudioObjectSystemObject, &defaultInputAddress, OPNSettingsAudioDevicesChanged, (__bridge void *)self);
    self.audioDeviceListenerInstalled = NO;
}

- (void)audioDevicesChanged {
    if ([self.sectionNames[self.selectedSection] isEqualToString:@"Audio"]) {
        [self rebuildContent];
    }
}

- (void)streamRegionsUpdated:(NSNotification *)notification {
    (void)notification;
    if ([self.sectionNames[self.selectedSection] isEqualToString:@"Stream"]) {
        [self rebuildContent];
    }
}

- (void)buildSidebarButtons {
    for (NSUInteger i = 0; i < self.sectionNames.count; i++) {
        NSButton *button = [[NSButton alloc] initWithFrame:NSZeroRect];
        button.title = OPNSettingsDisplayName(self.sectionNames[i]);
        button.tag = (NSInteger)i;
        button.bordered = NO;
        button.target = self;
        button.action = @selector(sectionClicked:);
        button.wantsLayer = YES;
        [self.sidebarButtons addObject:button];
        [self.sidebarView addSubview:button];
    }
    [self restyleSidebarButtons];
}

- (void)restyleSidebarButtons {
    for (NSButton *button in self.sidebarButtons) {
        BOOL selected = button.tag == self.selectedSection;
        NSString *section = self.sectionNames[(NSUInteger)button.tag];
        button.hidden = NO;
        button.title = OPNSettingsDisplayName(section);
        button.font = [NSFont systemFontOfSize:14.0 weight:selected ? NSFontWeightSemibold : NSFontWeightRegular];
        button.alignment = NSTextAlignmentCenter;
        button.contentTintColor = selected ? OpnColor(kTextPrimary) : OpnColor(kTextSecondary);
        button.layer.cornerRadius = 10.0;
        button.layer.borderWidth = 0.0;
        button.layer.borderColor = (selected ? OpnColor(0xEFC8F7, 0.95) : OpnColor(0xFFFFFF, 0.18)).CGColor;
        button.layer.backgroundColor = selected ? OpnColor(kBrandGreen, 0.12).CGColor : [NSColor clearColor].CGColor;
    }
}

- (void)sectionClicked:(NSButton *)sender {
    self.selectedSection = sender.tag;
    [self restyleSidebarButtons];
    [self rebuildContent];
}

- (void)registerControllerFocusableControl:(NSControl *)control {
    if (!control || !OpnControllerModeEnabled()) return;
    control.wantsLayer = YES;
    control.layer.masksToBounds = NO;
    [self.controllerFocusableControls addObject:control];
}

- (void)updateControllerFocusedControl {
    if (!OpnControllerModeEnabled() || self.controllerFocusableControls.count == 0) {
        self.controllerFocusedControlIndex = -1;
    } else {
        self.controllerFocusedControlIndex = MAX(0, MIN(self.controllerFocusedControlIndex, (NSInteger)self.controllerFocusableControls.count - 1));
    }
    for (NSUInteger index = 0; index < self.controllerFocusableControls.count; index++) {
        NSControl *control = self.controllerFocusableControls[index];
        BOOL focused = (NSInteger)index == self.controllerFocusedControlIndex;
        control.layer.shadowColor = OpnColor(kBrandGreen).CGColor;
        control.layer.shadowOpacity = focused ? 0.56 : 0.0;
        control.layer.shadowRadius = focused ? 18.0 : 0.0;
        control.layer.shadowOffset = CGSizeZero;
        if (focused) {
            [control.window makeFirstResponder:self];
            [control scrollRectToVisible:NSInsetRect(control.bounds, -28.0, -18.0)];
        }
    }
}

- (void)moveControllerFocusBy:(NSInteger)delta {
    if (self.controllerFocusableControls.count == 0) return;
    NSInteger next = self.controllerFocusedControlIndex < 0 ? 0 : self.controllerFocusedControlIndex + delta;
    next = MAX(0, MIN(next, (NSInteger)self.controllerFocusableControls.count - 1));
    if (next == self.controllerFocusedControlIndex) return;
    self.controllerFocusedControlIndex = next;
    [self updateControllerFocusedControl];
    OpnPlayConsoleTone(OPNConsoleToneMove);
}

- (NSControl *)controllerFocusedControl {
    if (self.controllerFocusedControlIndex < 0 || self.controllerFocusedControlIndex >= (NSInteger)self.controllerFocusableControls.count) return nil;
    return self.controllerFocusableControls[(NSUInteger)self.controllerFocusedControlIndex];
}

- (void)activateControllerFocusedControl {
    NSControl *control = [self controllerFocusedControl];
    if (!control) return;
    OpnPlayConsoleTone(OPNConsoleToneSelect);
    if ([control isKindOfClass:NSButton.class]) {
        [(NSButton *)control performClick:self];
    }
}

- (void)adjustControllerFocusedSliderBy:(NSInteger)direction {
    NSControl *control = [self controllerFocusedControl];
    if (![control isKindOfClass:NSSlider.class]) return;
    NSSlider *slider = (NSSlider *)control;
    CGFloat step = slider.maxValue > 200.0 ? 8.0 : 5.0;
    CGFloat value = MAX(slider.minValue, MIN(slider.maxValue, slider.doubleValue + (CGFloat)direction * step));
    if (std::fabs(value - slider.doubleValue) < 0.001) return;
    slider.doubleValue = value;
    [NSApp sendAction:slider.action to:slider.target from:slider];
    OpnPlayConsoleTone(OPNConsoleToneChange);
}

- (void)layout {
    [super layout];
    CGFloat width = NSWidth(self.bounds);
    self.scrollView.hidden = NO;
    self.titleLabel.font = [NSFont systemFontOfSize:28.0 weight:NSFontWeightSemibold];
    CGFloat outerMargin = width < 900.0 ? 24.0 : 64.0;
    CGFloat contentWidth = MIN(1560.0, MAX(360.0, width - outerMargin * 2.0));
    CGFloat x = floor((width - contentWidth) / 2.0);
    CGFloat navHeight = OpnControllerModeEnabled() ? kSettingsControllerNavHeight : kSettingsNavHeight;
    CGFloat y = navHeight + kSettingsTopInset;
    self.titleLabel.frame = NSMakeRect(x, y - 48.0, 240.0, 34.0);
    CGFloat shellHeight = MAX(360.0, NSHeight(self.bounds) - y - 34.0);
    self.shellView.frame = NSMakeRect(x, y, contentWidth, shellHeight);
    self.shellView.layer.cornerRadius = 18.0;
    self.shellView.layer.borderWidth = 1.0;
    CGFloat sidebarWidth = width < 900.0 ? 210.0 : MIN(kSettingsSidebarWidth, MAX(240.0, contentWidth * 0.26));
    CGFloat columnGap = width < 900.0 ? 16.0 : kSettingsColumnGap;
    self.sidebarView.frame = NSMakeRect(0, 0, sidebarWidth, shellHeight);

    CGFloat buttonY = 22.0;
    for (NSButton *button in self.sidebarButtons) {
        if (button.hidden) continue;
        button.frame = NSMakeRect(18, buttonY, MAX(160.0, sidebarWidth - 36.0), 44);
        buttonY += 54.0;
    }

    CGFloat scrollX = sidebarWidth + columnGap;
    CGFloat scrollWidth = MAX(260.0, contentWidth - scrollX - 28.0);
    self.scrollView.frame = NSMakeRect(scrollX, 22.0, scrollWidth, shellHeight - 44.0);
    if (std::fabs(self.contentAreaWidth - scrollWidth) > 1.0) {
        self.contentAreaWidth = scrollWidth;
        [self rebuildContent];
    }
    self.documentView.frame = NSMakeRect(0, 0, NSWidth(self.scrollView.frame), MAX(NSHeight(self.scrollView.frame), NSHeight(self.documentView.frame)));
    [self layoutContentSubviews];
}

- (void)layoutContentSubviews {
    CGFloat width = NSWidth(self.documentView.bounds);
    CGFloat y = 0;
    for (NSView *subview in self.documentView.subviews) {
        subview.frame = NSMakeRect(0, y, width, NSHeight(subview.frame));
        y += NSHeight(subview.frame) + 24.0;
    }
    self.documentView.frame = NSMakeRect(0, 0, width, MAX(y, NSHeight(self.scrollView.contentView.bounds)));
}

- (void)rebuildContent {
    for (NSView *view in [self.documentView.subviews copy]) {
        [view removeFromSuperview];
    }
    [self.controllerFocusableControls removeAllObjects];
    self.controllerFocusedControlIndex = -1;
    NSString *section = self.sectionNames[self.selectedSection];
    self.titleLabel.stringValue = @"Settings";
    if ([section isEqualToString:@"Stream"]) {
        [self buildStreamContent];
    } else if ([section isEqualToString:@"Video"]) {
        [self buildVideoContent];
    } else if ([section isEqualToString:@"Audio"]) {
        [self buildAudioContent];
    } else if ([section isEqualToString:@"Input"]) {
        [self buildInputContent];
    } else if ([section isEqualToString:@"Interface"]) {
        [self buildInterfaceContent];
    } else {
        [self buildSimpleSectionContent:section];
    }
    [self setNeedsLayout:YES];
    if (OpnControllerModeEnabled() && self.controllerFocusableControls.count > 0) {
        self.controllerFocusedControlIndex = 0;
        [self updateControllerFocusedControl];
    }
}

- (NSView *)panelWithTitle:(NSString *)title height:(CGFloat)height {
    CGFloat panelWidth = MAX(320.0, self.contentAreaWidth > 0 ? self.contentAreaWidth : 720.0);
    NSView *panel = [[OPNSettingsFlippedView alloc] initWithFrame:NSMakeRect(0, 0, panelWidth, height)];
    panel.wantsLayer = YES;
    panel.layer.backgroundColor = OpnColor(kSurfaceRaised, 0.66).CGColor;
    panel.layer.cornerRadius = 18.0;
    panel.layer.borderWidth = 1.0;
    panel.layer.borderColor = OpnColor(0xFFFFFF, 0.08).CGColor;
    [panel addSubview:OpnLabel(title, NSMakeRect(24, 26, 260, 28), 19.0, OpnColor(kTextPrimary), NSFontWeightSemibold)];
    NSView *divider = [[NSView alloc] initWithFrame:NSMakeRect(24, 72, MAX(120.0, panelWidth - 48.0), 1)];
    divider.wantsLayer = YES;
    divider.layer.backgroundColor = OpnColor(0xFFFFFF, 0.08).CGColor;
    [panel addSubview:divider];
    return panel;
}

- (void)buildStreamContent {
    CGFloat panelWidth = MAX(320.0, self.contentAreaWidth > 0 ? self.contentAreaWidth : 720.0);
    CGFloat controlX = [self controlXForPanelWidth:panelWidth];
    CGFloat controlWidth = [self controlWidthForPanelWidth:panelWidth];
    NSView *region = [self panelWithTitle:@"Region" height:202.0];
    [region addSubview:[self rowLabel:@"Server" y:112.0]];
    [region addSubview:[self regionPopupWithFrame:NSMakeRect(controlX, 100.0, controlWidth, 42.0)]];
    NSTextField *hint = OpnLabel(@"Automatic uses the lowest measured region when available. Pick a region to override it.",
                                 NSMakeRect(controlX, 152.0, controlWidth, 34.0),
                                 12.0,
                                 OpnColor(kTextMuted),
                                 NSFontWeightRegular);
    hint.maximumNumberOfLines = 2;
    [region addSubview:hint];
    [self.documentView addSubview:region];

    NSView *network = [self panelWithTitle:@"Network" height:292.0];
    OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
    [network addSubview:[self rowLabel:@"Profile" y:112.0]];
    [self addOptionGroupTo:network group:9 titles:@[@"Low Latency", @"Quality"] selected:OPNSelectedPerformanceProfile(profile) y:102.0 widths:@[@126.0, @92.0]];
    NSTextField *profileHint = OpnLabel(@"Presets adjust codec, FPS, bitrate, and disable experimental L4S for new sessions. Manual controls can still override them.",
                                         NSMakeRect(controlX, 152.0, controlWidth, 40.0),
                                        12.0,
                                        OpnColor(kTextMuted),
                                        NSFontWeightRegular);
    profileHint.maximumNumberOfLines = 2;
    [network addSubview:profileHint];

    [network addSubview:[self rowLabel:@"L4S Mode" y:224.0]];
    NSButton *l4sToggle = [[NSButton alloc] initWithFrame:NSMakeRect(controlX, 216.0, controlWidth, 28.0)];
    l4sToggle.buttonType = NSButtonTypeSwitch;
    l4sToggle.title = @"Enable experimental L4S requests";
    l4sToggle.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
    l4sToggle.contentTintColor = OpnColor(kBrandGreen);
    l4sToggle.state = self.enableL4S ? NSControlStateValueOn : NSControlStateValueOff;
    l4sToggle.target = self;
    l4sToggle.action = @selector(l4sToggleChanged:);
    [network addSubview:l4sToggle];
    NSTextField *l4sHint = OpnLabel(@"May reduce latency on compatible paths, but can destabilize streams on some networks. Leave off unless testing.",
                                    NSMakeRect(controlX, 256.0, controlWidth, 34.0),
                                    12.0,
                                    OpnColor(kTextMuted),
                                    NSFontWeightRegular);
    l4sHint.maximumNumberOfLines = 2;
    [network addSubview:l4sHint];
    [self.documentView addSubview:network];

    [self buildWebRTCBackendDiagnosticsContentWithPanelWidth:panelWidth controlX:controlX controlWidth:controlWidth];
}

- (void)buildVideoContent {
    CGFloat panelWidth = MAX(320.0, self.contentAreaWidth > 0 ? self.contentAreaWidth : 720.0);
    CGFloat controlX = [self controlXForPanelWidth:panelWidth];
    CGFloat controlWidth = [self controlWidthForPanelWidth:panelWidth];
    NSView *video = [self panelWithTitle:@"Video" height:760.0];
    [video addSubview:[self rowLabel:@"Aspect Ratio" y:112.0]];
    NSMutableArray<NSString *> *aspectTitles = [NSMutableArray array];
    for (const OPN::StreamAspectOption &option : OPN::StreamAspectOptions()) {
        [aspectTitles addObject:[NSString stringWithUTF8String:option.label.c_str()]];
    }
    [self addOptionGroupTo:video group:1 titles:aspectTitles selected:self.selectedAspect y:102.0 widths:@[@86.0, @92.0, @86.0, @86.0]];

    [video addSubview:[self rowLabel:@"Resolution" y:188.0]];
    [video addSubview:[self resolutionPopupWithFrame:NSMakeRect(controlX, 176, controlWidth, 42)]];

    NSMutableArray<NSString *> *fpsTitles = [NSMutableArray array];
    for (int fps : OPN::StreamFpsOptions()) {
        [fpsTitles addObject:[NSString stringWithFormat:@"%d", fps]];
    }
    [video addSubview:[self rowLabel:@"FPS" y:264.0]];
    [self addOptionGroupTo:video group:3 titles:fpsTitles selected:self.selectedFps y:254.0 widths:@[@62.0, @62.0, @62.0]];
    NSMutableArray<NSString *> *bitrateTitles = [NSMutableArray array];
    for (const OPN::StreamBitrateOption &option : OPN::StreamBitrateOptions()) {
        [bitrateTitles addObject:[NSString stringWithUTF8String:option.label.c_str()]];
    }
    [video addSubview:[self rowLabel:@"Bitrate" y:340.0]];
    [self addOptionGroupTo:video group:8 titles:bitrateTitles selected:self.selectedBitrate y:330.0 widths:@[@86.0, @86.0, @86.0, @86.0, @94.0]];

    NSMutableArray<NSString *> *codecTitles = [NSMutableArray array];
    for (const OPN::StreamCodecOption &option : OPN::StreamCodecOptions()) {
        [codecTitles addObject:[NSString stringWithUTF8String:option.label.c_str()]];
    }
    [video addSubview:[self rowLabel:@"Codec" y:416.0]];
    [self addOptionGroupTo:video group:4 titles:codecTitles selected:self.selectedCodec y:406.0 widths:@[@142.0, @116.0, @96.0, @70.0]];
    NSMutableArray<NSString *> *colorDepthTitles = [NSMutableArray array];
    for (const OPN::StreamColorQualityOption &option : OPN::StreamColorQualityOptions()) {
        [colorDepthTitles addObject:[NSString stringWithUTF8String:option.label.c_str()]];
    }
    [video addSubview:[self rowLabel:@"Color Depth" y:492.0]];
    [self addOptionGroupTo:video group:7 titles:colorDepthTitles selected:self.selectedColorDepth y:482.0 widths:@[@112.0, @112.0, @124.0, @124.0]];

    NSMutableArray<NSString *> *rendererPacingTitles = [NSMutableArray array];
    for (int fps : OPN::StreamRendererPacingOptions()) {
        [rendererPacingTitles addObject:[NSString stringWithFormat:@"%d", fps]];
    }
    [video addSubview:[self rowLabel:@"Advanced: Renderer Pacing" y:582.0]];
    [self addOptionGroupTo:video group:10 titles:rendererPacingTitles selected:self.selectedRendererPacing y:572.0 widths:@[@62.0, @62.0, @62.0]];
    NSTextField *pacingHint = OpnLabel(@"Advanced experimental renderer timing. Do not change unless you know what it does; incorrect values can make motion look worse.",
                                       NSMakeRect(controlX, 614.0, controlWidth, 34.0),
                                       12.0,
                                       OpnColor(kTextMuted),
                                       NSFontWeightRegular);
    pacingHint.maximumNumberOfLines = 2;
    [video addSubview:pacingHint];
    [self.documentView addSubview:video];
}

- (void)addInfoRowToPanel:(NSView *)panel
                    title:(NSString *)title
                    value:(NSString *)value
                        y:(CGFloat)y
               valueWidth:(CGFloat)valueWidth
            monospaceValue:(BOOL)monospaceValue {
    [panel addSubview:[self rowLabel:title y:y]];
    NSTextField *valueLabel = OpnLabel(value ?: @"Unavailable",
                                      NSMakeRect([self controlXForPanelWidth:NSWidth(panel.frame)], y - 2.0, valueWidth, 44.0),
                                      12.0,
                                      OpnColor(kTextSecondary),
                                      NSFontWeightRegular);
    valueLabel.maximumNumberOfLines = 2;
    valueLabel.lineBreakMode = NSLineBreakByTruncatingMiddle;
    if (monospaceValue) {
        valueLabel.font = [NSFont monospacedSystemFontOfSize:11.5 weight:NSFontWeightRegular];
    }
    [panel addSubview:valueLabel];
}

- (void)buildWebRTCBackendDiagnosticsContentWithPanelWidth:(CGFloat)panelWidth controlX:(CGFloat)controlX controlWidth:(CGFloat)controlWidth {
    (void)controlX;
    NSDictionary<NSString *, NSString *> *runtime = OPNWebRTCBackendRuntimeInfo();
    NSView *panel = [self panelWithTitle:@"WebRTC Backend" height:354.0];
    NSTextField *description = OpnLabel(@"OpenNOW streams through libwebrtc only. New sessions fail fast if the libwebrtc framework is unavailable in this build.",
                                         NSMakeRect(24.0, 92.0, MAX(260.0, NSWidth(panel.frame) - 48.0), 38.0),
                                         12.0,
                                         OpnColor(kTextMuted),
                                        NSFontWeightRegular);
    description.maximumNumberOfLines = 2;
    [panel addSubview:description];

    [self addInfoRowToPanel:panel title:@"Status" value:runtime[@"status"] y:146.0 valueWidth:controlWidth monospaceValue:NO];
    [self addInfoRowToPanel:panel title:@"Active" value:runtime[@"effective"] y:198.0 valueWidth:controlWidth monospaceValue:NO];
    [self addInfoRowToPanel:panel title:@"Codec" value:runtime[@"codec"] y:250.0 valueWidth:controlWidth monospaceValue:NO];
    [self addInfoRowToPanel:panel title:@"libwebrtc" value:runtime[@"libwebrtc"] y:302.0 valueWidth:controlWidth monospaceValue:NO];
    [self.documentView addSubview:panel];
}

- (void)buildAudioContent {
    OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
    std::vector<OPN::StreamMicrophoneModeOption> modes = OPN::StreamMicrophoneModeOptions();
    std::vector<OPN::StreamMicrophoneDeviceOption> devices = OPN::LoadMicrophoneDeviceOptions();

    BOOL microphoneEnabled = profile.microphoneMode != "disabled";
    CGFloat shortcutY = microphoneEnabled ? 304.0 : 204.0;
    NSView *panel = [self panelWithTitle:@"Audio" height:microphoneEnabled ? 506.0 : 420.0];
    CGFloat panelWidth = MAX(320.0, NSWidth(panel.frame));
    CGFloat controlX = [self controlXForPanelWidth:panelWidth];
    CGFloat controlWidth = [self controlWidthForPanelWidth:panelWidth];

    [panel addSubview:[self rowLabel:@"Microphone" y:104.0]];
    NSPopUpButton *modePopup = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(controlX, 96.0, controlWidth, 38.0) pullsDown:NO];
    modePopup.target = self;
    modePopup.action = @selector(microphoneModePopupChanged:);
    modePopup.bordered = NO;
    modePopup.font = [NSFont systemFontOfSize:14.0 weight:NSFontWeightRegular];
    modePopup.contentTintColor = OpnColor(kTextPrimary);
    modePopup.wantsLayer = YES;
    modePopup.layer.backgroundColor = OpnColor(0x090A0C, 0.72).CGColor;
    modePopup.layer.cornerRadius = 11.0;
    modePopup.layer.borderWidth = 1.0;
    modePopup.layer.borderColor = OpnColor(kPanelBorder, 0.78).CGColor;
    [modePopup removeAllItems];
    NSInteger selectedMode = 0;
    for (size_t i = 0; i < modes.size(); i++) {
        [modePopup addItemWithTitle:[NSString stringWithUTF8String:modes[i].label.c_str()]];
        if (modes[i].value == profile.microphoneMode) selectedMode = (NSInteger)i;
    }
    [modePopup selectItemAtIndex:selectedMode];
    self.selectedMicrophoneMode = selectedMode;
    [panel addSubview:modePopup];

    NSTextField *modeHint = OpnLabel(@"Open Mic is always live. Push-to-Talk only sends audio while the configured shortcut is held.",
                                    NSMakeRect(controlX, 140.0, controlWidth, 38.0),
                                    12.0,
                                    OpnColor(kTextMuted),
                                    NSFontWeightRegular);
    modeHint.maximumNumberOfLines = 2;
    [panel addSubview:modeHint];

    if (microphoneEnabled) {
        [panel addSubview:[self rowLabel:@"Input Device" y:204.0]];
        NSPopUpButton *devicePopup = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(controlX, 196.0, controlWidth, 38.0) pullsDown:NO];
        devicePopup.target = self;
        devicePopup.action = @selector(microphoneDevicePopupChanged:);
        devicePopup.bordered = NO;
        devicePopup.font = [NSFont systemFontOfSize:14.0 weight:NSFontWeightRegular];
        devicePopup.contentTintColor = OpnColor(kTextPrimary);
        devicePopup.wantsLayer = YES;
        devicePopup.layer.backgroundColor = OpnColor(0x090A0C, 0.72).CGColor;
        devicePopup.layer.cornerRadius = 11.0;
        devicePopup.layer.borderWidth = 1.0;
        devicePopup.layer.borderColor = OpnColor(kPanelBorder, 0.78).CGColor;
        [devicePopup removeAllItems];
        NSInteger selectedDevice = 0;
        for (size_t i = 0; i < devices.size(); i++) {
            [devicePopup addItemWithTitle:[NSString stringWithUTF8String:devices[i].label.c_str()]];
            if (devices[i].uniqueId == profile.microphoneDeviceId) selectedDevice = (NSInteger)i;
        }
        [devicePopup selectItemAtIndex:selectedDevice];
        self.selectedMicrophoneDevice = selectedDevice;
        [panel addSubview:devicePopup];

        NSTextField *deviceHint = OpnLabel(@"macOS may ask for microphone permission the first time a stream starts with mic enabled.",
                                          NSMakeRect(controlX, 240.0, controlWidth, 38.0),
                                          12.0,
                                          OpnColor(kTextMuted),
                                          NSFontWeightRegular);
        deviceHint.maximumNumberOfLines = 2;
        [panel addSubview:deviceHint];
    }

    [panel addSubview:[self rowLabel:@"Push-to-Talk" y:shortcutY + 8.0]];
    OPNPushToTalkShortcutField *shortcutField = [[OPNPushToTalkShortcutField alloc] initWithFrame:NSMakeRect(controlX, shortcutY, controlWidth, 38.0)];
    [shortcutField configureWithKeyCode:(uint16_t)profile.microphonePushToTalkKeyCode
                           modifierMask:(uint16_t)profile.microphonePushToTalkModifierMask];
    shortcutField.onShortcutChanged = ^(uint16_t keyCode, uint16_t modifierMask) {
        OPN::SaveStreamMicrophonePushToTalkKeyCode((int)keyCode);
        OPN::SaveStreamMicrophonePushToTalkModifierMask((int)modifierMask);
    };
    [panel addSubview:shortcutField];

    NSTextField *hint = OpnLabel(@"Click the box, hold any modifiers, then press the final key. Used when Microphone is Push-to-Talk and not sent to the game while streaming.",
                                 NSMakeRect(controlX, shortcutY + 52.0, controlWidth, 54.0),
                                 12.0,
                                 OpnColor(kTextMuted),
                                 NSFontWeightRegular);
    hint.maximumNumberOfLines = 3;
    [panel addSubview:hint];

    CGFloat toggleY = shortcutY + 124.0;
    [panel addSubview:[self rowLabel:@"Mic Toggle" y:toggleY + 8.0]];
    NSTextField *toggleShortcut = OpnLabel(@"Command-M",
                                           NSMakeRect(controlX, toggleY, MIN(180.0, controlWidth), 32.0),
                                           13.0,
                                           OpnColor(kTextPrimary),
                                           NSFontWeightSemibold,
                                           NSTextAlignmentCenter);
    toggleShortcut.wantsLayer = YES;
    toggleShortcut.layer.cornerRadius = 10.0;
    toggleShortcut.layer.borderWidth = 1.0;
    toggleShortcut.layer.borderColor = OpnColor(kPanelBorder, 0.78).CGColor;
    toggleShortcut.layer.backgroundColor = OpnColor(kInputBackground, 0.72).CGColor;
    [panel addSubview:toggleShortcut];

    NSTextField *toggleHint = OpnLabel(@"While streaming, press Command-M to mute or re-enable the microphone without changing this saved microphone mode.",
                                       NSMakeRect(controlX, toggleY + 42.0, controlWidth, 38.0),
                                       12.0,
                                       OpnColor(kTextMuted),
                                       NSFontWeightRegular);
    toggleHint.maximumNumberOfLines = 2;
    [panel addSubview:toggleHint];
    [self.documentView addSubview:panel];
}

- (void)buildInputContent {
    OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
    self.suppressInputWhenInactive = profile.suppressInputWhenInactive;

    NSView *panel = [self panelWithTitle:@"Input" height:252.0];
    CGFloat panelWidth = MAX(320.0, NSWidth(panel.frame));
    CGFloat controlX = [self controlXForPanelWidth:panelWidth];
    CGFloat controlWidth = [self controlWidthForPanelWidth:panelWidth];

    [panel addSubview:[self rowLabel:@"Window Focus" y:104.0]];
    NSButton *focusToggle = [[NSButton alloc] initWithFrame:NSMakeRect(controlX, 96.0, controlWidth, 28.0)];
    focusToggle.buttonType = NSButtonTypeSwitch;
    focusToggle.title = @"Block game input when OpenNOW is inactive";
    focusToggle.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
    focusToggle.contentTintColor = OpnColor(kBrandGreen);
    focusToggle.state = self.suppressInputWhenInactive ? NSControlStateValueOn : NSControlStateValueOff;
    focusToggle.target = self;
    focusToggle.action = @selector(suppressInputWhenInactiveToggleChanged:);
    [panel addSubview:focusToggle];

    NSTextField *hint = OpnLabel(@"When enabled, keyboard, mouse, push-to-talk, and gamepad events are ignored unless the stream window is active.",
                                 NSMakeRect(controlX, 136.0, controlWidth, 54.0),
                                 12.0,
                                 OpnColor(kTextMuted),
                                 NSFontWeightRegular);
    hint.maximumNumberOfLines = 3;
    [panel addSubview:hint];
    [self.documentView addSubview:panel];
}

- (void)buildInterfaceContent {
    NSView *panel = [self panelWithTitle:@"Interface" height:936.0];
    CGFloat panelWidth = MAX(320.0, NSWidth(panel.frame));
    CGFloat controlX = [self controlXForPanelWidth:panelWidth];
    CGFloat controlWidth = [self controlWidthForPanelWidth:panelWidth];

    unsigned accent = OpnCurrentAccentRGB();
    NSInteger red = (NSInteger)((accent >> 16) & 0xFF);
    NSInteger green = (NSInteger)((accent >> 8) & 0xFF);
    NSInteger blue = (NSInteger)(accent & 0xFF);

    [panel addSubview:[self rowLabel:@"Controller Mode" y:104.0]];
    NSButton *controllerModeToggle = [[NSButton alloc] initWithFrame:NSMakeRect(controlX, 96.0, controlWidth, 28.0)];
    controllerModeToggle.buttonType = NSButtonTypeSwitch;
    controllerModeToggle.title = @"Use a console-style library optimized for gamepad navigation";
    controllerModeToggle.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
    controllerModeToggle.contentTintColor = OpnColor(kBrandGreen);
    controllerModeToggle.state = OpnControllerModeEnabled() ? NSControlStateValueOn : NSControlStateValueOff;
    controllerModeToggle.target = self;
    controllerModeToggle.action = @selector(controllerModeToggleChanged:);
    [panel addSubview:controllerModeToggle];
    [self registerControllerFocusableControl:controllerModeToggle];

    NSTextField *controllerHint = OpnLabel(@"Controller Mode keeps mouse and keyboard support while giving the library larger focus states, smoother carousel navigation, and a lean-back launch flow.",
                                            NSMakeRect(controlX, 132.0, controlWidth, 38.0),
                                           12.0,
                                           OpnColor(kTextMuted),
                                           NSFontWeightRegular);
    controllerHint.maximumNumberOfLines = 2;
    [panel addSubview:controllerHint];

    [panel addSubview:[self rowLabel:@"Background" y:198.0]];
    NSButton *backgroundAnimationToggle = [[NSButton alloc] initWithFrame:NSMakeRect(controlX, 190.0, controlWidth, 28.0)];
    backgroundAnimationToggle.buttonType = NSButtonTypeSwitch;
    backgroundAnimationToggle.title = @"Animate the game-accent background";
    backgroundAnimationToggle.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
    backgroundAnimationToggle.contentTintColor = OpnColor(kBrandGreen);
    backgroundAnimationToggle.state = OpnBackgroundAnimationEnabled() ? NSControlStateValueOn : NSControlStateValueOff;
    backgroundAnimationToggle.target = self;
    backgroundAnimationToggle.action = @selector(backgroundAnimationToggleChanged:);
    [panel addSubview:backgroundAnimationToggle];
    [self registerControllerFocusableControl:backgroundAnimationToggle];

    NSTextField *backgroundHint = OpnLabel(@"When off, the animated ribbons and sparkles pause while keeping the static artwork-tinted background.",
                                           NSMakeRect(controlX, 226.0, controlWidth, 38.0),
                                           12.0,
                                           OpnColor(kTextMuted),
                                           NSFontWeightRegular);
    backgroundHint.maximumNumberOfLines = 2;
    [panel addSubview:backgroundHint];

    [panel addSubview:[self rowLabel:@"Derived Accent" y:292.0]];
    NSButton *derivedAccentToggle = [[NSButton alloc] initWithFrame:NSMakeRect(controlX, 284.0, controlWidth, 28.0)];
    derivedAccentToggle.buttonType = NSButtonTypeSwitch;
    derivedAccentToggle.title = @"Use focused game artwork to tint Controller Mode";
    derivedAccentToggle.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
    derivedAccentToggle.contentTintColor = OpnColor(kBrandGreen);
    derivedAccentToggle.state = OpnDerivedAccentColorsEnabled() ? NSControlStateValueOn : NSControlStateValueOff;
    derivedAccentToggle.target = self;
    derivedAccentToggle.action = @selector(derivedAccentToggleChanged:);
    [panel addSubview:derivedAccentToggle];
    [self registerControllerFocusableControl:derivedAccentToggle];

    NSTextField *derivedAccentHint = OpnLabel(@"When off, Controller Mode uses your manual accent color instead of per-game artwork colors.",
                                             NSMakeRect(controlX, 320.0, controlWidth, 38.0),
                                             12.0,
                                             OpnColor(kTextMuted),
                                             NSFontWeightRegular);
    derivedAccentHint.maximumNumberOfLines = 2;
    [panel addSubview:derivedAccentHint];

    [panel addSubview:[self rowLabel:@"Background Tint" y:386.0]];
    NSSlider *backgroundTintSlider = [[NSSlider alloc] initWithFrame:NSMakeRect(controlX, 380.0, MIN(300.0, controlWidth - 72.0), 28.0)];
    backgroundTintSlider.minValue = 0.0;
    backgroundTintSlider.maxValue = 100.0;
    backgroundTintSlider.doubleValue = OpnBackgroundTintStrength() * 100.0;
    backgroundTintSlider.continuous = YES;
    backgroundTintSlider.target = self;
    backgroundTintSlider.action = @selector(backgroundTintSliderChanged:);
    [panel addSubview:backgroundTintSlider];
    [self registerControllerFocusableControl:backgroundTintSlider];

    self.backgroundTintValueLabel = OpnLabel([NSString stringWithFormat:@"%.0f%%", backgroundTintSlider.doubleValue],
                                             NSMakeRect(controlX + MIN(312.0, controlWidth - 60.0), 384.0, 60.0, 22.0),
                                             12.0,
                                             OpnColor(kTextSecondary),
                                             NSFontWeightSemibold,
                                             NSTextAlignmentRight);
    [panel addSubview:self.backgroundTintValueLabel];

    NSTextField *tintHint = OpnLabel(@"Lower values keep the animated background brighter. Set to 0% to remove the dark tint.",
                                     NSMakeRect(controlX, 418.0, controlWidth, 38.0),
                                     12.0,
                                     OpnColor(kTextMuted),
                                     NSFontWeightRegular);
    tintHint.maximumNumberOfLines = 2;
    [panel addSubview:tintHint];

    [panel addSubview:[self rowLabel:@"Stream Library Shortcut" y:480.0]];
    CGFloat shortcutButtonWidth = MIN(300.0, MAX(170.0, controlWidth - 112.0));
    NSButton *shortcutButton = [[NSButton alloc] initWithFrame:NSMakeRect(controlX, 470.0, shortcutButtonWidth, 38.0)];
    shortcutButton.title = OPNControllerShortcutLabel(OpnControllerLibraryShortcutMask());
    shortcutButton.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightSemibold];
    shortcutButton.bordered = NO;
    shortcutButton.contentTintColor = OpnColor(kTextPrimary);
    shortcutButton.wantsLayer = YES;
    shortcutButton.layer.cornerRadius = 10.0;
    shortcutButton.layer.borderWidth = 1.0;
    shortcutButton.layer.borderColor = OpnColor(kBrandGreen, 0.38).CGColor;
    shortcutButton.layer.backgroundColor = OpnColor(kInputBackground, 0.72).CGColor;
    shortcutButton.target = self;
    shortcutButton.action = @selector(controllerShortcutCaptureClicked:);
    self.controllerShortcutCaptureButton = shortcutButton;
    [panel addSubview:shortcutButton];
    [self registerControllerFocusableControl:shortcutButton];

    NSButton *shortcutReset = [[NSButton alloc] initWithFrame:NSMakeRect(controlX + shortcutButtonWidth + 10.0, 470.0, MIN(96.0, MAX(76.0, controlWidth - shortcutButtonWidth - 10.0)), 38.0)];
    shortcutReset.title = @"Default";
    shortcutReset.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
    shortcutReset.bordered = NO;
    shortcutReset.contentTintColor = OpnColor(kTextMuted);
    shortcutReset.wantsLayer = YES;
    shortcutReset.layer.cornerRadius = 10.0;
    shortcutReset.layer.borderWidth = 1.0;
    shortcutReset.layer.borderColor = OpnColor(kPanelBorder, 0.72).CGColor;
    shortcutReset.layer.backgroundColor = OpnColor(kInputBackground, 0.52).CGColor;
    shortcutReset.target = self;
    shortcutReset.action = @selector(controllerShortcutResetClicked:);
    [panel addSubview:shortcutReset];
    [self registerControllerFocusableControl:shortcutReset];

    self.controllerShortcutStatusLabel = OpnLabel(@"Click the combo, press and hold any controller button or combo, then release after it saves.",
                                                  NSMakeRect(controlX, 518.0, controlWidth, 54.0),
                                                  12.0,
                                                  OpnColor(kTextMuted),
                                                  NSFontWeightRegular);
    self.controllerShortcutStatusLabel.maximumNumberOfLines = 3;
    [panel addSubview:self.controllerShortcutStatusLabel];

    [panel addSubview:[self rowLabel:@"Accent Color" y:588.0]];
    NSTextField *accentSummary = OpnLabel([NSString stringWithFormat:@"RGB %ld, %ld, %ld", (long)red, (long)green, (long)blue],
                                          NSMakeRect(controlX, 588.0, controlWidth, 20.0),
                                          13.0,
                                          OpnColor(kTextPrimary),
                                          NSFontWeightSemibold);
    [panel addSubview:accentSummary];

    NSView *swatch = [[NSView alloc] initWithFrame:NSMakeRect(controlX + MIN(162.0, controlWidth - 36.0), 585.0, 34.0, 24.0)];
    swatch.wantsLayer = YES;
    swatch.layer.cornerRadius = 8.0;
    swatch.layer.backgroundColor = OpnColor(kBrandGreen).CGColor;
    swatch.layer.borderWidth = 1.0;
    swatch.layer.borderColor = OpnColor(0xFFFFFF, 0.22).CGColor;
    [panel addSubview:swatch];

    NSArray<NSString *> *channelNames = @[@"Red", @"Green", @"Blue"];
    NSArray<NSNumber *> *channelValues = @[@(red), @(green), @(blue)];
    for (NSInteger i = 0; i < 3; i++) {
        CGFloat y = 624.0 + i * 42.0;
        NSTextField *label = OpnLabel(channelNames[(NSUInteger)i], NSMakeRect(controlX, y + 3.0, 62.0, 20.0), 12.0, OpnColor(kTextSecondary), NSFontWeightMedium);
        [panel addSubview:label];

        NSSlider *slider = [[NSSlider alloc] initWithFrame:NSMakeRect(controlX + 68.0, y, MIN(260.0, controlWidth - 136.0), 28.0)];
        slider.minValue = 0.0;
        slider.maxValue = 255.0;
        slider.integerValue = channelValues[(NSUInteger)i].integerValue;
        slider.continuous = YES;
        slider.tag = i;
        slider.target = self;
        slider.action = @selector(accentColorSliderChanged:);
        [panel addSubview:slider];
        [self registerControllerFocusableControl:slider];

        NSTextField *valueLabel = OpnLabel([NSString stringWithFormat:@"%ld", (long)slider.integerValue],
                                           NSMakeRect(controlX + MIN(338.0, controlWidth - 54.0), y + 3.0, 54.0, 20.0),
                                           12.0,
                                           OpnColor(kTextSecondary),
                                           NSFontWeightSemibold,
                                           NSTextAlignmentRight);
        [panel addSubview:valueLabel];
        if (i == 0) self.accentRedValueLabel = valueLabel;
        if (i == 1) self.accentGreenValueLabel = valueLabel;
        if (i == 2) self.accentBlueValueLabel = valueLabel;
    }

    [panel addSubview:[self rowLabel:@"Poster Size" y:758.0]];
    NSSlider *posterSlider = [[NSSlider alloc] initWithFrame:NSMakeRect(controlX, 752.0, MIN(300.0, controlWidth - 72.0), 28.0)];
    posterSlider.minValue = 80.0;
    posterSlider.maxValue = 130.0;
    posterSlider.doubleValue = OpnPosterSizeScale() * 100.0;
    posterSlider.continuous = YES;
    posterSlider.target = self;
    posterSlider.action = @selector(posterSizeSliderChanged:);
    [panel addSubview:posterSlider];
    [self registerControllerFocusableControl:posterSlider];

    self.posterSizeValueLabel = OpnLabel([NSString stringWithFormat:@"%.0f%%", posterSlider.doubleValue],
                                          NSMakeRect(controlX + MIN(312.0, controlWidth - 60.0), 756.0, 60.0, 22.0),
                                         12.0,
                                         OpnColor(kTextSecondary),
                                         NSFontWeightSemibold,
                                         NSTextAlignmentRight);
    [panel addSubview:self.posterSizeValueLabel];

    [panel addSubview:[self rowLabel:@"Controller Grid Size" y:830.0]];
    NSSlider *controllerGridSlider = [[NSSlider alloc] initWithFrame:NSMakeRect(controlX, 824.0, MIN(300.0, controlWidth - 72.0), 28.0)];
    controllerGridSlider.minValue = 80.0;
    controllerGridSlider.maxValue = 140.0;
    controllerGridSlider.doubleValue = OpnControllerGridItemScale() * 100.0;
    controllerGridSlider.continuous = YES;
    controllerGridSlider.target = self;
    controllerGridSlider.action = @selector(controllerGridSizeSliderChanged:);
    [panel addSubview:controllerGridSlider];
    [self registerControllerFocusableControl:controllerGridSlider];

    self.controllerGridSizeValueLabel = OpnLabel([NSString stringWithFormat:@"%.0f%%", controllerGridSlider.doubleValue],
                                                 NSMakeRect(controlX + MIN(312.0, controlWidth - 60.0), 828.0, 60.0, 22.0),
                                                 12.0,
                                                 OpnColor(kTextSecondary),
                                                 NSFontWeightSemibold,
                                                 NSTextAlignmentRight);
    [panel addSubview:self.controllerGridSizeValueLabel];

    [panel addSubview:[self rowLabel:@"Auto Full Screen" y:896.0]];
    NSButton *autoFullScreenToggle = [[NSButton alloc] initWithFrame:NSMakeRect(controlX, 888.0, controlWidth, 28.0)];
    autoFullScreenToggle.buttonType = NSButtonTypeSwitch;
    autoFullScreenToggle.title = @"Enter full screen automatically when a stream starts";
    autoFullScreenToggle.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
    autoFullScreenToggle.contentTintColor = OpnColor(kBrandGreen);
    autoFullScreenToggle.state = OpnAutoFullScreenEnabled() ? NSControlStateValueOn : NSControlStateValueOff;
    autoFullScreenToggle.target = self;
    autoFullScreenToggle.action = @selector(autoFullScreenToggleChanged:);
    [panel addSubview:autoFullScreenToggle];
    [self registerControllerFocusableControl:autoFullScreenToggle];

    [self.documentView addSubview:panel];
}

- (void)buildSimpleSectionContent:(NSString *)section {
    NSView *panel = [self panelWithTitle:section height:220.0];
    NSDictionary<NSString *, NSString *> *messages = @{
        @"Input": @"Keyboard, mouse, and gamepad input are detected automatically while streaming.",
        @"Interface": @"OpenNOW adapts spacing as the window resizes.",
        @"About": @"OpenNOW is an open-source macOS client for launching and streaming cloud games.",
        @"Thanks": @"Thanks to the open-source projects and contributors that make this client possible.",
    };
    NSString *message = messages[section] ?: @"Settings are managed automatically for this section.";
    NSTextField *label = OpnLabel(message, NSMakeRect(24, 104, 560, 44), 14.0, OpnColor(kTextSecondary), NSFontWeightRegular);
    label.maximumNumberOfLines = 2;
    [panel addSubview:label];
    [self.documentView addSubview:panel];
}

- (NSTextField *)rowLabel:(NSString *)text y:(CGFloat)y {
    return OpnLabel(text, NSMakeRect(24, y, 160, 24), 14.0, OpnColor(kTextSecondary), NSFontWeightMedium);
}

- (CGFloat)controlXForPanelWidth:(CGFloat)panelWidth {
    return panelWidth < 620.0 ? 150.0 : 220.0;
}

- (CGFloat)controlWidthForPanelWidth:(CGFloat)panelWidth {
    CGFloat controlX = [self controlXForPanelWidth:panelWidth];
    return MAX(120.0, panelWidth - controlX - 24.0);
}

- (NSView *)selectField:(NSString *)title detail:(NSString *)detail frame:(NSRect)frame {
    NSView *view = [[OPNSettingsFlippedView alloc] initWithFrame:frame];
    view.wantsLayer = YES;
    view.layer.backgroundColor = OpnColor(0x090A0C, 0.72).CGColor;
    view.layer.cornerRadius = 11.0;
    view.layer.borderWidth = 1.0;
    view.layer.borderColor = OpnColor(kPanelBorder, 0.78).CGColor;
    [view addSubview:OpnLabel(title, NSMakeRect(14, 11, NSWidth(frame) - 138, 20), 14.0, OpnColor(kTextPrimary), NSFontWeightRegular)];
    if (detail.length > 0) {
        NSTextField *detailLabel = OpnLabel(detail, NSMakeRect(NSWidth(frame) - 104, 11, 62, 20), 12.0, OpnColor(kErrorRed), NSFontWeightSemibold, NSTextAlignmentCenter);
        detailLabel.wantsLayer = YES;
        detailLabel.layer.backgroundColor = OpnColor(kErrorRed, 0.12).CGColor;
        detailLabel.layer.cornerRadius = 6.0;
        [view addSubview:detailLabel];
    }
    [view addSubview:OpnLabel(@"v", NSMakeRect(NSWidth(frame) - 30, 10, 16, 20), 13.0, OpnColor(kTextMuted), NSFontWeightRegular, NSTextAlignmentCenter)];
    return view;
}

- (NSPopUpButton *)resolutionPopupWithFrame:(NSRect)frame {
    OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
    std::vector<OPN::StreamResolutionOption> resolutions = OPN::StreamResolutionOptionsForAspect(profile.aspectIndex);

    NSPopUpButton *popup = [[NSPopUpButton alloc] initWithFrame:frame pullsDown:NO];
    popup.target = self;
    popup.action = @selector(resolutionPopupChanged:);
    popup.bordered = NO;
    popup.font = [NSFont systemFontOfSize:14.0 weight:NSFontWeightRegular];
    popup.contentTintColor = OpnColor(kTextPrimary);
    popup.wantsLayer = YES;
    popup.layer.backgroundColor = OpnColor(0x090A0C, 0.72).CGColor;
    popup.layer.cornerRadius = 11.0;
    popup.layer.borderWidth = 1.0;
    popup.layer.borderColor = OpnColor(kPanelBorder, 0.78).CGColor;
    [popup removeAllItems];
    for (const OPN::StreamResolutionOption &resolution : resolutions) {
        [popup addItemWithTitle:[NSString stringWithUTF8String:resolution.Label().c_str()]];
    }
    if (!resolutions.empty()) {
        [popup selectItemAtIndex:MAX(0, MIN((NSInteger)profile.resolutionIndex, (NSInteger)resolutions.size() - 1))];
    }
    return popup;
}

- (NSPopUpButton *)regionPopupWithFrame:(NSRect)frame {
    std::vector<OPN::StreamRegionOption> regions = OPN::LoadCachedStreamRegions();
    std::string selectedUrl = OPN::LoadSelectedStreamRegionUrl();
    NSPopUpButton *popup = [[NSPopUpButton alloc] initWithFrame:frame pullsDown:NO];
    popup.target = self;
    popup.action = @selector(regionPopupChanged:);
    popup.bordered = NO;
    popup.font = [NSFont systemFontOfSize:14.0 weight:NSFontWeightRegular];
    popup.contentTintColor = OpnColor(kTextPrimary);
    popup.wantsLayer = YES;
    popup.layer.backgroundColor = OpnColor(0x090A0C, 0.72).CGColor;
    popup.layer.cornerRadius = 11.0;
    popup.layer.borderWidth = 1.0;
    popup.layer.borderColor = OpnColor(kPanelBorder, 0.78).CGColor;
    [popup removeAllItems];
    [popup addItemWithTitle:@"Automatic (lowest latency)"];
    NSInteger selectedIndex = 0;
    for (size_t i = 0; i < regions.size(); i++) {
        [popup addItemWithTitle:[NSString stringWithUTF8String:regions[i].Label().c_str()]];
        if (!selectedUrl.empty() && regions[i].url == selectedUrl) {
            selectedIndex = (NSInteger)i + 1;
        }
    }
    if (regions.empty()) {
        [popup addItemWithTitle:@"Discovering regions..."];
        [[popup itemAtIndex:1] setEnabled:NO];
    }
    [popup selectItemAtIndex:selectedIndex];
    self.selectedRegion = selectedIndex;
    return popup;
}

- (void)addOptionGroupTo:(NSView *)parent
                   group:(NSInteger)group
                  titles:(NSArray<NSString *> *)titles
                selected:(NSInteger)selected
                       y:(CGFloat)y
                   widths:(NSArray<NSNumber *> *)widths {
    CGFloat panelWidth = MAX(320.0, NSWidth(parent.frame));
    CGFloat x = [self controlXForPanelWidth:panelWidth];
    CGFloat availableWidth = MAX(80.0, panelWidth - x - 24.0);
    CGFloat requestedWidth = 0;
    for (NSNumber *width in widths) {
        requestedWidth += width.doubleValue;
    }
    requestedWidth += MAX(0, (NSInteger)titles.count - 1) * 8.0;
    CGFloat scale = requestedWidth > availableWidth ? availableWidth / requestedWidth : 1.0;
    for (NSUInteger i = 0; i < titles.count; i++) {
        CGFloat width = MAX(48.0, floor(widths[i].doubleValue * scale));
        NSButton *button = [[NSButton alloc] initWithFrame:NSMakeRect(x, y, width, 38.0)];
        button.title = titles[i];
        button.tag = group * 100 + (NSInteger)i;
        button.target = self;
        button.action = @selector(optionClicked:);
        button.bordered = NO;
        button.wantsLayer = YES;
        [self styleOptionButton:button selected:(NSInteger)i == selected];
        [parent addSubview:button];
        x += width + 8.0;
    }
}

- (void)styleOptionButton:(NSButton *)button selected:(BOOL)selected {
    button.font = [NSFont systemFontOfSize:13.0 weight:selected ? NSFontWeightSemibold : NSFontWeightRegular];
    button.contentTintColor = selected ? OpnColor(kBrandGreen) : OpnColor(kTextMuted);
    button.layer.cornerRadius = 10.0;
    button.layer.borderWidth = 1.0;
    button.layer.borderColor = (selected ? OpnColor(kBrandGreen, 0.50) : OpnColor(kPanelBorder, 0.72)).CGColor;
    button.layer.backgroundColor = (selected ? OpnColor(kBrandGreen, 0.16) : OpnColor(kInputBackground, 0.58)).CGColor;
}

- (void)optionClicked:(NSButton *)sender {
    NSInteger group = sender.tag / 100;
    NSInteger index = sender.tag % 100;
    switch (group) {
        case 1: OPN::SaveStreamAspectIndex((int)index); break;
        case 3: OPN::SaveStreamFpsIndex((int)index); break;
        case 4: OPN::SaveStreamCodecIndex((int)index); break;
        case 7: OPN::SaveStreamColorQualityIndex((int)index); break;
        case 8: OPN::SaveStreamBitrateIndex((int)index); break;
        case 9: [self applyPerformanceProfile:index]; break;
        case 10: OPN::SaveStreamRendererPacingIndex((int)index); break;
        default: break;
    }
    OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
    self.selectedAspect = profile.aspectIndex;
    self.selectedResolution = profile.resolutionIndex;
    self.selectedFps = profile.fpsIndex;
    self.selectedCodec = profile.codecIndex;
    self.selectedBitrate = profile.bitrateIndex;
    self.selectedColorDepth = profile.colorQualityIndex;
    self.selectedRendererPacing = profile.rendererPacingIndex;
    self.enableL4S = profile.enableL4S;
    [self rebuildContent];
}

- (void)applyPerformanceProfile:(NSInteger)index {
    switch (index) {
        case 0:
            OPN::SaveStreamCodecIndex(0);
            OPN::SaveStreamFpsIndex(1);
            OPN::SaveStreamBitrateIndex(2);
            OPN::SaveStreamPowerSaverEnabled(false);
            OPN::SaveStreamL4SEnabled(false);
            break;
        case 1:
            OPN::SaveStreamCodecIndex(1);
            OPN::SaveStreamFpsIndex(1);
            OPN::SaveStreamBitrateIndex(4);
            OPN::SaveStreamPowerSaverEnabled(false);
            OPN::SaveStreamL4SEnabled(false);
            break;
        case 2:
            break;
        default:
            break;
    }
}

- (void)resolutionPopupChanged:(NSPopUpButton *)sender {
    OPN::SaveStreamResolutionIndex((int)sender.indexOfSelectedItem);
    OPN::StreamPreferenceProfile profile = OPN::LoadStreamPreferenceProfile();
    self.selectedResolution = profile.resolutionIndex;
    [self rebuildContent];
}

- (void)regionPopupChanged:(NSPopUpButton *)sender {
    std::vector<OPN::StreamRegionOption> regions = OPN::LoadCachedStreamRegions();
    NSInteger index = sender.indexOfSelectedItem;
    if (index <= 0) {
        OPN::SaveSelectedStreamRegionUrl("");
    } else {
        size_t regionIndex = (size_t)(index - 1);
        if (regionIndex < regions.size()) {
            OPN::SaveSelectedStreamRegionUrl(regions[regionIndex].url);
        }
    }
    OPN::GameService::Shared().SetStreamingBaseUrl(OPN::LoadSelectedStreamingBaseUrl());
    self.selectedRegion = index;
    [self rebuildContent];
}

- (void)l4sToggleChanged:(NSButton *)sender {
    self.enableL4S = sender.state == NSControlStateValueOn;
    OPN::SaveStreamL4SEnabled(self.enableL4S);
    [self rebuildContent];
}

- (void)suppressInputWhenInactiveToggleChanged:(NSButton *)sender {
    self.suppressInputWhenInactive = sender.state == NSControlStateValueOn;
    OPN::SaveStreamSuppressInputWhenInactive(self.suppressInputWhenInactive);
    [self rebuildContent];
}

- (void)posterSizeSliderChanged:(NSSlider *)sender {
    OpnSetPosterSizeScale((CGFloat)sender.doubleValue / 100.0);
    self.posterSizeValueLabel.stringValue = [NSString stringWithFormat:@"%.0f%%", sender.doubleValue];
}

- (void)controllerGridSizeSliderChanged:(NSSlider *)sender {
    OpnSetControllerGridItemScale((CGFloat)sender.doubleValue / 100.0);
    self.controllerGridSizeValueLabel.stringValue = [NSString stringWithFormat:@"%.0f%%", sender.doubleValue];
}

- (void)accentColorSliderChanged:(NSSlider *)sender {
    unsigned accent = OpnCurrentAccentRGB();
    NSInteger red = (NSInteger)((accent >> 16) & 0xFF);
    NSInteger green = (NSInteger)((accent >> 8) & 0xFF);
    NSInteger blue = (NSInteger)(accent & 0xFF);
    NSInteger value = MAX(0, MIN(sender.integerValue, 255));
    if (sender.tag == 0) red = value;
    if (sender.tag == 1) green = value;
    if (sender.tag == 2) blue = value;
    unsigned next = ((unsigned)red << 16) | ((unsigned)green << 8) | (unsigned)blue;
    OpnSetCurrentAccentRGB(next);
    self.accentRedValueLabel.stringValue = [NSString stringWithFormat:@"%ld", (long)red];
    self.accentGreenValueLabel.stringValue = [NSString stringWithFormat:@"%ld", (long)green];
    self.accentBlueValueLabel.stringValue = [NSString stringWithFormat:@"%ld", (long)blue];
}

- (void)autoFullScreenToggleChanged:(NSButton *)sender {
    OpnSetAutoFullScreenEnabled(sender.state == NSControlStateValueOn);
}

- (void)backgroundAnimationToggleChanged:(NSButton *)sender {
    OpnSetBackgroundAnimationEnabled(sender.state == NSControlStateValueOn);
}

- (void)derivedAccentToggleChanged:(NSButton *)sender {
    OpnSetDerivedAccentColorsEnabled(sender.state == NSControlStateValueOn);
}

- (void)backgroundTintSliderChanged:(NSSlider *)sender {
    OpnSetBackgroundTintStrength((CGFloat)sender.doubleValue / 100.0);
    self.backgroundTintValueLabel.stringValue = [NSString stringWithFormat:@"%.0f%%", sender.doubleValue];
}

- (void)controllerModeToggleChanged:(NSButton *)sender {
    OpnSetControllerModeEnabled(sender.state == NSControlStateValueOn);
    [self rebuildContent];
}

- (void)keyDown:(NSEvent *)event {
    if (!OpnControllerModeEnabled()) {
        [super keyDown:event];
        return;
    }
    switch (event.keyCode) {
        case 123: [self adjustControllerFocusedSliderBy:-1]; return;
        case 124: [self adjustControllerFocusedSliderBy:1]; return;
        case 125: [self moveControllerFocusBy:1]; return;
        case 126: [self moveControllerFocusBy:-1]; return;
        case 36:
        case 49:
            [self activateControllerFocusedControl];
            return;
        case 53:
            if (self.onBackRequested) {
                OpnPlayConsoleTone(OPNConsoleToneBack);
                self.onBackRequested();
                return;
            }
            break;
        default:
            break;
    }
    [super keyDown:event];
}

- (void)startGamepadNavigationIfNeeded {
    if (!OpnControllerModeEnabled() || self.gamepadNavigationTimer || !OPNSettingsGamepadNavigationActive(self)) return;
    self.gamepadNavigationTimer = [NSTimer scheduledTimerWithTimeInterval:(1.0 / 30.0)
                                                                   target:self
                                                                 selector:@selector(pollGamepadNavigation)
                                                                 userInfo:nil
                                                                  repeats:YES];
}

- (void)stopGamepadNavigation {
    [self.gamepadNavigationTimer invalidate];
    self.gamepadNavigationTimer = nil;
    self.previousGamepadButtons = 0;
}

- (void)pollGamepadNavigation {
    if (!OpnControllerModeEnabled() || !OPNSettingsGamepadNavigationActive(self)) {
        [self stopGamepadNavigation];
        return;
    }
    [self.window makeFirstResponder:self];
    uint16_t buttons = OPNSettingsGamepadButtons();
    uint16_t pressed = buttons & (uint16_t)~self.previousGamepadButtons;
    CFTimeInterval now = CACurrentMediaTime();
    BOOL repeatMove = (now - self.lastGamepadMoveTime) > 0.22;
    uint16_t moves = buttons & ((1u << 2) | (1u << 3) | (1u << 4) | (1u << 5));
    if (moves && repeatMove) {
        pressed |= moves;
        self.lastGamepadMoveTime = now;
    }
    if (pressed & (1u << 0)) [self activateControllerFocusedControl];
    if (pressed & (1u << 1)) {
        if (self.onBackRequested) {
            OpnPlayConsoleTone(OPNConsoleToneBack);
            self.onBackRequested();
        }
    }
    if (pressed & (1u << 2)) [self moveControllerFocusBy:-1];
    if (pressed & (1u << 3)) [self moveControllerFocusBy:1];
    if (pressed & (1u << 4)) [self adjustControllerFocusedSliderBy:-1];
    if (pressed & (1u << 5)) [self adjustControllerFocusedSliderBy:1];
    self.previousGamepadButtons = buttons;
}

- (void)controllerShortcutCaptureClicked:(NSButton *)sender {
    (void)sender;
    [self.controllerShortcutCaptureTimer invalidate];
    self.controllerShortcutPendingMask = 0;
    self.controllerShortcutPendingSince = 0;
    self.controllerShortcutCaptureDeadline = CACurrentMediaTime() + 6.0;
    self.controllerShortcutCaptureButton.title = @"Listening...";
    self.controllerShortcutCaptureButton.layer.borderColor = OpnColor(kBrandGreen, 0.70).CGColor;
    self.controllerShortcutStatusLabel.stringValue = @"Press and hold the controller button or combo you want to use for opening the stream library.";
    self.controllerShortcutCaptureTimer = [NSTimer scheduledTimerWithTimeInterval:(1.0 / 60.0)
                                                                           target:self
                                                                         selector:@selector(controllerShortcutCaptureTimerFired:)
                                                                         userInfo:nil
                                                                          repeats:YES];
}

- (void)controllerShortcutResetClicked:(NSButton *)sender {
    (void)sender;
    [self.controllerShortcutCaptureTimer invalidate];
    self.controllerShortcutCaptureTimer = nil;
    OpnSetControllerLibraryShortcutMask((uint16_t)(OPN::Input::GAMEPAD_BACK | OPN::Input::GAMEPAD_START));
    self.controllerShortcutCaptureButton.title = OPNControllerShortcutLabel(OpnControllerLibraryShortcutMask());
    self.controllerShortcutCaptureButton.layer.borderColor = OpnColor(kBrandGreen, 0.38).CGColor;
    self.controllerShortcutStatusLabel.stringValue = @"Default restored. While streaming, press View + Menu to open the controller library.";
}

- (void)controllerShortcutCaptureTimerFired:(NSTimer *)timer {
    if (timer != self.controllerShortcutCaptureTimer) return;

    CFTimeInterval now = CACurrentMediaTime();
    if (now >= self.controllerShortcutCaptureDeadline) {
        [self.controllerShortcutCaptureTimer invalidate];
        self.controllerShortcutCaptureTimer = nil;
        self.controllerShortcutCaptureButton.title = OPNControllerShortcutLabel(OpnControllerLibraryShortcutMask());
        self.controllerShortcutCaptureButton.layer.borderColor = OpnColor(kBrandGreen, 0.38).CGColor;
        self.controllerShortcutStatusLabel.stringValue = @"No controller input detected. Connect a controller and try again.";
        return;
    }

    uint16_t mask = OPNPressedControllerShortcutMask();
    if (mask == 0) {
        self.controllerShortcutPendingMask = 0;
        self.controllerShortcutPendingSince = 0;
        return;
    }

    if (mask != self.controllerShortcutPendingMask) {
        self.controllerShortcutPendingMask = mask;
        self.controllerShortcutPendingSince = now;
        self.controllerShortcutCaptureButton.title = OPNControllerShortcutLabel(mask);
        self.controllerShortcutStatusLabel.stringValue = @"Hold briefly to save this combo.";
        return;
    }

    if (now - self.controllerShortcutPendingSince < 0.24) return;
    [self.controllerShortcutCaptureTimer invalidate];
    self.controllerShortcutCaptureTimer = nil;
    OpnSetControllerLibraryShortcutMask(mask);
    self.controllerShortcutCaptureButton.title = OPNControllerShortcutLabel(mask);
    self.controllerShortcutCaptureButton.layer.borderColor = OpnColor(kBrandGreen, 0.38).CGColor;
    self.controllerShortcutStatusLabel.stringValue = [NSString stringWithFormat:@"Saved. While streaming, press %@ to open the controller library.", OPNControllerShortcutLabel(mask)];
}

- (void)microphoneModePopupChanged:(NSPopUpButton *)sender {
    std::vector<OPN::StreamMicrophoneModeOption> modes = OPN::StreamMicrophoneModeOptions();
    NSInteger index = MAX(0, MIN(sender.indexOfSelectedItem, (NSInteger)modes.size() - 1));
    if (!modes.empty()) {
        OPN::SaveStreamMicrophoneMode(modes[(size_t)index].value);
    }
    self.selectedMicrophoneMode = index;
    [self rebuildContent];
}

- (void)microphoneDevicePopupChanged:(NSPopUpButton *)sender {
    std::vector<OPN::StreamMicrophoneDeviceOption> devices = OPN::LoadMicrophoneDeviceOptions();
    NSInteger index = MAX(0, MIN(sender.indexOfSelectedItem, (NSInteger)devices.size() - 1));
    if (!devices.empty()) {
        OPN::SaveStreamMicrophoneDeviceId(devices[(size_t)index].uniqueId);
    }
    self.selectedMicrophoneDevice = index;
    [self rebuildContent];
}

@end
