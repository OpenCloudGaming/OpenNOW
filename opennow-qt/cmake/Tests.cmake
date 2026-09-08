include(CTest)
if(BUILD_TESTING)
    qt_add_executable(opennow-hdrcolor-tests tests/tst_hdrcolor.cpp
        src/streaming/rendering/HdrChromeEffect.cpp src/streaming/rendering/HdrOutput.cpp)
    target_include_directories(opennow-hdrcolor-tests PRIVATE src)
    target_link_libraries(opennow-hdrcolor-tests PRIVATE Qt6::Test Qt6::GuiPrivate Qt6::Quick Qt6::QuickPrivate)
    if(MSVC)
        target_compile_options(opennow-hdrcolor-tests PRIVATE /Zi)
        target_link_options(opennow-hdrcolor-tests PRIVATE /DEBUG)
    endif()
    qt_add_shaders(opennow-hdrcolor-tests "opennow-hdrcolor-test-shaders"
        PREFIX "/hdr-test" BASE "tests" FILES tests/hdrcolor_test.vert tests/hdrcolor_test.frag)
    qt_add_shaders(opennow-hdrcolor-tests "opennow-hdroutput-test-shaders"
        PREFIX "/opennow/shaders" BASE "shaders" FILES shaders/framegen.vert shaders/hdroutput.frag)
    qt_add_shaders(opennow-hdrcolor-tests "opennow-hdrchrome-test-shaders"
        BATCHABLE PREFIX "/opennow/shaders" BASE "shaders" FILES ${OPENNOW_CHROME_SHADERS})
    find_package(Qt6 6.8 REQUIRED COMPONENTS QuickTest)
    qt_add_executable(opennow-theme-tests tests/tst_theme.cpp)
    target_link_libraries(opennow-theme-tests PRIVATE Qt6::QuickTest Qt6::Quick)
    target_compile_definitions(opennow-theme-tests PRIVATE
        OPENNOW_QML_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/qml")
    add_test(NAME opennow-theme-tests COMMAND opennow-theme-tests
        -input "${CMAKE_CURRENT_SOURCE_DIR}/tests/theme")
    set_tests_properties(opennow-theme-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 30)
    qt_add_resources(opennow-qt "theme-settings-acceptance"
        PREFIX "/acceptance" BASE tests FILES tests/ThemeSettingsAcceptance.qml)
    foreach(width 960 1440)
        foreach(mode dark light)
            set(theme_mode_args)
            if(mode STREQUAL "light")
                list(APPEND theme_mode_args --smoke-light-theme)
            endif()
            add_test(NAME "qml-theme-settings-${width}-${mode}"
                COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                    --route settings-themes --smoke-theme-settings --reduced-motion
                    --smoke-width ${width} ${theme_mode_args})
            set_tests_properties("qml-theme-settings-${width}-${mode}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 15)
        endforeach()
    endforeach()
    qt_add_executable(opennow-framepacer-tests tests/tst_framepacer.cpp)
    target_include_directories(opennow-framepacer-tests PRIVATE src)
    target_link_libraries(opennow-framepacer-tests PRIVATE Qt6::Test)
    add_test(NAME opennow-framepacer-tests COMMAND opennow-framepacer-tests -o -,txt)
    qt_add_executable(opennow-frameinterpolator-tests
        tests/tst_frameinterpolator.cpp
        src/streaming/rendering/StreamFrameInterpolator.cpp)
    target_include_directories(opennow-frameinterpolator-tests PRIVATE src)
    target_link_libraries(opennow-frameinterpolator-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui Qt6::GuiPrivate)
    qt_add_shaders(opennow-frameinterpolator-tests "opennow-framegen-test-shaders"
        PREFIX "/opennow/shaders" BASE "shaders" FILES ${OPENNOW_STREAM_SHADERS})
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
        find_program(OPENNOW_XVFB_RUN xvfb-run)
    endif()
    if(OPENNOW_XVFB_RUN)
        add_test(NAME opennow-hdrcolor-tests
            COMMAND "${OPENNOW_XVFB_RUN}" -a "$<TARGET_FILE:opennow-hdrcolor-tests>" -o -,txt)
        set_tests_properties(opennow-hdrcolor-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=xcb")
        add_test(NAME opennow-frameinterpolator-tests
            COMMAND "${OPENNOW_XVFB_RUN}" -a "$<TARGET_FILE:opennow-frameinterpolator-tests>" -o -,txt)
        set_tests_properties(opennow-frameinterpolator-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=xcb")
    else()
        add_test(NAME opennow-hdrcolor-tests COMMAND opennow-hdrcolor-tests -o -,txt)
        add_test(NAME opennow-frameinterpolator-tests COMMAND opennow-frameinterpolator-tests -o -,txt)
        if(WIN32 OR CMAKE_SYSTEM_NAME STREQUAL "Linux")
            set_tests_properties(opennow-frameinterpolator-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
        endif()
    endif()
    if(WIN32)
        set_tests_properties(opennow-hdrcolor-tests PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=windows;QT_FORCE_STDERR_LOGGING=1")
    endif()
    set_tests_properties(opennow-hdrcolor-tests PROPERTIES TIMEOUT 60)
    set_tests_properties(opennow-frameinterpolator-tests PROPERTIES TIMEOUT 60)
    qt_add_executable(opennow-streamcolor-tests tests/tst_streamcolor.cpp)
    target_include_directories(opennow-streamcolor-tests PRIVATE src)
    target_link_libraries(opennow-streamcolor-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui Qt6::GuiPrivate)
    qt_add_shaders(opennow-streamcolor-tests "opennow-streamcolor-test-shaders"
        PREFIX "/opennow/shaders" BASE "shaders"
        FILES shaders/streamvideo.vert shaders/streamvideo.frag)
    if(OPENNOW_XVFB_RUN)
        add_test(NAME opennow-streamcolor-tests
            COMMAND "${OPENNOW_XVFB_RUN}" -a "$<TARGET_FILE:opennow-streamcolor-tests>" -o -,txt)
        set_tests_properties(opennow-streamcolor-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=xcb")
    else()
        add_test(NAME opennow-streamcolor-tests COMMAND opennow-streamcolor-tests -o -,txt)
        if(WIN32 OR CMAKE_SYSTEM_NAME STREQUAL "Linux")
            set_tests_properties(opennow-streamcolor-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
        endif()
    endif()
    set_tests_properties(opennow-streamcolor-tests PROPERTIES TIMEOUT 60)
    qt_add_resources(opennow-qt "region-ping-acceptance"
        PREFIX "/acceptance" BASE tests FILES tests/RegionPingAcceptance.qml tests/RegionChoicesAcceptance.qml tests/StorePagingAcceptance.qml tests/BackendAvailabilityAcceptance.qml tests/StreamRecoveryAcceptance.qml tests/IdleModeAcceptance.qml tests/FrameGenerationAcceptance.qml tests/AudioOutputAcceptance.qml tests/CollectionsAcceptance.qml tests/SteamBigPictureAcceptance.qml tests/ControllerMetadataAcceptance.qml tests/MicrophoneAcceptance.qml tests/RecordingAcceptance.qml)
    add_test(NAME qml-recording
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings --smoke-recording --reduced-motion)
    set_tests_properties(qml-recording PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    add_test(NAME qml-microphone
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-audio --smoke-microphone --reduced-motion)
    set_tests_properties(qml-microphone PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    foreach(surface desktop console)
        set(microphone_settings_route settings-audio)
        set(microphone_overlay desktop-stream-menu)
        if(surface STREQUAL "console")
            set(microphone_settings_route settings-input)
            set(microphone_overlay guide-session)
        endif()
        add_test(NAME qml-microphone-settings-${surface}
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --${surface}
                --route ${microphone_settings_route} --smoke-microphone-supported --reduced-motion)
        add_test(NAME qml-microphone-muted-${surface}
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --${surface}
                --route stream --overlay ${microphone_overlay} --smoke-microphone-muted --reduced-motion)
        set_tests_properties(qml-microphone-settings-${surface} qml-microphone-muted-${surface}
            PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    endforeach()
    add_test(NAME qml-audio-output
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-audio --smoke-audio-output --reduced-motion)
    set_tests_properties(qml-audio-output PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    foreach(width 960 1440)
        add_test(NAME "qml-collections-${width}"
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                --route library --smoke-collections --smoke-width ${width} --reduced-motion)
        set_tests_properties("qml-collections-${width}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 15)
        add_test(NAME "qml-collections-light-${width}"
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                --route library --smoke-collections --smoke-light-theme --smoke-width ${width} --reduced-motion)
        set_tests_properties("qml-collections-light-${width}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 15)
    endforeach()
    add_test(NAME qml-controller-metadata
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-input --smoke-controller-metadata --reduced-motion)
    set_tests_properties(qml-controller-metadata PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    qt_add_resources(opennow-qt "custom-background-acceptance"
        PREFIX "/acceptance" BASE tests FILES tests/CustomBackgroundAcceptance.qml)
    foreach(width 960 1600)
        add_test(NAME qml-custom-background-${width}
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                --route settings-themes --smoke-custom-background --smoke-width ${width} --reduced-motion)
        set_tests_properties(qml-custom-background-${width} PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    endforeach()
    add_test(NAME qml-steam-big-picture
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-streaming --smoke-steam-big-picture --reduced-motion)
    set_tests_properties(qml-steam-big-picture PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    add_test(NAME qml-frame-generation
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-streaming --smoke-frame-generation --reduced-motion)
    set_tests_properties(qml-frame-generation PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    foreach(surface desktop console)
        add_test(NAME qml-frame-generation-stats-${surface}
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --${surface}
                --route stream --smoke-frame-generation-stats --reduced-motion)
        set_tests_properties(qml-frame-generation-stats-${surface} PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 15)
    endforeach()
    add_test(NAME qml-idle-mode
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-streaming --smoke-idle-mode --reduced-motion)
    set_tests_properties(qml-idle-mode PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    add_test(NAME qml-stream-recovery
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-streaming --smoke-stream-recovery --reduced-motion)
    set_tests_properties(qml-stream-recovery PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    add_test(NAME qml-backend-availability
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-streaming --smoke-backend-availability --reduced-motion)
    set_tests_properties(qml-backend-availability PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    add_test(NAME qml-store-paging
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route store --smoke-store-paging --reduced-motion)
    set_tests_properties(qml-store-paging PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    foreach(width 960 1440)
        foreach(motion normal reduced)
            set(store_motion_args)
            if(motion STREQUAL "reduced")
                list(APPEND store_motion_args --reduced-motion)
            endif()
            add_test(NAME "qml-store-navigation-${width}-${motion}"
                COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                    --route store --smoke-store-paging --smoke-store-navigation
                    --smoke-width ${width} --smoke-height 900 ${store_motion_args})
            set_tests_properties("qml-store-navigation-${width}-${motion}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
        endforeach()
    endforeach()
    foreach(width 960 1440)
        add_test(NAME "qml-region-ping-${width}"
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                --route settings-network --smoke-region-ping --smoke-width ${width} --reduced-motion)
        set_tests_properties("qml-region-ping-${width}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
    endforeach()
    set(OPENNOW_QT_SMOKE_TIMEOUT 5)
    if(APPLE AND "x86_64" IN_LIST CMAKE_OSX_ARCHITECTURES)
        set(OPENNOW_QT_SMOKE_TIMEOUT 15)
    endif()

    add_executable(opennow-fake-core tests/fake_core.cpp)

    qt_add_executable(opennow-localization-tests
        tests/tst_localization.cpp
        src/localization/Localization.cpp
        src/localization/Localization.h
    )
    target_include_directories(opennow-localization-tests PRIVATE src)
    target_link_libraries(opennow-localization-tests PRIVATE Qt6::Test Qt6::Core)
    target_compile_definitions(opennow-localization-tests PRIVATE
        OPENNOW_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/..")
    qt_add_resources(opennow-localization-tests "opennow-test-locales"
        PREFIX "/locales"
        FILES ${OPENNOW_LOCALE_FILES}
    )
    add_test(NAME opennow-localization-tests
             COMMAND opennow-localization-tests -o -,txt)

    qt_add_executable(opennow-qt-tests
        tests/tst_appcontroller.cpp
        src/app/AppController.cpp
        src/app/AppController.h
    )
    target_include_directories(opennow-qt-tests PRIVATE src)
    target_link_libraries(opennow-qt-tests PRIVATE Qt6::Test Qt6::Core Qt6::Gui)
    add_test(NAME opennow-qt-tests COMMAND opennow-qt-tests -o -,txt)
    set_tests_properties(opennow-qt-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

    qt_add_executable(opennow-coreclient-tests
        tests/tst_coreclient.cpp
        src/core/CoreClient.cpp
        src/core/CoreClient.h
    )
    target_include_directories(opennow-coreclient-tests PRIVATE src)
    target_link_libraries(opennow-coreclient-tests PRIVATE Qt6::Test Qt6::Core)
    target_compile_definitions(opennow-coreclient-tests PRIVATE OPENNOW_VERSION="${OPENNOW_BUILD_VERSION}")
    add_dependencies(opennow-coreclient-tests opennow-fake-core)
    add_test(NAME opennow-coreclient-tests COMMAND opennow-coreclient-tests -o -,txt)

    qt_add_executable(opennow-streamvideo-tests
        tests/tst_streamvideoitem.cpp
        ${OPENNOW_STREAM_RUNTIME_SOURCES}
        ${OPENNOW_STREAM_PRESENTATION_SOURCES}
    )
    target_include_directories(opennow-streamvideo-tests PRIVATE src)
    qt_add_shaders(opennow-streamvideo-tests "opennow-stream-test-shaders"
        PREFIX "/opennow/shaders"
        BASE "shaders"
        FILES ${OPENNOW_STREAM_SHADERS}
    )
    target_link_libraries(opennow-streamvideo-tests PRIVATE
        opennow-platform-input
        Qt6::Test Qt6::Core Qt6::Gui Qt6::GuiPrivate Qt6::Qml Qt6::Quick Qt6::QuickPrivate
        opennow-streamer-ffi)
    if(WIN32)
        target_link_libraries(opennow-streamvideo-tests PRIVATE user32)
    endif()
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
        target_link_libraries(opennow-streamvideo-tests PRIVATE Vulkan::Vulkan)
    endif()
    add_dependencies(opennow-streamvideo-tests opennow-streamer-ffi-build)
    if(WIN32 OR CMAKE_SYSTEM_NAME STREQUAL "Linux")
        qt_add_executable(opennow-nativeframegeneration-tests
            tests/tst_nativeframegeneration.cpp
            ${OPENNOW_STREAM_RUNTIME_SOURCES}
            src/streaming/rendering/NativeStreamRenderCallback.cpp
            src/streaming/rendering/HdrOutput.cpp
            src/streaming/rendering/StreamFrameInterpolator.cpp
            src/streaming/rendering/LinuxVulkanGraphics.cpp)
        target_include_directories(opennow-nativeframegeneration-tests PRIVATE src)
        target_link_libraries(opennow-nativeframegeneration-tests PRIVATE
            Qt6::Test Qt6::GuiPrivate Qt6::Quick Qt6::QuickPrivate opennow-streamer-ffi)
        if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
            target_link_libraries(opennow-nativeframegeneration-tests PRIVATE Vulkan::Vulkan)
        endif()
        add_dependencies(opennow-nativeframegeneration-tests opennow-streamer-ffi-build)
        qt_add_shaders(opennow-nativeframegeneration-tests "opennow-native-framegen-test-shaders"
            PREFIX "/opennow/shaders" BASE "shaders" FILES ${OPENNOW_STREAM_SHADERS})
        if(OPENNOW_XVFB_RUN)
            add_test(NAME opennow-nativeframegeneration-tests
                COMMAND "${OPENNOW_XVFB_RUN}" -a "$<TARGET_FILE:opennow-nativeframegeneration-tests>" -o -,txt)
            set_tests_properties(opennow-nativeframegeneration-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=xcb")
        else()
            add_test(NAME opennow-nativeframegeneration-tests COMMAND opennow-nativeframegeneration-tests -o -,txt)
            set_tests_properties(opennow-nativeframegeneration-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
        endif()
        set_tests_properties(opennow-nativeframegeneration-tests PROPERTIES TIMEOUT 60)
    endif()
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
        qt_add_executable(opennow-linuxvulkangraphics-tests
            tests/tst_linuxvulkangraphics.cpp
            src/streaming/rendering/LinuxVulkanGraphics.cpp
            src/streaming/rendering/LinuxVulkanGraphics.h)
        target_include_directories(opennow-linuxvulkangraphics-tests PRIVATE src)
        target_link_libraries(opennow-linuxvulkangraphics-tests PRIVATE
            Qt6::Test Qt6::GuiPrivate Qt6::Quick opennow-streamer-ffi Vulkan::Vulkan)
        add_dependencies(opennow-linuxvulkangraphics-tests opennow-streamer-ffi-build)
        add_test(NAME opennow-linuxvulkangraphics-tests
            COMMAND opennow-linuxvulkangraphics-tests -o -,txt)
        set_tests_properties(opennow-linuxvulkangraphics-tests PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 30)
    endif()
    add_test(NAME opennow-streamvideo-tests
             COMMAND opennow-streamvideo-tests -o -,txt)
    set_tests_properties(opennow-streamvideo-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

    qt_add_executable(opennow-waylandpointer-tests tests/tst_waylandpointercapture.cpp)
    target_link_libraries(opennow-waylandpointer-tests PRIVATE opennow-platform-input Qt6::Test)
    add_test(NAME opennow-waylandpointer-tests COMMAND opennow-waylandpointer-tests -o -,txt)
    set_tests_properties(opennow-waylandpointer-tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

    qt_add_executable(opennow-nativestreamruntime-tests
        tests/tst_nativestreamruntime.cpp
        ${OPENNOW_STREAM_RUNTIME_SOURCES}
    )
    target_include_directories(opennow-nativestreamruntime-tests PRIVATE src)
    target_link_libraries(opennow-nativestreamruntime-tests PRIVATE
        Qt6::Test Qt6::Core opennow-streamer-ffi)
    add_dependencies(opennow-nativestreamruntime-tests opennow-streamer-ffi-build)
    add_test(NAME opennow-nativestreamruntime-tests
             COMMAND opennow-nativestreamruntime-tests -o -,txt)
    set_tests_properties(opennow-nativestreamruntime-tests PROPERTIES TIMEOUT 8)

    if(APPLE OR WIN32)
        add_custom_target(opennow-streamer-ffi-test-runtime ALL
            COMMAND ${CMAKE_COMMAND} -E make_directory
                "$<TARGET_FILE_DIR:opennow-streamvideo-tests>"
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${OPENNOW_STREAMER_FFI_RUNTIME}"
                "$<TARGET_FILE_DIR:opennow-streamvideo-tests>/"
            COMMAND ${CMAKE_COMMAND} -E make_directory
                "$<TARGET_FILE_DIR:opennow-nativestreamruntime-tests>"
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${OPENNOW_STREAMER_FFI_RUNTIME}"
                "$<TARGET_FILE_DIR:opennow-nativestreamruntime-tests>/"
            DEPENDS opennow-streamer-ffi-build
            COMMENT "Deploying the embedded streamer runtime for CTest")
        foreach(test_target IN ITEMS
                opennow-streamvideo-tests
                opennow-nativestreamruntime-tests)
            add_dependencies(${test_target} opennow-streamer-ffi-test-runtime)
            set_property(TARGET ${test_target} APPEND PROPERTY BUILD_RPATH "@loader_path")
        endforeach()
    endif()

    qt_add_executable(opennow-embedded-orchestration-tests
        tests/tst_embeddedorchestration.cpp
    )
    target_link_libraries(opennow-embedded-orchestration-tests PRIVATE Qt6::Test Qt6::Core Qt6::Qml)
    target_compile_definitions(opennow-embedded-orchestration-tests PRIVATE
        OPENNOW_QT_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME opennow-embedded-orchestration-tests
             COMMAND opennow-embedded-orchestration-tests -o -,txt)

    qt_add_executable(opennow-singleinstance-tests
        tests/tst_singleinstance.cpp
        src/app/SingleInstance.cpp
        src/app/SingleInstance.h
    )
    target_include_directories(opennow-singleinstance-tests PRIVATE src)
    target_link_libraries(opennow-singleinstance-tests PRIVATE Qt6::Test Qt6::Core Qt6::Network)
    add_test(NAME opennow-singleinstance-tests
             COMMAND opennow-singleinstance-tests -o -,txt)

    qt_add_executable(opennow-thumbnail-tests
        tests/tst_thumbnailgenerator.cpp
        src/media/ThumbnailGenerator.cpp
        src/media/ThumbnailGenerator.h
    )
    target_include_directories(opennow-thumbnail-tests PRIVATE src)
    target_link_libraries(opennow-thumbnail-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui Qt6::Multimedia)
    add_test(NAME opennow-thumbnail-tests COMMAND opennow-thumbnail-tests -o -,txt)
    set_tests_properties(opennow-thumbnail-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 8
    )
    qt_add_executable(opennow-controllerinput-tests
        tests/tst_controllerinput.cpp
        src/input/ControllerInput.cpp
        src/input/ControllerInput.h
    )
    target_include_directories(opennow-controllerinput-tests PRIVATE src)
    target_link_libraries(opennow-controllerinput-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui SDL3::SDL3)
    add_test(NAME opennow-controllerinput-tests
             COMMAND opennow-controllerinput-tests -o -,txt)
    set_tests_properties(opennow-controllerinput-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 8
    )
    qt_add_executable(opennow-controllersources-tests
        tests/tst_controllersources.cpp
        src/input/ControllerInput.cpp
        src/input/ControllerInput.h
    )
    target_include_directories(opennow-controllersources-tests PRIVATE src)
    target_link_libraries(opennow-controllersources-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui SDL3::SDL3)
    add_test(NAME opennow-controllersources-tests
             COMMAND opennow-controllersources-tests -o -,txt)
    set_tests_properties(opennow-controllersources-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 20
    )
    qt_add_executable(opennow-controllermetadata-tests
        tests/tst_controllermetadata.cpp
        src/input/ControllerInput.cpp
        src/input/ControllerInput.h
    )
    target_include_directories(opennow-controllermetadata-tests PRIVATE src)
    target_link_libraries(opennow-controllermetadata-tests PRIVATE
        Qt6::Test Qt6::Core Qt6::Gui SDL3::SDL3)
    add_test(NAME opennow-controllermetadata-tests
             COMMAND opennow-controllermetadata-tests -o -,txt)
    set_tests_properties(opennow-controllermetadata-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 30
    )
    if(WIN32)
        add_dependencies(opennow-nativeframegeneration-tests opennow-streamer-ffi-test-runtime)
        # Qt's executable helper defaults to the GUI subsystem on Windows. Keep
        # test runners as console programs so CTest captures QtTest failures.
        set_target_properties(
            opennow-localization-tests
            opennow-qt-tests
            opennow-coreclient-tests
            opennow-streamvideo-tests
            opennow-waylandpointer-tests
            opennow-nativestreamruntime-tests
            opennow-nativeframegeneration-tests
            opennow-embedded-orchestration-tests
            opennow-singleinstance-tests
            opennow-thumbnail-tests
            opennow-controllerinput-tests
            opennow-controllersources-tests
            opennow-controllermetadata-tests
            PROPERTIES WIN32_EXECUTABLE FALSE)

        # windeployqt follows the application graph and therefore does not copy
        # Qt6Test.dll. All unit-test runners share the application output
        # directory, so deploy the test runtime explicitly and make the runners
        # depend on it. This keeps a clean build directly runnable by CTest.
        add_custom_target(opennow-qt-test-runtime ALL
            COMMAND ${CMAKE_COMMAND} -E make_directory
                "$<TARGET_FILE_DIR:opennow-qt>"
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "$<TARGET_FILE:Qt6::Test>"
                "$<TARGET_FILE_DIR:opennow-qt>/"
            COMMAND "${WINDEPLOYQT_EXECUTABLE}" ${OPENNOW_WINDEPLOYQT_ARGS}
                --quick --multimedia --network --test
                --dir "$<TARGET_FILE_DIR:opennow-qt>"
                "$<TARGET_FILE:Qt6::Test>"
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "$<TARGET_FILE:${OPENNOW_SDL3_RUNTIME_TARGET}>"
                "$<TARGET_FILE_DIR:opennow-qt>"
            COMMAND "${CMAKE_COMMAND}" -E make_directory
                "$<TARGET_FILE_DIR:opennow-qt>/platforms"
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "$<TARGET_FILE:Qt6::QOffscreenIntegrationPlugin>"
                "$<TARGET_FILE_DIR:opennow-qt>/platforms"
            COMMENT "Deploying the Qt Test runtime for CTest")
        if(TARGET opennow-compiler-runtime)
            add_dependencies(opennow-qt-test-runtime opennow-compiler-runtime)
        endif()
        if(TARGET opennow-msvc-runtime)
            add_dependencies(opennow-qt-test-runtime opennow-msvc-runtime)
        endif()
        foreach(test_target IN ITEMS
                opennow-localization-tests
                opennow-qt-tests
                opennow-coreclient-tests
                opennow-streamvideo-tests
                opennow-waylandpointer-tests
                opennow-nativestreamruntime-tests
                opennow-nativeframegeneration-tests
                opennow-embedded-orchestration-tests
                opennow-singleinstance-tests
                opennow-thumbnail-tests
                opennow-controllerinput-tests
                opennow-controllersources-tests
                opennow-controllermetadata-tests)
            add_dependencies(${test_target} opennow-qt-test-runtime)
        endforeach()
    endif()

    foreach(route home library store theme-store controllers settings settings-account settings-streaming settings-video settings-video-dropdown settings-input settings-network settings-themes settings-advanced settings-advanced-dropdown game-detail game-detail-platform-dropdown sign-in joining inserting stream accounts profile-pin game-accounts persistent-storage media diagnostics updates feedback)
        add_test(NAME "qml-route-${route}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances
                         --console --route "${route}" --reduced-motion)
        set_tests_properties("qml-route-${route}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    foreach(motion_mode normal reduced)
        foreach(motion_window windowed fullscreen)
            set(launch_args --smoke-test --allow-multiple-instances --desktop --route library --smoke-session-launch)
            if(motion_mode STREQUAL "reduced")
                list(APPEND launch_args --reduced-motion)
            endif()
            if(motion_window STREQUAL "fullscreen")
                list(APPEND launch_args --smoke-motion-fullscreen)
            endif()
            add_test(NAME "qml-session-launch-${motion_mode}-${motion_window}" COMMAND opennow-qt ${launch_args})
            set_tests_properties("qml-session-launch-${motion_mode}-${motion_window}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 20)
            if(OPENNOW_XVFB_RUN)
                add_test(NAME "qml-session-launch-video-${motion_mode}-${motion_window}"
                    COMMAND "${OPENNOW_XVFB_RUN}" -a "$<TARGET_FILE:opennow-qt>"
                        ${launch_args} --smoke-session-launch-video)
                set_tests_properties("qml-session-launch-video-${motion_mode}-${motion_window}" PROPERTIES
                    ENVIRONMENT "QT_QPA_PLATFORM=xcb" TIMEOUT 30)
            endif()
            set(motion_args --smoke-test --allow-multiple-instances --desktop --route home --smoke-paper-design --smoke-motion)
            if(motion_mode STREQUAL "reduced")
                list(APPEND motion_args --reduced-motion)
            endif()
            if(motion_window STREQUAL "fullscreen")
                list(APPEND motion_args --smoke-motion-fullscreen)
            endif()
            add_test(NAME "qml-motion-${motion_mode}-${motion_window}" COMMAND opennow-qt ${motion_args})
            add_test(NAME "qml-sidebar-${motion_mode}-${motion_window}" COMMAND opennow-qt ${motion_args} --smoke-sidebar)
            add_test(NAME "qml-settings-motion-${motion_mode}-${motion_window}" COMMAND opennow-qt ${motion_args} --smoke-settings-motion)
            # This sequence deliberately exercises over four seconds of motion.
            math(EXPR settings_motion_timeout "${OPENNOW_QT_SMOKE_TIMEOUT} + 10")
            set_tests_properties("qml-settings-motion-${motion_mode}-${motion_window}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${settings_motion_timeout})
            set_tests_properties("qml-sidebar-${motion_mode}-${motion_window}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
            set_tests_properties("qml-motion-${motion_mode}-${motion_window}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
        endforeach()
    endforeach()

    foreach(route home library store friends updates settings settings-account settings-streaming settings-input settings-network settings-themes settings-advanced game-detail stream)
        add_test(NAME "qml-desktop-route-${route}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop --route "${route}" --reduced-motion)
        set_tests_properties("qml-desktop-route-${route}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    foreach(route settings-streaming settings-audio settings-console settings-input settings-themes settings-account settings-network settings-advanced)
        foreach(size desktop compact)
            if(size STREQUAL "desktop")
                set(renew_width 1440)
                set(renew_height 900)
            else()
                set(renew_width 960)
                set(renew_height 640)
            endif()
            add_test(NAME "qml-renew-${route}-${size}"
                COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                    --route "${route}" --smoke-paper-design --smoke-width ${renew_width}
                    --smoke-height ${renew_height} --reduced-motion)
            set_tests_properties("qml-renew-${route}-${size}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
        endforeach()
    endforeach()
    add_test(NAME qml-renew-resolution-picker
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-streaming --smoke-paper-design --smoke-width 1440
            --smoke-height 900 --smoke-resolution-open --reduced-motion)
    set_tests_properties(qml-renew-resolution-picker PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
    foreach(panel stats audio interface console shortcuts controllers subscription recording)
        foreach(size desktop compact)
            if(size STREQUAL "desktop")
                set(renew_width 1440)
                set(renew_height 900)
            else()
                set(renew_width 960)
                set(renew_height 640)
            endif()
            add_test(NAME "qml-renew-advanced-${panel}-${size}"
                COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                    --route settings --smoke-paper-design --smoke-settings-panel "${panel}"
                    --smoke-width ${renew_width} --smoke-height ${renew_height} --reduced-motion)
            set_tests_properties("qml-renew-advanced-${panel}-${size}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
        endforeach()
    endforeach()
    add_test(NAME qml-renew-network-picker
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-network --smoke-paper-design --smoke-choice-open --reduced-motion)
    add_test(NAME qml-renew-language-picker
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings-themes --smoke-paper-design --smoke-settings-panel interface --smoke-choice-open --reduced-motion)
    set_tests_properties(qml-renew-network-picker qml-renew-language-picker PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
    foreach(route settings-network settings-advanced settings-themes settings-input)
        add_test(NAME "qml-renew-actions-${route}"
            COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
                --route "${route}" --smoke-paper-design --smoke-renew-settings-actions --reduced-motion)
        set_tests_properties("qml-renew-actions-${route}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
    endforeach()
    add_test(NAME qml-renew-actions-stats
        COMMAND opennow-qt --smoke-test --allow-multiple-instances --desktop
            --route settings --smoke-paper-design --smoke-settings-panel stats --smoke-renew-settings-actions --reduced-motion)
    set_tests_properties(qml-renew-actions-stats PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
    foreach(overlay desktop-stream-menu desktop-stream-stats desktop-stream-stats-expanded desktop-stream-exit-confirm)
        add_test(NAME "qml-${overlay}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances
                         --desktop --route stream --overlay "${overlay}" --reduced-motion)
        set_tests_properties("qml-${overlay}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    foreach(surface desktop console)
        set(input_notice_mode --console)
        if(surface STREQUAL "desktop")
            set(input_notice_mode --desktop)
        endif()
        add_test(NAME "qml-${surface}-input-capture-error"
            COMMAND opennow-qt --smoke-test --allow-multiple-instances ${input_notice_mode}
                --route stream --smoke-input-capture-error --reduced-motion)
        set_tests_properties("qml-${surface}-input-capture-error" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
    endforeach()
    foreach(surface desktop console)
        foreach(mode windowed fullscreen)
            foreach(key return enter tab-space)
                add_test(NAME "qml-stream-exit-${surface}-${mode}-${key}"
                    COMMAND opennow-qt --smoke-test --allow-multiple-instances
                        --${surface} --route stream --smoke-stream-exit
                        --smoke-exit-${key} --smoke-exit-${mode} --reduced-motion)
                set_tests_properties("qml-stream-exit-${surface}-${mode}-${key}" PROPERTIES
                    ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
            endforeach()
        endforeach()
    endforeach()
    foreach(surface desktop console)
        foreach(mode windowed maximized)
            add_test(NAME "qml-session-fullscreen-${surface}-${mode}"
                COMMAND opennow-qt --smoke-test --allow-multiple-instances
                    --${surface} --route stream --smoke-session-fullscreen
                    --smoke-fullscreen-restore-${mode} --reduced-motion)
            set_tests_properties("qml-session-fullscreen-${surface}-${mode}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 10)
        endforeach()
    endforeach()
    add_test(NAME qml-fullscreen-stream-stats-shortcut
             COMMAND opennow-qt --smoke-test --allow-multiple-instances
                     --desktop --route stream --smoke-fullscreen-stats-shortcut --reduced-motion)
    set_tests_properties(qml-fullscreen-stream-stats-shortcut PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 5
    )
    add_test(NAME qml-fullscreen-configured-stats-shortcut
             COMMAND opennow-qt --smoke-test --allow-multiple-instances
                     --desktop --route stream --smoke-fullscreen-stats-shortcut
                     --smoke-configured-stats-shortcut --reduced-motion)
    set_tests_properties(qml-fullscreen-configured-stats-shortcut PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT 5)
    add_test(NAME qml-console-mode-persistence-rollback
             COMMAND opennow-qt
                     --smoke-test
                     --allow-multiple-instances
                     --core "$<TARGET_FILE:opennow-fake-core>"
                     --smoke-console-persistence-rollback --desktop
                     --reduced-motion)
    set_tests_properties(qml-console-mode-persistence-rollback PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
    )
    foreach(notes_route updates settings-advanced)
        foreach(notes_width 960 1600)
            add_test(NAME "qml-release-notes-${notes_route}-${notes_width}"
                COMMAND opennow-qt --smoke-test --allow-multiple-instances
                    --desktop --route ${notes_route} --smoke-release-notes
                    --smoke-width ${notes_width} --reduced-motion)
            set_tests_properties("qml-release-notes-${notes_route}-${notes_width}" PROPERTIES
                ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT})
        endforeach()
    endforeach()
    add_test(NAME qml-streamer-event-contract
             COMMAND opennow-qt
                     --smoke-test
                     --allow-multiple-instances
                     --core "$<TARGET_FILE:opennow-fake-core>"
                     --smoke-streamer-event
                     --reduced-motion)
    set_tests_properties(qml-streamer-event-contract PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
    )
    foreach(overlay friends friend-actions quick-settings session-conflict session-report queue-ad guide-session guide-controls guide-media guide-shortcuts)
        add_test(NAME "qml-overlay-${overlay}"
                 COMMAND opennow-qt --smoke-test --allow-multiple-instances --route home --overlay "${overlay}" --reduced-motion)
        set_tests_properties("qml-overlay-${overlay}" PROPERTIES
            ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
            TIMEOUT ${OPENNOW_QT_SMOKE_TIMEOUT}
        )
    endforeach()
    add_test(NAME opennow-performance-report-harness
             COMMAND opennow-qt
                     --allow-multiple-instances
                     --performance-report "${CMAKE_BINARY_DIR}/performance-harness.json"
                     --performance-width 960
                     --performance-height 540
                     --performance-cycles 1
                     --performance-refresh-hz 30
                     --performance-label ci-harness)
    set_tests_properties(opennow-performance-report-harness PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        RUN_SERIAL TRUE
        TIMEOUT 15
    )
    add_test(NAME opennow-performance-relative-path-rejected
             COMMAND opennow-qt
                     --allow-multiple-instances
                     --performance-report performance-report-must-be-absolute.json
                     --performance-width 960
                     --performance-height 540
                     --performance-cycles 1)
    set_tests_properties(opennow-performance-relative-path-rejected PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
        WILL_FAIL TRUE
        TIMEOUT 5
    )
    add_test(NAME opennow-acceptance-verifier-help
             COMMAND "$<TARGET_FILE_DIR:opennow-qt>/opennow-acceptance-verify${OPENNOW_CORE_SUFFIX}"
                     --help)
    set_tests_properties(opennow-acceptance-verifier-help PROPERTIES TIMEOUT 5)
endif()
