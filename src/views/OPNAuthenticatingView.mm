#import "OPNAuthenticatingView.h"
#import "OPNLoadingView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"

@interface OPNAuthenticatingView ()
@property (nonatomic, strong) OPNLoadingView *loadingView;
@end

@implementation OPNAuthenticatingView

- (instancetype)initWithFrame:(NSRect)frame message:(NSString *)message {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.layer.backgroundColor = OpnColor(OPN::kOverlay, 0.18).CGColor;

        self.loadingView = [[OPNLoadingView alloc] initWithFrame:NSMakeRect(0, 0, 420, 252)
                                                         message:message];
        self.loadingView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        [self addSubview:self.loadingView];
        self.statusLabel = self.loadingView.messageLabel;
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (void)layout {
    [super layout];
    self.loadingView.frame = self.bounds;
}

@end
