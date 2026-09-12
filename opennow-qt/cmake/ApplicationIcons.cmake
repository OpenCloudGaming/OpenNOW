set(OPENNOW_APPLICATION_ICON_SIZES 16 24 32 48 64 128 256 512 1024)
set(OPENNOW_APPLICATION_ICON_FILES)
foreach(size IN LISTS OPENNOW_APPLICATION_ICON_SIZES)
    list(APPEND OPENNOW_APPLICATION_ICON_FILES "packaging/icons/opennow-${size}.png")
endforeach()

function(opennow_add_application_icons target)
    qt_add_resources(${target} "${target}-application-icons"
        PREFIX "/icons" BASE "packaging/icons"
        FILES ${OPENNOW_APPLICATION_ICON_FILES})
endfunction()

opennow_add_application_icons(opennow-qt)

if(WIN32)
    enable_language(RC)
    configure_file(packaging/OpenNOW.rc.in "${CMAKE_CURRENT_BINARY_DIR}/OpenNOW.rc" @ONLY)
    set_source_files_properties("${CMAKE_CURRENT_BINARY_DIR}/OpenNOW.rc" PROPERTIES
        OBJECT_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/packaging/icons/OpenNOW.ico")
    target_sources(opennow-qt PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/OpenNOW.rc")
elseif(APPLE)
    set_source_files_properties(packaging/icons/OpenNOW.icns PROPERTIES
        MACOSX_PACKAGE_LOCATION Resources)
    target_sources(opennow-qt PRIVATE packaging/icons/OpenNOW.icns)
    set_target_properties(opennow-qt PROPERTIES MACOSX_BUNDLE_ICON_FILE OpenNOW.icns)
endif()
