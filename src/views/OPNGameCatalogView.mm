#import "OPNGameCatalogView.h"
#import "OPNGameCardView.h"
#import "OPNLoadingView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNCoreAnimationCoordinator.h"
#import "../common/OPNUIHelpers.h"
#import "../streaming/OPNStreamPreferences.h"
#import <CoreImage/CoreImage.h>
#import <GameController/GameController.h>
#include <QuartzCore/QuartzCore.h>
#include <algorithm>
#include <cmath>

static const CGFloat kGridPadding = 28.0;
static const CGFloat kCardSpacing = 18.0;
static const CGFloat kNavHeight = 62.0;
static const CGFloat kToolbarHeight = 82.0;
static const CGFloat kControllerRailSelectorOverlap = 22.0;
static const CGFloat kControllerRailDetailOverlap = 22.0;
static const CGFloat kControllerGameHubMinimumHeight = 374.0;
static const CGFloat kControllerGameHubVerticalReserve = 96.0;
static NSString *const OPNFavoriteGameIdsDefaultsKey = @"OpenNOW.Library.FavoriteGameIds";

static unsigned OPNControllerAccentRGB(void) {
    return OpnCurrentAccentRGB();
}

static unsigned OPNControllerAccentSoftRGB(void) {
    return OpnBlendRGB(OpnCurrentAccentRGB(), 0xFFFFFF, 0.42);
}

static unsigned OPNControllerAccentBlackRGB(CGFloat blackMix) {
    return OpnBlendRGB(OpnCurrentAccentRGB(), 0x000000, blackMix);
}

static unsigned OPNRGBFromColor(NSColor *color, unsigned fallbackRGB) {
    if (!color) return fallbackRGB;
    NSColor *rgbColor = [color colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    if (!rgbColor) return fallbackRGB;
    NSInteger red = (NSInteger)lrint(MAX(0.0, MIN(1.0, rgbColor.redComponent)) * 255.0);
    NSInteger green = (NSInteger)lrint(MAX(0.0, MIN(1.0, rgbColor.greenComponent)) * 255.0);
    NSInteger blue = (NSInteger)lrint(MAX(0.0, MIN(1.0, rgbColor.blueComponent)) * 255.0);
    return ((unsigned)red << 16) | ((unsigned)green << 8) | (unsigned)blue;
}

static NSString *OPNCatalogString(const std::string &value, NSString *fallback = @"") {
    return value.empty() ? fallback : [NSString stringWithUTF8String:value.c_str()];
}

static NSString *OPNCatalogDisplayLabel(NSString *value) {
    NSString *trimmed = [value ?: @"" stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    if (trimmed.length == 0) return @"";

    NSString *specialKey = [[trimmed stringByReplacingOccurrencesOfString:@"-" withString:@"_"] uppercaseString];
    NSDictionary<NSString *, NSString *> *specialLabels = @{
        @"FREE_TO_PLAY": @"Free to Play",
        @"MASSIVELY_MULTIPLAYER_ONLINE": @"Massively Multiplayer Online",
        @"MASSIVELY_MULTIPLAYER": @"Massively Multiplayer",
        @"MMO": @"MMO",
    };
    NSString *specialLabel = specialLabels[specialKey];
    if (specialLabel.length > 0) return specialLabel;

    NSString *spaced = [[trimmed stringByReplacingOccurrencesOfString:@"_" withString:@" "] stringByReplacingOccurrencesOfString:@"-" withString:@" "];
    NSArray<NSString *> *tokens = [spaced componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    NSSet<NSString *> *acronyms = [NSSet setWithArray:@[@"AI", @"DLC", @"EA", @"ESRB", @"FPS", @"GOG", @"HDR", @"MMO", @"MOBA", @"NVIDIA", @"PC", @"PVE", @"PVP", @"RTX", @"VR"]];
    NSSet<NSString *> *lowercaseWords = [NSSet setWithArray:@[@"and", @"of", @"or", @"to"]];
    NSMutableArray<NSString *> *labels = [NSMutableArray array];

    for (NSString *token in tokens) {
        if (token.length == 0) continue;
        NSString *upper = token.uppercaseString;
        if ([acronyms containsObject:upper]) {
            [labels addObject:upper];
            continue;
        }

        NSString *lower = token.lowercaseString;
        if (labels.count > 0 && [lowercaseWords containsObject:lower]) {
            [labels addObject:lower];
            continue;
        }

        NSString *first = [lower substringToIndex:1].uppercaseString;
        NSString *rest = lower.length > 1 ? [lower substringFromIndex:1] : @"";
        [labels addObject:[first stringByAppendingString:rest]];
    }

    return labels.count > 0 ? [labels componentsJoinedByString:@" "] : trimmed;
}

static NSString *OPNCatalogDisplayString(const std::string &value, NSString *fallback = @"") {
    NSString *raw = OPNCatalogString(value, @"");
    NSString *label = OPNCatalogDisplayLabel(raw);
    return label.length > 0 ? label : fallback;
}

static NSAttributedString *OPNOutlinedControllerStoreText(NSString *text) {
    NSMutableParagraphStyle *style = [[NSMutableParagraphStyle alloc] init];
    style.lineBreakMode = NSLineBreakByTruncatingTail;
    NSShadow *shadow = [[NSShadow alloc] init];
    shadow.shadowColor = OpnColor(0x000000, 0.78);
    shadow.shadowBlurRadius = 8.0;
    shadow.shadowOffset = NSMakeSize(0.0, 1.0);
    return [[NSAttributedString alloc] initWithString:text ?: @""
                                           attributes:@{
        NSFontAttributeName: [NSFont systemFontOfSize:17.0 weight:NSFontWeightMedium],
        NSForegroundColorAttributeName: NSColor.whiteColor,
        NSShadowAttributeName: shadow,
        NSParagraphStyleAttributeName: style,
    }];
}

static NSAttributedString *OPNOutlinedControllerDescriptionText(NSString *text) {
    NSMutableParagraphStyle *style = [[NSMutableParagraphStyle alloc] init];
    style.lineBreakMode = NSLineBreakByWordWrapping;
    style.lineSpacing = 5.0;
    NSShadow *shadow = [[NSShadow alloc] init];
    shadow.shadowColor = OpnColor(0x000000, 0.82);
    shadow.shadowBlurRadius = 9.0;
    shadow.shadowOffset = NSMakeSize(0.0, 1.0);
    return [[NSAttributedString alloc] initWithString:text ?: @""
                                           attributes:@{
        NSFontAttributeName: [NSFont systemFontOfSize:17.0 weight:NSFontWeightRegular],
        NSForegroundColorAttributeName: NSColor.whiteColor,
        NSShadowAttributeName: shadow,
        NSParagraphStyleAttributeName: style,
    }];
}

@interface OPNFlippedGridDocumentView : NSView
@end

@implementation OPNFlippedGridDocumentView
- (BOOL)isFlipped { return YES; }
@end

@interface OPNControllerPreviewBackgroundView : NSView
@property (nonatomic, strong) NSImage *image;
@property (nonatomic, assign) CGFloat cornerRadius;
@end

@implementation OPNControllerPreviewBackgroundView

- (BOOL)isFlipped { return YES; }

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        _cornerRadius = 0.0;
    }
    return self;
}

- (void)setImage:(NSImage *)image {
    _image = image;
    self.needsDisplay = YES;
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    NSRect bounds = self.bounds;
    if (NSIsEmptyRect(bounds)) return;

    if (self.cornerRadius > 0.0) {
        NSBezierPath *clipPath = [NSBezierPath bezierPathWithRoundedRect:bounds xRadius:self.cornerRadius yRadius:self.cornerRadius];
        [clipPath addClip];
    }
    [OpnColor(OPNControllerAccentBlackRGB(0.96), 0.98) setFill];
    NSRectFill(bounds);

    if (self.image && self.image.size.width > 0.0 && self.image.size.height > 0.0) {
        CGFloat imageAspect = self.image.size.width / self.image.size.height;
        CGFloat boundsAspect = NSWidth(bounds) / MAX(1.0, NSHeight(bounds));
        NSRect sourceRect = NSMakeRect(0.0, 0.0, self.image.size.width, self.image.size.height);
        if (imageAspect > boundsAspect) {
            CGFloat sourceWidth = self.image.size.height * boundsAspect;
            sourceRect.origin.x = floor((self.image.size.width - sourceWidth) * 0.5);
            sourceRect.size.width = sourceWidth;
        } else {
            CGFloat sourceHeight = self.image.size.width / boundsAspect;
            sourceRect.origin.y = floor((self.image.size.height - sourceHeight) * 0.5);
            sourceRect.size.height = sourceHeight;
        }
        [self.image drawInRect:bounds fromRect:sourceRect operation:NSCompositingOperationSourceOver fraction:1.0 respectFlipped:YES hints:@{NSImageHintInterpolation: @(NSImageInterpolationHigh)}];
    }

    [OpnColor(0x020406, 0.23) setFill];
    NSRectFillUsingOperation(bounds, NSCompositingOperationSourceOver);

    NSRect fadeRect = bounds;
    NSGradient *rightFade = [[NSGradient alloc] initWithColorsAndLocations:
        OpnColor(0x020406, 0.44), 0.0,
        OpnColor(0x020406, 0.34), 0.42,
        OpnColor(0x020406, 0.47), 1.0,
        nil];
    [rightFade drawInRect:fadeRect angle:0.0];
}

@end

@interface OPNCenteredSearchFieldCell : NSSearchFieldCell
@end

@implementation OPNCenteredSearchFieldCell

- (NSRect)opn_centeredTextRectForBounds:(NSRect)rect {
    NSRect textRect = [super drawingRectForBounds:rect];
    NSFont *font = self.font ?: [NSFont systemFontOfSize:[NSFont systemFontSize]];
    CGFloat textHeight = ceil(font.ascender - font.descender + 2.0);
    textRect.origin.y = rect.origin.y + floor((NSHeight(rect) - textHeight) / 2.0) + 2.0;
    textRect.size.height = textHeight;
    return textRect;
}

- (NSRect)drawingRectForBounds:(NSRect)rect {
    return [self opn_centeredTextRectForBounds:rect];
}

- (void)editWithFrame:(NSRect)rect
               inView:(NSView *)controlView
               editor:(NSText *)textObj
             delegate:(id)delegate
                event:(NSEvent *)event {
    [super editWithFrame:[self opn_centeredTextRectForBounds:rect]
                  inView:controlView
                  editor:textObj
                delegate:delegate
                   event:event];
}

- (void)selectWithFrame:(NSRect)rect
                 inView:(NSView *)controlView
                 editor:(NSText *)textObj
               delegate:(id)delegate
                  start:(NSInteger)selStart
                 length:(NSInteger)selLength {
    [super selectWithFrame:[self opn_centeredTextRectForBounds:rect]
                    inView:controlView
                    editor:textObj
                  delegate:delegate
                     start:selStart
                    length:selLength];
}

@end

@interface OPNControllerElectricBackgroundView : NSView
@property (nonatomic, strong) NSTimer *animationTimer;
@property (nonatomic, assign) CFTimeInterval animationStartTime;
@property (nonatomic, assign) unsigned accentRGB;
@end

@implementation OPNControllerElectricBackgroundView

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        _accentRGB = OPNControllerAccentRGB();
        _animationStartTime = CACurrentMediaTime();
    }
    return self;
}

- (void)setAccentRGB:(unsigned)accentRGB {
    accentRGB &= 0xFFFFFF;
    if (_accentRGB == accentRGB) return;
    _accentRGB = accentRGB;
    [self setNeedsDisplay:YES];
}

- (unsigned)accentSoftRGB {
    return OpnBlendRGB(self.accentRGB, 0xFFFFFF, 0.42);
}

- (unsigned)accentBlackRGB:(CGFloat)blackMix {
    return OpnBlendRGB(self.accentRGB, 0x000000, blackMix);
}

- (void)dealloc {
    [self.animationTimer invalidate];
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    if (self.window) {
        if (!self.animationTimer) {
            self.animationTimer = [NSTimer timerWithTimeInterval:(1.0 / 30.0)
                                                          target:self
                                                        selector:@selector(animationTick:)
                                                        userInfo:nil
                                                         repeats:YES];
            [NSRunLoop.mainRunLoop addTimer:self.animationTimer forMode:NSRunLoopCommonModes];
        }
    } else {
        [self.animationTimer invalidate];
        self.animationTimer = nil;
    }
}

- (void)animationTick:(NSTimer *)timer {
    (void)timer;
    if (!self.hidden && self.window) [self setNeedsDisplay:YES];
}

- (BOOL)isFlipped { return YES; }

- (CGFloat)unitHashForSeed:(NSUInteger)seed index:(NSUInteger)index {
    uint32_t value = (uint32_t)(seed * 1103515245u + index * 12345u + 0x9E3779B9u);
    value ^= value >> 16;
    value *= 0x7FEB352Du;
    value ^= value >> 15;
    return (CGFloat)(value % 10000u) / 10000.0;
}

- (NSPoint)pointOnBoltFrom:(NSPoint)start to:(NSPoint)end t:(CGFloat)t seed:(NSUInteger)seed {
    CGFloat dx = end.x - start.x;
    CGFloat dy = end.y - start.y;
    CGFloat normalX = -dy;
    CGFloat normalY = dx;
    CGFloat normalLength = MAX(1.0, hypot(normalX, normalY));
    normalX /= normalLength;
    normalY /= normalLength;
    NSUInteger bucket = (NSUInteger)floor(t * 18.0);
    CGFloat hash = [self unitHashForSeed:seed index:bucket + 3];
    CGFloat envelope = sin(t * 3.14159);
    CGFloat offset = (hash - 0.5) * 58.0 * envelope;
    return NSMakePoint(start.x + dx * t + normalX * offset,
                       start.y + dy * t + normalY * offset);
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    NSRect bounds = self.bounds;
    CGFloat phase = (CGFloat)(CACurrentMediaTime() - self.animationStartTime);
    NSGradient *base = [[NSGradient alloc] initWithColors:@[
        OpnColor([self accentBlackRGB:0.95], 1.0),
        OpnColor([self accentBlackRGB:0.90], 1.0),
        OpnColor([self accentBlackRGB:0.97], 1.0),
    ]];
    [base drawInRect:bounds angle:88.0];

    CGFloat width = NSWidth(bounds);
    CGFloat height = NSHeight(bounds);

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
        NSColor *stroke = band % 3 == 0 ? OpnColor(0xFFFFFF, 0.030) : OpnColor([self accentSoftRGB], 0.038);
        [stroke setStroke];
        ribbon.lineWidth = band == 4 ? 2.4 : 1.1;
        [ribbon stroke];
    }

    for (NSInteger i = 0; i < 72; i++) {
        CGFloat x = fmod((CGFloat)(i * 97) + phase * (8.0 + (CGFloat)(i % 5)), MAX(1.0, width));
        CGFloat y = fmod((CGFloat)(i * 43) + sin(phase * 0.24 + (CGFloat)i) * 18.0, MAX(1.0, height));
        CGFloat radius = 0.7 + (CGFloat)(i % 3) * 0.32;
        NSBezierPath *spark = [NSBezierPath bezierPathWithOvalInRect:NSMakeRect(x, y, radius, radius)];
        [OpnColor(i % 5 == 0 ? 0xFFFFFF : [self accentSoftRGB], i % 5 == 0 ? 0.10 : 0.07) setFill];
        [spark fill];
    }

    NSGradient *vignette = [[NSGradient alloc] initWithStartingColor:OpnColor([self accentBlackRGB:0.90], 0.0)
                                                        endingColor:OpnColor([self accentBlackRGB:0.99], 0.42)];
    [vignette drawInRect:bounds angle:-90.0];
}

@end

@class OPNControllerPromptBarView;
@class OPNControllerGameHubView;
@class OPNControllerCategoryCardView;

typedef NS_ENUM(NSInteger, OPNControllerOverviewSpecialTileKind) {
    OPNControllerOverviewSpecialTileNone = 0,
    OPNControllerOverviewSpecialTileStream = 1,
    OPNControllerOverviewSpecialTileLastPlayed = 2,
};

@interface OPNGameCatalogView ()
@property (nonatomic, strong) NSScrollView *scrollView;
@property (nonatomic, strong) NSView *gridContentView;
@property (nonatomic, strong) NSSearchField *searchField;
@property (nonatomic, strong) NSTextField *userLabel;
@property (nonatomic, strong) NSButton *signOutButton;
@property (nonatomic, strong) NSTextField *libraryIconLabel;
@property (nonatomic, strong) NSTextField *titleLabel;
@property (nonatomic, strong) NSButton *sortButton;
@property (nonatomic, strong) NSButton *filterButton;
@property (nonatomic, strong) NSTextField *gameCountLabel;
@property (nonatomic, strong) NSTextField *statusLabel;
@property (nonatomic, strong) OPNLoadingView *loadingView;
@property (nonatomic, strong) NSView *categoryBarView;
@property (nonatomic, strong) NSMutableArray<NSButton *> *categoryButtons;
@property (nonatomic, copy) NSArray<NSDictionary<NSString *, NSString *> *> *categoryItems;
@property (nonatomic, copy) NSString *selectedCategoryId;
@property (nonatomic, strong) NSMutableSet<NSString *> *favoriteGameIds;
@property (nonatomic, strong) OPNControllerElectricBackgroundView *controllerElectricBackgroundView;
@property (nonatomic, strong) NSView *controllerDetailView;
@property (nonatomic, strong) OPNControllerPreviewBackgroundView *controllerDetailBackgroundView;
@property (nonatomic, strong) NSTextField *controllerDetailStatsLabel;
@property (nonatomic, strong) NSTextField *controllerDetailFeaturesLabel;
@property (nonatomic, strong) OPNControllerGameHubView *controllerGameHubView;
@property (nonatomic, strong) OPNControllerPromptBarView *controllerPromptBarView;
@property (nonatomic, strong) NSView *streamPipContainerView;
@property (nonatomic, strong) NSView *streamPipHostView;
@property (nonatomic, strong) NSTextField *streamPipTitleLabel;
@property (nonatomic, strong) NSTextField *streamPipHintLabel;
@property (nonatomic, weak) NSView *streamPipContentView;
@property (nonatomic, assign, getter=isStreamPipFocused) BOOL streamPipFocused;
@property (nonatomic, strong) NSView *lastPlayedPanelView;
@property (nonatomic, strong) NSImageView *lastPlayedImageView;
@property (nonatomic, strong) NSTextField *lastPlayedEyebrowLabel;
@property (nonatomic, strong) NSTextField *lastPlayedTitleLabel;
@property (nonatomic, strong) NSTextField *lastPlayedMetaLabel;
@property (nonatomic, strong) NSTextField *lastPlayedHintLabel;
@property (nonatomic, copy) NSString *lastPlayedImageURL;
@property (nonatomic, assign, getter=isLastPlayedFocused) BOOL lastPlayedFocused;
@property (nonatomic, assign) CGFloat lastPlayedImageAspectRatio;
@property (nonatomic, strong) CAGradientLayer *controllerDetailGradientLayer;
@property (nonatomic, strong) CALayer *controllerDetailAccentLayer;
@property (nonatomic, strong) NSMutableArray<OPNGameCardView *> *cardViews;
@property (nonatomic, strong) NSMutableArray<OPNControllerCategoryCardView *> *categoryCardViews;
@property (nonatomic, assign) std::vector<OPN::GameInfo> allGames;
@property (nonatomic, assign) CGFloat lastLayoutWidth;
@property (nonatomic, assign) BOOL needsGridRenderAfterResize;
@property (nonatomic, copy) NSString *selectedSortId;
@property (nonatomic, strong) NSMutableSet<NSString *> *selectedFilterIds;
@property (nonatomic, assign) std::vector<OPN::CatalogFilterGroup> catalogFilterGroups;
@property (nonatomic, assign) std::vector<OPN::CatalogSortOption> catalogSortOptions;
@property (nonatomic, assign) NSInteger catalogTotalCount;
@property (nonatomic, assign) NSInteger catalogSupportedCount;
@property (nonatomic, assign) NSInteger focusedCardIndex;
@property (nonatomic, assign) NSInteger focusedCategoryIndex;
@property (nonatomic, assign) OPNControllerOverviewSpecialTileKind controllerOverviewSpecialTileKind;
@property (nonatomic, assign) BOOL controllerCategoryOverviewVisible;
@property (nonatomic, assign) NSInteger gridColumnCount;
@property (nonatomic, strong) NSView *detailsOverlayView;
@property (nonatomic, strong) NSTimer *gamepadNavigationTimer;
@property (nonatomic, strong) NSTimer *controllerDetailBackgroundTimer;
@property (nonatomic, copy) NSArray<NSString *> *controllerDetailBackgroundURLs;
@property (nonatomic, copy) NSString *controllerDetailBackgroundURL;
@property (nonatomic, copy) NSString *controllerDetailBackgroundGameId;
@property (nonatomic, assign) NSInteger controllerDetailBackgroundIndex;
@property (nonatomic, assign) uint16_t previousGamepadButtons;
@property (nonatomic, assign) CFTimeInterval lastGamepadMoveTime;
- (void)stopGamepadNavigation;
- (void)scrollLibraryToTop;
- (void)requestCatalogBrowse;
- (void)rebuildCategoryBar;
- (BOOL)game:(const OPN::GameInfo &)game matchesCategory:(NSString *)categoryId;
- (void)cycleCategoryBy:(NSInteger)delta;
- (NSInteger)gameCountForCategory:(NSString *)categoryId;
- (std::vector<OPN::GameInfo>)gamesForCategory:(NSString *)categoryId limit:(NSInteger)limit;
- (const OPN::GameInfo *)currentLastPlayedGame;
- (int)preferredVariantIndexForGame:(const OPN::GameInfo &)game;
- (void)renderControllerCategoryOverview;
- (void)updateLastPlayedPanel;
- (void)loadLastPlayedImageFromCandidates:(NSArray<NSString *> *)candidates index:(NSUInteger)index expectedURL:(NSString *)expectedURL;
- (void)focusCategoryAtIndex:(NSInteger)index scrollIntoView:(BOOL)scrollIntoView;
- (NSInteger)controllerOverviewItemCount;
- (NSInteger)controllerOverviewCategoryOffset;
- (NSView *)controllerOverviewViewAtIndex:(NSInteger)index;
- (void)moveCategoryFocusByRows:(NSInteger)rows columns:(NSInteger)columns;
- (void)openFocusedCategory;
- (void)returnToControllerCategoryOverview;
- (NSString *)favoriteIdentifierForGame:(const OPN::GameInfo &)game;
- (BOOL)isFavoriteGame:(const OPN::GameInfo &)game;
- (void)toggleFavoriteForFocusedGame;
- (void)persistFavoriteGameIds;
- (void)focusCardAtIndex:(NSInteger)index scrollIntoView:(BOOL)scrollIntoView;
- (void)openFocusedGameDetails;
- (void)closeGameDetails;
- (void)launchFocusedGame;
- (void)launchLastPlayedGame;
- (void)cycleFocusedVariant;
- (void)updateControllerDetailContent;
- (void)configureControllerDetailBackgroundForGame:(const OPN::GameInfo &)game;
- (void)startControllerDetailBackgroundRotationIfNeeded;
- (void)stopControllerDetailBackgroundRotation;
- (void)controllerDetailBackgroundTimerFired:(NSTimer *)timer;
- (void)loadControllerDetailBackgroundFromCandidates:(NSArray<NSString *> *)candidates index:(NSUInteger)index expectedURL:(NSString *)expectedURL;
- (void)setStreamPipFocused:(BOOL)focused;
- (void)setLastPlayedFocused:(BOOL)focused;
- (void)startGamepadNavigationIfNeeded;
- (void)controllerDidConnect:(NSNotification *)notification;
- (void)controllerDidDisconnect:(NSNotification *)notification;
@end

static uint16_t OPNCatalogGamepadButtons(void) {
    NSArray<GCController *> *controllers = [GCController controllers];
    if (controllers.count == 0) return 0;
    GCExtendedGamepad *pad = controllers.firstObject.extendedGamepad;
    if (!pad) return 0;
    uint16_t buttons = 0;
    if (pad.buttonA.value > 0.5) buttons |= 1u << 0;
    if (pad.buttonB.value > 0.5) buttons |= 1u << 1;
    if (pad.buttonY.value > 0.5) buttons |= 1u << 2;
    if (pad.leftShoulder.value > 0.5) buttons |= 1u << 3;
    if (pad.rightShoulder.value > 0.5) buttons |= 1u << 4;
    if (pad.dpad.up.value > 0.5 || pad.leftThumbstick.yAxis.value > 0.65) buttons |= 1u << 5;
    if (pad.dpad.down.value > 0.5 || pad.leftThumbstick.yAxis.value < -0.65) buttons |= 1u << 6;
    if (pad.dpad.left.value > 0.5 || pad.leftThumbstick.xAxis.value < -0.65) buttons |= 1u << 7;
    if (pad.dpad.right.value > 0.5 || pad.leftThumbstick.xAxis.value > 0.65) buttons |= 1u << 8;
    if (pad.buttonX.value > 0.5) buttons |= 1u << 9;
    return buttons;
}

static BOOL OPNCatalogGamepadNavigationActive(NSView *view) {
    NSWindow *window = view.window;
    if (!window || window.contentViewController != nil) return NO;
    return window.contentView == view || [view isDescendantOf:window.contentView];
}

static NSString *OPNCatalogJoinedStrings(const std::vector<std::string> &values, NSString *fallback) {
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (const std::string &value : values) {
        if (!value.empty()) {
            NSString *label = OPNCatalogDisplayString(value, @"");
            if (label.length > 0) [items addObject:label];
        }
        if (items.count >= 4) break;
    }
    return items.count > 0 ? [items componentsJoinedByString:@"  /  "] : fallback;
}

static NSArray<NSString *> *OPNControllerCategoryArtworkCandidates(const OPN::GameInfo &game) {
    NSMutableArray<NSString *> *urls = [NSMutableArray arrayWithCapacity:2];
    NSString *poster = OPNCatalogString(game.imageUrl, @"");
    NSString *hero = OPNCatalogString(game.heroImageUrl, @"");
    if (hero.length > 0) [urls addObject:hero];
    if (poster.length > 0 && ![poster isEqualToString:hero]) [urls addObject:poster];
    return urls;
}

static NSArray<NSString *> *OPNControllerCategoryBackgroundCandidates(const OPN::GameInfo &game) {
    NSMutableArray<NSString *> *urls = [NSMutableArray array];
    for (const std::string &screenshotUrl : game.screenshotUrls) {
        NSString *url = OPNCatalogString(screenshotUrl, @"");
        if (url.length > 0 && ![urls containsObject:url]) [urls addObject:url];
    }
    if (urls.count > 0) return urls;
    for (NSString *fallback in OPNControllerCategoryArtworkCandidates(game)) {
        if (fallback.length > 0 && ![urls containsObject:fallback]) [urls addObject:fallback];
    }
    return urls;
}

static NSString *OPNCatalogCommaJoinedStrings(const std::vector<std::string> &values) {
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (const std::string &value : values) {
        if (!value.empty()) {
            NSString *label = OPNCatalogDisplayString(value, @"");
            if (label.length > 0) [items addObject:label];
        }
    }
    return [items componentsJoinedByString:@", "];
}

static NSString *OPNCatalogCommaJoinedStrings(const std::vector<std::string> &values, NSUInteger limit) {
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (const std::string &value : values) {
        if (!value.empty()) {
            NSString *label = OPNCatalogDisplayString(value, @"");
            if (label.length > 0 && ![items containsObject:label]) [items addObject:label];
        }
        if (items.count >= limit) break;
    }
    return [items componentsJoinedByString:@", "];
}

static NSString *OPNCatalogPlayerCountText(int localPlayers, int onlinePlayers) {
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    if (localPlayers > 0) [items addObject:[NSString stringWithFormat:@"%d local", localPlayers]];
    if (onlinePlayers > 0) [items addObject:[NSString stringWithFormat:@"%d online", onlinePlayers]];
    return [items componentsJoinedByString:@", "];
}

static NSString *OPNCategoryId(NSString *prefix, NSString *value) {
    NSString *cleanValue = [[value ?: @"" stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] lowercaseString];
    if (cleanValue.length == 0) return @"";
    return [NSString stringWithFormat:@"%@:%@", prefix, cleanValue];
}

static NSString *OPNStoreCategoryTitle(NSString *store) {
    NSString *upper = store.uppercaseString;
    if ([upper containsString:@"STEAM"]) return @"Steam";
    if ([upper containsString:@"EPIC"] || [upper containsString:@"EGS"]) return @"Epic";
    if ([upper containsString:@"UBISOFT"] || [upper containsString:@"UPLAY"]) return @"Ubisoft";
    if ([upper containsString:@"BATTLE"]) return @"Battle.net";
    if ([upper containsString:@"XBOX"] || [upper containsString:@"MICROSOFT"]) return @"Xbox";
    if ([upper containsString:@"EA"] || [upper containsString:@"ORIGIN"]) return @"EA";
    if ([upper containsString:@"GOG"]) return @"GOG";
    return OPNCatalogDisplayLabel(store);
}

static NSString *OPNCatalogStoreSummary(const std::vector<std::string> &stores, NSString *fallback) {
    NSMutableArray<NSString *> *uniqueItems = [NSMutableArray array];
    for (const std::string &storeValue : stores) {
        NSString *label = OPNStoreCategoryTitle(OPNCatalogString(storeValue, @""));
        if (label.length > 0 && ![uniqueItems containsObject:label]) [uniqueItems addObject:label];
    }
    if (uniqueItems.count == 0) return fallback ?: @"Store not listed";
    NSUInteger visibleCount = MIN((NSUInteger)3, uniqueItems.count);
    NSArray<NSString *> *visibleItems = [uniqueItems subarrayWithRange:NSMakeRange(0, visibleCount)];
    NSString *summary = [visibleItems componentsJoinedByString:@", "];
    NSUInteger remainingCount = uniqueItems.count > visibleCount ? uniqueItems.count - visibleCount : 0;
    return remainingCount > 0 ? [summary stringByAppendingFormat:@" +%lu", (unsigned long)remainingCount] : summary;
}

typedef NS_ENUM(NSInteger, OPNControllerPromptStyle) {
    OPNControllerPromptStyleGeneric = 0,
    OPNControllerPromptStylePlayStation = 1,
    OPNControllerPromptStyleXbox = 2,
    OPNControllerPromptStyleNintendo = 3,
};

static OPNControllerPromptStyle OPNCurrentControllerPromptStyle(void) {
    GCController *controller = GCController.controllers.firstObject;
    if (!controller) return OPNControllerPromptStyleGeneric;
    NSMutableArray<NSString *> *parts = [NSMutableArray array];
    if (controller.vendorName.length > 0) [parts addObject:controller.vendorName];
    if ([controller respondsToSelector:@selector(productCategory)] && controller.productCategory.length > 0) {
        [parts addObject:controller.productCategory];
    }
    NSString *descriptor = [[parts componentsJoinedByString:@" "] lowercaseString];
    if ([descriptor containsString:@"dualsense"] || [descriptor containsString:@"dualshock"] ||
        [descriptor containsString:@"playstation"] || [descriptor containsString:@"sony"]) {
        return OPNControllerPromptStylePlayStation;
    }
    if ([descriptor containsString:@"xbox"] || [descriptor containsString:@"microsoft"]) return OPNControllerPromptStyleXbox;
    if ([descriptor containsString:@"nintendo"] || [descriptor containsString:@"switch"] || [descriptor containsString:@"joy-con"]) {
        return OPNControllerPromptStyleNintendo;
    }
    return OPNControllerPromptStyleGeneric;
}

static NSString *OPNPromptLetter(NSString *button, OPNControllerPromptStyle style) {
    if ([button isEqualToString:@"primary"]) return style == OPNControllerPromptStyleNintendo ? @"A" : @"A";
    if ([button isEqualToString:@"favorite"]) return style == OPNControllerPromptStyleNintendo ? @"X" : @"Y";
    if ([button isEqualToString:@"store"]) return style == OPNControllerPromptStyleNintendo ? @"Y" : @"X";
    if ([button isEqualToString:@"back"]) return style == OPNControllerPromptStyleNintendo ? @"B" : @"B";
    return @"";
}

static void OPNStrokePath(NSBezierPath *path, NSColor *color, CGFloat width) {
    [color setStroke];
    path.lineWidth = width;
    path.lineCapStyle = NSLineCapStyleRound;
    path.lineJoinStyle = NSLineJoinStyleRound;
    [path stroke];
}

static NSImage *OPNControllerPromptIcon(NSString *button, OPNControllerPromptStyle style) {
    const CGFloat size = 24.0;
    NSImage *image = [[NSImage alloc] initWithSize:NSMakeSize(size, size)];
    [image lockFocus];

    NSColor *strokeColor = OpnColor(0xEAFBF0, 0.92);
    NSColor *fillColor = style == OPNControllerPromptStylePlayStation ? OpnColor(0xFFFFFF, 0.035) : OpnColor(0xEAFBF0, 0.14);
    NSRect iconRect = NSMakeRect(2.5, 2.5, 19.0, 19.0);

    if ([button isEqualToString:@"category"]) {
        NSBezierPath *pad = [NSBezierPath bezierPathWithRoundedRect:iconRect xRadius:5.0 yRadius:5.0];
        [OpnColor(0xEAFBF0, 0.08) setFill];
        [pad fill];
        OPNStrokePath(pad, OpnColor(0xEAFBF0, 0.68), 1.4);
        NSBezierPath *up = [NSBezierPath bezierPath];
        [up moveToPoint:NSMakePoint(12.0, 6.0)];
        [up lineToPoint:NSMakePoint(8.5, 10.0)];
        [up moveToPoint:NSMakePoint(12.0, 6.0)];
        [up lineToPoint:NSMakePoint(15.5, 10.0)];
        OPNStrokePath(up, strokeColor, 1.6);
        NSBezierPath *down = [NSBezierPath bezierPath];
        [down moveToPoint:NSMakePoint(12.0, 18.0)];
        [down lineToPoint:NSMakePoint(8.5, 14.0)];
        [down moveToPoint:NSMakePoint(12.0, 18.0)];
        [down lineToPoint:NSMakePoint(15.5, 14.0)];
        OPNStrokePath(down, strokeColor, 1.6);
    } else if (style == OPNControllerPromptStylePlayStation) {
        if ([button isEqualToString:@"primary"]) {
            NSBezierPath *cross = [NSBezierPath bezierPath];
            [cross moveToPoint:NSMakePoint(7.0, 7.0)];
            [cross lineToPoint:NSMakePoint(17.0, 17.0)];
            [cross moveToPoint:NSMakePoint(17.0, 7.0)];
            [cross lineToPoint:NSMakePoint(7.0, 17.0)];
            OPNStrokePath(cross, strokeColor, 2.2);
        } else if ([button isEqualToString:@"favorite"]) {
            NSBezierPath *triangle = [NSBezierPath bezierPath];
            [triangle moveToPoint:NSMakePoint(12.0, 4.5)];
            [triangle lineToPoint:NSMakePoint(20.0, 18.5)];
            [triangle lineToPoint:NSMakePoint(4.0, 18.5)];
            [triangle closePath];
            [fillColor setFill];
            [triangle fill];
            OPNStrokePath(triangle, strokeColor, 1.8);
        } else if ([button isEqualToString:@"store"]) {
            NSBezierPath *square = [NSBezierPath bezierPathWithRoundedRect:NSMakeRect(5.2, 5.2, 13.6, 13.6) xRadius:1.8 yRadius:1.8];
            [fillColor setFill];
            [square fill];
            OPNStrokePath(square, strokeColor, 1.8);
        } else if ([button isEqualToString:@"back"]) {
            NSBezierPath *circle = [NSBezierPath bezierPathWithOvalInRect:NSMakeRect(5.0, 5.0, 14.0, 14.0)];
            [fillColor setFill];
            [circle fill];
            OPNStrokePath(circle, strokeColor, 1.8);
        }
    } else {
        NSBezierPath *circle = [NSBezierPath bezierPathWithOvalInRect:iconRect];
        [fillColor setFill];
        [circle fill];
        OPNStrokePath(circle, strokeColor, 1.4);
        NSString *letter = OPNPromptLetter(button, style);
        NSMutableParagraphStyle *styleCenter = [[NSMutableParagraphStyle alloc] init];
        styleCenter.alignment = NSTextAlignmentCenter;
        [letter drawInRect:NSMakeRect(2.5, 5.3, 19.0, 14.0) withAttributes:@{
            NSFontAttributeName: [NSFont systemFontOfSize:11.0 weight:NSFontWeightBold],
            NSForegroundColorAttributeName: strokeColor,
            NSParagraphStyleAttributeName: styleCenter,
        }];
    }

    [image unlockFocus];
    return image;
}

@interface OPNControllerPromptBarView : NSView
@property (nonatomic, assign) BOOL includeStore;
@property (nonatomic, assign) BOOL includeBack;
@end

@interface OPNControllerGameHubView : NSView
@property (nonatomic, copy) NSString *gameTitle;
@property (nonatomic, copy) NSString *genreSummary;
@property (nonatomic, copy) NSString *launchStatus;
@property (nonatomic, copy) NSString *studioInfo;
@property (nonatomic, copy) NSString *playerInfo;
@property (nonatomic, copy) NSString *controlInfo;
@property (nonatomic, copy) NSString *storeInfo;
@property (nonatomic, assign) unsigned accentRGB;
@end

@interface OPNControllerCategoryCardView : NSView
@property (nonatomic, copy) NSString *categoryId;
@property (nonatomic, copy) NSString *categoryTitle;
@property (nonatomic, assign) NSInteger gameCount;
@property (nonatomic, assign, getter=isControllerFocused) BOOL controllerFocused;
@property (nonatomic, copy) void (^onSelect)(void);
- (instancetype)initWithFrame:(NSRect)frame title:(NSString *)title categoryId:(NSString *)categoryId gameCount:(NSInteger)gameCount games:(const std::vector<OPN::GameInfo> &)games;
@end

@interface OPNControllerCategoryCardView ()
@property (nonatomic, strong) NSMutableArray<NSImageView *> *thumbnailViews;
@property (nonatomic, strong) NSTextField *titleLabel;
@property (nonatomic, strong) NSTextField *countLabel;
@property (nonatomic, strong) NSTextField *kindLabel;
@property (nonatomic, strong) NSTrackingArea *trackingArea;
@end

@implementation OPNControllerCategoryCardView

- (instancetype)initWithFrame:(NSRect)frame title:(NSString *)title categoryId:(NSString *)categoryId gameCount:(NSInteger)gameCount games:(const std::vector<OPN::GameInfo> &)games {
    self = [super initWithFrame:frame];
    if (self) {
        _categoryTitle = [title copy] ?: @"Category";
        _categoryId = [categoryId copy] ?: @"all";
        _gameCount = gameCount;
        _thumbnailViews = [NSMutableArray arrayWithCapacity:6];
        self.wantsLayer = YES;
        self.layer.cornerRadius = 16.0;
        self.layer.masksToBounds = NO;
        self.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.90), 0.76).CGColor;
        self.layer.borderWidth = 1.0;
        self.layer.borderColor = OpnColor(0xFFFFFF, 0.12).CGColor;
        self.layer.shadowColor = NSColor.blackColor.CGColor;
        self.layer.shadowOpacity = 0.24;
        self.layer.shadowRadius = 16.0;
        self.layer.shadowOffset = CGSizeMake(0.0, 10.0);

        for (NSInteger i = 0; i < 6; i++) {
            NSImageView *thumbnail = [[NSImageView alloc] initWithFrame:NSZeroRect];
            thumbnail.imageScaling = NSImageScaleProportionallyUpOrDown;
            thumbnail.wantsLayer = YES;
            thumbnail.layer.cornerRadius = 6.0;
            thumbnail.layer.masksToBounds = YES;
            thumbnail.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.82), 0.78).CGColor;
            thumbnail.layer.borderWidth = 1.0;
            thumbnail.layer.borderColor = OpnColor(0xFFFFFF, 0.11).CGColor;
            [self addSubview:thumbnail];
            [self.thumbnailViews addObject:thumbnail];
        }

        _titleLabel = OpnLabel(_categoryTitle, NSZeroRect, 15.0, OpnColor(OPN::kTextPrimary), NSFontWeightBold);
        _titleLabel.lineBreakMode = NSLineBreakByTruncatingTail;
        [self addSubview:_titleLabel];

        NSString *countText = [_categoryId hasPrefix:@"system:"] ? @"Open" : [NSString stringWithFormat:@"%ld", (long)_gameCount];
        _countLabel = OpnLabel(countText, NSZeroRect, 12.0, OpnColor(OPN::kTextSecondary), NSFontWeightSemibold, NSTextAlignmentCenter);
        _countLabel.wantsLayer = YES;
        _countLabel.layer.cornerRadius = 11.0;
        _countLabel.layer.backgroundColor = OpnColor(0xFFFFFF, 0.070).CGColor;
        _countLabel.layer.borderWidth = 1.0;
        _countLabel.layer.borderColor = OpnColor(0xFFFFFF, 0.13).CGColor;
        [self addSubview:_countLabel];

        NSString *kindText = [_categoryId hasPrefix:@"system:"] ? @"SETTINGS" : @"STORE";
        _kindLabel = OpnLabel(kindText, NSZeroRect, 10.0, OpnColor(OPN::kBrandGreen), NSFontWeightBold, NSTextAlignmentCenter);
        _kindLabel.hidden = ![_categoryId hasPrefix:@"store:"] && ![_categoryId hasPrefix:@"system:"];
        _kindLabel.wantsLayer = YES;
        _kindLabel.layer.cornerRadius = 9.0;
        _kindLabel.layer.backgroundColor = OpnColor(OPN::kBrandGreen, 0.12).CGColor;
        _kindLabel.layer.borderWidth = 1.0;
        _kindLabel.layer.borderColor = OpnColor(OPN::kBrandGreen, 0.26).CGColor;
        [self addSubview:_kindLabel];

        [self loadThumbnailsFromGames:games];
        [self updateTrackingAreas];
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (void)setControllerFocused:(BOOL)controllerFocused {
    if (_controllerFocused == controllerFocused) return;
    _controllerFocused = controllerFocused;
    [self applyFocusStyle];
}

- (void)layout {
    [super layout];
    CGFloat width = NSWidth(self.bounds);
    CGFloat thumbWidth = floor((width - 42.0) / 3.0);
    CGFloat thumbHeight = floor(thumbWidth * 9.0 / 16.0);
    CGFloat startX = 16.0;
    CGFloat startY = 14.0;
    for (NSUInteger i = 0; i < self.thumbnailViews.count; i++) {
        NSInteger column = (NSInteger)i % 3;
        NSInteger row = (NSInteger)i / 3;
        self.thumbnailViews[i].frame = NSMakeRect(startX + column * (thumbWidth + 5.0), startY + row * (thumbHeight + 6.0), thumbWidth, thumbHeight);
    }
    CGFloat labelY = startY + thumbHeight * 2.0 + 18.0;
    self.titleLabel.frame = NSMakeRect(16.0, labelY, width - 32.0, 22.0);
    CGFloat countWidth = [self.categoryId hasPrefix:@"system:"] ? 58.0 : 42.0;
    self.countLabel.frame = NSMakeRect(16.0, labelY + 28.0, countWidth, 22.0);
    self.kindLabel.frame = NSMakeRect(width - 72.0, labelY + 30.0, 56.0, 18.0);
}

- (void)mouseDown:(NSEvent *)event {
    (void)event;
    if (self.onSelect) self.onSelect();
}

- (void)mouseEntered:(NSEvent *)event {
    (void)event;
    if (!self.controllerFocused) self.layer.borderColor = OpnColor(0xFFFFFF, 0.22).CGColor;
}

- (void)mouseExited:(NSEvent *)event {
    (void)event;
    if (!self.controllerFocused) self.layer.borderColor = OpnColor(0xFFFFFF, 0.12).CGColor;
}

- (void)updateTrackingAreas {
    if (self.trackingArea && [self.trackingAreas containsObject:self.trackingArea]) {
        [self removeTrackingArea:self.trackingArea];
    }
    self.trackingArea = [[NSTrackingArea alloc] initWithRect:self.bounds
                                                     options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInActiveApp
                                                       owner:self
                                                    userInfo:nil];
    [self addTrackingArea:self.trackingArea];
}

- (void)applyFocusStyle {
    [CATransaction begin];
    [CATransaction setAnimationDuration:0.20];
    [CATransaction setAnimationTimingFunction:[OPNCoreAnimationCoordinator appleQuinticTimingFunction]];
    self.layer.zPosition = self.controllerFocused ? 24.0 : 0.0;
    self.layer.borderWidth = self.controllerFocused ? 3.0 : 1.0;
    self.layer.borderColor = (self.controllerFocused ? OpnColor(0xFFFFFF, 0.94) : OpnColor(0xFFFFFF, 0.12)).CGColor;
    self.layer.shadowColor = (self.controllerFocused ? OpnColor(OPNControllerAccentSoftRGB()) : NSColor.blackColor).CGColor;
    self.layer.shadowOpacity = self.controllerFocused ? 0.42 : 0.24;
    self.layer.shadowRadius = self.controllerFocused ? 26.0 : 16.0;
    CATransform3D transform = CATransform3DIdentity;
    if (self.controllerFocused) transform = CATransform3DScale(transform, 1.045, 1.045, 1.0);
    self.layer.transform = transform;
    self.countLabel.textColor = self.controllerFocused ? OpnColor(OPNControllerAccentBlackRGB(0.88)) : OpnColor(OPN::kTextSecondary);
    self.countLabel.layer.backgroundColor = (self.controllerFocused ? OpnColor(OPNControllerAccentSoftRGB(), 0.92) : OpnColor(0xFFFFFF, 0.070)).CGColor;
    [CATransaction commit];
}

- (void)loadThumbnailsFromGames:(const std::vector<OPN::GameInfo> &)games {
    NSInteger thumbnailIndex = 0;
    for (const OPN::GameInfo &game : games) {
        if (thumbnailIndex >= (NSInteger)self.thumbnailViews.count) break;
        NSArray<NSString *> *candidates = OPNControllerCategoryArtworkCandidates(game);
        if (candidates.count == 0) continue;
        [self loadImageForThumbnail:self.thumbnailViews[(NSUInteger)thumbnailIndex] candidates:candidates index:0];
        thumbnailIndex++;
    }
}

- (void)loadImageForThumbnail:(NSImageView *)thumbnail candidates:(NSArray<NSString *> *)candidates index:(NSUInteger)index {
    if (index >= candidates.count) return;
    NSURL *url = [NSURL URLWithString:candidates[index]];
    if (!url) {
        [self loadImageForThumbnail:thumbnail candidates:candidates index:index + 1];
        return;
    }
    __weak __typeof__(self) weakSelf = self;
    __weak NSImageView *weakThumbnail = thumbnail;
    [[[NSURLSession sharedSession] dataTaskWithURL:url completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        NSHTTPURLResponse *http = [response isKindOfClass:NSHTTPURLResponse.class] ? (NSHTTPURLResponse *)response : nil;
        if (error || !data || (http && http.statusCode >= 400)) {
            dispatch_async(dispatch_get_main_queue(), ^{
                __typeof__(self) strongSelf = weakSelf;
                NSImageView *strongThumbnail = weakThumbnail;
                if (strongSelf && strongThumbnail) [strongSelf loadImageForThumbnail:strongThumbnail candidates:candidates index:index + 1];
            });
            return;
        }
        NSImage *image = [[NSImage alloc] initWithData:data];
        if (!image) return;
        dispatch_async(dispatch_get_main_queue(), ^{
            NSImageView *strongThumbnail = weakThumbnail;
            if (strongThumbnail) strongThumbnail.image = image;
        });
    }] resume];
}

@end

@implementation OPNControllerGameHubView

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        _accentRGB = OPNControllerAccentRGB();
        _gameTitle = @"";
        _genreSummary = @"";
        _launchStatus = @"";
        _studioInfo = @"";
        _playerInfo = @"";
        _controlInfo = @"";
        _storeInfo = @"";
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (void)setAccentRGB:(unsigned)accentRGB {
    accentRGB &= 0xFFFFFF;
    if (_accentRGB == accentRGB) return;
    _accentRGB = accentRGB;
    [self setNeedsDisplay:YES];
}

- (void)setGameTitle:(NSString *)gameTitle {
    _gameTitle = [gameTitle copy] ?: @"";
    [self setNeedsDisplay:YES];
}

- (void)setGenreSummary:(NSString *)genreSummary {
    _genreSummary = [genreSummary copy] ?: @"";
    [self setNeedsDisplay:YES];
}

- (void)setLaunchStatus:(NSString *)launchStatus {
    _launchStatus = [launchStatus copy] ?: @"";
    [self setNeedsDisplay:YES];
}

- (void)setStudioInfo:(NSString *)studioInfo {
    _studioInfo = [studioInfo copy] ?: @"";
    [self setNeedsDisplay:YES];
}

- (void)setPlayerInfo:(NSString *)playerInfo {
    _playerInfo = [playerInfo copy] ?: @"";
    [self setNeedsDisplay:YES];
}

- (void)setControlInfo:(NSString *)controlInfo {
    _controlInfo = [controlInfo copy] ?: @"";
    [self setNeedsDisplay:YES];
}

- (void)setStoreInfo:(NSString *)storeInfo {
    _storeInfo = [storeInfo copy] ?: @"";
    [self setNeedsDisplay:YES];
}

- (NSDictionary<NSAttributedStringKey, id> *)attributesWithSize:(CGFloat)size
                                                         weight:(NSFontWeight)weight
                                                          color:(NSColor *)color
                                                      alignment:(NSTextAlignment)alignment {
    NSMutableParagraphStyle *paragraph = [[NSMutableParagraphStyle alloc] init];
    paragraph.lineBreakMode = NSLineBreakByTruncatingTail;
    paragraph.alignment = alignment;
    return @{
        NSFontAttributeName: [NSFont systemFontOfSize:size weight:weight],
        NSForegroundColorAttributeName: color,
        NSParagraphStyleAttributeName: paragraph,
    };
}

- (void)drawStatusRowWithTitle:(NSString *)title value:(NSString *)value y:(CGFloat)y {
    NSRect bounds = self.bounds;
    CGFloat x = 22.0;
    CGFloat width = MAX(0.0, NSWidth(bounds) - 44.0);
    NSRect rowRect = NSMakeRect(x, y, width, 38.0);
    NSBezierPath *row = [NSBezierPath bezierPathWithRoundedRect:rowRect xRadius:13.0 yRadius:13.0];
    [OpnColor(0xFFFFFF, 0.045) setFill];
    [row fill];
    OPNStrokePath(row, OpnColor(0xFFFFFF, 0.070), 1.0);

    [title drawInRect:NSMakeRect(NSMinX(rowRect) + 14.0, NSMinY(rowRect) + 11.0, width * 0.35, 18.0)
       withAttributes:[self attributesWithSize:11.0 weight:NSFontWeightSemibold color:OpnColor(0xF4FFF7, 0.58) alignment:NSTextAlignmentLeft]];
    [value drawInRect:NSMakeRect(NSMinX(rowRect) + width * 0.34, NSMinY(rowRect) + 10.0, width * 0.62, 18.0)
       withAttributes:[self attributesWithSize:12.0 weight:NSFontWeightSemibold color:OpnColor(0xFFFFFF, 0.88) alignment:NSTextAlignmentRight]];
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    NSRect bounds = self.bounds;
    if (NSWidth(bounds) < 80.0 || NSHeight(bounds) < 80.0) return;

    NSBezierPath *panel = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(bounds, 0.5, 0.5) xRadius:24.0 yRadius:24.0];
    NSGradient *panelGradient = [[NSGradient alloc] initWithColors:@[
        OpnColor(OPNControllerAccentBlackRGB(0.93), 0.80),
        OpnColor(OPNControllerAccentBlackRGB(0.88), 0.64),
        OpnColor(0x05070A, 0.70),
    ]];
    [panelGradient drawInBezierPath:panel angle:-18.0];
    OPNStrokePath(panel, OpnColor(0xFFFFFF, 0.15), 1.0);

    NSRect glowRect = NSMakeRect(22.0, 20.0, 72.0, 4.0);
    NSBezierPath *glow = [NSBezierPath bezierPathWithRoundedRect:glowRect xRadius:2.0 yRadius:2.0];
    [OpnColor(self.accentRGB, 0.86) setFill];
    [glow fill];

    [@"GAME HUB" drawInRect:NSMakeRect(22.0, 34.0, NSWidth(bounds) - 44.0, 18.0)
             withAttributes:[self attributesWithSize:11.0 weight:NSFontWeightBold color:OpnColor(0xFFFFFF, 0.55) alignment:NSTextAlignmentLeft]];
    [self.gameTitle drawInRect:NSMakeRect(22.0, 56.0, NSWidth(bounds) - 44.0, 31.0)
                withAttributes:[self attributesWithSize:23.0 weight:NSFontWeightSemibold color:OpnColor(OPN::kTextPrimary) alignment:NSTextAlignmentLeft]];
    [self.genreSummary drawInRect:NSMakeRect(22.0, 88.0, NSWidth(bounds) - 44.0, 20.0)
                  withAttributes:[self attributesWithSize:12.0 weight:NSFontWeightMedium color:OpnColor(0xF4FFF7, 0.68) alignment:NSTextAlignmentLeft]];

    CGFloat playY = 126.0;
    NSRect playRect = NSMakeRect(22.0, playY, NSWidth(bounds) - 44.0, 54.0);
    NSBezierPath *play = [NSBezierPath bezierPathWithRoundedRect:playRect xRadius:20.0 yRadius:20.0];
    NSGradient *playGradient = [[NSGradient alloc] initWithStartingColor:OpnColor(self.accentRGB, 0.92)
                                                             endingColor:OpnColor(OPNControllerAccentSoftRGB(), 0.92)];
    [playGradient drawInBezierPath:play angle:0.0];
    [@"Play Now" drawInRect:NSMakeRect(NSMinX(playRect) + 18.0, NSMinY(playRect) + 15.0, NSWidth(playRect) * 0.48, 24.0)
             withAttributes:[self attributesWithSize:17.0 weight:NSFontWeightBold color:OpnColor(OPNControllerAccentBlackRGB(0.92)) alignment:NSTextAlignmentLeft]];
    [self.launchStatus drawInRect:NSMakeRect(NSMinX(playRect) + NSWidth(playRect) * 0.46, NSMinY(playRect) + 17.0, NSWidth(playRect) * 0.46, 20.0)
                   withAttributes:[self attributesWithSize:12.0 weight:NSFontWeightSemibold color:OpnColor(OPNControllerAccentBlackRGB(0.88), 0.74) alignment:NSTextAlignmentRight]];

    CGFloat rowY = playY + 72.0;
    [self drawStatusRowWithTitle:@"STUDIO" value:self.studioInfo y:rowY];
    [self drawStatusRowWithTitle:@"PLAYERS" value:self.playerInfo y:rowY + 46.0];
    [self drawStatusRowWithTitle:@"CONTROLS" value:self.controlInfo y:rowY + 92.0];
    [self drawStatusRowWithTitle:@"STORES" value:self.storeInfo y:rowY + 138.0];
}

@end

@implementation OPNControllerPromptBarView

- (BOOL)isFlipped { return YES; }

- (void)setIncludeStore:(BOOL)includeStore {
    if (_includeStore == includeStore) return;
    _includeStore = includeStore;
    [self setNeedsDisplay:YES];
}

- (void)setIncludeBack:(BOOL)includeBack {
    if (_includeBack == includeBack) return;
    _includeBack = includeBack;
    [self setNeedsDisplay:YES];
}

- (NSArray<NSDictionary<NSString *, NSString *> *> *)promptItems {
    NSMutableArray<NSDictionary<NSString *, NSString *> *> *items = [NSMutableArray arrayWithObjects:
        @{@"button": @"primary", @"title": @"Play"},
        @{@"button": @"favorite", @"title": @"Favorite"}, nil];
    if (self.includeStore) [items addObject:@{@"button": @"store", @"title": @"Store"}];
    [items addObject:@{@"button": @"category", @"title": @"Categories"}];
    if (self.includeBack) [items addObject:@{@"button": @"back", @"title": @"Back"}];
    return items;
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    OPNControllerPromptStyle style = OPNCurrentControllerPromptStyle();
    CGFloat x = 0.0;
    CGFloat y = 0.0;
    NSDictionary<NSAttributedStringKey, id> *labelAttributes = @{
        NSFontAttributeName: [NSFont systemFontOfSize:13.0 weight:NSFontWeightSemibold],
        NSForegroundColorAttributeName: OpnColor(0xF1FFF5, 0.82),
    };

    for (NSDictionary<NSString *, NSString *> *item in [self promptItems]) {
        NSString *title = item[@"title"] ?: @"";
        NSString *button = item[@"button"] ?: @"";
        CGFloat titleWidth = ceil([title sizeWithAttributes:labelAttributes].width);
        CGFloat chipWidth = MAX(82.0, titleWidth + 50.0);
        NSRect chipRect = NSMakeRect(x, y, chipWidth, 34.0);
        NSBezierPath *chip = [NSBezierPath bezierPathWithRoundedRect:chipRect xRadius:17.0 yRadius:17.0];
        [OpnColor(OPNControllerAccentRGB(), 0.070) setFill];
        [chip fill];
        OPNStrokePath(chip, OpnColor(OPNControllerAccentSoftRGB(), 0.14), 1.0);

        NSImage *icon = OPNControllerPromptIcon(button, style);
        [icon drawInRect:NSMakeRect(NSMinX(chipRect) + 9.0, NSMinY(chipRect) + 5.0, 24.0, 24.0)
                fromRect:NSZeroRect
               operation:NSCompositingOperationSourceOver
                fraction:1.0
          respectFlipped:YES
                   hints:@{NSImageHintInterpolation: @(NSImageInterpolationHigh)}];

        [title drawInRect:NSMakeRect(NSMinX(chipRect) + 39.0, NSMinY(chipRect) + 8.0, chipWidth - 47.0, 18.0)
           withAttributes:labelAttributes];
        x += chipWidth + 12.0;
    }
}

@end

@implementation OPNGameCatalogView

using namespace OPN;

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        _cardViews = [NSMutableArray array];
        _categoryCardViews = [NSMutableArray array];
        _categoryButtons = [NSMutableArray array];
        _categoryItems = @[];
        _selectedCategoryId = @"all";
        NSArray<NSString *> *storedFavorites = [NSUserDefaults.standardUserDefaults arrayForKey:OPNFavoriteGameIdsDefaultsKey];
        _favoriteGameIds = [NSMutableSet setWithArray:[storedFavorites isKindOfClass:NSArray.class] ? storedFavorites : @[]];
        _selectedSortId = @"last_played";
        _selectedFilterIds = [NSMutableSet set];
        _focusedCardIndex = -1;
        _focusedCategoryIndex = 0;
        _controllerOverviewSpecialTileKind = OPNControllerOverviewSpecialTileNone;
        _controllerCategoryOverviewVisible = YES;
        _lastPlayedImageAspectRatio = 16.0 / 9.0;
        _gridColumnCount = 1;
        self.wantsLayer = YES;
        self.layer.opaque = NO;
        self.layer.backgroundColor = [NSColor clearColor].CGColor;

        _controllerElectricBackgroundView = [[OPNControllerElectricBackgroundView alloc] initWithFrame:self.bounds];
        _controllerElectricBackgroundView.hidden = YES;
        [self addSubview:_controllerElectricBackgroundView];

        _controllerDetailBackgroundView = [[OPNControllerPreviewBackgroundView alloc] initWithFrame:self.bounds];
        _controllerDetailBackgroundView.hidden = YES;
        _controllerDetailBackgroundView.wantsLayer = YES;
        _controllerDetailBackgroundView.layer.masksToBounds = YES;
        CIFilter *detailBackgroundBlur = [CIFilter filterWithName:@"CIGaussianBlur"];
        [detailBackgroundBlur setValue:@8.4 forKey:kCIInputRadiusKey];
        _controllerDetailBackgroundView.layer.filters = @[detailBackgroundBlur];
        [self addSubview:_controllerDetailBackgroundView];

        _libraryIconLabel = OpnLabel(@"", NSMakeRect(30, kNavHeight + 36, 0, 0),
                                     1, OpnColor(kBrandGreen), NSFontWeightBold);
        _libraryIconLabel.hidden = YES;
        [self addSubview:_libraryIconLabel];

        _titleLabel = OpnLabel(@"", NSMakeRect(0, 0, 0, 0),
                               28, OpnColor(kTextPrimary), NSFontWeightSemibold);
        _titleLabel.hidden = YES;
        [self addSubview:_titleLabel];

        _userLabel = OpnLabel(@"", NSMakeRect(24, kNavHeight + 40, 240, 18),
                              12, OpnColor(kTextMuted), NSFontWeightMedium);
        _userLabel.hidden = YES;
        [self addSubview:_userLabel];

        _signOutButton = [[NSButton alloc] initWithFrame:
            NSMakeRect(frame.size.width - 116, 58, 92, 30)];
        _signOutButton.title = @"Sign Out";
        _signOutButton.bordered = NO;
        _signOutButton.font = [NSFont systemFontOfSize:12 weight:NSFontWeightMedium];
        _signOutButton.contentTintColor = OpnColor(kTextMuted);
        _signOutButton.target = self;
        _signOutButton.action = @selector(signOutClicked);
        _signOutButton.hidden = YES;
        [self addSubview:_signOutButton];

        _searchField = [[NSSearchField alloc] initWithFrame:NSMakeRect(238, kNavHeight + 27, 420, 38)];
        _searchField.cell = [[OPNCenteredSearchFieldCell alloc] initTextCell:@""];
        _searchField.placeholderString = @"Search your games";
        _searchField.delegate = self;
        _searchField.target = self;
        _searchField.action = @selector(searchChanged);
        _searchField.enabled = YES;
        _searchField.editable = YES;
        _searchField.selectable = YES;
        _searchField.font = [NSFont systemFontOfSize:14 weight:NSFontWeightMedium];
        _searchField.textColor = OpnColor(kTextPrimary);
        _searchField.bezeled = NO;
        _searchField.drawsBackground = NO;
        _searchField.focusRingType = NSFocusRingTypeNone;
        _searchField.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
        _searchField.wantsLayer = YES;
        _searchField.layer.cornerRadius = 14;
        _searchField.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.88), 0.92).CGColor;
        _searchField.layer.borderColor = OpnColor(0xFFFFFF, 0.13).CGColor;
        _searchField.layer.borderWidth = 1;
        if ([_searchField.cell respondsToSelector:@selector(setDrawsBackground:)]) {
            [(NSSearchFieldCell *)_searchField.cell setDrawsBackground:NO];
        }
        NSTextFieldCell *searchCell = (NSTextFieldCell *)_searchField.cell;
        searchCell.alignment = NSTextAlignmentCenter;
        NSMutableParagraphStyle *placeholderStyle = [[NSMutableParagraphStyle alloc] init];
        placeholderStyle.alignment = NSTextAlignmentCenter;
        searchCell.placeholderAttributedString = [[NSAttributedString alloc]
            initWithString:@"Search your games"
                attributes:@{NSForegroundColorAttributeName: OpnColor(kTextMuted, 0.82),
                             NSFontAttributeName: [NSFont systemFontOfSize:14 weight:NSFontWeightMedium],
                             NSParagraphStyleAttributeName: placeholderStyle}];
        [self addSubview:_searchField];

        _filterButton = [[NSButton alloc] initWithFrame:NSMakeRect(0, kNavHeight + 27, 110, 38)];
        _filterButton.title = @"Filters";
        _filterButton.bordered = NO;
        _filterButton.font = [NSFont systemFontOfSize:13 weight:NSFontWeightRegular];
        _filterButton.contentTintColor = OpnColor(kTextSecondary);
        _filterButton.target = self;
        _filterButton.action = @selector(filterClicked:);
        _filterButton.wantsLayer = YES;
        _filterButton.layer.cornerRadius = 11;
        _filterButton.layer.backgroundColor = OpnColor(kInputBackground, 0.74).CGColor;
        _filterButton.layer.borderColor = OpnColor(0xFFFFFF, 0.09).CGColor;
        _filterButton.layer.borderWidth = 1;
        [self addSubview:_filterButton];

        _sortButton = [[NSButton alloc] initWithFrame:NSMakeRect(674, kNavHeight + 27, 144, 38)];
        _sortButton.title = @"Last Played";
        _sortButton.bordered = NO;
        _sortButton.font = [NSFont systemFontOfSize:13 weight:NSFontWeightRegular];
        _sortButton.contentTintColor = OpnColor(kTextSecondary);
        _sortButton.target = self;
        _sortButton.action = @selector(sortClicked:);
        _sortButton.wantsLayer = YES;
        _sortButton.layer.cornerRadius = 11;
        _sortButton.layer.backgroundColor = OpnColor(kInputBackground, 0.74).CGColor;
        _sortButton.layer.borderColor = OpnColor(0xFFFFFF, 0.09).CGColor;
        _sortButton.layer.borderWidth = 1;
        [self addSubview:_sortButton];

        _gameCountLabel = OpnLabel(@"", NSMakeRect(0, 0, 0, 0),
                                   12, OpnColor(kTextMuted), NSFontWeightRegular, NSTextAlignmentRight);
        _gameCountLabel.hidden = YES;
        [self addSubview:_gameCountLabel];

        _categoryBarView = [[NSView alloc] initWithFrame:NSZeroRect];
        _categoryBarView.wantsLayer = YES;
        _categoryBarView.hidden = YES;
        [self addSubview:_categoryBarView];

        CGFloat gridY = kNavHeight + kToolbarHeight;
        NSRect scrollFrame = NSMakeRect(0, gridY, frame.size.width, frame.size.height - gridY);
        _scrollView = [[NSScrollView alloc] initWithFrame:scrollFrame];
        _scrollView.wantsLayer = YES;
        _scrollView.hasVerticalScroller = YES;
        _scrollView.hasHorizontalScroller = NO;
        _scrollView.autohidesScrollers = YES;
        _scrollView.drawsBackground = NO;
        _scrollView.borderType = NSNoBorder;
        _scrollView.layer.opaque = NO;
        _scrollView.layer.backgroundColor = NSColor.clearColor.CGColor;
        _scrollView.contentView.drawsBackground = NO;
        _scrollView.contentView.backgroundColor = NSColor.clearColor;
        _scrollView.contentView.wantsLayer = YES;
        _scrollView.contentView.layer.opaque = NO;
        _scrollView.contentView.layer.backgroundColor = NSColor.clearColor.CGColor;
        [self addSubview:_scrollView];


        _gridContentView = [[OPNFlippedGridDocumentView alloc] initWithFrame:NSMakeRect(0, 0, frame.size.width, 100)];
        _gridContentView.wantsLayer = YES;
        _gridContentView.layer.opaque = NO;
        _gridContentView.layer.backgroundColor = NSColor.clearColor.CGColor;
        _scrollView.documentView = _gridContentView;

        _statusLabel = OpnLabel(@"", NSMakeRect(0, gridY + 100, frame.size.width, 24),
                                15, OpnColor(kTextMuted));
        _statusLabel.alignment = NSTextAlignmentCenter;
        [self addSubview:_statusLabel];

        _controllerDetailView = [[OPNFlippedGridDocumentView alloc] initWithFrame:NSZeroRect];
        _controllerDetailView.hidden = YES;
        _controllerDetailView.wantsLayer = YES;
        _controllerDetailView.layer.cornerRadius = 0.0;
        _controllerDetailView.layer.borderWidth = 0.0;
        _controllerDetailView.layer.borderColor = OpnColor(0xFFFFFF, 0.0).CGColor;
        _controllerDetailView.layer.backgroundColor = NSColor.clearColor.CGColor;
        _controllerDetailView.layer.shadowColor = OpnColor(OPNControllerAccentRGB()).CGColor;
        _controllerDetailView.layer.shadowOpacity = 0.0;
        _controllerDetailView.layer.shadowRadius = 0.0;
        _controllerDetailView.layer.shadowOffset = CGSizeZero;
        _controllerDetailView.layer.transform = CATransform3DIdentity;

        _controllerDetailGradientLayer = [CAGradientLayer layer];
        _controllerDetailGradientLayer.colors = @[(id)NSColor.clearColor.CGColor,
                                                   (id)NSColor.clearColor.CGColor,
                                                   (id)NSColor.clearColor.CGColor];
        _controllerDetailGradientLayer.locations = @[@0.0, @0.46, @1.0];
        _controllerDetailGradientLayer.startPoint = CGPointMake(0.0, 0.0);
        _controllerDetailGradientLayer.endPoint = CGPointMake(1.0, 1.0);
        _controllerDetailGradientLayer.opacity = 0.0;
        [_controllerDetailView.layer addSublayer:_controllerDetailGradientLayer];

        _controllerDetailAccentLayer = [CALayer layer];
        _controllerDetailAccentLayer.backgroundColor = OpnColor(OPNControllerAccentSoftRGB(), 0.86).CGColor;
        _controllerDetailAccentLayer.cornerRadius = 2.0;
        [_controllerDetailView.layer addSublayer:_controllerDetailAccentLayer];
        [self addSubview:_controllerDetailView];

        _controllerDetailStatsLabel = OpnLabel(@"", NSZeroRect, 14.0, OpnColor(kTextSecondary), NSFontWeightMedium);
        [_controllerDetailView addSubview:_controllerDetailStatsLabel];

        _controllerDetailFeaturesLabel = OpnLabel(@"", NSZeroRect, 14.0, OpnColor(kTextMuted), NSFontWeightRegular);
        _controllerDetailFeaturesLabel.maximumNumberOfLines = 6;
        [_controllerDetailView addSubview:_controllerDetailFeaturesLabel];

        _controllerGameHubView = [[OPNControllerGameHubView alloc] initWithFrame:NSZeroRect];
        _controllerGameHubView.hidden = YES;
        [_controllerDetailView addSubview:_controllerGameHubView];

        _controllerPromptBarView = [[OPNControllerPromptBarView alloc] initWithFrame:NSZeroRect];
        _controllerPromptBarView.wantsLayer = YES;
        [_controllerDetailView addSubview:_controllerPromptBarView];

        _streamPipContainerView = [[NSView alloc] initWithFrame:NSZeroRect];
        _streamPipContainerView.hidden = YES;
        _streamPipContainerView.wantsLayer = YES;
        _streamPipContainerView.layer.cornerRadius = 22.0;
        _streamPipContainerView.layer.masksToBounds = NO;
        _streamPipContainerView.layer.backgroundColor = OpnColor(0x030507, 0.68).CGColor;
        _streamPipContainerView.layer.borderWidth = 1.0;
        _streamPipContainerView.layer.borderColor = OpnColor(0xFFFFFF, 0.16).CGColor;
        _streamPipContainerView.layer.shadowColor = NSColor.blackColor.CGColor;
        _streamPipContainerView.layer.shadowOpacity = 0.30;
        _streamPipContainerView.layer.shadowRadius = 24.0;
        _streamPipContainerView.layer.shadowOffset = CGSizeMake(0.0, 12.0);

        _streamPipHostView = [[NSView alloc] initWithFrame:NSZeroRect];
        _streamPipHostView.wantsLayer = YES;
        _streamPipHostView.layer.cornerRadius = 18.0;
        _streamPipHostView.layer.masksToBounds = YES;
        _streamPipHostView.layer.backgroundColor = NSColor.blackColor.CGColor;
        [_streamPipContainerView addSubview:_streamPipHostView];

        _streamPipTitleLabel = OpnLabel(@"Current Stream", NSZeroRect, 14.0, OpnColor(kTextPrimary), NSFontWeightSemibold);
        [_streamPipContainerView addSubview:_streamPipTitleLabel];
        _streamPipHintLabel = OpnLabel(@"Press A to return", NSZeroRect, 12.0, OpnColor(kTextSecondary), NSFontWeightMedium, NSTextAlignmentRight);
        [_streamPipContainerView addSubview:_streamPipHintLabel];
        [self addSubview:_streamPipContainerView];

        _lastPlayedPanelView = [[NSView alloc] initWithFrame:NSZeroRect];
        _lastPlayedPanelView.hidden = YES;
        _lastPlayedPanelView.wantsLayer = YES;
        _lastPlayedPanelView.layer.cornerRadius = 22.0;
        _lastPlayedPanelView.layer.masksToBounds = NO;
        _lastPlayedPanelView.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.90), 0.78).CGColor;
        _lastPlayedPanelView.layer.borderWidth = 1.0;
        _lastPlayedPanelView.layer.borderColor = OpnColor(0xFFFFFF, 0.16).CGColor;
        _lastPlayedPanelView.layer.shadowColor = NSColor.blackColor.CGColor;
        _lastPlayedPanelView.layer.shadowOpacity = 0.30;
        _lastPlayedPanelView.layer.shadowRadius = 24.0;
        _lastPlayedPanelView.layer.shadowOffset = CGSizeMake(0.0, 12.0);

        _lastPlayedImageView = [[NSImageView alloc] initWithFrame:NSZeroRect];
        _lastPlayedImageView.imageScaling = NSImageScaleProportionallyUpOrDown;
        _lastPlayedImageView.wantsLayer = YES;
        _lastPlayedImageView.layer.cornerRadius = 16.0;
        _lastPlayedImageView.layer.masksToBounds = YES;
        _lastPlayedImageView.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.82), 0.86).CGColor;
        [_lastPlayedPanelView addSubview:_lastPlayedImageView];

        _lastPlayedEyebrowLabel = OpnLabel(@"LAST PLAYED", NSZeroRect, 11.0, OpnColor(kBrandGreen), NSFontWeightBold);
        [_lastPlayedPanelView addSubview:_lastPlayedEyebrowLabel];

        _lastPlayedTitleLabel = OpnLabel(@"", NSZeroRect, 21.0, OpnColor(kTextPrimary), NSFontWeightBold);
        _lastPlayedTitleLabel.maximumNumberOfLines = 2;
        _lastPlayedTitleLabel.lineBreakMode = NSLineBreakByTruncatingTail;
        [_lastPlayedPanelView addSubview:_lastPlayedTitleLabel];

        _lastPlayedMetaLabel = OpnLabel(@"", NSZeroRect, 12.0, OpnColor(kTextSecondary), NSFontWeightSemibold);
        _lastPlayedMetaLabel.maximumNumberOfLines = 2;
        [_lastPlayedPanelView addSubview:_lastPlayedMetaLabel];

        _lastPlayedHintLabel = OpnLabel(@"Press A to play again", NSZeroRect, 12.0, OpnColor(kTextSecondary), NSFontWeightMedium, NSTextAlignmentRight);
        [_lastPlayedPanelView addSubview:_lastPlayedHintLabel];
        [self addSubview:_lastPlayedPanelView];

        _loadingView = [[OPNLoadingView alloc] initWithFrame:self.bounds
                                                      message:@"Loading games..."];
        _loadingView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        _loadingView.hidden = YES;
        [self addSubview:_loadingView];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(interfacePreferencesChanged:)
                                                     name:OPNInterfacePreferencesDidChangeNotification
                                                   object:nil];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(controllerDidConnect:)
                                                     name:GCControllerDidConnectNotification
                                                   object:nil];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(controllerDidDisconnect:)
                                                     name:GCControllerDidDisconnectNotification
                                                   object:nil];
        [self startGamepadNavigationIfNeeded];
        [self layoutCatalogSubviews];
    }
    return self;
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [self stopGamepadNavigation];
    [self stopControllerDetailBackgroundRotation];
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    if (self.window) {
        [self startGamepadNavigationIfNeeded];
        [self startControllerDetailBackgroundRotationIfNeeded];
    } else {
        [self stopGamepadNavigation];
        [self stopControllerDetailBackgroundRotation];
    }
}

- (void)applyControllerAccentColors {
    self.searchField.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.88), 0.92).CGColor;
    self.controllerDetailView.layer.backgroundColor = NSColor.clearColor.CGColor;
    self.controllerDetailView.layer.shadowColor = OpnColor(OPNControllerAccentRGB()).CGColor;
    self.controllerDetailGradientLayer.colors = @[(id)NSColor.clearColor.CGColor,
                                                  (id)NSColor.clearColor.CGColor,
                                                  (id)NSColor.clearColor.CGColor];
    self.controllerDetailGradientLayer.opacity = 0.0;
    self.controllerDetailAccentLayer.backgroundColor = OpnColor(OPNControllerAccentSoftRGB(), 0.86).CGColor;
    self.layer.backgroundColor = NSColor.clearColor.CGColor;
    [self.controllerElectricBackgroundView setNeedsDisplay:YES];
}

- (void)interfacePreferencesChanged:(NSNotification *)notification {
    (void)notification;
    [self applyControllerAccentColors];
    [self renderGrid];
    [self startGamepadNavigationIfNeeded];
}

- (BOOL)acceptsFirstResponder { return YES; }

- (BOOL)becomeFirstResponder {
    BOOL result = [super becomeFirstResponder];
    if (self.focusedCardIndex < 0 && self.cardViews.count > 0) [self focusCardAtIndex:0 scrollIntoView:NO];
    return result;
}

- (BOOL)isFlipped { return YES; }

- (void)mouseDown:(NSEvent *)event {
    NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
    if (!self.searchField.hidden && NSPointInRect(point, self.searchField.frame)) {
        [self.window makeFirstResponder:self.searchField];
        [self.searchField mouseDown:event];
        return;
    }
    [super mouseDown:event];
}

- (void)setUserName:(NSString *)name {
    _userLabel.stringValue = name ? [NSString stringWithFormat:@"Signed in as %@", name] : @"";
}

- (void)setStreamPictureInPictureView:(NSView *)view title:(NSString *)title {
    if (self.streamPipContentView == view) {
        self.streamPipTitleLabel.stringValue = title.length > 0 ? title : @"Current Stream";
        [self layoutCatalogSubviews];
        return;
    }

    [self.streamPipContentView removeFromSuperview];
    self.streamPipContentView = nil;
    self.streamPipTitleLabel.stringValue = title.length > 0 ? title : @"Current Stream";
    self.streamPipHintLabel.stringValue = @"Press A to return";

    if (view) {
        view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        view.frame = self.streamPipHostView.bounds;
        [self.streamPipHostView addSubview:view];
        self.streamPipContentView = view;
    } else {
        self.streamPipFocused = NO;
    }
    [self setStreamPipFocused:NO];
    [self layoutCatalogSubviews];
}

- (void)setLoading:(BOOL)loading {
    _loadingView.hidden = !loading;
    if (loading) {
        [_loadingView startAnimating];
        _statusLabel.stringValue = @"";
    } else {
        [_loadingView stopAnimating];
        _statusLabel.stringValue = @"";
    }
}

- (void)setError:(NSString *)message {
    _loadingView.hidden = YES;
    [_loadingView stopAnimating];
    _statusLabel.stringValue = message ? message : @"";
}

- (void)setGames:(const std::vector<OPN::GameInfo> &)games {
    _allGames = games;
    self.catalogTotalCount = (NSInteger)games.size();
    self.catalogSupportedCount = (NSInteger)games.size();
    [self rebuildCategoryBar];
    [self renderGrid];
    [self scrollLibraryToTop];
    dispatch_async(dispatch_get_main_queue(), ^{
        [self scrollLibraryToTop];
    });
}

- (const OPN::GameInfo *)currentLastPlayedGame {
    if (_allGames.empty() || ![self.selectedSortId isEqualToString:@"last_played"]) return nullptr;
    return &_allGames.front();
}

- (int)preferredVariantIndexForGame:(const OPN::GameInfo &)game {
    for (size_t index = 0; index < game.variants.size(); index++) {
        if (game.variants[index].librarySelected) return (int)index;
    }
    return 0;
}

- (void)setCatalogBrowseResult:(const OPN::CatalogBrowseResult &)result {
    _allGames = result.games;
    for (OPNGameCardView *card in self.cardViews) {
        NSString *identifier = [self favoriteIdentifierForGame:card.game];
        for (const OPN::GameInfo &game : _allGames) {
            NSString *candidate = [self favoriteIdentifierForGame:game];
            if (identifier.length == 0 || ![identifier isEqualToString:candidate]) continue;
            [card updateGame:game];
            break;
        }
    }
    self.catalogFilterGroups = result.filterGroups;
    self.catalogSortOptions = result.sortOptions;
    self.catalogTotalCount = result.totalCount;
    self.catalogSupportedCount = result.numberSupported;
    self.selectedSortId = OPNCatalogString(result.selectedSortId, @"last_played");
    [self.selectedFilterIds removeAllObjects];
    for (const std::string &filterId : result.selectedFilterIds) {
        if (!filterId.empty()) [self.selectedFilterIds addObject:[NSString stringWithUTF8String:filterId.c_str()]];
    }
    NSString *sortTitle = @"Last Played";
    for (const OPN::CatalogSortOption &option : self.catalogSortOptions) {
        if (option.id == result.selectedSortId) {
            sortTitle = OPNCatalogString(option.label, sortTitle);
            break;
        }
    }
    self.sortButton.title = sortTitle;
    self.filterButton.title = self.selectedFilterIds.count > 0
        ? [NSString stringWithFormat:@"Filters (%lu)", (unsigned long)self.selectedFilterIds.count]
        : @"Filters";
    self.searchField.stringValue = OPNCatalogString(result.searchQuery, @"");
    [self updateControllerDetailContent];
    [self rebuildCategoryBar];
    [self renderGrid];
    [self scrollLibraryToTop];
}

- (void)rebuildCategoryBar {
    NSMutableArray<NSDictionary<NSString *, NSString *> *> *items = [NSMutableArray array];
    [items addObject:@{@"id": @"all", @"title": @"All"}];
    [items addObject:@{@"id": @"favorites", @"title": @"Favorites"}];

    NSInteger libraryCount = 0;
    NSMutableDictionary<NSString *, NSNumber *> *storeCounts = [NSMutableDictionary dictionary];
    NSMutableDictionary<NSString *, NSString *> *storeTitles = [NSMutableDictionary dictionary];
    NSMutableDictionary<NSString *, NSNumber *> *genreCounts = [NSMutableDictionary dictionary];
    NSMutableDictionary<NSString *, NSString *> *genreTitles = [NSMutableDictionary dictionary];

    for (const OPN::GameInfo &game : self.allGames) {
        if (game.isInLibrary) libraryCount++;
        for (const std::string &storeValue : game.availableStores) {
            if (storeValue.empty()) continue;
            NSString *store = [NSString stringWithUTF8String:storeValue.c_str()];
            NSString *categoryId = OPNCategoryId(@"store", store);
            if (categoryId.length == 0) continue;
            storeCounts[categoryId] = @((storeCounts[categoryId] ?: @0).integerValue + 1);
            if (!storeTitles[categoryId]) storeTitles[categoryId] = OPNStoreCategoryTitle(store);
        }
        for (const std::string &genreValue : game.genres) {
            if (genreValue.empty()) continue;
            NSString *genre = [NSString stringWithUTF8String:genreValue.c_str()];
            NSString *categoryId = OPNCategoryId(@"genre", genre);
            if (categoryId.length == 0) continue;
            genreCounts[categoryId] = @((genreCounts[categoryId] ?: @0).integerValue + 1);
            if (!genreTitles[categoryId]) genreTitles[categoryId] = OPNCatalogDisplayLabel(genre);
        }
    }

    if (libraryCount > 0) [items addObject:@{@"id": @"library", @"title": @"Library"}];

    NSArray<NSString *> *sortedGenreIds = [[genreCounts allKeys] sortedArrayUsingComparator:^NSComparisonResult(NSString *a, NSString *b) {
        NSInteger countA = genreCounts[a].integerValue;
        NSInteger countB = genreCounts[b].integerValue;
        if (countA != countB) return countA > countB ? NSOrderedAscending : NSOrderedDescending;
        return [genreTitles[a] localizedCaseInsensitiveCompare:genreTitles[b]];
    }];
    for (NSString *categoryId in sortedGenreIds) {
        if (items.count >= 9) break;
        NSString *title = genreTitles[categoryId];
        if (title.length > 0) [items addObject:@{@"id": categoryId, @"title": title}];
    }

    NSArray<NSString *> *sortedStoreIds = [[storeCounts allKeys] sortedArrayUsingComparator:^NSComparisonResult(NSString *a, NSString *b) {
        NSInteger countA = storeCounts[a].integerValue;
        NSInteger countB = storeCounts[b].integerValue;
        if (countA != countB) return countA > countB ? NSOrderedAscending : NSOrderedDescending;
        return [storeTitles[a] localizedCaseInsensitiveCompare:storeTitles[b]];
    }];
    for (NSString *categoryId in sortedStoreIds) {
        if (items.count >= 14) break;
        NSString *title = storeTitles[categoryId];
        if (title.length > 0) [items addObject:@{@"id": categoryId, @"title": [@"Store: " stringByAppendingString:title]}];
    }

    BOOL selectedStillExists = NO;
    for (NSDictionary<NSString *, NSString *> *item in items) {
        if ([item[@"id"] isEqualToString:self.selectedCategoryId]) selectedStillExists = YES;
    }
    if (!selectedStillExists) self.selectedCategoryId = @"all";

    self.categoryItems = items;
    for (NSView *view in self.categoryBarView.subviews) [view removeFromSuperview];
    [self.categoryButtons removeAllObjects];

    CGFloat x = 0.0;
    for (NSDictionary<NSString *, NSString *> *item in items) {
        NSString *title = item[@"title"] ?: @"";
        CGFloat buttonWidth = MIN(130.0, MAX(60.0, title.length * 8.4 + 28.0));
        NSButton *button = [[NSButton alloc] initWithFrame:NSMakeRect(x, 0.0, buttonWidth, 30.0)];
        button.title = title;
        button.identifier = item[@"id"] ?: @"all";
        button.bordered = NO;
        button.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightSemibold];
        button.target = self;
        button.action = @selector(categoryButtonClicked:);
        button.wantsLayer = YES;
        button.layer.cornerRadius = 15.0;
        [self.categoryBarView addSubview:button];
        [self.categoryButtons addObject:button];
        x += buttonWidth + 8.0;
    }
}

- (void)categoryButtonClicked:(NSButton *)sender {
    NSString *categoryId = sender.identifier.length > 0 ? sender.identifier : @"all";
    if ([categoryId isEqualToString:self.selectedCategoryId]) return;
    self.selectedCategoryId = categoryId;
    self.focusedCardIndex = 0;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneChange);
    [self renderGrid];
    [self scrollLibraryToTop];
}

- (NSInteger)gameCountForCategory:(NSString *)categoryId {
    NSInteger count = 0;
    for (const OPN::GameInfo &game : self.allGames) {
        if ([self game:game matchesCategory:categoryId]) count++;
    }
    return count;
}

- (std::vector<OPN::GameInfo>)gamesForCategory:(NSString *)categoryId limit:(NSInteger)limit {
    std::vector<OPN::GameInfo> games;
    std::vector<OPN::GameInfo> matchingGames;
    if (limit <= 0) return games;
    for (const OPN::GameInfo &game : self.allGames) {
        if (![self game:game matchesCategory:categoryId]) continue;
        matchingGames.push_back(game);
    }

    NSInteger sampleCount = MIN(limit, (NSInteger)matchingGames.size());
    for (NSInteger i = 0; i < sampleCount; i++) {
        size_t index = (size_t)i;
        size_t remaining = matchingGames.size() - index;
        size_t randomOffset = remaining > 1 ? (size_t)arc4random_uniform((uint32_t)remaining) : 0;
        std::swap(matchingGames[index], matchingGames[index + randomOffset]);
        games.push_back(matchingGames[index]);
    }
    return games;
}

- (BOOL)game:(const OPN::GameInfo &)game matchesCategory:(NSString *)categoryId {
    if (categoryId.length == 0 || [categoryId isEqualToString:@"all"]) return YES;
    if ([categoryId isEqualToString:@"favorites"]) return [self isFavoriteGame:game];
    if ([categoryId isEqualToString:@"library"]) return game.isInLibrary;
    if ([categoryId hasPrefix:@"store:"]) {
        for (const std::string &storeValue : game.availableStores) {
            NSString *store = [NSString stringWithUTF8String:storeValue.c_str()];
            if ([OPNCategoryId(@"store", store) isEqualToString:categoryId]) return YES;
        }
        return NO;
    }
    if ([categoryId hasPrefix:@"genre:"]) {
        for (const std::string &genreValue : game.genres) {
            NSString *genre = [NSString stringWithUTF8String:genreValue.c_str()];
            if ([OPNCategoryId(@"genre", genre) isEqualToString:categoryId]) return YES;
        }
        return NO;
    }
    return YES;
}

- (NSString *)favoriteIdentifierForGame:(const OPN::GameInfo &)game {
    if (!game.id.empty()) return [NSString stringWithUTF8String:game.id.c_str()];
    if (!game.uuid.empty()) return [NSString stringWithUTF8String:game.uuid.c_str()];
    if (!game.launchAppId.empty()) return [NSString stringWithUTF8String:game.launchAppId.c_str()];
    return OPNCatalogString(game.title, @"");
}

- (BOOL)isFavoriteGame:(const OPN::GameInfo &)game {
    NSString *identifier = [self favoriteIdentifierForGame:game];
    return identifier.length > 0 && [self.favoriteGameIds containsObject:identifier];
}

- (void)persistFavoriteGameIds {
    NSArray<NSString *> *sortedIds = [[self.favoriteGameIds allObjects] sortedArrayUsingSelector:@selector(localizedCaseInsensitiveCompare:)];
    [NSUserDefaults.standardUserDefaults setObject:sortedIds forKey:OPNFavoriteGameIdsDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
}

- (void)toggleFavoriteForFocusedGame {
    OPNGameCardView *card = [self focusedCard];
    if (!card) return;
    NSString *identifier = [self favoriteIdentifierForGame:card.game];
    if (identifier.length == 0) return;
    if ([self.favoriteGameIds containsObject:identifier]) {
        [self.favoriteGameIds removeObject:identifier];
    } else {
        [self.favoriteGameIds addObject:identifier];
    }
    [self persistFavoriteGameIds];
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneChange);
    [self rebuildCategoryBar];
    if ([self.selectedCategoryId isEqualToString:@"favorites"] && ![self isFavoriteGame:card.game]) {
        self.focusedCardIndex = MIN(self.focusedCardIndex, MAX(0, (NSInteger)self.cardViews.count - 2));
        [self renderGrid];
    } else {
        [self updateControllerDetailContent];
        [self layoutCatalogSubviews];
    }
}

- (void)cycleCategoryBy:(NSInteger)delta {
    if (self.categoryItems.count <= 1 || delta == 0) return;
    NSInteger currentIndex = 0;
    for (NSUInteger i = 0; i < self.categoryItems.count; i++) {
        if ([self.categoryItems[i][@"id"] isEqualToString:self.selectedCategoryId]) {
            currentIndex = (NSInteger)i;
            break;
        }
    }
    NSInteger nextIndex = (currentIndex + delta) % (NSInteger)self.categoryItems.count;
    if (nextIndex < 0) nextIndex += (NSInteger)self.categoryItems.count;
    self.selectedCategoryId = self.categoryItems[(NSUInteger)nextIndex][@"id"] ?: @"all";
    self.focusedCardIndex = 0;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneChange);
    [self renderGrid];
    [self scrollLibraryToTop];
}

- (void)renderGrid {
    for (NSView *view in [self.gridContentView.subviews copy]) { [view removeFromSuperview]; }
    [_cardViews removeAllObjects];
    [self.categoryCardViews removeAllObjects];

    BOOL controllerMode = OpnControllerModeEnabled();
    if (controllerMode && self.controllerCategoryOverviewVisible) {
        [self renderControllerCategoryOverview];
        return;
    }

    CGFloat cardWidth = [OPNGameCardView cardSize].width;
    CGFloat cardHeight = [OPNGameCardView cardSize].height;
    CGFloat availableWidth = _scrollView.frame.size.width;
    NSInteger cols = controllerMode ? 1 : MAX(1, (NSInteger)((availableWidth + kCardSpacing) / (cardWidth + kCardSpacing)));
    self.gridColumnCount = cols;
    CGFloat gridSpacing = controllerMode ? 26.0 : (cols > 1 ? floor((availableWidth - cols * cardWidth) / (cols - 1)) : kCardSpacing);
    gridSpacing = MAX(kCardSpacing, gridSpacing);
    CGFloat xStart = controllerMode ? 64.0 : (cols > 1 ? 0.0 : floor(MAX(0.0, (_scrollView.frame.size.width - cardWidth) / 2.0)));
    CGFloat yPos = controllerMode ? 34.0 + kControllerRailSelectorOverlap : kGridPadding;

    std::vector<OPN::GameInfo> displayGames;
    for (const OPN::GameInfo &game : _allGames) {
        if ([self game:game matchesCategory:self.selectedCategoryId]) displayGames.push_back(game);
    }

    NSInteger col = 0;
    NSInteger visibleCount = 0;
    for (auto it = displayGames.begin(); it != displayGames.end(); ++it) {
        auto &game = *it;
        CGFloat x = controllerMode ? xStart + visibleCount * (cardWidth + gridSpacing) : xStart + col * (cardWidth + gridSpacing);
        NSRect cardFrame = NSMakeRect(x, yPos, cardWidth, cardHeight);
        OPNGameCardView *card = [[OPNGameCardView alloc] initWithFrame:cardFrame game:game];
        GameInfo gameCopy = game;
        __weak __typeof__(self) weakSelf = self;
        __weak OPNGameCardView *weakCard = card;
        card.onPlay = ^{
            __typeof__(self) s = weakSelf;
            OPNGameCardView *c = weakCard;
            if (!s || !c) return;
            NSUInteger cardIndex = [s.cardViews indexOfObject:c];
            if (cardIndex != NSNotFound) [s focusCardAtIndex:(NSInteger)cardIndex scrollIntoView:NO];
            if (OpnControllerModeEnabled()) {
                [s launchFocusedGame];
            } else if (s.onSelectGame) {
                int variantIdx = c.selectedVariantIndex;
                s.onSelectGame(gameCopy, variantIdx >= 0 ? variantIdx : 0);
            }
        };
        card.onArtworkAccentColorChanged = ^(NSColor *color) {
            (void)color;
            __typeof__(self) s = weakSelf;
            OPNGameCardView *c = weakCard;
            if (!s || !c) return;
            NSUInteger cardIndex = [s.cardViews indexOfObject:c];
            if (cardIndex == NSNotFound || (NSInteger)cardIndex != s.focusedCardIndex) return;
            [s updateControllerDetailContent];
        };
        [_gridContentView addSubview:card];
        [_cardViews addObject:card];

        col++;
        visibleCount++;
        if (!controllerMode && col >= cols) {
            col = 0;
            yPos += cardHeight + kCardSpacing;
        }
    }

    CGFloat totalHeight = controllerMode ? cardHeight + 104.0 : yPos + cardHeight + kGridPadding;
    if (!controllerMode && col == 0 && visibleCount > 0) totalHeight = yPos + kGridPadding;
    CGFloat totalWidth = controllerMode
        ? xStart * 2.0 + visibleCount * cardWidth + MAX(0, visibleCount - 1) * gridSpacing
        : _scrollView.frame.size.width;
    _gridContentView.frame = NSMakeRect(0, 0,
        MAX(totalWidth, _scrollView.frame.size.width),
        MAX(totalHeight, _scrollView.frame.size.height));

    _gameCountLabel.stringValue = [NSString stringWithFormat:@"%ld %@", (long)visibleCount, visibleCount == 1 ? @"game" : @"games"];
    if (self.onGameCountChanged) self.onGameCountChanged(visibleCount);
    _statusLabel.stringValue = visibleCount == 0 ? @"No games found." : @"";
    if (self.focusedCardIndex >= (NSInteger)self.cardViews.count) self.focusedCardIndex = (NSInteger)self.cardViews.count - 1;
    if (self.focusedCardIndex < 0 && self.cardViews.count > 0) self.focusedCardIndex = 0;
    [self focusCardAtIndex:self.focusedCardIndex scrollIntoView:NO];
    [self updateControllerDetailContent];
    [self layoutCatalogSubviews];
}

- (void)renderControllerCategoryOverview {
    [self updateLastPlayedPanel];

    BOOL showStreamTile = self.streamPipContentView != nil;
    BOOL showLastPlayedTile = !showStreamTile && !self.lastPlayedPanelView.hidden;
    self.controllerOverviewSpecialTileKind = showStreamTile
        ? OPNControllerOverviewSpecialTileStream
        : (showLastPlayedTile ? OPNControllerOverviewSpecialTileLastPlayed : OPNControllerOverviewSpecialTileNone);

    CGFloat scale = OpnControllerGridItemScale();
    CGFloat spacing = floor(34.0 * scale);
    CGFloat cardWidth = floor(164.0 * scale);
    CGFloat cardHeight = floor(cardWidth * 178.0 / 220.0);
    CGFloat railInset = floor(34.0 * scale);
    CGFloat railY = floor(42.0 * scale);
    self.gridColumnCount = 1;

    NSView *specialTileView = nil;
    if (self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileStream) {
        specialTileView = self.streamPipContainerView;
        self.streamPipContainerView.hidden = NO;
        self.lastPlayedPanelView.hidden = YES;
    } else if (self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileLastPlayed) {
        specialTileView = self.lastPlayedPanelView;
        self.streamPipContainerView.hidden = YES;
        self.lastPlayedPanelView.hidden = NO;
    } else {
        self.streamPipContainerView.hidden = YES;
        self.lastPlayedPanelView.hidden = YES;
    }

    CGFloat nextX = railInset;
    CGFloat railHeight = cardHeight;
    if (specialTileView) {
        [self.gridContentView addSubview:specialTileView];
        CGFloat tileWidth = floor(cardWidth * 1.92);
        CGFloat tileHeight = floor(cardHeight * 1.74);
        NSRect tileFrame = NSMakeRect(nextX, 0.0, tileWidth, tileHeight);
        specialTileView.frame = tileFrame;
        railHeight = MAX(railHeight, tileHeight);
        nextX = NSMaxX(tileFrame) + spacing;
        if (self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileStream) {
            CGFloat videoWidth = MAX(120.0, tileWidth - 24.0);
            CGFloat maxVideoHeight = MAX(70.0, tileHeight - 54.0);
            CGFloat videoHeight = MIN(maxVideoHeight, floor(videoWidth * 9.0 / 16.0));
            videoWidth = MIN(videoWidth, floor(videoHeight * 16.0 / 9.0));
            self.streamPipHostView.frame = NSMakeRect(floor((tileWidth - videoWidth) * 0.5), 12.0, videoWidth, videoHeight);
            self.streamPipContentView.frame = self.streamPipHostView.bounds;
            self.streamPipTitleLabel.frame = NSMakeRect(16.0, videoHeight + 22.0, tileWidth * 0.50, 22.0);
            self.streamPipHintLabel.frame = NSMakeRect(tileWidth * 0.50 - 12.0, videoHeight + 24.0, tileWidth * 0.50, 18.0);
        } else {
            CGFloat contentWidth = tileWidth - 32.0;
            CGFloat imageMaxHeight = MAX(64.0, MIN(tileHeight * 0.54, tileHeight - 132.0));
            CGFloat aspectRatio = self.lastPlayedImageAspectRatio > 0.1 ? self.lastPlayedImageAspectRatio : 16.0 / 9.0;
            CGFloat imageWidth = MIN(contentWidth, floor(imageMaxHeight * aspectRatio));
            CGFloat imageHeight = floor(imageWidth / aspectRatio);
            if (imageHeight > imageMaxHeight) {
                imageHeight = imageMaxHeight;
                imageWidth = floor(imageHeight * aspectRatio);
            }
            CGFloat imageX = floor((tileWidth - imageWidth) * 0.5);
            self.lastPlayedImageView.frame = NSMakeRect(imageX, 12.0, imageWidth, imageHeight);
            CGFloat labelY = NSMaxY(self.lastPlayedImageView.frame) + 12.0;
            CGFloat labelWidth = tileWidth - 32.0;
            CGFloat hintY = tileHeight - 26.0;
            CGFloat titleY = labelY + 18.0;
            CGFloat titleHeight = MIN(46.0, MAX(24.0, hintY - titleY - 40.0));
            CGFloat metaY = titleY + titleHeight + 4.0;
            CGFloat metaHeight = MIN(32.0, MAX(0.0, hintY - metaY - 4.0));
            self.lastPlayedEyebrowLabel.frame = NSMakeRect(16.0, labelY, labelWidth, 16.0);
            self.lastPlayedTitleLabel.frame = NSMakeRect(16.0, titleY, labelWidth, titleHeight);
            self.lastPlayedMetaLabel.frame = NSMakeRect(16.0, metaY, labelWidth, metaHeight);
            self.lastPlayedHintLabel.frame = NSMakeRect(16.0, hintY, labelWidth, 18.0);
        }
    }

    CGFloat cardY = floor((railHeight - cardHeight) * 0.5);
    {
        std::vector<OPN::GameInfo> emptyGames;
        OPNControllerCategoryCardView *settingsCard = [[OPNControllerCategoryCardView alloc] initWithFrame:NSMakeRect(nextX, cardY, cardWidth, cardHeight)
                                                                                                       title:@"Interface Settings"
                                                                                                  categoryId:@"system:interface-settings"
                                                                                                   gameCount:1
                                                                                                      games:emptyGames];
        __weak __typeof__(self) weakSelf = self;
        __weak OPNControllerCategoryCardView *weakCard = settingsCard;
        settingsCard.onSelect = ^{
            __typeof__(self) strongSelf = weakSelf;
            OPNControllerCategoryCardView *strongCard = weakCard;
            if (!strongSelf || !strongCard) return;
            NSUInteger cardIndex = [strongSelf.categoryCardViews indexOfObject:strongCard];
            if (cardIndex != NSNotFound) [strongSelf focusCategoryAtIndex:(NSInteger)cardIndex + [strongSelf controllerOverviewCategoryOffset] scrollIntoView:NO];
            [strongSelf openFocusedCategory];
        };
        [self.gridContentView addSubview:settingsCard];
        [self.categoryCardViews addObject:settingsCard];
        nextX = NSMaxX(settingsCard.frame) + spacing;
    }

    for (NSDictionary<NSString *, NSString *> *item in self.categoryItems) {
        NSString *categoryId = item[@"id"] ?: @"all";
        NSString *title = item[@"title"] ?: @"Category";
        if ([categoryId isEqualToString:@"all"]) title = @"All Games";
        if ([categoryId hasPrefix:@"store:"] && ![title hasPrefix:@"Store:"]) title = [@"Store: " stringByAppendingString:title];
        NSInteger gameCount = [self gameCountForCategory:categoryId];
        if (gameCount <= 0 && ![categoryId isEqualToString:@"favorites"]) continue;
        std::vector<OPN::GameInfo> thumbnailGames = [self gamesForCategory:categoryId limit:6];
        OPNControllerCategoryCardView *card = [[OPNControllerCategoryCardView alloc] initWithFrame:NSMakeRect(nextX, cardY, cardWidth, cardHeight)
                                                                                               title:title
                                                                                          categoryId:categoryId
                                                                                           gameCount:gameCount
                                                                                              games:thumbnailGames];
        __weak __typeof__(self) weakSelf = self;
        __weak OPNControllerCategoryCardView *weakCard = card;
        card.onSelect = ^{
            __typeof__(self) strongSelf = weakSelf;
            OPNControllerCategoryCardView *strongCard = weakCard;
            if (!strongSelf || !strongCard) return;
            NSUInteger cardIndex = [strongSelf.categoryCardViews indexOfObject:strongCard];
            if (cardIndex != NSNotFound) [strongSelf focusCategoryAtIndex:(NSInteger)cardIndex + [strongSelf controllerOverviewCategoryOffset] scrollIntoView:NO];
            [strongSelf openFocusedCategory];
        };
        [self.gridContentView addSubview:card];
        [self.categoryCardViews addObject:card];
        nextX = NSMaxX(card.frame) + spacing;
    }

    self.gridContentView.frame = NSMakeRect(0.0,
                                             0.0,
                                             MAX(self.scrollView.frame.size.width, nextX + railInset),
                                             MAX(self.scrollView.frame.size.height, railHeight + railY));
    NSInteger itemCount = [self controllerOverviewItemCount];
    if (self.focusedCategoryIndex >= itemCount) self.focusedCategoryIndex = itemCount - 1;
    if (self.focusedCategoryIndex < 0 && itemCount > 0) self.focusedCategoryIndex = 0;
    [self focusCategoryAtIndex:self.focusedCategoryIndex scrollIntoView:NO];
    NSInteger totalCount = [self gameCountForCategory:@"all"];
    self.gameCountLabel.stringValue = [NSString stringWithFormat:@"%ld %@", (long)totalCount, totalCount == 1 ? @"game" : @"games"];
    if (self.onGameCountChanged) self.onGameCountChanged(totalCount);
    self.statusLabel.stringValue = self.categoryCardViews.count == 0 ? @"No game categories found." : @"";
    [self layoutCatalogSubviews];
}

- (void)scrollLibraryToTop {
    NSClipView *clipView = self.scrollView.contentView;
    [clipView scrollToPoint:NSMakePoint(0, 0)];
    [self.scrollView reflectScrolledClipView:clipView];
}

- (void)sortClicked:(NSButton *)sender {
    NSMenu *menu = [[NSMenu alloc] initWithTitle:@"Sort Games"];
    if (self.catalogSortOptions.empty()) {
        NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:self.sortButton.title action:nil keyEquivalent:@""];
        [menu addItem:item];
    }
    for (const OPN::CatalogSortOption &option : self.catalogSortOptions) {
        NSString *sortId = OPNCatalogString(option.id, @"last_played");
        NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:OPNCatalogString(option.label, sortId)
                                                      action:@selector(sortMenuItemSelected:)
                                               keyEquivalent:@""];
        item.target = self;
        item.representedObject = sortId;
        item.state = [self.selectedSortId isEqualToString:sortId] ? NSControlStateValueOn : NSControlStateValueOff;
        [menu addItem:item];
    }
    [menu popUpMenuPositioningItem:nil
                        atLocation:NSMakePoint(0.0, NSHeight(sender.bounds) + 4.0)
                            inView:sender];
}

- (void)sortMenuItemSelected:(NSMenuItem *)sender {
    NSString *sortId = [sender.representedObject isKindOfClass:NSString.class] ? sender.representedObject : @"last_played";
    self.selectedSortId = sortId;
    self.sortButton.title = sender.title;
    [self requestCatalogBrowse];
}

- (void)filterClicked:(NSButton *)sender {
    NSMenu *menu = [[NSMenu alloc] initWithTitle:@"Filters"];
    BOOL addedAny = NO;
    for (const OPN::CatalogFilterGroup &group : self.catalogFilterGroups) {
        if (group.id != "digital_store" && group.id != "genre" && group.id != "subscriptions") continue;
        if (addedAny) [menu addItem:[NSMenuItem separatorItem]];
        NSString *groupTitle = OPNCatalogString(group.label, @"Filters");
        NSMenuItem *heading = [[NSMenuItem alloc] initWithTitle:groupTitle action:nil keyEquivalent:@""];
        heading.enabled = NO;
        [menu addItem:heading];
        NSInteger limit = group.id == "genre" ? 8 : (NSInteger)group.options.size();
        NSInteger index = 0;
        for (const OPN::CatalogFilterOption &option : group.options) {
            if (index++ >= limit) break;
            NSString *filterId = OPNCatalogString(option.id, @"");
            if (filterId.length == 0) continue;
            NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:OPNCatalogString(option.label, filterId)
                                                          action:@selector(filterMenuItemSelected:)
                                                   keyEquivalent:@""];
            item.target = self;
            item.representedObject = filterId;
            item.state = [self.selectedFilterIds containsObject:filterId] ? NSControlStateValueOn : NSControlStateValueOff;
            [menu addItem:item];
            addedAny = YES;
        }
    }
    if (!addedAny) {
        NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:@"No filters available" action:nil keyEquivalent:@""];
        [menu addItem:item];
    } else if (self.selectedFilterIds.count > 0) {
        [menu addItem:[NSMenuItem separatorItem]];
        NSMenuItem *clear = [[NSMenuItem alloc] initWithTitle:@"Clear Filters" action:@selector(clearFiltersSelected:) keyEquivalent:@""];
        clear.target = self;
        [menu addItem:clear];
    }
    [menu popUpMenuPositioningItem:nil
                        atLocation:NSMakePoint(0.0, NSHeight(sender.bounds) + 4.0)
                            inView:sender];
}

- (void)filterMenuItemSelected:(NSMenuItem *)sender {
    NSString *filterId = [sender.representedObject isKindOfClass:NSString.class] ? sender.representedObject : nil;
    if (filterId.length == 0) return;
    if ([self.selectedFilterIds containsObject:filterId]) {
        [self.selectedFilterIds removeObject:filterId];
    } else {
        [self.selectedFilterIds addObject:filterId];
    }
    self.filterButton.title = self.selectedFilterIds.count > 0
        ? [NSString stringWithFormat:@"Filters (%lu)", (unsigned long)self.selectedFilterIds.count]
        : @"Filters";
    [self requestCatalogBrowse];
}

- (void)clearFiltersSelected:(NSMenuItem *)sender {
    (void)sender;
    [self.selectedFilterIds removeAllObjects];
    self.filterButton.title = @"Filters";
    [self requestCatalogBrowse];
}

- (void)layout {
    [super layout];
    [self layoutCatalogSubviews];
    if (std::fabs(self.lastLayoutWidth - NSWidth(self.bounds)) > 1.0) {
        self.lastLayoutWidth = NSWidth(self.bounds);
        self.needsGridRenderAfterResize = YES;
    }
}

- (void)viewDidEndLiveResize {
    [super viewDidEndLiveResize];
    if (!self.needsGridRenderAfterResize || self.allGames.empty()) return;
    self.needsGridRenderAfterResize = NO;
    [self renderGrid];
}

- (void)layoutCatalogSubviews {
    CGFloat width = NSWidth(self.bounds);
    CGFloat height = NSHeight(self.bounds);
    BOOL controllerMode = OpnControllerModeEnabled();
    CGFloat controllerNavHeight = 118.0;
    self.controllerElectricBackgroundView.hidden = YES;
    self.controllerElectricBackgroundView.frame = self.bounds;
    self.scrollView.hasVerticalScroller = !controllerMode;
    self.scrollView.hasHorizontalScroller = NO;
    self.scrollView.drawsBackground = NO;
    self.scrollView.layer.opaque = NO;
    self.scrollView.layer.backgroundColor = NSColor.clearColor.CGColor;
    self.scrollView.contentView.drawsBackground = NO;
    self.scrollView.contentView.backgroundColor = NSColor.clearColor;
    self.scrollView.contentView.layer.opaque = NO;
    self.scrollView.contentView.layer.backgroundColor = NSColor.clearColor.CGColor;
    self.gridContentView.layer.opaque = NO;
    self.gridContentView.layer.backgroundColor = NSColor.clearColor.CGColor;
    BOOL compact = width < 900.0;
    self.searchField.hidden = controllerMode;
    self.filterButton.hidden = controllerMode || compact;
    self.signOutButton.hidden = YES;
    CGFloat searchWidth = compact ? MAX(240.0, width - 64.0) : MIN(520.0, MAX(360.0, width * 0.38));
    CGFloat searchX = floor((width - searchWidth) / 2.0);
    self.libraryIconLabel.frame = NSMakeRect(0, 0, 0, 0);
    self.titleLabel.frame = NSMakeRect(0, 0, 0, 0);
    self.userLabel.frame = NSMakeRect(24, kNavHeight + 36, 260, 18);
    self.searchField.frame = NSMakeRect(searchX, compact ? kNavHeight + 62 : kNavHeight + 25, searchWidth, 40);
    self.sortButton.hidden = controllerMode || compact;
    self.filterButton.frame = NSMakeRect(NSMaxX(self.searchField.frame) + 14, kNavHeight + 26, 124, 38);
    self.sortButton.frame = NSMakeRect(NSMaxX(self.filterButton.frame) + 10, kNavHeight + 26, 154, 38);
    self.gameCountLabel.frame = NSMakeRect(0, 0, 0, 0);
    self.gameCountLabel.hidden = YES;
    self.signOutButton.frame = NSMakeRect(width - 116, kNavHeight + 13, 92, 30);
    BOOL categoryOverview = controllerMode && self.controllerCategoryOverviewVisible;
    if (categoryOverview) {
        self.categoryBarView.hidden = YES;
        self.controllerDetailView.hidden = YES;
        self.controllerDetailBackgroundView.hidden = YES;
        [self stopControllerDetailBackgroundRotation];
        self.controllerGameHubView.hidden = YES;
        self.controllerPromptBarView.hidden = YES;
        self.scrollView.hasVerticalScroller = NO;
        self.scrollView.hasHorizontalScroller = YES;
        CGFloat gridY = controllerNavHeight + 28.0;
        self.scrollView.frame = NSMakeRect(0.0, gridY, width, MAX(0.0, height - gridY - 36.0));
        self.statusLabel.frame = NSMakeRect(28.0, gridY + NSHeight(self.scrollView.frame) + 10.0, width - 56.0, 24.0);
        self.loadingView.frame = self.bounds;
        self.detailsOverlayView.frame = self.bounds;
        return;
    }
    if (self.streamPipContainerView.superview != self) [self addSubview:self.streamPipContainerView];
    if (self.lastPlayedPanelView.superview != self) [self addSubview:self.lastPlayedPanelView];
    self.lastPlayedPanelView.hidden = YES;
    [self setLastPlayedFocused:NO];
    self.categoryBarView.hidden = controllerMode || self.categoryButtons.count <= 1;
    CGFloat cardHeight = [OPNGameCardView cardSize].height;
    CGFloat minimumDetailHeight = 220.0;
    CGFloat desiredCarouselHeight = cardHeight + 96.0;
    CGFloat categoryY = controllerNavHeight + 10.0;
    CGFloat railY = controllerNavHeight + 10.0;
    CGFloat bottomInset = 36.0;
    CGFloat selectorOverlap = controllerMode ? kControllerRailSelectorOverlap : 0.0;
    CGFloat detailOverlap = controllerMode ? kControllerRailDetailOverlap : 0.0;
    CGFloat detailGap = -detailOverlap;
    CGFloat carouselHeight = desiredCarouselHeight;
    CGFloat detailY = railY + carouselHeight + detailGap;
    CGFloat detailHeight = 0.0;
    CGFloat gridY = kNavHeight + (compact ? 116.0 : kToolbarHeight);
    if (controllerMode) {
        CGFloat availableContentHeight = MAX(0.0, height - railY - bottomInset);
        carouselHeight = MIN(desiredCarouselHeight, MAX(cardHeight + 62.0, availableContentHeight * 0.32));
        detailY = railY + carouselHeight + detailGap;
        detailHeight = MAX(minimumDetailHeight, height - detailY);
        gridY = railY;
    }
    if (controllerMode && !self.categoryBarView.hidden) {
        self.categoryBarView.frame = NSMakeRect(64.0, categoryY, MAX(240.0, width - 128.0), 30.0);
        CGFloat categoryX = 0.0;
        for (NSButton *button in self.categoryButtons) {
            BOOL selected = [button.identifier isEqualToString:self.selectedCategoryId];
            CGFloat buttonWidth = NSWidth(button.frame);
            button.frame = NSMakeRect(categoryX, 0.0, buttonWidth, 30.0);
            button.contentTintColor = selected ? OpnColor(OPNControllerAccentBlackRGB(0.88)) : OpnColor(kTextSecondary);
            button.font = [NSFont systemFontOfSize:12.0 weight:NSFontWeightSemibold];
            button.layer.cornerRadius = 15.0;
            button.layer.backgroundColor = selected ? OpnColor(OPNControllerAccentSoftRGB(), 0.88).CGColor : OpnColor(OPNControllerAccentRGB(), 0.080).CGColor;
            button.layer.borderWidth = selected ? 0.0 : 1.0;
            button.layer.borderColor = OpnColor(0xFFFFFF, 0.12).CGColor;
            categoryX += buttonWidth + 8.0;
        }
    }
    self.controllerDetailView.hidden = !controllerMode || self.cardViews.count == 0;
    self.controllerDetailBackgroundView.hidden = self.controllerDetailView.hidden;
    self.controllerDetailBackgroundView.frame = self.bounds;
    self.controllerDetailView.frame = NSMakeRect(0.0, detailY, width, detailHeight);
    self.controllerDetailView.layer.shadowPath = [NSBezierPath bezierPathWithRoundedRect:self.controllerDetailView.bounds xRadius:30.0 yRadius:30.0].CGPath;
    CGFloat detailWidth = NSWidth(self.controllerDetailView.frame);
    self.controllerDetailGradientLayer.frame = self.controllerDetailView.bounds;
    self.controllerDetailAccentLayer.frame = NSMakeRect(64.0, 18.0, 74.0, 3.0);
    CGFloat heroX = 64.0;
    BOOL showStreamPip = controllerMode && self.streamPipContentView != nil;
    CGFloat availableGameHubHeight = MAX(0.0, detailHeight - kControllerGameHubVerticalReserve);
    BOOL showGameHub = controllerMode && !showStreamPip && self.cardViews.count > 0 && detailWidth >= 1040.0 && availableGameHubHeight >= kControllerGameHubMinimumHeight;
    CGFloat gameHubWidth = showGameHub ? MIN(460.0, MAX(340.0, detailWidth * 0.27)) : 0.0;
    CGFloat gameHubHeight = showGameHub ? MIN(390.0, availableGameHubHeight) : 0.0;
    CGFloat gameHubX = detailWidth - gameHubWidth - 64.0;
    CGFloat gameHubY = showGameHub ? MAX(32.0, floor((detailHeight - gameHubHeight) * 0.42)) : 0.0;
    CGFloat rightContextInset = showGameHub ? gameHubWidth + 104.0 : 0.0;
    CGFloat heroWidth = MAX(260.0, detailWidth - 128.0 - rightContextInset);
    self.controllerDetailStatsLabel.hidden = YES;
    self.controllerDetailStatsLabel.frame = NSZeroRect;
    CGFloat featuresY = 38.0;
    self.controllerDetailFeaturesLabel.hidden = NO;
    self.controllerDetailFeaturesLabel.frame = NSMakeRect(heroX + 2.0, featuresY, MIN(980.0, heroWidth), MAX(0.0, detailHeight - featuresY - 88.0));
    self.controllerPromptBarView.frame = NSMakeRect(heroX + 2.0, MAX(188.0, detailHeight - 52.0), heroWidth, 36.0);
    self.controllerGameHubView.hidden = !showGameHub;
    if (showGameHub) {
        self.controllerGameHubView.frame = NSMakeRect(gameHubX, gameHubY, gameHubWidth, gameHubHeight);
    } else {
        self.controllerGameHubView.frame = NSZeroRect;
    }
    self.streamPipContainerView.hidden = !showStreamPip;
    if (showStreamPip) {
        CGFloat pipWidth = MIN(420.0, MAX(300.0, width * 0.24));
        CGFloat pipVideoHeight = floor(pipWidth * 9.0 / 16.0);
        CGFloat pipHeight = pipVideoHeight + 54.0;
        CGFloat pipX = MAX(heroX + 520.0, width - pipWidth - 64.0);
        CGFloat pipY = MIN(detailY + detailHeight - pipHeight - 48.0, detailY + MAX(34.0, detailHeight * 0.24));
        pipY = MAX(detailY + 28.0, pipY);
        self.streamPipContainerView.frame = NSMakeRect(pipX, pipY, pipWidth, pipHeight);
        self.streamPipHostView.frame = NSMakeRect(12.0, 12.0, pipWidth - 24.0, pipVideoHeight);
        self.streamPipContentView.frame = self.streamPipHostView.bounds;
        self.streamPipTitleLabel.frame = NSMakeRect(16.0, pipVideoHeight + 22.0, pipWidth * 0.50, 22.0);
        self.streamPipHintLabel.frame = NSMakeRect(pipWidth * 0.50 - 12.0, pipVideoHeight + 24.0, pipWidth * 0.50, 18.0);
    }
    CGFloat railFrameY = controllerMode ? MAX(0.0, gridY - selectorOverlap) : gridY;
    CGFloat railHeight = controllerMode ? MIN(carouselHeight + selectorOverlap, MAX(0.0, height - railFrameY)) : MAX(0.0, height - gridY);
    self.scrollView.frame = NSMakeRect(0, railFrameY, width, railHeight);
    self.statusLabel.frame = controllerMode ? NSMakeRect(28.0, MAX(kNavHeight + 30.0, gridY - 42.0), width - 56.0, 24.0) : NSMakeRect(0, gridY + 100, width, 24);
    if (controllerMode && self.cardViews.count > 0) {
        self.statusLabel.stringValue = @"";
        self.statusLabel.textColor = OpnColor(kTextSecondary);
        self.statusLabel.alignment = NSTextAlignmentCenter;
    }
    self.loadingView.frame = self.bounds;
    self.detailsOverlayView.frame = self.bounds;
}

- (void)searchChanged {
    [self requestCatalogBrowse];
}

- (void)controlTextDidChange:(NSNotification *)notification {
    if (notification.object == _searchField) {
        [self requestCatalogBrowse];
    }
}

- (void)requestCatalogBrowse {
    if (!self.onCatalogBrowseRequested) {
        [self renderGrid];
        [self scrollLibraryToTop];
        return;
    }
    std::vector<std::string> filters;
    NSArray<NSString *> *sortedFilters = [[self.selectedFilterIds allObjects] sortedArrayUsingSelector:@selector(localizedCaseInsensitiveCompare:)];
    for (NSString *filterId in sortedFilters) {
        filters.push_back([filterId UTF8String]);
    }
    self.onCatalogBrowseRequested(self.searchField.stringValue ?: @"", self.selectedSortId ?: @"last_played", filters);
}

- (void)focusCardAtIndex:(NSInteger)index scrollIntoView:(BOOL)scrollIntoView {
    if (self.cardViews.count == 0) {
        self.focusedCardIndex = -1;
        return;
    }
    [self setStreamPipFocused:NO];
    NSInteger previousIndex = self.focusedCardIndex;
    NSInteger clamped = MAX(0, MIN(index, (NSInteger)self.cardViews.count - 1));
    self.focusedCardIndex = clamped;
    for (NSUInteger i = 0; i < self.cardViews.count; i++) {
        BOOL selected = OpnControllerModeEnabled() && (NSInteger)i == clamped;
        self.cardViews[i].controllerFocused = selected;
        self.cardViews[i].alphaValue = 1.0;
    }
    if (OpnControllerModeEnabled() && scrollIntoView && previousIndex >= 0 && previousIndex != clamped) {
        OpnPlayConsoleTone(OPNConsoleToneMove);
    }
    [self updateControllerDetailContent];
    if (!scrollIntoView) return;
    OPNGameCardView *card = self.cardViews[(NSUInteger)clamped];
    NSRect visibleRect = self.scrollView.contentView.bounds;
    NSRect targetRect = NSInsetRect(card.frame, -24.0, -24.0);
    if (OpnControllerModeEnabled()) {
        NSSize contentSize = self.gridContentView.frame.size;
        CGFloat targetX = NSMidX(card.frame) - NSWidth(visibleRect) * 0.5;
        targetX = MAX(0.0, MIN(targetX, MAX(0.0, contentSize.width - NSWidth(visibleRect))));
        [[OPNCoreAnimationCoordinator sharedCoordinator] springScrollClipView:self.scrollView.contentView
                                                                          toX:targetX
                                                                     velocity:0.0];
        return;
    }
    if (!NSContainsRect(visibleRect, targetRect)) {
        [self.gridContentView scrollRectToVisible:targetRect];
        [self.scrollView reflectScrolledClipView:self.scrollView.contentView];
    }
}

- (void)focusCategoryAtIndex:(NSInteger)index scrollIntoView:(BOOL)scrollIntoView {
    NSInteger itemCount = [self controllerOverviewItemCount];
    if (itemCount == 0) {
        self.focusedCategoryIndex = -1;
        [self setStreamPipFocused:NO];
        [self setLastPlayedFocused:NO];
        return;
    }
    NSInteger previousIndex = self.focusedCategoryIndex;
    NSInteger clamped = MAX(0, MIN(index, itemCount - 1));
    self.focusedCategoryIndex = clamped;
    NSInteger categoryOffset = [self controllerOverviewCategoryOffset];
    BOOL specialFocused = categoryOffset > 0 && clamped == 0;
    [self setStreamPipFocused:specialFocused && self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileStream];
    [self setLastPlayedFocused:specialFocused && self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileLastPlayed];
    for (NSUInteger i = 0; i < self.categoryCardViews.count; i++) {
        self.categoryCardViews[i].controllerFocused = (NSInteger)i == clamped - categoryOffset;
    }
    if (OpnControllerModeEnabled() && scrollIntoView && previousIndex >= 0 && previousIndex != clamped) {
        OpnPlayConsoleTone(OPNConsoleToneMove);
    }
    if (!scrollIntoView) return;
    NSView *targetView = nil;
    if (specialFocused) {
        targetView = self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileStream ? self.streamPipContainerView : self.lastPlayedPanelView;
    } else {
        NSInteger categoryIndex = clamped - categoryOffset;
        if (categoryIndex >= 0 && categoryIndex < (NSInteger)self.categoryCardViews.count) {
            targetView = self.categoryCardViews[(NSUInteger)categoryIndex];
        }
    }
    if (targetView) {
        [self.gridContentView scrollRectToVisible:NSInsetRect(targetView.frame, -18.0, -18.0)];
        [self.scrollView reflectScrolledClipView:self.scrollView.contentView];
    }
}

- (NSInteger)controllerOverviewCategoryOffset {
    return self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileNone ? 0 : 1;
}

- (NSInteger)controllerOverviewItemCount {
    return [self controllerOverviewCategoryOffset] + (NSInteger)self.categoryCardViews.count;
}

- (NSView *)controllerOverviewViewAtIndex:(NSInteger)index {
    NSInteger categoryOffset = [self controllerOverviewCategoryOffset];
    if (categoryOffset > 0 && index == 0) {
        return self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileStream ? self.streamPipContainerView : self.lastPlayedPanelView;
    }
    NSInteger categoryIndex = index - categoryOffset;
    if (categoryIndex < 0 || categoryIndex >= (NSInteger)self.categoryCardViews.count) return nil;
    return self.categoryCardViews[(NSUInteger)categoryIndex];
}

- (void)updateLastPlayedPanel {
    if (self.streamPipContentView != nil) {
        self.lastPlayedPanelView.hidden = YES;
        [self setLastPlayedFocused:NO];
        return;
    }

    const OPN::GameInfo *lastPlayedGame = [self currentLastPlayedGame];
    self.lastPlayedPanelView.hidden = lastPlayedGame == nullptr;
    if (!lastPlayedGame) {
        [self setLastPlayedFocused:NO];
        self.lastPlayedTitleLabel.stringValue = @"";
        self.lastPlayedMetaLabel.stringValue = @"";
        self.lastPlayedImageView.image = nil;
        self.lastPlayedImageURL = @"";
        return;
    }

    const OPN::GameInfo &game = *lastPlayedGame;
    self.lastPlayedTitleLabel.stringValue = OPNCatalogString(game.title, @"Untitled Game");
    NSString *store = @"";
    int variantIndex = [self preferredVariantIndexForGame:game];
    if (variantIndex >= 0 && variantIndex < (int)game.variants.size()) {
        store = OPNCatalogString(game.variants[(size_t)variantIndex].appStore, @"");
    } else if (!game.availableStores.empty()) {
        store = OPNCatalogString(game.availableStores.front(), @"");
    }
    NSString *genres = OPNCatalogJoinedStrings(game.genres, @"");
    NSString *features = OPNCatalogJoinedStrings(game.featureLabels, @"");
    NSString *playability = OPNCatalogDisplayString(game.playabilityState, @"");
    NSMutableArray<NSString *> *meta = [NSMutableArray array];
    if (store.length > 0) [meta addObject:OPNStoreCategoryTitle(store)];
    if (genres.length > 0) [meta addObject:genres];
    if (features.length > 0 && meta.count < 3) [meta addObject:features];
    if (playability.length > 0 && meta.count < 3) [meta addObject:playability];
    self.lastPlayedMetaLabel.stringValue = meta.count > 0 ? [meta componentsJoinedByString:@"  /  "] : @"Ready to launch";

    NSArray<NSString *> *candidates = OPNControllerCategoryArtworkCandidates(game);
    NSString *expectedURL = candidates.count > 0 ? candidates.firstObject : @"";
    if ([self.lastPlayedImageURL isEqualToString:expectedURL]) return;
    self.lastPlayedImageURL = expectedURL;
    self.lastPlayedImageAspectRatio = 16.0 / 9.0;
    self.lastPlayedImageView.image = nil;
    [self loadLastPlayedImageFromCandidates:candidates index:0 expectedURL:expectedURL];
}

- (void)loadLastPlayedImageFromCandidates:(NSArray<NSString *> *)candidates index:(NSUInteger)index expectedURL:(NSString *)expectedURL {
    if (index >= candidates.count || expectedURL.length == 0) return;
    NSURL *url = [NSURL URLWithString:candidates[index]];
    if (!url) {
        [self loadLastPlayedImageFromCandidates:candidates index:index + 1 expectedURL:expectedURL];
        return;
    }
    __weak __typeof__(self) weakSelf = self;
    [[[NSURLSession sharedSession] dataTaskWithURL:url completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        NSHTTPURLResponse *http = [response isKindOfClass:NSHTTPURLResponse.class] ? (NSHTTPURLResponse *)response : nil;
        if (error || !data || (http && http.statusCode >= 400)) {
            dispatch_async(dispatch_get_main_queue(), ^{
                __typeof__(self) strongSelf = weakSelf;
                if (strongSelf && [strongSelf.lastPlayedImageURL isEqualToString:expectedURL]) {
                    [strongSelf loadLastPlayedImageFromCandidates:candidates index:index + 1 expectedURL:expectedURL];
                }
            });
            return;
        }
        NSImage *image = [[NSImage alloc] initWithData:data];
        if (!image) return;
        dispatch_async(dispatch_get_main_queue(), ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || ![strongSelf.lastPlayedImageURL isEqualToString:expectedURL]) return;
            if (image.size.width > 0.0 && image.size.height > 0.0) {
                strongSelf.lastPlayedImageAspectRatio = image.size.width / image.size.height;
            }
            strongSelf.lastPlayedImageView.image = image;
            [strongSelf layoutCatalogSubviews];
        });
    }] resume];
}

- (void)moveCategoryFocusByRows:(NSInteger)rows columns:(NSInteger)columns {
    NSInteger itemCount = [self controllerOverviewItemCount];
    if (itemCount == 0 || (rows == 0 && columns == 0)) return;
    NSView *currentView = [self controllerOverviewViewAtIndex:self.focusedCategoryIndex];
    if (!currentView) return;

    NSRect currentFrame = currentView.frame;
    CGFloat currentX = NSMidX(currentFrame);
    CGFloat currentY = NSMidY(currentFrame);
    CGFloat bestScore = CGFLOAT_MAX;
    NSInteger bestIndex = NSNotFound;

    for (NSInteger index = 0; index < itemCount; index++) {
        if (index == self.focusedCategoryIndex) continue;
        NSView *candidateView = [self controllerOverviewViewAtIndex:index];
        if (!candidateView || candidateView.hidden) continue;
        NSRect candidateFrame = candidateView.frame;
        CGFloat dx = NSMidX(candidateFrame) - currentX;
        CGFloat dy = NSMidY(candidateFrame) - currentY;
        CGFloat score = CGFLOAT_MAX;
        if (rows > 0 && dy > 1.0) {
            score = fabs(dx) * 3.0 + dy;
        } else if (rows < 0 && dy < -1.0) {
            score = fabs(dx) * 3.0 + fabs(dy);
        } else if (columns > 0 && dx > 1.0) {
            score = fabs(dy) * 3.0 + dx;
        } else if (columns < 0 && dx < -1.0) {
            score = fabs(dy) * 3.0 + fabs(dx);
        }
        if (score < bestScore) {
            bestScore = score;
            bestIndex = index;
        }
    }

    if (bestIndex != NSNotFound) [self focusCategoryAtIndex:bestIndex scrollIntoView:YES];
}

- (void)openFocusedCategory {
    NSInteger categoryOffset = [self controllerOverviewCategoryOffset];
    if (categoryOffset > 0 && self.focusedCategoryIndex == 0) {
        if (self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileStream && self.onStreamPictureInPictureSelected) {
            OpnPlayConsoleTone(OPNConsoleToneSelect);
            self.onStreamPictureInPictureSelected();
        } else if (self.controllerOverviewSpecialTileKind == OPNControllerOverviewSpecialTileLastPlayed) {
            [self launchLastPlayedGame];
        }
        return;
    }
    NSInteger categoryIndex = self.focusedCategoryIndex - categoryOffset;
    if (categoryIndex < 0 || categoryIndex >= (NSInteger)self.categoryCardViews.count) return;
    OPNControllerCategoryCardView *card = self.categoryCardViews[(NSUInteger)categoryIndex];
    if ([card.categoryId isEqualToString:@"system:interface-settings"]) {
        if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneSelect);
        if (self.onInterfaceSettingsRequested) self.onInterfaceSettingsRequested();
        return;
    }
    self.selectedCategoryId = card.categoryId.length > 0 ? card.categoryId : @"all";
    self.controllerCategoryOverviewVisible = NO;
    self.focusedCardIndex = 0;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneSelect);
    [self renderGrid];
    [self scrollLibraryToTop];
}

- (void)returnToControllerCategoryOverview {
    if (self.controllerCategoryOverviewVisible) return;
    self.controllerCategoryOverviewVisible = YES;
    self.focusedCardIndex = -1;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneBack);
    [self renderGrid];
    [self scrollLibraryToTop];
}

- (void)setStreamPipFocused:(BOOL)focused {
    if (focused && self.streamPipContentView == nil) focused = NO;
    if (focused) [self setLastPlayedFocused:NO];
    _streamPipFocused = focused;
    [CATransaction begin];
    [CATransaction setAnimationDuration:0.20];
    [CATransaction setAnimationTimingFunction:[OPNCoreAnimationCoordinator appleQuinticTimingFunction]];
    self.streamPipContainerView.layer.borderWidth = focused ? 3.0 : 1.0;
    self.streamPipContainerView.layer.borderColor = (focused ? OpnColor(0xFFFFFF, 0.92) : OpnColor(0xFFFFFF, 0.16)).CGColor;
    self.streamPipContainerView.layer.shadowColor = (focused ? OpnColor(OPNControllerAccentSoftRGB()) : NSColor.blackColor).CGColor;
    self.streamPipContainerView.layer.shadowOpacity = focused ? 0.48 : 0.30;
    self.streamPipContainerView.layer.shadowRadius = focused ? 38.0 : 24.0;
    CATransform3D transform = CATransform3DIdentity;
    if (focused) transform = CATransform3DScale(transform, 1.035, 1.035, 1.0);
    self.streamPipContainerView.layer.transform = transform;
    self.streamPipHintLabel.textColor = focused ? OpnColor(OPNControllerAccentSoftRGB()) : OpnColor(kTextSecondary);
    [CATransaction commit];
}

- (void)setLastPlayedFocused:(BOOL)focused {
    if (focused && (self.streamPipContentView != nil || self.lastPlayedPanelView.hidden)) focused = NO;
    if (focused) [self setStreamPipFocused:NO];
    _lastPlayedFocused = focused;
    [CATransaction begin];
    [CATransaction setAnimationDuration:0.20];
    [CATransaction setAnimationTimingFunction:[OPNCoreAnimationCoordinator appleQuinticTimingFunction]];
    self.lastPlayedPanelView.layer.borderWidth = focused ? 3.0 : 1.0;
    self.lastPlayedPanelView.layer.borderColor = (focused ? OpnColor(0xFFFFFF, 0.92) : OpnColor(0xFFFFFF, 0.16)).CGColor;
    self.lastPlayedPanelView.layer.shadowColor = (focused ? OpnColor(OPNControllerAccentSoftRGB()) : NSColor.blackColor).CGColor;
    self.lastPlayedPanelView.layer.shadowOpacity = focused ? 0.48 : 0.30;
    self.lastPlayedPanelView.layer.shadowRadius = focused ? 38.0 : 24.0;
    CATransform3D transform = CATransform3DIdentity;
    if (focused) transform = CATransform3DScale(transform, 1.035, 1.035, 1.0);
    self.lastPlayedPanelView.layer.transform = transform;
    self.lastPlayedHintLabel.textColor = focused ? OpnColor(OPNControllerAccentSoftRGB()) : OpnColor(kTextSecondary);
    [CATransaction commit];
}

- (OPNGameCardView *)focusedCard {
    if (self.focusedCardIndex < 0 || self.focusedCardIndex >= (NSInteger)self.cardViews.count) return nil;
    return self.cardViews[(NSUInteger)self.focusedCardIndex];
}

- (void)moveFocusByRows:(NSInteger)rows columns:(NSInteger)columns {
    if (OpnControllerModeEnabled() && self.controllerCategoryOverviewVisible) {
        [self moveCategoryFocusByRows:rows columns:columns];
        return;
    }
    if (OpnControllerModeEnabled() && rows != 0) {
        if (self.streamPipContentView) {
            if (self.isStreamPipFocused && rows < 0) {
                [self setStreamPipFocused:NO];
                return;
            }
            if (!self.isStreamPipFocused && rows > 0) {
                [self setStreamPipFocused:YES];
                OpnPlayConsoleTone(OPNConsoleToneMove);
                return;
            }
        }
        return;
    }
    if (OpnControllerModeEnabled() && self.isStreamPipFocused) {
        if (columns < 0 || columns > 0) {
            [self setStreamPipFocused:NO];
            OpnPlayConsoleTone(OPNConsoleToneMove);
        }
        return;
    }
    NSInteger next = self.focusedCardIndex + rows * MAX(1, self.gridColumnCount) + columns;
    [self focusCardAtIndex:next scrollIntoView:YES];
}

- (void)cycleFocusedVariant {
    OPNGameCardView *card = [self focusedCard];
    if (!card || card.game.variants.size() <= 1) return;
    int next = (card.selectedVariantIndex + 1) % (int)card.game.variants.size();
    [card selectVariantAtIndex:next];
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneChange);
    [self updateControllerDetailContent];
}

- (void)updateControllerDetailContent {
    if (!OpnControllerModeEnabled()) return;
    OPNGameCardView *card = [self focusedCard];
    NSColor *cardAccentColor = card.artworkAccentColor;
    NSColor *detailAccentColor = cardAccentColor ?: OpnColor(OPNControllerAccentRGB());
    NSColor *detailAccentSoftColor = cardAccentColor ?: OpnColor(OPNControllerAccentSoftRGB());
    self.controllerElectricBackgroundView.accentRGB = OPNRGBFromColor(cardAccentColor, OPNControllerAccentRGB());
    if (self.onFocusedArtworkAccentChanged) self.onFocusedArtworkAccentChanged(self.controllerElectricBackgroundView.accentRGB);
    CATransition *fade = [CATransition animation];
    fade.type = kCATransitionFade;
    fade.duration = 0.18;
    fade.timingFunction = [OPNCoreAnimationCoordinator appleQuinticTimingFunction];
    [self.controllerDetailView.layer addAnimation:fade forKey:@"opn.detail.fade"];
    [CATransaction begin];
    [CATransaction setAnimationDuration:0.32];
    [CATransaction setAnimationTimingFunction:[OPNCoreAnimationCoordinator appleQuinticTimingFunction]];
    self.controllerDetailGradientLayer.colors = @[(id)NSColor.clearColor.CGColor,
                                                   (id)NSColor.clearColor.CGColor,
                                                   (id)NSColor.clearColor.CGColor];
    self.controllerDetailGradientLayer.opacity = 0.0;
    self.controllerDetailAccentLayer.backgroundColor = [detailAccentSoftColor colorWithAlphaComponent:0.90].CGColor;
    self.controllerDetailView.layer.shadowColor = detailAccentColor.CGColor;
    self.controllerGameHubView.accentRGB = self.controllerElectricBackgroundView.accentRGB;
    [CATransaction commit];
    if (!card) {
        self.controllerDetailBackgroundView.image = nil;
        self.controllerDetailBackgroundURL = @"";
        self.controllerDetailBackgroundURLs = @[];
        self.controllerDetailBackgroundGameId = @"";
        [self stopControllerDetailBackgroundRotation];
        self.controllerDetailStatsLabel.stringValue = @"";
        self.controllerDetailFeaturesLabel.stringValue = @"";
        self.controllerGameHubView.hidden = YES;
        self.controllerPromptBarView.hidden = YES;
        return;
    }

    const OPN::GameInfo game = card.game;
    [self configureControllerDetailBackgroundForGame:game];

    NSString *genres = OPNCatalogJoinedStrings(game.genres, @"Cloud game");
    NSString *tier = OPNCatalogString(game.membershipTierLabel, @"");

    NSString *store = @"";
    if (card.selectedVariantIndex >= 0 && card.selectedVariantIndex < (int)game.variants.size()) {
        store = OPNCatalogString(game.variants[(size_t)card.selectedVariantIndex].appStore, @"");
    } else if (!game.availableStores.empty()) {
        store = OPNCatalogString(game.availableStores.front(), @"");
    }
    store = store.length > 0 ? OPNStoreCategoryTitle(store) : @"Default store";

    self.controllerDetailStatsLabel.stringValue = @"";
    NSString *description = OPNCatalogString(game.description, @"");
    if (description.length == 0) description = OPNCatalogJoinedStrings(game.featureLabels, @"");
    if (description.length == 0) description = @"Loading game details...";
    self.controllerDetailFeaturesLabel.attributedStringValue = OPNOutlinedControllerDescriptionText(description);
    NSString *launchStatus = game.playabilityState.empty()
        ? @"Ready"
        : OPNCatalogDisplayString(game.playabilityState, @"Ready");
    NSString *genreSummary = genres;
    if (tier.length > 0) genreSummary = [genreSummary stringByAppendingFormat:@"  /  %@", tier];
    NSString *developer = OPNCatalogString(game.developerName, @"");
    NSString *publisher = OPNCatalogString(game.publisherName, @"");
    NSString *studioInfo = @"Studio not listed";
    if (developer.length > 0 && publisher.length > 0 && ![developer isEqualToString:publisher]) {
        studioInfo = [NSString stringWithFormat:@"%@ / %@", developer, publisher];
    } else if (developer.length > 0) {
        studioInfo = developer;
    } else if (publisher.length > 0) {
        studioInfo = publisher;
    }
    NSString *playerInfo = OPNCatalogPlayerCountText(game.maxLocalPlayers, game.maxOnlinePlayers);
    if (playerInfo.length == 0) playerInfo = @"Player count not listed";
    NSString *controlInfo = OPNCatalogCommaJoinedStrings(game.supportedControls, 3);
    if (controlInfo.length == 0) controlInfo = OPNCatalogCommaJoinedStrings(game.featureLabels, 2);
    if (controlInfo.length == 0) controlInfo = @"Controls vary by store";
    NSString *storeInfo = OPNCatalogStoreSummary(game.availableStores, store);
    self.controllerGameHubView.gameTitle = OPNCatalogString(game.title, @"Untitled Game");
    self.controllerGameHubView.genreSummary = genreSummary;
    self.controllerGameHubView.launchStatus = launchStatus;
    self.controllerGameHubView.studioInfo = studioInfo;
    self.controllerGameHubView.playerInfo = playerInfo;
    self.controllerGameHubView.controlInfo = controlInfo;
    self.controllerGameHubView.storeInfo = storeInfo;
    self.controllerPromptBarView.hidden = NO;
    self.controllerPromptBarView.includeStore = game.variants.size() > 1;
    self.controllerPromptBarView.includeBack = NO;
}

- (void)configureControllerDetailBackgroundForGame:(const OPN::GameInfo &)game {
    NSArray<NSString *> *candidates = OPNControllerCategoryBackgroundCandidates(game);
    NSString *gameId = OPNCatalogString(game.id, @"");
    if (candidates.count == 0) {
        self.controllerDetailBackgroundView.image = nil;
        self.controllerDetailBackgroundURL = @"";
        self.controllerDetailBackgroundURLs = @[];
        self.controllerDetailBackgroundGameId = gameId;
        [self stopControllerDetailBackgroundRotation];
        return;
    }

    BOOL sameGame = [self.controllerDetailBackgroundGameId isEqualToString:gameId];
    BOOL sameCandidates = [self.controllerDetailBackgroundURLs isEqualToArray:candidates];
    if (!sameGame || !sameCandidates) {
        self.controllerDetailBackgroundGameId = gameId;
        self.controllerDetailBackgroundURLs = candidates;
        self.controllerDetailBackgroundIndex = arc4random_uniform((uint32_t)candidates.count);
        NSString *expectedURL = candidates[(NSUInteger)self.controllerDetailBackgroundIndex];
        self.controllerDetailBackgroundURL = expectedURL;
        [self loadControllerDetailBackgroundFromCandidates:candidates
                                                     index:(NSUInteger)self.controllerDetailBackgroundIndex
                                               expectedURL:expectedURL];
    }

    [self startControllerDetailBackgroundRotationIfNeeded];
}

- (void)startControllerDetailBackgroundRotationIfNeeded {
    if (!self.window || self.controllerDetailView.hidden || self.controllerDetailBackgroundURLs.count <= 1) {
        [self stopControllerDetailBackgroundRotation];
        return;
    }
    if (self.controllerDetailBackgroundTimer) return;
    self.controllerDetailBackgroundTimer = [NSTimer timerWithTimeInterval:3.0
                                                                    target:self
                                                                  selector:@selector(controllerDetailBackgroundTimerFired:)
                                                                  userInfo:nil
                                                                   repeats:YES];
    [[NSRunLoop mainRunLoop] addTimer:self.controllerDetailBackgroundTimer forMode:NSRunLoopCommonModes];
}

- (void)stopControllerDetailBackgroundRotation {
    [self.controllerDetailBackgroundTimer invalidate];
    self.controllerDetailBackgroundTimer = nil;
}

- (void)controllerDetailBackgroundTimerFired:(NSTimer *)timer {
    (void)timer;
    NSArray<NSString *> *candidates = self.controllerDetailBackgroundURLs;
    if (!self.window || self.controllerDetailView.hidden || candidates.count <= 1) {
        [self stopControllerDetailBackgroundRotation];
        return;
    }

    self.controllerDetailBackgroundIndex = (self.controllerDetailBackgroundIndex + 1) % (NSInteger)candidates.count;
    NSString *expectedURL = candidates[(NSUInteger)self.controllerDetailBackgroundIndex];
    self.controllerDetailBackgroundURL = expectedURL;
    [self loadControllerDetailBackgroundFromCandidates:candidates
                                                 index:(NSUInteger)self.controllerDetailBackgroundIndex
                                           expectedURL:expectedURL];
}

- (void)loadControllerDetailBackgroundFromCandidates:(NSArray<NSString *> *)candidates index:(NSUInteger)index expectedURL:(NSString *)expectedURL {
    if (index >= candidates.count || expectedURL.length == 0) return;
    NSURL *url = [NSURL URLWithString:candidates[index]];
    if (!url) {
        [self loadControllerDetailBackgroundFromCandidates:candidates index:index + 1 expectedURL:expectedURL];
        return;
    }

    __weak __typeof__(self) weakSelf = self;
    [[[NSURLSession sharedSession] dataTaskWithURL:url completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        NSHTTPURLResponse *http = [response isKindOfClass:NSHTTPURLResponse.class] ? (NSHTTPURLResponse *)response : nil;
        if (error || !data || (http && http.statusCode >= 400)) {
            dispatch_async(dispatch_get_main_queue(), ^{
                __typeof__(self) strongSelf = weakSelf;
                if (strongSelf && [strongSelf.controllerDetailBackgroundURL isEqualToString:expectedURL]) {
                    [strongSelf loadControllerDetailBackgroundFromCandidates:candidates index:index + 1 expectedURL:expectedURL];
                }
            });
            return;
        }

        NSImage *image = [[NSImage alloc] initWithData:data];
        if (!image) {
            dispatch_async(dispatch_get_main_queue(), ^{
                __typeof__(self) strongSelf = weakSelf;
                if (strongSelf && [strongSelf.controllerDetailBackgroundURL isEqualToString:expectedURL]) {
                    [strongSelf loadControllerDetailBackgroundFromCandidates:candidates index:index + 1 expectedURL:expectedURL];
                }
            });
            return;
        }

        dispatch_async(dispatch_get_main_queue(), ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf || ![strongSelf.controllerDetailBackgroundURL isEqualToString:expectedURL]) return;
            CATransition *fade = [CATransition animation];
            fade.type = kCATransitionFade;
            fade.duration = 0.42;
            fade.timingFunction = [OPNCoreAnimationCoordinator appleQuinticTimingFunction];
            [strongSelf.controllerDetailBackgroundView.layer addAnimation:fade forKey:@"opn.detail.background.fade"];
            strongSelf.controllerDetailBackgroundView.image = image;
        });
    }] resume];
}

- (void)openFocusedGameDetails {
    OPNGameCardView *card = [self focusedCard];
    if (!card) return;
    [self.detailsOverlayView removeFromSuperview];
    NSView *overlay = [[NSView alloc] initWithFrame:self.bounds];
    overlay.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    overlay.wantsLayer = YES;
    overlay.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.96), 0.62).CGColor;

    CGFloat panelWidth = MIN(760.0, MAX(420.0, NSWidth(self.bounds) - 96.0));
    CGFloat panelHeight = 390.0;
    NSView *panel = [[OPNFlippedGridDocumentView alloc] initWithFrame:NSMakeRect(floor((NSWidth(self.bounds) - panelWidth) / 2.0),
                                                                                   floor((NSHeight(self.bounds) - panelHeight) / 2.0),
                                                                                   panelWidth,
                                                                                   panelHeight)];
    panel.wantsLayer = YES;
    panel.layer.cornerRadius = 30.0;
    panel.layer.borderWidth = 1.5;
    panel.layer.borderColor = OpnColor(0xFFFFFF, 0.22).CGColor;
    panel.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.88), 0.96).CGColor;
    panel.layer.shadowColor = OpnColor(OPNControllerAccentRGB()).CGColor;
    panel.layer.shadowOpacity = 0.28;
    panel.layer.shadowRadius = 48.0;
    panel.layer.shadowOffset = CGSizeZero;
    [overlay addSubview:panel];

    NSString *title = OPNCatalogString(card.game.title, @"Game Details");
    NSTextField *titleLabel = OpnLabel(title, NSMakeRect(36.0, 34.0, panelWidth - 72.0, 42.0), 30.0, OpnColor(kTextPrimary), NSFontWeightSemibold);
    titleLabel.lineBreakMode = NSLineBreakByTruncatingTail;
    [panel addSubview:titleLabel];

    NSString *store = @"";
    if (card.selectedVariantIndex >= 0 && card.selectedVariantIndex < (int)card.game.variants.size()) {
        store = OPNCatalogString(card.game.variants[(size_t)card.selectedVariantIndex].appStore, @"");
    } else if (!card.game.availableStores.empty()) {
        store = OPNCatalogString(card.game.availableStores.front(), @"");
    }
    store = store.length > 0 ? OPNStoreCategoryTitle(store) : @"Default store";
    NSTextField *storeLabel = OpnLabel(@"",
                                       NSMakeRect(38.0, 92.0, panelWidth - 76.0, 24.0),
                                       15.0,
                                       NSColor.whiteColor,
                                       NSFontWeightSemibold);
    storeLabel.attributedStringValue = OPNOutlinedControllerStoreText([NSString stringWithFormat:@"Selected Store: %@", store]);
    [panel addSubview:storeLabel];

    NSString *body = OPNCatalogString(card.game.description, @"");
    if (body.length == 0) body = OPNCatalogJoinedStrings(card.game.featureLabels, @"");
    if (body.length == 0) body = @"Loading game details...";
    NSTextField *bodyLabel = OpnLabel(body, NSMakeRect(38.0, 136.0, panelWidth - 76.0, 112.0), 14.0, OpnColor(kTextSecondary), NSFontWeightRegular);
    bodyLabel.attributedStringValue = OPNOutlinedControllerDescriptionText(body);
    bodyLabel.maximumNumberOfLines = 6;
    [panel addSubview:bodyLabel];

    NSMutableArray<NSString *> *metadata = [NSMutableArray array];
    NSString *developer = OPNCatalogString(card.game.developerName, @"");
    NSString *publisher = OPNCatalogString(card.game.publisherName, @"");
    NSString *players = OPNCatalogPlayerCountText(card.game.maxLocalPlayers, card.game.maxOnlinePlayers);
    NSString *controls = OPNCatalogCommaJoinedStrings(card.game.supportedControls);
    NSString *ratings = OPNCatalogCommaJoinedStrings(card.game.contentRatings);
    NSString *nvidiaTech = OPNCatalogCommaJoinedStrings(card.game.nvidiaTech);
    if (developer.length > 0) [metadata addObject:[@"Developer: " stringByAppendingString:developer]];
    if (publisher.length > 0) [metadata addObject:[@"Publisher: " stringByAppendingString:publisher]];
    if (players.length > 0) [metadata addObject:[@"Players: " stringByAppendingString:players]];
    if (controls.length > 0) [metadata addObject:[@"Controls: " stringByAppendingString:controls]];
    if (ratings.length > 0) [metadata addObject:[@"Rating: " stringByAppendingString:ratings]];
    if (nvidiaTech.length > 0) [metadata addObject:[@"NVIDIA: " stringByAppendingString:nvidiaTech]];
    NSTextField *metadataLabel = OpnLabel([metadata componentsJoinedByString:@"\n"],
                                          NSMakeRect(38.0, 256.0, panelWidth - 76.0, 72.0),
                                          12.0,
                                          OpnColor(kTextMuted),
                                          NSFontWeightMedium);
    metadataLabel.maximumNumberOfLines = 4;
    [panel addSubview:metadataLabel];

    NSButton *playButton = OpnButton(@"Play", NSMakeRect(38.0, panelHeight - 96.0, 180.0, 52.0), OpnColor(OPNControllerAccentSoftRGB(), 0.96), OpnColor(OPNControllerAccentBlackRGB(0.88)));
    playButton.target = self;
    playButton.action = @selector(detailsPlayClicked:);
    playButton.layer.cornerRadius = 18.0;
    [panel addSubview:playButton];

    NSButton *closeButton = OpnButton(@"Back", NSMakeRect(232.0, panelHeight - 96.0, 132.0, 52.0), OpnColor(0xFFFFFF, 0.08), OpnColor(kTextPrimary), true, OpnColor(0xFFFFFF, 0.16));
    closeButton.target = self;
    closeButton.action = @selector(detailsCloseClicked:);
    closeButton.layer.cornerRadius = 18.0;
    [panel addSubview:closeButton];

    OPNControllerPromptBarView *hints = [[OPNControllerPromptBarView alloc] initWithFrame:NSMakeRect(38.0, panelHeight - 44.0, panelWidth - 76.0, 36.0)];
    hints.includeStore = card.game.variants.size() > 1;
    hints.includeBack = YES;
    [panel addSubview:hints];

    self.detailsOverlayView = overlay;
    [self addSubview:overlay];
    [[OPNCoreAnimationCoordinator sharedCoordinator] animateCardLayer:panel.layer
                                                    metadataContainer:self.controllerDetailView
                                                      backgroundLayer:self.controllerElectricBackgroundView.layer
                                                             expanded:YES
                                                          accentColor:card.artworkAccentColor ?: OpnColor(OPNControllerAccentRGB())];
}

- (void)closeGameDetails {
    [self.detailsOverlayView removeFromSuperview];
    self.detailsOverlayView = nil;
    [self.window makeFirstResponder:self];
}

- (void)launchFocusedGame {
    if (self.isStreamPipFocused && self.onStreamPictureInPictureSelected) {
        OpnPlayConsoleTone(OPNConsoleToneSelect);
        self.onStreamPictureInPictureSelected();
        return;
    }
    if (self.isLastPlayedFocused) {
        [self launchLastPlayedGame];
        return;
    }
    if (OpnControllerModeEnabled() && self.controllerCategoryOverviewVisible) {
        [self openFocusedCategory];
        return;
    }
    OPNGameCardView *card = [self focusedCard];
    if (!card || !self.onSelectGame) return;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneSelect);
    int variantIdx = card.selectedVariantIndex >= 0 ? card.selectedVariantIndex : 0;
    self.onSelectGame(card.game, variantIdx);
}

- (void)launchLastPlayedGame {
    const OPN::GameInfo *lastPlayedGame = [self currentLastPlayedGame];
    if (!lastPlayedGame || !self.onSelectGame) return;
    if (OpnControllerModeEnabled()) OpnPlayConsoleTone(OPNConsoleToneSelect);
    self.onSelectGame(*lastPlayedGame, [self preferredVariantIndexForGame:*lastPlayedGame]);
}

- (void)detailsPlayClicked:(id)sender {
    (void)sender;
    [self launchFocusedGame];
}

- (void)detailsCloseClicked:(id)sender {
    (void)sender;
    [self closeGameDetails];
}

- (void)keyDown:(NSEvent *)event {
    if (!OpnControllerModeEnabled()) {
        [super keyDown:event];
        return;
    }
    NSString *chars = event.charactersIgnoringModifiers.lowercaseString ?: @"";
    switch (event.keyCode) {
        case 123: [self moveFocusByRows:0 columns:-1]; return;
        case 124: [self moveFocusByRows:0 columns:1]; return;
        case 125: [self moveFocusByRows:1 columns:0]; return;
        case 126: [self moveFocusByRows:-1 columns:0]; return;
        case 36:
        case 49:
            [self launchFocusedGame];
            return;
        case 53:
            if (self.isLastPlayedFocused) [self setLastPlayedFocused:NO];
            if (!self.controllerCategoryOverviewVisible) [self returnToControllerCategoryOverview];
            return;
        default:
            break;
    }
    if ([chars isEqualToString:@"y"] || [chars isEqualToString:@"f"]) {
        [self toggleFavoriteForFocusedGame];
        return;
    }
    if ([chars isEqualToString:@"x"] || [chars isEqualToString:@"s"] || [chars isEqualToString:@"v"]) {
        [self cycleFocusedVariant];
        return;
    }
    if ([chars isEqualToString:@"b"]) {
        if (self.isLastPlayedFocused) [self setLastPlayedFocused:NO];
        if (!self.controllerCategoryOverviewVisible) [self returnToControllerCategoryOverview];
        return;
    }
    [super keyDown:event];
}

- (void)startGamepadNavigationIfNeeded {
    if (!OpnControllerModeEnabled() || self.gamepadNavigationTimer || !OPNCatalogGamepadNavigationActive(self)) return;
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

- (void)controllerDidConnect:(NSNotification *)notification {
    (void)notification;
    [self startGamepadNavigationIfNeeded];
}

- (void)controllerDidDisconnect:(NSNotification *)notification {
    (void)notification;
    self.previousGamepadButtons = 0;
}

- (void)pollGamepadNavigation {
    if (!OpnControllerModeEnabled() || !OPNCatalogGamepadNavigationActive(self)) {
        [self stopGamepadNavigation];
        return;
    }
    if (self.window.firstResponder != self.searchField) [self.window makeFirstResponder:self];
    uint16_t buttons = OPNCatalogGamepadButtons();
    uint16_t pressed = buttons & (uint16_t)~self.previousGamepadButtons;
    CFTimeInterval now = CACurrentMediaTime();
    BOOL repeatMove = (now - self.lastGamepadMoveTime) > 0.22;
    uint16_t moves = buttons & ((1u << 5) | (1u << 6) | (1u << 7) | (1u << 8));
    if (moves && repeatMove) {
        pressed |= moves;
        self.lastGamepadMoveTime = now;
    }
    if (pressed & (1u << 0)) {
        [self launchFocusedGame];
    }
    if (pressed & (1u << 1)) {
        if (self.isStreamPipFocused) {
            [self setStreamPipFocused:NO];
        } else if (self.isLastPlayedFocused) {
            [self setLastPlayedFocused:NO];
        } else if (!self.controllerCategoryOverviewVisible) {
            [self returnToControllerCategoryOverview];
        }
    }
    if (pressed & (1u << 2)) [self toggleFavoriteForFocusedGame];
    if (pressed & (1u << 5)) [self moveFocusByRows:-1 columns:0];
    if (pressed & (1u << 6)) [self moveFocusByRows:1 columns:0];
    if (pressed & (1u << 7)) [self moveFocusByRows:0 columns:-1];
    if (pressed & (1u << 8)) [self moveFocusByRows:0 columns:1];
    if (pressed & (1u << 9)) [self cycleFocusedVariant];
    self.previousGamepadButtons = buttons;
}

- (void)signOutClicked {
    if (self.onSignOut) self.onSignOut();
}

@end
