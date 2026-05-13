#pragma once

#import <Cocoa/Cocoa.h>
#import <CoreMedia/CoreMedia.h>

@interface OPNStreamRecordingManager : NSObject

@property (nonatomic, readonly, getter=isRecording) BOOL recording;
@property (nonatomic, readonly, getter=isStarting) BOOL starting;
@property (nonatomic, readonly) NSString *statusText;
@property (nonatomic, readonly) NSURL *currentRecordingURL;
@property (nonatomic, readonly) NSArray<NSURL *> *recentRecordingURLs;
@property (nonatomic, copy) void (^onStateChanged)(void);

- (void)toggleRecordingForGameTitle:(NSString *)gameTitle window:(NSWindow *)window;
- (void)startRecordingForGameTitle:(NSString *)gameTitle window:(NSWindow *)window;
- (void)stopRecording;
- (void)appendWebRTCVideoFrame:(void *)frame;
- (NSImage *)thumbnailForRecordingURL:(NSURL *)url size:(NSSize)size;

@end
