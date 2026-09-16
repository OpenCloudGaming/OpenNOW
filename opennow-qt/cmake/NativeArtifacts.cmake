set(OPENNOW_PREBUILT_NATIVE_DIR "" CACHE PATH
    "Directory containing prebuilt Linux native executables, streamer FFI library, and generated license notices")

if(OPENNOW_PREBUILT_NATIVE_DIR)
    if(NOT CMAKE_SYSTEM_NAME STREQUAL "Linux")
        message(FATAL_ERROR "OPENNOW_PREBUILT_NATIVE_DIR is supported only for Linux targets")
    endif()
    get_filename_component(OPENNOW_PREBUILT_NATIVE_DIR
        "${OPENNOW_PREBUILT_NATIVE_DIR}" ABSOLUTE BASE_DIR "${CMAKE_CURRENT_SOURCE_DIR}")
    foreach(artifact opennow-core opennow-update-helper opennow-acceptance-verify
            opennow-streamer libopennow_streamer_ffi.so THIRD_PARTY_NOTICES.generated)
        if(NOT EXISTS "${OPENNOW_PREBUILT_NATIVE_DIR}/${artifact}"
                OR IS_DIRECTORY "${OPENNOW_PREBUILT_NATIVE_DIR}/${artifact}")
            message(FATAL_ERROR "OPENNOW_PREBUILT_NATIVE_DIR requires a file: ${OPENNOW_PREBUILT_NATIVE_DIR}/${artifact}")
        endif()
    endforeach()
else()
    find_program(CARGO_EXECUTABLE cargo REQUIRED)
endif()
