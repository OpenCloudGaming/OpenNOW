#import "OPNBackdropView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#import "../common/OPNAuthTypes.h"
#import <GameController/GameController.h>

@interface OPNBackdropControllerMenuView : NSView
@end

@implementation OPNBackdropControllerMenuView
- (BOOL)isFlipped { return YES; }
@end

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
}

- (void)interfacePreferencesChanged:(NSNotification *)notification {
    (void)notification;
    [self setNeedsDisplay:YES];
    [self startControllerNavigationIfNeeded];
}

- (void)startControllerNavigationIfNeeded {
    if (!OpnControllerModeEnabled() || _controllerNavigationTimer) return;
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
    if (@available(macOS 10.15, *)) {
        if (pad.buttonMenu.value > 0.5 || pad.buttonOptions.value > 0.5) buttons |= 1u << 2;
    }
    return buttons;
}

- (void)selectPreviousControllerTab {
    if (self.mode == OPNBackdropModeSettings) {
        if (self.onLibrarySelected) self.onLibrarySelected();
    }
}

- (void)selectNextControllerTab {
    if (self.mode == OPNBackdropModeLibrary) {
        if (self.onSettingsSelected) self.onSettingsSelected();
    }
}

- (void)pollControllerNavigation {
    if (!OpnControllerModeEnabled()) {
        [_controllerNavigationTimer invalidate];
        _controllerNavigationTimer = nil;
        _previousControllerButtons = 0;
        return;
    }
    uint16_t buttons = [self currentControllerNavigationButtons];
    uint16_t pressed = buttons & (uint16_t)~_previousControllerButtons;
    if (pressed & (1u << 0)) [self selectPreviousControllerTab];
    if (pressed & (1u << 1)) [self selectNextControllerTab];
    if (pressed & (1u << 2)) [self accountButtonPressed:self];
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

- (void)setMode:(OPNBackdropMode)mode {
    _mode = mode;
    [self dismissControllerAccountMenu];
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
        _libraryNavFrame = NSMakeRect(28.0, 78.0, 86.0, 34.0);
        _settingsNavFrame = NSMakeRect(124.0, 78.0, 90.0, 34.0);
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

    NSGradient *edgeWash = controllerMode
        ? [[NSGradient alloc] initWithColors:@[
            OpnColor(kBackground, 1.0),
            OpnColor(kBrandGreen, 0.20),
            OpnColor(0x06080E, 1.0),
        ]]
        : [[NSGradient alloc] initWithColors:@[
            OpnColor(kBackgroundB, 0.94),
            OpnColor(kBackground, 1.0),
            OpnColor(0x0C0D10, 1.0),
        ]];
    [edgeWash drawInRect:bounds angle:270.0];

    NSGradient *spotlight = [[NSGradient alloc] initWithStartingColor:controllerMode ? OpnColor(kBrandGreen, 0.18) : OpnColor(0xFFFFFF, 0.045)
                                                          endingColor:OpnColor(0xFFFFFF, 0.0)];
    NSRect spotlightRect = controllerMode
        ? NSMakeRect(NSWidth(bounds) * 0.5 - 520.0, -360.0, 1040.0, 760.0)
        : NSMakeRect(NSWidth(bounds) * 0.5 - 360.0, -300.0, 720.0, 720.0);
    [spotlight drawInBezierPath:[NSBezierPath bezierPathWithOvalInRect:spotlightRect] angle:90.0];

    NSBezierPath *lowerGlow = [NSBezierPath bezierPathWithOvalInRect:
        NSMakeRect(NSWidth(bounds) - 500.0, NSHeight(bounds) - 360.0, 520.0, 520.0)];
    [controllerMode ? OpnColor(kBrandGreen, 0.10) : OpnColor(kLinkBlue, 0.045) setFill];
    [lowerGlow fill];

    if (controllerMode) {
        NSGradient *depthGlow = [[NSGradient alloc] initWithStartingColor:OpnColor(kBrandGreen, 0.16)
                                                            endingColor:OpnColor(kBrandGreen, 0.0)];
        [depthGlow drawInBezierPath:[NSBezierPath bezierPathWithOvalInRect:NSMakeRect(-260.0, 210.0, 760.0, 560.0)] angle:35.0];
        [depthGlow drawInBezierPath:[NSBezierPath bezierPathWithOvalInRect:NSMakeRect(NSWidth(bounds) * 0.48, NSHeight(bounds) - 420.0, 980.0, 520.0)] angle:210.0];

        NSBezierPath *horizon = [NSBezierPath bezierPathWithRoundedRect:NSMakeRect(24.0, 137.0, NSWidth(bounds) - 48.0, 1.0) xRadius:0.5 yRadius:0.5];
        [OpnColor(kBrandGreen, 0.18) setFill];
        [horizon fill];
    }

    if (self.mode == OPNBackdropModeAuth) {
        return;
    }

    CGFloat navHeight = controllerMode ? 136.0 : 64.0;
    NSRect navRect = NSMakeRect(0, 0, NSWidth(bounds), navHeight);
    [controllerMode ? OpnColor(0x06080D, 0.42) : OpnColor(0x1C1D21, 0.82) setFill];
    NSRectFill(navRect);
    [OpnColor(0xFFFFFF, 0.08) setFill];
    NSRectFill(NSMakeRect(0, navHeight - 1.0, NSWidth(bounds), 1));

    if (!controllerMode) {
        [@"OpenNOW" drawInRect:NSMakeRect(32.0, 21.0, 132, 22)
                withAttributes:OpnTextStyle(16.0, OpnColor(kTextPrimary), NSFontWeightSemibold)];
    }

    if (controllerMode) {
        NSDateFormatter *timeFormatter = [[NSDateFormatter alloc] init];
        timeFormatter.dateFormat = @"h:mm a";
        NSString *timeText = [[timeFormatter stringFromDate:NSDate.date] uppercaseString];
        NSBezierPath *timeGlow = [NSBezierPath bezierPathWithRoundedRect:NSMakeRect(20.0, 34.0, 128.0, 30.0) xRadius:15.0 yRadius:15.0];
        [OpnColor(kBrandGreen, 0.075) setFill];
        [timeGlow fill];
        [timeText drawInRect:NSMakeRect(32.0, 42.0, 112.0, 18.0)
              withAttributes:OpnTextStyle(13.0, OpnColor(kTextSecondary), NSFontWeightSemibold)];
    }

    NSArray<NSString *> *items = controllerMode ? @[@"Library", @"Settings"] : @[@"Store", @"Library", @"Settings"];
    CGFloat widths[] = {86.0, 90.0, 78.0};
    CGFloat navWidth = controllerMode ? widths[0] + widths[1] + 10.0 : widths[2] + widths[0] + widths[1] + 8.0;
    CGFloat x = controllerMode ? 28.0 : floor((NSWidth(bounds) - navWidth) / 2.0);
    _storeNavFrame = controllerMode ? NSZeroRect : _storeNavFrame;
    CGFloat navRowY = controllerMode ? 74.0 : 15.0;
    NSRect segmentedRect = NSMakeRect(x - 8.0, navRowY, navWidth + 16.0, controllerMode ? 42.0 : 34.0);
    NSBezierPath *segmented = [NSBezierPath bezierPathWithRoundedRect:segmentedRect xRadius:controllerMode ? 21.0 : 10.0 yRadius:controllerMode ? 21.0 : 10.0];
    [controllerMode ? OpnColor(0xFFFFFF, 0.055) : OpnColor(0xFFFFFF, 0.055) setFill];
    [segmented fill];
    if (controllerMode) {
        [OpnColor(kBrandGreen, 0.18) setStroke];
        segmented.lineWidth = 1.0;
        [segmented stroke];
    }
    for (NSUInteger i = 0; i < items.count; i++) {
        NSString *item = items[i];
        CGFloat itemWidth = [item isEqualToString:@"Store"] ? widths[2] : ([item isEqualToString:@"Library"] ? widths[0] : widths[1]);
        BOOL active = ([item isEqualToString:@"Store"] && self.mode == OPNBackdropModeStore) ||
                      ([item isEqualToString:@"Library"] && self.mode == OPNBackdropModeLibrary) ||
                      ([item isEqualToString:@"Settings"] && self.mode == OPNBackdropModeSettings);
        NSRect itemRect = NSMakeRect(x, controllerMode ? 78.0 : 18.0, itemWidth, controllerMode ? 34.0 : 28.0);
        if ([item isEqualToString:@"Store"]) _storeNavFrame = itemRect;
        if ([item isEqualToString:@"Library"]) _libraryNavFrame = itemRect;
        if ([item isEqualToString:@"Settings"]) _settingsNavFrame = itemRect;
        if (active) {
            NSBezierPath *pill = [NSBezierPath bezierPathWithRoundedRect:itemRect xRadius:controllerMode ? 17.0 : 8.0 yRadius:controllerMode ? 17.0 : 8.0];
            [controllerMode ? OpnColor(kBrandGreen, 0.26) : OpnColor(0xFFFFFF, 0.14) setFill];
            [pill fill];
            if (controllerMode) {
                [OpnColor(kBrandGreen, 0.58) setStroke];
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
    NSRect planRect = controllerMode ? NSMakeRect(controllerStatsX, 82.0, 132.0, 26.0) : NSMakeRect(NSWidth(bounds) - 294, 11.0, 108, 26);
    NSBezierPath *planPill = [NSBezierPath bezierPathWithRoundedRect:planRect xRadius:14 yRadius:14];
    [controllerMode ? OpnColor(kBrandGreen, 0.10) : OpnColor(0xFFFFFF, 0.075) setFill];
    [planPill fill];
    if (controllerMode) {
        [OpnColor(kBrandGreen, 0.24) setStroke];
        planPill.lineWidth = 1.0;
        [planPill stroke];
    }
    NSMutableParagraphStyle *remainingStyle = [[NSMutableParagraphStyle alloc] init];
    remainingStyle.alignment = NSTextAlignmentCenter;
    NSMutableDictionary<NSAttributedStringKey, id> *remainingAttrs = [OpnTextStyle(12, OpnColor(kTextSecondary), NSFontWeightSemibold) mutableCopy];
    remainingAttrs[NSParagraphStyleAttributeName] = remainingStyle;
    [remaining drawInRect:NSInsetRect(planRect, 0, 5)
              withAttributes:remainingAttrs];

    NSString *gameCount = self.gameCountText.length > 0 ? self.gameCountText : @"";
    NSMutableParagraphStyle *gameCountStyle = [[NSMutableParagraphStyle alloc] init];
    gameCountStyle.alignment = controllerMode ? NSTextAlignmentRight : NSTextAlignmentCenter;
    NSMutableDictionary<NSAttributedStringKey, id> *gameCountAttrs = [OpnTextStyle(10, OpnColor(kTextMuted), NSFontWeightMedium) mutableCopy];
    gameCountAttrs[NSParagraphStyleAttributeName] = gameCountStyle;
    [gameCount drawInRect:controllerMode ? NSMakeRect(NSMaxX(planRect) + 14.0, 88.0, 146.0, 14.0) : NSMakeRect(NSMinX(planRect), 40.0, NSWidth(planRect), 14)
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
        NSString *initial = name.length > 0 ? [[name substringToIndex:1] uppercaseString] : @"U";
        NSMutableParagraphStyle *avatarStyle = [[NSMutableParagraphStyle alloc] init];
        avatarStyle.alignment = NSTextAlignmentCenter;
        NSMutableDictionary<NSAttributedStringKey, id> *avatarAttrs = [OpnTextStyle(13, OpnColor(kAccentOn), NSFontWeightBold) mutableCopy];
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
    if (self.onStoreSelected) self.onStoreSelected();
}

- (void)libraryButtonPressed:(id)sender {
    (void)sender;
    if (self.onLibrarySelected) self.onLibrarySelected();
}

- (void)settingsButtonPressed:(id)sender {
    (void)sender;
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
    button.layer.backgroundColor = selected ? OpnColor(OPN::kBrandGreen, 0.22).CGColor : OpnColor(0xFFFFFF, 0.045).CGColor;
    button.layer.borderWidth = selected ? 1.0 : 0.0;
    button.layer.borderColor = OpnColor(OPN::kBrandGreen, 0.50).CGColor;
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
        [self dismissControllerAccountMenu];
        return;
    }

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
    menu.layer.borderColor = OpnColor(OPN::kBrandGreen, 0.28).CGColor;
    menu.layer.backgroundColor = OpnColor(0x080A10, 0.94).CGColor;
    menu.layer.shadowColor = OpnColor(OPN::kBrandGreen).CGColor;
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
    divider.layer.backgroundColor = OpnColor(0xFFFFFF, 0.10).CGColor;
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
    [self dismissControllerAccountMenu];
    if (identifier.length > 0 && self.onAccountSelected) self.onAccountSelected(identifier);
}

- (void)controllerAddAccountPressed:(id)sender {
    (void)sender;
    [self dismissControllerAccountMenu];
    if (self.onAddAccountSelected) self.onAddAccountSelected();
}

- (void)controllerSignOutPressed:(id)sender {
    (void)sender;
    [self dismissControllerAccountMenu];
    if (self.onSignOutSelected) self.onSignOutSelected();
}

- (void)controllerExitPressed:(id)sender {
    (void)sender;
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
