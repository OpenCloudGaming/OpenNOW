/*
 *  Copyright 2018 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#include "audio_device_module.h"

#include "api/environment/environment.h"
#include "api/make_ref_counted.h"
#include "rtc_base/logging.h"
#include "sdk/objc/native/src/audio/audio_device_module_ios.h"

#ifndef WEBRTC_IOS
#error This file shouldn't be compiled on platforms other than IOS.
#endif

namespace webrtc {

scoped_refptr<AudioDeviceModule> CreateAudioDeviceModule(
    const Environment& env, bool bypass_voice_processing) {
  RTC_DLOG(LS_INFO) << __FUNCTION__;
  return make_ref_counted<ios_adm::AudioDeviceModuleIOS>(
      env,
      bypass_voice_processing,
      /*muted_speech_event_handler=*/nullptr,
      /*error_handler=*/nullptr);
}

scoped_refptr<AudioDeviceModule> CreateMutedDetectAudioDeviceModule(
    const Environment& env,
    AudioDeviceModule::MutedSpeechEventHandler muted_speech_event_handler,
    ADMErrorHandler error_handler,
    bool bypass_voice_processing) {
  RTC_DLOG(LS_INFO) << __FUNCTION__;
  return make_ref_counted<ios_adm::AudioDeviceModuleIOS>(
      env, bypass_voice_processing, muted_speech_event_handler, error_handler);
}

}  // namespace webrtc
