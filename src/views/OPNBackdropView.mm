#import "OPNBackdropView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#import "../common/OPNAuthTypes.h"
#import <GameController/GameController.h>
#include <cmath>

@interface OPNBackdropControllerMenuView : NSView
@end

@implementation OPNBackdropControllerMenuView
- (BOOL)isFlipped { return YES; }
@end

static BOOL OPNBackdropControllerNavigationActive(NSView *view) {
    NSWindow *window = view.window;
    if (!window || window.contentViewController != nil) return NO;
    return window.contentView == view || [view isDescendantOf:window.contentView];
}

static unsigned OPNControllerAccentRGB(void) {
    return OpnCurrentAccentRGB();
}

static unsigned OPNControllerAccentSoftRGB(void) {
    return OpnBlendRGB(OpnCurrentAccentRGB(), 0xFFFFFF, 0.42);
}

static unsigned OPNControllerAccentBlackRGB(CGFloat blackMix) {
    return OpnBlendRGB(OpnCurrentAccentRGB(), 0x000000, blackMix);
}

static NSImage *OPNHeaderLogoImage(void) {
    static NSImage *logo = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        NSString *bundlePath = [NSBundle.mainBundle pathForResource:@"logo" ofType:@"png"];
        NSString *relativePath = @"assets/logo.png";
        logo = [[NSImage alloc] initWithContentsOfFile:bundlePath ?: relativePath];
    });
    return logo;
}

static NSString *OPNCurrentHeaderTimeText(void) {
    NSDateFormatter *timeFormatter = [[NSDateFormatter alloc] init];
    timeFormatter.dateFormat = @"h:mm a";
    return [[timeFormatter stringFromDate:NSDate.date] uppercaseString];
}

@implementation OPNBackdropView {
    NSRect _storeNavFrame;
    NSRect _libraryNavFrame;
    NSRect _settingsNavFrame;
    NSRect _accountFrame;
    NSButton *_storeButton;
    NSButton *_libraryButton;
    NSButton *_settingsButton;
    NSButton *_accountButton;
    NSView *_controllerAccountMenuView;
    NSTimer *_controllerNavigationTimer;
    NSTimer *_backgroundAnimationTimer;
    CFTimeInterval _backgroundAnimationStartTime;
    unsigned _controllerAccentRGB;
    uint16_t _previousControllerButtons;
}

static NSAttributedString *OPNMenuTitle(NSString *title, NSColor *color, NSFontWeight weight) {
    return [[NSAttributedString alloc] initWithString:title attributes:@{
        NSFontAttributeName: [NSFont systemFontOfSize:13.0 weight:weight],
        NSForegroundColorAttributeName: color,
    }];
}

static NSMenuItem *OPNStyledMenuItem(NSString *title, SEL action, id target, NSColor *color, NSFontWeight weight) {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:@""];
    item.target = target;
    item.attributedTitle = OPNMenuTitle(title, color, weight);
    return item;
}

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        _storeButton = [self navigationHitButtonWithAction:@selector(storeButtonPressed:)];
        _libraryButton = [self navigationHitButtonWithAction:@selector(libraryButtonPressed:)];
        _settingsButton = [self navigationHitButtonWithAction:@selector(settingsButtonPressed:)];
        _accountButton = [self navigationHitButtonWithAction:@selector(accountButtonPressed:)];
        _controllerAccentRGB = OPNControllerAccentRGB();
        _backgroundAnimationStartTime = CACurrentMediaTime();
        [self addSubview:_storeButton];
        [self addSubview:_libraryButton];
        [self addSubview:_settingsButton];
        [self addSubview:_accountButton];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(interfacePreferencesChanged:)
                                                     name:OPNInterfacePreferencesDidChangeNotification
                                                   object:nil];
        [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(controllerDidConnect:) name:GCControllerDidConnectNotification object:nil];
        [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(controllerDidDisconnect:) name:GCControllerDidDisconnectNotification object:nil];
        [self startControllerNavigationIfNeeded];
    }
    return self;
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [_controllerNavigationTimer invalidate];
    [_backgroundAnimationTimer invalidate];
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    if (self.window) {
        [self startControllerNavigationIfNeeded];
        [self startControllerBackgroundAnimationIfNeeded];
    } else {
        [_controllerNavigationTimer invalidate];
        _controllerNavigationTimer = nil;
        [_backgroundAnimationTimer invalidate];
        _backgroundAnimationTimer = nil;
        _previousControllerButtons = 0;
    }
}

- (void)interfacePreferencesChanged:(NSNotification *)notification {
    (void)notification;
    if (!OpnBackgroundAnimationEnabled()) {
        [_backgroundAnimationTimer invalidate];
        _backgroundAnimationTimer = nil;
    }
    [self setNeedsDisplay:YES];
    [self startControllerNavigationIfNeeded];
    [self startControllerBackgroundAnimationIfNeeded];
}

- (void)setControllerAccentRGB:(unsigned)controllerAccentRGB {
    controllerAccentRGB &= 0xFFFFFF;
    if (_controllerAccentRGB == controllerAccentRGB) return;
    _controllerAccentRGB = controllerAccentRGB;
    [self setNeedsDisplay:YES];
}

- (unsigned)resolvedControllerAccentRGB {
    return _controllerAccentRGB ? _controllerAccentRGB : OPNControllerAccentRGB();
}

- (unsigned)resolvedControllerAccentSoftRGB {
    return OpnBlendRGB([self resolvedControllerAccentRGB], 0xFFFFFF, 0.42);
}

- (unsigned)resolvedControllerAccentBlackRGB:(CGFloat)blackMix {
    return OpnBlendRGB([self resolvedControllerAccentRGB], 0x000000, blackMix);
}

- (void)startControllerBackgroundAnimationIfNeeded {
    if (!OpnControllerModeEnabled() || !OpnBackgroundAnimationEnabled() || _backgroundAnimationTimer || !self.window) return;
    _backgroundAnimationTimer = [NSTimer timerWithTimeInterval:(1.0 / 60.0)
                                                        target:self
                                                      selector:@selector(backgroundAnimationTick:)
                                                      userInfo:nil
                                                       repeats:YES];
    [NSRunLoop.mainRunLoop addTimer:_backgroundAnimationTimer forMode:NSRunLoopCommonModes];
}

- (void)backgroundAnimationTick:(NSTimer *)timer {
    (void)timer;
    if (!OpnControllerModeEnabled() || !OpnBackgroundAnimationEnabled() || !self.window) {
        [_backgroundAnimationTimer invalidate];
        _backgroundAnimationTimer = nil;
        return;
    }
    [self setNeedsDisplay:YES];
}

- (void)startControllerNavigationIfNeeded {
    if (!OpnControllerModeEnabled() || _controllerNavigationTimer || !OPNBackdropControllerNavigationActive(self)) return;
    _controllerNavigationTimer = [NSTimer scheduledTimerWithTimeInterval:(1.0 / 30.0)
                                                                   target:self
                                                                selector:@selector(pollControllerNavigation)
                                                                userInfo:nil
                                                                 repeats:YES];
}

- (void)controllerDidConnect:(NSNotification *)notification {
    (void)notification;
    [self startControllerNavigationIfNeeded];
}

- (void)controllerDidDisconnect:(NSNotification *)notification {
    (void)notification;
    _previousControllerButtons = 0;
}

- (uint16_t)currentControllerNavigationButtons {
    NSArray<GCController *> *controllers = [GCController controllers];
    if (controllers.count == 0) return 0;
    GCExtendedGamepad *pad = controllers.firstObject.extendedGamepad;
    if (!pad) return 0;
    uint16_t buttons = 0;
    if (pad.leftShoulder.value > 0.5) buttons |= 1u << 0;
    if (pad.rightShoulder.value > 0.5) buttons |= 1u << 1;
    return buttons;
}

- (void)selectPreviousControllerTab {
    if (self.mode == OPNBackdropModeSettings) {
        OpnPlayConsoleTone(OPNConsoleToneMove);
        if (self.onLibrarySelected) self.onLibrarySelected();
    }
}

- (void)selectNextControllerTab {
    if (self.mode == OPNBackdropModeLibrary) {
        OpnPlayConsoleTone(OPNConsoleToneMove);
        if (self.onSettingsSelected) self.onSettingsSelected();
    }
}

- (void)pollControllerNavigation {
    if (!OpnControllerModeEnabled() || !OPNBackdropControllerNavigationActive(self)) {
        [_controllerNavigationTimer invalidate];
        _controllerNavigationTimer = nil;
        _previousControllerButtons = 0;
        return;
    }
    uint16_t buttons = [self currentControllerNavigationButtons];
    uint16_t pressed = buttons & (uint16_t)~_previousControllerButtons;
    if (pressed & (1u << 0)) [self selectPreviousControllerTab];
    if (pressed & (1u << 1)) [self selectNextControllerTab];
    _previousControllerButtons = buttons;
}

- (NSButton *)navigationHitButtonWithAction:(SEL)action {
    NSButton *button = [[NSButton alloc] initWithFrame:NSZeroRect];
    button.title = @"";
    button.bordered = NO;
    button.target = self;
    button.action = action;
    button.wantsLayer = YES;
    button.layer.backgroundColor = [NSColor clearColor].CGColor;
    return button;
}

- (BOOL)isFlipped { return YES; }

- (CGFloat)unitHashForSeed:(NSUInteger)seed index:(NSUInteger)index {
    uint32_t value = (uint32_t)(seed * 1103515245u + index * 12345u + 0x9E3779B9u);
    value ^= value >> 16;
    value *= 0x7FEB352Du;
    value ^= value >> 15;
    return (CGFloat)(value % 10000u) / 10000.0;
}

- (void)drawSparkleAtPoint:(NSPoint)point radius:(CGFloat)radius alpha:(CGFloat)alpha color:(NSColor *)color {
    NSBezierPath *cross = [NSBezierPath bezierPath];
    [cross moveToPoint:NSMakePoint(point.x - radius, point.y)];
    [cross lineToPoint:NSMakePoint(point.x + radius, point.y)];
    [cross moveToPoint:NSMakePoint(point.x, point.y - radius)];
    [cross lineToPoint:NSMakePoint(point.x, point.y + radius)];
    [cross moveToPoint:NSMakePoint(point.x - radius * 0.55, point.y - radius * 0.55)];
    [cross lineToPoint:NSMakePoint(point.x + radius * 0.55, point.y + radius * 0.55)];
    [cross moveToPoint:NSMakePoint(point.x - radius * 0.55, point.y + radius * 0.55)];
    [cross lineToPoint:NSMakePoint(point.x + radius * 0.55, point.y - radius * 0.55)];
    cross.lineCapStyle = NSLineCapStyleRound;
    cross.lineWidth = MAX(0.7, radius * 0.16);
    [[color colorWithAlphaComponent:alpha] setStroke];
    [cross stroke];

    NSBezierPath *core = [NSBezierPath bezierPathWithOvalInRect:NSMakeRect(point.x - radius * 0.16,
                                                                            point.y - radius * 0.16,
                                                                            radius * 0.32,
                                                                            radius * 0.32)];
    [[NSColor.whiteColor colorWithAlphaComponent:alpha * 0.72] setFill];
    [core fill];
}

- (void)drawControllerElectricBackgroundInRect:(NSRect)bounds {
    BOOL animationEnabled = OpnBackgroundAnimationEnabled();
    CGFloat phase = animationEnabled ? (CGFloat)(CACurrentMediaTime() - _backgroundAnimationStartTime) : 0.0;
    CGFloat tintStrength = OpnBackgroundTintStrength();
    CGFloat baseBlackA = 0.18 + 0.71 * tintStrength;
    CGFloat baseBlackB = 0.10 + 0.71 * tintStrength;
    CGFloat baseBlackC = 0.22 + 0.70 * tintStrength;
    CGFloat vignetteBlack = 0.30 + 0.67 * tintStrength;
    CGFloat vignetteAlpha = 0.04 + 0.26 * tintStrength;
    NSGradient *base = [[NSGradient alloc] initWithColors:@[
        OpnColor([self resolvedControllerAccentBlackRGB:baseBlackA], 1.0),
        OpnColor([self resolvedControllerAccentBlackRGB:baseBlackB], 1.0),
        OpnColor([self resolvedControllerAccentBlackRGB:baseBlackC], 1.0),
    ]];
    [base drawInRect:bounds angle:88.0];

    CGFloat width = NSWidth(bounds);
    CGFloat height = NSHeight(bounds);
    unsigned accentRGB = [self resolvedControllerAccentRGB];
    unsigned accentSoftRGB = [self resolvedControllerAccentSoftRGB];

    for (NSInteger band = 0; band < 9; band++) {
        CGFloat yBase = height * (0.12 + (CGFloat)band * 0.092);
        NSBezierPath *ribbon = [NSBezierPath bezierPath];
        [ribbon moveToPoint:NSMakePoint(-120.0, yBase)];
        for (NSInteger point = 0; point <= 28; point++) {
            CGFloat t = (CGFloat)point / 28.0;
            CGFloat x = t * (width + 240.0) - 120.0;
            CGFloat drift = phase * (0.28 + (CGFloat)band * 0.018);
            CGFloat y = yBase
                + sin(t * 5.8 + (CGFloat)band * 0.72 + drift) * (20.0 + (CGFloat)band * 1.6)
                + sin(t * 13.0 - phase * 0.20 + (CGFloat)band) * 5.0;
            [ribbon lineToPoint:NSMakePoint(x, y)];
        }
        NSColor *stroke = band % 3 == 0 ? OpnColor(accentSoftRGB, 0.032) : OpnColor(accentRGB, 0.035);
        [stroke setStroke];
        ribbon.lineWidth = band == 4 ? 2.4 : 1.1;
        [ribbon stroke];
    }

    CGFloat streamY = height * 0.46;
    CGFloat travelWidth = width + 180.0;
    for (NSInteger i = 0; i < 54; i++) {
        CGFloat seed = (CGFloat)i;
        CGFloat lane = ((NSInteger)i % 7) - 3.0;
        CGFloat speed = 38.0 + (CGFloat)(i % 5) * 8.0;
        CGFloat x = fmod(seed * 131.0 + phase * speed, MAX(1.0, travelWidth)) - 90.0;
        CGFloat y = streamY + lane * 9.0 + sin(phase * (1.0 + seed * 0.017) + seed * 0.71) * 8.0;
        CGFloat shimmer = 0.5 + 0.5 * sin(phase * (2.2 + (CGFloat)(i % 4) * 0.28) + seed);
        CGFloat radius = 2.0 + (CGFloat)(i % 4) * 0.75 + shimmer * 1.6;
        CGFloat alpha = 0.047 + shimmer * 0.142;
        NSColor *sparkleColor = i % 6 == 0 ? NSColor.whiteColor : OpnColor(accentSoftRGB);
        [self drawSparkleAtPoint:NSMakePoint(x, y) radius:radius alpha:alpha color:sparkleColor];
    }

    NSGradient *vignette = [[NSGradient alloc] initWithStartingColor:OpnColor([self resolvedControllerAccentBlackRGB:vignetteBlack], 0.0)
                                                        endingColor:OpnColor([self resolvedControllerAccentBlackRGB:vignetteBlack], vignetteAlpha)];
    [vignette drawInRect:bounds angle:-90.0];
}

- (void)setMode:(OPNBackdropMode)mode {
    _mode = mode;
    [self dismissControllerAccountMenu];
    [self startControllerBackgroundAnimationIfNeeded];
    [self setNeedsDisplay:YES];
}

- (void)setAccountName:(NSString *)accountName {
    _accountName = [accountName copy];
    [self setNeedsDisplay:YES];
}

- (void)setAccountStatus:(NSString *)accountStatus {
    _accountStatus = [accountStatus copy];
    [self setNeedsDisplay:YES];
}

- (void)setAccountAvatarImage:(NSImage *)accountAvatarImage {
    _accountAvatarImage = accountAvatarImage;
    [self setNeedsDisplay:YES];
}

- (void)setRemainingPlayTime:(NSString *)remainingPlayTime {
    _remainingPlayTime = [remainingPlayTime copy];
    [self setNeedsDisplay:YES];
}

- (void)setGameCountText:(NSString *)gameCountText {
    _gameCountText = [gameCountText copy];
    [self setNeedsDisplay:YES];
}

- (void)setAccountMenuItems:(NSArray<NSDictionary<NSString *,NSString *> *> *)accountMenuItems {
    _accountMenuItems = [accountMenuItems copy];
    [self dismissControllerAccountMenu];
}

- (void)setCurrentAccountIdentifier:(NSString *)currentAccountIdentifier {
    _currentAccountIdentifier = [currentAccountIdentifier copy];
    [self dismissControllerAccountMenu];
}

- (void)layout {
    [super layout];
    BOOL showNavigation = self.mode != OPNBackdropModeAuth;
    BOOL controllerMode = OpnControllerModeEnabled();
    if (controllerMode && showNavigation) {
        _storeNavFrame = NSZeroRect;
        _libraryNavFrame = NSMakeRect(30.0, 68.0, 82.0, 34.0);
        _settingsNavFrame = NSMakeRect(124.0, 68.0, 92.0, 34.0);
        _accountFrame = NSMakeRect(NSWidth(self.bounds) - 304.0, 10.0, 284.0, 92.0);
    }
    BOOL showStore = showNavigation && !controllerMode;
    _storeButton.frame = showStore && !NSEqualRects(_storeNavFrame, NSZeroRect) ? _storeNavFrame : NSZeroRect;
    _libraryButton.frame = showNavigation && !NSEqualRects(_libraryNavFrame, NSZeroRect) ? _libraryNavFrame : NSZeroRect;
    _settingsButton.frame = showNavigation && !NSEqualRects(_settingsNavFrame, NSZeroRect) ? _settingsNavFrame : NSZeroRect;
    _accountButton.frame = showNavigation && !NSEqualRects(_accountFrame, NSZeroRect) ? _accountFrame : NSZeroRect;
    _storeButton.hidden = !showStore;
    _libraryButton.hidden = !showNavigation;
    _settingsButton.hidden = !showNavigation;
    _accountButton.hidden = !showNavigation;
    if (_controllerAccountMenuView) {
        CGFloat menuWidth = 320.0;
        _controllerAccountMenuView.frame = NSMakeRect(MAX(20.0, NSWidth(self.bounds) - menuWidth - 20.0), 106.0, menuWidth, NSHeight(_controllerAccountMenuView.frame));
    }
}

- (void)drawRect:(NSRect)dirtyRect {
    [super drawRect:dirtyRect];

    using namespace OPN;

    NSRect bounds = self.bounds;
    BOOL controllerMode = OpnControllerModeEnabled();
    [OpnColor(kBackground) setFill];
    NSRectFill(bounds);

    if (controllerMode) {
        [self drawControllerElectricBackgroundInRect:bounds];
    } else {
        NSGradient *edgeWash = [[NSGradient alloc] initWithColors:@[
            OpnColor(kBackgroundB, 0.94),
            OpnColor(kBackground, 1.0),
            OpnColor(0x0C0D10, 1.0),
        ]];
        [edgeWash drawInRect:bounds angle:270.0];
    }

    if (!controllerMode) {
        NSGradient *spotlight = [[NSGradient alloc] initWithStartingColor:OpnColor(0xFFFFFF, 0.045)
                                                               endingColor:OpnColor(0xFFFFFF, 0.0)];
        NSRect spotlightRect = NSMakeRect(NSWidth(bounds) * 0.5 - 360.0, -300.0, 720.0, 720.0);
        [spotlight drawInBezierPath:[NSBezierPath bezierPathWithOvalInRect:spotlightRect] angle:90.0];

        NSBezierPath *lowerGlow = [NSBezierPath bezierPathWithOvalInRect:
            NSMakeRect(NSWidth(bounds) - 500.0, NSHeight(bounds) - 360.0, 520.0, 520.0)];
        [OpnColor(kBrandGreen, 0.045) setFill];
        [lowerGlow fill];
    }

    if (self.mode == OPNBackdropModeAuth) {
        return;
    }

    CGFloat navHeight = controllerMode ? 118.0 : 64.0;
    NSRect navRect = NSMakeRect(0, 0, NSWidth(bounds), navHeight);
    if (!controllerMode) {
        [OpnColor(0x1C1D21, 0.82) setFill];
        NSRectFill(navRect);
        [OpnColor(0xFFFFFF, 0.08) setFill];
        NSRectFill(NSMakeRect(0, navHeight - 1.0, NSWidth(bounds), 1));
    }

    NSImage *logo = OPNHeaderLogoImage();
    if (logo) {
        CGFloat logoHeight = controllerMode ? 42.0 : 30.0;
        CGFloat aspect = logo.size.height > 0 ? logo.size.width / logo.size.height : 1.0;
        CGFloat logoWidth = MIN(controllerMode ? 180.0 : 132.0, logoHeight * aspect);
        NSRect logoRect = controllerMode ? NSMakeRect(28.0, 18.0, logoWidth, logoHeight) : NSMakeRect(28.0, 17.0, logoWidth, logoHeight);
        [logo drawInRect:logoRect
                fromRect:NSZeroRect
               operation:NSCompositingOperationSourceOver
                fraction:1.0
          respectFlipped:YES
                   hints:@{NSImageHintInterpolation: @(NSImageInterpolationHigh)}];
    } else if (!controllerMode) {
        [@"OpenNOW" drawInRect:NSMakeRect(32.0, 21.0, 132, 22)
                withAttributes:OpnTextStyle(16.0, OpnColor(kTextPrimary), NSFontWeightSemibold)];
    }

    NSArray<NSString *> *items = controllerMode ? @[@"Games", @"Settings"] : @[@"Store", @"Library", @"Settings"];
    CGFloat widths[] = {82.0, 92.0, 78.0};
    CGFloat navWidth = controllerMode ? widths[0] + widths[1] + 10.0 : widths[2] + widths[0] + widths[1] + 8.0;
    CGFloat x = floor((NSWidth(bounds) - navWidth) / 2.0);
    _storeNavFrame = controllerMode ? NSZeroRect : _storeNavFrame;
    CGFloat navRowY = controllerMode ? 64.0 : 15.0;
    NSRect segmentedRect = NSMakeRect(x - 8.0, navRowY, navWidth + 16.0, controllerMode ? 42.0 : 34.0);
    NSBezierPath *segmented = [NSBezierPath bezierPathWithRoundedRect:segmentedRect xRadius:controllerMode ? 21.0 : 10.0 yRadius:controllerMode ? 21.0 : 10.0];
    [controllerMode ? OpnColor([self resolvedControllerAccentRGB], 0.055) : OpnColor(0xFFFFFF, 0.055) setFill];
    [segmented fill];
    if (controllerMode) {
        [OpnColor(0xFFFFFF, 0.18) setStroke];
        segmented.lineWidth = 1.0;
        [segmented stroke];
    }
    for (NSUInteger i = 0; i < items.count; i++) {
        NSString *item = items[i];
        CGFloat itemWidth = [item isEqualToString:@"Store"] ? widths[2] : (([item isEqualToString:@"Library"] || [item isEqualToString:@"Games"]) ? widths[0] : widths[1]);
        BOOL active = ([item isEqualToString:@"Store"] && self.mode == OPNBackdropModeStore) ||
                      (([item isEqualToString:@"Library"] || [item isEqualToString:@"Games"]) && self.mode == OPNBackdropModeLibrary) ||
                      ([item isEqualToString:@"Settings"] && self.mode == OPNBackdropModeSettings);
        NSRect itemRect = NSMakeRect(x, controllerMode ? 68.0 : 18.0, itemWidth, controllerMode ? 34.0 : 28.0);
        if ([item isEqualToString:@"Store"]) _storeNavFrame = itemRect;
        if ([item isEqualToString:@"Library"] || [item isEqualToString:@"Games"]) _libraryNavFrame = itemRect;
        if ([item isEqualToString:@"Settings"]) _settingsNavFrame = itemRect;
        if (active) {
            NSBezierPath *pill = [NSBezierPath bezierPathWithRoundedRect:itemRect xRadius:controllerMode ? 17.0 : 8.0 yRadius:controllerMode ? 17.0 : 8.0];
            [controllerMode ? OpnColor([self resolvedControllerAccentRGB], 0.20) : OpnColor(0xFFFFFF, 0.14) setFill];
            [pill fill];
            if (controllerMode) {
                [OpnColor([self resolvedControllerAccentSoftRGB], 0.66) setStroke];
                pill.lineWidth = 1.0;
                [pill stroke];
            }
        }
        NSColor *textColor = active ? OpnColor(kTextPrimary) : OpnColor(kTextMuted);
        NSMutableParagraphStyle *style = [[NSMutableParagraphStyle alloc] init];
        style.alignment = NSTextAlignmentCenter;
        NSMutableDictionary<NSAttributedStringKey, id> *attrs = [OpnTextStyle(controllerMode ? 18 : 13, textColor, active ? NSFontWeightSemibold : NSFontWeightRegular) mutableCopy];
        attrs[NSParagraphStyleAttributeName] = style;
        [item drawInRect:NSInsetRect(itemRect, 0, controllerMode ? 8.0 : 6.0) withAttributes:attrs];
        x += itemWidth + (controllerMode ? 10.0 : 4.0);
    }

    NSString *remaining = self.remainingPlayTime.length > 0 ? self.remainingPlayTime : @"--";
    CGFloat controllerStatsWidth = 292.0;
    CGFloat controllerStatsX = MAX(NSMaxX(segmentedRect) + 18.0, NSWidth(bounds) - controllerStatsWidth - 28.0);
    NSRect planRect = controllerMode ? NSMakeRect(controllerStatsX, 72.0, 132.0, 26.0) : NSMakeRect(NSWidth(bounds) - 294, 11.0, 108, 26);
    NSBezierPath *planPill = [NSBezierPath bezierPathWithRoundedRect:planRect xRadius:14 yRadius:14];
    [controllerMode ? OpnColor([self resolvedControllerAccentRGB], 0.075) : OpnColor(0xFFFFFF, 0.075) setFill];
    [planPill fill];
    if (controllerMode) {
        [OpnColor([self resolvedControllerAccentSoftRGB], 0.24) setStroke];
        planPill.lineWidth = 1.0;
        [planPill stroke];
    }
    NSMutableParagraphStyle *remainingStyle = [[NSMutableParagraphStyle alloc] init];
    remainingStyle.alignment = NSTextAlignmentCenter;
    NSMutableDictionary<NSAttributedStringKey, id> *remainingAttrs = [OpnTextStyle(12, OpnColor(kTextSecondary), NSFontWeightSemibold) mutableCopy];
    remainingAttrs[NSParagraphStyleAttributeName] = remainingStyle;
    [remaining drawInRect:NSInsetRect(planRect, 0, 5)
              withAttributes:remainingAttrs];

    NSString *gameCount = controllerMode ? OPNCurrentHeaderTimeText() : (self.gameCountText.length > 0 ? self.gameCountText : @"");
    NSMutableParagraphStyle *gameCountStyle = [[NSMutableParagraphStyle alloc] init];
    gameCountStyle.alignment = controllerMode ? NSTextAlignmentRight : NSTextAlignmentCenter;
    NSMutableDictionary<NSAttributedStringKey, id> *gameCountAttrs = [OpnTextStyle(10, OpnColor(kTextMuted), NSFontWeightMedium) mutableCopy];
    gameCountAttrs[NSParagraphStyleAttributeName] = gameCountStyle;
    [gameCount drawInRect:controllerMode ? NSMakeRect(NSMaxX(planRect) + 14.0, 78.0, 146.0, 14.0) : NSMakeRect(NSMinX(planRect), 40.0, NSWidth(planRect), 14)
          withAttributes:gameCountAttrs];

    NSRect avatarRect = controllerMode ? NSMakeRect(NSWidth(bounds) - 292.0, 18.0, 30.0, 30.0) : NSMakeRect(NSWidth(bounds) - 164, 17.0, 30, 30);
    NSBezierPath *avatar = [NSBezierPath bezierPathWithOvalInRect:avatarRect];

    NSString *name = self.accountName.length > 0 ? self.accountName : @"User";
    if (self.accountAvatarImage) {
        [NSGraphicsContext saveGraphicsState];
        [avatar addClip];
        [self.accountAvatarImage drawInRect:avatarRect
                                   fromRect:NSZeroRect
                                  operation:NSCompositingOperationSourceOver
                                   fraction:1.0
                             respectFlipped:YES
                                      hints:@{NSImageHintInterpolation: @(NSImageInterpolationHigh)}];
        [NSGraphicsContext restoreGraphicsState];
    } else {
        [OpnColor([self resolvedControllerAccentSoftRGB], 0.90) setFill];
        [avatar fill];
        NSString *initial = name.length > 0 ? [[name substringToIndex:1] uppercaseString] : @"U";
        NSMutableParagraphStyle *avatarStyle = [[NSMutableParagraphStyle alloc] init];
        avatarStyle.alignment = NSTextAlignmentCenter;
        NSMutableDictionary<NSAttributedStringKey, id> *avatarAttrs = [OpnTextStyle(13, OpnColor([self resolvedControllerAccentBlackRGB:0.88]), NSFontWeightBold) mutableCopy];
        avatarAttrs[NSParagraphStyleAttributeName] = avatarStyle;
        [initial drawInRect:NSMakeRect(NSMinX(avatarRect), NSMinY(avatarRect) + 7, 30, 16) withAttributes:avatarAttrs];
    }

    if (controllerMode) {
        NSMutableParagraphStyle *accountTextStyle = [[NSMutableParagraphStyle alloc] init];
        accountTextStyle.alignment = NSTextAlignmentCenter;
        NSMutableDictionary<NSAttributedStringKey, id> *nameAttrs = [OpnTextStyle(12, OpnColor(kTextPrimary), NSFontWeightSemibold) mutableCopy];
        nameAttrs[NSParagraphStyleAttributeName] = accountTextStyle;
        [name drawInRect:NSMakeRect(NSWidth(bounds) - 252.0, 17.0, 200.0, 17.0) withAttributes:nameAttrs];
    } else {
        [name drawInRect:NSMakeRect(NSWidth(bounds) - 124, 16.0, 72, 17)
           withAttributes:OpnTextStyle(12, OpnColor(kTextPrimary), NSFontWeightSemibold)];
    }
    NSString *status = self.accountStatus.length > 0 ? self.accountStatus : @"Signed in";
    if (controllerMode) {
        NSMutableParagraphStyle *statusTextStyle = [[NSMutableParagraphStyle alloc] init];
        statusTextStyle.alignment = NSTextAlignmentCenter;
        NSMutableDictionary<NSAttributedStringKey, id> *statusAttrs = [OpnTextStyle(10, OpnColor(kTextMuted), NSFontWeightRegular) mutableCopy];
        statusAttrs[NSParagraphStyleAttributeName] = statusTextStyle;
        [status drawInRect:NSMakeRect(NSWidth(bounds) - 252.0, 33.0, 200.0, 14.0) withAttributes:statusAttrs];
    } else {
        [status drawInRect:NSMakeRect(NSWidth(bounds) - 124, 32.0, 72, 14)
                withAttributes:OpnTextStyle(10, OpnColor(kTextMuted), NSFontWeightRegular)];
    }

    _accountFrame = controllerMode ? NSMakeRect(NSWidth(bounds) - 304.0, 10.0, 284.0, 92.0) : NSMakeRect(NSWidth(bounds) - 174, 9.0, 154, 48);
    NSBezierPath *chevron = [NSBezierPath bezierPath];
    CGFloat chevronX = controllerMode ? NSWidth(bounds) - 36.0 : NSWidth(bounds) - 36.0;
    CGFloat chevronY = controllerMode ? 31.0 : 28.0;
    [chevron moveToPoint:NSMakePoint(chevronX - 4.0, chevronY - 2.0)];
    [chevron lineToPoint:NSMakePoint(chevronX, chevronY + 2.0)];
    [chevron lineToPoint:NSMakePoint(chevronX + 4.0, chevronY - 2.0)];
    chevron.lineWidth = 1.5;
    [OpnColor(kTextMuted, 0.82) setStroke];
    [chevron stroke];
}

- (void)storeButtonPressed:(id)sender {
    (void)sender;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneSelect);
    if (self.onStoreSelected) self.onStoreSelected();
}

- (void)libraryButtonPressed:(id)sender {
    (void)sender;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneSelect);
    if (self.onLibrarySelected) self.onLibrarySelected();
}

- (void)settingsButtonPressed:(id)sender {
    (void)sender;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneSelect);
    if (self.onSettingsSelected) self.onSettingsSelected();
}

- (void)dismissControllerAccountMenu {
    [_controllerAccountMenuView removeFromSuperview];
    _controllerAccountMenuView = nil;
}

- (NSButton *)controllerAccountMenuButtonWithTitle:(NSString *)title
                                               y:(CGFloat)y
                                          height:(CGFloat)height
                                          action:(SEL)action
                                      identifier:(NSString *)identifier
                                        selected:(BOOL)selected
                                         warning:(BOOL)warning {
    NSButton *button = [[NSButton alloc] initWithFrame:NSMakeRect(14.0, y, 292.0, height)];
    button.bordered = NO;
    button.target = self;
    button.action = action;
    button.identifier = identifier ?: @"";
    button.wantsLayer = YES;
    button.layer.cornerRadius = 14.0;
    button.layer.backgroundColor = selected ? OpnColor(OPNControllerAccentRGB(), 0.20).CGColor : OpnColor(OPNControllerAccentRGB(), 0.045).CGColor;
    button.layer.borderWidth = selected ? 1.0 : 0.0;
    button.layer.borderColor = OpnColor(OPNControllerAccentSoftRGB(), 0.52).CGColor;
    NSColor *textColor = warning ? OpnColor(0xFF8A8A) : (selected ? OpnColor(OPN::kTextPrimary) : OpnColor(OPN::kTextSecondary));
    NSString *displayTitle = selected ? [NSString stringWithFormat:@"%@  Current", title] : title;
    NSMutableParagraphStyle *style = [[NSMutableParagraphStyle alloc] init];
    style.alignment = NSTextAlignmentLeft;
    button.attributedTitle = [[NSAttributedString alloc] initWithString:displayTitle attributes:@{
        NSFontAttributeName: [NSFont systemFontOfSize:14.0 weight:selected ? NSFontWeightSemibold : NSFontWeightMedium],
        NSForegroundColorAttributeName: textColor,
        NSParagraphStyleAttributeName: style,
    }];
    return button;
}

- (void)showControllerAccountMenu {
    if (_controllerAccountMenuView) {
        OpnPlayConsoleTone(OPNConsoleToneBack);
        [self dismissControllerAccountMenu];
        return;
    }
    OpnPlayConsoleTone(OPNConsoleToneSelect);

    CGFloat menuWidth = 320.0;
    CGFloat rowHeight = 42.0;
    CGFloat y = 50.0;
    NSInteger accountCount = 0;
    for (NSDictionary<NSString *, NSString *> *account in self.accountMenuItems) {
        NSString *identifier = account[@"identifier"];
        NSString *title = account[@"label"];
        if (identifier.length == 0 || title.length == 0) continue;
        accountCount++;
        y += rowHeight + 8.0;
    }
    CGFloat menuHeight = y + 170.0;
    CGFloat menuX = MAX(20.0, NSWidth(self.bounds) - menuWidth - 20.0);

    NSView *menu = [[OPNBackdropControllerMenuView alloc] initWithFrame:NSMakeRect(menuX, 106.0, menuWidth, menuHeight)];
    menu.wantsLayer = YES;
    menu.layer.cornerRadius = 24.0;
    menu.layer.borderWidth = 1.0;
    menu.layer.borderColor = OpnColor(0xFFFFFF, 0.18).CGColor;
    menu.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.88), 0.96).CGColor;
    menu.layer.shadowColor = OpnColor(OPNControllerAccentRGB()).CGColor;
    menu.layer.shadowOpacity = 0.24;
    menu.layer.shadowRadius = 30.0;
    menu.layer.shadowOffset = CGSizeZero;

    NSTextField *titleLabel = OpnLabel(@"Account", NSMakeRect(18.0, 18.0, menuWidth - 36.0, 22.0), 15.0, OpnColor(OPN::kTextPrimary), NSFontWeightSemibold);
    [menu addSubview:titleLabel];

    y = 52.0;
    if (accountCount == 0) {
        NSTextField *emptyLabel = OpnLabel(@"No saved accounts", NSMakeRect(18.0, y, menuWidth - 36.0, 22.0), 13.0, OpnColor(OPN::kTextMuted), NSFontWeightMedium);
        [menu addSubview:emptyLabel];
        y += 34.0;
    } else {
        for (NSDictionary<NSString *, NSString *> *account in self.accountMenuItems) {
            NSString *identifier = account[@"identifier"];
            NSString *title = account[@"label"];
            if (identifier.length == 0 || title.length == 0) continue;
            BOOL selected = [identifier isEqualToString:self.currentAccountIdentifier];
            NSButton *button = [self controllerAccountMenuButtonWithTitle:title
                                                                        y:y
                                                                   height:rowHeight
                                                                   action:@selector(controllerAccountMenuItemPressed:)
                                                               identifier:identifier
                                                                 selected:selected
                                                                  warning:NO];
            [menu addSubview:button];
            y += rowHeight + 8.0;
        }
    }

    NSView *divider = [[NSView alloc] initWithFrame:NSMakeRect(18.0, y + 8.0, menuWidth - 36.0, 1.0)];
    divider.wantsLayer = YES;
    divider.layer.backgroundColor = OpnColor(OPNControllerAccentSoftRGB(), 0.10).CGColor;
    [menu addSubview:divider];
    y += 24.0;

    [menu addSubview:[self controllerAccountMenuButtonWithTitle:@"Add Account" y:y height:rowHeight action:@selector(controllerAddAccountPressed:) identifier:nil selected:NO warning:NO]];
    y += rowHeight + 8.0;
    [menu addSubview:[self controllerAccountMenuButtonWithTitle:@"Sign Out" y:y height:rowHeight action:@selector(controllerSignOutPressed:) identifier:nil selected:NO warning:NO]];
    y += rowHeight + 8.0;
    [menu addSubview:[self controllerAccountMenuButtonWithTitle:@"Exit OpenNOW" y:y height:rowHeight action:@selector(controllerExitPressed:) identifier:nil selected:NO warning:YES]];

    _controllerAccountMenuView = menu;
    [self addSubview:menu positioned:NSWindowAbove relativeTo:nil];
}

- (void)accountButtonPressed:(id)sender {
    (void)sender;
    if (OpnControllerModeEnabled()) {
        [self showControllerAccountMenu];
        return;
    }
    NSMenu *menu = [[NSMenu alloc] initWithTitle:@"Account"];
    menu.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
    menu.autoenablesItems = NO;
    if ([menu respondsToSelector:@selector(setMinimumWidth:)]) {
        menu.minimumWidth = 220.0;
    }
    for (NSDictionary<NSString *, NSString *> *account in self.accountMenuItems) {
        NSString *identifier = account[@"identifier"];
        NSString *title = account[@"label"];
        if (identifier.length == 0 || title.length == 0) continue;
        BOOL selected = [identifier isEqualToString:self.currentAccountIdentifier];
        NSString *displayTitle = selected ? [NSString stringWithFormat:@"%@  Current", title] : title;
        NSMenuItem *accountItem = OPNStyledMenuItem(displayTitle,
                                                    @selector(accountMenuItemPressed:),
                                                    self,
                                                    selected ? OpnColor(OPN::kTextPrimary) : OpnColor(OPN::kTextSecondary),
                                                    selected ? NSFontWeightSemibold : NSFontWeightMedium);
        accountItem.representedObject = identifier;
        accountItem.state = selected ? NSControlStateValueOn : NSControlStateValueOff;
        [menu addItem:accountItem];
    }
    if (menu.numberOfItems > 0) {
        [menu addItem:[NSMenuItem separatorItem]];
    }
    NSMenuItem *addItem = OPNStyledMenuItem(@"Add Account",
                                            @selector(addAccountMenuItemPressed:),
                                            self,
                                            OpnColor(OPN::kTextPrimary),
                                            NSFontWeightSemibold);
    [menu addItem:addItem];
    NSMenuItem *signOutItem = OPNStyledMenuItem(@"Sign Out",
                                                @selector(signOutMenuItemPressed:),
                                                self,
                                                OpnColor(OPN::kTextSecondary),
                                                NSFontWeightMedium);
    [menu addItem:signOutItem];
    [menu addItem:[NSMenuItem separatorItem]];
    NSMenuItem *exitItem = OPNStyledMenuItem(@"Exit OpenNOW",
                                             @selector(exitMenuItemPressed:),
                                             self,
                                             OpnColor(0xFF8A8A),
                                             NSFontWeightSemibold);
    [menu addItem:exitItem];
    [menu popUpMenuPositioningItem:nil
                        atLocation:NSMakePoint(0.0, NSHeight(_accountButton.bounds) + 2.0)
                            inView:_accountButton];
}

- (void)controllerAccountMenuItemPressed:(NSButton *)sender {
    NSString *identifier = sender.identifier;
    OpnPlayConsoleTone(OPNConsoleToneSelect);
    [self dismissControllerAccountMenu];
    if (identifier.length > 0 && self.onAccountSelected) self.onAccountSelected(identifier);
}

- (void)controllerAddAccountPressed:(id)sender {
    (void)sender;
    OpnPlayConsoleTone(OPNConsoleToneSelect);
    [self dismissControllerAccountMenu];
    if (self.onAddAccountSelected) self.onAddAccountSelected();
}

- (void)controllerSignOutPressed:(id)sender {
    (void)sender;
    OpnPlayConsoleTone(OPNConsoleToneBack);
    [self dismissControllerAccountMenu];
    if (self.onSignOutSelected) self.onSignOutSelected();
}

- (void)controllerExitPressed:(id)sender {
    (void)sender;
    OpnPlayConsoleTone(OPNConsoleToneBack);
    [self dismissControllerAccountMenu];
    if (self.onExitSelected) self.onExitSelected();
}

- (void)accountMenuItemPressed:(NSMenuItem *)sender {
    NSString *identifier = [sender.representedObject isKindOfClass:NSString.class] ? sender.representedObject : nil;
    if (identifier.length > 0 && self.onAccountSelected) self.onAccountSelected(identifier);
}

- (void)addAccountMenuItemPressed:(id)sender {
    (void)sender;
    if (self.onAddAccountSelected) self.onAddAccountSelected();
}

- (void)signOutMenuItemPressed:(id)sender {
    (void)sender;
    if (self.onSignOutSelected) self.onSignOutSelected();
}

- (void)exitMenuItemPressed:(id)sender {
    (void)sender;
    if (self.onExitSelected) self.onExitSelected();
}

- (void)mouseDown:(NSEvent *)event {
    NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
    if (_controllerAccountMenuView && !NSPointInRect(point, _controllerAccountMenuView.frame) && !NSPointInRect(point, _accountFrame)) {
        [self dismissControllerAccountMenu];
        return;
    }
    if (!OpnControllerModeEnabled() && NSPointInRect(point, _storeNavFrame)) {
        if (self.onStoreSelected) self.onStoreSelected();
        return;
    }
    if (NSPointInRect(point, _libraryNavFrame)) {
        if (self.onLibrarySelected) self.onLibrarySelected();
        return;
    }
    if (NSPointInRect(point, _settingsNavFrame)) {
        if (self.onSettingsSelected) self.onSettingsSelected();
        return;
    }
    if (NSPointInRect(point, _accountFrame)) {
        [self accountButtonPressed:self];
        return;
    }
    [super mouseDown:event];
}

@end
