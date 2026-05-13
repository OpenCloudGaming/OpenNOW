#pragma once

#import <Cocoa/Cocoa.h>

#ifdef __cplusplus
#include <string>
namespace OPN {
class IStreamSession;
}
#endif

@interface OPNStreamView : NSView

#ifdef __cplusplus
- (void)setStreamSession:(OPN::IStreamSession *)session;
- (void)setMicrophoneMode:(const std::string &)mode pushToTalkKeyCode:(uint16_t)keyCode modifierMask:(uint16_t)modifierMask;
#endif
- (void)setMaxBitrateMbps:(NSInteger)mbps;
- (BOOL)toggleMicrophoneEnabledShortcut;
- (BOOL)toggleRecordingShortcut;
- (void)toggleSidebarHUD;
- (void)setRecordingGameTitle:(NSString *)gameTitle;
- (void)stopRecordingIfNeeded;
- (void)setMicrophoneLevel:(double)level;
- (void)setSuppressInputWhenWindowInactive:(BOOL)suppress;
- (void)setInputSuspendedForLibraryOverlay:(BOOL)suspended;
- (void)attachToPipeline:(void *)pipeline;
- (void)detachFromPipeline;
- (void)handleKeyEvent:(NSEvent *)event;
- (void)handleMouseEvent:(NSEvent *)event;
- (NSView *)nativeVideoView;
- (void)setVideoAspectRatio:(CGFloat)aspectRatio;
- (void)takeFocus;
- (void)releasePointerLock;

@property (nonatomic, copy) void (^onGuideButtonPressed)(void);

@end
