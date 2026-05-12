#import "OPNLoadingView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#import <AVKit/AVKit.h>
#include <QuartzCore/QuartzCore.h>
#include <cmath>

@interface OPNLoadingView ()
@property (nonatomic, strong) CALayer *panelLayer;
@property (nonatomic, strong) CAGradientLayer *sweepLayer;
@property (nonatomic, strong) CAShapeLayer *orbitLayer;
@property (nonatomic, strong) CAShapeLayer *innerOrbitLayer;
@property (nonatomic, strong) CALayer *coreLayer;
@property (nonatomic, strong) CALayer *sparkLayer;
@property (nonatomic, strong) NSMutableArray<CALayer *> *barLayers;
@property (nonatomic, strong) NSMutableArray<CALayer *> *dotLayers;
@property (nonatomic, strong) NSMutableArray<CALayer *> *stepIndicatorLayers;
@property (nonatomic, strong, readwrite) NSTextField *messageLabel;
@property (nonatomic, strong) NSTextField *queuePositionLabel;
@property (nonatomic, strong) NSView *adContainerView;
@property (nonatomic, strong) NSTextField *adChipLabel;
@property (nonatomic, strong) NSTextField *adTitleLabel;
@property (nonatomic, strong) NSTextField *adMessageLabel;
@property (nonatomic, strong) AVPlayerView *adPlayerView;
@property (nonatomic, strong) AVPlayer *adPlayer;
@property (nonatomic, strong) id adTimeObserver;
@property (nonatomic, strong) NSTimer *adFallbackTimer;
@property (nonatomic, copy) NSString *activeAdId;
@property (nonatomic, assign) NSInteger activeAdDurationMs;
@property (nonatomic, strong) NSDate *adStartedAt;
@property (nonatomic, assign) BOOL adVisible;
@property (nonatomic, assign) BOOL adStartReported;
@property (nonatomic, assign) BOOL adFinishReported;
- (void)applyAccentColors;
@end

@implementation OPNLoadingView

- (instancetype)initWithFrame:(NSRect)frame message:(NSString *)message {
    self = [super initWithFrame:frame];
    if (self) {
        _message = [message copy] ?: @"Loading...";
        _steps = @[];
        _currentStepIndex = -1;
        _queuePosition = 0;
        _barLayers = [NSMutableArray array];
        _dotLayers = [NSMutableArray array];
        _stepIndicatorLayers = [NSMutableArray array];
        self.wantsLayer = YES;
        self.layer.backgroundColor = OpnColor(OPN::kBackground, 0.98).CGColor;
        self.layer.masksToBounds = YES;

        _panelLayer = [CALayer layer];
        _panelLayer.backgroundColor = OpnColor(OPN::kSurfaceRaised, 0.86).CGColor;
        _panelLayer.cornerRadius = 28.0;
        _panelLayer.borderWidth = 1.0;
        _panelLayer.borderColor = OpnColor(0xFFFFFF, 0.11).CGColor;
        _panelLayer.shadowColor = NSColor.blackColor.CGColor;
        _panelLayer.shadowOpacity = 0.32;
        _panelLayer.shadowRadius = 32.0;
        _panelLayer.shadowOffset = CGSizeMake(0.0, 18.0);
        [self.layer addSublayer:_panelLayer];

        _sweepLayer = [CAGradientLayer layer];
        _sweepLayer.locations = @[@0.0, @0.42, @0.50, @1.0];
        _sweepLayer.startPoint = CGPointMake(0.0, 0.5);
        _sweepLayer.endPoint = CGPointMake(1.0, 0.5);
        [self.layer addSublayer:_sweepLayer];

        _orbitLayer = [CAShapeLayer layer];
        _orbitLayer.fillColor = NSColor.clearColor.CGColor;
        _orbitLayer.lineWidth = 2.0;
        _orbitLayer.lineCap = kCALineCapRound;
        _orbitLayer.strokeStart = 0.04;
        _orbitLayer.strokeEnd = 0.72;
        [self.layer addSublayer:_orbitLayer];

        _innerOrbitLayer = [CAShapeLayer layer];
        _innerOrbitLayer.fillColor = NSColor.clearColor.CGColor;
        _innerOrbitLayer.strokeColor = OpnColor(0xFFFFFF, 0.26).CGColor;
        _innerOrbitLayer.lineWidth = 1.0;
        _innerOrbitLayer.lineDashPattern = @[@3, @7];
        [self.layer addSublayer:_innerOrbitLayer];

        _coreLayer = [CALayer layer];
        _coreLayer.shadowOpacity = 0.86;
        _coreLayer.shadowRadius = 14.0;
        _coreLayer.shadowOffset = CGSizeZero;
        [self.layer addSublayer:_coreLayer];

        _sparkLayer = [CALayer layer];
        _sparkLayer.shadowOpacity = 0.9;
        _sparkLayer.shadowRadius = 10.0;
        _sparkLayer.shadowOffset = CGSizeZero;
        [self.layer addSublayer:_sparkLayer];

        for (NSUInteger i = 0; i < 4; i++) {
            CALayer *bar = [CALayer layer];
            bar.cornerRadius = 2.0;
            [self.layer addSublayer:bar];
            [_barLayers addObject:bar];
        }

        for (NSUInteger i = 0; i < 5; i++) {
            CALayer *dot = [CALayer layer];
            dot.cornerRadius = 2.5;
            dot.shadowOpacity = 0.36;
            dot.shadowRadius = 5.0;
            dot.shadowOffset = CGSizeZero;
            [self.layer addSublayer:dot];
            [_dotLayers addObject:dot];
        }

        _messageLabel = OpnLabel(_message, NSZeroRect, 14.0, OpnColor(OPN::kTextPrimary), NSFontWeightMedium, NSTextAlignmentCenter);
        _messageLabel.maximumNumberOfLines = 2;
        [self addSubview:_messageLabel];

        _queuePositionLabel = OpnLabel(@"", NSZeroRect, 13.0, OpnColor(OPN::kBrandGreen), NSFontWeightSemibold, NSTextAlignmentCenter);
        _queuePositionLabel.hidden = YES;
        _queuePositionLabel.wantsLayer = YES;
        _queuePositionLabel.layer.backgroundColor = OpnColor(0x0A1624, 0.72).CGColor;
        _queuePositionLabel.layer.cornerRadius = 12.0;
        _queuePositionLabel.layer.borderWidth = 1.0;
        _queuePositionLabel.layer.borderColor = OpnColor(OPN::kBrandGreen, 0.22).CGColor;
        [self addSubview:_queuePositionLabel];

        _adContainerView = [[NSView alloc] initWithFrame:NSZeroRect];
        _adContainerView.wantsLayer = YES;
        _adContainerView.layer.backgroundColor = OpnColor(0x05070A, 0.48).CGColor;
        _adContainerView.layer.cornerRadius = 22.0;
        _adContainerView.layer.borderWidth = 1.0;
        _adContainerView.layer.borderColor = OpnColor(0xFFFFFF, 0.12).CGColor;
        _adContainerView.hidden = YES;
        [self addSubview:_adContainerView];

        _adChipLabel = OpnLabel(@"Ad Queue", NSZeroRect, 12.0, OpnColor(OPN::kBrandGreen), NSFontWeightSemibold, NSTextAlignmentLeft);
        [_adContainerView addSubview:_adChipLabel];
        _adTitleLabel = OpnLabel(@"Ad playback required", NSZeroRect, 20.0, OpnColor(OPN::kTextPrimary), NSFontWeightBold, NSTextAlignmentLeft);
        _adTitleLabel.maximumNumberOfLines = 2;
        [_adContainerView addSubview:_adTitleLabel];
        _adMessageLabel = OpnLabel(@"Finish this ad to keep your free-tier session moving.", NSZeroRect, 13.0, OpnColor(OPN::kTextSecondary), NSFontWeightRegular, NSTextAlignmentLeft);
        _adMessageLabel.maximumNumberOfLines = 3;
        [_adContainerView addSubview:_adMessageLabel];
        _adPlayerView = [[AVPlayerView alloc] initWithFrame:NSZeroRect];
        _adPlayerView.controlsStyle = AVPlayerViewControlsStyleNone;
        _adPlayerView.videoGravity = AVLayerVideoGravityResizeAspect;
        _adPlayerView.hidden = YES;
        [_adContainerView addSubview:_adPlayerView];
        [self applyAccentColors];
        [[NSNotificationCenter defaultCenter] addObserver:self
                                                 selector:@selector(interfacePreferencesChanged:)
                                                     name:OPNInterfacePreferencesDidChangeNotification
                                                   object:nil];
    }
    return self;
}

- (void)interfacePreferencesChanged:(NSNotification *)notification {
    (void)notification;
    [self applyAccentColors];
}

- (void)applyAccentColors {
    self.sweepLayer.colors = @[(id)OpnColor(OPN::kBrandGreen, 0.0).CGColor,
                               (id)OpnColor(OPN::kBrandGreen, 0.28).CGColor,
                               (id)OpnColor(OPN::kBrandGreenHover, 0.42).CGColor,
                               (id)OpnColor(OPN::kBrandGreen, 0.0).CGColor];
    self.orbitLayer.strokeColor = OpnColor(OPN::kBrandGreen, 0.78).CGColor;
    self.coreLayer.backgroundColor = OpnColor(OPN::kBrandGreenHover, 0.92).CGColor;
    self.coreLayer.shadowColor = OpnColor(OPN::kBrandGreen, 1.0).CGColor;
    self.sparkLayer.backgroundColor = OpnColor(OPN::kBrandGreen, 0.92).CGColor;
    self.sparkLayer.shadowColor = OpnColor(OPN::kBrandGreen, 1.0).CGColor;
    for (CALayer *bar in self.barLayers) {
        bar.backgroundColor = OpnColor(OPN::kBrandGreen, 0.54).CGColor;
    }
    for (CALayer *dot in self.dotLayers) {
        dot.backgroundColor = OpnColor(OPN::kBrandGreenHover, 0.74).CGColor;
        dot.shadowColor = OpnColor(OPN::kBrandGreen, 1.0).CGColor;
    }
    self.queuePositionLabel.textColor = OpnColor(OPN::kBrandGreen);
    self.queuePositionLabel.layer.borderColor = OpnColor(OPN::kBrandGreen, 0.22).CGColor;
    self.adChipLabel.textColor = OpnColor(OPN::kBrandGreen);
    [self restyleStepIndicators];
}

- (BOOL)isFlipped { return YES; }

- (void)setMessage:(NSString *)message {
    _message = [message copy] ?: @"Loading...";
    self.messageLabel.stringValue = _message;
}

- (void)setSteps:(NSArray<NSString *> *)steps {
    _steps = [steps copy] ?: @[];
    [self rebuildStepIndicators];
}

- (void)setCurrentStepIndex:(NSInteger)currentStepIndex {
    _currentStepIndex = currentStepIndex;
    [self restyleStepIndicators];
}

- (void)setSteps:(NSArray<NSString *> *)steps currentStepIndex:(NSInteger)currentStepIndex {
    _steps = [steps copy] ?: @[];
    _currentStepIndex = currentStepIndex;
    [self rebuildStepIndicators];
}

- (void)advanceToStep:(NSInteger)stepIndex message:(NSString *)message {
    self.currentStepIndex = stepIndex;
    self.message = message;
}

- (void)setQueuePosition:(NSInteger)queuePosition {
    _queuePosition = MAX(0, queuePosition);
    if (_queuePosition > 0) {
        self.queuePositionLabel.stringValue = [NSString stringWithFormat:@"Queue Position #%ld", (long)_queuePosition];
        self.queuePositionLabel.hidden = NO;
    } else {
        self.queuePositionLabel.stringValue = @"";
        self.queuePositionLabel.hidden = YES;
    }
    [self setNeedsLayout:YES];
}

- (void)updateQueuePosition:(NSInteger)queuePosition {
    self.queuePosition = queuePosition;
}

- (void)setLoadingChromeHidden:(BOOL)hidden {
    self.sweepLayer.hidden = hidden;
    self.orbitLayer.hidden = hidden;
    self.innerOrbitLayer.hidden = hidden;
    self.coreLayer.hidden = hidden;
    self.sparkLayer.hidden = hidden;
    for (CALayer *bar in self.barLayers) bar.hidden = hidden;
    for (CALayer *dot in self.dotLayers) dot.hidden = hidden;
}

- (NSInteger)currentAdWatchedTimeInMs {
    if (self.adPlayer.currentItem) {
        CMTime current = self.adPlayer.currentItem.currentTime;
        if (CMTIME_IS_NUMERIC(current)) {
            return (NSInteger)llround(CMTimeGetSeconds(current) * 1000.0);
        }
    }
    if (!self.adStartedAt) return 0;
    return MAX(0, (NSInteger)llround(-self.adStartedAt.timeIntervalSinceNow * 1000.0));
}

- (void)reportAdAction:(NSString *)action cancelReason:(NSString *)cancelReason {
    if (!self.activeAdId.length || !self.adPlaybackEventHandler) return;
    if ([action isEqualToString:@"start"]) {
        if (self.adStartReported) return;
        self.adStartReported = YES;
    }
    if ([action isEqualToString:@"finish"]) {
        if (self.adFinishReported) return;
        self.adFinishReported = YES;
    }
    self.adPlaybackEventHandler(self.activeAdId, action, [self currentAdWatchedTimeInMs], 0, cancelReason ?: @"");
}

- (void)handleAdFinished:(NSNotification *)notification {
    if (notification.object != self.adPlayer.currentItem) return;
    [self reportAdAction:@"finish" cancelReason:@""];
}

- (void)handleFallbackAdTimer:(NSTimer *)timer {
    (void)timer;
    [self reportAdAction:@"finish" cancelReason:@""];
}

- (void)removeAdTimeObserver {
    if (!self.adPlayer || !self.adTimeObserver) return;
    [self.adPlayer removeTimeObserver:self.adTimeObserver];
    self.adTimeObserver = nil;
}

- (void)updateAdState:(const OPN::SessionAdState &)adState {
    if (!adState.isAdsRequired || adState.sessionAds.empty()) {
        [self clearAdPresentation];
        return;
    }

    const OPN::SessionAdInfo &ad = adState.sessionAds.front();
    NSString *adId = ad.adId.empty() ? @"ad" : [NSString stringWithUTF8String:ad.adId.c_str()];
    if (!adId) adId = @"ad";
    NSString *title = ad.title.empty() ? @"Ad playback required" : [NSString stringWithUTF8String:ad.title.c_str()];
    NSString *message = adState.message.empty() ? @"Finish this ad to keep your free-tier session moving." : [NSString stringWithUTF8String:adState.message.c_str()];
    NSString *mediaUrl = ad.mediaUrl.empty() ? @"" : [NSString stringWithUTF8String:ad.mediaUrl.c_str()];
    NSInteger durationMs = ad.durationMs > 0 ? ad.durationMs : (ad.adLengthInSeconds > 0 ? ad.adLengthInSeconds * 1000 : 30000);

    BOOL sameAd = self.activeAdId && [self.activeAdId isEqualToString:adId];
    self.adVisible = YES;
    self.adContainerView.hidden = NO;
    self.adTitleLabel.stringValue = title ?: @"Ad playback required";
    self.adMessageLabel.stringValue = message ?: @"Finish this ad to keep your free-tier session moving.";
    [self setLoadingChromeHidden:YES];
    [self stopAnimating];

    if (sameAd) {
        [self setNeedsLayout:YES];
        return;
    }

    [self.adFallbackTimer invalidate];
    self.adFallbackTimer = nil;
    [[NSNotificationCenter defaultCenter] removeObserver:self name:AVPlayerItemDidPlayToEndTimeNotification object:nil];
    [self removeAdTimeObserver];
    self.activeAdId = adId;
    self.activeAdDurationMs = durationMs;
    self.adStartedAt = [NSDate date];
    self.adStartReported = NO;
    self.adFinishReported = NO;

    if (mediaUrl.length > 0) {
        NSURL *url = [NSURL URLWithString:mediaUrl];
        if (url) {
            AVPlayerItem *item = [AVPlayerItem playerItemWithURL:url];
            self.adPlayer = [AVPlayer playerWithPlayerItem:item];
            self.adPlayer.muted = NO;
            self.adPlayerView.player = self.adPlayer;
            self.adPlayerView.hidden = NO;
            [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(handleAdFinished:) name:AVPlayerItemDidPlayToEndTimeNotification object:item];
            __weak __typeof__(self) weakSelf = self;
            self.adTimeObserver = [self.adPlayer addPeriodicTimeObserverForInterval:CMTimeMakeWithSeconds(0.25, NSEC_PER_SEC)
                                                                              queue:dispatch_get_main_queue()
                                                                         usingBlock:^(CMTime time) {
                __typeof__(self) strongSelf = weakSelf;
                if (!strongSelf || strongSelf.adStartReported) return;
                if (CMTIME_IS_NUMERIC(time) && CMTimeGetSeconds(time) > 0.0) {
                    [strongSelf reportAdAction:@"start" cancelReason:@""];
                }
            }];
            [self.adPlayer play];
        }
    } else {
        self.adPlayer = nil;
        self.adPlayerView.player = nil;
        self.adPlayerView.hidden = YES;
        [self reportAdAction:@"start" cancelReason:@""];
        self.adFallbackTimer = [NSTimer scheduledTimerWithTimeInterval:MAX(5.0, (NSTimeInterval)durationMs / 1000.0)
                                                                 target:self
                                                               selector:@selector(handleFallbackAdTimer:)
                                                               userInfo:nil
                                                                repeats:NO];
    }
    [self setNeedsLayout:YES];
}

- (void)clearAdPresentation {
    if (!self.adVisible) return;
    [self.adFallbackTimer invalidate];
    self.adFallbackTimer = nil;
    [[NSNotificationCenter defaultCenter] removeObserver:self name:AVPlayerItemDidPlayToEndTimeNotification object:nil];
    [self removeAdTimeObserver];
    [self.adPlayer pause];
    self.adPlayer = nil;
    self.adPlayerView.player = nil;
    self.adPlayerView.hidden = YES;
    self.adContainerView.hidden = YES;
    self.activeAdId = nil;
    self.adVisible = NO;
    [self setLoadingChromeHidden:NO];
    if (self.window) [self startAnimating];
    [self setNeedsLayout:YES];
}

- (void)rebuildStepIndicators {
    for (CALayer *indicator in self.stepIndicatorLayers) {
        [indicator removeFromSuperlayer];
    }
    [self.stepIndicatorLayers removeAllObjects];
    for (NSUInteger i = 0; i < self.steps.count; i++) {
        CALayer *indicator = [CALayer layer];
        indicator.cornerRadius = 1.5;
        [self.layer addSublayer:indicator];
        [self.stepIndicatorLayers addObject:indicator];
    }
    [self restyleStepIndicators];
    [self setNeedsLayout:YES];
}

- (void)restyleStepIndicators {
    for (NSUInteger i = 0; i < self.stepIndicatorLayers.count; i++) {
        CALayer *indicator = self.stepIndicatorLayers[i];
        BOOL completed = (NSInteger)i < self.currentStepIndex;
        BOOL current = (NSInteger)i == self.currentStepIndex;
        indicator.backgroundColor = current
            ? OpnColor(OPN::kBrandGreenHover, 0.96).CGColor
            : (completed ? OpnColor(OPN::kBrandGreen, 0.54).CGColor : OpnColor(0xFFFFFF, 0.16).CGColor);
    }
}

- (void)layout {
    [super layout];
    CGFloat width = NSWidth(self.bounds);
    CGFloat height = NSHeight(self.bounds);
    BOOL hasSteps = self.stepIndicatorLayers.count > 0;
    CGFloat panelWidth = MIN(hasSteps ? 540.0 : 460.0, MAX(320.0, width - 48.0));
    CGFloat panelHeight = self.adVisible ? 460.0 : (hasSteps ? 318.0 : 276.0);
    panelHeight = MIN(panelHeight, MAX(252.0, height - 48.0));
    CGFloat panelX = floor((width - panelWidth) * 0.5);
    CGFloat panelY = floor((height - panelHeight) * 0.5);
    NSRect panelRect = NSMakeRect(panelX, panelY, panelWidth, panelHeight);
    CGFloat centerX = NSMidX(panelRect);
    CGFloat orbitSize = MIN(108.0, MAX(80.0, panelWidth * 0.22));
    CGFloat orbitY = panelY + 40.0;
    NSRect orbitRect = NSMakeRect(centerX - orbitSize * 0.5, orbitY, orbitSize, orbitSize);

    self.panelLayer.frame = panelRect;
    self.panelLayer.shadowPath = [NSBezierPath bezierPathWithRoundedRect:NSMakeRect(0.0, 0.0, panelWidth, panelHeight)
                                                                 xRadius:28.0
                                                                 yRadius:28.0].CGPath;
    self.sweepLayer.frame = NSMakeRect(-width * 0.8, 0.0, width * 0.72, height);
    self.orbitLayer.frame = NSRectToCGRect(orbitRect);
    self.orbitLayer.path = [NSBezierPath bezierPathWithOvalInRect:NSMakeRect(0, 0, orbitSize, orbitSize)].CGPath;
    self.innerOrbitLayer.frame = NSInsetRect(orbitRect, 13.0, 13.0);
    self.innerOrbitLayer.path = [NSBezierPath bezierPathWithOvalInRect:NSMakeRect(0, 0, orbitSize - 26.0, orbitSize - 26.0)].CGPath;

    self.coreLayer.frame = NSMakeRect(centerX - 7.0, orbitY + orbitSize * 0.5 - 7.0, 14.0, 14.0);
    self.coreLayer.cornerRadius = 7.0;
    self.sparkLayer.frame = NSMakeRect(NSMaxX(orbitRect) - 8.0, orbitY + orbitSize * 0.5 - 4.0, 8.0, 8.0);
    self.sparkLayer.cornerRadius = 4.0;

    CGFloat barWidth = MIN(148.0, panelWidth - 96.0);
    CGFloat barY = orbitY + orbitSize + 28.0;
    for (NSUInteger i = 0; i < self.barLayers.count; i++) {
        CALayer *bar = self.barLayers[i];
        CGFloat fraction = 1.0 - (CGFloat)i * 0.16;
        CGFloat x = centerX - (barWidth * fraction) * 0.5;
        bar.frame = NSMakeRect(x, barY + (CGFloat)i * 8.0, barWidth * fraction, 4.0);
    }

    CGFloat dotsWidth = 58.0;
    CGFloat dotStart = centerX - dotsWidth * 0.5;
    for (NSUInteger i = 0; i < self.dotLayers.count; i++) {
        self.dotLayers[i].frame = NSMakeRect(dotStart + (CGFloat)i * 13.0, barY + 45.0, 5.0, 5.0);
    }
    self.messageLabel.frame = NSMakeRect(panelX + 36.0, barY + 64.0, MAX(80.0, panelWidth - 72.0), 38.0);
    if (!self.queuePositionLabel.hidden) {
        self.queuePositionLabel.frame = NSMakeRect(centerX - 92.0, barY + 105.0, 184.0, 26.0);
    }

    if (self.adVisible) {
        CGFloat adInset = 28.0;
        CGFloat adY = panelY + 34.0;
        CGFloat queueRowHeight = !self.queuePositionLabel.hidden ? 38.0 : 0.0;
        CGFloat adHeight = panelHeight - 106.0 - queueRowHeight;
        self.adContainerView.frame = NSMakeRect(panelX + adInset, adY, panelWidth - adInset * 2.0, adHeight);
        CGFloat adWidth = NSWidth(self.adContainerView.bounds);
        CGFloat queueBadgeWidth = 192.0;
        BOOL showQueueBadge = !self.queuePositionLabel.hidden;
        CGFloat mediaHeight = MIN(190.0, MAX(120.0, adHeight * 0.54));
        self.adPlayerView.frame = NSMakeRect(18.0, 18.0, adWidth - 36.0, mediaHeight);
        self.adChipLabel.frame = NSMakeRect(20.0, mediaHeight + 32.0, adWidth - 40.0, 18.0);
        self.adTitleLabel.frame = NSMakeRect(20.0, mediaHeight + 54.0, adWidth - 40.0 - (showQueueBadge ? queueBadgeWidth + 14.0 : 0.0), 54.0);
        self.adMessageLabel.frame = NSMakeRect(20.0, mediaHeight + 112.0, adWidth - 40.0, 56.0);
        self.messageLabel.frame = NSMakeRect(panelX + 36.0, NSMaxY(panelRect) - 52.0, MAX(80.0, panelWidth - 72.0), 24.0);
        if (showQueueBadge) {
            self.queuePositionLabel.frame = NSMakeRect(centerX - queueBadgeWidth * 0.5,
                                                       NSMaxY(self.adContainerView.frame) + 10.0,
                                                       queueBadgeWidth,
                                                       26.0);
        }
    }

    NSUInteger stepCount = self.stepIndicatorLayers.count;
    if (stepCount > 0) {
        CGFloat gap = 7.0;
        CGFloat railWidth = MIN(240.0, panelWidth - 112.0);
        CGFloat segmentWidth = floor((railWidth - gap * (CGFloat)(stepCount - 1)) / (CGFloat)stepCount);
        CGFloat segmentX = centerX - railWidth * 0.5;
        CGFloat segmentY = barY + 116.0;
        for (NSUInteger i = 0; i < stepCount; i++) {
            self.stepIndicatorLayers[i].frame = NSMakeRect(segmentX + (segmentWidth + gap) * (CGFloat)i,
                                                           segmentY,
                                                           segmentWidth,
                                                           3.0);
        }
    }
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    if (self.window) {
        [self startAnimating];
    } else {
        [self stopAnimating];
    }
}

- (void)startAnimating {
    if ([self.orbitLayer animationForKey:@"opn.orbit.rotate"]) return;

    CABasicAnimation *orbitSpin = [CABasicAnimation animationWithKeyPath:@"transform.rotation.z"];
    orbitSpin.fromValue = @0.0;
    orbitSpin.toValue = @(M_PI * 2.0);
    orbitSpin.duration = 1.65;
    orbitSpin.repeatCount = HUGE_VALF;
    orbitSpin.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
    [self.orbitLayer addAnimation:orbitSpin forKey:@"opn.orbit.rotate"];

    CABasicAnimation *innerSpin = [CABasicAnimation animationWithKeyPath:@"transform.rotation.z"];
    innerSpin.fromValue = @(M_PI * 2.0);
    innerSpin.toValue = @0.0;
    innerSpin.duration = 4.2;
    innerSpin.repeatCount = HUGE_VALF;
    [self.innerOrbitLayer addAnimation:innerSpin forKey:@"opn.inner.rotate"];

    CABasicAnimation *sparkSpin = [CABasicAnimation animationWithKeyPath:@"transform.rotation.z"];
    sparkSpin.fromValue = @0.0;
    sparkSpin.toValue = @(M_PI * 2.0);
    sparkSpin.duration = 1.65;
    sparkSpin.repeatCount = HUGE_VALF;
    sparkSpin.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
    [self.sparkLayer addAnimation:sparkSpin forKey:@"opn.spark.rotate"];

    CABasicAnimation *pulse = [CABasicAnimation animationWithKeyPath:@"transform.scale"];
    pulse.fromValue = @0.82;
    pulse.toValue = @1.24;
    pulse.duration = 0.82;
    pulse.autoreverses = YES;
    pulse.repeatCount = HUGE_VALF;
    pulse.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
    [self.coreLayer addAnimation:pulse forKey:@"opn.core.pulse"];

    CABasicAnimation *sweep = [CABasicAnimation animationWithKeyPath:@"transform.translation.x"];
    sweep.fromValue = @0.0;
    sweep.toValue = @(NSWidth(self.bounds) * 2.1);
    sweep.duration = 2.65;
    sweep.repeatCount = HUGE_VALF;
    sweep.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
    [self.sweepLayer addAnimation:sweep forKey:@"opn.sweep"];

    for (NSUInteger i = 0; i < self.barLayers.count; i++) {
        CALayer *bar = self.barLayers[i];
        CABasicAnimation *scale = [CABasicAnimation animationWithKeyPath:@"transform.scale.x"];
        scale.fromValue = @0.28;
        scale.toValue = @1.0;
        scale.duration = 0.92;
        scale.autoreverses = YES;
        scale.repeatCount = HUGE_VALF;
        scale.beginTime = CACurrentMediaTime() + (CFTimeInterval)i * 0.12;
        scale.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
        [bar addAnimation:scale forKey:@"opn.bar.scale"];
    }

    for (NSUInteger i = 0; i < self.dotLayers.count; i++) {
        CALayer *dot = self.dotLayers[i];
        CABasicAnimation *opacity = [CABasicAnimation animationWithKeyPath:@"opacity"];
        opacity.fromValue = @0.22;
        opacity.toValue = @1.0;
        opacity.duration = 0.72;
        opacity.autoreverses = YES;
        opacity.repeatCount = HUGE_VALF;
        opacity.beginTime = CACurrentMediaTime() + (CFTimeInterval)i * 0.10;
        [dot addAnimation:opacity forKey:@"opn.dot.opacity"];
    }
}

- (void)stopAnimating {
    [self.sweepLayer removeAllAnimations];
    [self.orbitLayer removeAllAnimations];
    [self.innerOrbitLayer removeAllAnimations];
    [self.coreLayer removeAllAnimations];
    [self.sparkLayer removeAllAnimations];
    for (CALayer *bar in self.barLayers) [bar removeAllAnimations];
    for (CALayer *dot in self.dotLayers) [dot removeAllAnimations];
}

- (void)dealloc {
    [self.adFallbackTimer invalidate];
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

@end
