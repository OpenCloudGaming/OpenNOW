#import "OPNCarouselFocusLayout.h"

static CGFloat OPNSmoothstep(CGFloat value) {
    CGFloat x = MAX(0.0, MIN(1.0, value));
    return x * x * (3.0 - 2.0 * x);
}

@interface OPNCarouselFocusLayout ()
@property (nonatomic, strong) NSMutableArray<NSCollectionViewLayoutAttributes *> *cachedAttributes;
@property (nonatomic, assign) NSSize cachedContentSize;
@end

@implementation OPNCarouselFocusLayout

- (instancetype)init {
    self = [super init];
    if (self) {
        _itemSize = NSMakeSize(164.0, 164.0);
        _itemSpacing = 26.0;
        _sideInset = 64.0;
        _focusScale = 1.14;
        _minimumScale = 0.88;
        _minimumAlpha = 0.48;
        _cachedAttributes = [NSMutableArray array];
        _cachedContentSize = NSZeroSize;
    }
    return self;
}

- (void)prepareLayout {
    [super prepareLayout];

    NSCollectionView *collectionView = self.collectionView;
    [self.cachedAttributes removeAllObjects];

    if (!collectionView) {
        self.cachedContentSize = NSZeroSize;
        return;
    }

    NSInteger sectionCount = collectionView.numberOfSections;
    NSInteger itemCount = sectionCount > 0 ? [collectionView numberOfItemsInSection:0] : 0;
    NSRect bounds = collectionView.bounds;
    CGFloat centerY = floor(NSMidY(bounds));
    CGFloat x = self.sideInset;

    for (NSInteger item = 0; item < itemCount; item++) {
        NSIndexPath *indexPath = [NSIndexPath indexPathForItem:item inSection:0];
        NSCollectionViewLayoutAttributes *attributes = [NSCollectionViewLayoutAttributes layoutAttributesForItemWithIndexPath:indexPath];
        attributes.frame = NSMakeRect(x, centerY - self.itemSize.height * 0.5, self.itemSize.width, self.itemSize.height);
        attributes.zIndex = item;
        [self.cachedAttributes addObject:attributes];
        x += self.itemSize.width + self.itemSpacing;
    }

    CGFloat contentWidth = self.sideInset + itemCount * self.itemSize.width + MAX(0, itemCount - 1) * self.itemSpacing + self.sideInset;
    self.cachedContentSize = NSMakeSize(MAX(NSWidth(bounds), contentWidth), MAX(NSHeight(bounds), self.itemSize.height));
}

- (NSSize)collectionViewContentSize {
    return self.cachedContentSize;
}

- (NSArray<NSCollectionViewLayoutAttributes *> *)layoutAttributesForElementsInRect:(NSRect)rect {
    NSCollectionView *collectionView = self.collectionView;
    if (!collectionView) return @[];

    NSClipView *clipView = collectionView.enclosingScrollView.contentView;
    NSRect visibleRect = clipView ? clipView.bounds : collectionView.bounds;
    CGFloat viewportCenterX = NSMidX(visibleRect);
    CGFloat influenceRadius = MAX(1.0, NSWidth(visibleRect) * 0.48);
    NSRect expandedRect = NSInsetRect(rect, -self.itemSize.width, -self.itemSize.height);

    NSMutableArray<NSCollectionViewLayoutAttributes *> *visibleAttributes = [NSMutableArray array];

    for (NSCollectionViewLayoutAttributes *baseAttributes in self.cachedAttributes) {
        if (!NSIntersectsRect(baseAttributes.frame, expandedRect)) continue;

        NSCollectionViewLayoutAttributes *attributes = [baseAttributes copy];
        CGFloat distance = fabs(NSMidX(baseAttributes.frame) - viewportCenterX);
        CGFloat proximity = 1.0 - MIN(1.0, distance / influenceRadius);
        CGFloat prominence = OPNSmoothstep(proximity);
        CGFloat scale = self.minimumScale + (self.focusScale - self.minimumScale) * prominence;
        CGFloat width = self.itemSize.width * scale;
        CGFloat height = self.itemSize.height * scale;
        CGFloat midX = NSMidX(baseAttributes.frame);
        CGFloat midY = NSMidY(baseAttributes.frame);

        attributes.frame = NSMakeRect(floor(midX - width * 0.5), floor(midY - height * 0.5), width, height);
        attributes.alpha = self.minimumAlpha + (1.0 - self.minimumAlpha) * prominence;
        attributes.zIndex = (NSInteger)lrint(prominence * 1000.0);
        [visibleAttributes addObject:attributes];
    }

    return visibleAttributes;
}

- (NSCollectionViewLayoutAttributes *)layoutAttributesForItemAtIndexPath:(NSIndexPath *)indexPath {
    if (indexPath.item < 0 || indexPath.item >= (NSInteger)self.cachedAttributes.count) return nil;
    return [self.cachedAttributes[(NSUInteger)indexPath.item] copy];
}

- (BOOL)shouldInvalidateLayoutForBoundsChange:(NSRect)newBounds {
    (void)newBounds;
    return YES;
}

- (NSPoint)targetContentOffsetForProposedContentOffset:(NSPoint)proposedContentOffset
                                 withScrollingVelocity:(NSPoint)velocity {
    (void)velocity;
    NSCollectionView *collectionView = self.collectionView;
    if (!collectionView || self.cachedAttributes.count == 0) return proposedContentOffset;

    NSRect bounds = collectionView.bounds;
    CGFloat proposedCenterX = proposedContentOffset.x + NSWidth(bounds) * 0.5;
    CGFloat nearestDistance = CGFLOAT_MAX;
    CGFloat nearestCenterX = proposedCenterX;

    for (NSCollectionViewLayoutAttributes *attributes in self.cachedAttributes) {
        CGFloat distance = fabs(NSMidX(attributes.frame) - proposedCenterX);
        if (distance < nearestDistance) {
            nearestDistance = distance;
            nearestCenterX = NSMidX(attributes.frame);
        }
    }

    CGFloat targetX = nearestCenterX - NSWidth(bounds) * 0.5;
    targetX = MAX(0.0, MIN(targetX, MAX(0.0, self.cachedContentSize.width - NSWidth(bounds))));
    return NSMakePoint(targetX, proposedContentOffset.y);
}

@end
