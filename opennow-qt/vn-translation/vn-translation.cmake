# ==============================================================================
# OpenNOW-ORC: Self-Contained VN Translation Module (opennow-qt/vn-translation)
#
# To port this module across upstream OpenNOW updates:
#   1. Copy the entire `opennow-qt/vn-translation/` folder into `opennow-qt/`.
#   2. Add `include(vn-translation/vn-translation.cmake)` in `opennow-qt/CMakeLists.txt`.
# ==============================================================================

set(OPENNOW_TRANSLATION_SOURCES
    vn-translation/src/translation/TranslationCache.cpp
    vn-translation/src/translation/TranslationCache.h
    vn-translation/src/translation/WuWaDialogueDb.cpp
    vn-translation/src/translation/WuWaDialogueDb.h
    vn-translation/src/translation/TranslationHttp.cpp
    vn-translation/src/translation/TranslationHttp.h
    vn-translation/src/translation/CustomApi.cpp
    vn-translation/src/translation/CustomApi.h
    vn-translation/src/translation/OcrEngine.cpp
    vn-translation/src/translation/OcrEngine.h
    vn-translation/src/translation/TranslationProvider.cpp
    vn-translation/src/translation/TranslationProvider.h
    vn-translation/src/translation/TranslationWorker.cpp
    vn-translation/src/translation/TranslationWorker.h
    vn-translation/src/translation/TranslationEngine.cpp
    vn-translation/src/translation/TranslationEngine.h
)

target_sources(opennow-qt PRIVATE ${OPENNOW_TRANSLATION_SOURCES})
target_include_directories(opennow-qt PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/vn-translation/src"
)

if(WIN32)
    target_link_libraries(opennow-qt PRIVATE windowsapp)
endif()

qt_target_qml_sources(opennow-qt
    QML_FILES
        vn-translation/qml/DesktopSettingsTranslationPage.qml
        vn-translation/qml/TranslationOverlay.qml
)

# Bundled WuWa dialogue-DB seed
set_source_files_properties(
    "${CMAKE_CURRENT_SOURCE_DIR}/vn-translation/resources/data/wuwa-dialogue-db.json"
    PROPERTIES QT_RESOURCE_ALIAS "wuwa-dialogue-db.json"
)
qt_add_resources(opennow-qt "opennow-translation-data"
    PREFIX "/translation"
    FILES "${CMAKE_CURRENT_SOURCE_DIR}/vn-translation/resources/data/wuwa-dialogue-db.json"
)

# Bundled Be Vietnam Pro font family (SIL OFL 1.1) for full Vietnamese diacritics
file(GLOB OPENNOW_TRANSLATION_FONT_FILES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/vn-translation/resources/fonts/*.ttf"
)
foreach(OPENNOW_TRANSLATION_FONT_FILE IN LISTS OPENNOW_TRANSLATION_FONT_FILES)
    get_filename_component(OPENNOW_TRANSLATION_FONT_NAME "${OPENNOW_TRANSLATION_FONT_FILE}" NAME)
    set_source_files_properties("${OPENNOW_TRANSLATION_FONT_FILE}" PROPERTIES
        QT_RESOURCE_ALIAS "${OPENNOW_TRANSLATION_FONT_NAME}"
    )
endforeach()
qt_add_resources(opennow-qt "opennow-translation-fonts"
    PREFIX "/fonts"
    FILES ${OPENNOW_TRANSLATION_FONT_FILES}
)

include(CTest)
if(BUILD_TESTING)
    qt_add_executable(opennow-translation-tests
        vn-translation/tests/tst_translation.cpp
        ${OPENNOW_TRANSLATION_SOURCES}
    )
    target_include_directories(opennow-translation-tests PRIVATE
        src
        "${CMAKE_CURRENT_SOURCE_DIR}/vn-translation/src"
    )
    target_link_libraries(opennow-translation-tests PRIVATE Qt6::Test Qt6::Gui Qt6::Network)
    if(WIN32)
        target_link_libraries(opennow-translation-tests PRIVATE windowsapp)
        set_target_properties(opennow-translation-tests PROPERTIES WIN32_EXECUTABLE FALSE)
    endif()
    add_test(NAME opennow-translation-tests COMMAND opennow-translation-tests -o -,txt)
    set_tests_properties(opennow-translation-tests PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 30
    )
endif()
