add_library(opennow-platform-input STATIC
    "${CMAKE_CURRENT_LIST_DIR}/../src/input/platform/MacPointerCapture.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/../src/input/platform/MacPointerCapture.h"
    "${CMAKE_CURRENT_LIST_DIR}/../src/input/platform/WaylandPointerCapture.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/../src/input/platform/WaylandPointerCapture.h")
target_link_libraries(opennow-platform-input PUBLIC Qt6::Gui PRIVATE Qt6::GuiPrivate)
target_include_directories(opennow-platform-input PUBLIC "${CMAKE_CURRENT_LIST_DIR}/../src")

if(APPLE)
    enable_language(OBJCXX)
    set_property(TARGET opennow-platform-input PROPERTY OBJCXX_STANDARD 20)
    set_property(TARGET opennow-platform-input PROPERTY OBJCXX_STANDARD_REQUIRED ON)
    target_sources(opennow-platform-input PRIVATE
        "${CMAKE_CURRENT_LIST_DIR}/../src/input/platform/MacPointerCapture.mm")
    target_link_libraries(opennow-platform-input PRIVATE "-framework AppKit" "-framework CoreGraphics")
endif()

if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    enable_language(C)
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(WAYLAND_CLIENT REQUIRED IMPORTED_TARGET wayland-client)
    pkg_check_modules(WAYLAND_PROTOCOLS REQUIRED wayland-protocols)
    pkg_get_variable(WAYLAND_PROTOCOLS_DIR wayland-protocols pkgdatadir)
    find_program(WAYLAND_SCANNER wayland-scanner REQUIRED)
    set(OPENNOW_INPUT_PROTOCOL_DIR "${CMAKE_CURRENT_BINARY_DIR}/input-protocols")
    file(MAKE_DIRECTORY "${OPENNOW_INPUT_PROTOCOL_DIR}")
    foreach(protocol relative-pointer pointer-constraints)
        set(protocol_name "${protocol}-unstable-v1")
        set(protocol_xml "${WAYLAND_PROTOCOLS_DIR}/unstable/${protocol}/${protocol_name}.xml")
        set(protocol_header "${OPENNOW_INPUT_PROTOCOL_DIR}/${protocol_name}-client-protocol.h")
        set(protocol_code "${OPENNOW_INPUT_PROTOCOL_DIR}/${protocol_name}-protocol.c")
        add_custom_command(OUTPUT "${protocol_header}" "${protocol_code}"
            COMMAND "${WAYLAND_SCANNER}" client-header "${protocol_xml}" "${protocol_header}"
            COMMAND "${WAYLAND_SCANNER}" private-code "${protocol_xml}" "${protocol_code}"
            DEPENDS "${protocol_xml}" VERBATIM)
        target_sources(opennow-platform-input PRIVATE "${protocol_header}" "${protocol_code}")
    endforeach()
    target_include_directories(opennow-platform-input PRIVATE "${OPENNOW_INPUT_PROTOCOL_DIR}")
    target_compile_definitions(opennow-platform-input PRIVATE OPENNOW_WAYLAND_INPUT)
    target_link_libraries(opennow-platform-input PRIVATE PkgConfig::WAYLAND_CLIENT)
endif()
