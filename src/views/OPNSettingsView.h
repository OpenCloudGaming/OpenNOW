#import <Cocoa/Cocoa.h>

@interface OPNSettingsView : NSView

@property (nonatomic, copy) void (^onBackRequested)(void);

- (instancetype)initWithFrame:(NSRect)frame;
- (instancetype)initWithFrame:(NSRect)frame selectedSectionName:(NSString *)selectedSectionName;

@end
