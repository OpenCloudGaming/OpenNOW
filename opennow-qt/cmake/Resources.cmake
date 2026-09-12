file(GLOB OPENNOW_LOCALE_FILES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/../locales/*.json")
foreach(OPENNOW_LOCALE_FILE IN LISTS OPENNOW_LOCALE_FILES)
    get_filename_component(OPENNOW_LOCALE_NAME "${OPENNOW_LOCALE_FILE}" NAME)
    set_source_files_properties("${OPENNOW_LOCALE_FILE}" PROPERTIES
        QT_RESOURCE_ALIAS "${OPENNOW_LOCALE_NAME}")
endforeach()
qt_add_resources(opennow-qt "opennow-locales"
    PREFIX "/locales"
    FILES ${OPENNOW_LOCALE_FILES}
)

set(OPENNOW_STREAM_SHADERS
    shaders/streamvideo.vert shaders/streamvideo.frag
    shaders/hdroutput.frag
    shaders/framegen.vert shaders/framegen_copy.frag shaders/framegen_cut.frag
    shaders/framegen_motion.frag shaders/framegen_reduce.frag shaders/framegen_synthesize.frag)
qt_add_shaders(opennow-qt "opennow-stream-shaders"
    PREFIX "/opennow/shaders"
    BASE "shaders"
    FILES ${OPENNOW_STREAM_SHADERS}
)
set(OPENNOW_CHROME_SHADERS shaders/hdrchrome.vert shaders/hdrchrome.frag)
qt_add_shaders(opennow-qt "opennow-chrome-shaders"
    BATCHABLE PREFIX "/opennow/shaders" BASE "shaders" FILES ${OPENNOW_CHROME_SHADERS})
