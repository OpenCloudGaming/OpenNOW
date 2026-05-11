#import "OPNGameCatalogView.h"
#import "OPNGameCardView.h"
#import "OPNLoadingView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#import <GameController/GameController.h>
#include <QuartzCore/QuartzCore.h>
#include <algorithm>
#include <cmath>

static const CGFloat kGridPadding = 28.0;
static const CGFloat kCardSpacing = 18.0;
static const CGFloat kNavHeight = 62.0;
static const CGFloat kToolbarHeight = 82.0;

static NSString *OPNCatalogString(const std::string &value, NSString *fallback = @"") {
    return value.empty() ? fallback : [NSString stringWithUTF8String:value.c_str()];
}

@interface OPNFlippedGridDocumentView : NSView
@end

@implementation OPNFlippedGridDocumentView
- (BOOL)isFlipped { return YES; }
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
@property (nonatomic, strong) NSView *controllerDetailView;
@property (nonatomic, strong) NSTextField *controllerDetailTitleLabel;
@property (nonatomic, strong) NSTextField *controllerDetailMetaLabel;
@property (nonatomic, strong) NSTextField *controllerDetailStoreLabel;
@property (nonatomic, strong) NSTextField *controllerDetailFeaturesLabel;
@property (nonatomic, strong) NSTextField *controllerDetailHintLabel;
@property (nonatomic, strong) NSMutableArray<OPNGameCardView *> *cardViews;
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
@property (nonatomic, assign) NSInteger gridColumnCount;
@property (nonatomic, strong) NSView *detailsOverlayView;
@property (nonatomic, strong) NSTimer *gamepadNavigationTimer;
@property (nonatomic, assign) uint16_t previousGamepadButtons;
@property (nonatomic, assign) CFTimeInterval lastGamepadMoveTime;
- (void)scrollLibraryToTop;
- (void)requestCatalogBrowse;
- (void)focusCardAtIndex:(NSInteger)index scrollIntoView:(BOOL)scrollIntoView;
- (void)openFocusedGameDetails;
- (void)closeGameDetails;
- (void)launchFocusedGame;
- (void)cycleFocusedVariant;
- (void)updateControllerDetailContent;
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
    return buttons;
}

static NSString *OPNCatalogJoinedStrings(const std::vector<std::string> &values, NSString *fallback) {
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (const std::string &value : values) {
        if (!value.empty()) [items addObject:[NSString stringWithUTF8String:value.c_str()]];
        if (items.count >= 4) break;
    }
    return items.count > 0 ? [items componentsJoinedByString:@"  /  "] : fallback;
}

@implementation OPNGameCatalogView

using namespace OPN;

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        _cardViews = [NSMutableArray array];
        _selectedSortId = @"last_played";
        _selectedFilterIds = [NSMutableSet set];
        _focusedCardIndex = -1;
        _gridColumnCount = 1;
        self.wantsLayer = YES;
        self.layer.backgroundColor = [NSColor clearColor].CGColor;

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
        _searchField.layer.backgroundColor = OpnColor(0x15171C, 0.92).CGColor;
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

        CGFloat gridY = kNavHeight + kToolbarHeight;
        NSRect scrollFrame = NSMakeRect(0, gridY, frame.size.width, frame.size.height - gridY);
        _scrollView = [[NSScrollView alloc] initWithFrame:scrollFrame];
        _scrollView.hasVerticalScroller = YES;
        _scrollView.hasHorizontalScroller = NO;
        _scrollView.autohidesScrollers = YES;
        _scrollView.drawsBackground = NO;
        _scrollView.borderType = NSNoBorder;
        [self addSubview:_scrollView];

        // Grid content
        _gridContentView = [[OPNFlippedGridDocumentView alloc] initWithFrame:NSMakeRect(0, 0, frame.size.width, 100)];
        _gridContentView.wantsLayer = YES;
        _scrollView.documentView = _gridContentView;

        _statusLabel = OpnLabel(@"", NSMakeRect(0, gridY + 100, frame.size.width, 24),
                                15, OpnColor(kTextMuted));
        _statusLabel.alignment = NSTextAlignmentCenter;
        [self addSubview:_statusLabel];

        _controllerDetailView = [[OPNFlippedGridDocumentView alloc] initWithFrame:NSZeroRect];
        _controllerDetailView.hidden = YES;
        _controllerDetailView.wantsLayer = YES;
        _controllerDetailView.layer.cornerRadius = 26.0;
        _controllerDetailView.layer.borderWidth = 1.0;
        _controllerDetailView.layer.borderColor = OpnColor(kBrandGreen, 0.20).CGColor;
        _controllerDetailView.layer.backgroundColor = OpnColor(0x07090F, 0.32).CGColor;
        [self addSubview:_controllerDetailView];

        _controllerDetailTitleLabel = OpnLabel(@"Select a game", NSZeroRect, 42.0, OpnColor(kTextPrimary), NSFontWeightSemibold);
        _controllerDetailTitleLabel.lineBreakMode = NSLineBreakByTruncatingTail;
        [_controllerDetailView addSubview:_controllerDetailTitleLabel];

        _controllerDetailMetaLabel = OpnLabel(@"", NSZeroRect, 15.0, OpnColor(kTextSecondary), NSFontWeightMedium);
        [_controllerDetailView addSubview:_controllerDetailMetaLabel];

        _controllerDetailStoreLabel = OpnLabel(@"", NSZeroRect, 16.0, OpnColor(kBrandGreen), NSFontWeightSemibold);
        [_controllerDetailView addSubview:_controllerDetailStoreLabel];

        _controllerDetailFeaturesLabel = OpnLabel(@"", NSZeroRect, 14.0, OpnColor(kTextMuted), NSFontWeightRegular);
        _controllerDetailFeaturesLabel.maximumNumberOfLines = 2;
        [_controllerDetailView addSubview:_controllerDetailFeaturesLabel];

        _controllerDetailHintLabel = OpnLabel(@"✕ Play     △ Change Store     L1/R1 Menu     Options Account", NSZeroRect, 13.0, OpnColor(kTextMuted), NSFontWeightMedium);
        [_controllerDetailView addSubview:_controllerDetailHintLabel];

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
    [self.gamepadNavigationTimer invalidate];
}

- (void)interfacePreferencesChanged:(NSNotification *)notification {
    (void)notification;
    self.layer.backgroundColor = OpnControllerModeEnabled() ? OpnColor(kBrandGreen, 0.035).CGColor : [NSColor clearColor].CGColor;
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
    [self renderGrid];
    [self scrollLibraryToTop];
    dispatch_async(dispatch_get_main_queue(), ^{
        [self scrollLibraryToTop];
    });
}

- (void)setCatalogBrowseResult:(const OPN::CatalogBrowseResult &)result {
    _allGames = result.games;
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
    [self renderGrid];
    [self scrollLibraryToTop];
}

- (void)renderGrid {
    for (OPNGameCardView *cv in _cardViews) { [cv removeFromSuperview]; }
    [_cardViews removeAllObjects];

    CGFloat cardWidth = [OPNGameCardView cardSize].width;
    CGFloat cardHeight = [OPNGameCardView cardSize].height;
    BOOL controllerMode = OpnControllerModeEnabled();
    CGFloat availableWidth = _scrollView.frame.size.width;
    NSInteger cols = controllerMode ? 1 : MAX(1, (NSInteger)((availableWidth + kCardSpacing) / (cardWidth + kCardSpacing)));
    self.gridColumnCount = cols;
    CGFloat gridSpacing = controllerMode ? 26.0 : (cols > 1 ? floor((availableWidth - cols * cardWidth) / (cols - 1)) : kCardSpacing);
    gridSpacing = MAX(kCardSpacing, gridSpacing);
    CGFloat xStart = controllerMode ? 32.0 : (cols > 1 ? 0.0 : floor(MAX(0.0, (_scrollView.frame.size.width - cardWidth) / 2.0)));
    CGFloat yPos = controllerMode ? 24.0 : kGridPadding;

    std::vector<OPN::GameInfo> displayGames = _allGames;

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
        [_gridContentView addSubview:card];
        [_cardViews addObject:card];

        col++;
        visibleCount++;
        if (!controllerMode && col >= cols) {
            col = 0;
            yPos += cardHeight + kCardSpacing;
        }
    }

    CGFloat totalHeight = controllerMode ? cardHeight + 48.0 : yPos + cardHeight + kGridPadding;
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
    self.scrollView.hasVerticalScroller = !controllerMode;
    self.scrollView.hasHorizontalScroller = NO;
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
    CGFloat cardHeight = [OPNGameCardView cardSize].height;
    CGFloat carouselHeight = cardHeight + 86.0;
    CGFloat gridY = controllerMode ? MAX(kNavHeight + 116.0, height - carouselHeight - 88.0) : kNavHeight + (compact ? 116.0 : kToolbarHeight);
    CGFloat detailY = kNavHeight + 24.0;
    CGFloat detailHeight = controllerMode ? MAX(150.0, gridY - detailY - 58.0) : 0.0;
    self.controllerDetailView.hidden = !controllerMode || self.cardViews.count == 0;
    self.controllerDetailView.frame = NSMakeRect(28.0, detailY, MAX(260.0, width - 56.0), detailHeight);
    CGFloat detailWidth = NSWidth(self.controllerDetailView.frame);
    self.controllerDetailTitleLabel.frame = NSMakeRect(28.0, 24.0, MAX(220.0, detailWidth - 56.0), 52.0);
    self.controllerDetailMetaLabel.frame = NSMakeRect(30.0, 84.0, MAX(220.0, detailWidth - 60.0), 22.0);
    self.controllerDetailStoreLabel.frame = NSMakeRect(30.0, 118.0, MAX(220.0, detailWidth - 60.0), 24.0);
    self.controllerDetailFeaturesLabel.frame = NSMakeRect(30.0, 154.0, MAX(220.0, detailWidth - 60.0), 46.0);
    self.controllerDetailHintLabel.frame = NSMakeRect(30.0, MAX(116.0, detailHeight - 34.0), MAX(220.0, detailWidth - 60.0), 18.0);
    self.scrollView.frame = controllerMode
        ? NSMakeRect(0, gridY, width, MIN(carouselHeight, MAX(0.0, height - gridY)))
        : NSMakeRect(0, gridY, width, MAX(0.0, height - gridY));
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
    NSInteger clamped = MAX(0, MIN(index, (NSInteger)self.cardViews.count - 1));
    self.focusedCardIndex = clamped;
    for (NSUInteger i = 0; i < self.cardViews.count; i++) {
        self.cardViews[i].controllerFocused = OpnControllerModeEnabled() && (NSInteger)i == clamped;
    }
    [self updateControllerDetailContent];
    if (!scrollIntoView) return;
    OPNGameCardView *card = self.cardViews[(NSUInteger)clamped];
    NSRect visibleRect = self.scrollView.contentView.bounds;
    NSRect targetRect = NSInsetRect(card.frame, -24.0, -24.0);
    if (!NSContainsRect(visibleRect, targetRect)) {
        [self.gridContentView scrollRectToVisible:targetRect];
        [self.scrollView reflectScrolledClipView:self.scrollView.contentView];
    }
}

- (OPNGameCardView *)focusedCard {
    if (self.focusedCardIndex < 0 || self.focusedCardIndex >= (NSInteger)self.cardViews.count) return nil;
    return self.cardViews[(NSUInteger)self.focusedCardIndex];
}

- (void)moveFocusByRows:(NSInteger)rows columns:(NSInteger)columns {
    if (OpnControllerModeEnabled() && rows != 0) return;
    NSInteger next = self.focusedCardIndex + rows * MAX(1, self.gridColumnCount) + columns;
    [self focusCardAtIndex:next scrollIntoView:YES];
}

- (void)cycleFocusedVariant {
    OPNGameCardView *card = [self focusedCard];
    if (!card || card.game.variants.size() <= 1) return;
    int next = (card.selectedVariantIndex + 1) % (int)card.game.variants.size();
    [card selectVariantAtIndex:next];
    [self updateControllerDetailContent];
}

- (void)updateControllerDetailContent {
    if (!OpnControllerModeEnabled()) return;
    OPNGameCardView *card = [self focusedCard];
    if (!card) {
        self.controllerDetailTitleLabel.stringValue = @"Select a game";
        self.controllerDetailMetaLabel.stringValue = @"";
        self.controllerDetailStoreLabel.stringValue = @"";
        self.controllerDetailFeaturesLabel.stringValue = @"";
        return;
    }

    const OPN::GameInfo game = card.game;
    self.controllerDetailTitleLabel.stringValue = OPNCatalogString(game.title, @"Untitled Game");

    NSString *genres = OPNCatalogJoinedStrings(game.genres, @"Cloud game");
    NSString *tier = OPNCatalogString(game.membershipTierLabel, @"");
    NSString *playability = OPNCatalogString(game.playabilityState, @"");
    NSMutableArray<NSString *> *meta = [NSMutableArray arrayWithObject:genres];
    if (tier.length > 0) [meta addObject:tier];
    if (playability.length > 0) [meta addObject:playability.capitalizedString];
    self.controllerDetailMetaLabel.stringValue = [meta componentsJoinedByString:@"  •  "];

    NSString *store = @"Default store";
    if (card.selectedVariantIndex >= 0 && card.selectedVariantIndex < (int)game.variants.size()) {
        store = OPNCatalogString(game.variants[(size_t)card.selectedVariantIndex].appStore, store);
    } else if (!game.availableStores.empty()) {
        store = OPNCatalogString(game.availableStores.front(), store);
    }
    NSString *storePrefix = game.variants.size() > 1 ? @"Selected store" : @"Store";
    self.controllerDetailStoreLabel.stringValue = [NSString stringWithFormat:@"%@: %@", storePrefix, store];

    NSString *features = OPNCatalogJoinedStrings(game.featureLabels, @"");
    if (features.length == 0 && !game.shortName.empty()) features = OPNCatalogString(game.shortName, @"");
    self.controllerDetailFeaturesLabel.stringValue = features.length > 0 ? features : @"Press Cross / A to launch this game.";
    self.controllerDetailHintLabel.stringValue = game.variants.size() > 1
        ? @"✕ Play     △ Change Store     L1/R1 Menu     Options Account"
        : @"✕ Play     L1/R1 Menu     Options Account";
}

- (void)openFocusedGameDetails {
    OPNGameCardView *card = [self focusedCard];
    if (!card) return;
    [self.detailsOverlayView removeFromSuperview];
    NSView *overlay = [[NSView alloc] initWithFrame:self.bounds];
    overlay.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    overlay.wantsLayer = YES;
    overlay.layer.backgroundColor = OpnColor(kBlack, 0.58).CGColor;

    CGFloat panelWidth = MIN(760.0, MAX(420.0, NSWidth(self.bounds) - 96.0));
    CGFloat panelHeight = 390.0;
    NSView *panel = [[OPNFlippedGridDocumentView alloc] initWithFrame:NSMakeRect(floor((NSWidth(self.bounds) - panelWidth) / 2.0),
                                                                                   floor((NSHeight(self.bounds) - panelHeight) / 2.0),
                                                                                   panelWidth,
                                                                                   panelHeight)];
    panel.wantsLayer = YES;
    panel.layer.cornerRadius = 30.0;
    panel.layer.borderWidth = 1.5;
    panel.layer.borderColor = OpnColor(kBrandGreen, 0.58).CGColor;
    panel.layer.backgroundColor = OpnColor(0x080A10, 0.94).CGColor;
    panel.layer.shadowColor = OpnColor(kBrandGreen).CGColor;
    panel.layer.shadowOpacity = 0.34;
    panel.layer.shadowRadius = 42.0;
    panel.layer.shadowOffset = CGSizeZero;
    [overlay addSubview:panel];

    NSString *title = OPNCatalogString(card.game.title, @"Game Details");
    NSTextField *titleLabel = OpnLabel(title, NSMakeRect(36.0, 34.0, panelWidth - 72.0, 42.0), 30.0, OpnColor(kTextPrimary), NSFontWeightSemibold);
    titleLabel.lineBreakMode = NSLineBreakByTruncatingTail;
    [panel addSubview:titleLabel];

    NSString *store = @"Default store";
    if (card.selectedVariantIndex >= 0 && card.selectedVariantIndex < (int)card.game.variants.size()) {
        store = OPNCatalogString(card.game.variants[(size_t)card.selectedVariantIndex].appStore, store);
    }
    NSTextField *storeLabel = OpnLabel([NSString stringWithFormat:@"Selected store: %@", store],
                                       NSMakeRect(38.0, 92.0, panelWidth - 76.0, 24.0),
                                       15.0,
                                       OpnColor(kBrandGreen),
                                       NSFontWeightSemibold);
    [panel addSubview:storeLabel];

    NSString *body = card.game.variants.size() > 1
        ? @"Press Triangle / Y to cycle stores. Press Cross / A to launch. Press Circle / B to return to the library."
        : @"Press Cross / A to launch. Press Circle / B to return to the library.";
    NSTextField *bodyLabel = OpnLabel(body, NSMakeRect(38.0, 136.0, panelWidth - 76.0, 58.0), 14.0, OpnColor(kTextSecondary), NSFontWeightRegular);
    bodyLabel.maximumNumberOfLines = 3;
    [panel addSubview:bodyLabel];

    NSButton *playButton = OpnButton(@"Play", NSMakeRect(38.0, panelHeight - 96.0, 180.0, 52.0), OpnColor(kBrandGreen, 0.96), OpnColor(kAccentOn));
    playButton.target = self;
    playButton.action = @selector(detailsPlayClicked:);
    playButton.layer.cornerRadius = 18.0;
    [panel addSubview:playButton];

    NSButton *closeButton = OpnButton(@"Back", NSMakeRect(232.0, panelHeight - 96.0, 132.0, 52.0), OpnColor(0xFFFFFF, 0.08), OpnColor(kTextPrimary), true, OpnColor(0xFFFFFF, 0.16));
    closeButton.target = self;
    closeButton.action = @selector(detailsCloseClicked:);
    closeButton.layer.cornerRadius = 18.0;
    [panel addSubview:closeButton];

    NSTextField *hints = OpnLabel(@"✕ Play     △ Change Store     ○ Back", NSMakeRect(38.0, panelHeight - 34.0, panelWidth - 76.0, 20.0), 12.0, OpnColor(kTextMuted), NSFontWeightMedium);
    [panel addSubview:hints];

    self.detailsOverlayView = overlay;
    [self addSubview:overlay];
}

- (void)closeGameDetails {
    [self.detailsOverlayView removeFromSuperview];
    self.detailsOverlayView = nil;
    [self.window makeFirstResponder:self];
}

- (void)launchFocusedGame {
    OPNGameCardView *card = [self focusedCard];
    if (!card || !self.onSelectGame) return;
    int variantIdx = card.selectedVariantIndex >= 0 ? card.selectedVariantIndex : 0;
    self.onSelectGame(card.game, variantIdx);
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
            return;
        default:
            break;
    }
    if ([chars isEqualToString:@"v"] || [chars isEqualToString:@"y"]) {
        [self cycleFocusedVariant];
        return;
    }
    [super keyDown:event];
}

- (void)startGamepadNavigationIfNeeded {
    if (!OpnControllerModeEnabled() || self.gamepadNavigationTimer) return;
    self.gamepadNavigationTimer = [NSTimer scheduledTimerWithTimeInterval:(1.0 / 30.0)
                                                                   target:self
                                                                 selector:@selector(pollGamepadNavigation)
                                                                 userInfo:nil
                                                                  repeats:YES];
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
    if (!OpnControllerModeEnabled()) {
        [self.gamepadNavigationTimer invalidate];
        self.gamepadNavigationTimer = nil;
        self.previousGamepadButtons = 0;
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
    if (pressed & (1u << 1)) { }
    if (pressed & (1u << 2)) [self cycleFocusedVariant];
    if (pressed & (1u << 5)) [self moveFocusByRows:-1 columns:0];
    if (pressed & (1u << 6)) [self moveFocusByRows:1 columns:0];
    if (pressed & (1u << 7)) [self moveFocusByRows:0 columns:-1];
    if (pressed & (1u << 8)) [self moveFocusByRows:0 columns:1];
    self.previousGamepadButtons = buttons;
}

- (void)signOutClicked {
    if (self.onSignOut) self.onSignOut();
}

@end
