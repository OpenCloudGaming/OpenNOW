add_library(opennow-generic-hid STATIC
    src/input/GenericHidInput.cpp
    src/input/GenericHidInput.h
    src/input/GenericHidWire.h)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_sources(opennow-generic-hid PRIVATE
        src/input/platform/LinuxHidIdentity.cpp
        src/input/platform/LinuxHidIdentity.h)
endif()
target_include_directories(opennow-generic-hid PUBLIC src)
target_link_libraries(opennow-generic-hid PUBLIC Qt6::Core SDL3::SDL3)
if(APPLE)
    target_link_libraries(opennow-generic-hid PRIVATE "-framework IOKit" "-framework CoreFoundation")
elseif(WIN32)
    target_link_libraries(opennow-generic-hid PRIVATE setupapi hid)
endif()

set(OPENNOW_STREAM_RUNTIME_SOURCES
    src/streaming/NativeStreamRuntime.cpp
    src/streaming/NativeStreamRuntime.h
)

set(OPENNOW_STREAM_PRESENTATION_SOURCES
    src/streaming/rendering/HdrOutput.cpp
    src/streaming/rendering/HdrOutput.h
    src/streaming/rendering/HdrOutputPass.h
    src/streaming/rendering/HdrChromeEffect.cpp
    src/streaming/rendering/HdrChromeEffect.h
    src/streaming/PhysicalKeyMap.cpp
    src/streaming/PhysicalKeyMap.h
    src/streaming/PhysicalKeyMapData.h
    src/streaming/StreamVideoItem.cpp
    src/streaming/StreamVideoItem.h
    src/streaming/StreamVideoItemInput.cpp
    src/streaming/rendering/LinuxVulkanGraphics.cpp
    src/streaming/rendering/LinuxVulkanGraphics.h
    src/streaming/rendering/NativeStreamRenderCallback.cpp
    src/streaming/rendering/NativeStreamRenderCallback.h
    src/streaming/rendering/StreamVideoItemRendering.cpp
    src/streaming/rendering/StreamVideoRenderCallback.h
    src/streaming/rendering/StreamFrameSnapshot.h
    src/streaming/rendering/StreamPresentTimings.h
    src/streaming/rendering/StreamVideoTextureRenderer.h
    src/streaming/rendering/StreamFrameInterpolator.cpp
    src/streaming/rendering/StreamFrameInterpolator.h
    src/streaming/rendering/StreamFramePacer.h
)

qt_add_executable(opennow-qt
    src/main.cpp
    src/acceptance/AcceptanceProfiler.cpp
    src/acceptance/AcceptanceProfiler.h
    src/acceptance/AcceptanceSession.cpp
    src/acceptance/AcceptanceSession.h
    src/acceptance/MotionAcceptance.cpp
    src/acceptance/MotionAcceptance.h
    src/acceptance/SmokeAcceptance.cpp
    src/acceptance/SmokeFixtures.cpp
    src/acceptance/FrameGenerationStatsAcceptance.cpp
    src/acceptance/GpuSettingsAcceptance.cpp
    src/acceptance/StreamExitAcceptance.cpp
    src/acceptance/SessionFullscreenAcceptance.cpp
    src/acceptance/SteamLaunchAcceptance.cpp
    src/acceptance/SessionLaunchAcceptance.cpp
    src/acceptance/ConsoleSessionAcceptance.cpp
    src/app/AppController.cpp
    src/app/AppController.h
    src/app/platform/MacAwdlController.cpp
    src/app/platform/MacAwdlController.h
    src/app/platform/GraphicsDeviceSelection.cpp
    src/app/platform/GraphicsDeviceSelection.h
    src/app/platform/WindowTheme.cpp
    src/app/platform/WindowTheme.h
    src/app/ApplicationStartup.cpp
    src/app/ApplicationStartup.h
    src/app/SingleInstance.cpp
    src/app/SingleInstance.h
    src/core/CoreClient.cpp
    src/core/CoreClient.h
    src/diagnostics/DiagnosticsPaths.h
    src/input/ControllerInput.cpp
    src/input/ControllerInput.h
    src/input/SdlDeviceClaim.h
    src/input/SonySnapshotWire.h
    src/input/InputModeTracker.cpp
    src/input/InputModeTracker.h
    src/localization/Localization.cpp
    src/localization/Localization.h
    src/media/ThumbnailGenerator.cpp
    src/media/ThumbnailGenerator.h
    ${OPENNOW_STREAM_RUNTIME_SOURCES}
    ${OPENNOW_STREAM_PRESENTATION_SOURCES}
)
target_link_libraries(opennow-qt PRIVATE opennow-generic-hid)
