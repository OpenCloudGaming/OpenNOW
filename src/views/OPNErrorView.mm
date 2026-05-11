#import "OPNErrorView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"

@implementation OPNErrorView

- (instancetype)initWithFrame:(NSRect)frame
                      message:(NSString *)message
                     canRetry:(BOOL)canRetry {
    self = [super initWithFrame:frame];
    if (self) {
        using namespace OPN;

        if (!message || message.length == 0) {
            message = @"An unknown error occurred.";
        }
        NSString *title = @"Authentication Error";
        if ([message localizedCaseInsensitiveContainsString:@"stream"] ||
            [message localizedCaseInsensitiveContainsString:@"WebRTC"] ||
            [message localizedCaseInsensitiveContainsString:@"connection"]) {
            title = @"Connection Error";
        }

        self.wantsLayer = YES;
        self.layer.backgroundColor = [NSColor clearColor].CGColor;

        NSView *card = [[NSView alloc] initWithFrame:
            NSMakeRect(frame.size.width / 2.0 - 210, frame.size.height / 2.0 - 150, 420, 300)];
        card.autoresizingMask = NSViewMinXMargin | NSViewMaxXMargin | NSViewMinYMargin | NSViewMaxYMargin;
        card.wantsLayer = YES;
        card.layer.backgroundColor = OpnColor(kSurfaceRaised, 0.88).CGColor;
        card.layer.cornerRadius = 22;
        card.layer.borderWidth = 1;
        card.layer.borderColor = OpnColor(0xFFFFFF, 0.10).CGColor;
        card.layer.shadowColor = NSColor.blackColor.CGColor;
        card.layer.shadowOpacity = 0.24;
        card.layer.shadowRadius = 22;
        card.layer.shadowOffset = CGSizeMake(0, 12);
        [self addSubview:card];

        NSView *errorDot = [[NSView alloc] initWithFrame:NSMakeRect(199, 34, 22, 22)];
        errorDot.wantsLayer = YES;
        errorDot.layer.cornerRadius = 11;
        errorDot.layer.backgroundColor = OpnColor(kErrorRed, 0.18).CGColor;
        errorDot.layer.borderWidth = 1;
        errorDot.layer.borderColor = OpnColor(kErrorRed, 0.42).CGColor;
        [card addSubview:errorDot];

        [card addSubview:OpnLabel(title, NSMakeRect(0, 70, 420, 28),
                                  22, OpnColor(kTextPrimary), NSFontWeightSemibold, NSTextAlignmentCenter)];

        NSTextField *msgLabel = OpnLabel(message, NSMakeRect(44, 108, 332, 78),
                                    13, OpnColor(kTextSecondary), NSFontWeightRegular, NSTextAlignmentCenter);
        msgLabel.maximumNumberOfLines = 5;
        [card addSubview:msgLabel];

        if (canRetry) {
            NSButton *retryBtn = OpnButton(@"Try Again",
                NSMakeRect(64, 190, 292, 44),
                OpnColor(kBrandGreen), OpnColor(kAccentOn));
            retryBtn.target = self;
            retryBtn.action = @selector(retryClicked);
            [card addSubview:retryBtn];
        }

        NSButton *backBtn = [[NSButton alloc] initWithFrame:
            NSMakeRect(64, 242, 292, 30)];
        backBtn.title = @"Return to Sign In";
        backBtn.bordered = NO;
        backBtn.font = [NSFont systemFontOfSize:13];
        backBtn.contentTintColor = OpnColor(kLinkBlue);
        backBtn.target = self;
        backBtn.action = @selector(backClicked);
        [card addSubview:backBtn];
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (void)retryClicked {
    if (self.onRetry) self.onRetry();
}

- (void)backClicked {
    if (self.onBackToEmail) self.onBackToEmail();
}

@end
