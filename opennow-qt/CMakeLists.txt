cmake_minimum_required(VERSION 3.24)

if(POLICY CMP0207)
    cmake_policy(SET CMP0207 NEW)
endif()

project(OpenNOWQt VERSION 0.5.4 LANGUAGES CXX)

include(GNUInstallDirs)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)
# CMake's MinGW AutoMoc predefine probe is unreliable when invoked through
# cmake_autogen on Windows. All required platform and Qt definitions are already
# propagated by the targets below, so avoid the redundant compiler subprocess.
set(CMAKE_AUTOMOC_COMPILER_PREDEFINES OFF)

find_package(Qt6 6.8 REQUIRED COMPONENTS Quick QuickControls2 Gui GuiPrivate Network Multimedia Test)
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
set(OPENNOW_BUILD_VERSION "${PROJECT_VERSION}" CACHE STRING
    "Numeric OpenNOW release version embedded in Qt, Rust, bundles, and packages")
if(NOT OPENNOW_BUILD_VERSION MATCHES "^[0-9]+\\.[0-9]+\\.[0-9]+$")
    message(FATAL_ERROR
        "OPENNOW_BUILD_VERSION must be a numeric semantic version such as 0.5.4")
endif()
set(OPENNOW_UPDATE_ED25519_PUBLIC_KEY "" CACHE STRING
    "Base64 Ed25519 public key pinned into release builds for update manifests")
set(OPENNOW_RUST_TARGET "" CACHE STRING
    "Optional Rust target triple for cross-architecture desktop packages")

qt_standard_project_setup(REQUIRES 6.8)

set_source_files_properties(qml/theme/Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
set_source_files_properties(qml/state/ShellStore.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
set_source_files_properties(qml/desktop/DesktopTokens.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_executable(opennow-qt
    src/main.cpp
    src/AcceptanceProfiler.cpp
    src/AcceptanceProfiler.h
    src/AppController.cpp
    src/AppController.h
    src/ControllerInput.cpp
    src/ControllerInput.h
    src/CoreClient.cpp
    src/CoreClient.h
    src/InputModeTracker.cpp
    src/InputModeTracker.h
    src/Localization.cpp
    src/Localization.h
    src/NativeStreamRuntime.cpp
    src/NativeStreamRuntime.h
    src/SingleInstance.cpp
    src/SingleInstance.h
    src/StreamVideoItem.cpp
    src/StreamVideoItem.h
    src/ThumbnailGenerator.cpp
    src/ThumbnailGenerator.h
)

file(GLOB OPENNOW_LOCALE_FILES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../locales/*.json")
foreach(OPENNOW_LOCALE_FILE IN LISTS OPENNOW_LOCALE_FILES)
    get_filename_component(OPENNOW_LOCALE_NAME "${OPENNOW_LOCALE_FILE}" NAME)
    set_source_files_properties("${OPENNOW_LOCALE_FILE}" PROPERTIES
        QT_RESOURCE_ALIAS "${OPENNOW_LOCALE_NAME}")
endforeach()
qt_add_resources(opennow-qt "opennow-locales"
    PREFIX "/locales"
    FILES ${OPENNOW_LOCALE_FILES}
)

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
    MACOSX_BUNDLE_SHORT_VERSION_STRING "${OPENNOW_BUILD_VERSION}"
    MACOSX_BUNDLE_BUNDLE_VERSION "${OPENNOW_BUILD_VERSION}"
    MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/packaging/Info.plist.in"
)

qt_add_qml_module(opennow-qt
    URI OpenNOW
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/theme/Theme.qml
        qml/state/ShellStore.qml
        qml/desktop/DesktopTokens.qml
        qml/desktop/DesktopBackdrop.qml
        qml/desktop/DesktopButton.qml
        qml/desktop/DesktopCommandPalette.qml
        qml/desktop/DesktopFriendsScreen.qml
        qml/desktop/DesktopGameModal.qml
        qml/desktop/DesktopGlyph.qml
        qml/desktop/DesktopHomePoster.qml
        qml/desktop/DesktopHomeScreen.qml
        qml/desktop/DesktopKeyHint.qml
        qml/desktop/DesktopLibraryScreen.qml
        qml/desktop/DesktopPoster.qml
        qml/desktop/DesktopSettingsButton.qml
        qml/desktop/DesktopSettingsDropdown.qml
        qml/desktop/DesktopSettingsPanel.qml
        qml/desktop/DesktopSettingsRow.qml
        qml/desktop/DesktopSettingsScreen.qml
        qml/desktop/DesktopSettingsSegmented.qml
        qml/desktop/DesktopSettingsSlider.qml
        qml/desktop/DesktopSettingsToggle.qml
        qml/desktop/DesktopShell.qml
        qml/desktop/DesktopSidebar.qml
        qml/desktop/DesktopSignInScreen.qml
        qml/desktop/DesktopInStreamMenu.qml
        qml/desktop/DesktopStreamExitConfirm.qml
        qml/desktop/DesktopSessionStarting.qml
        qml/desktop/DesktopStreamOverlayHost.qml
        qml/desktop/DesktopStreamScreen.qml
        qml/desktop/DesktopStreamStats.qml
        qml/desktop/DesktopStoreCard.qml
        qml/desktop/DesktopStoreChip.qml
        qml/desktop/DesktopStoreContent.qml
        qml/desktop/DesktopStoreHero.qml
        qml/desktop/DesktopStoreShelf.qml
        qml/desktop/DesktopApp.qml
        qml/components/AppChrome.qml
        qml/components/ArtworkSource.qml
        qml/components/ControllerGlyph.qml
        qml/components/FocusFrame.qml
        qml/components/FilterDropdown.qml
        qml/components/GameTile.qml
        qml/components/GlassPanel.qml
        qml/components/GlassButton.qml
        qml/components/HintBar.qml
        qml/components/HomeTileMenu.qml
        qml/components/NavPill.qml
        qml/components/PosterTile.qml
        qml/components/PlatformPicker.qml
        qml/components/ScreenBackground.qml
        qml/components/SettingRow.qml
        qml/components/RoundedArtwork.qml
        qml/components/StoreBadge.qml
        qml/components/TogglePill.qml
        qml/components/VirtualKeyboard.qml
        qml/overlays/FriendsOverlay.qml
        qml/overlays/GuideOverlay.qml
        qml/overlays/OverlayHost.qml
        qml/overlays/QuickSettingsOverlay.qml
        qml/overlays/SessionConflictOverlay.qml
        qml/overlays/SessionReportOverlay.qml
        qml/overlays/QueueAdOverlay.qml
        qml/screens/GameDetailScreen.qml
        qml/screens/HomeScreen.qml
        qml/screens/InsertingScreen.qml
        qml/screens/JoiningScreen.qml
        qml/screens/LibraryScreen.qml
        qml/screens/SettingsScreen.qml
        qml/screens/SignInScreen.qml
        qml/screens/StoreScreen.qml
        qml/screens/ThemeStoreScreen.qml
        qml/screens/StreamScreen.qml
        qml/screens/AccountsScreen.qml
        qml/screens/ProfilePinScreen.qml
        qml/screens/GameAccountsScreen.qml
        qml/screens/PersistentStorageScreen.qml
        qml/screens/MediaScreen.qml
        qml/screens/DiagnosticsScreen.qml
        qml/screens/UpdateScreen.qml
        qml/screens/FeedbackScreen.qml
    RESOURCES
        res/icons/nav-home.svg
        res/icons/nav-library.svg
        res/icons/nav-controller.svg
        res/icons/nav-friends.svg
        res/icons/nav-settings.svg
        res/icons/nav-computer.svg
        res/icons/settings-account.svg
        res/icons/settings-streaming.svg
        res/icons/settings-video.svg
        res/icons/settings-input.svg
        res/icons/settings-network.svg
        res/icons/settings-themes.svg
        res/icons/settings-advanced.svg
        res/icons/store-steam.svg
        res/icons/store-epic.svg
        res/icons/store-ubisoft.svg
        res/icons/store-battlenet.svg
        res/brand/opennow-mark.png
        res/brand/signin-hero.jpg
        res/fonts/Nunito-Variable.ttf
        res/fonts/IBMPlexMono-Regular.ttf
        res/fonts/IBMPlexMono-Medium.ttf
        res/fonts/IBMPlexMono-Bold.ttf
        res/icons/desktop-nav-home.svg
        res/icons/desktop-nav-home-active.svg
        res/icons/desktop-nav-library.svg
        res/icons/desktop-nav-library-active.svg
        res/icons/desktop-nav-store.svg
        res/icons/desktop-nav-store-active.svg
        res/icons/desktop-nav-friends.svg
        res/icons/desktop-nav-friends-active.svg
        res/icons/desktop-nav-settings.svg
        res/icons/desktop-nav-settings-active.svg
        res/icons/desktop-collapse.svg
        res/icons/desktop-expand.svg
        res/icons/desktop-plus.svg
        res/icons/desktop-star.svg
        res/icons/desktop-clock.svg
        res/icons/desktop-coop.svg
        res/icons/desktop-rtx.svg
        res/icons/desktop-gamepad.svg
        res/icons/desktop-chevron-up.svg
        res/icons/desktop-chevron-down.svg
        res/icons/desktop-search.svg
        res/icons/desktop-play.svg
        res/icons/desktop-lock.svg
        res/icons/desktop-check.svg
        res/icons/desktop-shield.svg
        res/icons/desktop-qr.svg
        res/icons/desktop-info.svg
        res/icons/desktop-close.svg
        res/icons/desktop-folder.svg
        res/icons/desktop-check-mint.svg
        res/icons/desktop-check-light.svg
        res/icons/desktop-check-focus.svg
        res/icons/desktop-message.svg
        res/icons/desktop-sliders.svg
        res/icons/desktop-user-plus.svg
        res/icons/desktop-logout.svg
        res/icons/desktop-plus-ink.svg
        res/icons/desktop-mic.svg
        res/icons/desktop-mic-off.svg
        res/icons/desktop-play-stroke.svg
)

target_include_directories(opennow-qt PRIVATE src)
target_link_libraries(opennow-qt PRIVATE
    Qt6::Quick
    Qt6::QuickControls2
    Qt6::GuiPrivate
    Qt6::Network
    Qt6::Multimedia
    SDL3::SDL3
)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_link_libraries(opennow-qt PRIVATE Vulkan::Vulkan)
endif()

if(WIN32)
    target_link_libraries(opennow-qt PRIVATE user32)
    add_custom_command(TARGET opennow-qt POST_BUILD
        COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "$<TARGET_FILE:${OPENNOW_SDL3_RUNTIME_TARGET}>"
                "$<TARGET_FILE_DIR:opennow-qt>"
        COMMENT "Copying SDL3 next to the OpenNOW executable")
    get_filename_component(OPENNOW_QT_TARGET_BIN "${Qt6_DIR}/../../../bin" ABSOLUTE)
    set(OPENNOW_WINDEPLOYQT_ARGS
        --no-translations
        --qmldir "${CMAKE_CURRENT_SOURCE_DIR}/qml")
    if(QT_HOST_PATH)
        find_program(WINDEPLOYQT_EXECUTABLE NAMES windeployqt
            HINTS "${QT_HOST_PATH}/bin"
            NO_DEFAULT_PATH
            REQUIRED)
        find_file(OPENNOW_TARGET_QTPATHS_EXECUTABLE
            NAMES qtpaths6.bat qtpaths.bat
            HINTS "${OPENNOW_QT_TARGET_BIN}"
            NO_DEFAULT_PATH
            REQUIRED)
        list(PREPEND OPENNOW_WINDEPLOYQT_ARGS
            --qtpaths "${OPENNOW_TARGET_QTPATHS_EXECUTABLE}")
        set(OPENNOW_QT_DEPLOY_TOOL_ARGS
            DEPLOY_TOOL_OPTIONS
            --qtpaths "${OPENNOW_TARGET_QTPATHS_EXECUTABLE}")
    else()
        find_program(WINDEPLOYQT_EXECUTABLE NAMES windeployqt
            HINTS "${OPENNOW_QT_TARGET_BIN}"
            NO_DEFAULT_PATH
            REQUIRED)
    endif()
    add_custom_command(TARGET opennow-qt POST_BUILD
        COMMAND "${WINDEPLOYQT_EXECUTABLE}" ${OPENNOW_WINDEPLOYQT_ARGS}
                "$<TARGET_FILE:opennow-qt>"
        COMMAND "${CMAKE_COMMAND}" -E rm -f
                "$<TARGET_FILE_DIR:opennow-qt>/tls/qopensslbackend.dll"
                "$<TARGET_FILE_DIR:opennow-qt>/plugins/tls/qopensslbackend.dll"
        COMMENT "Deploying Qt runtime plugins and preferring Windows Schannel TLS"
        COMMAND_EXPAND_LISTS)
    # windeployqt intentionally deploys only the production `windows` QPA
    # plugin. CTest runs the GUI/QML smoke suite with `QT_QPA_PLATFORM=offscreen`,
    # so keep that test-only plugin beside the deployed application as well.
    if(TARGET Qt6::QOffscreenIntegrationPlugin)
        add_custom_command(TARGET opennow-qt POST_BUILD
            COMMAND "${CMAKE_COMMAND}" -E make_directory
                    "$<TARGET_FILE_DIR:opennow-qt>/platforms"
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                    "$<TARGET_FILE:Qt6::QOffscreenIntegrationPlugin>"
                    "$<TARGET_FILE_DIR:opennow-qt>/platforms"
            COMMENT "Deploying the Qt offscreen platform plugin for CTest")
    endif()
endif()

target_compile_definitions(opennow-qt PRIVATE
    QT_NO_CAST_FROM_ASCII
    QT_NO_CAST_TO_ASCII
    OPENNOW_EMBEDDED_STREAMER
    OPENNOW_VERSION="${OPENNOW_BUILD_VERSION}"
)

set(OPENNOW_CORE_TARGET_DIR "${CMAKE_BINARY_DIR}/rust-target")
set(OPENNOW_CORE_PROFILE "$<IF:$<CONFIG:Release>,release,debug>")
set(OPENNOW_CORE_SUFFIX "$<IF:$<PLATFORM_ID:Windows>,.exe,>")
set(OPENNOW_RUST_TARGET_ARGS)
set(OPENNOW_CORE_ARTIFACT_ROOT "${OPENNOW_CORE_TARGET_DIR}")
if(OPENNOW_RUST_TARGET)
    list(APPEND OPENNOW_RUST_TARGET_ARGS --target "${OPENNOW_RUST_TARGET}")
    set(OPENNOW_CORE_ARTIFACT_ROOT
        "${OPENNOW_CORE_TARGET_DIR}/${OPENNOW_RUST_TARGET}")
endif()
add_custom_target(opennow-core ALL
    COMMAND "${CMAKE_COMMAND}" -E env
            "OPENNOW_UPDATE_ED25519_PUBLIC_KEY=${OPENNOW_UPDATE_ED25519_PUBLIC_KEY}"
            "OPENNOW_BUILD_VERSION=${OPENNOW_BUILD_VERSION}"
            "${CARGO_EXECUTABLE}" build
            --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-core/Cargo.toml"
            --target-dir "${OPENNOW_CORE_TARGET_DIR}"
            --bin opennow-core
            --bin opennow-acceptance-verify
            ${OPENNOW_RUST_TARGET_ARGS}
            $<$<CONFIG:Release>:--release>
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${OPENNOW_CORE_ARTIFACT_ROOT}/${OPENNOW_CORE_PROFILE}/opennow-core${OPENNOW_CORE_SUFFIX}"
            "$<TARGET_FILE_DIR:opennow-qt>/opennow-core${OPENNOW_CORE_SUFFIX}"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${OPENNOW_CORE_ARTIFACT_ROOT}/${OPENNOW_CORE_PROFILE}/opennow-acceptance-verify${OPENNOW_CORE_SUFFIX}"
            "$<TARGET_FILE_DIR:opennow-qt>/opennow-acceptance-verify${OPENNOW_CORE_SUFFIX}"
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-core"
    COMMENT "Building and bundling the OpenNOW Rust application core"
    COMMAND_EXPAND_LISTS
    VERBATIM
)
add_dependencies(opennow-qt opennow-core)

set(OPENNOW_GENERATED_NOTICES "${CMAKE_BINARY_DIR}/THIRD_PARTY_NOTICES.generated")
add_custom_target(opennow-license-notices ALL
    COMMAND "${CARGO_EXECUTABLE}" build
            --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-core/Cargo.toml"
            --target-dir "${OPENNOW_CORE_TARGET_DIR}"
            --bin opennow-license-report
            $<$<CONFIG:Release>:--release>
    COMMAND
        "${OPENNOW_CORE_TARGET_DIR}/${OPENNOW_CORE_PROFILE}/opennow-license-report${OPENNOW_CORE_SUFFIX}"
        "${CMAKE_CURRENT_SOURCE_DIR}/../THIRD_PARTY_NOTICES"
        "${OPENNOW_GENERATED_NOTICES}"
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-core/Cargo.toml"
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.toml"
    DEPENDS opennow-core
    BYPRODUCTS "${OPENNOW_GENERATED_NOTICES}"
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/.."
    COMMENT "Generating exact Rust dependency license notices"
    COMMAND_EXPAND_LISTS
    VERBATIM
)
add_dependencies(opennow-qt opennow-license-notices)

set(OPENNOW_STREAMER_TARGET_DIR "${CMAKE_BINARY_DIR}/streamer-rust-target" CACHE PATH
    "Streamer Cargo artifacts; use a shorter path if Windows MSBuild exceeds MAX_PATH")
set(OPENNOW_STREAMER_ARTIFACT_ROOT "${OPENNOW_STREAMER_TARGET_DIR}")
if(OPENNOW_RUST_TARGET)
    set(OPENNOW_STREAMER_ARTIFACT_ROOT
        "${OPENNOW_STREAMER_TARGET_DIR}/${OPENNOW_RUST_TARGET}")
endif()
# The embedded streamer decodes and publishes GPU frames in process. A debug
# Rust build is too slow for real-time playback, so link its release library
# into every Qt configuration.
set(OPENNOW_STREAMER_PROFILE "release")
set(OPENNOW_STREAMER_CARGO_FEATURE_ARGS)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    # Distributable Linux builds carry FFmpeg's H.264/HEVC/AV1 software
    # fallback and dynamically load optional GPU driver interfaces. Every Qt
    # configuration now shares one cargo profile directory, so this feature has
    # to stay unconditional: gating it on Release would make each Debug/Release
    # switch rebuild the vendored FFmpeg instead of paying for it once.
    list(APPEND OPENNOW_STREAMER_CARGO_FEATURE_ARGS
        --features linux-ffmpeg-bundled)
endif()
if(MSVC)
    set(OPENNOW_STREAMER_FFI_RUNTIME_NAME "opennow_streamer_ffi.dll")
    set(OPENNOW_STREAMER_FFI_LINK_NAME "opennow_streamer_ffi.dll.lib")
elseif(WIN32)
    # MinGW ld links directly against the DLL; current rustc no longer emits
    # a separate GNU import library for cdylib targets.
    set(OPENNOW_STREAMER_FFI_RUNTIME_NAME "opennow_streamer_ffi.dll")
    set(OPENNOW_STREAMER_FFI_LINK_NAME "${OPENNOW_STREAMER_FFI_RUNTIME_NAME}")
elseif(APPLE)
    set(OPENNOW_STREAMER_FFI_RUNTIME_NAME "libopennow_streamer_ffi.dylib")
    set(OPENNOW_STREAMER_FFI_LINK_NAME "${OPENNOW_STREAMER_FFI_RUNTIME_NAME}")
else()
    set(OPENNOW_STREAMER_FFI_RUNTIME_NAME "libopennow_streamer_ffi.so")
    set(OPENNOW_STREAMER_FFI_LINK_NAME "${OPENNOW_STREAMER_FFI_RUNTIME_NAME}")
endif()
set(OPENNOW_STREAMER_FFI_RUNTIME
    "${OPENNOW_STREAMER_ARTIFACT_ROOT}/${OPENNOW_STREAMER_PROFILE}/${OPENNOW_STREAMER_FFI_RUNTIME_NAME}")
set(OPENNOW_STREAMER_FFI_LINK_LIBRARY
    "${OPENNOW_STREAMER_ARTIFACT_ROOT}/${OPENNOW_STREAMER_PROFILE}/${OPENNOW_STREAMER_FFI_LINK_NAME}")
set(OPENNOW_STREAMER_FFI_ARTIFACTS
    "${OPENNOW_STREAMER_FFI_RUNTIME}" "${OPENNOW_STREAMER_FFI_LINK_LIBRARY}")
list(REMOVE_DUPLICATES OPENNOW_STREAMER_FFI_ARTIFACTS)
file(GLOB_RECURSE OPENNOW_STREAMER_RUST_SOURCES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/crates/*.rs"
    "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/crates/*/Cargo.toml")
add_custom_command(
    OUTPUT ${OPENNOW_STREAMER_FFI_ARTIFACTS}
    COMMAND "${CMAKE_COMMAND}" -E env --unset=MAKEFLAGS --unset=MFLAGS
            "CMAKE=${CMAKE_COMMAND}"
            "${CARGO_EXECUTABLE}" build
            --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.toml"
            --target-dir "${OPENNOW_STREAMER_TARGET_DIR}"
            --package opennow-streamer-ffi
            --lib
            ${OPENNOW_RUST_TARGET_ARGS}
            ${OPENNOW_STREAMER_CARGO_FEATURE_ARGS}
            --release
    DEPENDS
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.lock"
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.toml"
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/crates/opennow-streamer-ffi/include/opennow_streamer_ffi.h"
        ${OPENNOW_STREAMER_RUST_SOURCES}
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer"
    COMMENT "Building the embedded OpenNOW streamer library"
    COMMAND_EXPAND_LISTS
    VERBATIM
)
add_custom_target(opennow-streamer-ffi-build
    DEPENDS ${OPENNOW_STREAMER_FFI_ARTIFACTS})

# The Rust core still probes streamer capabilities through the standalone
# streamer executable next to opennow-core, so build and ship it alongside
# the in-process FFI library. Always release: a debug streamer is too slow
# for real-time use and would only bloat dev trees and packages.
if(WIN32)
    set(OPENNOW_STREAMER_BIN_NAME "opennow-streamer.exe")
else()
    set(OPENNOW_STREAMER_BIN_NAME "opennow-streamer")
endif()
set(OPENNOW_STREAMER_BIN_ARTIFACT
    "${OPENNOW_STREAMER_ARTIFACT_ROOT}/${OPENNOW_STREAMER_PROFILE}/${OPENNOW_STREAMER_BIN_NAME}")
add_custom_command(
    OUTPUT "${OPENNOW_STREAMER_BIN_ARTIFACT}"
    COMMAND "${CMAKE_COMMAND}" -E env --unset=MAKEFLAGS --unset=MFLAGS
            "CMAKE=${CMAKE_COMMAND}"
            "${CARGO_EXECUTABLE}" build
            --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.toml"
            --target-dir "${OPENNOW_STREAMER_TARGET_DIR}"
            --package opennow-streamer
            --bin opennow-streamer
            ${OPENNOW_RUST_TARGET_ARGS}
            ${OPENNOW_STREAMER_CARGO_FEATURE_ARGS}
            --release
    DEPENDS
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.lock"
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.toml"
        ${OPENNOW_STREAMER_RUST_SOURCES}
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer"
    COMMENT "Building the standalone OpenNOW streamer executable"
    COMMAND_EXPAND_LISTS
    VERBATIM
)
add_custom_target(opennow-streamer-bin-build ALL
    DEPENDS "${OPENNOW_STREAMER_BIN_ARTIFACT}")
add_dependencies(opennow-qt opennow-streamer-bin-build)
add_custom_command(TARGET opennow-qt POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${OPENNOW_STREAMER_BIN_ARTIFACT}"
            "$<TARGET_FILE_DIR:opennow-qt>/${OPENNOW_STREAMER_BIN_NAME}")
add_library(opennow-streamer-ffi SHARED IMPORTED GLOBAL)
if(WIN32)
    set_target_properties(opennow-streamer-ffi PROPERTIES
        IMPORTED_IMPLIB "${OPENNOW_STREAMER_FFI_LINK_LIBRARY}"
        IMPORTED_LOCATION "${OPENNOW_STREAMER_FFI_RUNTIME}")
else()
    set_target_properties(opennow-streamer-ffi PROPERTIES
        IMPORTED_LOCATION "${OPENNOW_STREAMER_FFI_LINK_LIBRARY}")
endif()
set_target_properties(opennow-streamer-ffi PROPERTIES
    INTERFACE_INCLUDE_DIRECTORIES
        "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/crates/opennow-streamer-ffi/include")
add_dependencies(opennow-streamer-ffi opennow-streamer-ffi-build)
target_link_libraries(opennow-qt PRIVATE opennow-streamer-ffi)
add_dependencies(opennow-qt opennow-streamer-ffi-build)
add_custom_command(TARGET opennow-qt POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${OPENNOW_STREAMER_FFI_RUNTIME}"
            "$<TARGET_FILE_DIR:opennow-qt>/${OPENNOW_STREAMER_FFI_RUNTIME_NAME}"
    VERBATIM)
# POST_BUILD only runs when the C++ executable itself relinks. Rust-only edits rebuild the FFI
# artifact without relinking OpenNOW, which previously left a stale streamer DLL beside the app.
# Keep the post-build hook for target-specific builds and add an always-considered deployment
# target for normal/default builds; copy_if_different makes the up-to-date case inexpensive.
add_custom_target(opennow-streamer-ffi-deploy ALL
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${OPENNOW_STREAMER_FFI_RUNTIME}"
            "$<TARGET_FILE_DIR:opennow-qt>/${OPENNOW_STREAMER_FFI_RUNTIME_NAME}"
    COMMENT "Deploying the embedded OpenNOW streamer runtime"
    VERBATIM)
add_dependencies(opennow-streamer-ffi-deploy opennow-streamer-ffi-build opennow-qt)
if(APPLE)
    set_property(TARGET opennow-qt APPEND PROPERTY BUILD_RPATH "@loader_path")
    set_property(TARGET opennow-qt APPEND PROPERTY INSTALL_RPATH "@loader_path")
elseif(UNIX)
    set_property(TARGET opennow-qt APPEND PROPERTY BUILD_RPATH "$ORIGIN")
    set_property(TARGET opennow-qt APPEND PROPERTY INSTALL_RPATH "$ORIGIN")
endif()

if(MSVC)
    target_compile_options(opennow-qt PRIVATE /W4 /permissive-)
else()
    target_compile_options(opennow-qt PRIVATE -Wall -Wextra -Wpedantic)
endif()

include(CTest)
if(BUILD_TESTING)
    set(OPENNOW_QT_SMOKE_TIMEOUT 5)
    if(APPLE AND "x86_64" IN_LIST CMAKE_OSX_ARCHITECTURES)
        set(OPENNOW_QT_SMOKE_TIMEOUT 15)
    endif()

    add_executable(opennow-fake-core tests/fake_core.cpp)

    qt_add_executable(opennow-localization-tests
        tests/tst_localization.cpp
        src/Localization.cpp
        src/Localization.h
    )
    target_include_directories(opennow-localization-tests PRIVATE src)
    target_link_libraries(opennow-localization-tests PRIVATE Qt6::Test Qt6::Core)
    target_compile_definitions(opennow-localization-tests PRIVATE
        OPENNOW_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/..")
    qt_add_resources(opennow-localization-tests "opennow-test-locales"
        PREFIX "/locales"
        FILES ${OPENNOW_LOCALE_FILES}
    )
    add_test(NAME opennow-localization-tests
             COMMAND opennow-localization-tests -o -,txt)

    qt_add_executable(opennow-qt-tests
        tests/tst_appcontroller.cpp
        src/AppController.cpp
        src/AppController.h
    )
    target_include_directories(opennow-qt-tests PRIVATE src)
    target_link_libraries(opennow-qt-tests PRIVATE Qt6::Test Qt6::Core Qt6::Gui)
    add_test(NAME opennow-qt-tests COMMAND opennow-qt-tests -o -,txt)
    set_tests_properties(opennow-qt-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

    qt_add_executable(opennow-coreclient-tests
        tests/tst_coreclient.cpp
        src/CoreClient.cpp
        src/CoreClient.h
    )
    target_include_directories(opennow-coreclient-tests PRIVATE src)
    target_link_libraries(opennow-coreclient-tests PRIVATE Qt6::Test Qt6::Core)
    target_compile_definitions(opennow-coreclient-tests PRIVATE OPENNOW_VERSION="${OPENNOW_BUILD_VERSION}")
    add_dependencies(opennow-coreclient-tests opennow-fake-core)
    add_test(NAME opennow-coreclient-tests COMMAND opennow-coreclient-tests -o -,txt)

    qt_add_executable(opennow-streamvideo-tests
        tests/tst_streamvideoitem.cpp
        src/NativeStreamRuntime.cpp
        src/NativeStreamRuntime.h
        src/StreamVideoItem.cpp
        src/StreamVideoItem.h
    )
    target_include_directories(opennow-streamvideo-tests PRIVATE src)
    target_link_libraries(opennow-streamvideo-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui Qt6::GuiPrivate Qt6::Qml Qt6::Quick
        opennow-streamer-ffi)
    if(WIN32)
        target_link_libraries(opennow-streamvideo-tests PRIVATE user32)
    endif()
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
        target_link_libraries(opennow-streamvideo-tests PRIVATE Vulkan::Vulkan)
    endif()
    add_dependencies(opennow-streamvideo-tests opennow-streamer-ffi-build)
    add_test(NAME opennow-streamvideo-tests
             COMMAND opennow-streamvideo-tests -o -,txt)
    set_tests_properties(opennow-streamvideo-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

    qt_add_executable(opennow-nativestreamruntime-tests
        tests/tst_nativestreamruntime.cpp
        src/NativeStreamRuntime.cpp
        src/NativeStreamRuntime.h
    )
    target_include_directories(opennow-nativestreamruntime-tests PRIVATE src)
    target_link_libraries(opennow-nativestreamruntime-tests PRIVATE
        Qt6::Test Qt6::Core opennow-streamer-ffi)
    add_dependencies(opennow-nativestreamruntime-tests opennow-streamer-ffi-build)
    add_test(NAME opennow-nativestreamruntime-tests
             COMMAND opennow-nativestreamruntime-tests -o -,txt)
    set_tests_properties(opennow-nativestreamruntime-tests PROPERTIES TIMEOUT 8)

    if(APPLE)
        add_custom_target(opennow-streamer-ffi-test-runtime ALL
            COMMAND ${CMAKE_COMMAND} -E make_directory
                "$<TARGET_FILE_DIR:opennow-streamvideo-tests>"
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${OPENNOW_STREAMER_FFI_RUNTIME}"
                "$<TARGET_FILE_DIR:opennow-streamvideo-tests>/"
            COMMAND ${CMAKE_COMMAND} -E make_directory
                "$<TARGET_FILE_DIR:opennow-nativestreamruntime-tests>"
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${OPENNOW_STREAMER_FFI_RUNTIME}"
                "$<TARGET_FILE_DIR:opennow-nativestreamruntime-tests>/"
            DEPENDS opennow-streamer-ffi-build
            COMMENT "Deploying the embedded streamer runtime for CTest")
        foreach(test_target IN ITEMS
                opennow-streamvideo-tests
                opennow-nativestreamruntime-tests)
            add_dependencies(${test_target} opennow-streamer-ffi-test-runtime)
            set_property(TARGET ${test_target} APPEND PROPERTY BUILD_RPATH "@loader_path")
        endforeach()
    endif()

    qt_add_executable(opennow-embedded-orchestration-tests
        tests/tst_embeddedorchestration.cpp
    )
    target_link_libraries(opennow-embedded-orchestration-tests PRIVATE Qt6::Test Qt6::Core Qt6::Qml)
    target_compile_definitions(opennow-embedded-orchestration-tests PRIVATE
        OPENNOW_QT_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME opennow-embedded-orchestration-tests
             COMMAND opennow-embedded-orchestration-tests -o -,txt)

    qt_add_executable(opennow-singleinstance-tests
        tests/tst_singleinstance.cpp
        src/SingleInstance.cpp
        src/SingleInstance.h
    )
    target_include_directories(opennow-singleinstance-tests PRIVATE src)
    target_link_libraries(opennow-singleinstance-tests PRIVATE Qt6::Test Qt6::Core Qt6::Network)
    add_test(NAME opennow-singleinstance-tests
             COMMAND opennow-singleinstance-tests -o -,txt)

    qt_add_executable(opennow-thumbnail-tests
        tests/tst_thumbnailgenerator.cpp
        src/ThumbnailGenerator.cpp
        src/ThumbnailGenerator.h
    )
    target_include_directories(opennow-thumbnail-tests PRIVATE src)
    target_link_libraries(opennow-thumbnail-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui Qt6::Multimedia)
    add_test(NAME opennow-thumbnail-tests COMMAND opennow-thumbnail-tests -o -,txt)
    set_tests_properties(opennow-thumbnail-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 8
    )
    qt_add_executable(opennow-controllerinput-tests
        tests/tst_controllerinput.cpp
        src/ControllerInput.cpp
        src/ControllerInput.h
    )
    target_include_directories(opennow-controllerinput-tests PRIVATE src)
    target_link_libraries(opennow-controllerinput-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui SDL3::SDL3)
    add_test(NAME opennow-controllerinput-tests
             COMMAND opennow-controllerinput-tests -o -,txt)
    set_tests_properties(opennow-controllerinput-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 8
    )
    if(WIN32)
        # Qt's executable helper defaults to the GUI subsystem on Windows. Keep
        # test runners as console programs so CTest captures QtTest failures.
        set_target_properties(
            opennow-localization-tests
            opennow-qt-tests
            opennow-coreclient-tests
            opennow-streamvideo-tests
            opennow-nativestreamruntime-tests
            opennow-embedded-orchestration-tests
            opennow-singleinstance-tests
            opennow-thumbnail-tests
            opennow-controllerinput-tests
            PROPERTIES WIN32_EXECUTABLE FALSE)

        # windeployqt follows the application graph and therefore does not copy
        # Qt6Test.dll. All unit-test runners share the application output
        # directory, so deploy the test runtime explicitly and make the runners
        # depend on it. This keeps a clean build directly runnable by CTest.
        add_custom_target(opennow-qt-test-runtime ALL
            COMMAND ${CMAKE_COMMAND} -E make_directory
                "$<TARGET_FILE_DIR:opennow-qt>"
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "$<TARGET_FILE:Qt6::Test>"
                "$<TARGET_FILE_DIR:opennow-qt>/"
            COMMENT "Deploying the Qt Test runtime for CTest")
        foreach(test_target IN ITEMS
                opennow-localization-tests
                opennow-qt-tests
                opennow-coreclient-tests
                opennow-streamvideo-tests
                opennow-nativestreamruntime-tests
                opennow-embedded-orchestration-tests
                opennow-singleinstance-tests
                opennow-thumbnail-tests
                opennow-controllerinput-tests)
            add_dependencies(${test_target} opennow-qt-test-runtime)
        endforeach()
    endif()

    foreach(route home library store theme-store controllers settings settings-account settings-streaming settings-video settings-video-dropdown settings-input settings-network settings-themes settings-advanced settings-advanced-dropdown game-detail game-detail-platform-dropdown sign-in joining inserting stream accounts profile-pin game-accounts persistent-storage media diagnostics updates feedback)
        add_test(NAME "qml-route-${route}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances
                         --console --route "${route}" --reduced-motion)
        set_tests_properties("qml-route-${route}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    foreach(route home library store friends settings stream)
        add_test(NAME "qml-desktop-route-${route}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop --route "${route}" --reduced-motion)
        set_tests_properties("qml-desktop-route-${route}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    foreach(overlay desktop-stream-menu desktop-stream-stats desktop-stream-stats-expanded desktop-stream-exit-confirm)
        add_test(NAME "qml-${overlay}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances
                         --desktop --route stream --overlay "${overlay}" --reduced-motion)
        set_tests_properties("qml-${overlay}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    add_test(NAME qml-fullscreen-stream-stats-shortcut
             COMMAND opennow-qt --smoke-test --allow-multiple-instances
                     --desktop --route stream --smoke-fullscreen-stats-shortcut --reduced-motion)
    set_tests_properties(qml-fullscreen-stream-stats-shortcut PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 5
    )
    add_test(NAME qml-console-mode-persistence-rollback
             COMMAND opennow-qt
                     --smoke-test
                     --allow-multiple-instances
                     --core "$<TARGET_FILE:opennow-fake-core>"
                     --smoke-console-persistence-rollback
                     --reduced-motion)
    set_tests_properties(qml-console-mode-persistence-rollback PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
    )
    add_test(NAME qml-streamer-event-contract
             COMMAND opennow-qt
                     --smoke-test
                     --allow-multiple-instances
                     --core "$<TARGET_FILE:opennow-fake-core>"
                     --smoke-streamer-event
                     --reduced-motion)
    set_tests_properties(qml-streamer-event-contract PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
    )
    foreach(overlay friends friend-actions quick-settings session-conflict session-report queue-ad guide-session guide-controls guide-media guide-shortcuts)
        add_test(NAME "qml-overlay-${overlay}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances --route home --overlay "${overlay}" --reduced-motion)
        set_tests_properties("qml-overlay-${overlay}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    add_test(NAME opennow-performance-report-harness
             COMMAND opennow-qt
                     --allow-multiple-instances
                     --performance-report "${CMAKE_BINARY_DIR}/performance-harness.json"
                     --performance-width 960
                     --performance-height 540
                     --performance-cycles 1
                     --performance-refresh-hz 30
                     --performance-label ci-harness)
    set_tests_properties(opennow-performance-report-harness PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        RUN_SERIAL TRUE
        TIMEOUT 15
    )
    add_test(NAME opennow-performance-relative-path-rejected
             COMMAND opennow-qt
                     --allow-multiple-instances
                     --performance-report performance-report-must-be-absolute.json
                     --performance-width 960
                     --performance-height 540
                     --performance-cycles 1)
    set_tests_properties(opennow-performance-relative-path-rejected PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        WILL_FAIL TRUE
        TIMEOUT 5
    )
    add_test(NAME opennow-acceptance-verifier-help
             COMMAND "$<TARGET_FILE_DIR:opennow-qt>/opennow-acceptance-verify${OPENNOW_CORE_SUFFIX}"
                     --help)
    set_tests_properties(opennow-acceptance-verifier-help PROPERTIES TIMEOUT 5)
endif()

install(TARGETS opennow-qt
    BUNDLE DESTINATION .
    RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}"
)

if(WIN32)
    install(IMPORTED_RUNTIME_ARTIFACTS ${OPENNOW_SDL3_RUNTIME_TARGET}
        RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}"
    )
elseif(APPLE)
    install(IMPORTED_RUNTIME_ARTIFACTS ${OPENNOW_SDL3_RUNTIME_TARGET}
        LIBRARY DESTINATION "${OPENNOW_EXECUTABLE_NAME}.app/Contents/Frameworks"
        FRAMEWORK DESTINATION "${OPENNOW_EXECUTABLE_NAME}.app/Contents/Frameworks"
    )
endif()

if(NOT APPLE)
    install(PROGRAMS
        "$<TARGET_FILE_DIR:opennow-qt>/opennow-core${OPENNOW_CORE_SUFFIX}"
        "$<TARGET_FILE_DIR:opennow-qt>/opennow-acceptance-verify${OPENNOW_CORE_SUFFIX}"
        DESTINATION "${CMAKE_INSTALL_BINDIR}"
    )
    install(PROGRAMS "${OPENNOW_STREAMER_FFI_RUNTIME}"
        DESTINATION "${CMAKE_INSTALL_BINDIR}")
    install(PROGRAMS "${OPENNOW_STREAMER_BIN_ARTIFACT}"
        DESTINATION "${CMAKE_INSTALL_BINDIR}")
else()
    install(FILES "${OPENNOW_STREAMER_FFI_RUNTIME}"
        DESTINATION "${OPENNOW_EXECUTABLE_NAME}.app/Contents/MacOS")
    install(PROGRAMS "${OPENNOW_STREAMER_BIN_ARTIFACT}"
        DESTINATION "${OPENNOW_EXECUTABLE_NAME}.app/Contents/MacOS")
endif()

if(APPLE)
    set(OPENNOW_LICENSE_DESTINATION "${OPENNOW_EXECUTABLE_NAME}.app/Contents/Resources/licenses")
else()
    set(OPENNOW_LICENSE_DESTINATION "${CMAKE_INSTALL_DATADIR}/doc/opennow")
endif()
install(FILES "${CMAKE_CURRENT_SOURCE_DIR}/../LICENSE"
    DESTINATION "${OPENNOW_LICENSE_DESTINATION}"
)
install(FILES "${OPENNOW_GENERATED_NOTICES}"
    DESTINATION "${OPENNOW_LICENSE_DESTINATION}"
    RENAME THIRD_PARTY_NOTICES
)
install(DIRECTORY packaging/licenses/
    DESTINATION "${OPENNOW_LICENSE_DESTINATION}"
)

if(UNIX AND NOT APPLE)
    install(FILES packaging/io.github.opencloudgaming.OpenNOW.desktop
        DESTINATION "${CMAKE_INSTALL_DATADIR}/applications")
    install(FILES packaging/io.github.opencloudgaming.OpenNOW.metainfo.xml
        DESTINATION "${CMAKE_INSTALL_DATADIR}/metainfo")
    install(FILES packaging/io.github.opencloudgaming.OpenNOW.svg
        DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps")
endif()

if(WIN32 OR APPLE)
    qt_generate_deploy_qml_app_script(
        TARGET opennow-qt
        OUTPUT_SCRIPT opennow_deploy_script
        NO_UNSUPPORTED_PLATFORM_ERROR
        ${OPENNOW_QT_DEPLOY_TOOL_ARGS}
    )
    install(SCRIPT "${opennow_deploy_script}")
endif()

set(CPACK_PACKAGE_NAME "OpenNOW")
set(CPACK_PACKAGE_VENDOR "OpenCloudGaming")
set(CPACK_PACKAGE_CONTACT "OpenCloudGaming <support@opennow.app>")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "Controller-first GeForce NOW client")
set(CPACK_PACKAGE_HOMEPAGE_URL "https://github.com/OpenCloudGaming/OpenNOW")
set(CPACK_PACKAGE_VERSION "${OPENNOW_BUILD_VERSION}")
string(REPLACE "." ";" OPENNOW_BUILD_VERSION_PARTS "${OPENNOW_BUILD_VERSION}")
list(GET OPENNOW_BUILD_VERSION_PARTS 0 CPACK_PACKAGE_VERSION_MAJOR)
list(GET OPENNOW_BUILD_VERSION_PARTS 1 CPACK_PACKAGE_VERSION_MINOR)
list(GET OPENNOW_BUILD_VERSION_PARTS 2 CPACK_PACKAGE_VERSION_PATCH)
set(CPACK_PACKAGE_FILE_NAME "OpenNOW-Qt-${OPENNOW_BUILD_VERSION}-${CMAKE_SYSTEM_NAME}-${CMAKE_SYSTEM_PROCESSOR}")
set(CPACK_STRIP_FILES TRUE)
if(WIN32)
    set(CPACK_GENERATOR "WIX;ZIP")
    set(CPACK_WIX_UPGRADE_GUID "6E81F7AE-B19D-4E87-A94A-2B2F01EBF762")
elseif(APPLE)
    set(CPACK_GENERATOR "DragNDrop;ZIP")
    set(CPACK_DMG_VOLUME_NAME "OpenNOW")
else()
    # Linux packages intentionally use the distribution Qt runtime. AppImage
    # assembly uses linuxdeploy in release CI so plugin selection is explicit.
    set(CPACK_GENERATOR "DEB")
    set(CPACK_DEBIAN_PACKAGE_SECTION "games")
    set(CPACK_DEBIAN_FILE_NAME DEB-DEFAULT)
    set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS TRUE)
    get_target_property(OPENNOW_SDL3_RUNTIME_LOCATION
        ${OPENNOW_SDL3_RUNTIME_TARGET} IMPORTED_LOCATION_RELEASE)
    if(NOT OPENNOW_SDL3_RUNTIME_LOCATION)
        get_target_property(OPENNOW_SDL3_RUNTIME_LOCATION
            ${OPENNOW_SDL3_RUNTIME_TARGET} IMPORTED_LOCATION)
    endif()
    if(OPENNOW_SDL3_RUNTIME_LOCATION)
        get_filename_component(OPENNOW_SDL3_RUNTIME_DIRECTORY
            "${OPENNOW_SDL3_RUNTIME_LOCATION}" DIRECTORY)
        set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS_PRIVATE_DIRS
            "${OPENNOW_SDL3_RUNTIME_DIRECTORY}")
    endif()
    set(CPACK_DEBIAN_PACKAGE_DEPENDS
        "libqt6core6 (>= 6.8) | libqt6core6t64 (>= 6.8), libqt6gui6 (>= 6.8), libqt6network6 (>= 6.8), libqt6qml6 (>= 6.8), libqt6quick6 (>= 6.8), libqt6quickcontrols2-6 (>= 6.8), libqt6multimedia6 (>= 6.8), qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-dialogs, qml6-module-qtquick-effects, qml6-module-qtmultimedia, libsdl3-0 | libsdl3-0.0")
    if(CMAKE_SYSTEM_PROCESSOR MATCHES "^(x86_64|amd64|AMD64)$")
        set(CPACK_DEBIAN_PACKAGE_ARCHITECTURE "amd64")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "^(aarch64|arm64|ARM64)$")
        set(CPACK_DEBIAN_PACKAGE_ARCHITECTURE "arm64")
    endif()
endif()
include(CPack)
