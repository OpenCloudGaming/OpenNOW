/*
 *  Copyright 2018 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#import "RTCPeerConnectionFactoryBuilder.h"
#import "RTCPeerConnectionFactory+Native.h"

#include "api/audio/audio_device.h"
#include "api/audio/audio_processing.h"
#include "api/audio_codecs/audio_decoder_factory.h"
#include "api/audio_codecs/audio_encoder_factory.h"
#include "api/environment/environment_factory.h"
#include "api/video_codecs/video_decoder_factory.h"
#include "api/video_codecs/video_encoder_factory.h"

@implementation RTCPeerConnectionFactoryBuilder {
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> (^_audioDeviceModuleBuilder)(
      const webrtc::Environment &);
  webrtc::PeerConnectionFactoryDependencies _dependencies;
}

+ (RTCPeerConnectionFactoryBuilder *)builder {
  return [[RTCPeerConnectionFactoryBuilder alloc] init];
}

- (RTC_OBJC_TYPE(RTCPeerConnectionFactory) *)createPeerConnectionFactory {
  if (_audioDeviceModuleBuilder != nil) {
    if (!_dependencies.env.has_value()) {
      _dependencies.env = webrtc::CreateEnvironment();
    }
    _dependencies.adm = _audioDeviceModuleBuilder(*_dependencies.env);
  }
  return [[RTC_OBJC_TYPE(RTCPeerConnectionFactory) alloc]
      initWithMediaAndDependencies:_dependencies];
}

- (void)setFieldTrials:(std::unique_ptr<webrtc::FieldTrialsView>)fieldTrials {
  _dependencies.env = webrtc::CreateEnvironment(std::move(fieldTrials));
}

- (void)setVideoEncoderFactory:
    (std::unique_ptr<webrtc::VideoEncoderFactory>)videoEncoderFactory {
  _dependencies.video_encoder_factory = std::move(videoEncoderFactory);
}

- (void)setVideoDecoderFactory:
    (std::unique_ptr<webrtc::VideoDecoderFactory>)videoDecoderFactory {
  _dependencies.video_decoder_factory = std::move(videoDecoderFactory);
}

- (void)setAudioEncoderFactory:
    (webrtc::scoped_refptr<webrtc::AudioEncoderFactory>)audioEncoderFactory {
  _dependencies.audio_encoder_factory = std::move(audioEncoderFactory);
}

- (void)setAudioDecoderFactory:
    (webrtc::scoped_refptr<webrtc::AudioDecoderFactory>)audioDecoderFactory {
  _dependencies.audio_decoder_factory = std::move(audioDecoderFactory);
}

- (void)setAudioDeviceModule:
    (webrtc::scoped_refptr<webrtc::AudioDeviceModule>)audioDeviceModule {
  _audioDeviceModuleBuilder = ^(const webrtc::Environment &) {
    return audioDeviceModule;
  };
}

- (void)setAudioDeviceModuleBuilder:
    (webrtc::scoped_refptr<webrtc::AudioDeviceModule> (^)(
        const webrtc::Environment &))audioDeviceModuleBuilder {
  _audioDeviceModuleBuilder = audioDeviceModuleBuilder;
}

- (void)setAudioProcessingModule:
    (webrtc::scoped_refptr<webrtc::AudioProcessing>)audioProcessingModule {
  _dependencies.audio_processing_builder =
      CustomAudioProcessing(std::move(audioProcessingModule));
}

- (void)setAudioProcessingBuilder:
    (std::unique_ptr<webrtc::AudioProcessingBuilderInterface>)
        audioProcessingBuilder {
  _dependencies.audio_processing_builder = std::move(audioProcessingBuilder);
}

@end
