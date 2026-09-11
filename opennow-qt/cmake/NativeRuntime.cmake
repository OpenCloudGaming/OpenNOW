set(OPENNOW_CORE_TARGET_DIR "${CMAKE_BINARY_DIR}/rust-target")
set(OPENNOW_CORE_PROFILE "$<IF:$<CONFIG:Release>,release,debug>")
set(OPENNOW_CORE_SUFFIX "$<IF:$<PLATFORM_ID:Windows>,.exe,>")
if(APPLE)
    if(OPENNOW_PACKAGE_ARCH STREQUAL "arm64")
        set(OPENNOW_MACOS_RUST_TARGET "aarch64-apple-darwin")
    else()
        set(OPENNOW_MACOS_RUST_TARGET "x86_64-apple-darwin")
    endif()
    if(OPENNOW_RUST_TARGET AND NOT OPENNOW_RUST_TARGET STREQUAL OPENNOW_MACOS_RUST_TARGET)
        message(FATAL_ERROR "OPENNOW_RUST_TARGET must match the macOS Qt architecture: ${OPENNOW_MACOS_RUST_TARGET}")
    endif()
    set(OPENNOW_RUST_TARGET "${OPENNOW_MACOS_RUST_TARGET}")
endif()
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
    # fallback and native H.264 VAAPI with host-provided GPU drivers. Every Qt
    # configuration now shares one cargo profile directory, so this feature has
    # to stay unconditional: gating it on Release would make each Debug/Release
    # switch rebuild the vendored FFmpeg instead of paying for it once.
    list(APPEND OPENNOW_STREAMER_CARGO_FEATURE_ARGS
        --features linux-ffmpeg-bundled,linux-vaapi)
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(OPENNOW_VAAPI REQUIRED libva libva-drm)
    if(OPENNOW_RUST_TARGET MATCHES "^aarch64-.*-linux-"
        OR (NOT OPENNOW_RUST_TARGET AND CMAKE_SYSTEM_PROCESSOR MATCHES "^(aarch64|arm64|ARM64)$"))
        pkg_check_modules(OPENNOW_V4L2_REQUEST REQUIRED libdrm libudev)
    endif()
endif()
set(OPENNOW_RUST_EFFECTIVE_TARGET "${OPENNOW_RUST_TARGET}")
if(WIN32 AND NOT OPENNOW_RUST_EFFECTIVE_TARGET)
    find_program(OPENNOW_RUSTC_EXECUTABLE rustc REQUIRED)
    execute_process(COMMAND "${OPENNOW_RUSTC_EXECUTABLE}" -vV
        OUTPUT_VARIABLE OPENNOW_RUSTC_VERSION
        RESULT_VARIABLE OPENNOW_RUSTC_RESULT)
    if(NOT OPENNOW_RUSTC_RESULT EQUAL 0)
        message(FATAL_ERROR "Could not determine the Rust host target")
    endif()
    string(REGEX MATCH "host: ([^\r\n]+)" OPENNOW_RUST_HOST_MATCH "${OPENNOW_RUSTC_VERSION}")
    set(OPENNOW_RUST_EFFECTIVE_TARGET "${CMAKE_MATCH_1}")
endif()

set(OPENNOW_UPDATE_HELPER_TARGET_DIR "${OPENNOW_CORE_TARGET_DIR}")
set(OPENNOW_UPDATE_HELPER_ARTIFACT_ROOT "${OPENNOW_CORE_ARTIFACT_ROOT}")
set(OPENNOW_UPDATE_HELPER_TARGET_ARGS ${OPENNOW_RUST_TARGET_ARGS})
set(OPENNOW_UPDATE_HELPER_ENV)
if(WIN32)
    set(OPENNOW_UPDATE_HELPER_TARGET_DIR "${CMAKE_BINARY_DIR}/update-helper-rust-target")
    set(OPENNOW_UPDATE_HELPER_ARTIFACT_ROOT
        "${OPENNOW_UPDATE_HELPER_TARGET_DIR}/${OPENNOW_RUST_EFFECTIVE_TARGET}")
    set(OPENNOW_UPDATE_HELPER_TARGET_ARGS --target "${OPENNOW_RUST_EFFECTIVE_TARGET}")
    set(OPENNOW_UPDATE_HELPER_ENV --unset=CARGO_ENCODED_RUSTFLAGS
        "RUSTFLAGS=-C target-feature=+crt-static")
endif()
add_custom_target(opennow-update-helper-build ALL
    COMMAND "${CMAKE_COMMAND}" -E env ${OPENNOW_UPDATE_HELPER_ENV}
            "OPENNOW_UPDATE_ED25519_PUBLIC_KEY=${OPENNOW_UPDATE_ED25519_PUBLIC_KEY}"
            "OPENNOW_BUILD_VERSION=${OPENNOW_BUILD_VERSION}"
            "${CARGO_EXECUTABLE}" build
            --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-core/Cargo.toml"
            --target-dir "${OPENNOW_UPDATE_HELPER_TARGET_DIR}"
            --bin opennow-update-helper
            ${OPENNOW_UPDATE_HELPER_TARGET_ARGS}
            $<$<CONFIG:Release>:--release>
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${OPENNOW_UPDATE_HELPER_ARTIFACT_ROOT}/${OPENNOW_CORE_PROFILE}/opennow-update-helper${OPENNOW_CORE_SUFFIX}"
            "$<TARGET_FILE_DIR:opennow-qt>/opennow-update-helper${OPENNOW_CORE_SUFFIX}"
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-core"
    COMMAND_EXPAND_LISTS
    VERBATIM
)
if(NOT WIN32)
    add_dependencies(opennow-update-helper-build opennow-core)
endif()
add_dependencies(opennow-qt opennow-update-helper-build)

# The Rust target determines its import-library format, not the Qt compiler.
# MinGW can consume the MSVC COFF import library for our C-only FFI.
if(WIN32 AND MINGW AND OPENNOW_RUST_EFFECTIVE_TARGET MATCHES "-msvc$")
    # windeployqt sees the MinGW Qt toolchain, not the MSVC Rust toolchain.
    # Bundle the redistributable CRT as well; a developer's System32 can mask
    # this missing dependency until the package reaches a clean machine.
    set(OPENNOW_MSVC_REDIST_DIR "" CACHE PATH "MSVC CRT redistributable directory for the Rust target")
    if(NOT OPENNOW_MSVC_REDIST_DIR)
        find_program(OPENNOW_VSWHERE_EXECUTABLE NAMES vswhere
            HINTS "$ENV{ProgramFiles\(x86\)}/Microsoft Visual Studio/Installer")
        if(OPENNOW_VSWHERE_EXECUTABLE)
            execute_process(COMMAND "${OPENNOW_VSWHERE_EXECUTABLE}" -latest -products * -property installationPath
                OUTPUT_VARIABLE OPENNOW_VS_INSTALL OUTPUT_STRIP_TRAILING_WHITESPACE)
            file(TO_CMAKE_PATH "${OPENNOW_VS_INSTALL}" OPENNOW_VS_INSTALL)
            if(OPENNOW_RUST_EFFECTIVE_TARGET MATCHES "^aarch64")
                set(OPENNOW_CRT_ARCH arm64)
            elseif(OPENNOW_RUST_EFFECTIVE_TARGET MATCHES "^i686")
                set(OPENNOW_CRT_ARCH x86)
            else()
                set(OPENNOW_CRT_ARCH x64)
            endif()
            file(GLOB OPENNOW_CRT_DIRECTORIES "${OPENNOW_VS_INSTALL}/VC/Redist/MSVC/*/${OPENNOW_CRT_ARCH}/Microsoft.VC*.CRT")
            list(SORT OPENNOW_CRT_DIRECTORIES COMPARE NATURAL ORDER DESCENDING)
            if(OPENNOW_CRT_DIRECTORIES)
                list(GET OPENNOW_CRT_DIRECTORIES 0 OPENNOW_MSVC_REDIST_DIR)
            endif()
        endif()
    endif()
    # Install scripts require CMake-style paths, including user-supplied paths.
    file(TO_CMAKE_PATH "${OPENNOW_MSVC_REDIST_DIR}" OPENNOW_MSVC_REDIST_DIR)
    if(NOT EXISTS "${OPENNOW_MSVC_REDIST_DIR}/vcruntime140.dll")
        message(FATAL_ERROR "Set OPENNOW_MSVC_REDIST_DIR to the Visual C++ CRT redistributable directory matching the Rust target")
    endif()
    file(GLOB OPENNOW_MSVC_RUNTIME_FILES "${OPENNOW_MSVC_REDIST_DIR}/*.dll")
    add_custom_target(opennow-msvc-runtime ALL
        COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:opennow-qt>"
        COMMAND "${CMAKE_COMMAND}" -E copy_if_different ${OPENNOW_MSVC_RUNTIME_FILES} "$<TARGET_FILE_DIR:opennow-qt>"
        COMMENT "Deploying the MSVC runtime required by the Rust core and streamer"
        COMMAND_EXPAND_LISTS)
    add_dependencies(opennow-qt opennow-msvc-runtime)
    install(FILES ${OPENNOW_MSVC_RUNTIME_FILES} DESTINATION "${CMAKE_INSTALL_BINDIR}")
endif()
if(WIN32 AND OPENNOW_RUST_EFFECTIVE_TARGET MATCHES "-msvc$")
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
set(OPENNOW_STREAMER_FFI_CARGO_COMMAND build)
set(OPENNOW_STREAMER_FFI_RUSTC_ARGS)
if(APPLE)
    set(OPENNOW_STREAMER_FFI_CARGO_COMMAND rustc)
    set(OPENNOW_STREAMER_FFI_RUSTC_ARGS -- -C rpath=yes)
endif()
file(GLOB_RECURSE OPENNOW_STREAMER_RUST_SOURCES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/crates/*.rs"
    "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/crates/*/Cargo.toml")
add_custom_command(
    OUTPUT ${OPENNOW_STREAMER_FFI_ARTIFACTS}
    COMMAND "${CMAKE_COMMAND}" -E env --unset=MAKEFLAGS --unset=MFLAGS
            "CMAKE=${CMAKE_COMMAND}"
            "${CARGO_EXECUTABLE}" ${OPENNOW_STREAMER_FFI_CARGO_COMMAND}
            --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../native/opennow-streamer/Cargo.toml"
            --target-dir "${OPENNOW_STREAMER_TARGET_DIR}"
            --package opennow-streamer-ffi
            --lib
            ${OPENNOW_RUST_TARGET_ARGS}
            ${OPENNOW_STREAMER_CARGO_FEATURE_ARGS}
            --release
            ${OPENNOW_STREAMER_FFI_RUSTC_ARGS}
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
