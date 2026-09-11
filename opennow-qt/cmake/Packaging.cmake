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

if(WIN32)
    include(packaging/WindowsReleaseBinaries.cmake)
elseif(NOT APPLE)
    install(PROGRAMS
        "$<TARGET_FILE_DIR:opennow-qt>/opennow-core${OPENNOW_CORE_SUFFIX}"
        "$<TARGET_FILE_DIR:opennow-qt>/opennow-acceptance-verify${OPENNOW_CORE_SUFFIX}"
        "$<TARGET_FILE_DIR:opennow-qt>/opennow-update-helper${OPENNOW_CORE_SUFFIX}"
        DESTINATION "${CMAKE_INSTALL_BINDIR}"
    )
    install(PROGRAMS "${OPENNOW_STREAMER_FFI_RUNTIME}"
        DESTINATION "${CMAKE_INSTALL_BINDIR}")
    install(PROGRAMS "${OPENNOW_STREAMER_BIN_ARTIFACT}"
        DESTINATION "${CMAKE_INSTALL_BINDIR}")
else()
    install(PROGRAMS
        "${OPENNOW_CORE_ARTIFACT_ROOT}/${OPENNOW_CORE_PROFILE}/opennow-core${OPENNOW_CORE_SUFFIX}"
        "${OPENNOW_CORE_ARTIFACT_ROOT}/${OPENNOW_CORE_PROFILE}/opennow-acceptance-verify${OPENNOW_CORE_SUFFIX}"
        "${OPENNOW_CORE_ARTIFACT_ROOT}/${OPENNOW_CORE_PROFILE}/opennow-update-helper${OPENNOW_CORE_SUFFIX}"
        DESTINATION "${OPENNOW_EXECUTABLE_NAME}.app/Contents/MacOS")
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

if(APPLE)
    option(OPENNOW_MACOS_ADHOC_SIGN "Ad-hoc seal deployed macOS bundles without a signing identity" OFF)
    set(OPENNOW_QT_DEPLOY_TOOL_ARGS DEPLOY_TOOL_OPTIONS
        "-executable=${OPENNOW_EXECUTABLE_NAME}.app/Contents/MacOS/opennow-core"
        "-executable=${OPENNOW_EXECUTABLE_NAME}.app/Contents/MacOS/opennow-acceptance-verify"
        "-executable=${OPENNOW_EXECUTABLE_NAME}.app/Contents/MacOS/opennow-update-helper"
        "-executable=${OPENNOW_EXECUTABLE_NAME}.app/Contents/MacOS/opennow-streamer")
    if(OPENNOW_MACOS_ADHOC_SIGN)
        list(APPEND OPENNOW_QT_DEPLOY_TOOL_ARGS "-codesign=-")
    endif()
endif()

if(WIN32 OR APPLE)
    qt_generate_deploy_qml_app_script(
        TARGET opennow-qt
        OUTPUT_SCRIPT opennow_deploy_script
        NO_UNSUPPORTED_PLATFORM_ERROR
        ${OPENNOW_QT_DEPLOY_TOOL_ARGS}
    )
    install(SCRIPT "${opennow_deploy_script}")
    if(APPLE AND OPENNOW_MACOS_ADHOC_SIGN)
        get_target_property(OPENNOW_MACOS_BUNDLE_IDENTIFIER opennow-qt MACOSX_BUNDLE_GUI_IDENTIFIER)
        if(NOT OPENNOW_MACOS_BUNDLE_IDENTIFIER)
            message(FATAL_ERROR "Ad-hoc bundle signing requires a stable macOS bundle identifier")
        endif()
        configure_file("${CMAKE_CURRENT_LIST_DIR}/../packaging/macos-adhoc-seal.cmake.in"
            "${CMAKE_CURRENT_BINARY_DIR}/macos-adhoc-seal.cmake" @ONLY)
        install(SCRIPT "${CMAKE_CURRENT_BINARY_DIR}/macos-adhoc-seal.cmake")
    endif()
endif()

set(CPACK_PACKAGE_NAME "OpenNOW")
set(CPACK_PACKAGE_VENDOR "OpenCloudGaming")
set(CPACK_PACKAGE_CONTACT "OpenCloudGaming <support@opennow.app>")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "Controller-first GeForce NOW client")
set(CPACK_PACKAGE_HOMEPAGE_URL "https://github.com/OpenCloudGaming/OpenNOW")
set(CPACK_PACKAGE_VERSION "${OPENNOW_BUILD_VERSION}")
string(REPLACE "." ";" OPENNOW_BUILD_VERSION_PARTS "${OPENNOW_NUMERIC_VERSION}")
list(GET OPENNOW_BUILD_VERSION_PARTS 0 CPACK_PACKAGE_VERSION_MAJOR)
list(GET OPENNOW_BUILD_VERSION_PARTS 1 CPACK_PACKAGE_VERSION_MINOR)
list(GET OPENNOW_BUILD_VERSION_PARTS 2 CPACK_PACKAGE_VERSION_PATCH)
set(CPACK_PACKAGE_FILE_NAME "${OPENNOW_PACKAGE_FILE_NAME}")
set(CPACK_STRIP_FILES TRUE)
if(WIN32)
    include("${CMAKE_CURRENT_LIST_DIR}/WindowsInstaller.cmake")
elseif(APPLE)
    set(CPACK_GENERATOR "DragNDrop;ZIP")
    set(CPACK_DMG_VOLUME_NAME "OpenNOW")
else()
    # Linux packages intentionally use the distribution Qt runtime. AppImage
    # assembly uses linuxdeploy in release CI so plugin selection is explicit.
    set(CPACK_GENERATOR "DEB")
    set(CPACK_DEBIAN_PACKAGE_SECTION "games")
    set(CPACK_DEBIAN_FILE_NAME "${OPENNOW_PACKAGE_FILE_NAME}.deb")
    set(CPACK_DEBIAN_PACKAGE_VERSION "${OPENNOW_DEBIAN_VERSION}")
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
        "libqt6core6 (>= 6.8) | libqt6core6t64 (>= 6.8), libqt6gui6 (>= 6.8), libqt6network6 (>= 6.8), libqt6qml6 (>= 6.8), libqt6quick6 (>= 6.8), libqt6quickcontrols2-6 (>= 6.8), libqt6multimedia6 (>= 6.8), qt6-svg-plugins (>= 6.8), qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-dialogs, qml6-module-qtquick-effects, qml6-module-qtmultimedia, libsdl3-0 | libsdl3-0.0, libva2, libva-drm2")
    set(CPACK_DEBIAN_PACKAGE_ARCHITECTURE "${OPENNOW_DEBIAN_ARCH}")
endif()
include(CPack)
