cmake_minimum_required(VERSION 3.24)

if(POLICY CMP0207)
    cmake_policy(SET CMP0207 NEW)
endif()

project(OpenNOWQt VERSION 1.0.0 LANGUAGES CXX)

include(GNUInstallDirs)
include(cmake/BuildMetadata.cmake)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)
# CMake's MinGW AutoMoc predefine probe is unreliable when invoked through
# cmake_autogen on Windows. All required platform and Qt definitions are already
# propagated by the targets below, so avoid the redundant compiler subprocess.
set(CMAKE_AUTOMOC_COMPILER_PREDEFINES OFF)

find_package(Qt6 6.8 REQUIRED COMPONENTS Quick QuickControls2 Gui Network Multimedia Test ShaderTools)
if(APPLE)
    find_package(Qt6 ${Qt6_VERSION} EXACT REQUIRED COMPONENTS Svg)
endif()
# Qt 6.8 exports GuiPrivate with Gui; newer Qt versions package it separately.
# Require the actual target on both layouts without requesting a 6.8 package
# that does not exist. Private headers remain a mandatory build dependency.
if(NOT TARGET Qt6::GuiPrivate)
    find_package(Qt6GuiPrivate ${Qt6_VERSION} EXACT REQUIRED CONFIG)
endif()
if(NOT TARGET Qt6::QuickPrivate)
    find_package(Qt6QuickPrivate ${Qt6_VERSION} EXACT REQUIRED CONFIG)
endif()
find_package(SDL3 REQUIRED CONFIG)
find_package(Threads REQUIRED)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    find_package(Vulkan REQUIRED)
endif()
if(TARGET SDL3::SDL3-shared)
    set(OPENNOW_SDL3_RUNTIME_TARGET SDL3::SDL3-shared)
elseif(TARGET SDL3::SDL3-static)
    set(OPENNOW_SDL3_RUNTIME_TARGET SDL3::SDL3-static)
else()
    set(OPENNOW_SDL3_RUNTIME_TARGET SDL3::SDL3)
endif()
find_program(CARGO_EXECUTABLE cargo REQUIRED)
set(OPENNOW_UPDATE_ED25519_PUBLIC_KEY "" CACHE STRING
    "Base64 Ed25519 public key pinned into release builds for update manifests")
set(OPENNOW_RUST_TARGET "" CACHE STRING
    "Optional Rust target triple for cross-architecture desktop packages")

qt_standard_project_setup(REQUIRES 6.8)
include(cmake/PlatformInput.cmake)
include(cmake/PlatformHdr.cmake)

include(cmake/Sources.cmake)

include(cmake/Resources.cmake)

if(WIN32 OR APPLE)
    set(OPENNOW_EXECUTABLE_NAME "OpenNOW")
else()
    set(OPENNOW_EXECUTABLE_NAME "opennow-qt")
endif()
set_target_properties(opennow-qt PROPERTIES
    OUTPUT_NAME "${OPENNOW_EXECUTABLE_NAME}"
    WIN32_EXECUTABLE TRUE
    MACOSX_BUNDLE TRUE
    MACOSX_BUNDLE_BUNDLE_NAME "OpenNOW"
    MACOSX_BUNDLE_GUI_IDENTIFIER "io.github.opencloudgaming.OpenNOW"
    MACOSX_BUNDLE_SHORT_VERSION_STRING "${OPENNOW_NUMERIC_VERSION}"
    MACOSX_BUNDLE_BUNDLE_VERSION "${OPENNOW_NUMERIC_VERSION}"
    MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/packaging/Info.plist.in"
)

include(cmake/QmlModule.cmake)

target_include_directories(opennow-qt PRIVATE src)
target_link_libraries(opennow-qt PRIVATE
    opennow-platform-input
    opennow-platform-hdr
    Qt6::Quick
    Qt6::QuickControls2
    Qt6::GuiPrivate
    Qt6::QuickPrivate
    Qt6::Network
    Qt6::Multimedia
    SDL3::SDL3
)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_link_libraries(opennow-qt PRIVATE Vulkan::Vulkan)
elseif(APPLE)
    target_link_libraries(opennow-qt PRIVATE Qt6::Svg)
endif()

include(cmake/WindowsRuntime.cmake)

target_compile_definitions(opennow-qt PRIVATE
    QT_NO_CAST_FROM_ASCII
    QT_NO_CAST_TO_ASCII
    OPENNOW_EMBEDDED_STREAMER
)
set_source_files_properties(src/app/ApplicationStartup.cpp src/core/CoreClient.cpp
    PROPERTIES COMPILE_DEFINITIONS "OPENNOW_VERSION=\"${OPENNOW_BUILD_VERSION}\"")

include(cmake/NativeRuntime.cmake)

if(MSVC)
    target_compile_options(opennow-qt PRIVATE /W4 /permissive-)
else()
    target_compile_options(opennow-qt PRIVATE -Wall -Wextra -Wpedantic)
endif()

include(cmake/Tests.cmake)

include(cmake/Packaging.cmake)
