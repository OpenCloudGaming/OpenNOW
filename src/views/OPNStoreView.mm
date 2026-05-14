#import "OPNStoreView.h"
#import "OPNLoadingView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#include <QuartzCore/QuartzCore.h>
#include <cctype>
#include <cmath>

static const CGFloat kStoreTopInset = 108.0;
static const CGFloat kStoreNavigationClearance = 64.0;
static const CGFloat kStoreHeroTopOffset = 116.0;
static const CGFloat kStoreRowHeight = 246.0;
static const CGFloat kStoreCardSpacing = 18.0;
static const CGFloat kStoreTileWidth = 256.0;
static const CGFloat kStoreTileHeight = 154.0;

@interface OPNStoreDocumentView : NSView
@end

@implementation OPNStoreDocumentView
- (BOOL)isFlipped { return YES; }
@end

@interface OPNStoreRailScrollView : NSScrollView
@end

@implementation OPNStoreRailScrollView

- (void)scrollWheel:(NSEvent *)event {
    CGFloat horizontal = std::fabs(event.scrollingDeltaX);
    CGFloat vertical = std::fabs(event.scrollingDeltaY);
    if (vertical > horizontal) {
        NSScrollView *pageScrollView = self.enclosingScrollView;
        if (pageScrollView && pageScrollView != self) {
            [pageScrollView scrollWheel:event];
            return;
        }
    }
    [super scrollWheel:event];
}

@end

static NSString *OPNStorePrimaryStoreName(const OPN::GameInfo &game) {
    std::string raw;
    if (!game.variants.empty()) raw = game.variants.front().appStore;
    if (raw.empty() && !game.availableStores.empty()) raw = game.availableStores.front();
    NSString *name = raw.empty() ? @"Cloud" : [NSString stringWithUTF8String:raw.c_str()];
    NSString *upper = name.uppercaseString;
    if ([upper containsString:@"STEAM"]) return @"Steam";
    if ([upper containsString:@"BATTLE"]) return @"Battle.net";
    if ([upper containsString:@"UBISOFT"] || [upper containsString:@"UPLAY"]) return @"Ubisoft";
    if ([upper containsString:@"XBOX"]) return @"Xbox";
    if ([upper containsString:@"EPIC"]) return @"Epic";
    if ([upper containsString:@"EA"]) return @"EA";
    return name.capitalizedString;
}

static bool OPNStoreStringEqualsCaseInsensitive(const std::string &lhs, const std::string &rhs) {
    if (lhs.size() != rhs.size()) return false;
    for (size_t i = 0; i < lhs.size(); i++) {
        if (std::tolower((unsigned char)lhs[i]) != std::tolower((unsigned char)rhs[i])) return false;
    }
    return true;
}

static bool OPNStoreGameMatchesLibraryGame(const OPN::GameInfo &storeGame, const OPN::GameInfo &libraryGame) {
    if (!storeGame.uuid.empty() && storeGame.uuid == libraryGame.uuid) return true;
    if (!storeGame.id.empty() && storeGame.id == libraryGame.id) return true;
    if (!storeGame.launchAppId.empty() && storeGame.launchAppId == libraryGame.launchAppId) return true;
    if (!storeGame.title.empty() && OPNStoreStringEqualsCaseInsensitive(storeGame.title, libraryGame.title)) return true;
    return false;
}

static bool OPNStoreVariantIsLibrarySelected(const OPN::GameVariant &variant) {
    return variant.librarySelected || variant.inLibrary ||
           variant.serviceStatus == "MANUAL" ||
           variant.serviceStatus == "PLATFORM_SYNC" ||
           variant.serviceStatus == "IN_LIBRARY";
}

static int OPNStoreSelectedLibraryVariantIndex(const OPN::GameInfo &libraryGame) {
    for (size_t i = 0; i < libraryGame.variants.size(); i++) {
        if (libraryGame.variants[i].librarySelected) return (int)i;
    }
    for (size_t i = 0; i < libraryGame.variants.size(); i++) {
        if (OPNStoreVariantIsLibrarySelected(libraryGame.variants[i])) return (int)i;
    }
    return libraryGame.variants.empty() ? -1 : 0;
}

@interface OPNStoreGameTile : NSView
@property (nonatomic, readonly) OPN::GameInfo game;
@property (nonatomic, assign) int selectedVariantIndex;
@property (nonatomic, copy) void (^onSelect)(void);
- (instancetype)initWithFrame:(NSRect)frame game:(const OPN::GameInfo &)game prominent:(BOOL)prominent;
- (void)setStoreFocused:(BOOL)focused;
@end

@interface OPNStoreGameTile ()
@property (nonatomic, assign) OPN::GameInfo gameData;
@property (nonatomic, strong) NSImageView *imageView;
@property (nonatomic, strong) NSView *gradientOverlay;
@property (nonatomic, strong) NSTextField *storeLabel;
@property (nonatomic, strong) NSTextField *titleLabel;
@property (nonatomic, strong) NSButton *playButton;
@property (nonatomic, strong) NSTrackingArea *trackingArea;
@property (nonatomic, assign) BOOL prominent;
@property (nonatomic, assign) BOOL storeFocused;
@end

@implementation OPNStoreGameTile

- (void)setSelectedVariantIndex:(int)selectedVariantIndex {
    _selectedVariantIndex = selectedVariantIndex;
    if (_selectedVariantIndex >= 0 && _selectedVariantIndex < (int)_gameData.variants.size()) {
        NSString *store = [NSString stringWithUTF8String:_gameData.variants[(size_t)_selectedVariantIndex].appStore.c_str()];
        self.storeLabel.stringValue = OPNStorePrimaryStoreName(_gameData);
        if (store.length > 0) {
            OPN::GameInfo selectedGame = _gameData;
            selectedGame.variants = {_gameData.variants[(size_t)_selectedVariantIndex]};
            self.storeLabel.stringValue = OPNStorePrimaryStoreName(selectedGame);
        }
    }
}

- (instancetype)initWithFrame:(NSRect)frame game:(const OPN::GameInfo &)game prominent:(BOOL)prominent {
    self = [super initWithFrame:frame];
    if (self) {
        _gameData = game;
        _prominent = prominent;
        _selectedVariantIndex = game.variants.empty() ? -1 : 0;
        self.wantsLayer = YES;
        self.layer.cornerRadius = prominent ? 24.0 : 16.0;
        self.layer.masksToBounds = YES;
        self.layer.backgroundColor = OpnColor(OPN::kSurfaceRaised, 0.72).CGColor;
        self.layer.borderWidth = 1.0;
        self.layer.borderColor = OpnColor(0xFFFFFF, prominent ? 0.16 : 0.10).CGColor;

        _imageView = [[NSImageView alloc] initWithFrame:self.bounds];
        _imageView.imageScaling = NSImageScaleProportionallyUpOrDown;
        _imageView.wantsLayer = YES;
        _imageView.layer.backgroundColor = OpnColor(OPN::kBackgroundC).CGColor;
        [self addSubview:_imageView];

        _gradientOverlay = [[NSView alloc] initWithFrame:self.bounds];
        _gradientOverlay.wantsLayer = YES;
        CAGradientLayer *gradient = [CAGradientLayer layer];
        gradient.colors = @[(id)OpnColor(OPN::kBlack, 0.0).CGColor,
                            (id)OpnColor(OPN::kBlack, prominent ? 0.18 : 0.08).CGColor,
                            (id)OpnColor(OPN::kBlack, prominent ? 0.84 : 0.74).CGColor];
        gradient.locations = @[@0.0, @0.46, @1.0];
        gradient.startPoint = CGPointMake(0.5, 0.0);
        gradient.endPoint = CGPointMake(0.5, 1.0);
        _gradientOverlay.layer = gradient;
        [self addSubview:_gradientOverlay];

        CGFloat titleSize = prominent ? 31.0 : 15.0;
        CGFloat storeSize = prominent ? 13.0 : 12.0;
        _storeLabel = OpnLabel(OPNStorePrimaryStoreName(game), NSZeroRect, storeSize, OpnColor(OPN::kTextSecondary), NSFontWeightSemibold);
        _storeLabel.lineBreakMode = NSLineBreakByTruncatingTail;
        [self addSubview:_storeLabel];

        NSString *title = game.title.empty() ? @"Untitled" : [NSString stringWithUTF8String:game.title.c_str()];
        _titleLabel = OpnLabel(title, NSZeroRect, titleSize, OpnColor(OPN::kTextPrimary), NSFontWeightBold);
        _titleLabel.lineBreakMode = NSLineBreakByTruncatingTail;
        _titleLabel.maximumNumberOfLines = prominent ? 2 : 1;
        [self addSubview:_titleLabel];

        _playButton = [[NSButton alloc] initWithFrame:NSZeroRect];
        _playButton.title = prominent ? @"Play Now" : @"▶";
        _playButton.bordered = NO;
        _playButton.font = [NSFont systemFontOfSize:prominent ? 14.0 : 16.0 weight:NSFontWeightBold];
        _playButton.contentTintColor = OpnColor(OPN::kAccentOn);
        _playButton.wantsLayer = YES;
        _playButton.layer.backgroundColor = OpnColor(OPN::kBrandGreen, 0.96).CGColor;
        _playButton.layer.shadowColor = OpnColor(OPN::kBrandGreen).CGColor;
        _playButton.layer.shadowOpacity = prominent ? 0.30 : 0.0;
        _playButton.layer.shadowRadius = prominent ? 18.0 : 0.0;
        _playButton.layer.shadowOffset = CGSizeZero;
        _playButton.hidden = !prominent;
        _playButton.target = self;
        _playButton.action = @selector(selectPressed);
        [self addSubview:_playButton];

        [self loadImage];
        [self updateTrackingAreas];
    }
    return self;
}

- (BOOL)isFlipped { return YES; }
- (OPN::GameInfo)game { return _gameData; }

- (void)layout {
    [super layout];
    CGFloat width = NSWidth(self.bounds);
    CGFloat height = NSHeight(self.bounds);
    self.imageView.frame = self.bounds;
    self.gradientOverlay.frame = self.bounds;
    if (self.prominent) {
        self.storeLabel.frame = NSMakeRect(30.0, height - 116.0, width - 210.0, 20.0);
        self.titleLabel.frame = NSMakeRect(30.0, height - 92.0, width - 220.0, 74.0);
        self.playButton.frame = NSMakeRect(width - 152.0, height - 70.0, 112.0, 42.0);
        self.playButton.layer.cornerRadius = 21.0;
    } else {
        self.storeLabel.frame = NSMakeRect(14.0, height - 54.0, width - 28.0, 17.0);
        self.titleLabel.frame = NSMakeRect(14.0, height - 32.0, width - 28.0, 20.0);
        self.playButton.frame = NSMakeRect(width - 52.0, height - 54.0, 38.0, 38.0);
        self.playButton.layer.cornerRadius = 19.0;
    }
}

- (void)setStoreFocused:(BOOL)focused {
    _storeFocused = focused;
    self.alphaValue = 1.0;
    self.layer.borderWidth = focused ? 2.5 : 1.0;
    self.layer.borderColor = (focused ? OpnColor(0xF4D3FF, 0.98) : OpnColor(0xFFFFFF, self.prominent ? 0.16 : 0.10)).CGColor;
    self.layer.shadowColor = OpnColor(0xDFAAFF, 1.0).CGColor;
    self.layer.shadowOpacity = focused ? 0.34 : 0.0;
    self.layer.shadowRadius = focused ? 22.0 : 0.0;
    self.layer.shadowOffset = CGSizeZero;
    self.playButton.hidden = !(self.prominent || focused);
}

- (void)selectPressed {
    if (self.onSelect) self.onSelect();
}

- (void)mouseDown:(NSEvent *)event {
    (void)event;
    [self selectPressed];
}

- (void)mouseEntered:(NSEvent *)event {
    (void)event;
    if (!self.prominent) self.playButton.hidden = NO;
    if (!self.storeFocused) self.layer.borderColor = OpnColor(0xFFFFFF, 0.24).CGColor;
}

- (void)mouseExited:(NSEvent *)event {
    (void)event;
    if (!self.prominent && !self.storeFocused) self.playButton.hidden = YES;
    if (!self.storeFocused) self.layer.borderColor = OpnColor(0xFFFFFF, self.prominent ? 0.16 : 0.10).CGColor;
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

- (void)loadImage {
    NSString *hero = _gameData.heroImageUrl.empty() ? nil : [NSString stringWithUTF8String:_gameData.heroImageUrl.c_str()];
    NSString *poster = _gameData.imageUrl.empty() ? nil : [NSString stringWithUTF8String:_gameData.imageUrl.c_str()];
    NSArray<NSString *> *candidates = self.prominent ? @[hero ?: @"", poster ?: @""] : @[poster ?: @"", hero ?: @""];
    [self loadImageFromCandidates:candidates index:0];
}

- (void)loadImageFromCandidates:(NSArray<NSString *> *)urlStrings index:(NSUInteger)index {
    if (index >= urlStrings.count) return;
    NSString *urlString = urlStrings[index];
    if (urlString.length == 0) {
        [self loadImageFromCandidates:urlStrings index:index + 1];
        return;
    }
    NSURL *url = [NSURL URLWithString:urlString];
    if (!url) {
        [self loadImageFromCandidates:urlStrings index:index + 1];
        return;
    }

    __weak __typeof__(self) weakSelf = self;
    [[[NSURLSession sharedSession] dataTaskWithURL:url completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        NSHTTPURLResponse *http = [response isKindOfClass:NSHTTPURLResponse.class] ? (NSHTTPURLResponse *)response : nil;
        if (error || !data || (http && http.statusCode >= 400)) {
            dispatch_async(dispatch_get_main_queue(), ^{
                __typeof__(self) strongSelf = weakSelf;
                [strongSelf loadImageFromCandidates:urlStrings index:index + 1];
            });
            return;
        }
        NSImage *image = [[NSImage alloc] initWithData:data];
        if (!image) return;
        dispatch_async(dispatch_get_main_queue(), ^{
            __typeof__(self) strongSelf = weakSelf;
            if (!strongSelf) return;
            strongSelf.imageView.image = image;
        });
    }] resume];
}

@end

@interface OPNStoreView ()
@property (nonatomic, strong) NSScrollView *scrollView;
@property (nonatomic, strong) OPNStoreDocumentView *documentView;
@property (nonatomic, strong) OPNLoadingView *loadingView;
@property (nonatomic, strong) NSTextField *statusLabel;
@property (nonatomic, assign) std::vector<OPN::PanelResult> panels;
@property (nonatomic, assign) std::vector<OPN::GameInfo> libraryGames;
@property (nonatomic, strong) NSMutableArray<NSMutableArray<OPNStoreGameTile *> *> *rowCards;
@property (nonatomic, strong) NSTimer *heroRotationTimer;
@property (nonatomic, strong) NSView *streamPipContainerView;
@property (nonatomic, strong) NSView *streamPipHostView;
@property (nonatomic, strong) NSTextField *streamPipTitleLabel;
@property (nonatomic, strong) NSTextField *streamPipHintLabel;
@property (nonatomic, strong) NSButton *streamPipButton;
@property (nonatomic, weak) NSView *streamPipContentView;
@property (nonatomic, assign) NSInteger currentHeroIndex;
@property (nonatomic, assign) CGFloat lastLayoutWidth;
@end

@implementation OPNStoreView

using namespace OPN;

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.layer.backgroundColor = [NSColor clearColor].CGColor;
        _rowCards = [NSMutableArray array];
        _scrollView = [[NSScrollView alloc] initWithFrame:self.bounds];
        _scrollView.drawsBackground = NO;
        _scrollView.borderType = NSNoBorder;
        _scrollView.hasVerticalScroller = YES;
        _scrollView.hasHorizontalScroller = NO;
        _scrollView.autohidesScrollers = YES;
        _scrollView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        [self addSubview:_scrollView];

        _documentView = [[OPNStoreDocumentView alloc] initWithFrame:NSMakeRect(0, 0, NSWidth(frame), NSHeight(frame))];
        _documentView.wantsLayer = YES;
        _scrollView.documentView = _documentView;

        _statusLabel = OpnLabel(@"", NSZeroRect, 15.0, OpnColor(kTextMuted), NSFontWeightMedium, NSTextAlignmentCenter);
        [self addSubview:_statusLabel];

        _loadingView = [[OPNLoadingView alloc] initWithFrame:self.bounds message:@"Loading Store..."];
        _loadingView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        _loadingView.hidden = YES;
        [self addSubview:_loadingView];

        _streamPipContainerView = [[NSView alloc] initWithFrame:NSZeroRect];
        _streamPipContainerView.hidden = YES;
        _streamPipContainerView.wantsLayer = YES;
        _streamPipContainerView.layer.cornerRadius = 22.0;
        _streamPipContainerView.layer.masksToBounds = NO;
        _streamPipContainerView.layer.backgroundColor = OpnColor(0x030507, 0.72).CGColor;
        _streamPipContainerView.layer.borderWidth = 1.0;
        _streamPipContainerView.layer.borderColor = OpnColor(0xFFFFFF, 0.18).CGColor;
        _streamPipContainerView.layer.shadowColor = NSColor.blackColor.CGColor;
        _streamPipContainerView.layer.shadowOpacity = 0.32;
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

        _streamPipButton = [[NSButton alloc] initWithFrame:NSZeroRect];
        _streamPipButton.bordered = NO;
        _streamPipButton.title = @"";
        _streamPipButton.target = self;
        _streamPipButton.action = @selector(streamPictureInPicturePressed:);
        [_streamPipContainerView addSubview:_streamPipButton];
        [self addSubview:_streamPipContainerView];
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (void)dealloc {
    [self.heroRotationTimer invalidate];
}

- (void)setLoading:(BOOL)loading {
    self.loadingView.hidden = !loading;
    self.statusLabel.stringValue = @"";
    if (loading) {
        [self.loadingView startAnimating];
    } else {
        [self.loadingView stopAnimating];
    }
}

- (void)setError:(NSString *)message {
    [self.heroRotationTimer invalidate];
    self.heroRotationTimer = nil;
    [self setLoading:NO];
    self.statusLabel.stringValue = message ?: @"";
}

- (void)setPanels:(const std::vector<OPN::PanelResult> &)panels {
    _panels = panels;
    self.currentHeroIndex = 0;
    [self configureHeroRotationTimer];
    [self renderStore];
}

- (void)setLibraryGames:(const std::vector<OPN::GameInfo> &)games {
    _libraryGames = games;
    [self renderStore];
}

- (void)setStreamPictureInPictureView:(NSView *)view title:(NSString *)title {
    if (self.streamPipContentView == view) {
        self.streamPipTitleLabel.stringValue = title.length > 0 ? title : @"Current Stream";
        [self setNeedsLayout:YES];
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
    }
    [self setNeedsLayout:YES];
}

- (void)streamPictureInPicturePressed:(id)sender {
    (void)sender;
    if (self.onStreamPictureInPictureSelected) self.onStreamPictureInPictureSelected();
}

- (int)selectedVariantIndexForStoreGame:(const GameInfo &)storeGame {
    for (const GameInfo &libraryGame : _libraryGames) {
        if (!OPNStoreGameMatchesLibraryGame(storeGame, libraryGame)) continue;
        int libraryVariantIndex = OPNStoreSelectedLibraryVariantIndex(libraryGame);
        if (libraryVariantIndex < 0 || libraryVariantIndex >= (int)libraryGame.variants.size()) return storeGame.variants.empty() ? -1 : 0;

        const GameVariant &libraryVariant = libraryGame.variants[(size_t)libraryVariantIndex];
        for (size_t i = 0; i < storeGame.variants.size(); i++) {
            const GameVariant &storeVariant = storeGame.variants[i];
            if (!libraryVariant.id.empty() && storeVariant.id == libraryVariant.id) return (int)i;
        }
        for (size_t i = 0; i < storeGame.variants.size(); i++) {
            const GameVariant &storeVariant = storeGame.variants[i];
            if (!libraryVariant.appStore.empty() && OPNStoreStringEqualsCaseInsensitive(storeVariant.appStore, libraryVariant.appStore)) return (int)i;
        }
    }
    return storeGame.variants.empty() ? -1 : 0;
}

- (NSInteger)heroCandidateCount {
    NSInteger count = 0;
    for (const PanelResult &panel : _panels) {
        for (const PanelSection &section : panel.sections) {
            count += (NSInteger)section.games.size();
        }
    }
    return count;
}

- (const GameInfo *)currentHeroGame {
    NSInteger candidateCount = [self heroCandidateCount];
    if (candidateCount <= 0) return nullptr;
    NSInteger target = ((self.currentHeroIndex % candidateCount) + candidateCount) % candidateCount;
    NSInteger index = 0;
    for (const PanelResult &panel : _panels) {
        for (const PanelSection &section : panel.sections) {
            for (const GameInfo &game : section.games) {
                if (index == target) return &game;
                index++;
            }
        }
    }
    return nullptr;
}

- (void)configureHeroRotationTimer {
    [self.heroRotationTimer invalidate];
    self.heroRotationTimer = nil;
    if ([self heroCandidateCount] < 2) return;

    self.heroRotationTimer = [NSTimer scheduledTimerWithTimeInterval:7.0
                                                              target:self
                                                            selector:@selector(heroRotationTimerFired:)
                                                            userInfo:nil
                                                             repeats:YES];
}

- (void)heroRotationTimerFired:(NSTimer *)timer {
    (void)timer;
    NSInteger candidateCount = [self heroCandidateCount];
    if (candidateCount < 2) return;
    self.currentHeroIndex = (self.currentHeroIndex + 1) % candidateCount;
    [self renderStore];
}

- (void)layout {
    [super layout];
    CGFloat navClearance = kStoreNavigationClearance;
    self.scrollView.frame = NSMakeRect(0.0, navClearance, NSWidth(self.bounds), MAX(0.0, NSHeight(self.bounds) - navClearance));
    self.loadingView.frame = self.bounds;
    self.statusLabel.frame = NSMakeRect(0, NSHeight(self.bounds) * 0.5, NSWidth(self.bounds), 26.0);
    BOOL showStreamPip = OpnControllerModeEnabled() && self.streamPipContentView != nil;
    self.streamPipContainerView.hidden = !showStreamPip;
    if (showStreamPip) {
        CGFloat pipWidth = MIN(420.0, MAX(300.0, NSWidth(self.bounds) * 0.24));
        CGFloat pipVideoHeight = floor(pipWidth * 9.0 / 16.0);
        CGFloat pipHeight = pipVideoHeight + 54.0;
        CGFloat pipX = MAX(28.0, NSWidth(self.bounds) - pipWidth - 42.0);
        CGFloat pipY = MAX(116.0, MIN(NSHeight(self.bounds) - pipHeight - 34.0, 144.0));
        self.streamPipContainerView.frame = NSMakeRect(pipX, pipY, pipWidth, pipHeight);
        self.streamPipHostView.frame = NSMakeRect(12.0, 12.0, pipWidth - 24.0, pipVideoHeight);
        self.streamPipContentView.frame = self.streamPipHostView.bounds;
        self.streamPipTitleLabel.frame = NSMakeRect(16.0, pipVideoHeight + 22.0, pipWidth * 0.50, 22.0);
        self.streamPipHintLabel.frame = NSMakeRect(pipWidth * 0.50 - 12.0, pipVideoHeight + 24.0, pipWidth * 0.50, 18.0);
        self.streamPipButton.frame = self.streamPipContainerView.bounds;
    }
    if (std::fabs(self.lastLayoutWidth - NSWidth(self.bounds)) > 1.0) {
        self.lastLayoutWidth = NSWidth(self.bounds);
        [self renderStore];
    }
}

- (void)renderStore {
    for (NSView *view in [self.documentView.subviews copy]) {
        [view removeFromSuperview];
    }
    [self.rowCards removeAllObjects];

    CGFloat width = MAX(960.0, NSWidth(self.bounds));
    CGFloat contentX = 78.0;
    CGFloat contentWidth = MAX(640.0, width - contentX * 2.0);
    CGFloat y = kStoreTopInset;

    NSTextField *eyebrow = OpnLabel(@"GEFORCE NOW STORE", NSMakeRect(contentX, y, contentWidth, 18.0), 12.0, OpnColor(kBrandGreen), NSFontWeightBold, NSTextAlignmentCenter);
    eyebrow.stringValue = [eyebrow.stringValue uppercaseString];
    [self.documentView addSubview:eyebrow];
    NSTextField *title = OpnLabel(@"Featured Cloud Games", NSMakeRect(contentX, y + 20.0, contentWidth, 52.0), 36.0, OpnColor(kTextPrimary), NSFontWeightBold, NSTextAlignmentCenter);
    [self.documentView addSubview:title];
    NSTextField *subtitle = OpnLabel(@"Browse curated collections with fast launch access across your linked stores.", NSMakeRect(contentX, y + 72.0, contentWidth, 24.0), 14.0, OpnColor(kTextSecondary), NSFontWeightMedium, NSTextAlignmentCenter);
    [self.documentView addSubview:subtitle];

    const GameInfo *heroGame = [self currentHeroGame];

    CGFloat heroHeight = 0.0;
    if (heroGame) {
        CGFloat heroWidth = MIN(contentWidth, 1120.0) * 0.5;
        heroHeight = floor(heroWidth * 9.0 / 16.0);
        CGFloat heroX = contentX + floor((contentWidth - heroWidth) / 2.0);
        [self addHeroGame:*heroGame y:y + kStoreHeroTopOffset contentX:heroX width:heroWidth height:heroHeight];
    }

    CGFloat rowY = heroGame ? y + kStoreHeroTopOffset + heroHeight + 58.0 : y + 128.0;
    NSInteger renderedRows = 0;
    for (const PanelResult &panel : _panels) {
        for (const PanelSection &section : panel.sections) {
            if (section.games.empty()) continue;
            [self addSection:section index:renderedRows y:rowY contentX:contentX width:width];
            rowY += kStoreRowHeight;
            renderedRows++;
        }
    }

    if (renderedRows == 0 && !self.loadingView.hidden) {
        self.statusLabel.stringValue = @"";
    } else if (renderedRows == 0) {
        self.statusLabel.stringValue = @"No Store collections found.";
    } else {
        self.statusLabel.stringValue = @"";
    }

    self.documentView.frame = NSMakeRect(0, 0, width, MAX(NSHeight(self.bounds), rowY + 80.0));
}

- (void)addHeroGame:(const GameInfo &)game y:(CGFloat)y contentX:(CGFloat)contentX width:(CGFloat)width height:(CGFloat)height {
    NSRect heroRect = NSMakeRect(contentX, y, width, height);
    OPNStoreGameTile *hero = [[OPNStoreGameTile alloc] initWithFrame:heroRect game:game prominent:YES];
    hero.selectedVariantIndex = [self selectedVariantIndexForStoreGame:game];
    __weak __typeof__(self) weakSelf = self;
    __weak OPNStoreGameTile *weakHero = hero;
    hero.onSelect = ^{
        __typeof__(self) strongSelf = weakSelf;
        OPNStoreGameTile *strongHero = weakHero;
        if (!strongSelf || !strongHero || !strongSelf.onSelectGame) return;
        strongSelf.onSelectGame(strongHero.game, strongHero.selectedVariantIndex);
    };
    [self.documentView addSubview:hero];
}

- (void)addSection:(const PanelSection &)section index:(NSInteger)sectionIndex y:(CGFloat)y contentX:(CGFloat)contentX width:(CGFloat)width {
    NSString *sectionTitle = section.title.empty() ? @"Featured" : [NSString stringWithUTF8String:section.title.c_str()];
    NSTextField *label = OpnLabel(sectionTitle, NSMakeRect(contentX, y, width - contentX * 2.0, 28.0), 22.0, OpnColor(kTextPrimary), NSFontWeightBold);
    [self.documentView addSubview:label];
    NSTextField *railHint = OpnLabel(@"Browse", NSMakeRect(width - contentX - 92.0, y + 5.0, 92.0, 18.0), 12.0, OpnColor(kTextMuted), NSFontWeightSemibold, NSTextAlignmentRight);
    [self.documentView addSubview:railHint];

    OPNStoreRailScrollView *rowScroll = [[OPNStoreRailScrollView alloc] initWithFrame:NSMakeRect(contentX, y + 44.0, MAX(320.0, width - contentX * 2.0), kStoreTileHeight + 24.0)];
    rowScroll.drawsBackground = NO;
    rowScroll.borderType = NSNoBorder;
    rowScroll.hasHorizontalScroller = YES;
    rowScroll.hasVerticalScroller = NO;
    rowScroll.autohidesScrollers = YES;
    [self.documentView addSubview:rowScroll];

    OPNStoreDocumentView *rowDocument = [[OPNStoreDocumentView alloc] initWithFrame:NSMakeRect(0, 0, NSWidth(rowScroll.frame), kStoreTileHeight + 24.0)];
    rowDocument.wantsLayer = YES;
    rowScroll.documentView = rowDocument;

    NSMutableArray<OPNStoreGameTile *> *cards = [NSMutableArray array];
    CGFloat x = 0.0;
    NSInteger column = 0;
    for (const GameInfo &game : section.games) {
        BOOL focused = NO;
        CGFloat cardWidth = focused ? kStoreTileWidth + 28.0 : kStoreTileWidth;
        CGFloat cardHeight = focused ? kStoreTileHeight + 16.0 : kStoreTileHeight;
        CGFloat cardY = focused ? 0.0 : 8.0;
        OPNStoreGameTile *card = [[OPNStoreGameTile alloc] initWithFrame:NSMakeRect(x, cardY, cardWidth, cardHeight) game:game prominent:NO];
        card.selectedVariantIndex = [self selectedVariantIndexForStoreGame:game];
        [card setStoreFocused:focused];
        __weak __typeof__(self) weakSelf = self;
        __weak OPNStoreGameTile *weakCard = card;
        card.onSelect = ^{
            __typeof__(self) strongSelf = weakSelf;
            OPNStoreGameTile *strongCard = weakCard;
            if (!strongSelf || !strongCard || !strongSelf.onSelectGame) return;
            int variantIndex = strongCard.selectedVariantIndex >= 0 ? strongCard.selectedVariantIndex : 0;
            strongSelf.onSelectGame(strongCard.game, variantIndex);
        };
        [rowDocument addSubview:card];
        [cards addObject:card];
        x += cardWidth + kStoreCardSpacing;
        column++;
        if (column >= 18) break;
    }
    rowDocument.frame = NSMakeRect(0, 0, MAX(x + contentX, NSWidth(rowScroll.frame)), kStoreTileHeight + 24.0);
    [self.rowCards addObject:cards];
}

@end
