cmake_minimum_required(VERSION 3.19)
project(omnitiler-kwin-qv4-tests LANGUAGES CXX)

find_package(Qt6 REQUIRED COMPONENTS Core Qml)
include(CTest)

if(BUILD_TESTING)
    set(OMNITILER_KWIN_BUNDLE "${CMAKE_CURRENT_SOURCE_DIR}/../../contents/code/main.js"
        CACHE FILEPATH "Built OmniTiler KWin script (npm run build or build:trace)")
    add_executable(omnitiler-focus-scope-test focusscope_test.cpp)
    set_target_properties(omnitiler-focus-scope-test PROPERTIES AUTOMOC ON)
    target_compile_features(omnitiler-focus-scope-test PRIVATE cxx_std_20)
    target_compile_options(omnitiler-focus-scope-test PRIVATE -Wall -Wextra -Werror)
    target_link_libraries(omnitiler-focus-scope-test PRIVATE Qt6::Core Qt6::Qml)
    add_test(NAME kwin-qv4-focus-scope COMMAND omnitiler-focus-scope-test "${OMNITILER_KWIN_BUNDLE}")
endif()
