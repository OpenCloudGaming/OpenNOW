#import <Cocoa/Cocoa.h>

@interface OPNEmailEntryView : NSView
@property (nonatomic, copy) void (^onSignInWithBrowser)(void);
@property (nonatomic, strong) NSButton *stayLoggedInToggle;
@end