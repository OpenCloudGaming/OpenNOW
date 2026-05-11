#pragma once

#import <Foundation/Foundation.h>

namespace OPN {

void StartLogCapture();
void AppendLogEvent(NSString *message);
void CopyCapturedLogToClipboard(NSString *reason);
NSString *CapturedLogPath();

}
