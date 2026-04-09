#include "opennow/native/media_pipeline.hpp"

#include <algorithm>
#include <cstring>
#include <sstream>

#include "opennow/native/input_protocol.hpp"
#include "opennow/native/macos_surface_renderer.hpp"
#include "opennow/native/video_decoder_selector.hpp"

#if defined(OPENNOW_HAS_FFMPEG)
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/channel_layout.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libavutil/log.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>
}
#endif

namespace opennow::native {

namespace {
constexpr std::uint64_t kMaxPacketsWithoutDecodedFrameBeforeReset = 30;
constexpr std::uint64_t kMaxHardwareDecodeResetsBeforeSoftwareFallback = 2;
constexpr std::size_t kMinBufferedVideoFrames = 1;
constexpr std::size_t kMaxBufferedVideoFrames = 2;
constexpr std::uint64_t kPacingTuneIntervalUs = 500000;
}

#if defined(OPENNOW_HAS_FFMPEG)
namespace {
std::mutex g_ffmpeg_log_mutex;
bool g_suppressed_deprecated_pixel_format_warning = false;
constexpr std::size_t kMaxPendingVideoFrames = 3;

void OpenNowFfmpegLogCallback(void* avcl, int level, const char* fmt, va_list args) {
  if (fmt != nullptr && std::strstr(fmt, "deprecated pixel format used") != nullptr) {
    std::lock_guard<std::mutex> lock(g_ffmpeg_log_mutex);
    if (g_suppressed_deprecated_pixel_format_warning) {
      return;
    }
    g_suppressed_deprecated_pixel_format_warning = true;
  }
  av_log_default_callback(avcl, level, fmt, args);
}

const AVCodec* FindVideoDecoder(const std::string& codec) {
  if (codec == "H265" || codec == "HEVC") {
    return avcodec_find_decoder(AV_CODEC_ID_HEVC);
  }
  if (codec == "AV1") {
    return avcodec_find_decoder(AV_CODEC_ID_AV1);
  }
  return avcodec_find_decoder(AV_CODEC_ID_H264);
}

bool IsPlanarYuv420(enum AVPixelFormat format) {
  return format == AV_PIX_FMT_YUV420P || format == AV_PIX_FMT_YUVJ420P;
}

bool IsNv12Like(enum AVPixelFormat format) {
  return format == AV_PIX_FMT_NV12 || format == AV_PIX_FMT_NV21;
}
}  // namespace
#endif

MediaPipeline::~MediaPipeline() {
  Shutdown();
}

void MediaPipeline::SetLogger(LogFn logger) {
  logger_ = std::move(logger);
}

bool MediaPipeline::Initialize(SDL_Window* window, SDL_Renderer* renderer, std::string& error) {
  window_ = window;
  renderer_ = renderer;
#if !defined(OPENNOW_HAS_SDL3) || !defined(OPENNOW_HAS_FFMPEG)
  (void)error;
#endif
#if defined(OPENNOW_HAS_SDL3)
#if defined(__APPLE__)
  macos_surface_renderer_ = std::make_unique<MacosSurfaceRenderer>();
  std::string macos_surface_error;
  if (macos_surface_renderer_ && macos_surface_renderer_->Initialize(window_, macos_surface_error)) {
    using_native_surface_present_ = macos_surface_renderer_->SupportsNativeSurfacePath();
    if (using_native_surface_present_) {
      LogVideoPath("video path: macOS VideoToolbox + Metal/CVPixelBuffer direct presentation");
    }
  } else if (!macos_surface_error.empty()) {
    Log(std::string("macOS native surface presenter unavailable, using staged upload fallback: ") + macos_surface_error);
  }
#endif

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
  if (!SDL_SetRenderVSync(renderer_, 1)) {
    Log(std::string("Renderer VSync request failed: ") + SDL_GetError());
  } else {
    Log("Renderer VSync enabled for native video presentation");
  }
#endif
#if defined(OPENNOW_HAS_FFMPEG)
  ConfigureFfmpegLogging();
  packet_ = av_packet_alloc();
  video_frame_ = av_frame_alloc();
  audio_frame_ = av_frame_alloc();
  transfer_frame_ = av_frame_alloc();
  if (!packet_ || !video_frame_ || !audio_frame_ || !transfer_frame_) {
    error = "Failed to allocate FFmpeg frame buffers";
    return false;
  }
#endif
  return true;
}

void MediaPipeline::Shutdown() {
#if defined(OPENNOW_HAS_SDL3)
  if (macos_surface_renderer_) {
    macos_surface_renderer_->Shutdown();
    macos_surface_renderer_.reset();
  }
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
  if (hw_device_ctx_) {
    av_buffer_unref(&hw_device_ctx_);
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
  if (transfer_frame_) {
    av_frame_free(&transfer_frame_);
  }
#endif
}

void MediaPipeline::ConfigureVideoCodec(const std::string& codec) {
  video_codec_ = codec;
  prefer_rgba_upload_ = false;
  target_buffered_video_frames_ = kMinBufferedVideoFrames;
}

void MediaPipeline::ConfigureAudioCodec(const std::string& codec, int payload_type, int clock_rate, int channels) {
  audio_codec_ = codec;
  audio_payload_type_ = payload_type;
  audio_clock_rate_ = clock_rate;
  audio_channels_ = channels;
}

void MediaPipeline::PushVideoFrame(std::vector<std::uint8_t> encoded_frame, std::uint64_t timestamp_us) {
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  (void)timestamp_us;
  received_video_frames_ += 1;
  DecodeVideoFrame(encoded_frame);
#else
  (void)encoded_frame;
  (void)timestamp_us;
#endif
}

void MediaPipeline::PushAudioFrame(std::vector<std::uint8_t> encoded_frame, std::uint64_t timestamp_us) {
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  (void)timestamp_us;
  DecodeAudioFrame(encoded_frame);
#else
  (void)encoded_frame;
  (void)timestamp_us;
#endif
}

void MediaPipeline::RenderFrame() {
#if defined(OPENNOW_HAS_SDL3)
  const auto render_started_at_us = TimestampUs();
  TuneFramePacingTarget(render_started_at_us);
  std::optional<PendingVideoFrame> pending_frame;
  std::size_t pending_queue_depth = 0;
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    while (pending_video_frames_.size() > TargetBufferedVideoFrames() + 1) {
      pending_video_frames_.pop_front();
      dropped_pending_video_frames_ += 1;
      dropped_catchup_video_frames_ += 1;
    }
    if (!pending_video_frames_.empty()) {
      pending_frame = std::move(pending_video_frames_.front());
      pending_video_frames_.pop_front();
    }
    pending_queue_depth = pending_video_frames_.size();
    queue_depth_total_ += pending_queue_depth;
    queue_depth_samples_ += 1;
  }
  bool uploaded_new_frame = false;
  if (pending_frame) {
    uploaded_new_frame = UploadPendingFrame(*pending_frame);
  }
  if (using_native_surface_present_) {
    const auto presented_at_us = TimestampUs();
    if (uploaded_new_frame) {
      presented_video_frames_ += 1;
      last_presented_at_us_ = presented_at_us;
    } else {
      duplicate_present_cycles_ += 1;
    }
  } else if (video_texture_ && renderer_) {
    SDL_RenderTexture(renderer_, video_texture_, nullptr, nullptr);
    const auto presented_at_us = TimestampUs();
    if (uploaded_new_frame) {
      presented_video_frames_ += 1;
      last_presented_at_us_ = presented_at_us;
    } else {
      duplicate_present_cycles_ += 1;
    }
  }
  render_time_total_us_ += TimestampUs() - render_started_at_us;
  MaybeLogVideoDiagnostics(render_started_at_us);
#endif
}

std::string MediaPipeline::DescribeCapabilities() const {
#if defined(OPENNOW_HAS_FFMPEG) && defined(OPENNOW_HAS_SDL3)
  return video_path_;
#elif defined(OPENNOW_HAS_FFMPEG)
  return "video path: FFmpeg decode available without SDL3 render path";
#else
  return "video path: decode pipeline unavailable";
#endif
}

DebugOverlaySnapshot MediaPipeline::GetDebugOverlaySnapshot() const {
  DebugOverlaySnapshot snapshot;
  snapshot.codec = video_codec_;
  snapshot.decoder_name = decoder_name_;
  snapshot.decode_mode = using_hardware_decode_ ? "hardware" : "software";
  snapshot.video_path = video_path_;
  snapshot.width = current_video_width_;
  snapshot.height = current_video_height_;
  snapshot.presented_fps = current_presented_fps_;
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    snapshot.pending_queue_depth = pending_video_frames_.size();
  }
  snapshot.average_queue_depth =
      queue_depth_samples_ == 0 ? 0.0 : static_cast<double>(queue_depth_total_) / static_cast<double>(queue_depth_samples_);
  snapshot.dropped_frames = dropped_pending_video_frames_;
  return snapshot;
}

void MediaPipeline::Log(const std::string& message) const {
  if (logger_) {
    logger_(message);
  }
}

void MediaPipeline::ConfigureFfmpegLogging() {
#if defined(OPENNOW_HAS_FFMPEG)
  av_log_set_callback(OpenNowFfmpegLogCallback);
  Log("FFmpeg logging configured; duplicate deprecated pixel-format warnings will be suppressed");
#endif
}

void MediaPipeline::LogVideoPath(const std::string& path) {
  video_path_ = path;
  if (!logged_decoder_path_) {
    logged_decoder_path_ = true;
    Log(path);
  }
}

void MediaPipeline::MaybeLogVideoDiagnostics(std::uint64_t now_us) {
  constexpr std::uint64_t DIAGNOSTIC_INTERVAL_US = 2000000;
  if (now_us == 0 || now_us < last_diagnostics_log_us_ + DIAGNOSTIC_INTERVAL_US) {
    return;
  }
  std::ostringstream diagnostics;
  diagnostics << "Video diagnostics: received=" << received_video_frames_ << ", staged=" << staged_video_frames_
              << ", uploaded=" << uploaded_video_frames_ << ", presented_unique=" << presented_video_frames_
              << ", presented_duplicate=" << duplicate_present_cycles_ << ", dropped_pending=" << dropped_pending_video_frames_
              << ", dropped_catchup=" << dropped_catchup_video_frames_ << ", decode_stall_packets="
              << consecutive_video_packets_without_frame_ << ", decoder_flushes=" << decoder_flushes_
              << ", software_fallbacks=" << software_decode_fallbacks_;
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    diagnostics << ", pending_queue=" << pending_video_frames_.size();
  }
  if (queue_depth_samples_ != 0) {
    diagnostics << ", avg_queue_depth="
                << (static_cast<double>(queue_depth_total_) / static_cast<double>(queue_depth_samples_));
  }
  if (staged_video_frames_ != 0) {
    diagnostics << ", avg_decode_ms=" << (static_cast<double>(decode_time_total_us_) / 1000.0 / static_cast<double>(staged_video_frames_));
  }
  if (uploaded_video_frames_ != 0) {
    diagnostics << ", avg_upload_ms=" << (static_cast<double>(upload_time_total_us_) / 1000.0 / static_cast<double>(uploaded_video_frames_));
  }
  if (presented_video_frames_ != 0 || duplicate_present_cycles_ != 0) {
    const auto render_cycles = presented_video_frames_ + duplicate_present_cycles_;
    diagnostics << ", avg_render_ms=" << (static_cast<double>(render_time_total_us_) / 1000.0 / static_cast<double>(render_cycles));
  }
  diagnostics << ", path=" << video_path_;
  Log(diagnostics.str());
  last_diagnostics_log_us_ = now_us;
}

void MediaPipeline::ResetVideoDecoder(const std::string& reason, bool force_software) {
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  Log(std::string(force_software ? "Switching video decode path to software after stall: " : "Resetting video decoder after stall: ") +
      reason);
  if (video_decoder_ctx_) {
    avcodec_free_context(&video_decoder_ctx_);
  }
  if (hw_device_ctx_) {
    av_buffer_unref(&hw_device_ctx_);
  }
  using_hardware_decode_ = false;
  hw_pixel_format_ = AV_PIX_FMT_NONE;
  consecutive_video_packets_without_frame_ = 0;
  decode_stall_flush_attempted_ = false;
  logged_decoder_path_ = false;
  if (force_software) {
    force_software_decode_ = true;
    software_decode_fallbacks_ += 1;
    LogVideoPath(prefer_rgba_upload_ ? "video path: software decode + RGBA upload fallback"
                                     : "video path: software decode + SDL YUV/RGBA upload fallback");
  } else {
    hardware_decode_resets_ += 1;
  }
#else
  (void)reason;
  (void)force_software;
#endif
}

void MediaPipeline::NoteDecodedVideoFrame() {
  consecutive_video_packets_without_frame_ = 0;
  decode_stall_flush_attempted_ = false;
  if (using_hardware_decode_) {
    hardware_decode_resets_ = 0;
  }
}

void MediaPipeline::HandleVideoDecodeFailure(const std::string& reason) {
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  consecutive_video_packets_without_frame_ += 1;
  if (consecutive_video_packets_without_frame_ == 1 || consecutive_video_packets_without_frame_ == kMaxPacketsWithoutDecodedFrameBeforeReset) {
    std::ostringstream message;
    message << (using_hardware_decode_ ? "VideoToolbox" : "Software") << " decode produced no frame ("
            << consecutive_video_packets_without_frame_ << " consecutive packet"
            << (consecutive_video_packets_without_frame_ == 1 ? "" : "s") << "): " << reason;
    Log(message.str());
  }
  if (!video_decoder_ctx_) {
    return;
  }

  const bool missing_reference_chain =
      reason.find("Could not find ref") != std::string::npos || reason.find("frame RPS") != std::string::npos ||
      reason.find("undecodable NALU") != std::string::npos;

  if (missing_reference_chain && !decode_stall_flush_attempted_ &&
      consecutive_video_packets_without_frame_ >= (kMaxPacketsWithoutDecodedFrameBeforeReset / 3)) {
    Log(std::string("Decoder stall suggests missing HEVC reference chain; flushing decoder state and awaiting recovery/keyframe: ") +
        reason);
    avcodec_flush_buffers(video_decoder_ctx_);
    decode_stall_flush_attempted_ = true;
    decoder_flushes_ += 1;
    return;
  }

  if (consecutive_video_packets_without_frame_ < kMaxPacketsWithoutDecodedFrameBeforeReset) {
    return;
  }

  if (using_hardware_decode_) {
    if (hardware_decode_resets_ >= kMaxHardwareDecodeResetsBeforeSoftwareFallback) {
      ResetVideoDecoder(reason, true);
      return;
    }
    ResetVideoDecoder(reason, false);
    return;
  }

  Log(std::string("Software decoder stalled after prior recovery attempts; flushing decoder state again: ") + reason);
  avcodec_flush_buffers(video_decoder_ctx_);
  decoder_flushes_ += 1;
  consecutive_video_packets_without_frame_ = 0;
  decode_stall_flush_attempted_ = true;
#else
  (void)reason;
#endif
}

void MediaPipeline::TuneFramePacingTarget(std::uint64_t now_us) {
  if (now_us < last_pacing_tune_us_ + kPacingTuneIntervalUs) {
    return;
  }
  last_pacing_tune_us_ = now_us;
  const auto render_cycles = presented_video_frames_ + duplicate_present_cycles_;
  const auto duplicate_ratio = render_cycles == 0 ? 0.0 : static_cast<double>(duplicate_present_cycles_) / static_cast<double>(render_cycles);
  const auto drop_ratio =
      received_video_frames_ == 0 ? 0.0 : static_cast<double>(dropped_pending_video_frames_ + dropped_catchup_video_frames_) /
                                        static_cast<double>(received_video_frames_);
  std::size_t pending_depth = 0;
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    pending_depth = pending_video_frames_.size();
  }
  const bool sustained_queue_growth = pending_depth > kMinBufferedVideoFrames;
  const bool excessive_duplicate_presents = duplicate_ratio > 0.35;
  const bool excessive_frame_drops = drop_ratio > 0.015;
  target_buffered_video_frames_ =
      (sustained_queue_growth || excessive_duplicate_presents || excessive_frame_drops) ? kMaxBufferedVideoFrames : kMinBufferedVideoFrames;
}

std::size_t MediaPipeline::TargetBufferedVideoFrames() const {
  return std::clamp(target_buffered_video_frames_, kMinBufferedVideoFrames, kMaxBufferedVideoFrames);
}

#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
enum AVPixelFormat MediaPipeline::SelectHardwarePixelFormat(AVCodecContext* context, const enum AVPixelFormat* pixel_formats) {
  auto* self = static_cast<MediaPipeline*>(context->opaque);
  for (const enum AVPixelFormat* current = pixel_formats; current != nullptr && *current != AV_PIX_FMT_NONE; ++current) {
    if (*current == self->hw_pixel_format_) {
      return *current;
    }
  }
  self->Log("Hardware decode pixel format negotiation failed; falling back to software surfaces");
  self->using_hardware_decode_ = false;
  return pixel_formats[0];
}

bool MediaPipeline::TryInitializeHardwareDecode(const AVCodec* codec, std::string& error) {
#if defined(__APPLE__)
  if (!codec) {
    return false;
  }
  for (int i = 0;; ++i) {
    const AVCodecHWConfig* config = avcodec_get_hw_config(codec, i);
    if (config == nullptr) {
      break;
    }
    if ((config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) == 0) {
      continue;
    }
    if (config->device_type != AV_HWDEVICE_TYPE_VIDEOTOOLBOX) {
      continue;
    }
    hw_pixel_format_ = config->pix_fmt;
    if (av_hwdevice_ctx_create(&hw_device_ctx_, AV_HWDEVICE_TYPE_VIDEOTOOLBOX, nullptr, nullptr, 0) < 0) {
      using_hardware_decode_ = false;
      error = "Failed to create VideoToolbox hardware device context";
      return false;
    }
    using_hardware_decode_ = true;
    return true;
  }
#else
  (void)codec;
  (void)error;
#endif
  return false;
}

bool MediaPipeline::EnsureVideoDecoder(std::string& error) {
  if (video_decoder_ctx_) {
    return true;
  }

  auto cleanup_decoder = [this]() {
    if (video_decoder_ctx_) {
      avcodec_free_context(&video_decoder_ctx_);
    }
    if (hw_device_ctx_) {
      av_buffer_unref(&hw_device_ctx_);
    }
    using_hardware_decode_ = false;
    hw_pixel_format_ = AV_PIX_FMT_NONE;
  };

  auto initialize_decoder = [&](const VideoDecoderCandidate& candidate, std::string& init_error) -> bool {
    cleanup_decoder();
    const AVCodec* codec = candidate.decoder_name.empty() ? FindVideoDecoder(video_codec_)
                                                          : avcodec_find_decoder_by_name(candidate.decoder_name.c_str());
    if (!codec) {
      init_error = candidate.decoder_name.empty() ? "Requested video decoder is unavailable in FFmpeg"
                                                  : "FFmpeg decoder backend was not found: " + candidate.decoder_name;
      return false;
    }
    video_decoder_ctx_ = avcodec_alloc_context3(codec);
    if (!video_decoder_ctx_) {
      init_error = "Failed to allocate video decoder context";
      return false;
    }
    video_decoder_ctx_->opaque = this;
    decoder_name_ = codec->name ? codec->name : "unknown";
    if (candidate.use_hw_device && candidate.hardware) {
      std::string hardware_error;
      TryInitializeHardwareDecode(codec, hardware_error);
      if (using_hardware_decode_) {
        video_decoder_ctx_->get_format = &MediaPipeline::SelectHardwarePixelFormat;
        video_decoder_ctx_->hw_device_ctx = av_buffer_ref(hw_device_ctx_);
      } else if (!hardware_error.empty()) {
        init_error = hardware_error;
        return false;
      }
    } else {
      using_hardware_decode_ = candidate.hardware;
      hw_pixel_format_ = AV_PIX_FMT_NONE;
    }
    if (using_hardware_decode_ && candidate.use_hw_device && using_native_surface_present_) {
      LogVideoPath("video path: macOS VideoToolbox + Metal/CVPixelBuffer direct presentation");
    } else {
      LogVideoPath(candidate.path_description);
    }
    if (avcodec_open2(video_decoder_ctx_, codec, nullptr) < 0) {
      init_error = using_hardware_decode_ ? "Failed to open hardware-accelerated video decoder" : "Failed to open video decoder";
      return false;
    }
    return true;
  };

  const auto candidates = BuildVideoDecoderCandidates(video_codec_, force_software_decode_, prefer_rgba_upload_);
  if (IsRaspberryPi4Runtime()) {
    if (video_codec_ == "AV1") {
      Log("Raspberry Pi 4 has no practical hardware AV1 decoder; selecting software AV1 decode fallback");
    } else {
      Log(std::string("Raspberry Pi 4 decoder ladder active for codec ") + video_codec_ +
          ": preferring V4L2 M2M hardware decode before software fallback");
    }
  }

  std::string last_error;
  for (const auto& candidate : candidates) {
    std::string attempt_error;
    const auto backend_name = candidate.decoder_name.empty() ? std::string("default decoder") : candidate.decoder_name;
    Log(std::string("Trying decoder candidate (") + backend_name + ") -> " + candidate.path_description);
    if (initialize_decoder(candidate, attempt_error)) {
      Log(std::string("Selected decoder candidate (") + backend_name + ") -> " + candidate.path_description);
      return true;
    }
    last_error = attempt_error;
    Log(std::string("Decoder candidate rejected (") + backend_name + "): " + attempt_error);
  }
  error = last_error.empty() ? "Failed to initialize any video decoder candidate" : last_error;
  cleanup_decoder();
  return false;
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

bool MediaPipeline::EnsureTransferFrame() {
  if (!transfer_frame_) {
    transfer_frame_ = av_frame_alloc();
  }
  return transfer_frame_ != nullptr;
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
  const auto decode_started_at_us = TimestampUs();
  const int send_result = avcodec_send_packet(video_decoder_ctx_, packet_);
  if (send_result < 0) {
    HandleVideoDecodeFailure(std::string("avcodec_send_packet failed with code ") + std::to_string(send_result));
    return;
  }
  bool decoded_frame = false;
  bool counted_failure_for_packet = false;
  while (true) {
    const int receive_result = avcodec_receive_frame(video_decoder_ctx_, video_frame_);
    if (receive_result == 0) {
      decoded_frame = true;
      NoteDecodedVideoFrame();
      StageFrame(video_frame_);
      continue;
    }
    if (receive_result == AVERROR(EAGAIN) || receive_result == AVERROR_EOF) {
      break;
    }
    HandleVideoDecodeFailure(std::string("avcodec_receive_frame failed with code ") + std::to_string(receive_result));
    counted_failure_for_packet = true;
    break;
  }
  if (!decoded_frame && !counted_failure_for_packet) {
    HandleVideoDecodeFailure("No frame decoded");
  }
  decode_time_total_us_ += TimestampUs() - decode_started_at_us;
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
  if (StageFrameNativeSurface(frame)) {
    return;
  }
  AVFrame* source = frame;
  if (using_hardware_decode_ && frame->format == hw_pixel_format_) {
    if (!EnsureTransferFrame()) {
      Log("Failed to allocate FFmpeg transfer frame for hardware decode output");
      return;
    }
    av_frame_unref(transfer_frame_);
    if (av_hwframe_transfer_data(transfer_frame_, frame, 0) < 0) {
      Log("VideoToolbox frame transfer to CPU-visible surface failed; dropping frame");
      return;
    }
    source = transfer_frame_;
  }
  if (!StageFrameDirect(source)) {
    StageFrameRgba(source);
  }
}

bool MediaPipeline::StageFrameNativeSurface(AVFrame* frame) {
#if defined(__APPLE__)
  if (!using_hardware_decode_ || !using_native_surface_present_ || !macos_surface_renderer_ || frame->format != hw_pixel_format_) {
    return false;
  }
  void* native_surface = frame->data[3];
  auto retained_surface = macos_surface_renderer_->RetainNativeSurface(native_surface);
  if (!retained_surface) {
    return false;
  }
  PendingVideoFrame pending;
  pending.format = PendingVideoFormat::NativeSurface;
  pending.native_surface = std::move(retained_surface);
  pending.width = frame->width;
  pending.height = frame->height;
  pending.timestamp_us = TimestampUs();
  pending.staged_at_us = pending.timestamp_us;
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    if (pending_video_frames_.size() >= kMaxPendingVideoFrames) {
      pending_video_frames_.pop_front();
      dropped_pending_video_frames_ += 1;
    }
    pending_video_frames_.push_back(std::move(pending));
  }
  current_video_width_ = frame->width;
  current_video_height_ = frame->height;
  staged_video_frames_ += 1;
  LogVideoPath("video path: macOS VideoToolbox + Metal/CVPixelBuffer direct presentation");
  return true;
#else
  (void)frame;
  return false;
#endif
}

bool MediaPipeline::StageFrameDirect(AVFrame* frame) {
  if (prefer_rgba_upload_) {
    return false;
  }
  PendingVideoFrame pending;
  pending.width = frame->width;
  pending.height = frame->height;
  pending.timestamp_us = TimestampUs();
  pending.staged_at_us = pending.timestamp_us;
  const auto format = static_cast<AVPixelFormat>(frame->format);
  if (IsNv12Like(format)) {
    pending.format = PendingVideoFormat::NV12;
    pending.pitch0 = frame->linesize[0];
    pending.pitch1 = frame->linesize[1];
    pending.plane0.resize(static_cast<std::size_t>(pending.pitch0 * pending.height));
    pending.plane1.resize(static_cast<std::size_t>(pending.pitch1 * ((pending.height + 1) / 2)));
    for (int y = 0; y < pending.height; ++y) {
      std::memcpy(pending.plane0.data() + static_cast<std::size_t>(y * pending.pitch0), frame->data[0] + static_cast<std::size_t>(y * frame->linesize[0]), static_cast<std::size_t>(pending.pitch0));
    }
    const int chroma_height = (pending.height + 1) / 2;
    for (int y = 0; y < chroma_height; ++y) {
      std::memcpy(pending.plane1.data() + static_cast<std::size_t>(y * pending.pitch1), frame->data[1] + static_cast<std::size_t>(y * frame->linesize[1]), static_cast<std::size_t>(pending.pitch1));
    }
  } else if (IsPlanarYuv420(format)) {
    pending.format = PendingVideoFormat::IYUV;
    pending.pitch0 = frame->linesize[0];
    pending.pitch1 = frame->linesize[1];
    pending.pitch2 = frame->linesize[2];
    pending.plane0.resize(static_cast<std::size_t>(pending.pitch0 * pending.height));
    pending.plane1.resize(static_cast<std::size_t>(pending.pitch1 * ((pending.height + 1) / 2)));
    pending.plane2.resize(static_cast<std::size_t>(pending.pitch2 * ((pending.height + 1) / 2)));
    for (int y = 0; y < pending.height; ++y) {
      std::memcpy(pending.plane0.data() + static_cast<std::size_t>(y * pending.pitch0), frame->data[0] + static_cast<std::size_t>(y * frame->linesize[0]), static_cast<std::size_t>(pending.pitch0));
    }
    const int chroma_height = (pending.height + 1) / 2;
    for (int y = 0; y < chroma_height; ++y) {
      std::memcpy(pending.plane1.data() + static_cast<std::size_t>(y * pending.pitch1), frame->data[1] + static_cast<std::size_t>(y * frame->linesize[1]), static_cast<std::size_t>(pending.pitch1));
      std::memcpy(pending.plane2.data() + static_cast<std::size_t>(y * pending.pitch2), frame->data[2] + static_cast<std::size_t>(y * frame->linesize[2]), static_cast<std::size_t>(pending.pitch2));
    }
  } else {
    return false;
  }
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    if (pending_video_frames_.size() >= kMaxPendingVideoFrames) {
      pending_video_frames_.pop_front();
      dropped_pending_video_frames_ += 1;
    }
    pending_video_frames_.push_back(std::move(pending));
  }
  current_video_width_ = pending.width;
  current_video_height_ = pending.height;
  staged_video_frames_ += 1;
  if (!logged_stage_thread_) {
    logged_stage_thread_ = true;
    Log("Staged decoded video frames on worker thread using direct YUV planes");
  }
  return true;
}

void MediaPipeline::StageFrameRgba(AVFrame* frame) {
  sws_context_ = sws_getCachedContext(
      sws_context_,
      frame->width,
      frame->height,
      static_cast<AVPixelFormat>(frame->format),
      frame->width,
      frame->height,
      AV_PIX_FMT_RGBA,
      SWS_FAST_BILINEAR,
      nullptr,
      nullptr,
      nullptr);
  if (!sws_context_) {
    Log("Failed to create FFmpeg swscale context for fallback RGBA staging");
    return;
  }
  PendingVideoFrame pending;
  pending.format = PendingVideoFormat::RGBA;
  pending.width = frame->width;
  pending.height = frame->height;
  pending.pitch0 = frame->width * 4;
  pending.timestamp_us = TimestampUs();
  pending.staged_at_us = pending.timestamp_us;
  pending.plane0.resize(static_cast<std::size_t>(pending.pitch0 * pending.height));
  std::uint8_t* dst_data[4] = {pending.plane0.data(), nullptr, nullptr, nullptr};
  int dst_linesize[4] = {pending.pitch0, 0, 0, 0};
  sws_scale(sws_context_, frame->data, frame->linesize, 0, frame->height, dst_data, dst_linesize);
  {
    std::lock_guard<std::mutex> lock(pending_video_mutex_);
    if (pending_video_frames_.size() >= kMaxPendingVideoFrames) {
      pending_video_frames_.pop_front();
      dropped_pending_video_frames_ += 1;
    }
    pending_video_frames_.push_back(std::move(pending));
  }
  current_video_width_ = pending.width;
  current_video_height_ = pending.height;
  staged_video_frames_ += 1;
  LogVideoPath(using_hardware_decode_ ? "video path: macOS VideoToolbox decode + RGBA upload fallback" : "video path: software decode + RGBA upload fallback");
  if (!logged_stage_thread_) {
    logged_stage_thread_ = true;
    Log("Staged decoded video frames on worker thread using RGBA fallback");
  }
}

std::optional<PendingVideoFrame> MediaPipeline::ConvertPendingFrameToRgba(const PendingVideoFrame& frame) {
  if (frame.format == PendingVideoFormat::RGBA) {
    return frame;
  }

  const AVPixelFormat source_format = frame.format == PendingVideoFormat::NV12 ? AV_PIX_FMT_NV12 : AV_PIX_FMT_YUV420P;
  sws_context_ = sws_getCachedContext(
      sws_context_,
      frame.width,
      frame.height,
      source_format,
      frame.width,
      frame.height,
      AV_PIX_FMT_RGBA,
      SWS_FAST_BILINEAR,
      nullptr,
      nullptr,
      nullptr);
  if (!sws_context_) {
    Log("Failed to create FFmpeg swscale context for YUV->RGBA fallback conversion");
    return std::nullopt;
  }

  const std::uint8_t* src_data[4] = {
      frame.plane0.data(),
      frame.plane1.empty() ? nullptr : frame.plane1.data(),
      frame.plane2.empty() ? nullptr : frame.plane2.data(),
      nullptr,
  };
  int src_linesize[4] = {frame.pitch0, frame.pitch1, frame.pitch2, 0};

  PendingVideoFrame rgba_frame;
  rgba_frame.format = PendingVideoFormat::RGBA;
  rgba_frame.width = frame.width;
  rgba_frame.height = frame.height;
  rgba_frame.pitch0 = frame.width * 4;
  rgba_frame.timestamp_us = frame.timestamp_us;
  rgba_frame.staged_at_us = TimestampUs();
  rgba_frame.plane0.resize(static_cast<std::size_t>(rgba_frame.pitch0 * rgba_frame.height));
  std::uint8_t* dst_data[4] = {rgba_frame.plane0.data(), nullptr, nullptr, nullptr};
  int dst_linesize[4] = {rgba_frame.pitch0, 0, 0, 0};

  sws_scale(sws_context_, src_data, src_linesize, 0, frame.height, dst_data, dst_linesize);
  return rgba_frame;
}

bool MediaPipeline::UploadPendingFrame(const PendingVideoFrame& frame) {
  if (!renderer_) {
    return false;
  }
  const auto upload_started_at_us = TimestampUs();
  if (frame.format == PendingVideoFormat::NativeSurface) {
    if (!macos_surface_renderer_) {
      return false;
    }
    std::string error;
    if (!macos_surface_renderer_->PresentNativeSurface(frame.native_surface, frame.width, frame.height, error)) {
      Log(std::string("Native-surface present failed; falling back to staged upload path: ") + error);
      using_native_surface_present_ = false;
      target_buffered_video_frames_ = kMinBufferedVideoFrames;
      LogVideoPath("video path: macOS VideoToolbox hardware decode + SDL YUV GPU upload");
      return false;
    }
    upload_time_total_us_ += TimestampUs() - upload_started_at_us;
    const auto now_us = TimestampUs();
    uploaded_video_frames_ += 1;
    if (fps_window_started_us_ == 0) {
      fps_window_started_us_ = now_us;
      fps_window_frames_ = 0;
    }
    fps_window_frames_ += 1;
    const auto fps_window_elapsed_us = now_us - fps_window_started_us_;
    if (fps_window_elapsed_us >= 500000) {
      current_presented_fps_ = static_cast<double>(fps_window_frames_) * 1000000.0 / static_cast<double>(fps_window_elapsed_us);
      fps_window_started_us_ = now_us;
      fps_window_frames_ = 0;
    }
    return true;
  }
  SDL_PixelFormat desired_format = SDL_PIXELFORMAT_RGBA32;
  if (!prefer_rgba_upload_ && frame.format == PendingVideoFormat::NV12) {
    desired_format = SDL_PIXELFORMAT_NV12;
  } else if (!prefer_rgba_upload_ && frame.format == PendingVideoFormat::IYUV) {
    desired_format = SDL_PIXELFORMAT_IYUV;
  }
  if (!video_texture_ || texture_width_ != frame.width || texture_height_ != frame.height || texture_format_ != desired_format) {
    if (video_texture_) {
      SDL_DestroyTexture(video_texture_);
      video_texture_ = nullptr;
    }
    texture_width_ = frame.width;
    texture_height_ = frame.height;
    texture_format_ = desired_format;
    video_texture_ = SDL_CreateTexture(renderer_, desired_format, SDL_TEXTUREACCESS_STREAMING, texture_width_, texture_height_);
    if (!video_texture_) {
      const bool tried_yuv_format = desired_format == SDL_PIXELFORMAT_NV12 || desired_format == SDL_PIXELFORMAT_IYUV;
      const std::string creation_error = SDL_GetError();
      if (tried_yuv_format && !prefer_rgba_upload_) {
        Log(std::string("YUV SDL texture creation failed; switching to sticky RGBA upload fallback: ") + creation_error);
        prefer_rgba_upload_ = true;
        texture_format_ = SDL_PIXELFORMAT_UNKNOWN;
        auto rgba_frame = ConvertPendingFrameToRgba(frame);
        if (rgba_frame) {
          return UploadPendingFrame(*rgba_frame);
        }
        Log("Failed to convert YUV frame into RGBA fallback after SDL texture creation failure");
        return false;
      }
      std::ostringstream fallback;
      fallback << "Failed to create SDL texture for format " << static_cast<std::uint32_t>(desired_format) << ": " << creation_error;
      Log(fallback.str());
      return false;
    }
    Log("Created/updated SDL video texture on render thread");
  }
  bool uploaded = false;
  if (!prefer_rgba_upload_ && frame.format == PendingVideoFormat::NV12) {
    uploaded = SDL_UpdateNVTexture(video_texture_, nullptr, frame.plane0.data(), frame.pitch0, frame.plane1.data(), frame.pitch1);
  } else if (!prefer_rgba_upload_ && frame.format == PendingVideoFormat::IYUV) {
    uploaded = SDL_UpdateYUVTexture(video_texture_, nullptr, frame.plane0.data(), frame.pitch0, frame.plane1.data(), frame.pitch1, frame.plane2.data(), frame.pitch2);
  } else {
    uploaded = SDL_UpdateTexture(video_texture_, nullptr, frame.plane0.data(), frame.pitch0);
  }
  if (!uploaded) {
    const bool tried_yuv_upload = !prefer_rgba_upload_ && (frame.format == PendingVideoFormat::NV12 || frame.format == PendingVideoFormat::IYUV);
    const std::string upload_error = SDL_GetError();
    if (tried_yuv_upload) {
      Log(std::string("YUV SDL texture upload failed; switching to sticky RGBA upload fallback: ") + upload_error);
      prefer_rgba_upload_ = true;
      if (video_texture_) {
        SDL_DestroyTexture(video_texture_);
        video_texture_ = nullptr;
      }
      texture_format_ = SDL_PIXELFORMAT_UNKNOWN;
      auto rgba_frame = ConvertPendingFrameToRgba(frame);
      if (rgba_frame) {
        return UploadPendingFrame(*rgba_frame);
      }
      Log("Failed to convert YUV frame into RGBA fallback after SDL upload failure");
      return false;
    }
    Log(upload_error);
    return false;
  }
  upload_time_total_us_ += TimestampUs() - upload_started_at_us;
  const auto now_us = TimestampUs();
  uploaded_video_frames_ += 1;
  if (fps_window_started_us_ == 0) {
    fps_window_started_us_ = now_us;
    fps_window_frames_ = 0;
  }
  fps_window_frames_ += 1;
  const auto fps_window_elapsed_us = now_us - fps_window_started_us_;
  if (fps_window_elapsed_us >= 500000) {
    current_presented_fps_ =
        static_cast<double>(fps_window_frames_) * 1000000.0 / static_cast<double>(fps_window_elapsed_us);
    fps_window_started_us_ = now_us;
    fps_window_frames_ = 0;
  }
  if (!logged_upload_thread_) {
    logged_upload_thread_ = true;
    Log("Uploaded staged video frames on render thread");
  }
  return true;
}
#endif

}  // namespace opennow::native
