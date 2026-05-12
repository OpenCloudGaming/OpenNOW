#pragma once

#import <Cocoa/Cocoa.h>

@interface OPNCarouselFocusLayout : NSCollectionViewLayout

@property (nonatomic, assign) NSSize itemSize;
@property (nonatomic, assign) CGFloat itemSpacing;
@property (nonatomic, assign) CGFloat sideInset;
@property (nonatomic, assign) CGFloat focusScale;
@property (nonatomic, assign) CGFloat minimumScale;
@property (nonatomic, assign) CGFloat minimumAlpha;

@end
