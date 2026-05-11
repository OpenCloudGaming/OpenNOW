#include "OPNLogCapture.h"

#import <AppKit/AppKit.h>
#include <fcntl.h>
#include <unistd.h>

namespace OPN {

static NSString *OPNLogCapturePath() {
    NSString *directory = [NSTemporaryDirectory() stringByAppendingPathComponent:@"OpenNOW"];
    [NSFileManager.defaultManager createDirectoryAtPath:directory
                            withIntermediateDirectories:YES
                                             attributes:nil
                                                  error:nil];
    return [directory stringByAppendingPathComponent:@"OpenNOW-current.log"];
}

static void OPNAppendDataToLog(NSData *data) {
    if (data.length == 0) return;
    NSString *path = OPNLogCapturePath();
    if (![NSFileManager.defaultManager fileExistsAtPath:path]) {
        [data writeToFile:path atomically:YES];
        return;
    }
    NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
    if (!handle) return;
    @try {
        [handle seekToEndOfFile];
        [handle writeData:data];
    } @catch (__unused NSException *exception) {
    }
    [handle closeFile];
}

void StartLogCapture() {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        NSString *path = OPNLogCapturePath();
        NSString *header = [NSString stringWithFormat:@"=== OpenNOW log started %@ ===\n", NSDate.date];
        [header writeToFile:path atomically:YES encoding:NSUTF8StringEncoding error:nil];

        int pipeFds[2] = {-1, -1};
        if (pipe(pipeFds) != 0) return;

        int stdoutCopy = dup(STDOUT_FILENO);
        if (stdoutCopy < 0) {
            close(pipeFds[0]);
            close(pipeFds[1]);
            return;
        }

        fflush(stdout);
        fflush(stderr);
        dup2(pipeFds[1], STDOUT_FILENO);
        dup2(pipeFds[1], STDERR_FILENO);
        close(pipeFds[1]);
        int readFd = pipeFds[0];

        dispatch_queue_t queue = dispatch_queue_create("com.opennow.logcapture", DISPATCH_QUEUE_SERIAL);
        dispatch_async(queue, ^{
            char buffer[4096];
            for (;;) {
                ssize_t bytesRead = read(readFd, buffer, sizeof(buffer));
                if (bytesRead <= 0) break;
                NSData *data = [NSData dataWithBytes:buffer length:(NSUInteger)bytesRead];
                OPNAppendDataToLog(data);
                ssize_t written = 0;
                while (written < bytesRead) {
                    ssize_t n = write(stdoutCopy, buffer + written, (size_t)(bytesRead - written));
                    if (n <= 0) break;
                    written += n;
                }
            }
            close(readFd);
            close(stdoutCopy);
        });
    });
}

void AppendLogEvent(NSString *message) {
    if (message.length == 0) return;
    NSString *line = [NSString stringWithFormat:@"%@ %@\n", NSDate.date, message];
    NSData *data = [line dataUsingEncoding:NSUTF8StringEncoding];
    OPNAppendDataToLog(data);
}

void CopyCapturedLogToClipboard(NSString *reason) {
    if (reason.length > 0) {
        AppendLogEvent([NSString stringWithFormat:@"[Clipboard] Copying log to clipboard: %@", reason]);
    }

    NSString *path = OPNLogCapturePath();
    NSString *log = [NSString stringWithContentsOfFile:path encoding:NSUTF8StringEncoding error:nil];
    if (log.length == 0) {
        log = reason.length > 0 ? reason : @"OpenNOW log copy requested, but no captured log was available.";
    }

    NSPasteboard *pasteboard = NSPasteboard.generalPasteboard;
    [pasteboard clearContents];
    [pasteboard setString:log forType:NSPasteboardTypeString];
    NSLog(@"[LogCapture] Copied captured log to clipboard (%lu chars)", (unsigned long)log.length);
}

NSString *CapturedLogPath() {
    return OPNLogCapturePath();
}

}
