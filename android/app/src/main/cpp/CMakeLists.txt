cmake_minimum_required(VERSION 3.22.1)

project(opennow_native)

add_library(opennow_native SHARED opennow_native.cpp)

find_library(log-lib log)
find_library(android-lib android)
find_library(mediandk-lib mediandk)

target_link_libraries(
    opennow_native
    ${android-lib}
    ${log-lib}
    ${mediandk-lib}
)
