#import <Cocoa/Cocoa.h>
#include "../common/OPNGameTypes.h"

@interface OPNGameCardView : NSView

@property (nonatomic, readonly) OPN::GameInfo game;
@property (nonatomic, strong, readonly) NSColor *artworkAccentColor;
@property (nonatomic, assign) int selectedVariantIndex;
@property (nonatomic, assign, getter=isControllerFocused) BOOL controllerFocused;
@property (nonatomic, copy) void (^onPlay)();
@property (nonatomic, copy) void (^onArtworkAccentColorChanged)(NSColor *color);

- (instancetype)initWithFrame:(NSRect)frame game:(const OPN::GameInfo &)game;
- (void)updateGame:(const OPN::GameInfo &)game;
- (void)selectVariantAtIndex:(int)index;

+ (NSSize)cardSize;
+ (CGFloat)imageHeight;
+ (CGFloat)infoHeight;

@end
