#import "OPNBackdropView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#import "../common/OPNAuthTypes.h"

@implementation OPNBackdropView {
    NSRect _storeNavFrame;
    NSRect _libraryNavFrame;
    NSRect _settingsNavFrame;
    NSRect _accountFrame;
    NSButton *_storeButton;
    NSButton *_libraryButton;
    NSButton *_settingsButton;
    NSButton *_accountButton;
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
    }
    return self;
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
}

- (void)setCurrentAccountIdentifier:(NSString *)currentAccountIdentifier {
    _currentAccountIdentifier = [currentAccountIdentifier copy];
}

- (void)layout {
    [super layout];
    BOOL showNavigation = self.mode != OPNBackdropModeAuth;
    _storeButton.frame = showNavigation && !NSEqualRects(_storeNavFrame, NSZeroRect) ? _storeNavFrame : NSZeroRect;
    _libraryButton.frame = showNavigation && !NSEqualRects(_libraryNavFrame, NSZeroRect) ? _libraryNavFrame : NSZeroRect;
    _settingsButton.frame = showNavigation && !NSEqualRects(_settingsNavFrame, NSZeroRect) ? _settingsNavFrame : NSZeroRect;
    _accountButton.frame = showNavigation && !NSEqualRects(_accountFrame, NSZeroRect) ? _accountFrame : NSZeroRect;
    _storeButton.hidden = !showNavigation;
    _libraryButton.hidden = !showNavigation;
    _settingsButton.hidden = !showNavigation;
    _accountButton.hidden = !showNavigation;
}

- (void)drawRect:(NSRect)dirtyRect {
    [super drawRect:dirtyRect];

    using namespace OPN;

    NSRect bounds = self.bounds;
    [OpnColor(kBackground) setFill];
    NSRectFill(bounds);

    NSGradient *edgeWash = [[NSGradient alloc] initWithColors:@[
        OpnColor(kBackgroundB, 0.94),
        OpnColor(kBackground, 1.0),
        OpnColor(0x0C0D10, 1.0),
    ]];
    [edgeWash drawInRect:bounds angle:270.0];

    NSGradient *spotlight = [[NSGradient alloc] initWithStartingColor:OpnColor(0xFFFFFF, 0.045)
                                                          endingColor:OpnColor(0xFFFFFF, 0.0)];
    NSRect spotlightRect = NSMakeRect(NSWidth(bounds) * 0.5 - 360.0, -300.0, 720.0, 720.0);
    [spotlight drawInBezierPath:[NSBezierPath bezierPathWithOvalInRect:spotlightRect] angle:90.0];

    NSBezierPath *lowerGlow = [NSBezierPath bezierPathWithOvalInRect:
        NSMakeRect(NSWidth(bounds) - 500.0, NSHeight(bounds) - 360.0, 520.0, 520.0)];
    [OpnColor(kLinkBlue, 0.045) setFill];
    [lowerGlow fill];

    if (self.mode == OPNBackdropModeAuth) {
        return;
    }

    CGFloat navHeight = 64.0;
    NSRect navRect = NSMakeRect(0, 0, NSWidth(bounds), navHeight);
    [OpnColor(0x1C1D21, 0.82) setFill];
    NSRectFill(navRect);
    [OpnColor(0xFFFFFF, 0.08) setFill];
    NSRectFill(NSMakeRect(0, navHeight - 1.0, NSWidth(bounds), 1));

    [@"OpenNOW" drawInRect:NSMakeRect(24.0, 21.0, 132, 22)
            withAttributes:OpnTextStyle(16.0, OpnColor(kTextPrimary), NSFontWeightSemibold)];

    NSArray<NSString *> *items = @[@"Store", @"Library", @"Settings"];
    CGFloat widths[] = {78.0, 86.0, 90.0};
    CGFloat navWidth = widths[0] + widths[1] + widths[2] + 8.0;
    CGFloat x = floor((NSWidth(bounds) - navWidth) / 2.0);
    NSRect segmentedRect = NSMakeRect(x - 4.0, 15.0, navWidth + 8.0, 34.0);
    NSBezierPath *segmented = [NSBezierPath bezierPathWithRoundedRect:segmentedRect xRadius:10.0 yRadius:10.0];
    [OpnColor(0xFFFFFF, 0.055) setFill];
    [segmented fill];
    for (NSUInteger i = 0; i < items.count; i++) {
        BOOL active = (self.mode == OPNBackdropModeStore && i == 0) ||
                      (self.mode == OPNBackdropModeLibrary && i == 1) ||
                      (self.mode == OPNBackdropModeSettings && i == 2);
        NSRect itemRect = NSMakeRect(x, 18.0, widths[i], 28.0);
        if (i == 0) _storeNavFrame = itemRect;
        if (i == 1) _libraryNavFrame = itemRect;
        if (i == 2) _settingsNavFrame = itemRect;
        if (active) {
            NSBezierPath *pill = [NSBezierPath bezierPathWithRoundedRect:itemRect xRadius:8.0 yRadius:8.0];
            [OpnColor(0xFFFFFF, 0.14) setFill];
            [pill fill];
        }
        NSColor *textColor = active ? OpnColor(kTextPrimary) : OpnColor(kTextMuted);
        NSMutableParagraphStyle *style = [[NSMutableParagraphStyle alloc] init];
        style.alignment = NSTextAlignmentCenter;
        NSMutableDictionary<NSAttributedStringKey, id> *attrs = [OpnTextStyle(13, textColor, active ? NSFontWeightSemibold : NSFontWeightRegular) mutableCopy];
        attrs[NSParagraphStyleAttributeName] = style;
        [items[i] drawInRect:NSInsetRect(itemRect, 0, 6.0) withAttributes:attrs];
        x += widths[i] + 4.0;
    }

    NSString *remaining = self.remainingPlayTime.length > 0 ? self.remainingPlayTime : @"--";
    NSRect planRect = NSMakeRect(NSWidth(bounds) - 294, 11.0, 108, 26);
    NSBezierPath *planPill = [NSBezierPath bezierPathWithRoundedRect:planRect xRadius:14 yRadius:14];
    [OpnColor(0xFFFFFF, 0.075) setFill];
    [planPill fill];
    NSMutableParagraphStyle *remainingStyle = [[NSMutableParagraphStyle alloc] init];
    remainingStyle.alignment = NSTextAlignmentCenter;
    NSMutableDictionary<NSAttributedStringKey, id> *remainingAttrs = [OpnTextStyle(12, OpnColor(kTextSecondary), NSFontWeightSemibold) mutableCopy];
    remainingAttrs[NSParagraphStyleAttributeName] = remainingStyle;
    [remaining drawInRect:NSInsetRect(planRect, 0, 5)
              withAttributes:remainingAttrs];

    NSString *gameCount = self.gameCountText.length > 0 ? self.gameCountText : @"";
    NSMutableParagraphStyle *gameCountStyle = [[NSMutableParagraphStyle alloc] init];
    gameCountStyle.alignment = NSTextAlignmentCenter;
    NSMutableDictionary<NSAttributedStringKey, id> *gameCountAttrs = [OpnTextStyle(10, OpnColor(kTextMuted), NSFontWeightMedium) mutableCopy];
    gameCountAttrs[NSParagraphStyleAttributeName] = gameCountStyle;
    [gameCount drawInRect:NSMakeRect(NSMinX(planRect), 40.0, NSWidth(planRect), 14)
          withAttributes:gameCountAttrs];

    NSRect avatarRect = NSMakeRect(NSWidth(bounds) - 164, 17.0, 30, 30);
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

    [name drawInRect:NSMakeRect(NSWidth(bounds) - 124, 16.0, 72, 17)
       withAttributes:OpnTextStyle(12, OpnColor(kTextPrimary), NSFontWeightSemibold)];
    NSString *status = self.accountStatus.length > 0 ? self.accountStatus : @"Signed in";
    [status drawInRect:NSMakeRect(NSWidth(bounds) - 124, 32.0, 72, 14)
              withAttributes:OpnTextStyle(10, OpnColor(kTextMuted), NSFontWeightRegular)];

    _accountFrame = NSMakeRect(NSWidth(bounds) - 174, 9.0, 154, 48);
    NSBezierPath *chevron = [NSBezierPath bezierPath];
    CGFloat chevronX = NSWidth(bounds) - 36.0;
    CGFloat chevronY = 28.0;
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

- (void)accountButtonPressed:(id)sender {
    (void)sender;
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
    if (NSPointInRect(point, _storeNavFrame)) {
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
