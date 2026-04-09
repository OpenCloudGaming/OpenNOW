cmake_minimum_required(VERSION 3.21)
project(opennow-native-streamer VERSION 0.1.0 LANGUAGES CXX)

include(CheckCXXSourceCompiles)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

option(OPENNOW_ENABLE_WARNINGS "Enable extra compiler warnings" ON)

set(OPENNOW_SOURCES
  src/main.cpp
  src/app/application.cpp
  src/ipc/ipc_client.cpp
  src/ipc/protocol.cpp
  src/input/input_protocol.cpp
  src/input/input_bridge.cpp
  src/media/media_pipeline.cpp
  src/platform/platform_info.cpp
  src/webrtc/av1_rtp_depacketizer.cpp
  src/webrtc/sdp_helpers.cpp
  src/webrtc/webrtc_session.cpp
)

add_executable(opennow-native-streamer ${OPENNOW_SOURCES})
target_include_directories(opennow-native-streamer PRIVATE include)

function(opennow_resolve_imported_target out_var)
  foreach(candidate IN LISTS ARGN)
    if(TARGET ${candidate})
      set(${out_var} ${candidate} PARENT_SCOPE)
      return()
    endif()
  endforeach()
  set(${out_var} "" PARENT_SCOPE)
endfunction()

if(OPENNOW_ENABLE_WARNINGS)
  if(MSVC)
    target_compile_options(opennow-native-streamer PRIVATE /W4 /permissive-)
  else()
    target_compile_options(opennow-native-streamer PRIVATE -Wall -Wextra -Wpedantic)
  endif()
endif()

find_package(SDL3 CONFIG QUIET)
if(SDL3_FOUND)
  opennow_resolve_imported_target(OPENNOW_SDL3_TARGET SDL3::SDL3 SDL3::SDL3-shared SDL3::SDL3-static)
  if(OPENNOW_SDL3_TARGET)
    target_link_libraries(opennow-native-streamer PRIVATE ${OPENNOW_SDL3_TARGET})
    target_compile_definitions(opennow-native-streamer PRIVATE OPENNOW_HAS_SDL3=1)
  else()
    message(WARNING "SDL3 package was found, but no supported imported target was exported.")
  endif()
else()
  message(WARNING "SDL3 was not found. The native streamer requires SDL3 to build.")
endif()

find_package(LibDataChannel CONFIG QUIET)
if(LibDataChannel_FOUND)
  opennow_resolve_imported_target(OPENNOW_LIBDATACHANNEL_TARGET LibDataChannel::LibDataChannel LibDataChannel::datachannel)
  if(OPENNOW_LIBDATACHANNEL_TARGET)
    target_link_libraries(opennow-native-streamer PRIVATE ${OPENNOW_LIBDATACHANNEL_TARGET})
    target_compile_definitions(opennow-native-streamer PRIVATE OPENNOW_HAS_LIBDATACHANNEL=1)

    set(CMAKE_REQUIRED_LIBRARIES ${OPENNOW_LIBDATACHANNEL_TARGET})
    check_cxx_source_compiles(
      "#include <memory>\n#include <rtc/rtc.hpp>\nint main() { auto dep = std::make_shared<rtc::H264RtpDepacketizer>(); auto rtcp = std::make_shared<rtc::RtcpReceivingSession>(); return dep && rtcp ? 0 : 0; }"
      OPENNOW_LIBDATACHANNEL_HAS_MEDIA
    )
    unset(CMAKE_REQUIRED_LIBRARIES)

    if(OPENNOW_LIBDATACHANNEL_HAS_MEDIA)
      target_compile_definitions(opennow-native-streamer PRIVATE OPENNOW_HAS_LIBDATACHANNEL_MEDIA=1)
    else()
      message(WARNING "libdatachannel was found without media support. Native media track handlers will be disabled unless media-enabled packages are installed.")
    endif()
  else()
    message(WARNING "libdatachannel package was found, but no supported imported target was exported.")
  endif()
else()
  message(WARNING "libdatachannel was not found. WebRTC negotiation will remain disabled until the dependency is installed.")
endif()

find_package(PkgConfig QUIET)
if(PkgConfig_FOUND)
  pkg_check_modules(FFMPEG QUIET IMPORTED_TARGET libavcodec libavformat libavutil libswresample libswscale)
endif()

if(FFMPEG_FOUND)
  target_link_libraries(opennow-native-streamer PRIVATE PkgConfig::FFMPEG)
  target_compile_definitions(opennow-native-streamer PRIVATE OPENNOW_HAS_FFMPEG=1)
else()
  find_path(FFMPEG_INCLUDE_DIR libavcodec/avcodec.h)
  find_library(FFMPEG_AVCODEC_LIBRARY NAMES avcodec)
  find_library(FFMPEG_AVFORMAT_LIBRARY NAMES avformat)
  find_library(FFMPEG_AVUTIL_LIBRARY NAMES avutil)
  find_library(FFMPEG_SWRESAMPLE_LIBRARY NAMES swresample)
  find_library(FFMPEG_SWSCALE_LIBRARY NAMES swscale)

  if(
    FFMPEG_INCLUDE_DIR
    AND FFMPEG_AVCODEC_LIBRARY
    AND FFMPEG_AVFORMAT_LIBRARY
    AND FFMPEG_AVUTIL_LIBRARY
    AND FFMPEG_SWRESAMPLE_LIBRARY
    AND FFMPEG_SWSCALE_LIBRARY
  )
    add_library(OPENNOW_FFMPEG INTERFACE)
    target_include_directories(OPENNOW_FFMPEG INTERFACE ${FFMPEG_INCLUDE_DIR})
    target_link_libraries(
      OPENNOW_FFMPEG
      INTERFACE
        ${FFMPEG_AVCODEC_LIBRARY}
        ${FFMPEG_AVFORMAT_LIBRARY}
        ${FFMPEG_AVUTIL_LIBRARY}
        ${FFMPEG_SWRESAMPLE_LIBRARY}
        ${FFMPEG_SWSCALE_LIBRARY}
    )
    target_link_libraries(opennow-native-streamer PRIVATE OPENNOW_FFMPEG)
    target_compile_definitions(opennow-native-streamer PRIVATE OPENNOW_HAS_FFMPEG=1)
  else()
    message(WARNING "FFmpeg development libraries were not found. Decode and audio playback remain scaffold-only.")
  endif()
endif()

if(WIN32)
  target_compile_definitions(opennow-native-streamer PRIVATE WIN32_LEAN_AND_MEAN NOMINMAX)
  target_link_libraries(opennow-native-streamer PRIVATE ws2_32)
endif()
