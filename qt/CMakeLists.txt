cmake_minimum_required(VERSION 3.21)

project(OpenNOWQt VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(Qt6 6.5 REQUIRED COMPONENTS Widgets Network WebSockets)

option(OPNQT_ENABLE_LIBWEBRTC "Build Qt native C++ libwebrtc streaming backend" OFF)
set(OPNQT_LIBWEBRTC_ROOT "${CMAKE_SOURCE_DIR}/../third_party/libwebrtc" CACHE PATH "Root of the native C++ libwebrtc SDK")

qt_standard_project_setup()

qt_add_executable(OpenNOWQt
    src/main.cpp
    src/OpnQtAppWindow.cpp
    src/OpnQtAppWindow.h
    src/OpnQtAuthService.cpp
    src/OpnQtAuthService.h
    src/OpnQtCatalogView.cpp
    src/OpnQtCatalogView.h
    src/OpnQtGameService.cpp
    src/OpnQtGameService.h
    src/OpnQtSessionManager.cpp
    src/OpnQtSessionManager.h
    src/OpnQtSignalingClient.cpp
    src/OpnQtSignalingClient.h
    src/OpnQtStreamPreferences.cpp
    src/OpnQtStreamPreferences.h
    src/OpnQtStreamTypes.h
    src/OpnQtWebRtcStreamSession.cpp
    src/OpnQtWebRtcStreamSession.h
)

target_link_libraries(OpenNOWQt
    PRIVATE
        Qt6::Network
        Qt6::WebSockets
        Qt6::Widgets
)

target_compile_features(OpenNOWQt PRIVATE cxx_std_20)

if(OPNQT_ENABLE_LIBWEBRTC)
    if(APPLE)
        set(OPNQT_LIBWEBRTC_PLATFORM_DIR "${OPNQT_LIBWEBRTC_ROOT}/lib/macos")
    elseif(WIN32)
        set(OPNQT_LIBWEBRTC_PLATFORM_DIR "${OPNQT_LIBWEBRTC_ROOT}/lib/windows")
    else()
        set(OPNQT_LIBWEBRTC_PLATFORM_DIR "${OPNQT_LIBWEBRTC_ROOT}/lib/linux")
    endif()

    find_path(LIBWEBRTC_INCLUDE_DIR api/peer_connection_interface.h PATHS "${OPNQT_LIBWEBRTC_ROOT}/include" NO_DEFAULT_PATH REQUIRED)
    find_library(LIBWEBRTC_LIBRARY NAMES webrtc libwebrtc PATHS "${OPNQT_LIBWEBRTC_PLATFORM_DIR}" NO_DEFAULT_PATH REQUIRED)
    target_compile_definitions(OpenNOWQt PRIVATE OPNQT_HAVE_LIBWEBRTC=1)
    target_include_directories(OpenNOWQt PRIVATE ${LIBWEBRTC_INCLUDE_DIR})
    target_link_libraries(OpenNOWQt PRIVATE ${LIBWEBRTC_LIBRARY})
endif()

if(MSVC)
    target_compile_options(OpenNOWQt PRIVATE /W4 /permissive-)
else()
    target_compile_options(OpenNOWQt PRIVATE -Wall -Wextra -Wpedantic)
endif()

set_target_properties(OpenNOWQt PROPERTIES
    OUTPUT_NAME OpenNOW-Qt
    WIN32_EXECUTABLE TRUE
    MACOSX_BUNDLE TRUE
    MACOSX_BUNDLE_GUI_IDENTIFIER com.opennow.qt
    MACOSX_BUNDLE_BUNDLE_NAME OpenNOW-Qt
)

add_custom_target(run
    COMMAND $<TARGET_FILE:OpenNOWQt>
    DEPENDS OpenNOWQt
    WORKING_DIRECTORY $<TARGET_FILE_DIR:OpenNOWQt>
    VERBATIM
)
