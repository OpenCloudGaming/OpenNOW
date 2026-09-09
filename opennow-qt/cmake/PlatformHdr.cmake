add_library(opennow-platform-hdr STATIC
    "${CMAKE_CURRENT_LIST_DIR}/../src/streaming/rendering/WaylandHdrOutput.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/../src/streaming/rendering/WaylandHdrOutput.h")
target_link_libraries(opennow-platform-hdr PUBLIC Qt6::Gui PRIVATE Qt6::GuiPrivate)
target_include_directories(opennow-platform-hdr PUBLIC "${CMAKE_CURRENT_LIST_DIR}/../src")

if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    find_package(PkgConfig QUIET)
    if(PkgConfig_FOUND)
        pkg_check_modules(HDR_WAYLAND_CLIENT QUIET IMPORTED_TARGET wayland-client)
    endif()
    find_program(HDR_WAYLAND_SCANNER wayland-scanner)
    set(hdr_protocol_xml "${CMAKE_CURRENT_LIST_DIR}/../protocols/color-management-v1.xml")
    if(HDR_WAYLAND_CLIENT_FOUND AND HDR_WAYLAND_SCANNER)
        enable_language(C)
        set(hdr_protocol_dir "${CMAKE_CURRENT_BINARY_DIR}/hdr-protocols")
        file(MAKE_DIRECTORY "${hdr_protocol_dir}")
        set(hdr_protocol_header "${hdr_protocol_dir}/color-management-v1-client-protocol.h")
        set(hdr_protocol_code "${hdr_protocol_dir}/color-management-v1-protocol.c")
        add_custom_command(OUTPUT "${hdr_protocol_header}" "${hdr_protocol_code}"
            COMMAND "${HDR_WAYLAND_SCANNER}" client-header "${hdr_protocol_xml}" "${hdr_protocol_header}"
            COMMAND "${HDR_WAYLAND_SCANNER}" private-code "${hdr_protocol_xml}" "${hdr_protocol_code}"
            DEPENDS "${hdr_protocol_xml}" VERBATIM)
        target_sources(opennow-platform-hdr PRIVATE "${hdr_protocol_header}" "${hdr_protocol_code}")
        target_include_directories(opennow-platform-hdr PRIVATE "${hdr_protocol_dir}")
        target_compile_definitions(opennow-platform-hdr PRIVATE OPENNOW_WAYLAND_HDR)
        target_link_libraries(opennow-platform-hdr PRIVATE PkgConfig::HDR_WAYLAND_CLIENT)
        message(STATUS "Wayland HDR output observer enabled with bundled color-management-v1 protocol")
    else()
        message(STATUS "Wayland client or scanner unavailable; HDR output observation disabled")
    endif()
endif()
