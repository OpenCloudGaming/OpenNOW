#import <Cocoa/Cocoa.h>

@interface OPNAuthenticatingView : NSView
@property (nonatomic, strong) NSTextField *statusLabel;
- (instancetype)initWithFrame:(NSRect)frame message:(NSString *)message;
@end
