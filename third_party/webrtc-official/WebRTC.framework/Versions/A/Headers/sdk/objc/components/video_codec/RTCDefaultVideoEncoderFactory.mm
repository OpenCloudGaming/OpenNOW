/*
 *  Copyright 2017 The WebRTC Project Authors. All rights reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import "RTCDefaultVideoEncoderFactory.h"

#import "RTCH264ProfileLevelId.h"
#import "RTCVideoEncoderH264.h"
#import "api/peerconnection/RTCVideoCodecInfo+Private.h"
#import "api/video_codec/RTCVideoCodecConstants.h"
#import "api/video_codec/RTCVideoEncoderVP8.h"
#import "api/video_codec/RTCVideoEncoderVP9.h"
#import "base/RTCVideoCodecInfo.h"
#import "helpers/NSString+StdString.h"

#include "absl/algorithm/container.h"
#include "api/video_codecs/scalability_mode_helper.h"

#if defined(RTC_USE_LIBAOM_AV1_ENCODER)
#import "api/video_codec/RTCVideoEncoderAV1.h"  // nogncheck
#endif

@implementation RTC_OBJC_TYPE (RTCDefaultVideoEncoderFactory)

@synthesize preferredCodec;

+ (NSArray<RTC_OBJC_TYPE(RTCVideoCodecInfo) *> *)supportedCodecs {
  NSMutableArray<RTC_OBJC_TYPE(RTCVideoCodecInfo) *> *result =
      [NSMutableArray array];

  [result
      addObjectsFromArray:[RTC_OBJC_TYPE(RTCVideoEncoderH264) supportedCodecs]];
  [result
      addObjectsFromArray:[RTC_OBJC_TYPE(RTCVideoEncoderVP8) supportedCodecs]];

  [result
      addObjectsFromArray:[RTC_OBJC_TYPE(RTCVideoEncoderVP9) supportedCodecs]];

#if defined(RTC_USE_LIBAOM_AV1_ENCODER)
  [result
      addObjectsFromArray:[RTC_OBJC_TYPE(RTCVideoEncoderAV1) supportedCodecs]];
#endif

  return result;
}

- (id<RTC_OBJC_TYPE(RTCVideoEncoder)>)createEncoder:
    (RTC_OBJC_TYPE(RTCVideoCodecInfo) *)info {
  if ([info.name isEqualToString:kRTCVideoCodecH264Name]) {
    return [[RTC_OBJC_TYPE(RTCVideoEncoderH264) alloc] initWithCodecInfo:info];
  } else if ([info.name isEqualToString:kRTCVideoCodecVp8Name]) {
    return [RTC_OBJC_TYPE(RTCVideoEncoderVP8) vp8Encoder];
  } else if ([info.name isEqualToString:kRTCVideoCodecVp9Name] &&
             [RTC_OBJC_TYPE(RTCVideoEncoderVP9) isSupported]) {
    return [RTC_OBJC_TYPE(RTCVideoEncoderVP9) vp9Encoder];
  }

#if defined(RTC_USE_LIBAOM_AV1_ENCODER)
  if ([info.name isEqualToString:kRTCVideoCodecAv1Name]) {
    return [RTC_OBJC_TYPE(RTCVideoEncoderAV1) av1Encoder];
  }
#endif

  return nil;
}

- (NSArray<RTC_OBJC_TYPE(RTCVideoCodecInfo) *> *)supportedCodecs {
  NSMutableArray<RTC_OBJC_TYPE(RTCVideoCodecInfo) *> *codecs =
      [[[self class] supportedCodecs] mutableCopy];

  NSMutableArray<RTC_OBJC_TYPE(RTCVideoCodecInfo) *> *orderedCodecs =
      [NSMutableArray array];
  NSUInteger index = [codecs indexOfObject:self.preferredCodec];
  if (index != NSNotFound) {
    [orderedCodecs addObject:[codecs objectAtIndex:index]];
    [codecs removeObjectAtIndex:index];
  }
  [orderedCodecs addObjectsFromArray:codecs];

  return [orderedCodecs copy];
}

- (RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) *)
    queryCodecSupport:(RTC_OBJC_TYPE(RTCVideoCodecInfo) *)info
      scalabilityMode:(nullable NSString *)scalabilityMode {
  std::optional<webrtc::ScalabilityMode> mode;
  if (scalabilityMode) {
    mode = webrtc::ScalabilityModeStringToEnum([scalabilityMode stdString]);
    if (!mode.has_value()) {
      return [[RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) alloc]
          initWithSupported:NO];
    }
  }
  webrtc::SdpVideoFormat format = [info nativeSdpVideoFormat];
  for (RTC_OBJC_TYPE(RTCVideoCodecInfo) *
       supportedCodec in [[self class] supportedCodecs]) {
    webrtc::SdpVideoFormat supportedFormat =
        [supportedCodec nativeSdpVideoFormat];
    if (!format.IsSameCodec(supportedFormat)) {
      continue;
    }
    if (!mode.has_value()) {
      return [[RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) alloc]
          initWithSupported:YES];
    }
    bool modeSupported =
        absl::c_linear_search(supportedFormat.scalability_modes, *mode);
    return [[RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) alloc]
        initWithSupported:modeSupported];
  }
  return
      [[RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) alloc] initWithSupported:NO];
}

@end
