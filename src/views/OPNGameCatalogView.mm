#import "OPNGameCatalogView.h"
#import "OPNGameCardView.h"
#import "OPNLoadingView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
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
- (void)scrollLibraryToTop;
- (void)requestCatalogBrowse;
@end

@implementation OPNGameCatalogView

using namespace OPN;

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        _cardViews = [NSMutableArray array];
        _selectedSortId = @"last_played";
        _selectedFilterIds = [NSMutableSet set];
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

        _loadingView = [[OPNLoadingView alloc] initWithFrame:self.bounds
                                                     message:@"Loading games..."];
        _loadingView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        _loadingView.hidden = YES;
        [self addSubview:_loadingView];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(interfacePreferencesChanged:)
                                                     name:OPNInterfacePreferencesDidChangeNotification
                                                   object:nil];
        [self layoutCatalogSubviews];
    }
    return self;
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)interfacePreferencesChanged:(NSNotification *)notification {
    (void)notification;
    [self renderGrid];
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
    CGFloat availableWidth = _scrollView.frame.size.width;
    NSInteger cols = MAX(1, (NSInteger)((availableWidth + kCardSpacing) / (cardWidth + kCardSpacing)));
    CGFloat gridSpacing = cols > 1 ? floor((availableWidth - cols * cardWidth) / (cols - 1)) : kCardSpacing;
    gridSpacing = MAX(kCardSpacing, gridSpacing);
    CGFloat xStart = cols > 1 ? 0.0 : floor(MAX(0.0, (_scrollView.frame.size.width - cardWidth) / 2.0));
    CGFloat yPos = kGridPadding;

    std::vector<OPN::GameInfo> displayGames = _allGames;

    NSInteger col = 0;
    NSInteger visibleCount = 0;
    for (auto it = displayGames.begin(); it != displayGames.end(); ++it) {
        auto &game = *it;
        CGFloat x = xStart + col * (cardWidth + gridSpacing);
        NSRect cardFrame = NSMakeRect(x, yPos, cardWidth, cardHeight);
        OPNGameCardView *card = [[OPNGameCardView alloc] initWithFrame:cardFrame game:game];
        GameInfo gameCopy = game;
        __weak __typeof__(self) weakSelf = self;
        __weak OPNGameCardView *weakCard = card;
        card.onPlay = ^{
            __typeof__(self) s = weakSelf;
            OPNGameCardView *c = weakCard;
            if (s && s.onSelectGame && c) {
                int variantIdx = c.selectedVariantIndex;
                s.onSelectGame(gameCopy, variantIdx >= 0 ? variantIdx : 0);
            }
        };
        [_gridContentView addSubview:card];
        [_cardViews addObject:card];

        col++;
        visibleCount++;
        if (col >= cols) {
            col = 0;
            yPos += cardHeight + kCardSpacing;
        }
    }

    CGFloat totalHeight = yPos + cardHeight + kGridPadding;
    if (col == 0 && visibleCount > 0) totalHeight = yPos + kGridPadding;
    CGFloat totalWidth = _scrollView.frame.size.width;
    _gridContentView.frame = NSMakeRect(0, 0,
        MAX(totalWidth, _scrollView.frame.size.width),
        MAX(totalHeight, _scrollView.frame.size.height));

    _gameCountLabel.stringValue = [NSString stringWithFormat:@"%ld %@", (long)visibleCount, visibleCount == 1 ? @"game" : @"games"];
    if (self.onGameCountChanged) self.onGameCountChanged(visibleCount);
    _statusLabel.stringValue = visibleCount == 0 ? @"No games found." : @"";
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
    self.scrollView.hasVerticalScroller = YES;
    self.scrollView.hasHorizontalScroller = NO;
    BOOL compact = width < 900.0;
    self.searchField.hidden = NO;
    self.filterButton.hidden = compact;
    self.signOutButton.hidden = YES;
    CGFloat searchWidth = compact ? MAX(240.0, width - 64.0) : MIN(520.0, MAX(360.0, width * 0.38));
    CGFloat searchX = floor((width - searchWidth) / 2.0);
    self.libraryIconLabel.frame = NSMakeRect(0, 0, 0, 0);
    self.titleLabel.frame = NSMakeRect(0, 0, 0, 0);
    self.userLabel.frame = NSMakeRect(24, kNavHeight + 36, 260, 18);
    self.searchField.frame = NSMakeRect(searchX, compact ? kNavHeight + 62 : kNavHeight + 25, searchWidth, 40);
    self.sortButton.hidden = compact;
    self.filterButton.frame = NSMakeRect(NSMaxX(self.searchField.frame) + 14, kNavHeight + 26, 124, 38);
    self.sortButton.frame = NSMakeRect(NSMaxX(self.filterButton.frame) + 10, kNavHeight + 26, 154, 38);
    self.gameCountLabel.frame = NSMakeRect(0, 0, 0, 0);
    self.gameCountLabel.hidden = YES;
    self.signOutButton.frame = NSMakeRect(width - 116, kNavHeight + 13, 92, 30);
    CGFloat gridY = kNavHeight + (compact ? 116.0 : kToolbarHeight);
    self.scrollView.frame = NSMakeRect(0, gridY, width, MAX(0.0, height - gridY));
    self.statusLabel.frame = NSMakeRect(0, gridY + 100, width, 24);
    self.loadingView.frame = self.bounds;
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

- (void)signOutClicked {
    if (self.onSignOut) self.onSignOut();
}

@end
