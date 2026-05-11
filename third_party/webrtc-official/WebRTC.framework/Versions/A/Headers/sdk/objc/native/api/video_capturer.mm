/*
 *  Copyright 2018 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#include "sdk/objc/native/api/video_capturer.h"

#include "absl/memory/memory.h"
#include "api/environment/environment.h"
#include "api/scoped_refptr.h"
#include "api/video_track_source_proxy_factory.h"
#include "sdk/objc/native/src/objc_video_track_source.h"

namespace webrtc {

scoped_refptr<VideoTrackSourceInterface> ObjCToNativeVideoCapturer(
    RTC_OBJC_TYPE(RTCVideoCapturer) * objc_video_capturer,
    const Environment &env,
    Thread *signaling_thread,
    Thread *worker_thread) {
  RTCObjCVideoSourceAdapter *adapter = [[RTCObjCVideoSourceAdapter alloc] init];
  scoped_refptr<ObjCVideoTrackSource> objc_video_track_source =
      make_ref_counted<ObjCVideoTrackSource>(env, adapter);
  scoped_refptr<VideoTrackSourceInterface> video_source =
      CreateVideoTrackSourceProxy(
          signaling_thread, worker_thread, objc_video_track_source.get());

  objc_video_capturer.delegate = adapter;

  return video_source;
}

}  // namespace webrtc
