/*
 *  Copyright 2025 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import <CoreAudioTypes/CoreAudioTypes.h>

#include <optional>

namespace webrtc {

std::optional<int64_t> AudioTimeStampGetNanoseconds(
    const AudioTimeStamp* timeStamp);

}  // namespace webrtc
