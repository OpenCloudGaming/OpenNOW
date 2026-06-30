cmake_minimum_required(VERSION 3.22.1)

project(opennow_native)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_library(opennow_native SHARED opennow_native.cpp)

target_compile_options(opennow_native PRIVATE -fexceptions -frtti)

find_library(log-lib log)
find_library(android-lib android)
find_library(mediandk-lib mediandk)

target_link_libraries(
    opennow_native
    ${android-lib}
    ${log-lib}
    ${mediandk-lib}
)
