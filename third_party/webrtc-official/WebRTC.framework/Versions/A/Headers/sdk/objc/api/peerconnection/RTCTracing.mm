/*
 *  Copyright 2016 The WebRTC Project Authors. All rights reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import "RTCTracing.h"

#include "api/environment/environment.h"
#include "api/environment/environment_factory.h"
#include "rtc_base/event_tracer.h"

void RTCSetupInternalTracer(void) {
  webrtc::Environment env = webrtc::CreateEnvironment();
  webrtc::tracing::SetupInternalTracer(env);
}

BOOL RTCStartInternalCapture(NSString *filePath) {
  return webrtc::tracing::StartInternalCapture(filePath.UTF8String);
}

void RTCStopInternalCapture(void) {
  webrtc::tracing::StopInternalCapture();
}

void RTCShutdownInternalTracer(void) {
  webrtc::tracing::ShutdownInternalTracer();
}
