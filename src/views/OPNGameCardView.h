#import <Cocoa/Cocoa.h>
#include "../common/OPNGameTypes.h"

@interface OPNGameCardView : NSView

@property (nonatomic, readonly) OPN::GameInfo game;
@property (nonatomic, assign) int selectedVariantIndex;
@property (nonatomic, copy) void (^onPlay)();

- (instancetype)initWithFrame:(NSRect)frame game:(const OPN::GameInfo &)game;
- (void)selectVariantAtIndex:(int)index;

+ (NSSize)cardSize;
+ (CGFloat)imageHeight;
+ (CGFloat)infoHeight;

@end
