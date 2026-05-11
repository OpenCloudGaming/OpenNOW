#import <Cocoa/Cocoa.h>

@interface OPNErrorView : NSView
@property (nonatomic, copy) void (^onRetry)(void);
@property (nonatomic, copy) void (^onBackToEmail)(void);
- (instancetype)initWithFrame:(NSRect)frame
                      message:(NSString *)message
                     canRetry:(BOOL)canRetry;
@end
