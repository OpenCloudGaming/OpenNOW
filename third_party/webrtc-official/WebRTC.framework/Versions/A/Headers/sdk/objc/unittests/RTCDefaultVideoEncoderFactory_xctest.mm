/*
 *  Copyright 2026 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import <XCTest/XCTest.h>

#include "test/gtest.h"

#import "base/RTCVideoCodecInfo.h"
#import "base/RTCVideoEncoderFactory.h"
#import "components/video_codec/RTCDefaultVideoEncoderFactory.h"

@interface RTCDefaultVideoEncoderFactoryTests : XCTestCase
@end

@implementation RTCDefaultVideoEncoderFactoryTests

- (void)testQueryCodecSupportNoScalabilityMode {
  RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) *factory =
      [[RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) alloc] init];
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *vp8 =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc] initWithName:@"VP8"
                                                  parameters:@{}];

  RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) *result =
      [factory queryCodecSupport:vp8 scalabilityMode:nil];

  EXPECT_TRUE(result.isSupported);
}

- (void)testQueryCodecSupportUnsupportedCodec {
  RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) *factory =
      [[RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) alloc] init];
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *unsupported =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc] initWithName:@"UnsupportedCodec"
                                                  parameters:@{}];

  RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) *result =
      [factory queryCodecSupport:unsupported scalabilityMode:nil];

  EXPECT_FALSE(result.isSupported);
}

- (void)testQueryCodecSupportWithMatchingScalabilityMode {
  RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) *factory =
      [[RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) alloc] init];
  // VP8 encoder advertises scalability modes including L1T2.
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *vp8 =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc] initWithName:@"VP8"
                                                  parameters:@{}];

  RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) *result =
      [factory queryCodecSupport:vp8 scalabilityMode:@"L1T2"];

  EXPECT_TRUE(result.isSupported);
}

- (void)testQueryCodecSupportWithNonMatchingScalabilityMode {
  RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) *factory =
      [[RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) alloc] init];
  // VP8 encoder does not support spatial scalability modes like L3T3.
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *vp8 =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc] initWithName:@"VP8"
                                                  parameters:@{}];

  RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) *result =
      [factory queryCodecSupport:vp8 scalabilityMode:@"L3T3"];

  EXPECT_FALSE(result.isSupported);
}

- (void)testQueryCodecSupportWithInvalidScalabilityMode {
  RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) *factory =
      [[RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) alloc] init];
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *vp8 =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc] initWithName:@"VP8"
                                                  parameters:@{}];

  RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) *result =
      [factory queryCodecSupport:vp8 scalabilityMode:@"INVALID"];

  EXPECT_FALSE(result.isSupported);
}

#if defined(RTC_ENABLE_VP9)
- (void)testQueryCodecSupportDistinguishesVP9Profiles {
  RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) *factory =
      [[RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) alloc] init];
  // VP9 profile 0 is always supported when VP9 is enabled.
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *vp9Profile0 =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc]
          initWithName:@"VP9"
            parameters:@{@"profile-id" : @"0"}];
  // VP9 profile 1 is not in the encoder's supported codecs list.
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *vp9Profile1 =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc]
          initWithName:@"VP9"
            parameters:@{@"profile-id" : @"1"}];

  EXPECT_TRUE([factory queryCodecSupport:vp9Profile0 scalabilityMode:@"L1T2"]
                  .isSupported);
  EXPECT_FALSE([factory queryCodecSupport:vp9Profile1 scalabilityMode:@"L1T2"]
                   .isSupported);
}
#endif

- (void)testQueryCodecSupportScalabilityModeUnsupportedCodec {
  RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) *factory =
      [[RTC_OBJC_TYPE(RTCDefaultVideoEncoderFactory) alloc] init];
  RTC_OBJC_TYPE(RTCVideoCodecInfo) *unsupported =
      [[RTC_OBJC_TYPE(RTCVideoCodecInfo) alloc] initWithName:@"UnsupportedCodec"
                                                  parameters:@{}];

  RTC_OBJC_TYPE(RTCVideoEncoderCodecSupport) *result =
      [factory queryCodecSupport:unsupported scalabilityMode:@"L1T2"];

  EXPECT_FALSE(result.isSupported);
}

@end
