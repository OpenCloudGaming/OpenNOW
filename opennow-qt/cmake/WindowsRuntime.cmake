if(WIN32)
    target_link_libraries(opennow-qt PRIVATE user32 dxgi)
    add_custom_command(TARGET opennow-qt POST_BUILD
        COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "$<TARGET_FILE:${OPENNOW_SDL3_RUNTIME_TARGET}>"
                "$<TARGET_FILE_DIR:opennow-qt>"
        COMMENT "Copying SDL3 next to the OpenNOW executable")
    get_filename_component(OPENNOW_QT_TARGET_BIN "${Qt6_DIR}/../../../bin" ABSOLUTE)
    set(OPENNOW_WINDEPLOYQT_ARGS
        --no-translations
        --compiler-runtime
        --qmldir "${CMAKE_CURRENT_SOURCE_DIR}/qml")
    # A narrow test build may never link OpenNOW, so its POST_BUILD deployment
    # cannot be the owner of the shared compiler runtime.
    if(MINGW)
        get_filename_component(OPENNOW_COMPILER_BIN "${CMAKE_CXX_COMPILER}" DIRECTORY)
        set(OPENNOW_MINGW_RUNTIME_FILES)
        foreach(runtime_name libgcc_s_seh-1.dll libstdc++-6.dll libwinpthread-1.dll)
            find_file(OPENNOW_RUNTIME_${runtime_name} NAMES "${runtime_name}"
                PATHS "${OPENNOW_COMPILER_BIN}" "${OPENNOW_QT_TARGET_BIN}"
                NO_DEFAULT_PATH REQUIRED)
            list(APPEND OPENNOW_MINGW_RUNTIME_FILES "${OPENNOW_RUNTIME_${runtime_name}}")
        endforeach()
        add_custom_target(opennow-compiler-runtime ALL
            COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:opennow-qt>"
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                ${OPENNOW_MINGW_RUNTIME_FILES} "$<TARGET_FILE_DIR:opennow-qt>"
            COMMENT "Deploying the MinGW runtime DLLs"
            COMMAND_EXPAND_LISTS)
        add_dependencies(opennow-qt opennow-compiler-runtime)
    endif()
    if(QT_HOST_PATH)
        find_program(WINDEPLOYQT_EXECUTABLE NAMES windeployqt
            HINTS "${QT_HOST_PATH}/bin"
            NO_DEFAULT_PATH
            REQUIRED)
        find_file(OPENNOW_TARGET_QTPATHS_EXECUTABLE
            NAMES qtpaths6.bat qtpaths.bat
            HINTS "${OPENNOW_QT_TARGET_BIN}"
            NO_DEFAULT_PATH
            REQUIRED)
        list(PREPEND OPENNOW_WINDEPLOYQT_ARGS
            --qtpaths "${OPENNOW_TARGET_QTPATHS_EXECUTABLE}")
        set(OPENNOW_QT_DEPLOY_TOOL_ARGS
            DEPLOY_TOOL_OPTIONS
            --qtpaths "${OPENNOW_TARGET_QTPATHS_EXECUTABLE}")
    else()
        find_program(WINDEPLOYQT_EXECUTABLE NAMES windeployqt
            HINTS "${OPENNOW_QT_TARGET_BIN}"
            NO_DEFAULT_PATH
            REQUIRED)
    endif()
    add_custom_command(TARGET opennow-qt POST_BUILD
        COMMAND "${WINDEPLOYQT_EXECUTABLE}" ${OPENNOW_WINDEPLOYQT_ARGS}
                "$<TARGET_FILE:opennow-qt>"
        COMMAND "${CMAKE_COMMAND}" -E rm -f
                "$<TARGET_FILE_DIR:opennow-qt>/tls/qopensslbackend.dll"
                "$<TARGET_FILE_DIR:opennow-qt>/plugins/tls/qopensslbackend.dll"
        COMMENT "Deploying Qt runtime plugins and preferring Windows Schannel TLS"
        COMMAND_EXPAND_LISTS)
    # windeployqt intentionally deploys only the production `windows` QPA
    # plugin. CTest runs the GUI/QML smoke suite with `QT_QPA_PLATFORM=offscreen`,
    # so keep that test-only plugin beside the deployed application as well.
    if(TARGET Qt6::QOffscreenIntegrationPlugin)
        add_custom_command(TARGET opennow-qt POST_BUILD
            COMMAND "${CMAKE_COMMAND}" -E make_directory
                    "$<TARGET_FILE_DIR:opennow-qt>/platforms"
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                    "$<TARGET_FILE:Qt6::QOffscreenIntegrationPlugin>"
                    "$<TARGET_FILE_DIR:opennow-qt>/platforms"
            COMMENT "Deploying the Qt offscreen platform plugin for CTest")
    endif()
endif()
