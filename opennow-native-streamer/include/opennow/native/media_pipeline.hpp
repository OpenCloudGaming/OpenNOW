#pragma once

#include <cstdint>
#include <functional>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#if defined(OPENNOW_HAS_FFMPEG)
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/frame.h>
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
#endif

namespace opennow::native {

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

 private:
  void Log(const std::string& message) const;

#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  bool EnsureVideoDecoder(std::string& error);
  bool EnsureAudioDecoder(std::string& error);
  void DecodeVideoFrame(const std::vector<std::uint8_t>& encoded_frame);
  void DecodeAudioFrame(const std::vector<std::uint8_t>& encoded_frame);
  void UploadFrame(::AVFrame* frame);
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
  std::uint64_t rendered_frames_ = 0;
#if defined(OPENNOW_HAS_SDL3) && defined(OPENNOW_HAS_FFMPEG)
  ::AVCodecContext* video_decoder_ctx_ = nullptr;
  ::AVCodecContext* audio_decoder_ctx_ = nullptr;
  ::AVFrame* video_frame_ = nullptr;
  ::AVFrame* audio_frame_ = nullptr;
  ::AVPacket* packet_ = nullptr;
  ::SwsContext* sws_context_ = nullptr;
  ::SwrContext* swr_context_ = nullptr;
  int texture_width_ = 0;
  int texture_height_ = 0;
#endif
};

}  // namespace opennow::native
