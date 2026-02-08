cmake_minimum_required(VERSION 3.20)
project(OpenNOWRewrite VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

option(OPENNOW_REWRITE_BUILD_DESKTOP "Build desktop bootstrap app" ON)
option(OPENNOW_REWRITE_BUILD_TESTS "Build rewrite tests" ON)

add_library(opennow_core
    src/core/app.cpp
    src/auth/login.cpp
)

target_include_directories(opennow_core
    PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/include
)

if(OPENNOW_REWRITE_BUILD_DESKTOP)
    add_executable(opennow_desktop_bootstrap
        apps/desktop/main.cpp
    )

    target_link_libraries(opennow_desktop_bootstrap
        PRIVATE
            opennow_core
    )
endif()

if(OPENNOW_REWRITE_BUILD_TESTS)
    enable_testing()

    add_executable(opennow_login_smoke
        tests/login_smoke.cpp
    )

    target_link_libraries(opennow_login_smoke
        PRIVATE
            opennow_core
    )

    add_test(NAME opennow_login_smoke COMMAND opennow_login_smoke)
endif()
