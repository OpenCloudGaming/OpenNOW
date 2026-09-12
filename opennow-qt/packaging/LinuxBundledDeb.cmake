if(NOT DEFINED OPENNOW_APPDIR OR NOT IS_ABSOLUTE "${OPENNOW_APPDIR}")
    message(FATAL_ERROR "OPENNOW_APPDIR must be an absolute path")
endif()
if(NOT CPACK_DEBIAN_PACKAGE_ARCHITECTURE MATCHES "^(amd64|arm64)$")
    message(FATAL_ERROR "Bundled DEBs require an amd64 or arm64 Linux build")
endif()

foreach(required IN ITEMS
    AppRun AppRun.wrapped apprun-hooks/linuxdeploy-plugin-qt-hook.sh
    usr/bin/opennow-qt usr/bin/opennow-core usr/bin/opennow-update-helper
    usr/bin/opennow-acceptance-verify usr/bin/opennow-streamer
    usr/bin/libopennow_streamer_ffi.so usr/bin/qt.conf
    usr/lib/libQt6Core.so.6 usr/lib/libSDL3.so.0
    usr/lib/libva.so.2 usr/lib/libva-drm.so.2
    usr/plugins/imageformats/libqsvg.so
    usr/plugins/platforms/libqxcb.so usr/plugins/platforms/libqoffscreen.so
    usr/plugins/platforms/libqwayland-egl.so usr/plugins/platforms/libqwayland-generic.so
    usr/plugins/wayland-shell-integration/libxdg-shell.so
    usr/qml/QtQuick/qmldir usr/qml/QtQuick/Controls/qmldir)
    if(NOT EXISTS "${OPENNOW_APPDIR}/${required}")
        message(FATAL_ERROR "The deployed Linux runtime is missing ${required}")
    endif()
endforeach()

set(integration "${OPENNOW_APPDIR}/../deb-integration")
file(REMOVE_RECURSE "${integration}")
file(MAKE_DIRECTORY "${integration}/usr/bin")
file(WRITE "${integration}/usr/bin/opennow-qt"
    "#!/bin/sh\nexec /opt/opennow/AppRun \"$@\"\n")
file(CHMOD "${integration}/usr/bin/opennow-qt"
    PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE)
foreach(directory IN ITEMS applications metainfo icons)
    file(COPY "${OPENNOW_APPDIR}/usr/share/${directory}" DESTINATION "${integration}/usr/share")
endforeach()
file(COPY "${OPENNOW_APPDIR}/usr/share/doc/opennow" DESTINATION "${integration}/usr/share/doc")

set(CPACK_GENERATOR DEB)
set(CPACK_INSTALL_CMAKE_PROJECTS "")
set(CPACK_INSTALLED_DIRECTORIES "${OPENNOW_APPDIR};/opt/opennow;${integration};/")
set(CPACK_PACKAGING_INSTALL_PREFIX "/")
set(CPACK_SET_DESTDIR OFF)
set(CPACK_STRIP_FILES OFF)
set(CPACK_DEBIAN_PACKAGE_DEPENDS "ca-certificates, libssl3t64, libvulkan1, libasound2t64, libudev1")
set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS TRUE)
set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS_PRIVATE_DIRS
    "${OPENNOW_APPDIR}/usr/lib;${OPENNOW_APPDIR}/usr/bin")
find_program(OPENNOW_DPKG_SHLIBDEPS dpkg-shlibdeps REQUIRED)
