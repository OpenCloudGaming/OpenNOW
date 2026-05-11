/*
 *  Copyright 2025 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import "AudioTimeStamp+Nanoseconds.h"

#include <mach/mach_time.h>
#include "rtc_base/checks.h"

namespace webrtc {

std::optional<int64_t> AudioTimeStampGetNanoseconds(
    const AudioTimeStamp* timeStamp) {
  if (!timeStamp || ((timeStamp->mFlags & kAudioTimeStampHostTimeValid) == 0) ||
      timeStamp->mHostTime == 0) {
    return std::nullopt;
  }

  static mach_timebase_info_data_t mach_timebase;
  if (mach_timebase.denom == 0) {
    if (mach_timebase_info(&mach_timebase) != KERN_SUCCESS) {
      RTC_DCHECK_NOTREACHED() << "mach_timebase_info bad return code";
      return std::nullopt;
    }
  }

  if (mach_timebase.denom == 0 || mach_timebase.numer == 0) {
    RTC_DCHECK_NOTREACHED() << "mach_timebase_info provided bad data";
    return std::nullopt;
  }

  uint64_t time_scaled = 0;
  if (__builtin_umulll_overflow(
          timeStamp->mHostTime, mach_timebase.numer, &time_scaled)) {
    RTC_DCHECK_NOTREACHED() << "numeric overflow computing scaled host time";
    return std::nullopt;
  }

  uint64_t nanoseconds = time_scaled / mach_timebase.denom;
  if (nanoseconds >
      static_cast<uint64_t>(std::numeric_limits<int64_t>::max())) {
    RTC_DCHECK_NOTREACHED() << "numeric overflow computing nanoseconds";
    return std::nullopt;
  }

  return static_cast<int64_t>(nanoseconds);
}

}  // namespace webrtc
