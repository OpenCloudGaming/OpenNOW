#import "OPNCoreAnimationCoordinator.h"

#import <CoreImage/CoreImage.h>
#import <MetalKit/MetalKit.h>

static CASpringAnimation *OPNSpringAnimation(NSString *keyPath,
                                             id fromValue,
                                             id toValue,
                                             CGFloat mass,
                                             CGFloat stiffness,
                                             CGFloat damping,
                                             CGFloat velocity) {
    CASpringAnimation *animation = [CASpringAnimation animationWithKeyPath:keyPath];
    animation.fromValue = fromValue;
    animation.toValue = toValue;
    animation.mass = mass;
    animation.stiffness = stiffness;
    animation.damping = damping;
    animation.initialVelocity = velocity;
    animation.duration = MIN(0.82, animation.settlingDuration);
    animation.removedOnCompletion = YES;
    return animation;
}

static NSValue *OPNCurrentTransformValue(CALayer *layer) {
    CALayer *presentationLayer = layer.presentationLayer;
    return [NSValue valueWithCATransform3D:(presentationLayer ? presentationLayer.transform : layer.transform)];
}

@interface OPNCoreAnimationCoordinator ()
@property (nonatomic, strong) NSCache<NSString *, NSColor *> *colorCache;
@property (nonatomic, strong) dispatch_queue_t colorQueue;
@end

@implementation OPNCoreAnimationCoordinator

+ (instancetype)sharedCoordinator {
    static OPNCoreAnimationCoordinator *coordinator;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        coordinator = [[OPNCoreAnimationCoordinator alloc] init];
    });
    return coordinator;
}

- (instancetype)init {
    self = [super init];
    if (self) {
        _colorCache = [[NSCache alloc] init];
        _colorCache.countLimit = 256;
        _colorQueue = dispatch_queue_create("com.opennow.artwork-color-extraction", DISPATCH_QUEUE_CONCURRENT);
    }
    return self;
}

+ (CAMediaTimingFunction *)appleQuinticTimingFunction {
    return [CAMediaTimingFunction functionWithControlPoints:0.22 :1.0 :0.36 :1.0];
}

- (void)animateFocusForCardLayer:(CALayer *)cardLayer
                       glowLayer:(CALayer *)glowLayer
                         focused:(BOOL)focused
                      prominence:(CGFloat)prominence
                      accentColor:(NSColor *)accentColor {
    if (!cardLayer) return;

    NSColor *resolvedAccent = accentColor ?: NSColor.whiteColor;
    CGFloat focusAmount = focused ? 1.0 : MAX(0.0, MIN(1.0, prominence));
    CGFloat scale = 1.0 + 0.075 * focusAmount;

    CATransform3D targetTransform = CATransform3DIdentity;
    targetTransform.m34 = -1.0 / 760.0;
    targetTransform = CATransform3DTranslate(targetTransform, 0.0, -10.0 * focusAmount, 42.0 * focusAmount);
    targetTransform = CATransform3DScale(targetTransform, scale, scale, 1.0);
    targetTransform = CATransform3DRotate(targetTransform, -0.030 * focusAmount, 1.0, 0.0, 0.0);
    NSValue *currentTransform = OPNCurrentTransformValue(cardLayer);

    [CATransaction begin];
    [CATransaction setDisableActions:YES];

    cardLayer.transform = targetTransform;
    cardLayer.zPosition = 100.0 * focusAmount;
    cardLayer.shadowColor = resolvedAccent.CGColor;
    cardLayer.shadowOpacity = 0.24 + 0.34 * focusAmount;
    cardLayer.shadowRadius = 18.0 + 34.0 * focusAmount;
    cardLayer.shadowOffset = CGSizeMake(0.0, 12.0 + 16.0 * focusAmount);

    CASpringAnimation *transformSpring = OPNSpringAnimation(@"transform",
                                                            currentTransform,
                                                            [NSValue valueWithCATransform3D:targetTransform],
                                                            0.78,
                                                            560.0,
                                                            40.0,
                                                            0.0);
    [cardLayer addAnimation:transformSpring forKey:@"opn.focus.transform"];

    if (glowLayer) {
        CALayer *presentationGlow = glowLayer.presentationLayer;
        CGFloat targetOpacity = focused ? 0.74 : 0.0;
        NSNumber *fromOpacity = @((presentationGlow ? presentationGlow.opacity : glowLayer.opacity));
        glowLayer.backgroundColor = resolvedAccent.CGColor;
        glowLayer.opacity = targetOpacity;
        glowLayer.shadowColor = resolvedAccent.CGColor;
        glowLayer.shadowOpacity = targetOpacity;
        glowLayer.shadowRadius = 24.0 + 18.0 * focusAmount;

        CASpringAnimation *opacitySpring = OPNSpringAnimation(@"opacity",
                                                              fromOpacity,
                                                              @(targetOpacity),
                                                              0.70,
                                                              500.0,
                                                              38.0,
                                                              0.0);
        [glowLayer addAnimation:opacitySpring forKey:@"opn.focus.glow"];
    }

    [CATransaction commit];
}

- (void)animateCardLayer:(CALayer *)cardLayer
       metadataContainer:(NSView *)metadataContainer
         backgroundLayer:(CALayer *)backgroundLayer
                expanded:(BOOL)expanded
             accentColor:(NSColor *)accentColor {
    if (!cardLayer || !metadataContainer.layer || !backgroundLayer) return;

    NSColor *resolvedAccent = accentColor ?: NSColor.whiteColor;
    CGFloat scale = expanded ? 1.18 : 1.0;
    CGFloat blurRadius = expanded ? 22.0 : 0.0;
    CGFloat metadataOpacity = expanded ? 0.28 : 1.0;

    CATransform3D targetTransform = CATransform3DIdentity;
    targetTransform.m34 = -1.0 / 900.0;
    targetTransform = CATransform3DTranslate(targetTransform, 0.0, expanded ? -18.0 : 0.0, expanded ? 80.0 : 0.0);
    targetTransform = CATransform3DScale(targetTransform, scale, scale, 1.0);
    NSValue *currentTransform = OPNCurrentTransformValue(cardLayer);

    CIFilter *blurFilter = [CIFilter filterWithName:@"CIGaussianBlur"];
    if (!blurFilter) return;
    blurFilter.name = @"opnMetadataBlur";
    [blurFilter setDefaults];
    [blurFilter setValue:@(blurRadius) forKey:kCIInputRadiusKey];

    [CATransaction begin];
    [CATransaction setAnimationDuration:0.42];
    [CATransaction setAnimationTimingFunction:[OPNCoreAnimationCoordinator appleQuinticTimingFunction]];

    cardLayer.transform = targetTransform;
    cardLayer.shadowColor = resolvedAccent.CGColor;
    cardLayer.shadowOpacity = expanded ? 0.62 : 0.34;
    cardLayer.shadowRadius = expanded ? 64.0 : 22.0;
    cardLayer.shadowOffset = CGSizeMake(0.0, expanded ? 34.0 : 14.0);
    metadataContainer.layer.opacity = metadataOpacity;
    backgroundLayer.filters = @[blurFilter];

    CABasicAnimation *blurAnimation = [CABasicAnimation animationWithKeyPath:@"filters.opnMetadataBlur.inputRadius"];
    blurAnimation.fromValue = @(!expanded ? 22.0 : 0.0);
    blurAnimation.toValue = @(blurRadius);
    blurAnimation.duration = 0.42;
    blurAnimation.timingFunction = [OPNCoreAnimationCoordinator appleQuinticTimingFunction];
    [backgroundLayer addAnimation:blurAnimation forKey:@"opn.metadata.blur"];

    CASpringAnimation *transformSpring = OPNSpringAnimation(@"transform",
                                                            currentTransform,
                                                            [NSValue valueWithCATransform3D:targetTransform],
                                                            0.85,
                                                            360.0,
                                                            34.0,
                                                            0.0);
    [cardLayer addAnimation:transformSpring forKey:@"opn.expand.transform"];

    [CATransaction commit];
}

- (void)springScrollClipView:(NSClipView *)clipView
                         toX:(CGFloat)targetX
                    velocity:(CGFloat)velocity {
    if (!clipView) return;

    clipView.wantsLayer = YES;
    NSRect currentBounds = clipView.bounds;
    CGFloat currentX = currentBounds.origin.x;
    CGFloat distance = targetX - currentX;
    CGFloat normalizedVelocity = fabs(distance) > 1.0 ? velocity / distance : 0.0;
    NSRect targetBounds = currentBounds;
    targetBounds.origin.x = targetX;

    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    clipView.bounds = targetBounds;

    CASpringAnimation *spring = OPNSpringAnimation(@"bounds.origin.x",
                                                   @(currentX),
                                                   @(targetX),
                                                   1.0,
                                                   220.0,
                                                   29.0,
                                                   normalizedVelocity);
    [clipView.layer addAnimation:spring forKey:@"opn.carousel.snap"];
    [CATransaction commit];

    [clipView scrollToPoint:NSMakePoint(targetX, currentBounds.origin.y)];
    [clipView.enclosingScrollView reflectScrolledClipView:clipView];
}

- (void)configureMetalViewForProMotion:(MTKView *)metalView {
    if (!metalView) return;

    NSInteger maximumFramesPerSecond = metalView.window.screen.maximumFramesPerSecond;
    if (maximumFramesPerSecond <= 0) maximumFramesPerSecond = 60;
    metalView.preferredFramesPerSecond = MIN(120, maximumFramesPerSecond);
    metalView.enableSetNeedsDisplay = NO;
    metalView.paused = NO;
    metalView.framebufferOnly = YES;
}

- (void)extractDominantColorFromImage:(CGImageRef)image
                              cacheKey:(NSString *)cacheKey
                            completion:(void (^)(NSColor *color))completion {
    if (!image || cacheKey.length == 0 || !completion) return;

    NSColor *cachedColor = [self.colorCache objectForKey:cacheKey];
    if (cachedColor) {
        dispatch_async(dispatch_get_main_queue(), ^{
            completion(cachedColor);
        });
        return;
    }

    CGImageRef retainedImage = CGImageRetain(image);
    dispatch_async(self.colorQueue, ^{
        const size_t width = 32;
        const size_t height = 32;
        const size_t bytesPerPixel = 4;
        const size_t bytesPerRow = width * bytesPerPixel;
        NSMutableData *pixelData = [NSMutableData dataWithLength:height * bytesPerRow];
        CGColorSpaceRef colorSpace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        CGBitmapInfo bitmapInfo = (CGBitmapInfo)kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big;
        CGContextRef context = CGBitmapContextCreate(pixelData.mutableBytes,
                                                     width,
                                                     height,
                                                     8,
                                                     bytesPerRow,
                                                     colorSpace,
                                                     bitmapInfo);

        if (!context || !colorSpace) {
            if (context) CGContextRelease(context);
            if (colorSpace) CGColorSpaceRelease(colorSpace);
            CGImageRelease(retainedImage);
            return;
        }

        CGContextSetInterpolationQuality(context, kCGInterpolationMedium);
        CGContextDrawImage(context, CGRectMake(0.0, 0.0, width, height), retainedImage);

        const uint8_t *pixels = (const uint8_t *)pixelData.bytes;
        CGFloat redTotal = 0.0;
        CGFloat greenTotal = 0.0;
        CGFloat blueTotal = 0.0;
        CGFloat weightTotal = 0.0;

        for (size_t index = 0; index < width * height; index++) {
            const uint8_t *pixel = pixels + index * bytesPerPixel;
            CGFloat red = pixel[0] / 255.0;
            CGFloat green = pixel[1] / 255.0;
            CGFloat blue = pixel[2] / 255.0;
            CGFloat alpha = pixel[3] / 255.0;
            CGFloat maxChannel = MAX(red, MAX(green, blue));
            CGFloat minChannel = MIN(red, MIN(green, blue));
            CGFloat saturation = maxChannel <= 0.0 ? 0.0 : (maxChannel - minChannel) / maxChannel;
            CGFloat luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
            CGFloat highlightPenalty = 1.0 - MAX(0.0, luminance - 0.92);
            CGFloat weight = alpha * (0.25 + saturation) * (0.35 + luminance) * highlightPenalty;

            redTotal += red * weight;
            greenTotal += green * weight;
            blueTotal += blue * weight;
            weightTotal += weight;
        }

        CGContextRelease(context);
        CGColorSpaceRelease(colorSpace);
        CGImageRelease(retainedImage);

        if (weightTotal <= 0.001) weightTotal = 1.0;

        NSColor *color = [NSColor colorWithCalibratedRed:redTotal / weightTotal
                                                   green:greenTotal / weightTotal
                                                    blue:blueTotal / weightTotal
                                                   alpha:1.0];
        [self.colorCache setObject:color forKey:cacheKey];

        dispatch_async(dispatch_get_main_queue(), ^{
            completion(color);
        });
    });
}

@end
