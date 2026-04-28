cmake_minimum_required(VERSION 3.21)

project(OpenNOWCore VERSION 0.1.0 LANGUAGES CXX)

include(GNUInstallDirs)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(OPENNOW_BUILD_TESTS "Build native core tests" ON)

add_library(opennow_core
  src/input_protocol.cpp
  src/open_now_core.cpp
  src/service_builders.cpp
  src/service_clients.cpp
  src/service_protocol.cpp
  src/service_transport.cpp
  src/settings.cpp
  src/sdp.cpp
  src/stream_client.cpp
)

target_include_directories(opennow_core
  PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
)

if(APPLE)
  target_compile_definitions(opennow_core PUBLIC OPENNOW_PLATFORM_MACOS=1)
endif()

if(MSVC)
  target_compile_options(opennow_core PRIVATE /W4)
else()
  target_compile_options(opennow_core PRIVATE -Wall -Wextra -Wpedantic)
endif()

install(TARGETS opennow_core
  EXPORT OpenNOWCoreTargets
  ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
  LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
  RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)

install(DIRECTORY include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})

install(EXPORT OpenNOWCoreTargets
  NAMESPACE OpenNOW::
  DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/OpenNOWCore
)

if(OPENNOW_BUILD_TESTS)
  enable_testing()

  add_executable(opennow_core_tests
    tests/test_core.cpp
  )

  target_link_libraries(opennow_core_tests PRIVATE opennow_core)

  add_test(NAME opennow_core_tests COMMAND opennow_core_tests)
endif()

set(CPACK_PACKAGE_NAME "OpenNOWNativeCore")
set(CPACK_PACKAGE_VENDOR "OpenNOW")
set(CPACK_PACKAGE_VERSION "${PROJECT_VERSION}")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "OpenNOW native macOS C++ core")
set(CPACK_PACKAGE_FILE_NAME
  "opennow-native-core-${PROJECT_VERSION}-${CMAKE_SYSTEM_NAME}-${CMAKE_SYSTEM_PROCESSOR}"
)
set(CPACK_GENERATOR "TGZ")

include(CPack)
