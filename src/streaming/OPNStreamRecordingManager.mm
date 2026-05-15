#import "OPNStreamRecordingManager.h"

#import <AVFoundation/AVFoundation.h>
#import <CoreImage/CoreImage.h>
#import <CoreVideo/CoreVideo.h>
#import <QuartzCore/QuartzCore.h>

#if __has_include(<ScreenCaptureKit/ScreenCaptureKit.h>)
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#define OPN_HAVE_SCREENCAPTUREKIT 1
#else
#define OPN_HAVE_SCREENCAPTUREKIT 0
#endif

#if defined(OPN_HAVE_LIBWEBRTC)
#import <WebRTC/WebRTC.h>
#import <WebRTC/RTCCVPixelBuffer.h>
#import <WebRTC/RTCI420Buffer.h>
#endif

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <vector>

typedef NS_ENUM(NSInteger, OPNRecordingAudioKind) {
    OPNRecordingAudioKindSystem = 0,
    OPNRecordingAudioKindMicrophone = 1,
};

#if OPN_HAVE_SCREENCAPTUREKIT
@interface OPNRecordingScreenCaptureOutput : NSObject <SCStreamOutput, SCStreamDelegate>
@property (nonatomic, weak) OPNStreamRecordingManager *manager;
@end
#endif

@interface OPNStreamRecordingManager () <AVCaptureAudioDataOutputSampleBufferDelegate>
@property (nonatomic, readwrite, getter=isRecording) BOOL recording;
@property (nonatomic, readwrite, getter=isStarting) BOOL starting;
@property (nonatomic, readwrite) NSString *statusText;
@property (nonatomic, readwrite) NSURL *currentRecordingURL;
@property (nonatomic, readwrite) NSArray<NSURL *> *recentRecordingURLs;
@end

@implementation OPNStreamRecordingManager {
    dispatch_queue_t _writerQueue;
    dispatch_queue_t _audioQueue;
    AVAssetWriter *_writer;
    AVAssetWriterInput *_videoInput;
    AVAssetWriterInput *_systemAudioInput;
    AVAssetWriterInput *_microphoneAudioInput;
    AVAssetWriterInputPixelBufferAdaptor *_pixelBufferAdaptor;
    CIContext *_ciContext;
    CGSize _videoSize;
    BOOL _acceptingSamples;
    BOOL _finishRequested;
    CFTimeInterval _recordingStartHostTime;
    CMTime _lastVideoTime;
    CMTime _systemAudioSourceStartTime;
    CMTime _microphoneAudioSourceStartTime;
    CMTime _systemAudioTimelineOffset;
    CMTime _microphoneAudioTimelineOffset;
    BOOL _videoFrameAppendInFlight;
    uint64_t _droppedVideoFrames;
    CFTimeInterval _lastDroppedVideoFrameLogTime;
    AVCaptureSession *_microphoneCaptureSession;
#if OPN_HAVE_SCREENCAPTUREKIT
    SCStream *_audioStream;
    OPNRecordingScreenCaptureOutput *_audioOutput;
#endif
}

- (instancetype)init {
    self = [super init];
    if (self) {
        _writerQueue = dispatch_queue_create("com.opennow.recording.writer", DISPATCH_QUEUE_SERIAL);
        _audioQueue = dispatch_queue_create("com.opennow.recording.audio", DISPATCH_QUEUE_SERIAL);
        _ciContext = [CIContext contextWithOptions:nil];
        _statusText = @"Ready";
        _recentRecordingURLs = @[];
        _lastVideoTime = kCMTimeInvalid;
        _systemAudioSourceStartTime = kCMTimeInvalid;
        _microphoneAudioSourceStartTime = kCMTimeInvalid;
        _systemAudioTimelineOffset = kCMTimeInvalid;
        _microphoneAudioTimelineOffset = kCMTimeInvalid;
        _videoFrameAppendInFlight = NO;
        _droppedVideoFrames = 0;
        _lastDroppedVideoFrameLogTime = 0.0;
        [self refreshRecentRecordings];
    }
    return self;
}

- (void)dealloc {
    [self stopRecording];
}

- (void)toggleRecordingForGameTitle:(NSString *)gameTitle window:(NSWindow *)window {
    if (self.recording || self.starting) {
        [self stopRecording];
    } else {
        [self startRecordingForGameTitle:gameTitle window:window];
    }
}

- (void)startRecordingForGameTitle:(NSString *)gameTitle window:(NSWindow *)window {
    if (self.recording || self.starting) return;

    NSURL *moviesURL = [NSFileManager.defaultManager URLsForDirectory:NSMoviesDirectory inDomains:NSUserDomainMask].firstObject;
    if (!moviesURL) {
        [self updateStatus:@"Movies folder unavailable" starting:NO recording:NO notify:YES];
        return;
    }

    NSError *directoryError = nil;
    if (![NSFileManager.defaultManager createDirectoryAtURL:moviesURL withIntermediateDirectories:YES attributes:nil error:&directoryError]) {
        NSString *message = directoryError.localizedDescription ?: @"Unable to create Movies folder";
        [self updateStatus:message starting:NO recording:NO notify:YES];
        return;
    }

    NSString *filename = OPNRecordingFilename(gameTitle);
    NSURL *outputURL = [moviesURL URLByAppendingPathComponent:filename];
    [NSFileManager.defaultManager removeItemAtURL:outputURL error:nil];

    self.currentRecordingURL = outputURL;
    [self updateStatus:@"Starting recording" starting:YES recording:NO notify:YES];

    dispatch_async(_writerQueue, ^{
        self->_writer = nil;
        self->_videoInput = nil;
        self->_systemAudioInput = nil;
        self->_microphoneAudioInput = nil;
        self->_pixelBufferAdaptor = nil;
        self->_videoSize = CGSizeZero;
        self->_acceptingSamples = YES;
        self->_finishRequested = NO;
        self->_recordingStartHostTime = CACurrentMediaTime();
        self->_lastVideoTime = kCMTimeInvalid;
        self->_systemAudioSourceStartTime = kCMTimeInvalid;
        self->_microphoneAudioSourceStartTime = kCMTimeInvalid;
        self->_systemAudioTimelineOffset = kCMTimeInvalid;
        self->_microphoneAudioTimelineOffset = kCMTimeInvalid;
        self->_videoFrameAppendInFlight = NO;
        self->_droppedVideoFrames = 0;
        self->_lastDroppedVideoFrameLogTime = 0.0;
    });

    [self startAudioCaptureForWindow:window];
}

- (void)stopRecording {
    if (!self.recording && !self.starting) return;

    [self updateStatus:@"Finishing recording" starting:NO recording:NO notify:YES];
    [self stopAudioCapture];
    [self stopAVMicrophoneCapture];

    dispatch_async(_writerQueue, ^{
        self->_acceptingSamples = NO;
        self->_finishRequested = YES;
        self->_videoFrameAppendInFlight = NO;
        AVAssetWriter *writer = self->_writer;
        NSURL *outputURL = self.currentRecordingURL;
        if (!writer) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (outputURL) [NSFileManager.defaultManager removeItemAtURL:outputURL error:nil];
                self.currentRecordingURL = nil;
                [self updateStatus:@"Recording canceled" starting:NO recording:NO notify:YES];
            });
            return;
        }

        [self->_videoInput markAsFinished];
        [self->_systemAudioInput markAsFinished];
        [self->_microphoneAudioInput markAsFinished];
        [writer finishWritingWithCompletionHandler:^{
            NSError *error = writer.error;
            dispatch_async(self->_writerQueue, ^{
                self->_writer = nil;
                self->_videoInput = nil;
                self->_systemAudioInput = nil;
                self->_microphoneAudioInput = nil;
                self->_pixelBufferAdaptor = nil;
            });
            dispatch_async(dispatch_get_main_queue(), ^{
                if (writer.status == AVAssetWriterStatusCompleted && !error) {
                    [self refreshRecentRecordings];
                    [self updateStatus:@"Recording saved" starting:NO recording:NO notify:YES];
                    NSLog(@"[Recording] Saved %@", outputURL.path);
                } else {
                    if (outputURL) [NSFileManager.defaultManager removeItemAtURL:outputURL error:nil];
                    NSString *message = error.localizedDescription ?: @"Recording failed";
                    [self updateStatus:message starting:NO recording:NO notify:YES];
                    NSLog(@"[Recording] Finish failed: %@", message);
                }
            });
        }];
    });
}

- (void)appendWebRTCVideoFrame:(void *)frame {
#if defined(OPN_HAVE_LIBWEBRTC)
    if (!frame || (!self.recording && !self.starting)) return;
    RTCVideoFrame *videoFrame = (__bridge RTCVideoFrame *)frame;
    @synchronized (self) {
        if (_videoFrameAppendInFlight) {
            [self recordDroppedVideoFrame];
            return;
        }
        _videoFrameAppendInFlight = YES;
    }
    dispatch_async(_writerQueue, ^{
        @autoreleasepool {
            RTCVideoFrame *retainedFrame = videoFrame;
            if (!retainedFrame || !self->_acceptingSamples || !self.currentRecordingURL) {
                [self finishVideoFrameAppend];
                return;
            }

            CGSize size = OPNRecordingFrameSize(retainedFrame);
            if (size.width < 2.0 || size.height < 2.0) {
                [self finishVideoFrameAppend];
                return;
            }
            if (!self->_writer && ![self createWriterWithVideoSize:size]) {
                [self finishVideoFrameAppend];
                return;
            }
            if (self->_writer.status != AVAssetWriterStatusWriting || !self->_videoInput.readyForMoreMediaData) {
                [self finishVideoFrameAppend];
                return;
            }

            CVPixelBufferRef pixelBuffer = [self copyPixelBufferFromVideoFrame:retainedFrame];
            if (!pixelBuffer) {
                [self finishVideoFrameAppend];
                return;
            }

            CMTime presentationTime = [self nextVideoPresentationTime];
            BOOL appended = [self->_pixelBufferAdaptor appendPixelBuffer:pixelBuffer withPresentationTime:presentationTime];
            CVPixelBufferRelease(pixelBuffer);
            if (!appended) {
                NSLog(@"[Recording] Video append failed: %@", self->_writer.error.localizedDescription ?: @"unknown");
            } else if (self.starting) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    [self updateStatus:@"Recording" starting:NO recording:YES notify:YES];
                });
            }
            [self finishVideoFrameAppend];
        }
    });
#else
    (void)frame;
#endif
}

- (void)finishVideoFrameAppend {
    @synchronized (self) {
        _videoFrameAppendInFlight = NO;
    }
}

- (void)recordDroppedVideoFrame {
    _droppedVideoFrames++;
    CFTimeInterval now = CACurrentMediaTime();
    if (now - _lastDroppedVideoFrameLogTime >= 5.0) {
        NSLog(@"[Recording] Dropping video frames while writer is busy (total=%llu)", (unsigned long long)_droppedVideoFrames);
        _lastDroppedVideoFrameLogTime = now;
    }
}

- (NSImage *)thumbnailForRecordingURL:(NSURL *)url size:(NSSize)size {
    if (!url) return nil;
    AVURLAsset *asset = [AVURLAsset URLAssetWithURL:url options:nil];
    AVAssetImageGenerator *generator = [[AVAssetImageGenerator alloc] initWithAsset:asset];
    generator.appliesPreferredTrackTransform = YES;
    generator.maximumSize = size;
    NSError *error = nil;
    CGImageRef image = [generator copyCGImageAtTime:CMTimeMakeWithSeconds(0.5, 600) actualTime:nil error:&error];
    if (!image) image = [generator copyCGImageAtTime:kCMTimeZero actualTime:nil error:&error];
    if (!image) return nil;
    NSImage *thumbnail = [[NSImage alloc] initWithCGImage:image size:size];
    CGImageRelease(image);
    return thumbnail;
}

- (BOOL)createWriterWithVideoSize:(CGSize)size {
    NSError *error = nil;
    AVAssetWriter *writer = [[AVAssetWriter alloc] initWithURL:self.currentRecordingURL fileType:AVFileTypeMPEG4 error:&error];
    if (!writer || error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            [self updateStatus:error.localizedDescription ?: @"Unable to start writer" starting:NO recording:NO notify:YES];
        });
        return NO;
    }

    NSInteger width = std::max<NSInteger>(2, (NSInteger)std::llround(size.width));
    NSInteger height = std::max<NSInteger>(2, (NSInteger)std::llround(size.height));
    NSDictionary *videoSettings = @{
        AVVideoCodecKey: AVVideoCodecTypeH264,
        AVVideoWidthKey: @(width),
        AVVideoHeightKey: @(height),
        AVVideoCompressionPropertiesKey: @{
            AVVideoAverageBitRateKey: @(std::min<NSInteger>(60000000, std::max<NSInteger>(8000000, width * height * 8))),
            AVVideoExpectedSourceFrameRateKey: @60,
            AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
        },
    };
    AVAssetWriterInput *videoInput = [[AVAssetWriterInput alloc] initWithMediaType:AVMediaTypeVideo outputSettings:videoSettings];
    videoInput.expectsMediaDataInRealTime = YES;
    NSDictionary *pixelAttributes = @{
        (NSString *)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA),
        (NSString *)kCVPixelBufferWidthKey: @(width),
        (NSString *)kCVPixelBufferHeightKey: @(height),
        (NSString *)kCVPixelBufferIOSurfacePropertiesKey: @{},
    };
    AVAssetWriterInputPixelBufferAdaptor *adaptor = [[AVAssetWriterInputPixelBufferAdaptor alloc] initWithAssetWriterInput:videoInput sourcePixelBufferAttributes:pixelAttributes];

    AVAssetWriterInput *systemAudio = [[AVAssetWriterInput alloc] initWithMediaType:AVMediaTypeAudio outputSettings:OPNRecordingAudioSettings(2, 160000)];
    systemAudio.expectsMediaDataInRealTime = YES;
    AVAssetWriterInput *microphoneAudio = [[AVAssetWriterInput alloc] initWithMediaType:AVMediaTypeAudio outputSettings:OPNRecordingAudioSettings(1, 96000)];
    microphoneAudio.expectsMediaDataInRealTime = YES;

    if (![writer canAddInput:videoInput]) return NO;
    [writer addInput:videoInput];
    if ([writer canAddInput:systemAudio]) [writer addInput:systemAudio];
    if ([writer canAddInput:microphoneAudio]) [writer addInput:microphoneAudio];

    if (![writer startWriting]) {
        NSLog(@"[Recording] startWriting failed: %@", writer.error.localizedDescription ?: @"unknown");
        return NO;
    }
    [writer startSessionAtSourceTime:kCMTimeZero];

    _writer = writer;
    _videoInput = videoInput;
    _systemAudioInput = systemAudio;
    _microphoneAudioInput = microphoneAudio;
    _pixelBufferAdaptor = adaptor;
    _videoSize = CGSizeMake(width, height);
    NSLog(@"[Recording] Writer started %@ %.0fx%.0f", self.currentRecordingURL.path, _videoSize.width, _videoSize.height);
    return YES;
}

- (CMTime)nextVideoPresentationTime {
    CFTimeInterval elapsed = std::max<CFTimeInterval>(0.0, CACurrentMediaTime() - _recordingStartHostTime);
    CMTime time = CMTimeMakeWithSeconds(elapsed, 600);
    if (CMTIME_IS_VALID(_lastVideoTime) && CMTimeCompare(time, _lastVideoTime) <= 0) {
        time = CMTimeAdd(_lastVideoTime, CMTimeMake(1, 600));
    }
    _lastVideoTime = time;
    return time;
}

- (CVPixelBufferRef)copyPixelBufferFromVideoFrame:(RTCVideoFrame *)frame {
#if defined(OPN_HAVE_LIBWEBRTC)
    CVPixelBufferRef output = nil;
    CVPixelBufferPoolRef pool = _pixelBufferAdaptor.pixelBufferPool;
    if (!pool || CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault, pool, &output) != kCVReturnSuccess || !output) {
        return nil;
    }

    id<RTCVideoFrameBuffer> buffer = frame.buffer;
    if ([buffer isKindOfClass:RTCCVPixelBuffer.class]) {
        RTCCVPixelBuffer *cvBuffer = (RTCCVPixelBuffer *)buffer;
        CIImage *image = [CIImage imageWithCVPixelBuffer:cvBuffer.pixelBuffer];
        if (cvBuffer.requiresCropping) {
            CGRect crop = CGRectMake(cvBuffer.cropX, cvBuffer.cropY, cvBuffer.cropWidth, cvBuffer.cropHeight);
            image = [[image imageByCroppingToRect:crop] imageByApplyingTransform:CGAffineTransformMakeTranslation(-crop.origin.x, -crop.origin.y)];
        }
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
        [_ciContext render:image toCVPixelBuffer:output bounds:CGRectMake(0, 0, _videoSize.width, _videoSize.height) colorSpace:colorSpace];
        CGColorSpaceRelease(colorSpace);
        return output;
    }

    RTCVideoFrame *i420Frame = [frame newI420VideoFrame];
    id<RTCI420Buffer> i420 = (id<RTCI420Buffer>)i420Frame.buffer;
    if (!i420) {
        CVPixelBufferRelease(output);
        return nil;
    }
    [self copyI420Buffer:i420 toBGRAOutput:output];
    return output;
#else
    (void)frame;
    return nil;
#endif
}

- (void)copyI420Buffer:(id)buffer toBGRAOutput:(CVPixelBufferRef)output {
#if defined(OPN_HAVE_LIBWEBRTC)
    id<RTCI420Buffer> i420 = (id<RTCI420Buffer>)buffer;
    CVPixelBufferLockBaseAddress(output, 0);
    uint8_t *dst = (uint8_t *)CVPixelBufferGetBaseAddress(output);
    const size_t dstStride = CVPixelBufferGetBytesPerRow(output);
    const int width = std::min<int>((int)_videoSize.width, i420.width);
    const int height = std::min<int>((int)_videoSize.height, i420.height);
    for (int y = 0; y < height; y++) {
        uint8_t *row = dst + (size_t)y * dstStride;
        const uint8_t *yRow = i420.dataY + y * i420.strideY;
        const uint8_t *uRow = i420.dataU + (y / 2) * i420.strideU;
        const uint8_t *vRow = i420.dataV + (y / 2) * i420.strideV;
        for (int x = 0; x < width; x++) {
            int yy = (int)yRow[x];
            int uu = (int)uRow[x / 2] - 128;
            int vv = (int)vRow[x / 2] - 128;
            int r = yy + (int)std::lround(1.402 * vv);
            int g = yy - (int)std::lround(0.344136 * uu + 0.714136 * vv);
            int b = yy + (int)std::lround(1.772 * uu);
            row[x * 4 + 0] = (uint8_t)std::max(0, std::min(255, b));
            row[x * 4 + 1] = (uint8_t)std::max(0, std::min(255, g));
            row[x * 4 + 2] = (uint8_t)std::max(0, std::min(255, r));
            row[x * 4 + 3] = 255;
        }
    }
    CVPixelBufferUnlockBaseAddress(output, 0);
#else
    (void)buffer;
    (void)output;
#endif
}

- (void)appendAudioSampleBuffer:(CMSampleBufferRef)sampleBuffer kind:(OPNRecordingAudioKind)kind {
    if (!sampleBuffer || (!self.recording && !self.starting)) return;
    CFRetain(sampleBuffer);
    dispatch_async(_writerQueue, ^{
        if (!self->_acceptingSamples || !self->_writer || self->_writer.status != AVAssetWriterStatusWriting) {
            CFRelease(sampleBuffer);
            return;
        }
        AVAssetWriterInput *input = kind == OPNRecordingAudioKindMicrophone ? self->_microphoneAudioInput : self->_systemAudioInput;
        if (!input || !input.readyForMoreMediaData) {
            CFRelease(sampleBuffer);
            return;
        }
        CMSampleBufferRef retimed = [self copyAudioSampleBuffer:sampleBuffer kind:kind];
        CFRelease(sampleBuffer);
        if (!retimed) return;
        BOOL appended = [input appendSampleBuffer:retimed];
        CFRelease(retimed);
        if (!appended) {
            NSLog(@"[Recording] Audio append failed: %@", self->_writer.error.localizedDescription ?: @"unknown");
        }
    });
}

- (CMSampleBufferRef)copyAudioSampleBuffer:(CMSampleBufferRef)sampleBuffer kind:(OPNRecordingAudioKind)kind {
    CMTime sourceTime = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
    if (!CMTIME_IS_VALID(sourceTime)) return nil;

    CMTime *sourceStart = kind == OPNRecordingAudioKindMicrophone ? &_microphoneAudioSourceStartTime : &_systemAudioSourceStartTime;
    CMTime *timelineOffset = kind == OPNRecordingAudioKindMicrophone ? &_microphoneAudioTimelineOffset : &_systemAudioTimelineOffset;
    if (!CMTIME_IS_VALID(*sourceStart)) {
        *sourceStart = sourceTime;
        *timelineOffset = CMTimeMakeWithSeconds(std::max<CFTimeInterval>(0.0, CACurrentMediaTime() - _recordingStartHostTime), 600);
    }

    CMTime delta = CMTimeSubtract(sourceTime, *sourceStart);
    CMTime targetTime = CMTimeAdd(*timelineOffset, delta);
    CMItemCount count = CMSampleBufferGetNumSamples(sampleBuffer);
    if (count <= 0) return nil;

    std::vector<CMSampleTimingInfo> timing((size_t)count);
    OSStatus status = CMSampleBufferGetSampleTimingInfoArray(sampleBuffer, count, timing.data(), nullptr);
    if (status != noErr) return nil;
    CMTime shift = CMTimeSubtract(targetTime, sourceTime);
    for (CMSampleTimingInfo &info : timing) {
        if (CMTIME_IS_VALID(info.presentationTimeStamp)) info.presentationTimeStamp = CMTimeAdd(info.presentationTimeStamp, shift);
        if (CMTIME_IS_VALID(info.decodeTimeStamp)) info.decodeTimeStamp = CMTimeAdd(info.decodeTimeStamp, shift);
    }
    CMSampleBufferRef copy = nil;
    status = CMSampleBufferCreateCopyWithNewTiming(kCFAllocatorDefault, sampleBuffer, count, timing.data(), &copy);
    return status == noErr ? copy : nil;
}

- (void)startAudioCaptureForWindow:(NSWindow *)window {
#if OPN_HAVE_SCREENCAPTUREKIT
    if (@available(macOS 13.0, *)) {
        NSScreen *screen = window.screen ?: NSScreen.mainScreen;
        NSNumber *screenNumber = screen.deviceDescription[@"NSScreenNumber"];
        CGDirectDisplayID targetDisplayID = screenNumber ? (CGDirectDisplayID)screenNumber.unsignedIntValue : CGMainDisplayID();
        __weak OPNStreamRecordingManager *weakSelf = self;
        [SCShareableContent getShareableContentExcludingDesktopWindows:YES onScreenWindowsOnly:YES completionHandler:^(SCShareableContent *content, NSError *error) {
            OPNStreamRecordingManager *strongSelf = weakSelf;
            if (!strongSelf || error || !content) {
                NSLog(@"[Recording] Audio capture unavailable: %@", error.localizedDescription ?: @"no shareable content");
                [strongSelf startAVMicrophoneCapture];
                return;
            }
            SCDisplay *display = content.displays.firstObject;
            for (SCDisplay *candidate in content.displays) {
                if (candidate.displayID == targetDisplayID) {
                    display = candidate;
                    break;
                }
            }
            if (!display) return;
            SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];
            SCStreamConfiguration *configuration = [[SCStreamConfiguration alloc] init];
            configuration.width = 2;
            configuration.height = 2;
            configuration.minimumFrameInterval = CMTimeMake(1, 1);
            configuration.queueDepth = 3;
            configuration.capturesAudio = YES;
            configuration.excludesCurrentProcessAudio = NO;
            configuration.sampleRate = 48000;
            configuration.channelCount = 2;
            if (@available(macOS 15.0, *)) {
                configuration.captureMicrophone = YES;
            }
            OPNRecordingScreenCaptureOutput *output = [[OPNRecordingScreenCaptureOutput alloc] init];
            output.manager = strongSelf;
            SCStream *stream = [[SCStream alloc] initWithFilter:filter configuration:configuration delegate:output];
            NSError *screenOutputError = nil;
            if (![stream addStreamOutput:output type:SCStreamOutputTypeScreen sampleHandlerQueue:strongSelf->_audioQueue error:&screenOutputError]) {
                NSLog(@"[Recording] Screen output sink failed: %@", screenOutputError.localizedDescription ?: @"unknown");
            }
            NSError *outputError = nil;
            if (![stream addStreamOutput:output type:SCStreamOutputTypeAudio sampleHandlerQueue:strongSelf->_audioQueue error:&outputError]) {
                NSLog(@"[Recording] System audio output failed: %@", outputError.localizedDescription ?: @"unknown");
            }
            if (@available(macOS 15.0, *)) {
                NSError *micError = nil;
                if (![stream addStreamOutput:output type:SCStreamOutputTypeMicrophone sampleHandlerQueue:strongSelf->_audioQueue error:&micError]) {
                    NSLog(@"[Recording] Microphone audio output failed: %@", micError.localizedDescription ?: @"unknown");
                    [strongSelf startAVMicrophoneCapture];
                }
            } else {
                [strongSelf startAVMicrophoneCapture];
            }
            strongSelf->_audioOutput = output;
            strongSelf->_audioStream = stream;
            [stream startCaptureWithCompletionHandler:^(NSError *startError) {
                if (startError) {
                    NSLog(@"[Recording] Audio capture start failed: %@", startError.localizedDescription ?: @"unknown");
                } else {
                    NSLog(@"[Recording] Audio capture started");
                }
            }];
        }];
    } else {
        [self startAVMicrophoneCapture];
    }
#else
    (void)window;
    [self startAVMicrophoneCapture];
#endif
}

- (void)stopAudioCapture {
#if OPN_HAVE_SCREENCAPTUREKIT
    if (@available(macOS 13.0, *)) {
        SCStream *stream = _audioStream;
        _audioStream = nil;
        _audioOutput = nil;
        [stream stopCaptureWithCompletionHandler:^(NSError *error) {
            if (error) NSLog(@"[Recording] Audio capture stop failed: %@", error.localizedDescription ?: @"unknown");
        }];
    }
#endif
}

- (void)startAVMicrophoneCapture {
    if (_microphoneCaptureSession) return;
    AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
    if (status == AVAuthorizationStatusDenied || status == AVAuthorizationStatusRestricted) {
        NSLog(@"[Recording] Microphone recording unavailable: permission denied");
        return;
    }
    if (status == AVAuthorizationStatusNotDetermined) {
        __weak OPNStreamRecordingManager *weakSelf = self;
        [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) {
            if (!granted) return;
            dispatch_async(dispatch_get_main_queue(), ^{
                [weakSelf startAVMicrophoneCapture];
            });
        }];
        return;
    }

    AVCaptureDevice *device = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
    if (!device) return;
    NSError *error = nil;
    AVCaptureDeviceInput *input = [AVCaptureDeviceInput deviceInputWithDevice:device error:&error];
    if (!input || error) {
        NSLog(@"[Recording] Microphone input failed: %@", error.localizedDescription ?: @"unknown");
        return;
    }
    AVCaptureAudioDataOutput *output = [[AVCaptureAudioDataOutput alloc] init];
    [output setSampleBufferDelegate:self queue:_audioQueue];
    AVCaptureSession *session = [[AVCaptureSession alloc] init];
    if (![session canAddInput:input] || ![session canAddOutput:output]) return;
    [session addInput:input];
    [session addOutput:output];
    _microphoneCaptureSession = session;
    [session startRunning];
    NSLog(@"[Recording] AVFoundation microphone capture started");
}

- (void)stopAVMicrophoneCapture {
    AVCaptureSession *session = _microphoneCaptureSession;
    _microphoneCaptureSession = nil;
    if (!session) return;
    dispatch_async(_audioQueue, ^{
        [session stopRunning];
    });
}

- (void)captureOutput:(AVCaptureOutput *)output didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer fromConnection:(AVCaptureConnection *)connection {
    (void)output;
    (void)connection;
    [self appendAudioSampleBuffer:sampleBuffer kind:OPNRecordingAudioKindMicrophone];
}

- (void)refreshRecentRecordings {
    NSURL *moviesURL = [NSFileManager.defaultManager URLsForDirectory:NSMoviesDirectory inDomains:NSUserDomainMask].firstObject;
    if (!moviesURL) {
        self.recentRecordingURLs = @[];
        return;
    }
    NSArray<NSURL *> *files = [NSFileManager.defaultManager contentsOfDirectoryAtURL:moviesURL includingPropertiesForKeys:@[NSURLContentModificationDateKey] options:NSDirectoryEnumerationSkipsHiddenFiles error:nil] ?: @[];
    NSMutableArray<NSURL *> *recordings = [NSMutableArray array];
    for (NSURL *url in files) {
        if (![url.lastPathComponent hasPrefix:@"OpenNOW-"] || ![url.pathExtension.lowercaseString isEqualToString:@"mp4"]) continue;
        [recordings addObject:url];
    }
    [recordings sortUsingComparator:^NSComparisonResult(NSURL *a, NSURL *b) {
        NSDate *dateA = nil;
        NSDate *dateB = nil;
        [a getResourceValue:&dateA forKey:NSURLContentModificationDateKey error:nil];
        [b getResourceValue:&dateB forKey:NSURLContentModificationDateKey error:nil];
        return [dateB ?: NSDate.distantPast compare:dateA ?: NSDate.distantPast];
    }];
    if (recordings.count > 6) {
        self.recentRecordingURLs = [recordings subarrayWithRange:NSMakeRange(0, 6)];
    } else {
        self.recentRecordingURLs = recordings;
    }
}

- (void)updateStatus:(NSString *)status starting:(BOOL)starting recording:(BOOL)recording notify:(BOOL)notify {
    self.statusText = status ?: @"Ready";
    self.starting = starting;
    self.recording = recording;
    if (notify && self.onStateChanged) self.onStateChanged();
}

static NSDictionary *OPNRecordingAudioSettings(NSInteger channels, NSInteger bitrate) {
    return @{
        AVFormatIDKey: @(kAudioFormatMPEG4AAC),
        AVSampleRateKey: @48000,
        AVNumberOfChannelsKey: @(channels),
        AVEncoderBitRateKey: @(bitrate),
    };
}

static NSString *OPNRecordingFilename(NSString *gameTitle) {
    NSString *title = gameTitle.length > 0 ? gameTitle : @"Stream";
    NSCharacterSet *allowed = [NSCharacterSet characterSetWithCharactersInString:@"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"];
    NSMutableString *safe = [NSMutableString string];
    for (NSUInteger i = 0; i < title.length; i++) {
        unichar c = [title characterAtIndex:i];
        if ([allowed characterIsMember:c]) {
            [safe appendFormat:@"%C", c];
        } else if (safe.length == 0 || ![[safe substringFromIndex:safe.length - 1] isEqualToString:@"-"]) {
            [safe appendString:@"-"];
        }
    }
    while ([safe hasSuffix:@"-"]) [safe deleteCharactersInRange:NSMakeRange(safe.length - 1, 1)];
    if (safe.length == 0) [safe appendString:@"Stream"];
    NSDateFormatter *formatter = [[NSDateFormatter alloc] init];
    formatter.locale = [NSLocale localeWithLocaleIdentifier:@"en_US_POSIX"];
    formatter.dateFormat = @"yyyyMMdd-HHmmss";
    return [NSString stringWithFormat:@"OpenNOW-%@-%@.mp4", safe, [formatter stringFromDate:NSDate.date]];
}

static CGSize OPNRecordingFrameSize(RTCVideoFrame *frame) {
#if defined(OPN_HAVE_LIBWEBRTC)
    if (!frame) return CGSizeZero;
    if (frame.rotation == RTCVideoRotation_90 || frame.rotation == RTCVideoRotation_270) {
        return CGSizeMake(frame.height, frame.width);
    }
    return CGSizeMake(frame.width, frame.height);
#else
    (void)frame;
    return CGSizeZero;
#endif
}

@end

#if OPN_HAVE_SCREENCAPTUREKIT
@implementation OPNRecordingScreenCaptureOutput

- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer ofType:(SCStreamOutputType)type {
    (void)stream;
    if (!sampleBuffer || !CMSampleBufferIsValid(sampleBuffer)) return;
    if (@available(macOS 13.0, *)) {
        if (type == SCStreamOutputTypeScreen) {
            return;
        }
        if (type == SCStreamOutputTypeAudio) {
            [self.manager appendAudioSampleBuffer:sampleBuffer kind:OPNRecordingAudioKindSystem];
        }
    }
    if (@available(macOS 15.0, *)) {
        if (type == SCStreamOutputTypeMicrophone) {
            [self.manager appendAudioSampleBuffer:sampleBuffer kind:OPNRecordingAudioKindMicrophone];
        }
    }
}

- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
    (void)stream;
    if (error) NSLog(@"[Recording] ScreenCaptureKit stopped: %@", error.localizedDescription ?: @"unknown");
}

@end
#endif
