/*
 *  Copyright 2017 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import "RTCVideoDecoderFactoryH264.h"

#import "RTCH264ProfileLevelId.h"
#import "RTCVideoDecoderH264.h"

@implementation RTC_OBJC_TYPE (RTCVideoDecoderFactoryH264)

- (NSArray<RTC_OBJC_TYPE(RTCVideoCodecInfo) *> *)supportedCodecs {
  return [RTC_OBJC_TYPE(RTCVideoDecoderH264) supportedCodecs];
}

- (id<RTC_OBJC_TYPE(RTCVideoDecoder)>)createDecoder:
    (RTC_OBJC_TYPE(RTCVideoCodecInfo) *)info {
  return [[RTC_OBJC_TYPE(RTCVideoDecoderH264) alloc] init];
}

@end
