# audiopus_sys builds the bundled Opus source with CMake for the selected APK ABI.
set(ANDROID_ABI "$ENV{OPENNOW_ANDROID_ABI}" CACHE STRING "" FORCE)
set(ANDROID_PLATFORM android-23 CACHE STRING "" FORCE)
set(CMAKE_POSITION_INDEPENDENT_CODE ON CACHE BOOL "" FORCE)
include("$ENV{OPENNOW_ANDROID_NDK}/build/cmake/android.toolchain.cmake")
