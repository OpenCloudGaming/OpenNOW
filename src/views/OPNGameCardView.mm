#import "OPNGameCardView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNCoreAnimationCoordinator.h"
#import "../common/OPNUIHelpers.h"
#include <QuartzCore/QuartzCore.h>

static const CGFloat gCardWidth = 220.0;
static const CGFloat gControllerCardWidth = 164.0;
static const CGFloat gImageHeight = gCardWidth * 9.0 / 16.0;
static const CGFloat gInfoHeight = 0.0;
static unsigned OPNControllerAccentRGB(void) {
    return OpnCurrentAccentRGB();
}

static unsigned OPNControllerAccentSoftRGB(void) {
    return OpnBlendRGB(OpnCurrentAccentRGB(), 0xFFFFFF, 0.42);
}

static unsigned OPNControllerAccentBlackRGB(CGFloat blackMix) {
    return OpnBlendRGB(OpnCurrentAccentRGB(), 0x000000, blackMix);
}
static CGFloat OPNScaledCardWidth(void) {
    if (OpnControllerModeEnabled()) return gControllerCardWidth;
    return floor(gCardWidth * OpnPosterSizeScale());
}

static CGFloat OPNScaledCardHeight(void) {
    if (OpnControllerModeEnabled()) return floor(gControllerCardWidth * 9.0 / 16.0);
    return floor(gImageHeight * OpnPosterSizeScale());
}

static NSString *OPNStorePrettyName(NSString *name) {
    NSString *upper = name.uppercaseString;
    if ([upper containsString:@"STEAM"]) return @"Steam";
    if ([upper containsString:@"EPIC"] || [upper containsString:@"EGS"]) return @"Epic";
    if ([upper containsString:@"UBISOFT"] || [upper containsString:@"UPLAY"]) return @"Ubisoft";
    if ([upper containsString:@"BATTLE"]) return @"Battle.net";
    if ([upper containsString:@"XBOX"] || [upper containsString:@"MICROSOFT"]) return @"Xbox";
    if ([upper containsString:@"EA"]) return @"EA";
    if ([upper containsString:@"ORIGIN"]) return @"EA";
    if ([upper containsString:@"GOG"]) return @"GOG";
    return name.capitalizedString;
}

static NSString *OPNStoreIconAssetName(NSString *name) {
    NSString *upper = name.uppercaseString;
    if ([upper containsString:@"STEAM"]) return @"steam";
    if ([upper containsString:@"EPIC"] || [upper containsString:@"EGS"]) return @"epic";
    if ([upper containsString:@"UBISOFT"] || [upper containsString:@"UPLAY"]) return @"ubisoft";
    if ([upper containsString:@"BATTLE"]) return @"battlenet";
    if ([upper containsString:@"XBOX"] || [upper containsString:@"MICROSOFT"]) return @"xbox";
    if ([upper containsString:@"EA"] || [upper containsString:@"ORIGIN"]) return @"ea";
    if ([upper containsString:@"GOG"]) return @"gog";
    return @"default";
}

static NSString *OPNStoreIconAssetPath(NSString *assetName) {
    NSString *bundlePath = [[NSBundle mainBundle] pathForResource:assetName ofType:@"svg" inDirectory:@"store-icons"];
    if (bundlePath.length > 0) return bundlePath;

    NSString *relativePath = [NSString stringWithFormat:@"assets/store-icons/%@.svg", assetName];
    NSString *workingPath = [[[NSFileManager defaultManager] currentDirectoryPath] stringByAppendingPathComponent:relativePath];
    if ([[NSFileManager defaultManager] fileExistsAtPath:workingPath]) return workingPath;

    NSString *sourcePath = [@"/Volumes/Projects/OpenNOW-Mac" stringByAppendingPathComponent:relativePath];
    if ([[NSFileManager defaultManager] fileExistsAtPath:sourcePath]) return sourcePath;

    return nil;
}

static NSImage *OPNStoreIconImage(NSString *name) {
    static NSMutableDictionary<NSString *, NSImage *> *cache;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        cache = [NSMutableDictionary dictionary];
    });

    NSString *assetName = OPNStoreIconAssetName(name ?: @"");
    NSImage *cached = cache[assetName];
    if (cached) return cached;

    NSString *path = OPNStoreIconAssetPath(assetName);
    NSImage *image = path.length > 0 ? [[NSImage alloc] initWithContentsOfFile:path] : nil;
    if (!image && ![assetName isEqualToString:@"default"]) {
        path = OPNStoreIconAssetPath(@"default");
        image = path.length > 0 ? [[NSImage alloc] initWithContentsOfFile:path] : nil;
    }
    if (!image) return nil;

    [image setTemplate:YES];
    cache[assetName] = image;
    return image;
}

static NSString *OPNStoreIconGlyph(NSString *name) {
    NSString *upper = name.uppercaseString;
    if ([upper containsString:@"STEAM"]) return @"●";
    if ([upper containsString:@"UBISOFT"] || [upper containsString:@"UPLAY"]) return @"◎";
    if ([upper containsString:@"BATTLE"]) return @"✦";
    if ([upper containsString:@"XBOX"] || [upper containsString:@"MICROSOFT"]) return @"X";
    if ([upper containsString:@"EPIC"] || [upper containsString:@"EGS"]) return @"E";
    if ([upper containsString:@"EA"] || [upper containsString:@"ORIGIN"]) return @"EA";
    if ([upper containsString:@"GOG"]) return @"G";
    return name.length > 0 ? [name substringToIndex:1].uppercaseString : @"?";
}

static NSColor *OPNStoreIconColor(NSString *name, BOOL selected) {
    (void)name;
    CGFloat alpha = selected ? 0.96 : 0.68;
    return OpnColor(OPNControllerAccentSoftRGB(), alpha);
}

static NSFont *OPNStoreIconFont(NSString *glyph) {
    return glyph.length > 1
        ? [NSFont systemFontOfSize:8.0 weight:NSFontWeightBlack]
        : [NSFont systemFontOfSize:13.0 weight:NSFontWeightBlack];
}

static BOOL OPNIsNumericString(const std::string &value) {
    return !value.empty() && value.find_first_not_of("0123456789") == std::string::npos;
}

static NSString *OPNSteamArtworkURLForGame(const OPN::GameInfo &game) {
    std::string appId;
    for (const auto &variant : game.variants) {
        NSString *store = [NSString stringWithUTF8String:variant.appStore.c_str()];
        if ([store.uppercaseString containsString:@"STEAM"] && OPNIsNumericString(variant.id)) {
            appId = variant.id;
            break;
        }
    }
    if (appId.empty() && OPNIsNumericString(game.launchAppId)) appId = game.launchAppId;
    if (appId.empty()) return nil;
    return [NSString stringWithFormat:@"https://cdn.cloudflare.steamstatic.com/steam/apps/%s/header.jpg", appId.c_str()];
}

@interface OPNGameCardView () <CALayerDelegate>
@property (nonatomic, assign) OPN::GameInfo gameData;
@property (nonatomic, strong) NSView *contentView;
@property (nonatomic, strong) NSImageView *imageView;
@property (nonatomic, strong) NSView *storeChipsContainer;
@property (nonatomic, strong) NSTrackingArea *trackingArea;
@property (nonatomic, strong) NSButton *playButton;
@property (nonatomic, strong) CALayer *reflectionLayer;
@property (nonatomic, strong) NSMutableArray<NSButton *> *storeChipButtons;
@property (nonatomic, strong, readwrite) NSColor *artworkAccentColor;
- (void)loadImageFromCandidates:(NSArray<NSString *> *)urlStrings index:(NSUInteger)index;
- (void)applyFocusStyle;
@end

@implementation OPNGameCardView

using namespace OPN;

+ (NSSize)cardSize { return NSMakeSize(OPNScaledCardWidth(), OPNScaledCardHeight()); }
+ (CGFloat)imageHeight { return OPNScaledCardHeight(); }
+ (CGFloat)infoHeight { return gInfoHeight; }

- (OPN::GameInfo)game { return _gameData; }

- (instancetype)initWithFrame:(NSRect)frame game:(const OPN::GameInfo &)game {
    self = [super initWithFrame:frame];
    if (self) {
        _gameData = game;
        self.wantsLayer = YES;
        self.layer.cornerRadius = 20.0;
        self.layer.masksToBounds = NO;
        self.layer.backgroundColor = NSColor.clearColor.CGColor;
        self.layer.borderWidth = 1.0;
        self.layer.borderColor = OpnColor(0xFFFFFF, 0.13).CGColor;
        self.layer.shadowColor = NSColor.blackColor.CGColor;
        self.layer.shadowOpacity = 0.38;
        self.layer.shadowRadius = 20.0;
        self.layer.shadowOffset = CGSizeMake(0.0, 16.0);

        _reflectionLayer = [CALayer layer];
        _reflectionLayer.backgroundColor = OpnColor(OPNControllerAccentSoftRGB(), 0.28).CGColor;
        _reflectionLayer.cornerRadius = 18.0;
        _reflectionLayer.opacity = 0.0;
        _reflectionLayer.shadowColor = OpnColor(OPNControllerAccentSoftRGB()).CGColor;
        _reflectionLayer.shadowOpacity = 0.68;
        _reflectionLayer.shadowRadius = 24.0;
        _reflectionLayer.shadowOffset = CGSizeZero;
        [self.layer addSublayer:_reflectionLayer];

        _contentView = [[NSView alloc] initWithFrame:self.bounds];
        _contentView.wantsLayer = YES;
        _contentView.layer.cornerRadius = 20.0;
        _contentView.layer.masksToBounds = YES;
        _contentView.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.88), 0.84).CGColor;
        [self addSubview:_contentView];

        _imageView = [[NSImageView alloc] initWithFrame:self.bounds];
        _imageView.imageScaling = NSImageScaleProportionallyUpOrDown;
        _imageView.wantsLayer = YES;
        _imageView.layer.backgroundColor = OpnColor(OPNControllerAccentBlackRGB(0.90)).CGColor;
        [_contentView addSubview:_imageView];

        _playButton = [[NSButton alloc] initWithFrame:
            NSMakeRect((NSWidth(self.bounds) - 76) / 2, NSHeight(self.bounds) - 52, 76, 34)];
        _playButton.title = @"PLAY";
        _playButton.bordered = NO;
        _playButton.font = [NSFont systemFontOfSize:12 weight:NSFontWeightBold];
        _playButton.contentTintColor = OpnColor(OPNControllerAccentBlackRGB(0.88));
        _playButton.wantsLayer = YES;
        _playButton.layer.cornerRadius = 17;
        _playButton.layer.backgroundColor = OpnColor(OPNControllerAccentSoftRGB(), 0.94).CGColor;
        _playButton.layer.shadowColor = OpnColor(OPNControllerAccentSoftRGB()).CGColor;
        _playButton.layer.shadowOpacity = 0.18;
        _playButton.layer.shadowRadius = 14;
        _playButton.layer.shadowOffset = CGSizeZero;
        _playButton.hidden = YES;
        _playButton.target = self;
        _playButton.action = @selector(playClicked);
        [self addSubview:_playButton];

        _storeChipButtons = [NSMutableArray array];

        _selectedVariantIndex = -1;
        int idx = 0;
        for (auto &v : game.variants) {
            if (v.librarySelected || v.inLibrary) {
                _selectedVariantIndex = idx;
                break;
            }
            idx++;
        }
        if (_selectedVariantIndex < 0 && !game.variants.empty()) {
            _selectedVariantIndex = 0;
        }

        _storeChipsContainer = [[NSView alloc] initWithFrame:
            NSMakeRect(16, NSHeight(self.bounds) - 37, NSWidth(self.bounds) - 32, 24)];
        [_contentView addSubview:_storeChipsContainer];
        [self buildStoreChips];

        [self loadImage];

        _trackingArea = [[NSTrackingArea alloc] initWithRect:self.bounds
            options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInActiveApp
            owner:self userInfo:nil];
        [self addTrackingArea:_trackingArea];
    }
    return self;
}

- (void)setControllerFocused:(BOOL)controllerFocused {
    if (_controllerFocused == controllerFocused) return;
    _controllerFocused = controllerFocused;
    [self applyFocusStyle];
}

- (void)applyFocusStyle {
    BOOL selected = self.controllerFocused;
    NSColor *accentColor = self.artworkAccentColor ?: OpnColor(OPNControllerAccentSoftRGB());
    self.playButton.hidden = OpnControllerModeEnabled() || !selected;
    [CATransaction begin];
    [CATransaction setAnimationDuration:0.22];
    [CATransaction setAnimationTimingFunction:[OPNCoreAnimationCoordinator appleQuinticTimingFunction]];
    self.layer.zPosition = selected ? 20.0 : 0.0;
    self.layer.borderColor = selected ? OpnColor(0xFFFFFF, 0.94).CGColor : OpnColor(0xFFFFFF, 0.13).CGColor;
    self.layer.borderWidth = selected ? 3.0 : 1.0;
    self.playButton.layer.shadowOpacity = selected ? 0.58 : 0.18;
    self.playButton.layer.shadowRadius = selected ? 22.0 : 14.0;
    [CATransaction commit];

    CGFloat prominence = selected ? 1.0 : 0.0;
    [[OPNCoreAnimationCoordinator sharedCoordinator] animateFocusForCardLayer:self.layer
                                                                    glowLayer:self.reflectionLayer
                                                                      focused:selected
                                                                   prominence:prominence
                                                                   accentColor:accentColor];
}

- (BOOL)isFlipped { return YES; }

- (void)layout {
    [super layout];
    CGFloat width = NSWidth(self.bounds);
    CGFloat height = NSHeight(self.bounds);
    self.contentView.frame = self.bounds;
    self.contentView.layer.cornerRadius = 20.0;
    self.imageView.frame = self.bounds;
    self.playButton.frame = NSMakeRect((width - 76.0) / 2.0, MAX(18.0, height - 52.0), 76.0, 34.0);
    self.storeChipsContainer.frame = NSMakeRect(16.0, MAX(0.0, height - 37.0), MAX(40.0, width - 32.0), 24.0);
    self.reflectionLayer.frame = NSMakeRect(16.0, height - 10.0, MAX(24.0, width - 32.0), 18.0);
    self.layer.shadowPath = [NSBezierPath bezierPathWithRoundedRect:self.bounds xRadius:20.0 yRadius:20.0].CGPath;
}

- (void)playClicked {
    if (self.onPlay) self.onPlay();
}

- (void)updateGame:(const OPN::GameInfo &)game {
    int selectedVariant = _selectedVariantIndex;
    _gameData = game;
    if (selectedVariant >= 0 && selectedVariant < (int)_gameData.variants.size()) {
        _selectedVariantIndex = selectedVariant;
    } else {
        _selectedVariantIndex = _gameData.variants.empty() ? -1 : 0;
    }
    [self buildStoreChips];
}

- (void)buildStoreChips {
    for (NSView *v in _storeChipsContainer.subviews) { [v removeFromSuperview]; }
    [_storeChipButtons removeAllObjects];
    self.storeChipsContainer.hidden = OpnControllerModeEnabled() || _gameData.variants.size() <= 1;
    if (self.storeChipsContainer.hidden) return;
    if (_gameData.variants.empty()) return;

    if (_gameData.variants.size() <= 1) return;

    CGFloat x = 0;
    NSInteger maxChips = 4;
    NSInteger count = 0;
    int idx = 0;
    for (auto &v : _gameData.variants) {
        if (count >= maxChips) break;
        NSString *name = [NSString stringWithUTF8String:v.appStore.c_str()];
        if (!name || name.length == 0) { idx++; continue; }
        NSString *glyph = OPNStoreIconGlyph(name);
        BOOL selected = idx == _selectedVariantIndex;
        NSImage *iconImage = OPNStoreIconImage(name);

        NSButton *chip = [[NSButton alloc] initWithFrame:NSMakeRect(x, 0, 28, 24)];
        if (iconImage) {
            chip.title = @"";
            chip.image = iconImage;
            chip.imagePosition = NSImageOnly;
            chip.imageScaling = NSImageScaleProportionallyDown;
            chip.contentTintColor = OPNStoreIconColor(name, selected);
        } else {
            chip.attributedTitle = [[NSAttributedString alloc] initWithString:glyph
                                                                    attributes:@{
                NSFontAttributeName: OPNStoreIconFont(glyph),
                NSForegroundColorAttributeName: OPNStoreIconColor(name, selected),
            }];
        }
        chip.bordered = NO;
        chip.wantsLayer = YES;
        chip.layer.cornerRadius = 8;
        chip.target = self;
        chip.action = @selector(chipClicked:);
        chip.tag = idx;
        chip.toolTip = OPNStorePrettyName(name ?: @"");

        if (selected) {
            chip.layer.backgroundColor = OpnColor(OPNControllerAccentRGB(), 0.18).CGColor;
            chip.layer.borderWidth = 1.0;
            chip.layer.borderColor = OPNStoreIconColor(name, YES).CGColor;
        } else {
            chip.layer.backgroundColor = OpnColor(OPNControllerAccentRGB(), 0.08).CGColor;
            chip.layer.borderColor = OpnColor(OPNControllerAccentRGB(), 0.14).CGColor;
            chip.layer.borderWidth = 1;
        }

        [_storeChipsContainer addSubview:chip];
        [_storeChipButtons addObject:chip];
        x += 32;
        count++;
        idx++;
    }
}

- (void)chipClicked:(NSButton *)sender {
    [self selectVariantAtIndex:(int)sender.tag];
}

- (void)selectVariantAtIndex:(int)index {
    if (index < 0 || index >= (int)_gameData.variants.size()) return;
    _selectedVariantIndex = index;
    [self buildStoreChips];
}

- (void)loadImage {
    NSMutableArray<NSString *> *urlStrings = [NSMutableArray array];
    NSString *primaryUrl = self.gameData.imageUrl.empty() ? nil : [NSString stringWithUTF8String:self.gameData.imageUrl.c_str()];
    NSString *heroUrl = self.gameData.heroImageUrl.empty() ? nil : [NSString stringWithUTF8String:self.gameData.heroImageUrl.c_str()];
    NSString *steamUrl = OPNSteamArtworkURLForGame(self.gameData);
    for (NSString *candidate in @[heroUrl ?: @"", primaryUrl ?: @"", steamUrl ?: @""]) {
        if (candidate.length > 0 && ![urlStrings containsObject:candidate]) {
            [urlStrings addObject:candidate];
        }
    }
    if (urlStrings.count == 0) return;

    [self loadImageFromCandidates:urlStrings index:0];
}

- (void)loadImageFromCandidates:(NSArray<NSString *> *)urlStrings index:(NSUInteger)index {
    if (index >= urlStrings.count) return;

    NSString *urlStr = urlStrings[index];
    NSURL *url = [NSURL URLWithString:urlStr];
    if (!url) {
        [self loadImageFromCandidates:urlStrings index:index + 1];
        return;
    }

    __weak __typeof__(self) weakSelf = self;
    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithURL:url
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            NSHTTPURLResponse *http = [response isKindOfClass:[NSHTTPURLResponse class]] ? (NSHTTPURLResponse *)response : nil;
            if (error || !data || (http && http.statusCode >= 400)) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    __typeof__(self) strongSelf = weakSelf;
                    [strongSelf loadImageFromCandidates:urlStrings index:index + 1];
                });
                return;
            }
            NSImage *img = [[NSImage alloc] initWithData:data];
            if (!img) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    __typeof__(self) strongSelf = weakSelf;
                    [strongSelf loadImageFromCandidates:urlStrings index:index + 1];
                });
                return;
            }
            dispatch_async(dispatch_get_main_queue(), ^{
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf) return;
                strongSelf.imageView.image = img;
                NSRect imageRect = NSMakeRect(0.0, 0.0, img.size.width, img.size.height);
                CGImageRef cgImage = [img CGImageForProposedRect:&imageRect context:nil hints:nil];
                if (cgImage) {
                    [[OPNCoreAnimationCoordinator sharedCoordinator] extractDominantColorFromImage:cgImage
                                                                                           cacheKey:urlStr
                                                                                         completion:^(NSColor *color) {
                        __typeof__(self) completedSelf = weakSelf;
                        if (!completedSelf || !color) return;
                        completedSelf.artworkAccentColor = color;
                        if (completedSelf.controllerFocused) [completedSelf applyFocusStyle];
                        if (completedSelf.onArtworkAccentColorChanged) completedSelf.onArtworkAccentColorChanged(color);
                    }];
                }
            });
        }];
    [task resume];
}

- (void)mouseEntered:(NSEvent *)event {
    [super mouseEntered:event];
    if (OpnControllerModeEnabled()) return;
    if (!self.controllerFocused) {
        self.playButton.hidden = NO;
        self.layer.borderColor = OpnColor(0xFFFFFF, 0.28).CGColor;
    }
}

- (void)mouseExited:(NSEvent *)event {
    [super mouseExited:event];
    if (OpnControllerModeEnabled()) return;
    if (!self.controllerFocused) {
        self.playButton.hidden = YES;
        self.layer.borderColor = OpnColor(0xFFFFFF, 0.10).CGColor;
    }
}

- (void)updateTrackingAreas {
    if (self.trackingArea && [self.trackingAreas containsObject:self.trackingArea]) {
        [self removeTrackingArea:self.trackingArea];
    }
    self.trackingArea = [[NSTrackingArea alloc] initWithRect:self.bounds
        options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInActiveApp
        owner:self userInfo:nil];
    [self addTrackingArea:self.trackingArea];
}

@end
