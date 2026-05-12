#pragma once

#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>

@class MTKView;

@interface OPNCoreAnimationCoordinator : NSObject

+ (instancetype)sharedCoordinator;
+ (CAMediaTimingFunction *)appleQuinticTimingFunction;

- (void)animateFocusForCardLayer:(CALayer *)cardLayer
                       glowLayer:(CALayer *)glowLayer
                         focused:(BOOL)focused
                      prominence:(CGFloat)prominence
                      accentColor:(NSColor *)accentColor;

- (void)animateCardLayer:(CALayer *)cardLayer
       metadataContainer:(NSView *)metadataContainer
         backgroundLayer:(CALayer *)backgroundLayer
                expanded:(BOOL)expanded
             accentColor:(NSColor *)accentColor;

- (void)springScrollClipView:(NSClipView *)clipView
                         toX:(CGFloat)targetX
                    velocity:(CGFloat)velocity;

- (void)configureMetalViewForProMotion:(MTKView *)metalView;

- (void)extractDominantColorFromImage:(CGImageRef)image
                              cacheKey:(NSString *)cacheKey
                            completion:(void (^)(NSColor *color))completion;

@end
