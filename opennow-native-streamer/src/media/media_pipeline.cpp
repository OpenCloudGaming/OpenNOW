#include "opennow/native/media_pipeline.hpp"

#include <algorithm>
#include <cstring>

#if defined(OPENNOW_HAS_FFMPEG)
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/channel_layout.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>
}
#endif

namespace opennow::native {

MediaPipeline::~MediaPipeline() {
  Shutdown();
}

void MediaPipeline::SetLogger(LogFn logger) {
  logger_ = std::move(logger);
}

bool MediaPipeline::Initialize(SDL_Renderer* renderer, std::string& error) {
  renderer_ = renderer;
#if !defined(OPENNOW_HAS_SDL3) || !defined(OPENNOW_HAS_FFMPEG)
  (void)error;
#endif
#if defined(OPENNOW_HAS_SDL3)
  SDL_AudioSpec desired{};
  desired.channels = 2;
  desired.format = SDL_AUDIO_S16;
  desired.freq = 48000;
  audio_stream_ = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &desired, nullptr, nullptr);
  if (!audio_stream_) {
    error = SDL_GetError();
    return false;
  }
  SDL_ResumeAudioStreamDevice(audio_stream_);
#endif
#if defined(OPENNOW_HAS_FFMPEG)
  packet_ = av_packet_alloc();
  video_frame_ = av_frame_alloc();
  audio_frame_ = av_frame_alloc();
  if (!packet_ || !video_frame_ || !audio_frame_) {
    error = "Failed to allocate FFmpeg frame buffers";
    return false;
  }
#endif
  return true;
}

void MediaPipeline::Shutdown() {
#if defined(OPENNOW_HAS_SDL3)
  if (video_texture_) {
    SDL_DestroyTexture(video_texture_);
    video_texture_ = nullptr;
  }
  if (audio_stream_) {
    SDL_DestroyAudioStream(audio_stream_);
    audio_stream_ = nullptr;
  }
#endif
#if defined(OPENNOW_HAS_FFMPEG)
  if (sws_context_) {
    sws_freeContext(sws_context_);
    sws_context_ = nullptr;
  }
  if (swr_context_) {
    swr_free(&swr_context_);
  }
  if (video_decoder_ctx_) {
    avcodec_free_context(&video_decoder_ctx_);
  }
  if (audio_decoder_ctx_) {
    avcodec_free_context(&audio_decoder_ctx_);
  }
  if (packet_) {
    av_packet_free(&packet_);
  }
  if (video_frame_) {
    av_frame_free(&video_frame_);
  }
  if (audio_frame_) {
    av_frame_free(&audio_frame_);
  }
#endif
}

void MediaPipeline::ConfigureVideoCodec(const std::string& codec) {
  video_codec_ = codec;
}

void MediaPipeline::ConfigureAudioCodec(const std::string& codec, int payload_type, int clock_rate, int channels) {
  audio_codec_ = codec;
  audio_payload_type_ = payload_type;
  audio_clock_rate_ = clock_rate;
  audio_channels_ = channels;
}

void MediaPipeline::PushVideoFrame(std::vector<std::uint8_t> encoded_frame, std::uint64_t) {
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  DecodeVideoFrame(encoded_frame);
#else
  (void)encoded_frame;
#endif
}

void MediaPipeline::PushAudioFrame(std::vector<std::uint8_t> encoded_frame, std::uint64_t) {
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  DecodeAudioFrame(encoded_frame);
#else
  (void)encoded_frame;
#endif
}

void MediaPipeline::RenderFrame() {
#if defined(OPENNOW_HAS_SDL3)
  std::optional<PendingVideoFrame> pending_frame;
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    if (pending_video_frame_) {
      pending_frame = std::move(pending_video_frame_);
      pending_video_frame_.reset();
    }
  }
  if (pending_frame) {
    UploadPendingFrame(*pending_frame);
  }
  if (video_texture_ && renderer_) {
    SDL_RenderTexture(renderer_, video_texture_, nullptr, nullptr);
  }
#endif
}

std::string MediaPipeline::DescribeCapabilities() const {
#if defined(OPENNOW_HAS_FFMPEG) && defined(OPENNOW_HAS_SDL3)
  return "FFmpeg decode + SDL3 render/audio available";
#elif defined(OPENNOW_HAS_FFMPEG)
  return "FFmpeg decode available";
#else
  return "FFmpeg decode pipeline unavailable";
#endif
}

void MediaPipeline::Log(const std::string& message) const {
  if (logger_) {
    logger_(message);
  }
}

#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
bool MediaPipeline::EnsureVideoDecoder(std::string& error) {
  if (video_decoder_ctx_) {
    return true;
  }
  const AVCodec* codec = nullptr;
  if (video_codec_ == "H265" || video_codec_ == "HEVC") {
    codec = avcodec_find_decoder(AV_CODEC_ID_HEVC);
  } else if (video_codec_ == "AV1") {
    codec = avcodec_find_decoder(AV_CODEC_ID_AV1);
  } else {
    codec = avcodec_find_decoder(AV_CODEC_ID_H264);
  }
  if (!codec) {
    error = "Requested video decoder is unavailable in FFmpeg";
    return false;
  }
  video_decoder_ctx_ = avcodec_alloc_context3(codec);
  if (!video_decoder_ctx_) {
    error = "Failed to allocate video decoder context";
    return false;
  }
  if (avcodec_open2(video_decoder_ctx_, codec, nullptr) < 0) {
    error = "Failed to open video decoder";
    return false;
  }
  return true;
}

bool MediaPipeline::EnsureAudioDecoder(std::string& error) {
  if (audio_decoder_ctx_) {
    return true;
  }
  const AVCodec* codec = avcodec_find_decoder(AV_CODEC_ID_OPUS);
  if (!codec) {
    error = "Opus decoder unavailable";
    return false;
  }
  audio_decoder_ctx_ = avcodec_alloc_context3(codec);
  if (!audio_decoder_ctx_) {
    error = "Failed to allocate audio decoder context";
    return false;
  }
  audio_decoder_ctx_->sample_rate = audio_clock_rate_;
  av_channel_layout_default(&audio_decoder_ctx_->ch_layout, audio_channels_);
  if (avcodec_open2(audio_decoder_ctx_, codec, nullptr) < 0) {
    error = "Failed to open audio decoder";
    return false;
  }
  return true;
}

void MediaPipeline::DecodeVideoFrame(const std::vector<std::uint8_t>& encoded_frame) {
  std::string error;
  if (!EnsureVideoDecoder(error)) {
    Log(error);
    return;
  }
  av_packet_unref(packet_);
  packet_->data = const_cast<std::uint8_t*>(encoded_frame.data());
  packet_->size = static_cast<int>(encoded_frame.size());
  if (avcodec_send_packet(video_decoder_ctx_, packet_) < 0) {
    return;
  }
  while (avcodec_receive_frame(video_decoder_ctx_, video_frame_) == 0) {
    StageFrame(video_frame_);
    rendered_frames_ += 1;
  }
}

void MediaPipeline::DecodeAudioFrame(const std::vector<std::uint8_t>& encoded_frame) {
  std::string error;
  if (!EnsureAudioDecoder(error)) {
    Log(error);
    return;
  }
  av_packet_unref(packet_);
  packet_->data = const_cast<std::uint8_t*>(encoded_frame.data());
  packet_->size = static_cast<int>(encoded_frame.size());
  if (avcodec_send_packet(audio_decoder_ctx_, packet_) < 0) {
    return;
  }
  while (avcodec_receive_frame(audio_decoder_ctx_, audio_frame_) == 0) {
    if (!swr_context_) {
      swr_alloc_set_opts2(
          &swr_context_,
          nullptr,
          AV_SAMPLE_FMT_S16,
          48000,
          &audio_frame_->ch_layout,
          static_cast<AVSampleFormat>(audio_frame_->format),
          audio_frame_->sample_rate,
          0,
          nullptr);
      swr_init(swr_context_);
    }
    const int out_samples = swr_get_out_samples(swr_context_, audio_frame_->nb_samples);
    std::vector<std::uint8_t> pcm(static_cast<std::size_t>(out_samples * 2 * 2));
    std::uint8_t* out[] = {pcm.data()};
    const int samples = swr_convert(
        swr_context_,
        out,
        audio_frame_->nb_samples,
        const_cast<const std::uint8_t**>(audio_frame_->extended_data),
        audio_frame_->nb_samples);
    if (samples > 0 && audio_stream_) {
      SDL_PutAudioStreamData(audio_stream_, pcm.data(), samples * 2 * 2);
    }
  }
}

void MediaPipeline::StageFrame(AVFrame* frame) {
  if (!renderer_) {
    return;
  }
  sws_context_ = sws_getCachedContext(
      sws_context_,
      frame->width,
      frame->height,
      static_cast<AVPixelFormat>(frame->format),
      frame->width,
      frame->height,
      AV_PIX_FMT_RGBA,
      SWS_BILINEAR,
      nullptr,
      nullptr,
      nullptr);
  if (!sws_context_) {
    Log("Failed to create FFmpeg swscale context for staging video frame");
    return;
  }
  PendingVideoFrame pending;
  pending.width = frame->width;
  pending.height = frame->height;
  pending.stride = frame->width * 4;
  pending.rgba.resize(static_cast<std::size_t>(pending.stride * pending.height));
  std::uint8_t* dst_data[4] = {pending.rgba.data(), nullptr, nullptr, nullptr};
  int dst_linesize[4] = {pending.stride, 0, 0, 0};
  sws_scale(sws_context_, frame->data, frame->linesize, 0, frame->height, dst_data, dst_linesize);
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    pending_video_frame_ = std::move(pending);
  }
  Log("Staged decoded video frame on worker thread");
}

void MediaPipeline::UploadPendingFrame(const PendingVideoFrame& frame) {
  if (!renderer_) {
    return;
  }
  if (!video_texture_ || texture_width_ != frame.width || texture_height_ != frame.height) {
    if (video_texture_) {
      SDL_DestroyTexture(video_texture_);
      video_texture_ = nullptr;
    }
    texture_width_ = frame.width;
    texture_height_ = frame.height;
    video_texture_ = SDL_CreateTexture(renderer_, SDL_PIXELFORMAT_RGBA32, SDL_TEXTUREACCESS_STREAMING, texture_width_, texture_height_);
    if (!video_texture_) {
      Log("Failed to create SDL streaming texture on render thread");
      return;
    }
    Log("Created/updated SDL video texture on render thread");
  }
  if (!SDL_UpdateTexture(video_texture_, nullptr, frame.rgba.data(), frame.stride)) {
    Log(SDL_GetError());
    return;
  }
  Log("Uploaded staged video frame on render thread");
}
#endif

}  // namespace opennow::native
