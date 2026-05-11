#import <Cocoa/Cocoa.h>
#include "../streaming/OPNStreamTypes.h"

@interface OPNLoadingView : NSView

@property (nonatomic, copy) NSString *message;
@property (nonatomic, copy) NSArray<NSString *> *steps;
@property (nonatomic, assign) NSInteger currentStepIndex;
@property (nonatomic, assign) NSInteger queuePosition;
@property (nonatomic, strong, readonly) NSTextField *messageLabel;
@property (nonatomic, copy) void (^adPlaybackEventHandler)(NSString *adId, NSString *action, NSInteger watchedTimeInMs, NSInteger pausedTimeInMs, NSString *cancelReason);

- (instancetype)initWithFrame:(NSRect)frame message:(NSString *)message;
- (void)setSteps:(NSArray<NSString *> *)steps currentStepIndex:(NSInteger)currentStepIndex;
- (void)advanceToStep:(NSInteger)stepIndex message:(NSString *)message;
- (void)updateQueuePosition:(NSInteger)queuePosition;
- (void)updateAdState:(const OPN::SessionAdState &)adState;
- (void)clearAdPresentation;
- (void)startAnimating;
- (void)stopAnimating;

@end
