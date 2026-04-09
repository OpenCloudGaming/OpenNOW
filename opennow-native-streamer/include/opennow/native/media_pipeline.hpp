#pragma once

#include <cstdint>
#include <deque>
#include <functional>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#if defined(OPENNOW_HAS_FFMPEG)
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>
}
#endif

#if defined(OPENNOW_HAS_SDL3)
#include <SDL3/SDL.h>
#else
struct SDL_Renderer;
struct SDL_Texture;
typedef void* SDL_AudioStream;
using SDL_PixelFormat = std::uint32_t;
#endif

namespace opennow::native {

struct DebugOverlaySnapshot {
  std::string codec;
  std::string decoder_name;
  std::string decode_mode;
  std::string video_path;
  int width = 0;
  int height = 0;
  double presented_fps = 0.0;
  std::size_t pending_queue_depth = 0;
  double average_queue_depth = 0.0;
  std::uint64_t dropped_frames = 0;
};

enum class PendingVideoFormat {
  NV12,
  IYUV,
  RGBA,
};

struct PendingVideoFrame {
  PendingVideoFormat format = PendingVideoFormat::RGBA;
  std::vector<std::uint8_t> plane0;
  std::vector<std::uint8_t> plane1;
  std::vector<std::uint8_t> plane2;
  int width = 0;
  int height = 0;
  int pitch0 = 0;
  int pitch1 = 0;
  int pitch2 = 0;
  std::uint64_t timestamp_us = 0;
  std::uint64_t staged_at_us = 0;
};

class MediaPipeline {
 public:
  using LogFn = std::function<void(const std::string&)>;

  ~MediaPipeline();

  void SetLogger(LogFn logger);
  bool Initialize(SDL_Renderer* renderer, std::string& error);
  void Shutdown();

  void ConfigureVideoCodec(const std::string& codec);
  void ConfigureAudioCodec(const std::string& codec, int payload_type, int clock_rate, int channels);

  void PushVideoFrame(std::vector<std::uint8_t> encoded_frame, std::uint64_t timestamp_us);
  void PushAudioFrame(std::vector<std::uint8_t> encoded_frame, std::uint64_t timestamp_us);

  void RenderFrame();
  std::string DescribeCapabilities() const;
  DebugOverlaySnapshot GetDebugOverlaySnapshot() const;

 private:
  void Log(const std::string& message) const;
  void ConfigureFfmpegLogging();
  void LogVideoPath(const std::string& path);
  void MaybeLogVideoDiagnostics(std::uint64_t now_us);
  void ResetVideoDecoder(const std::string& reason, bool force_software);
  void NoteDecodedVideoFrame();
  void HandleVideoDecodeFailure(const std::string& reason);

#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  bool EnsureVideoDecoder(std::string& error);
  bool EnsureAudioDecoder(std::string& error);
  void DecodeVideoFrame(const std::vector<std::uint8_t>& encoded_frame);
  void DecodeAudioFrame(const std::vector<std::uint8_t>& encoded_frame);
  void StageFrame(::AVFrame* frame);
  bool UploadPendingFrame(const PendingVideoFrame& frame);
  bool StageFrameDirect(::AVFrame* frame);
  void StageFrameRgba(::AVFrame* frame);
  std::optional<PendingVideoFrame> ConvertPendingFrameToRgba(const PendingVideoFrame& frame);
  bool EnsureTransferFrame();
  bool TryInitializeHardwareDecode(const ::AVCodec* codec, std::string& error);
  static enum AVPixelFormat SelectHardwarePixelFormat(::AVCodecContext* context, const enum AVPixelFormat* pixel_formats);
#endif

  LogFn logger_;
  SDL_Renderer* renderer_ = nullptr;
#if defined(OPENNOW_HAS_SDL3)
  SDL_Texture* video_texture_ = nullptr;
  SDL_AudioStream* audio_stream_ = nullptr;
#endif
  std::string video_codec_ = "H264";
  std::string audio_codec_ = "opus";
  int audio_payload_type_ = 111;
  int audio_clock_rate_ = 48000;
  int audio_channels_ = 2;
  std::uint64_t received_video_frames_ = 0;
  std::uint64_t staged_video_frames_ = 0;
  std::uint64_t uploaded_video_frames_ = 0;
  std::uint64_t dropped_pending_video_frames_ = 0;
  std::uint64_t dropped_catchup_video_frames_ = 0;
  std::uint64_t presented_video_frames_ = 0;
  std::uint64_t duplicate_present_cycles_ = 0;
  std::uint64_t decode_time_total_us_ = 0;
  std::uint64_t upload_time_total_us_ = 0;
  std::uint64_t render_time_total_us_ = 0;
  std::uint64_t queue_depth_samples_ = 0;
  std::uint64_t queue_depth_total_ = 0;
  std::uint64_t last_diagnostics_log_us_ = 0;
  std::uint64_t last_presented_at_us_ = 0;
  std::uint64_t consecutive_video_packets_without_frame_ = 0;
  std::uint64_t decoder_flushes_ = 0;
  std::uint64_t hardware_decode_resets_ = 0;
  std::uint64_t software_decode_fallbacks_ = 0;
  bool logged_stage_thread_ = false;
  bool logged_upload_thread_ = false;
  bool logged_decoder_path_ = false;
  bool using_hardware_decode_ = false;
  bool force_software_decode_ = false;
  bool decode_stall_flush_attempted_ = false;
  bool prefer_rgba_upload_ = false;
  std::string video_path_ = "video path: awaiting decoder initialization";
  std::string decoder_name_ = "unknown";
  int current_video_width_ = 0;
  int current_video_height_ = 0;
  std::uint64_t fps_window_started_us_ = 0;
  std::uint64_t fps_window_frames_ = 0;
  double current_presented_fps_ = 0.0;
  mutable std::mutex pending_video_mutex_;
  std::deque<PendingVideoFrame> pending_video_frames_;
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  ::AVCodecContext* video_decoder_ctx_ = nullptr;
  ::AVCodecContext* audio_decoder_ctx_ = nullptr;
  ::AVFrame* video_frame_ = nullptr;
  ::AVFrame* audio_frame_ = nullptr;
  ::AVFrame* transfer_frame_ = nullptr;
  ::AVPacket* packet_ = nullptr;
  ::SwsContext* sws_context_ = nullptr;
  ::SwrContext* swr_context_ = nullptr;
  ::AVBufferRef* hw_device_ctx_ = nullptr;
  enum AVPixelFormat hw_pixel_format_ = AV_PIX_FMT_NONE;
  int texture_width_ = 0;
  int texture_height_ = 0;
  SDL_PixelFormat texture_format_ = SDL_PIXELFORMAT_UNKNOWN;
#endif
};

}  // namespace opennow::native
