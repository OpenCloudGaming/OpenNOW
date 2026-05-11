/*
 *  Copyright 2017 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import <Foundation/Foundation.h>

#import "RTCVideoCodecInfo.h"
#import "RTCVideoDecoder.h"
#import "sdk/objc/base/RTCMacros.h"

NS_ASSUME_NONNULL_BEGIN

RTC_OBJC_EXPORT
@interface RTC_OBJC_TYPE (RTCVideoDecoderVP8) : NSObject

/* This returns a VP8 decoder that can be returned from a RTCVideoDecoderFactory injected into
 * RTCPeerConnectionFactory. Even though it implements the RTCVideoDecoder protocol, it can not be
 * used independently from the RTCPeerConnectionFactory.
 */
+ (id<RTC_OBJC_TYPE(RTCVideoDecoder)>)vp8Decoder;

/** Returns the list of supported codec formats for the VP8 decoder. */
+ (NSArray<RTC_OBJC_TYPE(RTCVideoCodecInfo) *> *)supportedCodecs;

@end

NS_ASSUME_NONNULL_END
