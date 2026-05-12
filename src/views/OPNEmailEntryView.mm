#import "OPNEmailEntryView.h"
#import "../common/OPNColorTokens.h"
#import "../common/OPNUIHelpers.h"
#import "../common/OPNAuthTypes.h"
#import "../auth/OPNAuthService.h"

@implementation OPNEmailEntryView

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        [self buildUI];
    }
    return self;
}

- (BOOL)isFlipped { return YES; }

- (void)buildUI {
    using namespace OPN;

    NSView *content = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 480, 460)];
    content.autoresizingMask = NSViewMinXMargin | NSViewMaxXMargin | NSViewMinYMargin | NSViewMaxYMargin;
    content.frame = NSMakeRect(floor((NSWidth(self.bounds) - 480) / 2.0),
                               floor((NSHeight(self.bounds) - 460) / 2.0), 480, 460);
    [self addSubview:content];

    NSView *brand = [[NSView alloc] initWithFrame:NSMakeRect(156, 12, 168, 42)];
    [content addSubview:brand];

    [brand addSubview:OpnLabel(@"OpenNOW", NSMakeRect(0, 9, 168, 24),
                               20, OpnColor(kTextPrimary), NSFontWeightSemibold, NSTextAlignmentCenter)];

    NSView *card = [[NSView alloc] initWithFrame:NSMakeRect(40, 72, 400, 332)];
    card.wantsLayer = YES;
    card.layer.backgroundColor = OpnColor(kSurfaceRaised, 0.86).CGColor;
    card.layer.cornerRadius = 22;
    card.layer.borderWidth = 1;
    card.layer.borderColor = OpnColor(0xFFFFFF, 0.10).CGColor;
    card.layer.shadowColor = NSColor.blackColor.CGColor;
    card.layer.shadowOpacity = 0.26;
    card.layer.shadowRadius = 24;
    card.layer.shadowOffset = CGSizeMake(0, 14);
    [content addSubview:card];

    [card addSubview:OpnLabel(@"Access your cloud gaming library with your NVIDIA account.",
                              NSMakeRect(56, 48, 288, 38), 13,
                              OpnColor(kTextMuted), NSFontWeightRegular, NSTextAlignmentCenter)];

    self.stayLoggedInToggle = [[NSButton alloc] initWithFrame:NSMakeRect(54, 168, 180, 24)];
    self.stayLoggedInToggle.buttonType = NSButtonTypeSwitch;
    self.stayLoggedInToggle.title = @"Keep me signed in";
    self.stayLoggedInToggle.font = [NSFont systemFontOfSize:13 weight:NSFontWeightMedium];
    self.stayLoggedInToggle.contentTintColor = OpnColor(kBrandGreen);
    self.stayLoggedInToggle.state = OPN::AuthService::Shared().GetStayLoggedIn()
        ? NSControlStateValueOn : NSControlStateValueOff;
    [card addSubview:self.stayLoggedInToggle];

    NSButton *browserButton = OpnButton(@"Continue with Browser",
        NSMakeRect(56, 224, 288, 48),
        OpnColor(kBrandGreen), OpnColor(kAccentOn));
    browserButton.font = [NSFont systemFontOfSize:14 weight:NSFontWeightSemibold];
    browserButton.target = self;
    browserButton.action = @selector(signInWithBrowserClicked);
    [card addSubview:browserButton];

    [content addSubview:OpnLabel(@"Open-source cloud gaming client for macOS",
                                 NSMakeRect(0, 428, 480, 20), 12,
                                 OpnColor(kTextMuted), NSFontWeightRegular, NSTextAlignmentCenter)];
}

- (void)signInWithBrowserClicked {
    using namespace OPN;
    bool stayLoggedIn = self.stayLoggedInToggle.state == NSControlStateValueOn;
    AuthService::Shared().SetStayLoggedIn(stayLoggedIn);
    if (self.onSignInWithBrowser) {
        self.onSignInWithBrowser();
    }
}

@end
