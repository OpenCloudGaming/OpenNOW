set(OPENNOW_CORE_SUFFIX "")
set(OPENNOW_STREAMER_FFI_RUNTIME_NAME "libopennow_streamer_ffi.so")
set(OPENNOW_STREAMER_FFI_RUNTIME
    "${OPENNOW_PREBUILT_NATIVE_DIR}/${OPENNOW_STREAMER_FFI_RUNTIME_NAME}")
set(OPENNOW_STREAMER_FFI_LINK_LIBRARY "${OPENNOW_STREAMER_FFI_RUNTIME}")
set(OPENNOW_STREAMER_BIN_NAME "opennow-streamer")
set(OPENNOW_STREAMER_BIN_ARTIFACT "${OPENNOW_PREBUILT_NATIVE_DIR}/${OPENNOW_STREAMER_BIN_NAME}")
set(OPENNOW_GENERATED_NOTICES "${CMAKE_BINARY_DIR}/THIRD_PARTY_NOTICES.generated")

add_custom_target(opennow-core ALL
    COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:opennow-qt>"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OPENNOW_PREBUILT_NATIVE_DIR}/opennow-core"
        "${OPENNOW_PREBUILT_NATIVE_DIR}/opennow-acceptance-verify"
        "$<TARGET_FILE_DIR:opennow-qt>"
    VERBATIM)
add_custom_target(opennow-update-helper-build ALL
    COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:opennow-qt>"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OPENNOW_PREBUILT_NATIVE_DIR}/opennow-update-helper"
        "$<TARGET_FILE_DIR:opennow-qt>"
    VERBATIM)
add_custom_target(opennow-streamer-bin-build ALL
    COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:opennow-qt>"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OPENNOW_STREAMER_BIN_ARTIFACT}"
        "$<TARGET_FILE_DIR:opennow-qt>"
    VERBATIM)
add_custom_target(opennow-streamer-ffi-build DEPENDS "${OPENNOW_STREAMER_FFI_RUNTIME}")
add_custom_target(opennow-license-notices ALL
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${OPENNOW_PREBUILT_NATIVE_DIR}/THIRD_PARTY_NOTICES.generated"
        "${OPENNOW_GENERATED_NOTICES}"
    BYPRODUCTS "${OPENNOW_GENERATED_NOTICES}"
    VERBATIM)
add_dependencies(opennow-qt opennow-core opennow-update-helper-build
    opennow-streamer-bin-build opennow-license-notices)
